# AC-8.4 远程审批过期 + 防重放 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire approval expiry (TTL-bound in-memory `allow_remote_yolo` grant → `SpawnOptions.permission_policy`) and anti-replay (webhook freshness window + nonce cache) into the remote_im engine, flipping AC-8.4 FAIL → PASS.

**Architecture:** New `remote_im/replay_guard.rs` (pure, clock-injected) sits in `Engine::handle()` before dedup. `IncomingMessage` gains `timestamp`/`nonce`, populated by the wecom + LINE webhook adapters. Engine gains an in-memory `approval_expires_at` (never persisted, dies on restart per §11); `BridgeRuntime::set_config` grants/revokes it on the running engine via the runtime handle; `run_agent_turn` passes `effective_permission_policy()`.

**Tech Stack:** Rust (parking_lot Mutex, tokio), cargo test; spec `docs/superpowers/specs/2026-07-31-remote-approval-expiry-anti-replay-design.md`.

## Global Constraints

- 审批状态**内存化、重启失效**（master design §11，`omp-desktop-design.md:212`）——永不落盘。
- `DEFAULT_FRESHNESS_WINDOW_SECS = 300`（Slack 标准）；`DEFAULT_APPROVAL_TTL_SECS = 3600`。
- Stale/Replayed 丢弃只 `tracing::warn!`，**不回复发送者**（避免 oracle）。
- Mutex 一律 `parking_lot::Mutex`（`.lock()` 直接得 guard，无 unwrap）。
- 时间注入：纯函数收 `now: i64` 参数；生产代码用 `now_secs()`。测试只用 `_at` 变体，禁止真实 sleep。
- 每任务 TDD：先写失败测试 → 红 → 实现 → 绿 → 提交。
- 提交信息英文，格式 `feat(remote_im): … (AC-8.4)`。

---

### Task 1: ReplayGuard 模块（freshness window + nonce cache）

**Files:**
- Create: `src-tauri/src/remote_im/replay_guard.rs`
- Modify: `src-tauri/src/remote_im/mod.rs:17`（`mod rate_limiter;` 之后插入 `mod replay_guard;`）

**Interfaces:**
- Consumes: 无（纯模块）。
- Produces: `DEFAULT_FRESHNESS_WINDOW_SECS: i64 = 300`；`pub enum ReplayVerdict { Allow, Stale, Replayed }`；`ReplayGuard::new(window_secs: i64)`；`ReplayGuard::check(&self, channel: &str, timestamp: Option<i64>, nonce: Option<&str>, now: i64) -> ReplayVerdict`。Task 3 的 Engine gate 依赖这些名字。

- [ ] **Step 1: Write the failing tests + module skeleton**

Create `src-tauri/src/remote_im/replay_guard.rs`:

```rust
//! Anti-replay guard for webhook-sourced remote IM messages (AC-8.4).
//!
//! Slack-standard freshness window (signed timestamp within ±300s) plus an
//! in-memory nonce cache rejecting exact replays inside the window. Messages
//! without timestamp/nonce (WS / long-poll channels) pass through — their
//! transport is platform-authenticated and DedupStore covers redelivery.
//! Pure + clock-injected: `now` is always a parameter.

use parking_lot::Mutex;
use std::collections::HashMap;

/// Slack-standard webhook freshness window (seconds).
pub const DEFAULT_FRESHNESS_WINDOW_SECS: i64 = 300;

/// Lazy-sweep trigger for the nonce cache.
const SWEEP_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayVerdict {
    Allow,
    /// Timestamp outside the freshness window (old replay OR forged future).
    Stale,
    /// Nonce seen before and still inside its window.
    Replayed,
}

pub struct ReplayGuard {
    /// "channel|nonce" → expiry (unix secs).
    inner: Mutex<HashMap<String, i64>>,
    window_secs: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_timestamp_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), None, 1000), ReplayVerdict::Allow);
        // Exactly at the window edge is still allowed (not > window).
        assert_eq!(g.check("wecom", Some(700), None, 1000), ReplayVerdict::Allow);
    }

    #[test]
    fn stale_old_timestamp_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(600), None, 1000), ReplayVerdict::Stale);
    }

    #[test]
    fn forged_future_timestamp_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1400), None, 1000), ReplayVerdict::Stale);
    }

    #[test]
    fn nonce_replay_within_window_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), Some("n1"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", Some(1001), Some("n1"), 1001), ReplayVerdict::Replayed);
    }

    #[test]
    fn nonce_reuse_after_expiry_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), Some("n1"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", Some(1301), Some("n1"), 1301), ReplayVerdict::Allow);
    }

    #[test]
    fn no_timestamp_no_nonce_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("telegram", None, None, 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("line", None, Some(""), 1000), ReplayVerdict::Allow);
    }

    #[test]
    fn nonce_cache_isolated_per_channel() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", None, Some("shared"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("line", None, Some("shared"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", None, Some("shared"), 1001), ReplayVerdict::Replayed);
    }
}
```

