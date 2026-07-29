//! Live voice host: local state machine + host tools → SessionManager.
//!
//! Plan 1 fail-closed shell: direct xAI realtime WebSocket and credential
//! resolution have been removed. The live realtime network loop
//! is gone; `start()` returns `runtime_unavailable` for the live path. Local
//! audio capture plumbing, mock mode, tool dispatch, and session delegation
//! remain so a later OMP Runtime integration can wire them back up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::session_manager::SessionManager;
use crate::store;
use crate::voice_tools;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSessionState {
    pub active: bool,
    pub mode: String,
    pub project_path: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub mock: bool,
    pub listening: bool,
    pub speaking: bool,
    pub error: Option<String>,
    pub delegated_session_ids: Vec<String>,
}

impl Default for VoiceSessionState {
    fn default() -> Self {
        Self {
            active: false,
            mode: "idle".into(),
            project_path: None,
            project_id: None,
            project_name: None,
            mock: false,
            listening: false,
            speaking: false,
            error: None,
            delegated_session_ids: vec![],
        }
    }
}

struct LiveVoiceInner {
    state: VoiceSessionState,
    /// Outbound PCM base64 chunks from the frontend mic.
    audio_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    stop: Arc<AtomicBool>,
}

pub struct VoiceHost {
    inner: Mutex<LiveVoiceInner>,
}

impl Default for VoiceHost {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceHost {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(LiveVoiceInner {
                state: VoiceSessionState::default(),
                audio_tx: None,
                stop: Arc::new(AtomicBool::new(false)),
            }),
        }
    }

    pub fn snapshot(&self) -> VoiceSessionState {
        self.inner.lock().state.clone()
    }

    pub fn is_mock_env() -> bool {
        std::env::var("GROK_APP_VOICE")
            .map(|v| v == "mock")
            .unwrap_or(false)
    }

    pub async fn start(
        self: &Arc<Self>,
        app: AppHandle,
        mgr: Arc<SessionManager>,
        project_path: Option<String>,
        project_id: Option<String>,
        project_name: Option<String>,
    ) -> Result<VoiceSessionState, String> {
        self.stop_internal(false).await;

        let mock = Self::is_mock_env();

        let stop = Arc::new(AtomicBool::new(false));
        let (audio_tx, _audio_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        {
            let mut g = self.inner.lock();
            g.stop = stop.clone();
            g.audio_tx = Some(audio_tx);
            g.state = VoiceSessionState {
                active: true,
                mode: if mock { "mock".into() } else { "live".into() },
                project_path: project_path.clone(),
                project_id: project_id.clone(),
                project_name: project_name.clone(),
                mock,
                listening: true,
                speaking: false,
                error: None,
                delegated_session_ids: vec![],
            };
        }
        self.emit_state(&app);

        if mock {
            let host = Arc::clone(self);
            let app2 = app.clone();
            tokio::spawn(async move {
                let _ = app2.emit(
                    "voice://transcript",
                    json!({
                        "role": "assistant",
                        "text": "Live voice mock is ready. Ask me to start an agent task.",
                        "final": true
                    }),
                );
                let mut st = host.snapshot();
                st.speaking = false;
                st.listening = true;
                host.inner.lock().state = st;
                host.emit_state(&app2);
            });
            return Ok(self.snapshot());
        }

        // Plan 1 fail-closed: live realtime voice is unavailable until an OMP
        // Runtime integration supplies the realtime transport. No direct xAI
        // WebSocket or credential code remains.
        let host = Arc::clone(self);
        {
            let mut g = host.inner.lock();
            g.state.active = false;
            g.state.listening = false;
            g.state.mode = "idle".into();
            g.state.error = Some("runtime_unavailable".into());
            g.audio_tx = None;
        }
        host.emit_state(&app);
        let _ = app.emit(
            "voice://error",
            json!({ "message": "Live voice is unavailable in this build." }),
        );
        let _ = mgr; // retained for later runtime integration
        Err("runtime_unavailable: live voice is unavailable in this build".into())
    }

    pub async fn stop(&self, app: &AppHandle) -> VoiceSessionState {
        self.stop_internal(true).await;
        self.emit_state(app);
        self.snapshot()
    }

    async fn stop_internal(&self, clear_audio: bool) {
        let stop_flag = {
            let mut g = self.inner.lock();
            g.stop.store(true, Ordering::SeqCst);
            if clear_audio {
                g.audio_tx = None;
            }
            g.state.active = false;
            g.state.listening = false;
            g.state.speaking = false;
            g.state.mode = "idle".into();
            g.stop.clone()
        };
        stop_flag.store(true, Ordering::SeqCst);
        // tiny yield so tasks notice
        tokio::task::yield_now().await;
    }

    pub fn push_pcm(&self, pcm: Vec<u8>) -> Result<(), String> {
        let g = self.inner.lock();
        if !g.state.active {
            return Err("voice session not active".into());
        }
        if let Some(tx) = &g.audio_tx {
            let _ = tx.send(pcm);
        }
        Ok(())
    }

    /// Mock / debug: run a host tool as if the voice model requested it.
    pub async fn invoke_tool(
        &self,
        app: &AppHandle,
        mgr: &Arc<SessionManager>,
        name: &str,
        args_json: &str,
    ) -> Result<Value, String> {
        let snap = self.snapshot();
        execute_tool(app, mgr, self, &snap, name, args_json).await
    }

    fn emit_state(&self, app: &AppHandle) {
        let st = self.snapshot();
        let _ = app.emit("voice://state", st);
    }

    fn push_delegated(&self, session_id: &str) {
        let mut g = self.inner.lock();
        if !g
            .state
            .delegated_session_ids
            .iter()
            .any(|s| s == session_id)
        {
            g.state.delegated_session_ids.push(session_id.to_string());
        }
    }
}

