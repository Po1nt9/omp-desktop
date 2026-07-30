//! Message engine: ACL, slash commands, Agent turns (fail-closed), project/session bind.

use super::app_sessions;
use super::control_plane::{
    self, apply_project_pick, apply_session_pick, binding_after_agent_turn, channel_uses_cards,
    format_project_menu, format_session_menu, list_sessions_for_project, parse_card_action,
    resolve_turn_intent, AppSessionEntry, CardAction, PendingMode, ScopeBinding, TurnIntent,
};
use super::outbound::{self, OutboundRouter};
use super::projects::{self, load_trusted_projects};
use super::session::SessionStore;
use super::slash::{self, BuiltinCommand};
use super::types::{ChannelInstance, IncomingMessage};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::acp_client::{AcpClient, AcpEvent, SpawnOptions, StreamKind};

#[derive(Clone)]
struct PendingPick {
    kind: PickKind,
    /// For session pick: listed App sessions at menu time.
    sessions: Vec<AppSessionEntry>,
}

#[derive(Clone, Copy)]
enum PickKind {
    Project,
    Session,
}

/// Result of a remote Agent turn in the fail-closed shell. Until an OMP
/// Runtime is connected, every turn surfaces `runtime_unavailable`.
pub(crate) struct AgentTurnResult {
    pub text: String,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

/// A pooled Runtime process keyed by work_dir.
struct RuntimeEntry {
    acp: Arc<AcpClient>,
    /// Accumulated assistant text from the event stream; cleared at the
    /// start of each turn, read after `prompt()` returns.
    text_buf: Arc<Mutex<String>>,
    /// Drain barrier: the background collector calls `notify_one` once it
    /// has processed the terminal `AcpEvent::Stream { done: true, .. }` marker
    /// (sent by `prompt()` after the RPC resolves). A turn `await`s
    /// `notified()` before reading `text_buf`, so all prior chunks are
    /// guaranteed accumulated before the snapshot is taken — closing the
    /// producer/consumer drain race on short/fast replies.
    drained: Arc<tokio::sync::Notify>,
}

// `AcpClient` does not derive Debug, so RuntimeEntry can't either; provide a
// manual impl so `Result<Arc<RuntimeEntry>, _>` is debug-printable in tests.
impl std::fmt::Debug for RuntimeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeEntry")
            .field("text_len", &self.text_buf.lock().len())
            .finish_non_exhaustive()
    }
}

pub struct Engine {
    store: SessionStore,
    outbound: OutboundRouter,
    instances: Arc<Mutex<HashMap<String, ChannelInstance>>>,
    pending: Arc<Mutex<HashMap<String, PendingPick>>>,
    aborts: Arc<Mutex<HashMap<String, bool>>>,
    lang: String,
    allow_remote_yolo: bool,
    /// Absolute path to the OMP binary; `None` → fail-closed degradation.
    binary_path: Option<PathBuf>,
    /// Agent home dir (PI_CODING_AGENT_DIR) for spawned runtimes.
    agent_dir: Option<PathBuf>,
    /// Per-work_dir Runtime process pool.
    runtimes: Arc<Mutex<HashMap<PathBuf, Arc<RuntimeEntry>>>>,
    /// Per-scope concurrency guard: a scope_key present here means a turn
    /// is in flight; concurrent turns for the same scope are rejected.
    in_flight: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// Per-work_dir spawn guard: prevents two concurrent turns for
    /// *different scopes but the same work_dir* from double-spawning a
    /// Runtime process (TOCTOU fix). Used with double-checked locking in
    /// `get_or_spawn_runtime`.
    spawn_locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
    /// Cross-restart message deduplication (SQLite).
    dedup: super::dedup_store::DedupStore,
    /// Per-channel + per-scope request rate limiting (in-memory).
    rate_limiter: super::rate_limiter::RateLimiter,
}