Register in `src-tauri/src/remote_im/mod.rs` after `mod rate_limiter;` (line 17):

```rust
mod rate_limiter;
mod replay_guard;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib remote_im::replay_guard 2>&1 | tail -5`
Expected: FAIL — `ReplayVerdict`/`ReplayGuard::new`/`check` not implemented (E0433/E0425/E0599).

- [ ] **Step 3: Implement ReplayGuard**

Insert into `replay_guard.rs` after the struct definition (before `#[cfg(test)]`):

```rust
impl ReplayGuard {
    pub fn new(window_secs: i64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            window_secs,
        }
    }

    /// Check one inbound message. Order: freshness first, then nonce.
    pub fn check(
        &self,
        channel: &str,
        timestamp: Option<i64>,
        nonce: Option<&str>,
        now: i64,
    ) -> ReplayVerdict {
        if let Some(ts) = timestamp {
            if (now - ts).abs() > self.window_secs {
                return ReplayVerdict::Stale;
            }
        }
        if let Some(nonce) = nonce {
            if nonce.is_empty() {
                return ReplayVerdict::Allow;
            }
            let key = format!("{channel}|{nonce}");
            let mut map = self.inner.lock();
            if let Some(exp) = map.get(&key) {
                if *exp > now {
                    return ReplayVerdict::Replayed;
                }
            }
            map.insert(key, now + self.window_secs);
            if map.len() > SWEEP_THRESHOLD {
                map.retain(|_, exp| *exp > now);
            }
        }
        ReplayVerdict::Allow
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib remote_im::replay_guard 2>&1 | tail -3`
Expected: `test result: ok. 7 passed`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/remote_im/replay_guard.rs src-tauri/src/remote_im/mod.rs
git commit -m "feat(remote_im): ReplayGuard — webhook freshness window + nonce cache (AC-8.4)"
```

---

### Task 2: IncomingMessage 增加 timestamp/nonce 字段（22 处字面量机械化更新）

**Files:**
- Modify: `src-tauri/src/remote_im/types.rs:19-30`（`IncomingMessage` 结构体）
- Modify: 16 个文件的 22 处 `IncomingMessage { … }` 字面量（engine.rs ×5、wecom.rs ×2、feishu.rs ×2、dingtalk.rs ×2、line/discord/matrix/qq/qqbot/slack/telegram/weibo/weixin/wps_xiezuo 各 ×1）

**Interfaces:**
- Consumes: 无。
- Produces: `IncomingMessage.timestamp: Option<i64>`、`IncomingMessage.nonce: Option<String>`。Task 3 的 engine gate 与 wecom/line 适配器依赖这两个字段名。

- [ ] **Step 1: Add the fields**

In `src-tauri/src/remote_im/types.rs`, extend the struct:

```rust
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub channel: String,
    pub instance_id: String,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String, // p2p | group
    pub sender_id: String,
    pub content: String,
    pub mentioned_bot: bool,
    pub attachments: Vec<Attachment>,
    /// AC-8.4 anti-replay: webhook-supplied unix timestamp (seconds).
    /// `None` for WS / long-poll channels (platform-authenticated transport).
    pub timestamp: Option<i64>,
    /// AC-8.4 anti-replay: webhook-supplied nonce (or derived replay key,
    /// e.g. LINE uses "sig:replyToken"). `None` for WS / long-poll channels.
    pub nonce: Option<String>,
}
```

- [ ] **Step 2: Scripted literal update**

Run from repo root:

```bash
python3 - << 'PYEOF'
import io, re, glob

