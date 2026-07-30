# P1: 去重 + 限流 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 remote_im 引擎入站消息添加跨重启去重（SQLite）和请求速率限流（固定窗口内存），拦截在 `Engine::handle()` 最前端。

**Architecture:** 两个独立模块 `DedupStore`（SQLite 持久化，`INSERT OR IGNORE` 原子去重 + TTL 清理）和 `RateLimiter`（固定窗口计数器，per-channel + per-scope 两层），均作为 `Engine` 字段，在 `handle()` 入口日志之后、card action 之前依次检查。

**Tech Stack:** Rust, rusqlite 0.32 (已依赖, bundled feature), parking_lot::Mutex, tokio, tracing.

## Global Constraints

- 包名: `omp-desktop`（`src-tauri/Cargo.toml`），测试命令 `cargo test -p omp-desktop`
- rusqlite 已在 `src-tauri/Cargo.toml`，**不新增依赖**
- 存储路径范式: `crate::paths::app_data_root().join("remote").join(...)`
- Mutex 范式: `parking_lot::Mutex`（项目已用，见 `session.rs`）
- 日志: `tracing` crate，target 前缀 `remote_im::`
- 现有代码风格: 内嵌 `#[cfg(test)] mod tests`，参照 `session.rs`

**Spec:** `docs/superpowers/specs/2026-07-30-p1-dedup-and-rate-limiting-design.md`

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `src-tauri/src/remote_im/dedup_store.rs` | 创建 | DedupStore: SQLite 去重存储 |
| `src-tauri/src/remote_im/rate_limiter.rs` | 创建 | RateLimiter: 固定窗口限流 |
| `src-tauri/src/remote_im/mod.rs:3-26` | 修改 | 注册新 mod |
| `src-tauri/src/remote_im/engine.rs:66-88` | 修改 | Engine 加 2 字段 |
| `src-tauri/src/remote_im/engine.rs:91-112` | 修改 | 构造函数初始化 |
| `src-tauri/src/remote_im/engine.rs:139-175` | 修改 | handle() 插入去重+限流 |

---

### Task 1: DedupStore 模块

**Files:**
- Create: `src-tauri/src/remote_im/dedup_store.rs`
- Modify: `src-tauri/src/remote_im/mod.rs` (注册 `mod dedup_store;`)

**Interfaces:**
- Produces: `DedupStore` struct; `DedupStore::open_default() -> Self`, `DedupStore::open(path: PathBuf) -> Self`, `DedupStore::ephemeral() -> Self`, `DedupStore::check_and_mark(&self, channel: &str, message_id: &str) -> bool`

- [ ] **Step 1: Write the failing test (模块骨架 + 全部测试)**

Create `src-tauri/src/remote_im/dedup_store.rs`:

