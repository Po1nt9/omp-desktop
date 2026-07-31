//! Bridge runtime: **in-process Rust** multi-IM connectors (no Node / agent-connect).

use super::config;
use super::engine::Engine;
use super::runtime::{self, RuntimeHandle};
use super::{BridgeStatusDto, ConnectedChannelDto};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;

struct RuntimeSlot {
    handle: Option<RuntimeHandle>,
    connected: Vec<ConnectedChannelDto>,
}

impl Default for RuntimeSlot {
    fn default() -> Self {
        Self {
            handle: None,
            connected: vec![],
        }
    }
}

fn runtime_slot() -> &'static AsyncMutex<RuntimeSlot> {
    static SLOT: OnceLock<AsyncMutex<RuntimeSlot>> = OnceLock::new();
    SLOT.get_or_init(|| AsyncMutex::new(RuntimeSlot::default()))
}

#[derive(Debug)]
pub struct BridgeRuntime {
    pub enabled: bool,
    pub lifecycle: String,
    pub allow_remote_yolo: bool,
    pub last_error: Option<String>,
    connected_cache: Mutex<Vec<ConnectedChannelDto>>,
    running: Mutex<bool>,
    /// AC-8.4: sync cache of the running engine (Arc clone) so `status()` /
    /// `set_config()` can read approval state / grant / revoke without
    /// awaiting the global runtime slot.
    engine_cache: Mutex<Option<Arc<Engine>>>,
}

impl Default for BridgeRuntime {
    fn default() -> Self {
        let cfg = config::load_bridge_config();
        Self {
            enabled: cfg.enabled,
            lifecycle: if cfg.lifecycle.is_empty() {
                "attached".into()
            } else {
                cfg.lifecycle
            },
            allow_remote_yolo: cfg.allow_remote_yolo,
            last_error: None,
            connected_cache: Mutex::new(Vec::new()),
            running: Mutex::new(false),
            engine_cache: Mutex::new(None),
        }
    }
}

impl BridgeRuntime {
    fn persist_config(&self) {
        let cfg = config::BridgePersistedConfig {
            enabled: self.enabled,
            lifecycle: if self.lifecycle.is_empty() {
                "attached".into()
            } else {
                self.lifecycle.clone()
            },
            allow_remote_yolo: self.allow_remote_yolo,
        };
        if let Err(e) = config::save_bridge_config(&cfg) {
            tracing::warn!("remote_im: persist bridge config failed: {e}");
        }
    }

    pub fn status_dto(&self) -> BridgeStatusDto {
        let running = *self.running.lock();
        let connected = if running {
            self.connected_cache.lock().clone()
        } else if self.enabled {
            config::list_instances()
                .into_iter()
                .filter(|i| i.enabled && i.has_credentials)
                .map(|i| ConnectedChannelDto {
                    channel: i.channel,
                    instance_id: i.id,
                    name: i.name,
                })
                .collect()
        } else {
            vec![]
        };
        BridgeStatusDto {
            state: if running {
                "running".into()
            } else if self.last_error.is_some() {
                "error".into()
            } else {
                "stopped".into()
            },
            enabled: self.enabled,
            lifecycle: if self.lifecycle.is_empty() {
                "attached".into()
            } else {
                self.lifecycle.clone()
            },
            allow_remote_yolo: self.allow_remote_yolo,
            approval_active: self
                .engine_cache
                .lock()
                .as_ref()
                .is_some_and(|e| e.approval_active()),
            approval_expires_at: self
                .engine_cache
                .lock()
                .as_ref()
                .and_then(|e| e.approval_expires_at()),
            connected_channels: connected,
            last_error: self.last_error.clone(),
            mock: false,
            remote_bridge_path: Some("rust://in-process".into()),
            backend: Some("rust".into()),
        }
    }

    pub fn set_config(
        &mut self,
        enabled: Option<bool>,
        lifecycle: Option<String>,
        allow_remote_yolo: Option<bool>,
    ) -> Result<(), String> {
        if let Some(e) = enabled {
            self.enabled = e;
        }
        if let Some(l) = lifecycle {
            self.lifecycle = l;
        }
        if let Some(y) = allow_remote_yolo {
            self.allow_remote_yolo = y;
            // AC-8.4: toggling yolo grants (TTL-bound) / revokes the
            // in-memory approval on the running engine, if any. A stopped
            // bridge has nothing to grant — the next start is inactive
            // anyway (approvals never survive restart).
            let eng = self.engine_cache.lock().clone();
            if let Some(e) = eng {
                if y {
                    e.grant_approval(crate::remote_im::engine::DEFAULT_APPROVAL_TTL_SECS);
                } else {
                    e.revoke_approval();
                }
            }
        }
        self.persist_config();
        Ok(())
    }