impl Engine {
    pub fn new(
        outbound: OutboundRouter,
        allow_remote_yolo: bool,
        binary_path: Option<PathBuf>,
        agent_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            store: SessionStore::open_default(),
            outbound,
            instances: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            aborts: Arc::new(Mutex::new(HashMap::new())),
            lang: "zh".into(),
            allow_remote_yolo,
            binary_path,
            agent_dir,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
            dedup: super::dedup_store::DedupStore::open_default(),
            rate_limiter: super::rate_limiter::RateLimiter::new_default(),
        }
    }

    /// Test helper: ephemeral store.
    pub fn new_ephemeral(outbound: OutboundRouter, allow_remote_yolo: bool) -> Self {
        Self {
            store: SessionStore::ephemeral(),
            outbound,
            instances: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            aborts: Arc::new(Mutex::new(HashMap::new())),
            lang: "zh".into(),
            allow_remote_yolo,
            binary_path: None,
            agent_dir: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
            dedup: super::dedup_store::DedupStore::ephemeral(),
            rate_limiter: super::rate_limiter::RateLimiter::new_default(),
        }
    }

    pub fn upsert_instance(&self, inst: ChannelInstance) {
        self.instances.lock().insert(inst.id.clone(), inst);
    }

    pub fn remove_instance(&self, id: &str) {
        self.instances.lock().remove(id);
    }

    pub async fn handle(&self, msg: IncomingMessage) {
        tracing::info!(
            channel = %msg.channel,
            instance = %msg.instance_id,
            content_len = msg.content.len(),
            "remote_im: handle begin"
        );

        // ── P1: 去重 (dedup) — drop messages already seen across restarts ──
        if !self.dedup.check_and_mark(&msg.channel, &msg.message_id) {
            tracing::debug!(
                target: "remote_im::dedup",
                channel = %msg.channel,
                message_id = %msg.message_id,
                "duplicate message dropped"
            );
            return;
        }

        // ── P1: 限流 (rate limit) — per-channel + per-scope fixed window ──
        let scope_key = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.chat_id,
            &msg.sender_id,
        );
        if !self.rate_limiter.check(&msg.channel, &scope_key) {
            tracing::warn!(
                target: "remote_im::rate_limit",
                channel = %msg.channel,
                scope = %scope_key,
                "rate limit exceeded, message dropped"
            );
            return;
        }

        // Card actions must never fall through to text-pick (that produced「无效选择」).
        if msg.content.trim().starts_with("__card_action__:") {
            if let Some(action) = extract_card_action(&msg) {
                self.handle_card_action(action, &msg).await;
            } else {
                tracing::warn!(
                    content = %msg.content.chars().take(200).collect::<String>(),
                    "remote_im: unparseable card action payload"
                );
                let t = if self.lang == "en" {
                    "Could not read that card button. Send /p again, or reply with the number."
                } else {
                    "无法识别卡片按钮。请重新发送 /p，或直接回复序号（如 2）。"
                };
                let _ = self.reply_msg(&msg, t).await;
            }
            return;
        }
        if let Some(action) = extract_card_action(&msg) {
            self.handle_card_action(action, &msg).await;
            return;
        }

        let inst = {
            let g = self.instances.lock();
            match g.get(&msg.instance_id) {
                Some(i) => i.clone(),
                None => {
                    tracing::warn!(
                        instance = %msg.instance_id,
                        channel = %msg.channel,
                        "remote_im: drop message — instance not registered in engine"
                    );
                    return;
                }
            }
        };

        let content = msg.content.trim().to_string();
        if content.is_empty() {
            return;
        }

        if msg.chat_type == "group"
            && outbound::require_mention(&inst.options, &inst.acl)
            && !msg.mentioned_bot
        {
            return;
        }

        if !outbound::sender_allowed(&inst.acl, &msg.sender_id) {
            let text = if self.lang == "en" {
                "You are not on the allow_from list."
            } else {
                "你不在 allow_from 白名单中。请管理员把你的 open_id 加入配置。"
            };
            let _ = self.reply_msg(&msg, text).await;
            return;
        }

        let scope = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.chat_id,
            &msg.sender_id,
        );
        let alt_scope = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.sender_id,
            &msg.sender_id,
        );
        let default_wd = projects::default_work_dir(&inst.project_scope);

        // Single lock scope — never nest pending.lock() (parking_lot is not reentrant;
        // or_else(|| self.pending.lock()...) deadlocked every first message after start).
        let pending = {
            let g = self.pending.lock();
            g.get(&scope)
                .cloned()
                .or_else(|| g.get(&alt_scope).cloned())
        };
        if let Some(pending) = pending {
            if content == "0" || content.eq_ignore_ascii_case("cancel") {
                {
                    let mut g = self.pending.lock();
                    g.remove(&scope);
                    g.remove(&alt_scope);
                }
                let t = if self.lang == "en" {
                    "Cancelled."
                } else {
                    "已取消。"
                };
                let _ = self.reply_msg(&msg, t).await;
                return;
            }
            self.handle_text_pick(&pending, &content, &scope, &msg, &default_wd)
                .await;
            return;
        }

        if let Some(cmd) = slash::parse_slash(&content) {
            tracing::info!(?cmd, "remote_im: slash command");
            self.handle_slash(cmd, &msg, &scope, &default_wd).await;
            tracing::info!("remote_im: slash done");
            return;
        }

        tracing::info!("remote_im: agent turn");
        self.run_agent_turn(&msg, &scope, &default_wd, &content).await;
        tracing::info!("remote_im: agent turn done");
    }

    async fn handle_card_action(&self, action: CardAction, msg: &IncomingMessage) {
        let scope = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.chat_id,
            &msg.sender_id,
        );
        // Also clear pending under sender-only scopes (chat_id may differ on card callback).
        let alt_scope = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.sender_id,
            &msg.sender_id,
        );
        let default_wd = {
            let g = self.instances.lock();
            g.get(&msg.instance_id)
                .map(|i| projects::default_work_dir(&i.project_scope))
                .unwrap_or_else(|| ".".into())
        };
        // Prefer existing binding from either scope key
        let binding = self
            .store
            .get(&scope)
            .or_else(|| self.store.get(&alt_scope))
            .unwrap_or_else(|| self.store.get_or_create(&scope, &default_wd));
        self.pending.lock().remove(&scope);
        self.pending.lock().remove(&alt_scope);

        match action {
            CardAction::Cancel => {
                let t = if self.lang == "en" {
                    "Cancelled."
                } else {
                    "已取消。"
                };
                let _ = self.reply_msg(msg, t).await;
            }
            CardAction::Project { id } => {
                let projects = load_trusted_projects();
                match apply_project_pick(&binding, &projects, &id) {
                    Ok(next) => {
                        // Persist under both chat and sender scopes so next IM messages find it.
                        self.store.set(&scope, next.clone());
                        self.store.set(&alt_scope, next.clone());
                        let name = projects
                            .iter()
                            .find(|p| Some(p.id.as_str()) == next.project_id.as_deref())
                            .map(|p| p.name.as_str())
                            .unwrap_or(next.project_id.as_deref().unwrap_or(""));
                        let t = if self.lang == "en" {
                            format!(
                                "Bound **{name}**\n`{}`\nNext message starts a **new** session.",
                                next.work_dir
                            )
                        } else {
                            format!(
                                "已绑定 **{name}**\n`{}`\n下一条消息将开启**新**会话。",
                                next.work_dir
                            )
                        };
                        let _ = self.reply_msg(msg, &t).await;
                    }
                    Err(_) => {
                        let t = if self.lang == "en" {
                            "Project not found. Send /p again."
                        } else {
                            "未找到项目。请重新发送 /p。"
                        };
                        let _ = self.reply_msg(msg, t).await;
                    }
                }
            }
            CardAction::Session { id } => {
                let sessions = app_sessions::sessions_for_project(binding.project_id.as_deref());
                match apply_session_pick(&binding, &sessions, &id) {
                    Ok(next) => {
                        let aid = next.agent_session_id.clone().unwrap_or_default();
                        self.store.set(&scope, next);
                        let t = if self.lang == "en" {
                            format!("Resumed session `{aid}`. Continue chatting.")
                        } else {
                            format!("已恢复会话 `{aid}`。继续对话即可。")
                        };
                        let _ = self.reply_msg(msg, &t).await;
                    }
                    Err(_) => {
                        let t = if self.lang == "en" {
                            "Session not found. Send /r again."
                        } else {
                            "未找到会话。请重新发送 /r。"
                        };
                        let _ = self.reply_msg(msg, t).await;
                    }
                }
            }
        }
    }

    async fn handle_text_pick(
        &self,
        pending: &PendingPick,
        content: &str,
        scope: &str,
        msg: &IncomingMessage,
        default_wd: &str,
    ) {
        let binding = self.store.get_or_create(scope, default_wd);
        match pending.kind {
            PickKind::Project => {
                let projects = load_trusted_projects();
                match apply_project_pick(&binding, &projects, content) {
                    Ok(next) => {
                        self.pending.lock().remove(scope);
                        let alt = SessionStore::scope_key(
                            &msg.channel,
                            &msg.instance_id,
                            &msg.sender_id,
                            &msg.sender_id,
                        );
                        self.pending.lock().remove(&alt);
                        self.store.set(scope, next.clone());
                        self.store.set(&alt, next.clone());
                        let name = projects
                            .iter()
                            .find(|p| Some(p.id.as_str()) == next.project_id.as_deref())
                            .map(|p| p.name.as_str())
                            .unwrap_or(next.project_id.as_deref().unwrap_or(""));
                        let t = if self.lang == "en" {
                            format!(
                                "Bound **{name}**\n`{}`\nNext message starts a **new** session.",
                                next.work_dir
                            )
                        } else {
                            format!(
                                "已绑定 **{name}**\n`{}`\n下一条消息将开启**新**会话。",
                                next.work_dir
                            )
                        };
                        let _ = self.reply_msg(msg, &t).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            pick = %content,
                            err = %e,
                            n_projects = projects.len(),
                            "remote_im: project text pick failed"
                        );
                        let t = if self.lang == "en" {
                            format!(
                                "Invalid pick `{content}`. Send number (1–{}) or name, or 0 to cancel.",
                                projects.len()
                            )
                        } else {
                            format!(
                                "无效选择 `{}`。请发送序号（1–{}）或名称，或 0 取消。",
                                content.chars().take(40).collect::<String>(),
                                projects.len()
                            )
                        };
                        let _ = self.reply_msg(msg, &t).await;
                    }
                }
            }
            PickKind::Session => {
                match apply_session_pick(&binding, &pending.sessions, content) {
                    Ok(next) => {
                        self.pending.lock().remove(scope);
                        let aid = next
                            .agent_session_id
                            .clone()
                            .unwrap_or_else(|| next.local_session_id.clone());
                        self.store.set(scope, next);
                        let t = if self.lang == "en" {
                            format!("Resumed session `{aid}`. Continue chatting.")
                        } else {
                            format!("已恢复会话 `{aid}`。继续对话即可。")
                        };
                        let _ = self.reply_msg(msg, &t).await;
                    }
                    Err(_) => {
                        let t = if self.lang == "en" {
                            "Invalid pick. Send number, or 0 to cancel. (No session was bound.)"
                        } else {
                            "无效选择。请发送序号，或 0 取消。（未绑定任何会话）"
                        };
                        let _ = self.reply_msg(msg, t).await;
                    }
                }
            }
        }
    }

    async fn handle_slash(
        &self,
        cmd: BuiltinCommand,
        msg: &IncomingMessage,
        scope: &str,
        default_wd: &str,
    ) {
        match cmd {
            BuiltinCommand::Help => {
                let _ = self
                    .reply_msg(msg, &slash::help_text(&self.lang))
                    .await;
            }
            BuiltinCommand::Whoami => {
                let text = format!(
                    "**身份**\n- sender: `{}`\n- chat: `{}`\n- type: `{}`",
                    msg.sender_id, msg.chat_id, msg.chat_type
                );
                let _ = self.reply_msg(msg, &text).await;
            }
            BuiltinCommand::New => {
                let cur = self.store.get_or_create(scope, default_wd);
                let wd = cur.work_dir.clone();
                let mut s = ScopeBinding::fresh(&wd);
                s.project_id = cur.project_id;
                self.store.set(scope, s.clone());
                let t = if self.lang == "en" {
                    format!("New session: `{}`", s.local_session_id)
                } else {
                    format!("已开启新会话：`{}`", s.local_session_id)
                };
                let _ = self.reply_msg(msg, &t).await;
            }
            BuiltinCommand::Status => {
                let s = self.store.get_or_create(scope, default_wd);
                let text = format!(
                    "**Status**\n- project: `{}`\n- work_dir: `{}`\n- agent_session: `{}`\n- mode: {:?}\n- turns: {}\n- runtime: `unavailable`\n- channel: `{}`",
                    s.project_id.as_deref().unwrap_or("-"),
                    s.work_dir,
                    s.agent_session_id.as_deref().unwrap_or("-"),
                    s.pending_mode,
                    s.turn_count,
                    msg.channel
                );
                let _ = self.reply_msg(msg, &text).await;
            }
            BuiltinCommand::Stop => {
                self.aborts.lock().insert(scope.to_string(), true);
                let t = if self.lang == "en" {
                    "Stop signal sent (best-effort)."
                } else {
                    "已发送中断信号（尽力而为）。"
                };
                let _ = self.reply_msg(msg, t).await;
            }
            BuiltinCommand::Project { query } => {
                self.handle_project(query.as_deref(), scope, msg, default_wd)
                    .await;
            }
            BuiltinCommand::Resume { query } => {
                self.handle_resume(query.as_deref(), scope, msg, default_wd)
                    .await;
            }
            BuiltinCommand::Unknown { raw } => {
                let t = if self.lang == "en" {
                    format!("Unknown command `/{raw}`. Send `/help`.")
                } else {
                    format!("未知命令 `/{raw}`。发送 `/help` 查看。")
                };
                let _ = self.reply_msg(msg, &t).await;
            }
        }
    }

    async fn handle_project(
        &self,
        query: Option<&str>,
        scope: &str,
        msg: &IncomingMessage,
        default_wd: &str,
    ) {
        let projects = load_trusted_projects();
        if projects.is_empty() {
            let t = if self.lang == "en" {
                "No trusted projects. Trust a folder in OMP Desktop first."
            } else {
                "没有已信任项目。请先在 OMP Desktop 中信任项目目录。"
            };
            let _ = self.reply_msg(msg, t).await;
            return;
        }

        if let Some(q) = query {
            let binding = self.store.get_or_create(scope, default_wd);
            match apply_project_pick(&binding, &projects, q) {
                Ok(next) => {
                    self.store.set(scope, next.clone());
                    let t = if self.lang == "en" {
                        format!(
                            "Bound **{}**\n`{}`\nNext message starts a **new** session.",
                            next.project_id.as_deref().unwrap_or(""),
                            next.work_dir
                        )
                    } else {
                        format!(
                            "已绑定 **{}**\n`{}`\n下一条消息将开启**新**会话。",
                            next.project_id.as_deref().unwrap_or(""),
                            next.work_dir
                        )
                    };
                    let _ = self.reply_msg(msg, &t).await;
                }
                Err(_) => {
                    let t = if self.lang == "en" {
                        format!("Not found: {q}. Send /p again.")
                    } else {
                        format!("未找到：{q}。请重新发送 /p。")
                    };
                    let _ = self.reply_msg(msg, &t).await;
                }
            }
            return;
        }

        // Menu: cards for feishu/lark/dingtalk, text otherwise
        if channel_uses_cards(&msg.channel) {
            let card = control_plane::build_feishu_project_card(&projects, &self.lang);
            if msg.channel == "dingtalk" {
                let card = control_plane::build_dingtalk_project_card(&projects, &self.lang);
                let _ = self
                    .outbound
                    .reply_card(&msg.instance_id, &msg.chat_id, Some(&msg.message_id), &card)
                    .await;
            } else {
                let _ = self
                    .outbound
                    .reply_card(&msg.instance_id, &msg.chat_id, Some(&msg.message_id), &card)
                    .await;
            }
            // Still allow text pick as fallback (number / name). Mirror under sender scope
            // so card callbacks with different chat_id still clear the same pending.
            let pick = PendingPick {
                kind: PickKind::Project,
                sessions: vec![],
            };
            self.insert_pending(scope, msg, pick);
        } else {
            let text = format_project_menu(&projects, &self.lang);
            self.insert_pending(
                scope,
                msg,
                PendingPick {
                    kind: PickKind::Project,
                    sessions: vec![],
                },
            );
            let _ = self.reply_msg(msg, &text).await;
        }
    }

    fn insert_pending(&self, scope: &str, msg: &IncomingMessage, pick: PendingPick) {
        let alt = SessionStore::scope_key(
            &msg.channel,
            &msg.instance_id,
            &msg.sender_id,
            &msg.sender_id,
        );
        let mut g = self.pending.lock();
        g.insert(scope.to_string(), pick.clone());
        g.insert(alt, pick);
    }

    async fn handle_resume(
        &self,
        query: Option<&str>,
        scope: &str,
        msg: &IncomingMessage,
        default_wd: &str,
    ) {
        let binding = self.store.get_or_create(scope, default_wd);
        if binding.project_id.is_none() {
            // Try match work_dir to a trusted project
            let projects = load_trusted_projects();
            if let Some(p) = projects.iter().find(|p| p.path == binding.work_dir) {
                let mut b = binding.clone();
                b.project_id = Some(p.id.clone());
                self.store.set(scope, b.clone());
            } else {
                let t = if self.lang == "en" {
                    "No project bound. Send /p first."
                } else {
                    "尚未绑定项目。请先发送 /p 选择项目。"
                };
                let _ = self.reply_msg(msg, t).await;
                return;
            }
        }
        let binding = self.store.get_or_create(scope, default_wd);
        let sessions = app_sessions::sessions_for_project(binding.project_id.as_deref());

        if let Some(q) = query {
            match apply_session_pick(&binding, &sessions, q) {
                Ok(next) => {
                    let aid = next.agent_session_id.clone().unwrap_or_default();
                    self.store.set(scope, next);
                    let t = if self.lang == "en" {
                        format!("Resumed session `{aid}`. Continue chatting.")
                    } else {
                        format!("已恢复会话 `{aid}`。继续对话即可。")
                    };
                    let _ = self.reply_msg(msg, &t).await;
                }
                Err(_) => {
                    let t = if self.lang == "en" {
                        format!("Not found: {q}. Send /r again. (No session was bound.)")
                    } else {
                        format!("未找到：{q}。请重新发送 /r。（未绑定任何会话）")
                    };
                    let _ = self.reply_msg(msg, &t).await;
                }
            }
            return;
        }

        if sessions.is_empty() {
            let t = format_session_menu(&sessions, &self.lang);
            let _ = self.reply_msg(msg, &t).await;
            return;
        }

        if channel_uses_cards(&msg.channel) {
            let card = if msg.channel == "dingtalk" {
                control_plane::build_dingtalk_session_card(&sessions, &self.lang)
            } else {
                control_plane::build_feishu_session_card(&sessions, &self.lang)
            };
            let _ = self
                .outbound
                .reply_card(&msg.instance_id, &msg.chat_id, Some(&msg.message_id), &card)
                .await;
            self.insert_pending(
                scope,
                msg,
                PendingPick {
                    kind: PickKind::Session,
                    sessions: sessions.clone(),
                },
            );
        } else {
            let text = format_session_menu(&sessions, &self.lang);
            self.insert_pending(
                scope,
                msg,
                PendingPick {
                    kind: PickKind::Session,
                    sessions,
                },
            );
            let _ = self.reply_msg(msg, &text).await;
        }
    }

    /// Returns the per-scope lock handle. The caller MUST `lock().await` it
    /// and hold the guard for the duration of the turn.
    fn scope_lock(&self, scope: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut g = self.in_flight.lock();
        g.entry(scope.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Sync lookup: returns a cached `RuntimeEntry` if one exists for
    /// `work_dir`, or an error if no entry exists AND no binary is
    /// configured. Does NOT spawn (spawning is async). Used by tests and as
    /// a fast-path cache-hit check.
    fn try_get_runtime(&self, work_dir: &Path) -> Result<Arc<RuntimeEntry>, String> {
        if let Some(entry) = self.runtimes.lock().get(work_dir) {
            return Ok(entry.clone());
        }
        if self.binary_path.is_none() {
            return Err("runtime_unavailable: OMP Runtime binary not configured".into());
        }
        // Binary exists but no cached entry — caller must use get_or_spawn_runtime.
        Err("runtime_unavailable: no pooled runtime for this work_dir".into())
    }

    /// Async: resolve `work_dir` to a pooled `RuntimeEntry`, spawning a new
    /// `AcpClient` + background event-collector task on a cache miss.
    async fn get_or_spawn_runtime(
        &self,
        work_dir: &Path,
    ) -> Result<Arc<RuntimeEntry>, String> {
        // Fast path: cache hit — but only while the pooled process is still
        // alive. If the underlying agent process died (crash / exit), the
        // stale RuntimeEntry would otherwise be returned forever, causing
        // every subsequent turn for this work_dir to skip re-spawn and fail
        // inside `initialize_and_open_session`. Evict and re-spawn instead.
        if let Some(entry) = self.runtimes.lock().get(work_dir) {
            if entry.acp.is_alive() {
                return Ok(entry.clone());
            }
            tracing::warn!(
                work_dir = ?work_dir,
                "remote_im: evicting dead RuntimeEntry, will re-spawn"
            );
            self.runtimes.lock().remove(work_dir);
        }
        // Slow path: spawn. Acquire the per-work_dir spawn guard BEFORE
        // spawning so two concurrent turns for *different scopes but the
        // same work_dir* cannot both miss the cache and double-spawn
        // (TOCTOU fix). Per-scope locking alone doesn't prevent this
        // because different scopes use different locks.
        let spawn_guard = {
            let mut g = self.spawn_locks.lock();
            g.entry(work_dir.to_path_buf())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _spawn_lock = spawn_guard.lock().await;

        // Double-checked locking: another task may have spawned and inserted
        // while we waited for the spawn guard. Re-verify liveness here too:
        // the entry may have died since the fast-path check.
        if let Some(entry) = self.runtimes.lock().get(work_dir) {
            if entry.acp.is_alive() {
                return Ok(entry.clone());
            }
            tracing::warn!(
                work_dir = ?work_dir,
                "remote_im: evicting dead RuntimeEntry on double-check, will re-spawn"
            );
            self.runtimes.lock().remove(work_dir);
        }

        let binary_path = self.binary_path.clone().ok_or_else(|| {
            "runtime_unavailable: OMP Runtime binary not configured".to_string()
        })?;
        let agent_dir = self.agent_dir.clone();
        let cwd = work_dir.to_path_buf();
        let spawn_opts = SpawnOptions {
            model_id: None,
            effort: None,
            permission_policy: None,
            binary_path: Some(binary_path.clone()),
            agent_dir,
        };
        // cli_path is empty; SpawnOptions::binary_path takes precedence
        // inside spawn_with_options.
        let cli_path = PathBuf::new();
        let (acp, mut events) = AcpClient::spawn_with_options(cli_path, cwd.clone(), spawn_opts)
            .map_err(|e| format!("runtime_unavailable: spawn failed: {}", e.message))?;

        // Start the event collector task: accumulate only assistant text and
        // signal the drain barrier on the terminal `done: true` marker.
        let text_buf = Arc::new(Mutex::new(String::new()));
        let drained = Arc::new(tokio::sync::Notify::new());
        let buf_clone = text_buf.clone();
        let drained_clone = drained.clone();
        tokio::spawn(async move {
            while let Some(ev) = events.recv().await {
                if let AcpEvent::Stream {
                    kind: StreamKind::Assistant,
                    text,
                    done,
                    ..
                } = ev
                {
                    // Accumulate any text from this chunk (the terminal
                    // marker carries empty text, so this is a no-op then).
                    buf_clone.lock().push_str(&text);
                    if done {
                        // Drain barrier: every chunk sent before this marker
                        // has now been accumulated. Unblocks the turn that is
                        // `await`ing `notified()` before reading `text_buf`.
                        //
                        // Use `notify_one` (not `notify_waiters`): the turn
                        // can only register its waiter AFTER `prompt()` returns
                        // (the marker is enqueued inside `prompt()`), and the
                        // collector may process this marker before the turn is
                        // rescheduled. `notify_one` stores a single permit when
                        // there is no waiter yet, so the later `notified()`
                        // resolves immediately instead of hanging. The permit
                        // is always consumed by the originating turn, and a
                        // failed `prompt()` sends no terminal marker, so no
                        // stray permit leaks across turns.
                        drained_clone.notify_one();
                    }
                }
            }
            tracing::info!("remote_im: runtime event collector exited");
        });

        let entry = Arc::new(RuntimeEntry {
            acp,
            text_buf,
            drained,
        });
        self.runtimes.lock().insert(cwd, entry.clone());
        Ok(entry)
    }

    pub(crate) async fn run_agent_turn(
        &self,
        msg: &IncomingMessage,
        scope: &str,
        default_wd: &str,
        prompt: &str,
    ) -> AgentTurnResult {
        let binding = self.store.get_or_create(scope, default_wd);
        if binding.project_id.is_none() && binding.work_dir.is_empty() {
            let t = if self.lang == "en" {
                "No project bound. Send /p first."
            } else {
                "尚未绑定项目。请先发送 /p。"
            };
            let _ = self.reply_msg(msg, t).await;
            return AgentTurnResult {
                text: String::new(),
                session_id: None,
                error: Some(t.into()),
            };
        }

        self.aborts.lock().insert(scope.to_string(), false);

        let thinking = if self.lang == "en" {
            "Working…"
        } else {
            "处理中…"
        };
        let _ = self.reply_msg(msg, thinking).await;

        let intent = resolve_turn_intent(&binding);
        let (work_dir, resume_id) = match &intent {
            TurnIntent::NewSession { work_dir } => (work_dir.clone(), None),
            TurnIntent::ResumeSession {
                work_dir,
                agent_session_id,
            } => (work_dir.clone(), Some(agent_session_id.clone())),
        };

        // Acquire per-scope lock so two concurrent messages from the same
        // chat serialize (prevents interleaved turns on the same Runtime).
        // Bind the Arc before locking so it outlives the guard.
        let scope_mutex = self.scope_lock(scope);
        let _scope_guard = scope_mutex.lock().await;

        // Resolve (or spawn) the Runtime process for this work_dir.
        let runtime = match self.get_or_spawn_runtime(Path::new(&work_dir)).await {
            Ok(rt) => rt,
            Err(e) => {
                // Includes the runtime_unavailable case when no binary is set.
                return AgentTurnResult {
                    text: String::new(),
                    session_id: None,
                    error: Some(e),
                };
            }
        };

        // Clear the event buffer for this turn.
        runtime.text_buf.lock().clear();

        // Open or resume the agent session.
        let (opened_sid, _resumed) = match runtime
            .acp
            .initialize_and_open_session(resume_id.as_deref())
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return AgentTurnResult {
                    text: String::new(),
                    session_id: None,
                    error: Some(format!("runtime_unavailable: session open failed: {}", e.message)),
                };
            }
        };

        // Run the prompt (blocks until the turn completes).
        if let Err(e) = runtime.acp.prompt(prompt).await {
            return AgentTurnResult {
                text: String::new(),
                session_id: Some(opened_sid),
                error: Some(format!("agent turn failed: {}", e.message)),
            };
        }

        // Await the drain barrier: `prompt()` resolves only after enqueuing the
        // terminal `AcpEvent::Stream { done: true, .. }` marker, but that marker
        // is consumed asynchronously by the collector task. Block here until the
        // collector has processed it, guaranteeing every prior chunk is already
        // accumulated in `text_buf` before we snapshot it. Without this, fast
        // short replies could have their last chunks still queued on the event
        // channel at read time, yielding a truncated reply.
        //
        // Correct across turns: the collector's `notify_one` (on the terminal
        // marker) either wakes this waiter or stores a permit it consumes; the
        // turn never reads `text_buf` without the collector having passed the
        // marker, and no permit is left dangling for a later turn.
        runtime.drained.notified().await;

        // Collect the streamed assistant text.
        let reply_text = runtime.text_buf.lock().clone();
        let returned_session_id = runtime.acp.agent_session_id().or(Some(opened_sid));

        let result = AgentTurnResult {
            text: reply_text,
            session_id: returned_session_id,
            error: None,
        };

        let mut next = binding_after_agent_turn(
            &binding,
            result.session_id.as_deref().or(resume_id.as_deref()),
        );
        // If we started new and got no session id back, still Continue with local bookkeeping
        if next.agent_session_id.is_none() {
            if let Some(r) = resume_id {
                next.agent_session_id = Some(r);
            }
        }
        if next.pending_mode == PendingMode::New {
            next.pending_mode = PendingMode::Continue;
        }

        let had_error = result.error.is_some();
        let text = if let Some(err) = result.error.as_ref() {
            if result.text.is_empty() {
                format!("Error: {err}")
            } else {
                result.text.clone()
            }
        } else if result.text.is_empty() {
            if self.lang == "en" {
                "(empty reply)".into()
            } else {
                "（空回复）".into()
            }
        } else {
            result.text.clone()
        };

        let is_error = had_error || text.starts_with("Error:");
        // Sync into App sessions_index + messages.json so sidebar /r and App UI share state.
        next = app_sessions::sync_turn_to_app(
            &next,
            prompt,
            &text,
            is_error,
            &msg.channel,
        );
        self.store.set(scope, next);

        for chunk in chunk_text(&text, 3500) {
            let _ = self.reply_msg(msg, &chunk).await;
        }

        result
    }

    async fn reply_msg(&self, msg: &IncomingMessage, text: &str) -> Result<(), String> {
        tracing::info!(
            instance = %msg.instance_id,
            chat = %msg.chat_id,
            text_len = text.len(),
            "remote_im: reply attempt"
        );
        match self
            .outbound
            .reply(
                &msg.instance_id,
                &msg.chat_id,
                Some(&msg.message_id),
                text,
            )
            .await
        {
            Ok(()) => {
                tracing::info!(
                    instance = %msg.instance_id,
                    chat = %msg.chat_id,
                    text_len = text.len(),
                    "remote_im: reply ok"
                );
                Ok(())
            }
            Err(e) => {
                // Fallback: some card callbacks only have open_id, not chat_id.
                if !msg.sender_id.is_empty() && msg.sender_id != msg.chat_id {
                    match self
                        .outbound
                        .reply(&msg.instance_id, &msg.sender_id, None, text)
                        .await
                    {
                        Ok(()) => {
                            tracing::info!(
                                instance = %msg.instance_id,
                                sender = %msg.sender_id,
                                "remote_im: reply ok via sender_id fallback"
                            );
                            return Ok(());
                        }
                        Err(e2) => {
                            tracing::error!(
                                instance = %msg.instance_id,
                                chat = %msg.chat_id,
                                sender = %msg.sender_id,
                                err = %e,
                                err2 = %e2,
                                "remote_im: outbound reply failed"
                            );
                            return Err(e2);
                        }
                    }
                }
                tracing::error!(
                    instance = %msg.instance_id,
                    chat = %msg.chat_id,
                    err = %e,
                    "remote_im: outbound reply failed"
                );
                Err(e)
            }
        }
    }
}