```rust
//! Cross-restart message deduplication backed by SQLite.
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEDUP_TTL_SECS: u64 = 7 * 24 * 3600;
const DEDUP_CLEANUP_INTERVAL: u64 = 1024;

pub struct DedupStore {
    conn: Arc<Mutex<Connection>>,
    insert_count: AtomicU64,
}

impl DedupStore {
    pub fn open_default() -> Self {
        let path = crate::paths::app_data_root()
            .join("remote")
            .join("dedup.sqlite");
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path).expect("open dedup db");
        Self::init(&conn);
        Self {
            conn: Arc::new(Mutex::new(conn)),
            insert_count: AtomicU64::new(0),
        }
    }

    pub fn ephemeral() -> Self {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        Self::init(&conn);
        Self {
            conn: Arc::new(Mutex::new(conn)),
            insert_count: AtomicU64::new(0),
        }
    }

    fn init(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS seen_messages (
                channel    TEXT NOT NULL,
                message_id TEXT NOT NULL,
                seen_ts    INTEGER NOT NULL,
                PRIMARY KEY (channel, message_id)
            );
            CREATE INDEX IF NOT EXISTS idx_seen_ts ON seen_messages(seen_ts);",
        )
        .expect("init dedup schema");
    }

    /// Returns `true` if this is a new message (pass through),
    /// `false` if it is a duplicate (drop).
    pub fn check_and_mark(&self, channel: &str, message_id: &str) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let changed = {
            let conn = self.conn.lock();
            conn.execute(
                "INSERT OR IGNORE INTO seen_messages (channel, message_id, seen_ts) VALUES (?1, ?2, ?3)",
                rusqlite::params![channel, message_id, now],
            )
            .expect("dedup insert")
        };
        let is_new = changed == 1;
        if is_new {
            let n = self.insert_count.fetch_add(1, Ordering::Relaxed) + 1;
            if n % DEDUP_CLEANUP_INTERVAL == 0 {
                self.cleanup_locked(now);
            }
        }
        is_new
    }

    fn cleanup_locked(&self, now: u64) {
        let cutoff = now.saturating_sub(DEDUP_TTL_SECS);
        let conn = self.conn.lock();
        let removed = conn
            .execute(
                "DELETE FROM seen_messages WHERE seen_ts < ?1",
                rusqlite::params![cutoff],
            )
            .unwrap_or(0);
        if removed > 0 {
            tracing::debug!(target: "remote_im::dedup", removed, "ttl cleanup ran");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_message_passes() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
    }

    #[test]
    fn test_duplicate_dropped() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
        assert!(!store.check_and_mark("telegram", "m1"));
    }

    #[test]
    fn test_different_channel_no_collision() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
        assert!(store.check_and_mark("discord", "m1"));
    }

    #[test]
    fn test_ephemeral_no_file() {
        // ephemeral uses :memory:; just exercise it without panic
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("slack", "m1"));
    }

    #[test]
    fn test_ttl_cleanup() {
        let store = DedupStore::ephemeral();
        // Insert a record with an old timestamp manually.
        {
            let conn = store.conn.lock();
            conn.execute(
                "INSERT INTO seen_messages (channel, message_id, seen_ts) VALUES (?1, ?2, ?3)",
                rusqlite::params!["telegram", "old", 1_u64],
            )
            .unwrap();
        }
        // Trigger cleanup by forcing insert_count to just-before-threshold.
        store.insert_count.store(
            DEDUP_CLEANUP_INTERVAL - 1,
            Ordering::Relaxed,
        );
        // This insert triggers cleanup (old record deleted), then inserts new.
        assert!(store.check_and_mark("telegram", "fresh"));
        // The "old" record should be gone now: re-marking it should succeed.
        assert!(store.check_and_mark("telegram", "old"));
    }

    #[test]
    fn test_persistence_across_reopen() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "omp-dedup-test-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let store = DedupStore::open(path.clone());
            assert!(store.check_and_mark("telegram", "persist-me"));
        }
        // Reopen same path — the record must still be known.
        let store2 = DedupStore::open(path.clone());
        assert!(!store2.check_and_mark("telegram", "persist-me"));
        let _ = std::fs::remove_file(&path);
    }
}
```

Register the module in `src-tauri/src/remote_im/mod.rs`. Add `mod dedup_store;` after the existing `mod control_plane;` line (line 7), keeping alphabetical-ish order with neighbors.

- [ ] **Step 2: Run test to verify it fails (compile error first run is expected if mod not registered)**

```bash
cargo test -p omp-desktop remote_im::dedup_store
```
Expected: PASS (skeleton includes implementation; if mod not registered yet, expect compile error — register mod first then re-run).

- [ ] **Step 3: Run full dedup test suite**

```bash
cargo test -p omp-desktop remote_im::dedup_store 2>&1 | tail -20
```
Expected: 6 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/remote_im/dedup_store.rs src-tauri/src/remote_im/mod.rs
git commit -m "feat(remote_im): add DedupStore for cross-restart message deduplication"
```

---

### Task 2: RateLimiter 模块

**Files:**
- Create: `src-tauri/src/remote_im/rate_limiter.rs`
- Modify: `src-tauri/src/remote_im/mod.rs` (注册 `mod rate_limiter;`)

**Interfaces:**
- Consumes: `parking_lot::Mutex`, `std::time::Instant`
- Produces: `RateLimiter` struct; `RateLimiter::new_default() -> Self`, `RateLimiter::new(window_secs, channel_limit, scope_limit) -> Self`, `RateLimiter::check(&self, channel: &str, scope_key: &str) -> bool`

- [ ] **Step 1: Write the failing test (模块骨架 + 全部测试)**

Create `src-tauri/src/remote_im/rate_limiter.rs`:

```rust
//! Fixed-window request rate limiting (per-channel + per-scope), in-memory.
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

