# v1 Transport Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `OmpExtension::request()` to dispatch real `_omp/desktop/v1/*` JSON-RPC calls through the negotiated session's `AcpClient`, replacing the Plan 2 fail-closed stub, while preserving fail-closed behavior when no transport is present.

**Architecture:** A new `V1Transport` trait abstracts the dispatch; `AcpClient` implements it by forwarding to its existing generic JSON-RPC `request()`. `OmpExtension` gains a `transport` slot set by `session_manager` at capability negotiation. `request()` adds a "no transport → runtime_unavailable" guard then forwards `_omp/desktop/v1/<method>`. All 4 existing contract tests stay green (they use `new()` = no transport).

**Tech Stack:** Rust (edition 2021), `async-trait` (new dep), `tokio`, `serde_json`, Tauri state.

**Spec:** [`docs/superpowers/specs/2026-07-31-v1-transport-injection-design.md`](../specs/2026-07-31-v1-transport-injection-design.md)

## Global Constraints

- Fail-closed MUST be preserved: `OmpExtension::new()` (no transport) returns `runtime_unavailable` for every request. All 4 existing contract tests in `omp_desktop_v1/tests/contract.rs` must still pass unchanged.
- The method passed to the transport is the FULL namespaced method `_omp/desktop/v1/<method>` (prefix applied exactly once).
- `V1Transport` must be object-safe (`Arc<dyn V1Transport>`) so a mock can be swapped in for tests — hence `async-trait`.
- No new Tauri command wrappers, no frontend changes, no real-Runtime E2E harness (focused scope).
- Transport errors map to `DesktopV1Error` code `runtime_unavailable` with a `detail` arg (precise code recovery is a later refinement).
- Run tests with `cargo test --manifest-path src-tauri/Cargo.toml`. Note: `store::tests::ensure_general_project_is_idempotent_and_not_removable` fails under filesystem sandboxing (known, not a bug) — run unsandboxed or ignore that one failure.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `src-tauri/Cargo.toml` | Add `async-trait = "0.1"` | Modify |
| `src-tauri/src/omp_desktop_v1/transport.rs` | `V1Transport` trait definition | Create |
| `src-tauri/src/omp_desktop_v1/mod.rs` | `transport` field, `set_transport`, new `request()` dispatch, `pub mod transport;` | Modify |
| `src-tauri/src/acp_client.rs` | `impl V1Transport for AcpClient` | Modify |
| `src-tauri/src/session_manager.rs` | Install/clear transport at capability negotiation (3 branches) | Modify |
| `src-tauri/src/omp_desktop_v1/tests/contract.rs` | Mock-transport dispatch tests | Modify |

---

### Task 1: Add `async-trait` dependency and create the `V1Transport` trait

**Files:**
- Modify: `src-tauri/Cargo.toml` (dependencies section, near `sha1`/`hmac`)
- Create: `src-tauri/src/omp_desktop_v1/transport.rs`
- Modify: `src-tauri/src/omp_desktop_v1/mod.rs` (add `pub mod transport;`)

**Interfaces:**
- Produces: `pub trait V1Transport: Send + Sync { async fn dispatch_v1(&self, full_method: &str, params: serde_json::Value) -> Result<serde_json::Value, String>; }` — consumed by Tasks 2, 3, 4.

- [ ] **Step 1: Add the dependency**

In `src-tauri/Cargo.toml`, in the `[dependencies]` block right after the `sha1 = "0.10"` line, add:

```toml
async-trait = "0.1"
```

- [ ] **Step 2: Create the trait file**

Create `src-tauri/src/omp_desktop_v1/transport.rs`:

