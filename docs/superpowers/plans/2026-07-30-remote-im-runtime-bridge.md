# Remote IM Runtime Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fail-closed gate in `remote_im/engine.rs` with real OMP Runtime invocation so remote IM messages can drive agent turns end-to-end.

**Architecture:** The `Engine` gains a per-work_dir `AcpClient` pool. Each IM scope that reaches `run_agent_turn` resolves its `work_dir` to a pooled `AcpClient` (spawning one if needed via `AcpClient::spawn_with_options`), opens/resumes an agent session, runs `prompt()`, collects streamed assistant text from a background event task, and returns the reply through the existing `sync_turn_to_app` + `reply` path. Graceful degradation to `runtime_unavailable` is preserved when no binary is configured.

**Tech Stack:** Rust, Tokio async runtime, `acp_client.rs` (JSON-RPC over stdio), `parking_lot` Mutex, existing `remote_im` module.

## Global Constraints

- All changes are in `src-tauri/` (Rust). No frontend/TypeScript changes in this plan.
- `cargo test --manifest-path src-tauri/Cargo.toml` must pass with 0 failures (existing 384 tests + new tests).
- `cargo clippy --manifest-path src-tauri/Cargo.toml` must report 0 warnings (matches existing `eslint --max-warnings 0` posture).
- Do NOT touch any of the 14 channel adapters in `remote_im/channels/`.
- Do NOT change `control_plane.rs`, `session.rs`, `app_sessions.rs`, or `outbound.rs`.
- Do NOT introduce new crate dependencies — use existing `tokio`, `parking_lot`, `serde_json`, `uuid`.
- `Engine::new` / `Engine::new_ephemeral` existing callers must be updated to pass the new `binary_path` parameter.
- Brand policy: user-visible strings must not contain "grok" — use OMP/OMP Runtime terminology.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `src-tauri/src/remote_im/engine.rs` | Message engine; `run_agent_turn`; new AcpClient pool + event collector | Modify (main work) |
| `src-tauri/src/remote_im/runtime.rs` | `start_runtime()` entry point | Modify (thread `binary_path` through) |
| `src-tauri/src/remote_im/bridge.rs` | Bridge lifecycle; reads Settings | Modify (read `manual_cli_path`) |
| `src-tauri/src/remote_im/mod.rs` | Module root | Modify (if `start_runtime` signature change ripples) |

---

## Task 1: Add AcpClient pool and per-scope concurrency guard to Engine

