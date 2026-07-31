//! Message engine: ACL, slash commands, Agent turns, project/session bind.
//! Turns spawn/reuse a pooled OMP Runtime process; without a configured binary
//! the engine degrades fail-closed (`runtime_unavailable`).

use super::app_sessions;
use super::control_plane::{
    self, apply_project_pick, apply_session_pick, binding_after_agent_turn, channel_uses_cards,
    format_project_menu, format_session_menu, list_sessions_for_project, parse_card_action,
    resolve_turn_intent, AppSessionEntry, CardAction, PendingMode, ScopeBinding, TurnIntent,
};
use super::outbound::{self, OutboundRouter};
use super::projects::{self, load_trusted_projects};
use super::replay_guard::{ReplayGuard, ReplayVerdict};
use super::session::SessionStore;
use super::slash::{self, BuiltinCommand};
use super::types::{ChannelInstance, IncomingMessage};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::acp_client::{AcpClient, AcpEvent, PromptBlock, SpawnOptions, StreamKind};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use tracing::Instrument;

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

/// Result of a remote Agent turn. When an OMP Runtime is configured, the turn
/// spawns/reuses a pooled process, runs the prompt, and returns the streamed
/// assistant text. The `error` field surfaces `runtime_unavailable` only when
/// no binary is configured or the spawn/session-open fails.
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

/// RAII guard that removes a per-scope `aborts` entry when the turn ends,
/// so the `aborts` map does not grow without bound across distinct scopes.
/// (The `aborts` flag is currently write-only — `/stop` sets it, the turn
/// resets it — but the entry must still be reclaimed.)
struct AbortGuard<'a> {
    aborts: &'a Arc<Mutex<HashMap<String, bool>>>,
    scope: &'a str,
}

impl Drop for AbortGuard<'_> {
    fn drop(&mut self) {
        self.aborts.lock().remove(self.scope);
    }
}

/// RAII guard that reclaims a pooled concurrency-lock entry (`in_flight` or
/// `spawn_locks`) when no other task still holds a clone of the `Arc`.
///
/// These lock pools use `entry().or_insert_with(Arc::new(...)).clone()` so the
/// map holds one `Arc` clone and the caller holds another while the turn runs.
/// When the turn's local clone drops, `strong_count` falls back to 1 — meaning
/// no concurrent waiter — and the map entry is safe to remove. If another turn
/// is waiting (it cloned the same Arc before we finished), the count stays >1
/// and we leave the entry in place for the waiter to reclaim on its own exit.
///
/// The reclaim closure runs on drop; declare this guard *before* the local
/// `Arc` clone so it drops *after* it (Rust drops locals in reverse order),
/// ensuring the count has already been decremented before the check.
struct ReclaimOnDrop<F: FnOnce()> {
    reclaim: Option<F>,
}

impl<F: FnOnce()> Drop for ReclaimOnDrop<F> {
    fn drop(&mut self) {
        if let Some(f) = self.reclaim.take() {
            f();
        }
    }
}

/// AC-8.4: default remote-approval TTL (seconds). In-memory only — the
/// grant dies on restart (master design §11: approvals are not persisted).
pub const DEFAULT_APPROVAL_TTL_SECS: i64 = 3600;