updated = 0
for path in glob.glob('src-tauri/src/remote_im/**/*.rs', recursive=True):
    if path.endswith('types.rs'):
        continue
    with io.open(path, encoding='utf-8') as f:
        lines = f.readlines()
    out = []
    i = 0
    n = 0
    while i < len(lines):
        out.append(lines[i])
        if 'IncomingMessage {' in lines[i]:
            # find the attachments line within the next 25 lines
            for j in range(i + 1, min(i + 25, len(lines))):
                if re.match(r'^\s*attachments\s*:', lines[j]):
                    indent = re.match(r'^(\s*)', lines[j]).group(1)
                    out.extend(lines[i + 1:j + 1])
                    out.append(f'{indent}timestamp: None,\n')
                    out.append(f'{indent}nonce: None,\n')
                    i = j
                    n += 1
                    break
        i += 1
    if n:
        with io.open(path, 'w', encoding='utf-8') as f:
            f.writelines(out)
        updated += n
        print(f'{path}: {n}')
print(f'total literals updated: {updated}')
PYEOF
```

Expected: `total literals updated: 22`（若某个文件的 attachments 行不在 25 行窗口内，脚本会漏——cargo build 报错处手工补 `timestamp: None, nonce: None,`）。

- [ ] **Step 3: Verify compile + full suite**

Run: `cd src-tauri && cargo test --lib remote_im 2>&1 | tail -3`
Expected: PASS（无新测试；全部既有测试编译+通过）。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/remote_im/
git commit -m "feat(remote_im): IncomingMessage timestamp/nonce fields (AC-8.4)"
```

---

### Task 3: Engine replay gate + wecom/LINE 适配器填充

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs`（字段、2 个构造函数、`handle()` :204-221、tests）
- Modify: `src-tauri/src/remote_im/channels/wecom.rs`（:262 提升 parse 位置；:283 字面量填充）
- Modify: `src-tauri/src/remote_im/channels/line.rs`（:121 字面量填充）

**Interfaces:**
- Consumes: Task 1 的 `ReplayGuard/ReplayVerdict/DEFAULT_FRESHNESS_WINDOW_SECS`；Task 2 的 `msg.timestamp/msg.nonce`。
- Produces: `Engine.replay_guard`（私有）；`now_secs() -> i64`（engine.rs 私有，Task 4 复用）。wecom 消息带 `timestamp`+`nonce`；LINE 消息带 `nonce = "{sig}:{replyToken}"`。

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `src-tauri/src/remote_im/engine.rs`（tests 与 Engine 同模块树，可直接访问私有 `engine.dedup`）：

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib remote_im::engine 2>&1 | tail -5`
Expected: FAIL — `now_secs` undefined（E0425）。

- [ ] **Step 3: Implement engine gate**

a) Top of `engine.rs`, add to the `super::` imports:

```rust
use super::replay_guard::{ReplayGuard, ReplayVerdict};
```

b) Add `now_secs` near the other free helpers (e.g. above `impl Engine`):

```rust
/// Current unix time in seconds (0 on clock-before-epoch, matching the
/// recovery module's defensive style).
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

c) Add the field to `struct Engine` after the `rate_limiter` field:

```rust
    /// Per-channel + per-scope request rate limiting (in-memory).
    rate_limiter: super::rate_limiter::RateLimiter,
    /// AC-8.4: webhook freshness + nonce anti-replay (in-memory).
    replay_guard: ReplayGuard,