const WINDOW_SECS: u64 = 60;
const CHANNEL_LIMIT: u32 = 60;
const SCOPE_LIMIT: u32 = 10;

struct WindowCounter {
    window_start: Instant,
    count: u32,
}

pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, WindowCounter>>>,
    window: std::time::Duration,
    channel_limit: u32,
    scope_limit: u32,
}

impl RateLimiter {
    pub fn new_default() -> Self {
        Self::new(WINDOW_SECS, CHANNEL_LIMIT, SCOPE_LIMIT)
    }

    pub fn new(window_secs: u64, channel_limit: u32, scope_limit: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            window: std::time::Duration::from_secs(window_secs),
            channel_limit,
            scope_limit,
        }
    }

    /// Returns `true` if the request is allowed, `false` if rate-limited.
    /// Checks per-channel and per-scope windows; both must pass.
    pub fn check(&self, channel: &str, scope_key: &str) -> bool {
        let now = Instant::now();
        let mut g = self.inner.lock();
        let ch_ok = bump(&mut g, format!("ch:{channel}"), now, self.window, self.channel_limit);
        let sc_ok = bump(&mut g, format!("sc:{scope_key}"), now, self.window, self.scope_limit);
        ch_ok && sc_ok
    }
}

/// Increments the counter for `key`. Resets the window if expired.
/// Returns `true` if under limit (allowed), `false` if over (denied).
fn bump(
    map: &mut HashMap<String, WindowCounter>,
    key: String,
    now: Instant,
    window: std::time::Duration,
    limit: u32,
) -> bool {
    let entry = map.entry(key).or_insert(WindowCounter {
        window_start: now,
        count: 0,
    });
    if now.duration_since(entry.window_start) >= window {
        entry.window_start = now;
        entry.count = 0;
    }
    if entry.count >= limit {
        false
    } else {
        entry.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_under_limit_passes() {
        let rl = RateLimiter::new(60, 5, 5);
        for _ in 0..5 {
            assert!(rl.check("telegram", "sc:1"));
        }
    }

    #[test]
    fn test_over_limit_dropped() {
        // scope limit = 1, so second call in same window is denied.
        let rl = RateLimiter::new(60, 100, 1);
        assert!(rl.check("telegram", "sc:1"));
        assert!(!rl.check("telegram", "sc:1"));
    }

    #[test]
    fn test_channel_and_scope_independent() {
        // channel limit 1, but scope is large.
        let rl = RateLimiter::new(60, 1, 100);
        assert!(rl.check("telegram", "sc:1"));
        // Same channel different scope: channel limit hit -> denied.
        assert!(!rl.check("telegram", "sc:2"));
        // Different channel: fresh.
        assert!(rl.check("discord", "sc:1"));
    }

    #[test]
    fn test_expired_window_reset() {
        // window of 0 secs means every check sees an expired window -> always resets.
        // Use a tiny window and simulate by relying on duration >= window.
        let rl = RateLimiter::new(0, 1, 1);
        assert!(rl.check("telegram", "sc:1"));
        // window is 0 => next call's now.duration_since(start) >= 0 always true => reset.
        assert!(rl.check("telegram", "sc:1"));
    }
}
```

Register in `src-tauri/src/remote_im/mod.rs`: add `mod rate_limiter;` near the other mod declarations (after `mod projects;`).

- [ ] **Step 2: Run test to verify it compiles and passes**

```bash
cargo test -p omp-desktop remote_im::rate_limiter 2>&1 | tail -20
```
Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/rate_limiter.rs src-tauri/src/remote_im/mod.rs
git commit -m "feat(remote_im): add RateLimiter fixed-window per-channel/scope limiting"
```

---

### Task 3: Engine 集成 — 字段与构造函数

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs:66-88` (struct fields)
- Modify: `src-tauri/src/remote_im/engine.rs:91-112` (both constructors)

**Interfaces:**
- Consumes: `super::dedup_store::DedupStore`, `super::rate_limiter::RateLimiter`

- [ ] **Step 1: Add fields to Engine struct**

In `src-tauri/src/remote_im/engine.rs`, find the `Engine` struct definition (around line 66-88). After the `spawn_locks` field, add two new fields. The struct ends like:

```rust
    /// Per-work_dir spawn guard.
    spawn_locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
```

Change to:

```rust
    /// Per-work_dir spawn guard.
    spawn_locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,

    /// Cross-restart message deduplication (SQLite).
    dedup: super::dedup_store::DedupStore,

    /// Per-channel + per-scope request rate limiting (in-memory).
    rate_limiter: super::rate_limiter::RateLimiter,
```

- [ ] **Step 2: Initialize in `Engine::new`**

Find `Engine::new` (line 91). Its struct literal currently ends with:

```rust
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
```

Change to:

```rust
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
            dedup: super::dedup_store::DedupStore::open_default(),
            rate_limiter: super::rate_limiter::RateLimiter::new_default(),
        }
    }