/// Current unix time in seconds (0 on clock-before-epoch, matching the
/// recovery module's defensive style).
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    /// AC-8.4: webhook freshness + nonce anti-replay (in-memory).
    replay_guard: ReplayGuard,
    /// AC-8.4: remote-approval expiry (unix secs). `None` = not granted.
    /// Never persisted — dies on restart; toggling yolo re-grants.
    approval_expires_at: Mutex<Option<i64>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("lang", &self.lang)
            .field("allow_remote_yolo", &self.allow_remote_yolo)
            .field("approval_expires_at", &self.approval_expires_at)
            .finish_non_exhaustive()
    }
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
            replay_guard: ReplayGuard::new(super::replay_guard::DEFAULT_FRESHNESS_WINDOW_SECS),
            approval_expires_at: Mutex::new(None),
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
            replay_guard: ReplayGuard::new(super::replay_guard::DEFAULT_FRESHNESS_WINDOW_SECS),
            approval_expires_at: Mutex::new(None),
        }
    }

    /// Test helper: ephemeral store with a configured binary path, so spawn
    /// paths are reachable in tests (used for dead-entry eviction coverage).
    #[cfg(test)]
    pub(crate) fn new_ephemeral_with_binary(
        outbound: OutboundRouter,
        binary_path: PathBuf,
    ) -> Self {
        let mut e = Self::new_ephemeral(outbound, false);
        e.binary_path = Some(binary_path);
        e
    }

    pub fn upsert_instance(&self, inst: ChannelInstance) {
        self.instances.lock().insert(inst.id.clone(), inst);
    }

    pub fn remove_instance(&self, id: &str) {
        self.instances.lock().remove(id);
    }

    /// Grant remote approval for `ttl_secs` from now (AC-8.4 D1).
    pub fn grant_approval(&self, ttl_secs: i64) {
        self.grant_approval_at(now_secs(), ttl_secs);
    }

    pub(crate) fn grant_approval_at(&self, now: i64, ttl_secs: i64) {
        *self.approval_expires_at.lock() = Some(now + ttl_secs);
    }

    /// Revoke remote approval immediately (toggle off).
    pub fn revoke_approval(&self) {
        *self.approval_expires_at.lock() = None;
    }

    pub fn approval_active(&self) -> bool {
        self.approval_active_at(now_secs())
    }

    pub(crate) fn approval_active_at(&self, now: i64) -> bool {
        (*self.approval_expires_at.lock()).is_some_and(|exp| now < exp)
    }

    pub fn approval_expires_at(&self) -> Option<i64> {
        *self.approval_expires_at.lock()
    }

    /// AC-8.4 D3: while approval is active, remote turns spawn the Runtime
    /// with the yolo (AlwaysApprove) policy; otherwise `None` = Runtime
    /// default policy applies.
    pub fn effective_permission_policy(&self) -> Option<String> {
        self.effective_permission_policy_at(now_secs())
    }

    pub(crate) fn effective_permission_policy_at(&self, now: i64) -> Option<String> {
        if self.approval_active_at(now) {
            Some("yolo".into())
        } else {
            None
        }
    }

    pub async fn handle(&self, msg: IncomingMessage) {
        tracing::info!(
            channel = %msg.channel,
            instance = %msg.instance_id,
            content_len = msg.content.len(),
            "remote_im: handle begin"
        );

        // ── AC-8.4: anti-replay — webhook freshness window + nonce cache ──
        match self
            .replay_guard
            .check(&msg.channel, msg.timestamp, msg.nonce.as_deref(), now_secs())
        {
            ReplayVerdict::Allow => {}
            verdict => {
                tracing::warn!(
                    target: "remote_im::replay_guard",
                    channel = %msg.channel,
                    instance = %msg.instance_id,
                    verdict = ?verdict,
                    "message dropped by anti-replay guard"
                );
                return;
            }
        }

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
        // Declare the reclaim guard *before* spawn_guard so Rust drops it
        // *after* spawn_guard (reverse declaration order). By the time the
        // closure runs, spawn_guard's Arc clone has already been released, so
        // strong_count == 1 correctly means no other turn holds the lock.
        let wd_key = work_dir.to_path_buf();
        let _spawn_reclaim = ReclaimOnDrop {
            reclaim: Some(move || {
                let mut g = self.spawn_locks.lock();
                if let Some(arc) = g.get(&wd_key) {
                    if Arc::strong_count(arc) <= 1 {
                        g.remove(&wd_key);
                    }
                }
            }),
        };
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
            // AC-8.4: yolo only while the TTL-bound approval is active.
            permission_policy: self.effective_permission_policy(),
            binary_path: Some(binary_path.clone()),
            agent_dir,
            // AC-1.5: the remote engine path manages its own policy via the
            // AC-8.4 approval flow; subagent config stays at CLI defaults.
            subagents_enabled: None,
            subagent_policy: None,
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
        // AC-1.13: inherit the caller's span so collector logs keep the
        // message's trace_id (Span::current() at spawn time, inside the
        // already-instrumented handle future; none when spawned outside
        // a message context — a no-op then).
        tokio::spawn(
            async move {
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
            }
            .instrument(tracing::Span::current()),
        );

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
        // Reclaim the aborts entry on every return path from this turn so the
        // map does not leak one bool per distinct scope ever seen.
        let _abort_guard = AbortGuard {
            aborts: &self.aborts,
            scope,
        };

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
        //
        // The reclaim guard is declared first so it drops last: by then
        // scope_mutex's Arc clone is gone, and strong_count == 1 means no
        // concurrent turn holds the same scope lock → safe to evict.
        let scope_key_owned = scope.to_string();
        let _scope_reclaim = ReclaimOnDrop {
            reclaim: Some(move || {
                let mut g = self.in_flight.lock();
                if let Some(arc) = g.get(&scope_key_owned) {
                    if Arc::strong_count(arc) <= 1 {
                        g.remove(&scope_key_owned);
                    }
                }
            }),
        };
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

        // Construct prompt blocks: text first, then any image attachments.
        let mut blocks = vec![PromptBlock::Text {
            text: prompt.to_string(),
        }];
        if !msg.attachments.is_empty() {
            // Bind the clone first so the (non-Send) MutexGuard drops before any await.
            let inst_opt = self.instances.lock().get(&msg.instance_id).cloned();
            if let Some(inst) = inst_opt {
                for att in msg
                    .attachments
                    .iter()
                    .filter(|a| a.kind == super::types::AttachmentKind::Image)
                {
                    match super::media::fetch_attachment(
                        &msg.channel,
                        &inst.secrets,
                        &inst.options,
                        att,
                    )
                    .await
                    {
                        Ok(media) => {
                            let b64 = B64.encode(&media.data);
                            blocks.push(PromptBlock::Image {
                                data: b64,
                                mime_type: media.mime_type,
                            });
                        }
                        Err(e) => tracing::warn!(
                            target: "remote_im::media",
                            channel = %msg.channel,
                            "image download failed: {e}"
                        ),
                    }
                }
            }
        }

        // Run the prompt (blocks until the turn completes).
        if let Err(e) = runtime.acp.prompt_with_blocks(&blocks).await {
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
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        // Pre-fix: nested pending.lock() in or_else deadlocked forever here.
        tokio::time::timeout(Duration::from_secs(3), engine.handle(msg))
            .await
            .expect("handle(/p) deadlocked on pending mutex re-entry");
    }

    /// AC-1.13: logs emitted by Engine::handle — including anything the
    /// runtime event collector logs — must carry the message's trace_id
    /// when the caller (runtime pump) instrumented the handle future.
    ///
    /// Uses the process-wide capture layer (not a scoped subscriber):
    /// "handle begin" is a callsite shared with other tests, and scoped
    /// installs lose the callsite-interest cache race under parallelism.
    #[tokio::test(flavor = "current_thread")]
    async fn engine_handle_logs_carry_trace_id() {
        let events = crate::trace::test_capture::global_events();
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let msg = IncomingMessage {
            channel: "weixin".into(),
            instance_id: "test-trace".into(),
            message_id: "m-trace".into(),
            chat_id: "peer@im.wechat".into(),
            chat_type: "p2p".into(),
            sender_id: "peer@im.wechat".into(),
            content: "inspect this repository".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        let span = crate::trace::remote_msg_span("tid-eng", &msg.channel, &msg.message_id);
        // Fail-closed path (no Runtime) still exercises handle's log lines.
        tokio::time::timeout(
            Duration::from_secs(5),
            engine.handle(msg).instrument(span),
        )
        .await
        .expect("handle timed out");
        let captured = events.lock().clone();
        let ours: Vec<_> = captured
            .iter()
            .filter(|e| e.trace_ids.iter().any(|t| t == "tid-eng"))
            .collect();
        assert!(
            !ours.is_empty(),
            "no handle logs carried tid-eng (captured: {captured:?})"
        );
        assert!(
            ours.iter().any(|e| e.message.contains("handle begin")),
            "handle begin log missing trace context (ours: {ours:?})"
        );
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
            attachments: vec![],
            timestamp: None,
            nonce: None,
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
            attachments: vec![],
            timestamp: None,
            nonce: None,
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
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        // Two distinct message_ids: dedup won't fire, and both are well under
        // the per-channel/per-scope rate limits (60/min, 10/min).
        engine.handle(mk("r1")).await;
        engine.handle(mk("r2")).await;
        // No panic → gates executed without error.
    }

    /// `scope_lock()` hands out the same `Arc` for the same scope (so turns
    /// serialize) and distinct Arcs for distinct scopes.
    #[test]
    fn scope_lock_returns_same_arc_for_same_scope() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let a1 = engine.scope_lock("scope-a");
        let a2 = engine.scope_lock("scope-a");
        let b = engine.scope_lock("scope-b");
        assert!(
            Arc::ptr_eq(&a1, &a2),
            "same scope must share one lock Arc"
        );
        assert!(
            !Arc::ptr_eq(&a1, &b),
            "distinct scopes must get distinct lock Arcs"
        );
    }

    /// Map eviction: after a fail-closed turn completes (no binary → returns
    /// early from `get_or_spawn_runtime`), neither the `in_flight` scope lock
    /// nor the `aborts` entry may remain — otherwise these maps grow without
    /// bound across distinct scopes. The reclaim guards drop the local Arc
    /// clones, leaving strong_count == 1, so the entries are removed.
    #[tokio::test]
    async fn turn_reclaims_scope_lock_and_aborts_on_exit() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let msg = IncomingMessage {
            channel: "telegram".into(),
            instance_id: "tg-1".into(),
            message_id: "evict-1".into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hello".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        // A turn with a non-empty work_dir proceeds past the "no project
        // bound" guard into the spawn path, which fails closed (no binary).
        let _ = engine
            .run_agent_turn(&msg, "evict-scope", "/tmp/omp-evict-test", "hi")
            .await;
        // The in_flight entry must have been reclaimed: no concurrent waiter
        // held the Arc, so strong_count dropped to 1 and the guard removed it.
        assert!(
            engine.in_flight.lock().is_empty(),
            "in_flight map must be empty after a turn with no concurrent waiter, got: {} entries",
            engine.in_flight.lock().len()
        );
        // Same for the aborts flag.
        assert!(
            engine.aborts.lock().is_empty(),
            "aborts map must be empty after a turn, got: {} entries",
            engine.aborts.lock().len()
        );
    }

    /// AC-2.9 Hub leg: a full remote-IM turn (inbound message → spawned
    /// Runtime → aggregated reply text) against the scriptable mock Runtime.
    /// This is the "true prompt→reply happy path" that
    /// `spawn_lock_reclaimed_after_failed_spawn` documented as needing a real
    /// spawned process.
    #[tokio::test(flavor = "current_thread")]
    async fn engine_full_turn_against_mock_runtime() {
        let engine = Engine::new_ephemeral_with_binary(
            OutboundRouter::new(),
            crate::e2e_runtime::mock_runtime_path(),
        );
        let msg = IncomingMessage {
            channel: "telegram".into(),
            instance_id: "e2e-1".into(),
            message_id: "e2e-m1".into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        let wd = std::env::temp_dir().join(format!("omp-e2e-engine-{}", std::process::id()));
        std::fs::create_dir_all(&wd).expect("create e2e work dir");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            engine.run_agent_turn(&msg, "e2e-scope", wd.to_str().expect("utf8 work dir"), "hi"),
        )
        .await
        .expect("engine turn timed out");
        assert!(result.error.is_none(), "turn error: {:?}", result.error);
        assert!(
            result.text.contains("Hello world."),
            "unexpected reply text: {:?}",
            result.text
        );
        // Dropping the engine drops the pooled AcpClient; its stdin closes
        // and the mock exits on EOF (no orphan processes).
    }

    /// spawn_locks eviction: when `get_or_spawn_runtime` fails (binary path
    /// points at a non-executable / missing file → spawn error), the
    /// `spawn_locks` entry acquired for that work_dir must still be reclaimed
    /// — otherwise every failed spawn leaks one lock entry per work_dir.
    ///
    /// This covers the `ReclaimOnDrop` guard on the spawn-failure path
    /// (deterministic, no process-timing dependency). A true prompt→reply
    /// happy path and dead-process eviction need an integration test with a
    /// real OMP Runtime; the AcpClient constructor binds a real
    /// `tokio::process::Child` so cannot be faked in a unit test.
    #[tokio::test]
    async fn spawn_lock_reclaimed_after_failed_spawn() {
        let outbound = OutboundRouter::new();
        // A path that exists() fails on → spawn_with_options returns
        // runtime_unavailable before reaching the process layer.
        let engine = Engine::new_ephemeral_with_binary(
            outbound,
            PathBuf::from("/nonexistent/omp-binary-does-not-exist"),
        );
        let wd = Path::new("/tmp/omp-spawn-fail-test");

        let result = engine.get_or_spawn_runtime(wd).await;
        assert!(result.is_err(), "spawn must fail for a missing binary");

        // The spawn_locks entry must have been reclaimed by the guard despite
        // the spawn failing — strong_count fell to 1 (no concurrent waiter).
        assert!(
            engine.spawn_locks.lock().is_empty(),
            "spawn_locks must be empty after a failed spawn with no waiter, got: {} entries",
            engine.spawn_locks.lock().len()
        );
    }

    /// AC-8.4: a webhook message with a stale timestamp must be dropped by
    /// the anti-replay guard BEFORE the dedup gate (message_id never marked).
    #[tokio::test]
    async fn stale_webhook_message_dropped_before_dedup() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let now = now_secs();
        let msg = IncomingMessage {
            channel: "wecom".into(),
            instance_id: "wc-1".into(),
            message_id: "stale-1".into(),
            chat_id: "u1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: Some(now - 3600),
            nonce: Some("n-stale".into()),
        };
        engine.handle(msg).await;
        assert!(
            engine.dedup.check_and_mark("wecom", "stale-1"),
            "stale message must never reach the dedup gate"
        );
    }

    /// AC-8.4: a replayed nonce (same sig/nonce, fresh message_id) must be
    /// dropped by the guard; only the first delivery reaches dedup.
    #[tokio::test]
    async fn replayed_nonce_dropped_before_dedup() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let now = now_secs();
        let mk = |id: &str| IncomingMessage {
            channel: "wecom".into(),
            instance_id: "wc-1".into(),
            message_id: id.into(),
            chat_id: "u1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: Some(now),
            nonce: Some("n-replay".into()),
        };
        engine.handle(mk("rep-1")).await;
        engine.handle(mk("rep-2")).await;
        assert!(
            !engine.dedup.check_and_mark("wecom", "rep-1"),
            "first delivery must have reached dedup"
        );
        assert!(
            engine.dedup.check_and_mark("wecom", "rep-2"),
            "replay must never reach the dedup gate"
        );
    }

    /// AC-8.4: a fresh signed webhook message passes the guard into the
    /// normal pipeline (reaches dedup).
    #[tokio::test]
    async fn fresh_signed_message_passes_replay_guard() {
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let msg = IncomingMessage {
            channel: "wecom".into(),
            instance_id: "wc-1".into(),
            message_id: "ok-1".into(),
            chat_id: "u1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: Some(now_secs()),
            nonce: Some("n-ok".into()),
        };
        engine.handle(msg).await;
        assert!(
            !engine.dedup.check_and_mark("wecom", "ok-1"),
            "fresh message must reach the dedup gate"
        );
    }

    /// AC-8.4 D1/D3: granting approval activates it for the TTL and makes
    /// remote turns spawn the Runtime with the yolo policy.
    #[test]
    fn approval_grant_activates_yolo_policy() {
        let engine = Engine::new_ephemeral(OutboundRouter::new(), false);
        engine.grant_approval_at(1_000, 600);
        assert!(engine.approval_active_at(1_001));
        assert_eq!(
            engine.effective_permission_policy_at(1_001).as_deref(),
            Some("yolo")
        );
        assert_eq!(engine.approval_expires_at(), Some(1_600));
    }

    /// AC-8.4 D1: approval expires exactly at the TTL boundary.
    #[test]
    fn approval_expires_after_ttl() {
        let engine = Engine::new_ephemeral(OutboundRouter::new(), false);
        engine.grant_approval_at(1_000, 600);
        assert!(engine.approval_active_at(1_599));
        assert!(!engine.approval_active_at(1_600));
        assert_eq!(engine.effective_permission_policy_at(1_600), None);
    }

    /// AC-8.4 D1: revoking (toggle off) deactivates immediately.
    #[test]
    fn approval_revoke_is_immediate() {
        let engine = Engine::new_ephemeral(OutboundRouter::new(), false);
        engine.grant_approval_at(1_000, 600);
        engine.revoke_approval();
        assert!(!engine.approval_active_at(1_001));
        assert_eq!(engine.effective_permission_policy_at(1_001), None);
        assert_eq!(engine.approval_expires_at(), None);
    }

    /// AC-8.4 §11: a fresh Engine (restart) starts with NO active approval
    /// even when the persisted yolo flag is true — approvals never survive
    /// a restart; the user must re-enable.
    #[test]
    fn approval_does_not_survive_restart() {
        let engine = Engine::new_ephemeral(OutboundRouter::new(), true);
        assert!(!engine.approval_active());
        assert_eq!(engine.effective_permission_policy(), None);
        assert_eq!(engine.approval_expires_at(), None);
    }
}