```

d) Init in **both** `Engine::new` and `Engine::new_ephemeral` after the `rate_limiter` init:

```rust
            rate_limiter: super::rate_limiter::RateLimiter::new_default(),
            replay_guard: ReplayGuard::new(super::replay_guard::DEFAULT_FRESHNESS_WINDOW_SECS),
```

（`new_ephemeral_with_binary` 委托 `new_ephemeral`，无需改。）

e) In `handle()`, insert immediately after the entry `tracing::info!` block and BEFORE the dedup gate:

```rust
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
```

- [ ] **Step 4: wecom adapter — populate timestamp/nonce**

In `src-tauri/src/remote_im/channels/wecom.rs`, hoist the parse out of the token branch. Replace:

```rust
                    // When a callback token is configured, require a valid
                    // msg_signature so attackers cannot POST forged messages.
                    if let Some(token) = cb_token.as_deref() {
                        let (sig, ts, nonce) = parse_wecom_sig(&req);
```

with:

```rust
                    // AC-8.4: parse sig/timestamp/nonce unconditionally so the
                    // anti-replay guard sees them even without a callback token.
                    let (sig, ts, nonce) = parse_wecom_sig(&req);
                    // When a callback token is configured, require a valid
                    // msg_signature so attackers cannot POST forged messages.
                    if let Some(token) = cb_token.as_deref() {
```

Then in the webhook `IncomingMessage` literal (:283 area) replace the Task-2 placeholders:

```rust
                                timestamp: None,
                                nonce: None,
```

with:

```rust
                                timestamp: ts.and_then(|s| s.parse::<i64>().ok()),
                                nonce,
```

（`ts`/`nonce` 之前仅以 `as_deref()` 借用，此处按值消费，无冲突。WS 模式的另一个字面量 `parse_ws_msg` 保持 `None`。）

- [ ] **Step 5: LINE adapter — populate derived nonce**

In `src-tauri/src/remote_im/channels/line.rs` webhook literal (:121 area) replace:

```rust
                                    timestamp: None,
                                    nonce: None,
```

with:

```rust
                                    timestamp: None,
                                    // AC-8.4: LINE signs only the body, and one
                                    // POST may carry multiple events sharing the
                                    // signature — derive a per-event replay key.
                                    nonce: sig.as_ref().map(|s| format!("{s}:{reply_token}")),
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test --lib remote_im 2>&1 | tail -3`
Expected: PASS（+3 engine 测试）。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs src-tauri/src/remote_im/channels/wecom.rs src-tauri/src/remote_im/channels/line.rs
git commit -m "feat(remote_im): engine anti-replay gate + wecom/LINE ts/nonce population (AC-8.4)"
```

---

### Task 4: Engine 审批 TTL + spawn policy 接线

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs`（字段、2 个构造函数、新方法块、`run_agent_turn` :904、tests）

**Interfaces:**
- Consumes: Task 3 的 `now_secs()`。
- Produces: `pub const DEFAULT_APPROVAL_TTL_SECS: i64 = 3600`；`Engine::grant_approval(&self, ttl_secs: i64)`；`Engine::revoke_approval(&self)`；`Engine::approval_active(&self) -> bool`；`Engine::approval_expires_at(&self) -> Option<i64>`；`Engine::effective_permission_policy(&self) -> Option<String>`。Task 5 的 bridge/runtime 依赖全部这些名字。`pub(crate)` 测试变体：`grant_approval_at(now, ttl)`、`approval_active_at(now)`、`effective_permission_policy_at(now)`。

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `engine.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib remote_im::engine 2>&1 | tail -5`
Expected: FAIL — `grant_approval_at`/`approval_active_at`/… undefined（E0599）。

- [ ] **Step 3: Implement**

a) Add the const near the top of `engine.rs`（`use` 块之后）:

```rust
/// AC-8.4: default remote-approval TTL (seconds). In-memory only — the
/// grant dies on restart (master design §11: approvals are not persisted).
pub const DEFAULT_APPROVAL_TTL_SECS: i64 = 3600;
```

b) Add the field to `struct Engine` after `replay_guard`:

```rust
    /// AC-8.4: webhook freshness + nonce anti-replay (in-memory).
    replay_guard: ReplayGuard,
    /// AC-8.4: remote-approval expiry (unix secs). `None` = not granted.
    /// Never persisted — dies on restart; toggling yolo re-grants.
    approval_expires_at: Mutex<Option<i64>>,
```

c) Init in both constructors:

```rust
            replay_guard: ReplayGuard::new(super::replay_guard::DEFAULT_FRESHNESS_WINDOW_SECS),
            approval_expires_at: Mutex::new(None),
```

d) Add the method block to `impl Engine`（放在 `remove_instance` 之后）:

```rust
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
```

e) In `run_agent_turn`（:901-907 area）replace:

```rust
        let spawn_opts = SpawnOptions {
            model_id: None,
            effort: None,
            permission_policy: None,
```

with:

```rust
        let spawn_opts = SpawnOptions {
            model_id: None,
            effort: None,
            // AC-8.4: yolo only while the TTL-bound approval is active.
            permission_policy: self.effective_permission_policy(),
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib remote_im 2>&1 | tail -3`
Expected: PASS（+4 approval 测试）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): TTL-bound remote approval wired to spawn permission_policy (AC-8.4)"
```

---

### Task 5: bridge/runtime 接线 + DTO（Rust + TS 镜像）

**Files:**
- Modify: `src-tauri/src/remote_im/runtime.rs`（`RuntimeHandle` accessor + tests 模块）
- Modify: `src-tauri/src/remote_im/bridge.rs`（`set_config` :124-128 area；`status()` :100-111 area）
- Modify: `src-tauri/src/remote_im/mod.rs:42-57`（`BridgeStatusDto`）
- Modify: `src/lib/remoteIm/types.ts:73-89`（`BridgeStatus` TS 镜像）

**Interfaces:**
- Consumes: Task 4 的 `grant_approval/revoke_approval/approval_active/approval_expires_at/DEFAULT_APPROVAL_TTL_SECS`。
- Produces: `RuntimeHandle::engine(&self) -> &Arc<Engine>`（`pub(crate)`）；`BridgeStatusDto.approval_active: bool` + `approval_expires_at: Option<i64>`（serde camelCase → 前端 `approvalActive`/`approvalExpiresAt`）。

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/remote_im/runtime.rs`（若无 `#[cfg(test)] mod tests` 则新建；`use super::*;` 继承 watch/mpsc 导入——用与 struct 字段相同的 mpsc/watch flavor）:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// AC-8.4: the handle exposes the engine so the bridge can grant/revoke
    /// approval; a fresh engine starts inactive even with persisted yolo.
    #[tokio::test]
    async fn handle_engine_accessor_exposes_approval_state() {
        let (cancel_tx, _rx) = watch::channel(false);
        let (keepalive_tx, _keepalive_rx) = mpsc::channel::<IncomingMessage>(1);
        let engine = Arc::new(Engine::new_ephemeral(OutboundRouter::new(), true));
        let h = RuntimeHandle {
            cancel_tx,
            pump: tokio::spawn(async {}),
            connectors: vec![],
            _keepalive_tx: keepalive_tx,
            outbound: OutboundRouter::new(),
            engine,
        };
        assert!(!h.engine().approval_active());
        h.engine()
            .grant_approval(super::engine::DEFAULT_APPROVAL_TTL_SECS);
        assert!(h.engine().approval_active());
        assert!(h.engine().approval_expires_at().is_some());
        h.engine().revoke_approval();
        assert!(!h.engine().approval_active());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib remote_im::runtime 2>&1 | tail -5`
Expected: FAIL — `engine()` method not found（E0599）。

- [ ] **Step 3: Implement RuntimeHandle accessor**

In `impl RuntimeHandle`（`stop` 方法之前）:

```rust
    /// AC-8.4: bridge-side access for approval grant/revoke + status.
    pub(crate) fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }
```

- [ ] **Step 4: BridgeRuntime::set_config wiring**

In `src-tauri/src/remote_im/bridge.rs` replace:

```rust
        if let Some(y) = allow_remote_yolo {
            self.allow_remote_yolo = y;
        }
```

with:

```rust
        if let Some(y) = allow_remote_yolo {
            self.allow_remote_yolo = y;
            // AC-8.4: toggling yolo grants (TTL-bound) / revokes the
            // in-memory approval on the running engine, if any. A stopped
            // bridge has nothing to grant — the next start is inactive
            // anyway (approvals never survive restart).
            if let Some(h) = self.handle.as_ref() {
                if y {
                    h.engine()
                        .grant_approval(super::engine::DEFAULT_APPROVAL_TTL_SECS);
                } else {
                    h.engine().revoke_approval();
                }
            }
        }
```

- [ ] **Step 5: BridgeStatusDto fields (Rust + TS)**

In `src-tauri/src/remote_im/mod.rs` extend the DTO:

```rust
pub struct BridgeStatusDto {
    pub state: String,
    pub enabled: bool,
    pub lifecycle: String,
    pub allow_remote_yolo: bool,
    /// AC-8.4: in-memory approval state (TTL-bound; dies on restart).
    #[serde(default)]
    pub approval_active: bool,
    #[serde(default)]
    pub approval_expires_at: Option<i64>,
    pub connected_channels: Vec<ConnectedChannelDto>,
    pub last_error: Option<String>,
    pub mock: bool,
    /// Legacy field: now always `rust://in-process`.
    pub remote_bridge_path: Option<String>,
    /// `rust` | historical
    #[serde(default)]
    pub backend: Option<String>,
}
```

In `bridge.rs` status()（构建 DTO 处）after `allow_remote_yolo: self.allow_remote_yolo,` add:

```rust
            approval_active: self
                .handle
                .as_ref()
                .is_some_and(|h| h.engine().approval_active()),
            approval_expires_at: self
                .handle
                .as_ref()
                .and_then(|h| h.engine().approval_expires_at()),
```

（若该文件还有第二处构建 `BridgeStatusDto` 字面量——例如未运行时的默认 status——同样补 `approval_active: false, approval_expires_at: None,`；cargo build 会指出所有缺失处。）

In `src/lib/remoteIm/types.ts` extend `BridgeStatus`:

```ts
export type BridgeStatus = {
  state: BridgeRunState;
  enabled: boolean;
  lifecycle: BridgeLifecycle;
  allowRemoteYolo: boolean;
  /** AC-8.4: in-memory approval state (TTL-bound; dies on restart). */
  approvalActive?: boolean;
  approvalExpiresAt?: number | null;
  connectedChannels: Array<{
```

- [ ] **Step 6: Run tests + typecheck**

Run: `cd src-tauri && cargo test --lib remote_im 2>&1 | tail -3 && cd .. && pnpm typecheck 2>&1 | tail -2`
Expected: cargo PASS（+1 runtime 测试）；typecheck clean。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/remote_im/runtime.rs src-tauri/src/remote_im/bridge.rs src-tauri/src/remote_im/mod.rs src/lib/remoteIm/types.ts
git commit -m "feat(remote_im): bridge approval grant/revoke wiring + status DTO (AC-8.4)"
```

---

### Task 6: 矩阵 / 安全清单 / 覆盖审计 / memory + 最终门

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md`（AC-8.4 行 :146、per-channel 表 AC-8.4 列、Audit Summary FAIL 列表、verdict 计数）
- Modify: `docs/release/security-audit-checklist.md`（SA-R.5 :61）
- Modify: `docs/release/test-coverage-audit.md`（cargo 计数、remote_im 计数、gap 表行）
- Modify: memory `omp-desktop-roadmap-status.md` + `MEMORY.md`（仓库外，不提交）

- [ ] **Step 1: Flip AC-8.4 to PASS**

Read the AC-8.4 row (`grep -n "AC-8.4" docs/release/1.0-acceptance-matrix.md`) and replace Status/Evidence with:

```
| AC-8.4 | Approval expiry + anti-replay per channel | `cargo test` remote_im suite | PASS | Approval expiry: `allow_remote_yolo` toggle grants an in-memory TTL-bound approval (`engine.rs` `grant_approval`/`DEFAULT_APPROVAL_TTL_SECS=3600`; never persisted, dies on restart per §11); while active, remote turns spawn the Runtime with `permission_policy="yolo"` (4 tests: grant/expiry/revoke/restart). Anti-replay: `replay_guard.rs` — Slack-standard ±300s freshness window + per-channel nonce cache (7 tests); engine gate drops Stale/Replayed before dedup (3 tests); wecom populates query timestamp+nonce, LINE derives `sig:replyToken` per-event nonce. WS/long-poll channels rely on platform-authenticated transport + DedupStore (documented in spec). |
```

Per-channel 表（:154-168 area）AC-8.4 列全部 ✗ → ✓（机制覆盖所有渠道：engine 统一 gate；webhook 渠道另有 freshness+nonce）。先 `sed -n '150,170p'` 看表结构再逐行替换。

- [ ] **Step 2: SA-R.5 → PASS**

In `docs/release/security-audit-checklist.md` replace the SA-R.5 row's status with PASS，证据：`replay_guard.rs (7 tests) + engine approval TTL (4 tests) + gate wiring (3 tests) + runtime accessor (1 test)`。

- [ ] **Step 3: Recompute verdict counts + Audit Summary**

Run: `for v in PASS PARTIAL BLOCKED FAIL; do printf "%s: " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l | tr -d ' '; done`
Expected: PASS 35, PARTIAL 16, BLOCKED 102, FAIL 5。更新 counts 表（PASS 34→35，FAIL 6→5，FAIL 行注明 grep 口径不再含自匹配行问题——按实测填）。Audit Summary 的 "Release-blocking FAIL items" 中把 AC-8.4 项按 v1-transport 的 ~~strikethrough~~ + **done 2026-07-31** 模式标记已解决。

- [ ] **Step 4: test-coverage-audit.md**

- Rust suite 计数 453 → 468（+15：replay_guard 7 + engine 7[3 gate + 4 approval] + runtime 1），追加说明句。
- remote_im 模块行 69 → 84，Scope 追加 replay_guard/approval。
- TC-M.8 行 Evidence 更新 remote_im (84)。
- gap 表 "Remote approval expiry + anti-replay" 行 → ~~strikethrough~~ **Resolved 2026-07-31**（同 v1 transport 行模式）。

- [ ] **Step 5: Memory**

更新 `omp-desktop-roadmap-status.md`：description（剩余 5 FAIL → 4 FAIL）；新增 AC-8.4 完成 bullet（D1/D2/D3 决策、15 测试、commits）；"How to apply" 优先级列表删 AC-8.4（下一最高杠杆：AC-1.5 subagent 策略继承 → AC-1.13 trace correlation → AC-10.9 → AC-12.3）。同步 `MEMORY.md` 索引行。

- [ ] **Step 6: Final gates + commit**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3 && cd .. && pnpm test 2>&1 | tail -3 && pnpm typecheck 2>&1 | tail -2 && pnpm check:i18n 2>&1 | tail -2 && pnpm check:brand 2>&1 | tail -2 && pnpm check:provenance 2>&1 | tail -2 && pnpm check:legal 2>&1 | tail -2`
Expected: 全绿（cargo 468、vitest 835、i18n 1885×3）。

```bash
git add docs/release/1.0-acceptance-matrix.md docs/release/security-audit-checklist.md docs/release/test-coverage-audit.md
git commit -m "docs(release): flip AC-8.4 remote approval expiry + anti-replay to PASS"
```

（memory 文件在仓库外，不提交。）