async fn execute_tool(
    app: &AppHandle,
    mgr: &Arc<SessionManager>,
    host: &VoiceHost,
    snap: &VoiceSessionState,
    name: &str,
    args_json: &str,
) -> Result<Value, String> {
    if VoiceHost::is_mock_env() {
        let out = voice_tools::mock_execute_tool(name, args_json)?;
        let _ = app.emit(
            "voice://tool",
            json!({ "name": name, "args": args_json, "result": out }),
        );
        if let Some(sid) = out.get("session_id").and_then(|x| x.as_str()) {
            host.push_delegated(sid);
            host.emit_state(app);
        }
        return Ok(out);
    }

    let tool = voice_tools::VoiceToolName::parse(name)
        .ok_or_else(|| format!("unknown tool: {name}"))?;

    let out = match tool {
        voice_tools::VoiceToolName::ListSessions => {
            let args = voice_tools::parse_list_sessions_args(args_json)?;
            let limit = args.limit.unwrap_or(20).min(50) as usize;
            let mut sessions = store::load_sessions_index();
            if let Some(pid) = &snap.project_id {
                sessions.retain(|s| s.project_id.as_deref() == Some(pid.as_str()));
            }
            store::sort_sessions_by_pin_then_updated(&mut sessions);
            sessions.truncate(limit);
            let rows: Vec<Value> = sessions
                .into_iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "title": s.title,
                        "projectId": s.project_id,
                        "updatedAt": s.updated_at,
                    })
                })
                .collect();
            json!({ "sessions": rows })
        }
        voice_tools::VoiceToolName::CreateAgentSession => {
            let args = voice_tools::parse_create_agent_args(args_json)?;
            let meta = store::create_session(
                snap.project_id.clone(),
                args.title.or_else(|| Some("Voice task".into())),
                false,
            )?;
            // Connect + send on that session (becomes live host).
            let path = snap.project_path.clone();
            mgr.connect(app.clone(), path, Some(meta.id.clone()), None)
                .await?;
            mgr.send_message(
                app.clone(),
                args.prompt.clone(),
                None,
                Some(meta.id.clone()),
            )
            .await?;
            host.push_delegated(&meta.id);
            host.emit_state(app);
            json!({
                "session_id": meta.id,
                "title": meta.title,
                "state": "streaming",
                "accepted_prompt": args.prompt
            })
        }
        voice_tools::VoiceToolName::PromptAgent => {
            let args = voice_tools::parse_prompt_agent_args(args_json)?;
            if let Some(sid) = &args.session_id {
                mgr.connect(
                    app.clone(),
                    snap.project_path.clone(),
                    Some(sid.clone()),
                    None,
                )
                .await?;
                host.push_delegated(sid);
            }
            mgr.send_message(
                app.clone(),
                args.prompt.clone(),
                None,
                args.session_id.clone(),
            )
            .await?;
            let live = mgr.snapshot();
            json!({
                "session_id": live.session_id,
                "state": live.state,
                "accepted_prompt": args.prompt
            })
        }
        voice_tools::VoiceToolName::GetAgentStatus => {
            let args = voice_tools::parse_session_ref_args(args_json)?;
            if let Some(sid) = &args.session_id {
                let _ = mgr
                    .connect(
                        app.clone(),
                        snap.project_path.clone(),
                        Some(sid.clone()),
                        None,
                    )
                    .await;
            }
            let live = mgr.snapshot();
            json!({
                "session_id": live.session_id,
                "state": live.state,
                "title": live.title,
                "backend": live.backend,
                "lastError": live.last_error,
            })
        }
        voice_tools::VoiceToolName::CancelAgent => {
            let args = voice_tools::parse_session_ref_args(args_json)?;
            if let Some(sid) = &args.session_id {
                let _ = mgr
                    .connect(
                        app.clone(),
                        snap.project_path.clone(),
                        Some(sid.clone()),
                        None,
                    )
                    .await;
            }
            let live = mgr.stop(app.clone(), args.session_id.clone()).await?;
            json!({
                "session_id": live.session_id,
                "state": live.state,
                "cancelled": true
            })
        }
    };

    let _ = app.emit(
        "voice://tool",
        json!({ "name": name, "args": args_json, "result": out }),
    );
    Ok(out)
}