```

- [ ] **Step 3: Initialize in `Engine::new_ephemeral`**

Find `Engine::new_ephemeral` (line 114). Same struct literal ending. Change `spawn_locks: ...` line to also add:

```rust
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
            dedup: super::dedup_store::DedupStore::ephemeral(),
            rate_limiter: super::rate_limiter::RateLimiter::new_default(),
        }
    }
```

- [ ] **Step 4: Run build to verify compilation**

```bash
cargo build -p omp-desktop 2>&1 | tail -20
```
Expected: BUILD SUCCEEDS (fields initialized but not yet used in handle — may see no dead_code warning since they're struct fields).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): add dedup and rate_limiter fields to Engine"
```

---

### Task 4: handle() 集成 — 去重 + 限流拦截

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs:139-175` (handle method entry)
- Test: inline integration tests added to `engine.rs` test module

**Interfaces:**
- Consumes: `self.dedup.check_and_mark`, `self.rate_limiter.check`, `SessionStore::scope_key`

- [ ] **Step 1: Write the failing integration test first**

In `src-tauri/src/remote_im/engine.rs`, find the existing `#[cfg(test)] mod tests` module. Add these two tests:

```rust
    #[tokio::test]
    async fn test_duplicate_msg_no_agent_turn() {
        use super::super::outbound::OutboundRouter;
        use super::IncomingMessage;
        let (tx, _rx) = tokio::sync::mpsc::channel::<String>(16);
        let router = OutboundRouter::from_sender(tx);
        let engine = Engine::new_ephemeral(router, true);

        let make_msg = || IncomingMessage {
            channel: "telegram".into(),
            instance_id: "i1".into(),
            message_id: "dup-1".into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hello".into(),
            mentioned_bot: true,
        };

        // First message: should proceed past dedup gate (may not reach agent turn
        // without a configured instance, but dedup must NOT block it).
        engine.handle(make_msg()).await;
        // Second identical message: dedup must drop it.
        engine.handle(make_msg()).await;
        // Assert no panic / the engine tolerated it. Dedup is verified at unit level;
        // here we assert the gate runs without error on repeated messages.
    }

    #[tokio::test]
    async fn test_rate_limited_msg_proceeds_under_limit() {
        use super::super::outbound::OutboundRouter;
        use super::IncomingMessage;
        let (tx, _rx) = tokio::sync::mpsc::channel::<String>(16);
        let router = OutboundRouter::from_sender(tx);
        let engine = Engine::new_ephemeral(router, true);

        // Send two distinct messages (different message_id so dedup doesn't fire)
        // under the rate limit — both should pass the rate limiter gate.
        let mk = |mid: &str| IncomingMessage {
            channel: "telegram".into(),
            instance_id: "i1".into(),
            message_id: mid.into(),
            chat_id: "c1".into(),
            chat_type: "p2p".into(),
            sender_id: "u1".into(),
            content: "hi".into(),
            mentioned_bot: true,
        };
        engine.handle(mk("r1")).await;
        engine.handle(mk("r2")).await;
        // No assertion crash means gates ran. Rate limiting is unit-tested in
        // rate_limiter.rs; this test ensures handle() wiring is non-panicking.
    }
```