This task adds the new fields and helper plumbing without changing `run_agent_turn` behavior yet. The fail-closed gate stays in place. It makes `Engine` *capable* of owning runtimes; the next task wires them in.

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs:39-82` (struct + constructors)

**Interfaces:**
- Consumes: `crate::acp_client::{AcpClient, AcpEvent, StreamKind}` (existing, at `src-tauri/src/acp_client.rs`)
- Produces: `Engine` with new fields `binary_path`, `agent_dir`, `runtimes`, `in_flight`; `get_or_spawn_runtime` method (stub returning Err in this task, filled in Task 2)

- [ ] **Step 1: Add new fields to Engine struct**

In `src-tauri/src/remote_im/engine.rs`, the struct currently is (lines 39-48):

```rust
pub struct Engine {
    store: SessionStore,
    outbound: OutboundRouter,
    instances: Arc<Mutex<HashMap<String, ChannelInstance>>>,
    pending: Arc<Mutex<HashMap<String, PendingPick>>>,
    aborts: Arc<Mutex<HashMap<String, bool>>>,
    lang: String,
    allow_remote_yolo: bool,
}
```

Replace with:

```rust
/// A pooled Runtime process keyed by work_dir.
struct RuntimeEntry {
    acp: Arc<AcpClient>,
    /// Accumulated assistant text from the event stream; cleared at the
    /// start of each turn, read after `prompt()` returns.
    text_buf: Arc<Mutex<String>>,
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
}
```

- [ ] **Step 2: Update imports at top of engine.rs**

Add to the `use` block at the top of `src-tauri/src/remote_im/engine.rs`:

```rust
use std::path::{Path, PathBuf};
use crate::acp_client::{AcpClient, AcpEvent, SpawnOptions, StreamKind};
```

(If `Path` is already imported, only add the missing items. Check existing imports first.)

- [ ] **Step 3: Update Engine constructors**

Replace `Engine::new` and `Engine::new_ephemeral` (currently lines 50-80):

```rust
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
    }
}
```

Note: `new_ephemeral` keeps `binary_path: None` so existing tests that expect fail-closed degradation keep passing unchanged.

- [ ] **Step 4: Add `acquire_scope_lock` helper for per-scope concurrency**

Add this method to `impl Engine` (before `run_agent_turn`):

```rust
/// Returns a guard that serializes turns for a given scope. The guard is
/// an owned `Arc<Mutex<()>>` held in a map so every concurrent caller for
/// the same scope contends on the same lock. Returns the lock guard once
/// acquired (caller holds it for the duration of the turn).
async fn acquire_scope_lock(&self, scope: &str) -> tokio::sync::MutexGuard<'_, ()> {
    let lock = {
        let mut g = self.in_flight.lock();
        g.entry(scope.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    lock.lock().await
}
```

Wait — this returns a guard bound to `&self`'s lifetime, which won't compile because the `Arc` is cloned out but the guard borrows the lock inside the map. Fix: return the cloned `Arc` and let the caller `.lock().await` it. Rewrite:

```rust
/// Returns the per-scope lock handle. The caller MUST `lock().await` it
/// and hold the guard for the duration of the turn.
fn scope_lock(&self, scope: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut g = self.in_flight.lock();
    g.entry(scope.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}
```

- [ ] **Step 5: Update the existing fail-closed test to use new constructor signature**

In `src-tauri/src/remote_im/engine.rs`, the test `remote_agent_turn_fails_closed_without_runtime` (around line 905) currently calls:

```rust
let engine = Engine::new_ephemeral(outbound, false);
```

This stays **unchanged** — `new_ephemeral` still takes 2 args. No test changes needed in this task.

But check: are there other callers of `Engine::new` in the codebase?

- [ ] **Step 6: Find and update all callers of `Engine::new`**

Run: `grep -rn "Engine::new(" src-tauri/src/`
Expected: `src-tauri/src/remote_im/runtime.rs` calls `Engine::new(outbound.clone(), allow_remote_yolo)`. This must be updated to pass `binary_path` and `agent_dir`. **This update happens in Task 4** (when `start_runtime` signature changes). For now, to keep compilation green, update `runtime.rs` to pass `None, None`:

```rust
let engine = Arc::new(Engine::new(outbound.clone(), allow_remote_yolo, None, None));
```

(Will be wired to real values in Task 4.)

- [ ] **Step 7: Run cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: compiles with no errors. The new fields exist but `get_or_spawn_runtime` and the real wiring come in Task 2/3. `PathBuf` import may be unused-warning if not referenced yet — acceptable temporarily, will be used in Task 2.

- [ ] **Step 8: Run existing tests to confirm no regression**

Run: `cargo test --manifest-path src-tauri/Cargo.toml remote_im 2>&1 | tail -15`
Expected: all remote_im tests pass (including `remote_agent_turn_fails_closed_without_runtime`).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs src-tauri/src/remote_im/runtime.rs
git commit -m "refactor(remote_im): add AcpClient pool + concurrency guard fields to Engine

Adds binary_path, agent_dir, runtimes (per-work_dir AcpClient pool),
and in_flight (per-scope concurrency) fields to Engine. Updates
constructor signatures. No behavior change yet — fail-closed gate
remains; pool wiring lands in the next task."
```

---

## Task 2: Implement `get_or_spawn_runtime` — pool management

This task implements the pool: resolve a `work_dir` to a cached `RuntimeEntry`, or spawn a new `AcpClient` + event-collector task if missing. No `run_agent_turn` changes yet.

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs` (add method to `impl Engine`)

**Interfaces:**
- Consumes: `AcpClient::spawn_with_options(cli_path, cwd, SpawnOptions) -> Result<(Arc<AcpClient>, UnboundedReceiver<AcpEvent>), AgentError>` (at `src-tauri/src/acp_client.rs:237`)
- Produces: `Engine::get_or_spawn_runtime(&self, work_dir: &Path) -> Result<Arc<RuntimeEntry>, String>`

- [ ] **Step 1: Write the failing test — pool caches by work_dir**

Add this test to the `#[cfg(test)]` block in `engine.rs`:

```rust
#[test]
fn runtime_pool_caches_by_work_dir() {
    let outbound = OutboundRouter::new();
    // binary_path None → get_or_spawn always returns Err, but we can
    // still verify the pool *map* logic by checking it doesn't double-spawn
    // on Err. The real caching test needs a binary; here we just verify
    // the None path returns a runtime_unavailable error string.
    let engine = Engine::new_ephemeral(outbound, false);
    let r = engine.get_or_spawn_blocking(Path::new("/tmp/omp-test-wd"));
    assert!(
        r.is_err(),
        "get_or_spawn must fail when binary_path is None"
    );
}
```

Note: `get_or_spawn_blocking` is a sync wrapper we'll add for testability (the real `get_or_spawn_runtime` is async; tests can use a blocking variant that calls it via `tauri::async_runtime::block_on` or we make the core logic sync and the async part thin). **Decision: make the pool lookup + spawn-decision sync (`get_or_spawn`), and have the async caller (`run_agent_turn`) await the spawn.** Rewrite the test:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml runtime_pool_returns_unavailable -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `no method named try_get_runtime found`.

- [ ] **Step 3: Implement `try_get_runtime` and `get_or_spawn_runtime`**

Add to `impl Engine`:

```rust
/// Sync lookup: returns a cached RuntimeEntry if one exists for work_dir,
/// or an error if no entry exists AND no binary is configured. Does NOT
/// spawn (spawning is async). Used by tests and as a fast-path check.
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

/// Async: resolve work_dir to a pooled RuntimeEntry, spawning a new
/// AcpClient + event collector if the pool misses.
async fn get_or_spawn_runtime(
    &self,
    work_dir: &Path,
) -> Result<Arc<RuntimeEntry>, String> {
    // Fast path: cache hit.
    if let Some(entry) = self.runtimes.lock().get(work_dir) {
        return Ok(entry.clone());
    }
    // Slow path: spawn.
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
    let cli_path = PathBuf::new();
    let (acp, mut events) = AcpClient::spawn_with_options(cli_path, cwd.clone(), spawn_opts)
        .map_err(|e| format!("runtime_unavailable: spawn failed: {}", e.message))?;

    // Start the event collector task.
    let text_buf = Arc::new(Mutex::new(String::new()));
    let buf_clone = text_buf.clone();
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let AcpEvent::Stream { kind: StreamKind::Assistant, text, .. } = ev {
                buf_clone.lock().push_str(&text);
            }
        }
        tracing::info!("remote_im: runtime event collector exited");
    });

    let entry = Arc::new(RuntimeEntry { acp, text_buf });
    self.runtimes.lock().insert(cwd, entry.clone());
    Ok(entry)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml runtime_pool_returns_unavailable -- --nocapture 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): implement AcpClient pool with event collector

get_or_spawn_runtime resolves work_dir to a cached RuntimeEntry or
spawns a new AcpClient + background event-collector task. try_get_runtime
provides a sync cache-hit check for tests. No run_agent_turn changes yet."
```

---

## Task 3: Replace fail-closed gate with real Runtime invocation in `run_agent_turn`

This is the core task: the fail-closed gate at `engine.rs:699-708` is replaced with `get_or_spawn_runtime` → `initialize_and_open_session` → `prompt` → read `text_buf`.

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs:659-708` (the `run_agent_turn` body)

**Interfaces:**
- Consumes: `Engine::get_or_spawn_runtime(work_dir) -> Result<Arc<RuntimeEntry>, String>` (from Task 2); `AcpClient::initialize_and_open_session(resume_id) -> Result<(String,bool), AgentError>`; `AcpClient::prompt(text) -> Result<(), AgentError>`; `AcpClient::agent_session_id() -> Option<String>` (all at `src-tauri/src/acp_client.rs`)
- Produces: a `run_agent_turn` that returns real agent text on success, `runtime_unavailable` on no-binary.

- [ ] **Step 1: Read the exact current body of run_agent_turn (lines 659-708) to know what to replace**

Run: `sed -n '659,708p' src-tauri/src/remote_im/engine.rs`

The key section to replace is lines 699-708 (the hardcoded `AgentTurnResult`). Lines 659-698 (setup: `resolve_turn_intent`, extracting `work_dir`/`resume_id`) stay as-is. Lines 709+ (post-turn: `binding_after_agent_turn`, `sync_turn_to_app`, `reply`) stay as-is.

- [ ] **Step 2: Replace the fail-closed gate (lines 699-708)**

Replace this block:

```rust
        // Plan 1 fail-closed: OMP Runtime is not connected in this build.
        // Every remote Agent turn surfaces `runtime_unavailable` — never
        // spawns a process or silently succeeds.
        let result = AgentTurnResult {
            text: String::new(),
            session_id: None,
            error: Some(
                "runtime_unavailable: OMP Runtime is not connected in this build.".into(),
            ),
        };
```

With:

```rust
        // Acquire per-scope lock so two concurrent messages from the same
        // chat serialize (prevents interleaved turns on the same Runtime).
        let _scope_guard = self.scope_lock(scope).lock().await;

        // Resolve (or spawn) the Runtime process for this work_dir.
        let runtime = match self.get_or_spawn_runtime(std::path::Path::new(work_dir)).await {
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

        // Collect the streamed assistant text.
        let reply_text = runtime.text_buf.lock().clone();
        let returned_session_id = runtime.acp.agent_session_id().or(Some(opened_sid));

        let result = AgentTurnResult {
            text: reply_text,
            session_id: returned_session_id,
            error: None,
        };
```

- [ ] **Step 3: Verify the existing fail-closed test still passes (degradation path)**

The test `remote_agent_turn_fails_closed_without_runtime` uses `Engine::new_ephemeral` (which sets `binary_path: None`). With the new code, `get_or_spawn_runtime` returns `Err("runtime_unavailable: ...")`, which is returned as `AgentTurnResult.error`. The test asserts `error.contains("runtime_unavailable")` — **still passes**.

Run: `cargo test --manifest-path src-tauri/Cargo.toml remote_agent_turn_fails_closed -- --nocapture 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 4: Run full remote_im test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml remote_im 2>&1 | tail -15`
Expected: all pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" | head -20`
Expected: 0 warnings (or only pre-existing ones — note any new warnings and fix them).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): replace fail-closed gate with real Runtime invocation

run_agent_turn now resolves a pooled AcpClient for the message's
work_dir, opens/resumes an agent session, runs prompt(), and collects
streamed assistant text. Falls back to runtime_unavailable when no
binary is configured (preserves degradation). Per-scope lock prevents
interleaved turns on the same Runtime process."
```

---

## Task 4: Wire binary_path and agent_dir from Settings through start_runtime

Thread the real `manual_cli_path` and `agent_grok_home` from app Settings into the Engine via `start_runtime` → `BridgeRuntime`.

**Files:**
- Modify: `src-tauri/src/remote_im/runtime.rs:37` (`start_runtime` signature)
- Modify: `src-tauri/src/remote_im/bridge.rs` (caller — reads Settings)

**Interfaces:**
- Consumes: `crate::store::Settings.manual_cli_path: Option<String>`, `crate::store::Settings.session_data_mode: String`, `crate::agent_prefs::agent_grok_home(&str) -> PathBuf`
- Produces: `start_runtime` that spawns Engines with real binary paths.

- [ ] **Step 1: Read current start_runtime signature and caller**

Run: `grep -n "start_runtime\|fn start_runtime" src-tauri/src/remote_im/runtime.rs src-tauri/src/remote_im/bridge.rs`

- [ ] **Step 2: Update start_runtime signature**

In `src-tauri/src/remote_im/runtime.rs`, change:

```rust
pub async fn start_runtime(
    allow_remote_yolo: bool,
) -> Result<(RuntimeHandle, Vec<ConnectedChannel>), String> {
```

To:

```rust
pub async fn start_runtime(
    allow_remote_yolo: bool,
    binary_path: Option<PathBuf>,
    agent_dir: Option<PathBuf>,
) -> Result<(RuntimeHandle, Vec<ConnectedChannel>), String> {
```

Add `use std::path::PathBuf;` to runtime.rs imports if not present.

- [ ] **Step 3: Update the Engine::new call inside start_runtime**

In `src-tauri/src/remote_im/runtime.rs` (the line that currently reads `Engine::new(outbound.clone(), allow_remote_yolo, None, None)` after Task 1 Step 6), change to:

```rust
    let engine = Arc::new(Engine::new(
        outbound.clone(),
        allow_remote_yolo,
        binary_path,
        agent_dir,
    ));
```

- [ ] **Step 4: Update the caller in bridge.rs**

Find where `start_runtime` is called in `src-tauri/src/remote_im/bridge.rs`. It currently calls something like `runtime::start_runtime(allow_remote_yolo)`. Change it to resolve Settings first:

```rust
    // Resolve binary path and agent dir from Settings (same source as SessionManager).
    let settings = crate::store::read_settings();
    let binary_path = settings
        .manual_cli_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists());
    let agent_dir = Some(crate::agent_prefs::agent_grok_home(&settings.session_data_mode));

    let (handle, active) = super::runtime::start_runtime(
        self.allow_remote_yolo,
        binary_path,
        agent_dir,
    ).await?;
```

Note: verify the exact function name for reading settings — check how `session_manager.rs` reads it. It may be `crate::store::load()` or `Settings::load()`. Run `grep -n "fn.*settings\|read_settings\|Settings::load\|store::load" src-tauri/src/store.rs | head` to confirm the exact API, and adjust the code above to match.

- [ ] **Step 5: Find and fix any other callers of start_runtime**

Run: `grep -rn "start_runtime" src-tauri/src/`
Update any test callers to pass `None, None` (tests don't have a real binary).

- [ ] **Step 6: Run cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: compiles. Fix any signature mismatches in callers.

- [ ] **Step 7: Run full test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15`
Expected: all 384+ tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/remote_im/runtime.rs src-tauri/src/remote_im/bridge.rs
git commit -m "feat(remote_im): wire binary_path and agent_dir from Settings

start_runtime now accepts binary_path and agent_dir, resolved from
Settings.manual_cli_path and agent_grok_home in bridge.rs — the same
source SessionManager uses. This connects remote_im to the real OMP
Runtime binary configured in Settings."
```

---

## Task 5: Update mod.rs / any remaining compilation breakage + final full-suite verification

Catch any remaining ripple from the constructor and `start_runtime` signature changes, then run the complete test + clippy gate.

**Files:**
- Modify: any file that still references old signatures (found via cargo check)

- [ ] **Step 1: Full cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -30`
Expected: clean. If errors, fix each one (they will be signature mismatches from Task 1/4 changes).

- [ ] **Step 2: Full test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: all tests pass, 0 failures.

- [ ] **Step 3: Clippy gate**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings 2>&1 | tail -20`
Expected: 0 warnings. Fix any new warnings introduced by this plan.

- [ ] **Step 4: Brand policy check (user-visible strings)**

Run: `node scripts/check-brand-policy.mjs 2>&1 | tail -5`
Expected: PASS (no new "grok" strings introduced).

- [ ] **Step 5: Commit if any fixes were needed**

```bash
git add -A
git commit -m "fix(remote_im): resolve remaining compilation + clippy breakage

Final pass: all signatures aligned, clippy clean, brand policy clean."
```

---

## Verification Record (fill in after implementation)

- `cargo test` result: ___ tests, 0 failures
- `cargo clippy` result: 0 warnings
- `check-brand-policy` result: PASS
- Manual E2E (if binary available): describe what was tested

## Notes for implementer

- The `Mutex` type in `Engine` is `parking_lot::Mutex` (not `std`) — verify this from the existing `use` statements. `parking_lot::Mutex::lock()` does not return a Result (infallible). The `in_flight` map uses `tokio::sync::Mutex` intentionally (async lock needed for the scope guard).
- `AgentError` has a public `.message: String` field (verified at `src-tauri/src/error.rs:52`).
- `AcpEvent::Stream` has fields `{ kind: StreamKind, text: String, message_id: Option<String>, done: bool }`. Only collect when `kind == StreamKind::Assistant`.
- `AgentTurnResult` is defined at `engine.rs:31` (not in `types.rs`).