```rust
//! `V1Transport` — abstraction over the wire used to dispatch
//! `_omp/desktop/v1/*` JSON-RPC calls to the OMP Runtime.
//!
//! Abstracted as a trait (object-safe via `async-trait`) so `OmpExtension`
//! can be unit-tested with a mock transport and stays decoupled from
//! `AcpClient` internals. In production the implementor is `AcpClient`
//! (see `acp_client.rs`); in tests it is a mock.

use async_trait::async_trait;
use serde_json::Value;

/// Dispatches a fully-qualified `_omp/desktop/v1/<method>` JSON-RPC call.
#[async_trait]
pub trait V1Transport: Send + Sync {
    /// Send the request and return the raw JSON-RPC result value.
    ///
    /// `full_method` already carries the `_omp/desktop/v1/` prefix. On any
    /// transport-layer failure returns `Err` with a short diagnostic string;
    /// `OmpExtension::request` maps that to a `runtime_unavailable` error.
    async fn dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String>;
}
```

- [ ] **Step 3: Register the module**

In `src-tauri/src/omp_desktop_v1/mod.rs`, the module declarations currently read:

```rust
pub mod capability;
pub mod errors;
pub mod generated;
pub mod ids;
```

Add `pub mod transport;` so they read:

```rust
pub mod capability;
pub mod errors;
pub mod generated;
pub mod ids;
pub mod transport;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: compiles (the trait is defined but not yet implemented/used; a dead-code warning is acceptable at this stage).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/omp_desktop_v1/transport.rs src-tauri/src/omp_desktop_v1/mod.rs
git commit -m "feat(v1): add V1Transport trait + async-trait dep (transport injection prep)"
```

---

### Task 2: Inject the transport into `OmpExtension` with mock-driven tests (TDD core)

**Files:**
- Modify: `src-tauri/src/omp_desktop_v1/mod.rs` (add `transport` field, `set_transport`, rewrite `request()`)
- Modify: `src-tauri/src/omp_desktop_v1/tests/contract.rs` (add mock transport + 2 tests)

**Interfaces:**
- Consumes: `V1Transport` from Task 1.
- Produces: `OmpExtension::set_transport(&self, t: Option<Arc<dyn V1Transport>>)` and a `request()` that forwards through the transport — consumed by Task 4 (session_manager wiring).

- [ ] **Step 1: Write the failing tests (mock transport)**

Append to `src-tauri/src/omp_desktop_v1/tests/contract.rs` (after the existing tests, before the final closing of the file). First add the imports at the top of the file — the current imports are:

```rust
use crate::omp_desktop_v1::generated::DesktopV1Capability;
use crate::omp_desktop_v1::ids::id_patterns;
use crate::omp_desktop_v1::OmpExtension;
```

Extend them to:

```rust
use crate::omp_desktop_v1::generated::DesktopV1Capability;
use crate::omp_desktop_v1::ids::id_patterns;
use crate::omp_desktop_v1::transport::V1Transport;
use crate::omp_desktop_v1::OmpExtension;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
```

Then append the mock + tests at the end of the file:

```rust
/// Test-only transport that records each call and returns a canned response.
struct MockTransport {
    calls: Mutex<Vec<(String, serde_json::Value)>>,
    response: Result<serde_json::Value, String>,
}

#[async_trait]
impl V1Transport for MockTransport {
    async fn dispatch_v1(
        &self,
        full_method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.calls
            .lock()
            .unwrap()
            .push((full_method.to_string(), params));
        self.response.clone()
    }
}

fn cap_with(method: &str) -> DesktopV1Capability {
    DesktopV1Capability {
        schema_version: 1,
        schema_digest: "test-digest".to_string(),
        methods: vec![format!("_omp/desktop/v1/{method}")],
        notifications: vec![],
        optional_features: vec![],
    }
}

#[tokio::test]
async fn request_dispatches_through_transport_when_present() {
    let client = OmpExtension::new();
    client.negotiate_capability(Some(cap_with("sessions.listAll"))).await;
    let mock = Arc::new(MockTransport {
        calls: Mutex::new(vec![]),
        response: Ok(serde_json::json!({ "sessions": [] })),
    });
    client.set_transport(Some(mock.clone())).await;

    let result = client
        .request("sessions.listAll", serde_json::json!({ "limit": 5 }))
        .await;
    assert_eq!(result.unwrap(), serde_json::json!({ "sessions": [] }));

    let calls = mock.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    // The transport receives the FULL namespaced method, prefix applied once.
    assert_eq!(calls[0].0, "_omp/desktop/v1/sessions.listAll");
    assert_eq!(calls[0].1, serde_json::json!({ "limit": 5 }));
}

#[tokio::test]
async fn request_maps_transport_error_to_runtime_unavailable() {
    let client = OmpExtension::new();
    client.negotiate_capability(Some(cap_with("mcp.list"))).await;
    let mock = Arc::new(MockTransport {
        calls: Mutex::new(vec![]),
        response: Err("boom".to_string()),
    });
    client.set_transport(Some(mock)).await;

    let err = client
        .request("mcp.list", serde_json::json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code, "runtime_unavailable");
    assert_eq!(
        err.args.get("detail").and_then(|d| d.as_str()),
        Some("boom")
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib omp_desktop_v1 2>&1 | tail -15`
Expected: compile error — `set_transport` not found on `OmpExtension` (the tests reference a method that doesn't exist yet).

- [ ] **Step 3: Add the `transport` field and `set_transport`**

In `src-tauri/src/omp_desktop_v1/mod.rs`, add the import near the top (the current imports include `use generated::DesktopV1Capability;` and `use std::sync::Arc;`). Add:

```rust
use transport::V1Transport;
```

Change the struct definition from:

```rust
pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    // When Plan 3 wires the real transport, this will hold a reference to the
    // `AcpClient` so `request` can issue the actual JSON-RPC call. For Plan 2
    // the field is intentionally absent and every advertised method also
    // returns `runtime_unavailable`.
}
```

to:

```rust
pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    /// The live transport (the negotiated session's `AcpClient` in production,
    /// a mock in tests). `None` ⇒ fail-closed: every request returns
    /// `runtime_unavailable` even if a capability is negotiated.
    transport: Arc<RwLock<Option<Arc<dyn V1Transport>>>>,
}
```

Change `new()` from:

```rust
    pub fn new() -> Self {
        Self {
            capability: Arc::new(RwLock::new(None)),
        }
    }
```

to:

```rust
    pub fn new() -> Self {
        Self {
            capability: Arc::new(RwLock::new(None)),
            transport: Arc::new(RwLock::new(None)),
        }
    }
```

Add `set_transport` right after `negotiate_capability` (which ends with `*self.capability.write().await = cap;` `}`):

```rust
    /// Install (or clear with `None`) the transport used to dispatch v1 calls.
    ///
    /// Called by `session_manager` alongside `negotiate_capability` so the
    /// capability and its transport always come from the same live client.
    pub async fn set_transport(&self, t: Option<Arc<dyn V1Transport>>) {
        *self.transport.write().await = t;
    }
```

- [ ] **Step 4: Rewrite `request()` to add guard #3 and forward**

Replace the current `request()` body (the one ending in `let _ = params; ... Err(DesktopV1Error::runtime_unavailable())`) with:

```rust
    /// Dispatch a `_omp/desktop/v1/<method>` request.
    ///
    /// 1. No capability → `runtime_unavailable`.
    /// 2. Method not in the capability's method list → `unknown_method`.
    /// 3. No transport → `runtime_unavailable` (fail-closed; this is what the
    ///    Plan 2 contract tests exercise via `OmpExtension::new()`).
    /// 4. Forward `_omp/desktop/v1/<method>` through the transport.
    pub async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, DesktopV1Error> {
        let full_method = {
            let cap_guard = self.capability.read().await;
            let cap = match cap_guard.as_ref() {
                None => return Err(DesktopV1Error::runtime_unavailable()),
                Some(c) => c,
            };
            let full = format!("{NAMESPACE}{method}");
            if !cap.methods.contains(&full) {
                return Err(DesktopV1Error::new(
                    "unknown_method",
                    serde_json::json!({ "method": full }),
                ));
            }
            full
        };
        let transport = {
            let guard = self.transport.read().await;
            match guard.as_ref() {
                None => return Err(DesktopV1Error::runtime_unavailable()),
                Some(t) => Arc::clone(t),
            }
        };
        match transport.dispatch_v1(&full_method, params).await {
            Ok(v) => Ok(v),
            Err(e) => Err(DesktopV1Error::new(
                "runtime_unavailable",
                serde_json::json!({ "detail": e }),
            )),
        }
    }
```

- [ ] **Step 5: Run the v1 tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib omp_desktop_v1 2>&1 | tail -15`
Expected: all `omp_desktop_v1` tests pass, including the 2 new ones AND the 4 pre-existing contract tests (which now hit guard #3).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/omp_desktop_v1/mod.rs src-tauri/src/omp_desktop_v1/tests/contract.rs
git commit -m "feat(v1): inject transport into OmpExtension; dispatch real v1 calls (fail-closed preserved)"
```

---

### Task 3: Implement `V1Transport` for `AcpClient`

**Files:**
- Modify: `src-tauri/src/acp_client.rs` (add the trait impl)

**Interfaces:**
- Consumes: `V1Transport` (Task 1), the private `AcpClient::request()` (acp_client.rs:804).
- Produces: `Arc<AcpClient>` usable as `Arc<dyn V1Transport>` — consumed by Task 4.

- [ ] **Step 1: Add the impl**

In `src-tauri/src/acp_client.rs`, add the impl. Place it right after the `impl AcpClient { ... }` block that contains the private `request()` method (anywhere at module top level after `AcpClient` is defined works). `Value` is already imported in this file (`serde_json::Value`).

```rust
#[async_trait::async_trait]
impl crate::omp_desktop_v1::transport::V1Transport for AcpClient {
    async fn dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String> {
        // Forward to the existing generic JSON-RPC round-trip. The OMP Runtime's
        // DesktopV1Dispatcher routes `_omp/desktop/v1/*` to its handlers.
        self.request(full_method, params).await
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: compiles cleanly (the impl satisfies the trait; `request` is accessible because the impl is in the same module).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/acp_client.rs
git commit -m "feat(v1): AcpClient implements V1Transport (forwards to JSON-RPC request)"
```

---

### Task 4: Wire `session_manager` to install/clear the transport at negotiation

**Files:**
- Modify: `src-tauri/src/session_manager.rs:2736-2762` (the 3 capability-negotiation branches)

**Interfaces:**
- Consumes: `OmpExtension::set_transport` (Task 2), `Arc<AcpClient> as Arc<dyn V1Transport>` (Task 3).
- Produces: production wiring — when a session negotiates the v1 capability, its `AcpClient` becomes the singleton `OmpExtension`'s transport.

- [ ] **Step 1: Edit the capability-Some branch**

In `src-tauri/src/session_manager.rs`, the negotiation block currently reads (lines ~2736-2762):

```rust
                        if let Some(cap) =
                            crate::omp_desktop_v1::extract_capability_from_initialize(init)
                        {
                            let method_count = cap.methods.len();
                            omp_extension.negotiate_capability(Some(cap)).await;
                            tracing::info!(
                                target: "session",
                                session = %meta.id,
                                "omp desktop v1 capability negotiated: {} methods",
                                method_count
                            );
                        } else {
                            tracing::warn!(
                                target: "session",
                                session = %meta.id,
                                "OMP Runtime did not advertise _omp/desktop/v1 capability"
                            );
                            omp_extension.negotiate_capability(None).await;
                        }
                    } else {
                        tracing::warn!(
                            target: "session",
                            session = %meta.id,
                            "ACP initialize result missing — cannot negotiate v1 capability"
                        );
                        omp_extension.negotiate_capability(None).await;
                    }
```

Replace it with (transport installed/cleared in every branch, paired with the capability):

```rust
                        if let Some(cap) =
                            crate::omp_desktop_v1::extract_capability_from_initialize(init)
                        {
                            let method_count = cap.methods.len();
                            // Plan 3: install this session's AcpClient as the v1
                            // transport, paired with the capability it advertised.
                            let transport: Arc<
                                dyn crate::omp_desktop_v1::transport::V1Transport,
                            > = client.clone();
                            omp_extension.set_transport(Some(transport)).await;
                            omp_extension.negotiate_capability(Some(cap)).await;
                            tracing::info!(
                                target: "session",
                                session = %meta.id,
                                "omp desktop v1 capability negotiated: {} methods",
                                method_count
                            );
                        } else {
                            tracing::warn!(
                                target: "session",
                                session = %meta.id,
                                "OMP Runtime did not advertise _omp/desktop/v1 capability"
                            );
                            omp_extension.set_transport(None).await;
                            omp_extension.negotiate_capability(None).await;
                        }
                    } else {
                        tracing::warn!(
                            target: "session",
                            session = %meta.id,
                            "ACP initialize result missing — cannot negotiate v1 capability"
                        );
                        omp_extension.set_transport(None).await;
                        omp_extension.negotiate_capability(None).await;
                    }
```

Note: `client` is the `Arc<AcpClient>` for this session (moved into `s.acp` later at line ~2793); `client.clone()` here is a cheap Arc clone and leaves `client` available for that later move. `Arc` is already in scope in this file.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/session_manager.rs
git commit -m "feat(v1): session_manager installs AcpClient as OmpExtension transport at negotiation"
```

---

### Task 5: Full verification + acceptance-matrix sync

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (flip newly-verifiable items, fix Audit Summary counts)
- Modify: `docs/release/test-coverage-audit.md` (note v1 transport now wired)

**Interfaces:**
- Consumes: the working injection from Tasks 1-4.
- Produces: an accurate acceptance matrix reflecting that the v1 transport is now real and unit-tested.

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | grep "test result:" | head -3`
Expected: `test result: ok. 421 passed; 0 failed; 1 ignored` (419 prior + 2 new v1 tests). The single ignored is pre-existing. If `store::tests::ensure_general_project_is_idempotent_and_not_removable` fails, that is the known sandbox-only failure — re-run unsandboxed to confirm it passes.

- [ ] **Step 2: Run typecheck + the 4 check gates**

Run:
```bash
pnpm typecheck 2>&1 | tail -2
for s in check-brand-policy check-provenance check-i18n-completeness check-legal-baseline; do node scripts/$s.mjs >/dev/null 2>&1 && echo "$s: PASS" || echo "$s: FAIL"; done
```
Expected: typecheck OK; all 4 gates PASS.

- [ ] **Step 3: Update the acceptance matrix**

In `docs/release/1.0-acceptance-matrix.md`:
- For AC-1.2, AC-1.3, AC-1.4, AC-1.8, AC-1.9, AC-5.1, AC-5.2, and AC-2.7: append to the Evidence cell a note that the v1 transport is now wired and unit-tested (mock-transport dispatch tests), so the remaining gap is verification against a real configured Runtime (cross-platform acceptance), not the Desktop stub. Leave the Status as PARTIAL/BLOCKED (final PASS needs a live Runtime) but update the evidence text to remove "OmpExtension::request() is a fail-closed stub" as the blocker.
- In the **Audit Summary** "Verdict counts" table and "Top unblockers" list: item #1 (v1 transport injection) is now DONE — move it out of the unblocker list into a "Resolved in this work" note. Also correct the verdict-count table to match the actual matrix grep (run `grep -oE "\| (PASS|PARTIAL|BLOCKED|FAIL) \|" docs/release/1.0-acceptance-matrix.md | sort | uniq -c` and use the real numbers; the prior summary undercounted BLOCKED).

- [ ] **Step 4: Update test-coverage-audit.md**

In `docs/release/test-coverage-audit.md`, in the "Release-Blocking Gaps" table, mark the **v1 transport injection** row as resolved (e.g. change Priority to `✅ Done (2026-07-31)` and note the mock-transport tests + cargo count bump).

- [ ] **Step 5: Commit**

```bash
git add docs/release/1.0-acceptance-matrix.md docs/release/test-coverage-audit.md
git commit -m "docs(release): v1 transport wired — sync acceptance matrix + coverage audit"
```

---

## Self-Review Notes

- **Spec coverage:** §3.1 trait → Task 1; §3.2 AcpClient impl → Task 3; §3.3 OmpExtension field → Task 2; §3.4 session_manager wiring → Task 4; §4 dispatch logic + fail-closed → Task 2 Step 4; §5 error handling → Task 2 Step 4 (detail arg); §6 testing → Task 2 Step 1; §7 acceptance impact → Task 5. All covered.
- **Type consistency:** `V1Transport::dispatch_v1(&self, &str, Value) -> Result<Value, String>` used identically in trait (Task 1), impl (Task 3), mock (Task 2). `set_transport(Option<Arc<dyn V1Transport>>)` consistent across Task 2 (def) and Task 4 (use). `Arc<AcpClient>` → `Arc<dyn V1Transport>` coercion made explicit with a typed `let` in Task 4 (Option coercion isn't automatic).
- **Fail-closed preserved:** Task 2 keeps `new()` transport-less; existing test #3 now exercises guard #3 and still passes.