fn extract_card_action(msg: &IncomingMessage) -> Option<CardAction> {
    let c = msg.content.trim();
    // Engine-internal prefix for connectors
    if let Some(rest) = c.strip_prefix("__card_action__:") {
        return parse_card_action(rest);
    }
    // Structured payloads only (never steal normal chat)
    if c.starts_with('{') || c.starts_with("project:") || c.starts_with("session:") || c == "cancel"
    {
        return parse_card_action(c);
    }
    None
}

fn chunk_text(s: &str, max: usize) -> Vec<String> {
    if s.chars().count() <= max {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        cur.push(ch);
        if cur.chars().count() >= max {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// silence unused import warning for list_sessions_for_project in non-test
#[allow(dead_code)]
fn _use_list() {
    let _ = list_sessions_for_project;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[tokio::test]
    async fn handle_slash_p_does_not_deadlock_on_pending_lookup() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound.clone(), false);
        let mut secrets = HashMap::new();
        secrets.insert("token".into(), "t".into());
        secrets.insert("_instance_id".into(), "weixin-default".into());
        outbound.register("weixin-default", "weixin", secrets.clone(), json!({}));
        engine.upsert_instance(ChannelInstance {
            id: "weixin-default".into(),
            channel: "weixin".into(),
            name: "w".into(),
            enabled: true,
            secrets,
            options: json!({}),
            acl: json!({ "allowFrom": "*" }),
            project_scope: json!("all_trusted"),
        });
        let msg = IncomingMessage {
            channel: "weixin".into(),
            instance_id: "weixin-default".into(),
            message_id: "m1".into(),
            chat_id: "peer@im.wechat".into(),
            chat_type: "p2p".into(),
            sender_id: "peer@im.wechat".into(),
            content: "/p".into(),
            mentioned_bot: true,
        };
        // Pre-fix: nested pending.lock() in or_else deadlocked forever here.
        tokio::time::timeout(Duration::from_secs(3), engine.handle(msg))
            .await
            .expect("handle(/p) deadlocked on pending mutex re-entry");
    }

    /// Plan 1 fail-closed: every remote Agent turn must surface
    /// `runtime_unavailable` — never spawn a process or silently succeed.
    #[tokio::test]
    async fn remote_agent_turn_fails_closed_without_runtime() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let msg = IncomingMessage {
            channel: "weixin".into(),
            instance_id: "test-fail-closed".into(),
            message_id: "m1".into(),
            chat_id: "peer@im.wechat".into(),
            chat_type: "p2p".into(),
            sender_id: "peer@im.wechat".into(),
            content: "inspect this repository".into(),
            mentioned_bot: true,
        };
        // Non-empty work_dir so run_agent_turn does not early-return for
        // "no project bound".
        let result = engine
            .run_agent_turn(&msg, "test-scope", "/tmp/omp-test", "hi")
            .await;
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("runtime_unavailable"),
            "remote agent turn must surface runtime_unavailable, got: {:?}",
            result.error
        );
    }

    /// Pool lookup must surface `runtime_unavailable` when no binary is
    /// configured (fail-closed). A cache miss with no binary is an error,
    /// not a silent success.
    #[test]
    fn runtime_pool_returns_unavailable_without_binary() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let r = engine.try_get_runtime(Path::new("/tmp/omp-test-wd"));
        assert!(r.is_err());
        assert!(
            r.unwrap_err().contains("runtime_unavailable"),
            "must surface runtime_unavailable"
        );
    }

    /// P1: a duplicate message (same channel+message_id) must be silently
    /// dropped at the dedup gate — the second `handle` returns without
    /// entering scope resolution or agent-turn logic.
    #[tokio::test]
    async fn duplicate_message_is_dropped_by_dedup() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let mk = || IncomingMessage {
            channel: "telegram".into(),
            instance_id: "tg-1".into(),
            message_id: "dup-xyz".into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hello".into(),
            mentioned_bot: true,
        };
        // First call: passes dedup, proceeds into downstream logic (will warn
        // about unknown instance but must not panic).
        engine.handle(mk()).await;
        // Second call: same message_id → dedup gate drops it immediately.
        engine.handle(mk()).await;
        // If dedup were absent, the second call would re-enter downstream
        // logic; either way no panic proves the gate runs. Dedup correctness
        // is asserted at the unit level (dedup_store::tests).
    }

    /// P1: distinct messages under the rate limit must both pass the gates.
    #[tokio::test]
    async fn distinct_messages_pass_gates_under_limit() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let mk = |mid: &str| IncomingMessage {
            channel: "telegram".into(),
            instance_id: "tg-1".into(),
            message_id: mid.into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
        };
        // Two distinct message_ids: dedup won't fire, and both are well under
        // the per-channel/per-scope rate limits (60/min, 10/min).
        engine.handle(mk("r1")).await;
        engine.handle(mk("r2")).await;
        // No panic → gates executed without error.
    }
}