// --- Tauri commands ---

#[tauri::command]
pub async fn voice_state(host: State<'_, Arc<VoiceHost>>) -> Result<VoiceSessionState, String> {
    Ok(host.snapshot())
}

#[tauri::command]
pub async fn voice_start(
    app: AppHandle,
    host: State<'_, Arc<VoiceHost>>,
    mgr: State<'_, Arc<SessionManager>>,
    project_path: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
) -> Result<VoiceSessionState, String> {
    host.start(
        app,
        mgr.inner().clone(),
        project_path,
        project_id,
        project_name,
    )
    .await
}

#[tauri::command]
pub async fn voice_stop(
    app: AppHandle,
    host: State<'_, Arc<VoiceHost>>,
) -> Result<VoiceSessionState, String> {
    Ok(host.stop(&app).await)
}

#[tauri::command]
pub async fn voice_push_pcm(
    host: State<'_, Arc<VoiceHost>>,
    pcm_base64: String,
) -> Result<(), String> {
    let bytes = B64
        .decode(pcm_base64.trim())
        .map_err(|e| format!("pcm base64: {e}"))?;
    host.push_pcm(bytes)
}

#[tauri::command]
pub async fn voice_invoke_tool(
    app: AppHandle,
    host: State<'_, Arc<VoiceHost>>,
    mgr: State<'_, Arc<SessionManager>>,
    name: String,
    args_json: Option<String>,
) -> Result<Value, String> {
    host.invoke_tool(
        &app,
        mgr.inner(),
        &name,
        args_json.as_deref().unwrap_or("{}"),
    )
    .await
}

#[tauri::command]
pub async fn voice_dictation_transcribe(
    audio_base64: String,
    mime: Option<String>,
    language: Option<String>,
) -> Result<crate::voice_stt::SttResult, String> {
    crate::voice_stt::transcribe_base64(
        &audio_base64,
        mime.as_deref(),
        language.as_deref(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_inactive() {
        let h = VoiceHost::new();
        assert!(!h.snapshot().active);
    }
}