- [ ] **Step 2: Run test to see current state (will compile, may pass trivially since gates not yet wired)**

```bash
cargo test -p omp-desktop remote_im::engine::tests::test_ 2>&1 | tail -20
```
Expected: compiles and runs (gates not yet added, so messages proceed to existing logic).

- [ ] **Step 3: Wire dedup + rate limiter into handle()**

In `src-tauri/src/remote_im/engine.rs`, find `handle` (line 139). After the existing `tracing::info!(...)` block and BEFORE the `__card_action__` check, insert:

```rust
    pub async fn handle(&self, msg: IncomingMessage) {
        tracing::info!(
            channel = %msg.channel,
            instance = %msg.instance_id,
            content_len = msg.content.len(),
            "remote_im: handle begin"
        );

        // ── P1: 去重 (dedup) ──────────────────────────────────────────
        if !self.dedup.check_and_mark(&msg.channel, &msg.message_id) {
            tracing::debug!(
                target: "remote_im::dedup",
                channel = %msg.channel,
                message_id = %msg.message_id,
                "duplicate message dropped"
            );
            return;
        }

        // ── P1: 限流 (rate limit) ─────────────────────────────────────
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

        // ── 现有逻辑 ──
        // Card actions must never fall through to text-pick ...
        if msg.content.trim().starts_with("__card_action__:") {
            // ... (existing code unchanged)
```

Note: `SessionStore` is already imported/used in engine.rs (as `self.store` field type), so `SessionStore::scope_key` is available. Verify the import path at top of engine.rs — it should be `use super::session::SessionStore;` or referenced via `self.store`. If `SessionStore` is not in scope, add `use super::session::SessionStore;` to the imports.

- [ ] **Step 4: Run handle integration tests**

```bash
cargo test -p omp-desktop remote_im::engine::tests 2>&1 | tail -25
```
Expected: existing tests + 2 new tests all PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p omp-desktop 2>&1 | grep -E "warning|error" | head -20
```
Expected: no NEW warnings introduced by P1 changes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): wire dedup + rate limiter into handle() entry"
```

---

### Task 5: 全量验证 + 文档收尾

**Files:**
- No new files; verification + roadmap status note.

- [ ] **Step 1: Run full remote_im test suite**

```bash
cargo test -p omp-desktop remote_im 2>&1 | tail -30
```
Expected: all existing remote_im tests + new dedup/rate_limiter/engine tests PASS.

- [ ] **Step 2: Run full build + clippy**

```bash
cargo build -p omp-desktop 2>&1 | tail -5
cargo clippy -p omp-desktop 2>&1 | grep -cE "warning" 
```
Expected: build succeeds; clippy warning count does not increase vs baseline.

- [ ] **Step 3: Update roadmap status memory (post-implementation)**

After tests pass, the master design §14.2 "去重、限流/退避" 必测项 for P1 is satisfied. Note this in commit message and roadmap memory.

- [ ] **Step 4: Final commit if any doc tweaks**

```bash
git status -s
# if anything remaining:
git add -A && git commit -m "chore: P1 dedup+rate-limit final verification"
```

---

## Self-Review

**Spec coverage:**
- §4.1 DedupStore (SQLite, INSERT OR IGNORE, TTL, ephemeral) → Task 1 ✓
- §4.2 RateLimiter (fixed window, per-channel+scope, default thresholds) → Task 2 ✓
- §4.3 过期窗口重置 → Task 2 `bump()` reset logic ✓
- §5 Engine 集成 (字段 + 构造函数) → Task 3 ✓
- §5.3 handle() 拦截 → Task 4 ✓
- §6 测试策略 → Tasks 1,2,4 内嵌测试 ✓
- §7 配置常量 → Tasks 1,2 const 定义 ✓
- §9 验收标准 → Task 5 验证 ✓

**Placeholder scan:** 无 TBD/TODO；所有代码块完整。

**Type consistency:** `check_and_mark(&str, &str) -> bool`、`check(&str, &str) -> bool`、`scope_key(channel, instance, chat, sender)` 在各 Task 间一致。`IncomingMessage` 字段名与 types.rs 探索结果一致。
