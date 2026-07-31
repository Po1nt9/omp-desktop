# OMP Desktop v1 Transport Injection — Design Spec

**Date:** 2026-07-31
**Status:** Approved (design confirmed; proceeding to implementation plan)
**Authority:** [Master Design §3 item 2/3](./2026-07-28-omp-desktop-design.md) · [Extension Protocol plan](../plans/2026-07-29-omp-desktop-extension-protocol.md) · [Plan 10 Audit Summary](../../release/1.0-acceptance-matrix.md#audit-summary)
**Plan 10 role:** Highest-leverage FAIL/PARTIAL unblocker identified by the Phase 2 code audit.

---

## 1. Problem

`OmpExtension` (`src-tauri/src/omp_desktop_v1/mod.rs`) is the Desktop-side client
for the versioned `_omp/desktop/v1/*` Extension Protocol. Its `request()` is a
**fail-closed stub**: even when a capability has been negotiated and the method is
advertised, it returns `runtime_unavailable` (mod.rs:89-92). The comment states
"Plan 3 will inject the AcpClient and dispatch the request here."

Consequence (from the Phase 2 audit): every `_omp/desktop/v1/*` call — skills.list,
mcp.list, diagnostics.selfCheck, extensions.list, and the ~13 other method groups
(queue/steer, credentials, sessions, providers, todo, subagents, rewind, usage,
config, media, branch) — cannot return real data. This is the single root cause
behind ~16 PARTIAL acceptance items (AC-1.2/1.3/1.4/1.8/1.9, AC-5.1/5.2, etc.) and
several FAILs.

**The OMP Runtime already implements the server side.** The submodule
(`runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/`) has a
`DesktopV1Dispatcher` plus ~17 handler modules (branch, config, credentials,
diagnostics, extensions, mcp, media, providers, queue, rewind, sessionConfig,
sessions, skills, steer, subagents, todo, usage) with contract tests. The Runtime
advertises all methods in its `initialize` capability descriptor (`queue`/`steer`
as `optionalFeatures`). The wire format is standard JSON-RPC with
`method = "_omp/desktop/v1/<shortName>"`.

So the Desktop injection is **generic forwarding** — one code path covers the whole
method surface; the Runtime does the per-method dispatch. Only the transport link is
missing.

## 2. Goal

Wire `OmpExtension::request()` to dispatch real `_omp/desktop/v1/*` JSON-RPC calls
through the negotiated session's `AcpClient`, while preserving the fail-closed
contract when no transport is present. Make the dispatch path unit-testable without
spawning a process.

**Non-goals (focused scope, user-confirmed):**
- New Tauri command wrappers for methods that have no Desktop consumer yet (YAGNI).
- Frontend `OmpDesktopV1Client` changes.
- A real-Runtime end-to-end test harness (depends on Runtime buildability).
- Precise recovery of structured Runtime error codes (noted as a later refinement).

## 3. Architecture

### 3.1 New trait — `V1Transport`

New file `src-tauri/src/omp_desktop_v1/transport.rs`:

```rust
use async_trait::async_trait;
use serde_json::Value;

/// Transport that can dispatch a fully-qualified `_omp/desktop/v1/*` JSON-RPC
/// call to the OMP Runtime and return the raw result value.
///
/// Abstracted as a trait so `OmpExtension` can be unit-tested with a mock
/// transport (no process spawning) and stays decoupled from `AcpClient`
/// internals.
#[async_trait]
pub trait V1Transport: Send + Sync {
    /// Dispatch `_omp/desktop/v1/<method>` (the caller passes the FULL method
    /// string including the namespace prefix). Returns the raw JSON-RPC result,
    /// or an error string from the transport layer.
    async fn dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String>;
}
```

`async-trait = "0.1"` is added to `src-tauri/Cargo.toml` (native `async fn` in
trait is not object-safe for the `Arc<dyn V1Transport>` we need; `OmpExtension`
lives in Tauri state as a concrete type but must swap in a mock for tests, so it
needs `dyn` dispatch).

### 3.2 `AcpClient` implements `V1Transport`

The impl lives in `src-tauri/src/acp_client.rs` (same module as the private
`request()` it forwards to):

```rust
#[async_trait]
impl crate::omp_desktop_v1::transport::V1Transport for AcpClient {
    async fn dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String> {
        self.request(full_method, params).await
    }
}
```

`AcpClient::request()` (acp_client.rs:804) is the existing generic JSON-RPC
round-trip over the ACP connection — it already handles id assignment, pending-map
correlation, write, and timeout. No change to it.

### 3.3 `OmpExtension` gains a transport slot

```rust
pub struct OmpExtension {
    capability: Arc<RwLock<Option<DesktopV1Capability>>>,
    transport:  Arc<RwLock<Option<Arc<dyn V1Transport>>>>,   // NEW
}
```

- `new()` → `transport = None` (fail-closed preserved).
- New method `set_transport(&self, t: Option<Arc<dyn V1Transport>>)`.
- `negotiate_capability` is unchanged (still sets capability only).

### 3.4 `session_manager` installs the transport at negotiation

At the capability-negotiation site (`session_manager.rs:2732`), the session's
`Arc<AcpClient>` (the same client whose `initialize_result` yielded the capability)
is installed as the transport, paired with the capability:

```rust
if let Some(cap) = extract_capability_from_initialize(init) {
    omp_extension.set_transport(Some(client.clone())).await;   // Arc<AcpClient> → Arc<dyn V1Transport>
    omp_extension.negotiate_capability(Some(cap)).await;
} else {
    omp_extension.set_transport(None).await;
    omp_extension.negotiate_capability(None).await;
}
```

Capability and transport always come from the same live client, so they stay
consistent. When that session closes, the next session to negotiate replaces both;
a stale/dead transport surfaces as a transport error → mapped to
`runtime_unavailable` (fail-closed).

## 4. Dispatch logic & fail-closed preservation

`request()` keeps its existing guards and adds one:

1. No capability → `runtime_unavailable` *(unchanged)*
2. Method not in the capability's `methods` list → `unknown_method` *(unchanged)*
3. **No transport → `runtime_unavailable`** *(new Plan 3 guard)*
4. Forward `_omp/desktop/v1/<method>` + params via `transport.dispatch_v1`; return
   the result.

```rust
pub async fn request(&self, method: &str, params: Value) -> Result<Value, DesktopV1Error> {
    let full_method = {
        let cap_guard = self.capability.read().await;
        let cap = match cap_guard.as_ref() {
            None => return Err(DesktopV1Error::runtime_unavailable()),
            Some(c) => c,
        };
        let full = format!("{NAMESPACE}{method}");
        if !cap.methods.contains(&full) {
            return Err(DesktopV1Error::new("unknown_method", json!({ "method": full })));
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
            json!({ "detail": e }),
        )),
    }
}
```

**All four existing contract tests pass unchanged.** Test #3
(`extension_client_returns_unavailable_for_advertised_method_in_plan_2`) uses
`OmpExtension::new()` (no transport) → now hits guard #3 → still
`runtime_unavailable`. The Plan 2 tests become the "no transport" cases; the new
dispatch behavior activates only when a transport is installed.

## 5. Error handling

`dispatch_v1` returns `Result<Value, String>` (the AcpClient's error string — a
stringified JSON-RPC error or a transport failure). On `Err`, `request()` maps it
to `DesktopV1Error` code `runtime_unavailable` with a `detail` arg carrying the
transport message. This is honest: the Runtime call did not succeed.

**Refinement (out of focused scope):** the Runtime returns structured
`DesktopV1Error` codes (`capability_missing`, `invalid_params`, etc.) as JSON-RPC
errors that `AcpClient` stringifies. Recovering the precise code from the
stringified error is a later enhancement; for the 5 diagnostic command sites in
scope, `runtime_unavailable` + detail is sufficient.

## 6. Testing

A mock `V1Transport` (records the method+params it was called with, returns a
canned `Value`) enables pure unit tests in `omp_desktop_v1/tests/`:

- **Dispatch forwards the full namespaced method:** transport present + advertised
  method `sessions.listAll` → mock receives `_omp/desktop/v1/sessions.listAll` and
  the params; `request()` returns the mock's canned response.
- **Transport error maps to DesktopV1Error:** transport returns `Err("boom")` →
  `request()` yields `runtime_unavailable` with `detail == "boom"`.
- **No transport → runtime_unavailable:** capability present, transport absent →
  guard #3 (this is also covered by existing test #3).
- **Namespace prefixing:** the method passed to the transport always carries the
  `_omp/desktop/v1/` prefix exactly once.

The mock is a test-only struct implementing `V1Transport`; no process spawning, no
`mock_acp` changes (that module is prompt-streaming only).

## 7. Acceptance impact

After this lands, the v1 dispatch path is real and unit-tested. Acceptance items
that were PARTIAL "because `request()` is a fail-closed stub" become verifiable
against a real Runtime (final PASS still needs a configured Runtime + Provider,
which is cross-platform acceptance work):

- AC-1.2 (Queue & Steer transport), AC-1.3 (Provider/Model/Credential transport),
  AC-1.4 (MCP transport), AC-1.8 (Attachment/resolveMedia transport),
  AC-1.9 (diagnostics.selfCheck transport), AC-5.1/5.2 (discovery/config via v1).
- AC-2.7 (protocol contract tests) strengthens: contract tests can now assert live
  dispatch, not just the closed state.

The 5 existing Tauri command sites (`skills.list`, `mcp.list`,
`diagnostics.selfCheck`, `extensions.list` ×2 via `route_through_extension`) work
end-to-end once capability+transport are negotiated — no per-method Desktop code.

## 8. Files touched

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | add `async-trait = "0.1"` |
| `src-tauri/src/omp_desktop_v1/transport.rs` | NEW: `V1Transport` trait |
| `src-tauri/src/omp_desktop_v1/mod.rs` | add `transport` field, `set_transport`, new `request()` dispatch (guard #3 + forward); `pub mod transport;` |
| `src-tauri/src/acp_client.rs` | `impl V1Transport for AcpClient` |
| `src-tauri/src/session_manager.rs` | install/clear transport at capability negotiation |
| `src-tauri/src/omp_desktop_v1/tests/contract.rs` | add mock-transport dispatch tests |

## 9. Risks & mitigations

- **Singleton transport vs. many sessions:** `OmpExtension` is a singleton but
  `AcpClient` is per-session. Mitigation: the transport is the most-recently-
  negotiated session's client; the v1 diagnostic methods are global (about the
  shared Active OMP Agent Directory + profile), so any live client to that
  agent_dir answers correctly. A dead transport fails closed.
- **`async-trait` dependency:** tiny, ubiquitous, well-established. Low risk.
- **Breaking the fail-closed contract:** guarded — `new()` has no transport, all
  existing tests preserved (Section 4).