    pub async fn start_async(&mut self) -> Result<(), String> {
        self.enabled = true;
        self.last_error = None;
        self.persist_config();
        let _ = self.stop_async_inner(false).await;

        // Resolve binary path and agent dir from Settings (same source as SessionManager).
        let settings = crate::store::load_settings();
        let binary_path = settings
            .manual_cli_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists());
        let agent_dir = Some(crate::agent_prefs::agent_grok_home(&settings.session_data_mode));

        match runtime::start_runtime(self.allow_remote_yolo, binary_path, agent_dir).await {
            Ok((handle, connected)) => {
                let dtos: Vec<ConnectedChannelDto> = connected
                    .into_iter()
                    .map(|c| ConnectedChannelDto {
                        channel: c.channel,
                        instance_id: c.instance_id,
                        name: c.name,
                    })
                    .collect();
                *self.connected_cache.lock() = dtos.clone();
                *self.running.lock() = true;
                *self.engine_cache.lock() = Some(handle.engine().clone());
                {
                    let mut slot = runtime_slot().lock().await;
                    slot.handle = Some(handle);
                    slot.connected = dtos;
                }
                tracing::info!(
                    channels = ?self.connected_cache.lock().iter().map(|c| c.channel.as_str()).collect::<Vec<_>>(),
                    "remote_im: Rust in-process bridge started"
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(error = %e, "remote_im: bridge start failed");
                self.last_error = Some(e.clone());
                *self.running.lock() = false;
                Err(e)
            }
        }
    }

    async fn stop_async_inner(&mut self, clear_enabled: bool) -> Result<(), String> {
        {
            let mut slot = runtime_slot().lock().await;
            if let Some(h) = slot.handle.take() {
                h.stop().await;
            }
            slot.connected.clear();
        }
        *self.running.lock() = false;
        self.connected_cache.lock().clear();
        *self.engine_cache.lock() = None;
        if clear_enabled {
            self.enabled = false;
            self.persist_config();
        }
        Ok(())
    }

    pub async fn stop_async(&mut self) -> Result<(), String> {
        self.stop_async_inner(true).await
    }

    /// Called on App launch: if Bridge was enabled (or ready channels exist), start connectors.
    pub async fn try_autostart_async(&mut self) -> Result<(), String> {
        if *self.running.lock() {
            return Ok(());
        }
        if !self.enabled && !config::has_ready_instances() {
            return Ok(());
        }
        // Prefer explicit enabled; also start when bound channels exist (user expectation).
        if !self.enabled && config::has_ready_instances() {
            self.enabled = true;
            self.persist_config();
        }
        if !self.enabled {
            return Ok(());
        }
        tracing::info!("remote_im: auto-starting bridge (enabled + ready instances)");
        self.start_async().await
    }

    pub async fn reload_async(&mut self, _channel: &str, _instance_id: &str) -> Result<(), String> {
        // Always (re)start after save/connect so channels actually receive messages.
        self.enabled = true;
        self.persist_config();
        self.start_async().await?;
        Ok(())
    }

    pub async fn drop_instance_async(&mut self, _instance_id: &str) {
        if self.enabled || *self.running.lock() {
            if config::has_ready_instances() {
                let _ = self.start_async().await;
            } else {
                let _ = self.stop_async_inner(true).await;
            }
        }
    }
}

pub fn doctor_report() -> serde_json::Value {
    let instances = config::list_instances();
    let enabled_with_creds = instances
        .iter()
        .filter(|i| i.enabled && i.has_credentials)
        .count();
    let channel_protocols: serde_json::Map<String, serde_json::Value> =
        super::channels::CATALOG_CHANNELS
            .iter()
            .map(|ch| {
                (
                    (*ch).to_string(),
                    serde_json::json!({
                        "protocol": super::channels::protocol_for(ch),
                        "real": super::channels::is_real_protocol(ch),
                    }),
                )
            })
            .collect();
    serde_json::json!({
        "backend": "rust",
        "inProcess": true,
        "externalAgentConnect": false,
        "nodeRemoteBridge": false,
        "instances": instances.len(),
        "enabledWithCreds": enabled_with_creds,
        "channelsSupported": super::channels::CATALOG_CHANNELS,
        "channelProtocols": channel_protocols,
        "scanSupported": ["feishu", "lark", "weixin"],
    })
}
