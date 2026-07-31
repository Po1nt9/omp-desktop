# Mock/Real-Runtime E2E Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author a scriptable mock ACP Runtime binary and drive real spawned-process E2E tests through it — flipping AC-2.9 (mock E2E happy-path) and AC-7.1 (crash injection, no auto-replay) to PASS, plus env-gated real-Runtime handshake/v1-probe evidence for AC-1.1/1.9/5.1/5.2.

**Architecture:** A ~180-line Rust `[[bin]]` (`mock_acp_runtime`, std + serde_json only) speaks newline-delimited JSON-RPC over stdio, sourcing replies from the existing golden fixtures (`tests/fixtures/acp/`) via `include_str!` — fixtures stay the single source of truth. Scenarios are selected per prompt by a `scenario:<name>` first-line prefix (race-free, unlike env vars). A `#[cfg(test)]` harness module (`src/e2e_runtime.rs`) spawns the mock through the real `AcpClient::spawn_with_options` path and collects `AcpEvent`s for assertions. Crash injection kills the spawned child mid-hang and asserts pending-failure + journal interrupted marking. A real-Runtime tier runs only when `OMP_E2E_REAL=1` and an `omp` binary exists.

**Tech Stack:** Rust, tokio (test side), serde_json, tracing-free test code, existing `AcpClient` / `remote_im::Engine` / `event_journal::recovery` APIs. Zero new dependencies.

**Spec:** `docs/superpowers/specs/2026-07-31-mock-runtime-e2e-design.md` (decisions D1–D6).

## Global Constraints

- **Zero new dependencies** — the mock bin uses only `std` + `serde_json` (already a dependency). No git deps (testy rejected, spec D1).
- **Log metadata only** — mock stderr logs method names only, never prompt content (SA-L.1 / AC-8.8).
- Every async test: `#[tokio::test(flavor = "current_thread")]` and an explicit `tokio::time::timeout` (≤10 s mock tests, ≤30 s real-Runtime tests) — no CI hangs.
- Tests touching the app data root must hold `crate::paths::APP_HOME_ENV_LOCK` and override `OMP_DESKTOP_HOME` for the whole test (portability/recovery flake precedent).
- No trace-capture assertions in this package (shared-callsite race; if ever needed, use `trace.rs test_capture::global_events()`).
- Gates after every task: `cargo test --manifest-path src-tauri/Cargo.toml --lib` green (baseline: 490 pass + 1 ignored).
- Commit per task, English messages: `feat(e2e): … (AC-2.9)` / `(AC-7.1)`.
- The mock bin must NOT enter the Tauri bundle — static-check `src-tauri/tauri.conf.json` in Task 1 (bundle resources/externalBin unchanged).
- cwd resets between shell calls — always `cd /Users/po1nt9/Github/grok-app-main` first.

---

### Task 1: mock_acp_runtime binary (happy scenario) + E2E harness + E1

**Files:**
- Create: `src-tauri/src/bin/mock_acp_runtime.rs`
- Create: `src-tauri/src/e2e_runtime.rs`
- Modify: `src-tauri/src/lib.rs` (insert one line after `mod trace;` at :32)

**Interfaces:**
- Consumes: `AcpClient::spawn_with_options(cli_path: PathBuf, cwd: PathBuf, opts: SpawnOptions) -> Result<(Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>), AgentError>`; `SpawnOptions { binary_path: Option<PathBuf>, ..Default::default() }`; `AcpClient::initialize_and_new_session() -> Result<String, AgentError>`; `AcpClient::prompt(&self, text: &str) -> Result<(), AgentError>`; `AcpClient::kill(&self)`; fixtures `handshake_initialize.json` (`sampleAgentResult`), `stream_chunks.json` (`inbound`, `sessionPromptResult`, `expect.joinedAssistantText = "Hello world."`).
- Produces (all `pub(crate)`, used by Tasks 2–6): `crate::e2e_runtime::mock_runtime_path() -> PathBuf`; `crate::e2e_runtime::spawn_mock() -> (Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>)`; `crate::e2e_runtime::collect_until(rx: &mut mpsc::UnboundedReceiver<AcpEvent>, stop: impl Fn(&AcpEvent) -> bool) -> Vec<AcpEvent>`; `crate::e2e_runtime::joined_assistant(events: &[AcpEvent]) -> String`.

- [ ] **Step 1: Write the failing harness + E1 test**

Create `src-tauri/src/e2e_runtime.rs`:

```rust
//! E2E harness + tests driving a real spawned mock ACP Runtime process
//! (AC-2.9 happy-path, AC-7.1 crash injection). The mock binary
//! (`src/bin/mock_acp_runtime.rs`) speaks newline-delimited JSON-RPC over
//! stdio and sources replies from the golden fixtures.

use crate::acp_client::{AcpClient, AcpEvent, SpawnOptions, StreamKind};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Absolute path to the built mock binary. `cargo test` builds all bin
/// targets of the package; `CARGO_BIN_EXE_*` covers the common case and the
/// target-dir fallback keeps `cargo test --lib` usable everywhere.
pub(crate) fn mock_runtime_path() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_mock_acp_runtime") {
        return PathBuf::from(p);
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/mock_acp_runtime");
    assert!(
        p.exists(),
        "mock_acp_runtime not built; run `cargo build --bin mock_acp_runtime` first"
    );
    p
}

/// Spawn the mock Runtime through the real `AcpClient` spawn path.
pub(crate) fn spawn_mock() -> (Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>) {
    let opts = SpawnOptions {
        binary_path: Some(mock_runtime_path()),
        ..Default::default()
    };
    AcpClient::spawn_with_options(PathBuf::new(), std::env::temp_dir(), opts)
        .expect("spawn mock_acp_runtime")
}

/// Drain events until `stop` matches (inclusive) or the 10s timeout hits.
pub(crate) async fn collect_until(
    rx: &mut mpsc::UnboundedReceiver<AcpEvent>,
    stop: impl Fn(&AcpEvent) -> bool,
) -> Vec<AcpEvent> {
    let mut out = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(ev) = rx.recv().await {
            let done = stop(&ev);
            out.push(ev);
            if done {
                break;
            }
        }
    })
    .await
    .expect("event collection timed out after 10s");
    out
}

/// Concatenated non-terminal assistant stream text.
pub(crate) fn joined_assistant(events: &[AcpEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            AcpEvent::Stream {
                kind: StreamKind::Assistant,
                text,
                done: false,
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn e2e_mock_happy_path_turn() {
    let (client, mut rx) = spawn_mock();
    let sid = client
        .initialize_and_new_session()
        .await
        .expect("initialize + session/new against mock");
    assert_eq!(sid, "mock-sess-1");

    client.prompt("say hi").await.expect("prompt rpc");
    let events = collect_until(&mut rx, |e| matches!(e, AcpEvent::PromptComplete { .. })).await;

    assert_eq!(joined_assistant(&events), "Hello world.");
    assert!(
        events.iter().any(|e| matches!(
            e,
            AcpEvent::Stream { kind: StreamKind::Thought, text, .. }
                if text.contains("Thinking about the answer")
        )),
        "thought chunk missing from {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AcpEvent::PromptComplete { stop_reason, authoritative: true } if stop_reason == "end_turn"
        )),
        "authoritative PromptComplete(end_turn) missing from {events:?}"
    );
    client.kill().await;
}
```

Register the module in `src-tauri/src/lib.rs` — insert between `mod trace;` (:32) and `mod turn_complete;`:

```rust
#[cfg(test)]
mod e2e_runtime;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock_happy_path_turn 2>&1 | tail -5`
Expected: FAIL — `mock_acp_runtime not built` (bin does not exist yet).

- [ ] **Step 3: Implement the mock binary (happy scenario + protocol skeleton)**

Create `src-tauri/src/bin/mock_acp_runtime.rs`:

```rust
//! mock_acp_runtime — scriptable ACP test double for E2E tests (AC-2.9/AC-7.1).
//!
//! Speaks newline-delimited JSON-RPC over stdio; replies are sourced from the
//! golden fixtures in `tests/fixtures/acp/` (single source of truth, locked
//! by `acp_golden_test.rs`). Ignores all argv — the Host spawns
//! `<binary> acp --stdio`.
//!
//! Scenarios are selected per prompt via a `scenario:<name>` first-line
//! prefix in the prompt text (race-free across parallel tests, unlike env
//! vars). Default scenario: `happy`.
//!
//! stderr carries method-name diagnostics only — never prompt content
//! (SA-L.1 / AC-8.8).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};

const HANDSHAKE: &str = include_str!("../../tests/fixtures/acp/handshake_initialize.json");
const STREAM: &str = include_str!("../../tests/fixtures/acp/stream_chunks.json");

fn fixture(raw: &str) -> Value {
    serde_json::from_str(raw).expect("fixture parses")
}

fn result_frame(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_frame(id: &Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn write_frame(out: &mut impl Write, frame: &Value) {
    let line = serde_json::to_string(frame).expect("serialize frame");
    writeln!(out, "{line}").expect("stdout write");
    out.flush().expect("stdout flush");
}

/// Read one JSON-RPC frame; `None` on EOF (Host closed the pipe).
fn read_frame(reader: &mut impl BufRead) -> Option<Value> {
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(frame) = serde_json::from_str::<Value>(trimmed) {
                    let method = frame
                        .get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("<result>");
                    eprintln!("mock-acp-runtime method={method}");
                    return Some(frame);
                }
            }
            Err(_) => return None,
        }
    }
}

fn prompt_text(params: &Value) -> String {
    params
        .get("prompt")
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .or_else(|| params.get("text").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string()
}

fn scenario_of(text: &str) -> &str {
    text.lines()
        .next()
        .unwrap_or("")
        .strip_prefix("scenario:")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("happy")
}

/// Send `session/update` notification frames, stamped with the live session id.
fn send_updates(out: &mut impl Write, session_id: &str, updates: &[Value]) {
    for u in updates {
        let mut frame = u.clone();
        frame["params"]["sessionId"] = json!(session_id);
        write_frame(out, &frame);
    }
}

fn main() {
    let mut reader = BufReader::new(std::io::stdin());
    let mut out = std::io::stdout();
    let handshake = fixture(HANDSHAKE);
    let stream = fixture(STREAM);
    let session_id = String::from("mock-sess-1");

    while let Some(frame) = read_frame(&mut reader) {
        let method = frame
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let id = frame.get("id").cloned();
        match method.as_str() {
            "initialize" => {
                if let Some(id) = id {
                    write_frame(&mut out, &result_frame(&id, handshake["sampleAgentResult"].clone()));
                }
            }
            // Host treats authenticate as best-effort; succeed trivially.
            "authenticate" => {
                if let Some(id) = id {
                    write_frame(&mut out, &result_frame(&id, json!({})));
                }
            }
            "session/new" | "session/load" => {
                if let Some(id) = id {
                    write_frame(&mut out, &result_frame(&id, json!({ "sessionId": session_id })));
                }
            }
            "session/prompt" => {
                let Some(id) = id else { continue };
                let scenario = scenario_of(&prompt_text(
                    frame.get("params").unwrap_or(&Value::Null),
                ));
                match scenario {
                    // Unknown/hang scenarios added by later tasks; default = happy.
                    _ => {
                        send_updates(
                            &mut out,
                            &session_id,
                            stream["inbound"].as_array().expect("stream inbound array"),
                        );
                        write_frame(
                            &mut out,
                            &result_frame(&id, stream["sessionPromptResult"].clone()),
                        );
                    }
                }
            }
            // Host stop notification: nothing to ack.
            "session/cancel" => {}
            _ => {
                if let Some(id) = id {
                    write_frame(&mut out, &error_frame(&id, -32601, "method not found"));
                }
            }
        }
    }
}
```

- [ ] **Step 4: Build the bin and run E1 to green**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --bin mock_acp_runtime 2>&1 | tail -3`
Expected: `Compiling` + `Finished` — zero errors. (Cargo auto-discovers `src/bin/*.rs` as `[[bin]] mock_acp_runtime`.)

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock_happy_path_turn 2>&1 | tail -5`
Expected: PASS (1 passed). If events hang: check mock stdout framing with `MOCK` manual run — `printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n' | ./src-tauri/target/debug/mock_acp_runtime` should print the `sampleAgentResult` frame.

- [ ] **Step 5: Verify the mock is not bundled by Tauri (static check)**

Run: `grep -n "externalBin\|resources" src-tauri/tauri.conf.json`
Expected: no `mock_acp_runtime` reference anywhere. Tauri bundles only the main binary (`productName` target); extra `[[bin]]` targets are compiled but never packaged. No conf change allowed in this task.

- [ ] **Step 6: Full lib suite, then commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3`
Expected: `491 passed; 0 failed; 1 ignored`.

```bash
git add src-tauri/src/bin/mock_acp_runtime.rs src-tauri/src/e2e_runtime.rs src-tauri/src/lib.rs
git commit -m "feat(e2e): scriptable mock ACP runtime bin + harness + happy-path E2E (AC-2.9)"
```

---

### Task 2: permission scenario + E2 (approval leg)

**Files:**
- Modify: `src-tauri/src/bin/mock_acp_runtime.rs` (add PERMISSION include, `permission` branch, `wait_for_reply`)
- Modify: `src-tauri/src/e2e_runtime.rs` (append E2 test)

**Interfaces:**
- Consumes: Task 1 harness; `AcpClient::respond_permission(&self, rpc_id: u64, outcome: PermissionOutcome) -> Result<(), String>`; `PermissionOutcome::Selected { option_id: String }`; `crate::permission::pick_option_id(options: &Value, prefer: &str) -> Option<String>`; `AcpEvent::PermissionRequest { rpc_id, tool_call_id, tool_name, title, options, raw }`; fixture `permission_request.json` (`inbound` request shape, options with kinds `allow_once`/`allow_always`/`reject_once`).
- Produces: nothing new for other tasks.

- [ ] **Step 1: Write the failing E2 test**

Append to `src-tauri/src/e2e_runtime.rs`:

```rust
#[tokio::test(flavor = "current_thread")]
async fn e2e_mock_permission_gate_turn() {
    let (client, mut rx) = spawn_mock();
    client
        .initialize_and_new_session()
        .await
        .expect("initialize + session/new against mock");

    let prompt = tokio::spawn({
        let c = Arc::clone(&client);
        async move { c.prompt("scenario:permission\nwrite the file").await }
    });

    // The mock must request permission before streaming any reply text.
    let events = collect_until(&mut rx, |e| matches!(e, AcpEvent::PermissionRequest { .. })).await;
    let (rpc_id, options) = events
        .iter()
        .find_map(|e| match e {
            AcpEvent::PermissionRequest { rpc_id, options, .. } => Some((*rpc_id, options.clone())),
            _ => None,
        })
        .expect("PermissionRequest event");

    let option_id = crate::permission::pick_option_id(&options, "allow_once")
        .expect("allow_once option present");
    client
        .respond_permission(rpc_id, crate::acp_client::PermissionOutcome::Selected { option_id })
        .await
        .expect("respond_permission");

    prompt.await.expect("prompt task join").expect("prompt rpc after approval");
    let rest = collect_until(&mut rx, |e| matches!(e, AcpEvent::PromptComplete { .. })).await;
    assert_eq!(joined_assistant(&rest), "Hello world.");
    client.kill().await;
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock_permission_gate_turn 2>&1 | tail -5`
Expected: FAIL — `event collection timed out after 10s` (mock ignores `scenario:permission`, streams happy flow instead; no PermissionRequest ever arrives).

- [ ] **Step 3: Implement the permission branch in the mock**

In `src-tauri/src/bin/mock_acp_runtime.rs`:

Add after the `STREAM` const:

```rust
const PERMISSION: &str = include_str!("../../tests/fixtures/acp/permission_request.json");
```

Add a helper after `send_updates`:

```rust
/// Block until the Host answers our reverse-request `rpc` (permission leg).
fn wait_for_reply(reader: &mut impl BufRead, rpc: u64) {
    while let Some(frame) = read_frame(reader) {
        if frame.get("id").and_then(|v| v.as_u64()) == Some(rpc) && frame.get("result").is_some() {
            return;
        }
    }
}
```

In `main`, add after the `stream` fixture binding:

```rust
    let permission = fixture(PERMISSION);
    let mut next_agent_rpc_id: u64 = 9000;
```

Replace the `match scenario { _ => { … } }` block inside `"session/prompt"` with:

```rust
                match scenario {
                    "permission" => {
                        let rpc = next_agent_rpc_id;
                        next_agent_rpc_id += 1;
                        let mut req = permission["inbound"].clone();
                        req["id"] = json!(rpc);
                        req["params"]["sessionId"] = json!(session_id);
                        write_frame(&mut out, &req);
                        wait_for_reply(&mut reader, rpc);
                        send_updates(
                            &mut out,
                            &session_id,
                            stream["inbound"].as_array().expect("stream inbound array"),
                        );
                        write_frame(
                            &mut out,
                            &result_frame(&id, stream["sessionPromptResult"].clone()),
                        );
                    }
                    _ => {
                        send_updates(
                            &mut out,
                            &session_id,
                            stream["inbound"].as_array().expect("stream inbound array"),
                        );
                        write_frame(
                            &mut out,
                            &result_frame(&id, stream["sessionPromptResult"].clone()),
                        );
                    }
                }
```

- [ ] **Step 4: Run E2 to green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock_permission 2>&1 | tail -5`
Expected: PASS (1 passed). E1 must still pass — run both: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock 2>&1 | tail -3` → 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/mock_acp_runtime.rs src-tauri/src/e2e_runtime.rs
git commit -m "feat(e2e): permission-gate scenario + approval leg E2E (AC-2.9)"
```

---

### Task 3: tool_call lifecycle scenario + fixture + E3 (tool exec leg)

**Files:**
- Create: `src-tauri/tests/fixtures/acp/tool_call_updates.json`
- Modify: `src-tauri/src/bin/mock_acp_runtime.rs` (TOOLCALL include + `toolcall` branch)
- Modify: `src-tauri/src/e2e_runtime.rs` (append E3 test)

**Interfaces:**
- Consumes: Task 1 harness; `AcpEvent::ToolCall { tool_call_id, title, kind, status, raw }`; decoder expects **flat** update fields (`sessionUpdate`, `toolCallId`, `title`, `kind`, `status` — `acp_client.rs:1609-1640`).
- Produces: fixture `tool_call_updates.json` (may be reused by future golden decode tests).

- [ ] **Step 1: Write the failing E3 test**

Append to `src-tauri/src/e2e_runtime.rs`:

```rust
#[tokio::test(flavor = "current_thread")]
async fn e2e_mock_tool_call_lifecycle() {
    let (client, mut rx) = spawn_mock();
    client
        .initialize_and_new_session()
        .await
        .expect("initialize + session/new against mock");

    client
        .prompt("scenario:toolcall\nwrite hello.txt")
        .await
        .expect("prompt rpc");
    let events = collect_until(&mut rx, |e| matches!(e, AcpEvent::PromptComplete { .. })).await;

    let statuses: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AcpEvent::ToolCall { tool_call_id, status, .. } if tool_call_id == "call-mock-1" => {
                Some(status.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        vec!["pending", "running", "completed"],
        "tool_call lifecycle mismatch in {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AcpEvent::ToolCall { kind, title, .. } if kind == "write" && title == "Write hello.txt"
        )),
        "tool_call kind/title missing in {events:?}"
    );
    assert_eq!(joined_assistant(&events), "Hello world.");
    client.kill().await;
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock_tool_call_lifecycle 2>&1 | tail -5`
Expected: FAIL — `tool_call lifecycle mismatch` with empty `statuses` (mock has no `toolcall` branch yet; happy flow carries no ToolCall events).

- [ ] **Step 3: Add the fixture and the mock branch**

Create `src-tauri/tests/fixtures/acp/tool_call_updates.json`:

```json
{
  "_comment": "tool_call lifecycle session/update samples for the mock Runtime E2E (AC-2.9 tool leg). Flat update fields locked by decode_session_update (acp_client.rs).",
  "inbound": [
    {
      "jsonrpc": "2.0",
      "method": "session/update",
      "params": {
        "sessionId": "agent-sess-1",
        "update": {
          "sessionUpdate": "tool_call",
          "toolCallId": "call-mock-1",
          "title": "Write hello.txt",
          "kind": "write",
          "status": "pending"
        }
      }
    },
    {
      "jsonrpc": "2.0",
      "method": "session/update",
      "params": {
        "sessionId": "agent-sess-1",
        "update": {
          "sessionUpdate": "tool_call_update",
          "toolCallId": "call-mock-1",
          "title": "Write hello.txt",
          "kind": "write",
          "status": "running"
        }
      }
    },
    {
      "jsonrpc": "2.0",
      "method": "session/update",
      "params": {
        "sessionId": "agent-sess-1",
        "update": {
          "sessionUpdate": "tool_call_update",
          "toolCallId": "call-mock-1",
          "title": "Write hello.txt",
          "kind": "write",
          "status": "completed"
        }
      }
    }
  ],
  "expect": {
    "toolCallId": "call-mock-1",
    "statuses": ["pending", "running", "completed"]
  }
}
```

In `src-tauri/src/bin/mock_acp_runtime.rs`, add after the `PERMISSION` const:

```rust
const TOOLCALL: &str = include_str!("../../tests/fixtures/acp/tool_call_updates.json");
```

Add in `main` after the `permission` binding:

```rust
    let toolcall = fixture(TOOLCALL);
```

Add a `"toolcall"` arm inside the prompt `match scenario`, before the `_` arm:

```rust
                    "toolcall" => {
                        send_updates(
                            &mut out,
                            &session_id,
                            stream["inbound"].as_array().expect("stream inbound array"),
                        );
                        send_updates(
                            &mut out,
                            &session_id,
                            toolcall["inbound"].as_array().expect("toolcall inbound array"),
                        );
                        write_frame(
                            &mut out,
                            &result_frame(&id, stream["sessionPromptResult"].clone()),
                        );
                    }
```

- [ ] **Step 4: Run E3 to green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_mock 2>&1 | tail -3`
Expected: 3 passed (E1, E2, E3).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/fixtures/acp/tool_call_updates.json src-tauri/src/bin/mock_acp_runtime.rs src-tauri/src/e2e_runtime.rs
git commit -m "feat(e2e): tool_call lifecycle scenario + fixture + tool leg E2E (AC-2.9)"
```

---

### Task 4: engine full turn against the spawned mock (E4, Hub leg)

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs` (append one test in the existing `#[cfg(test)]` module)

**Interfaces:**
- Consumes: `crate::e2e_runtime::mock_runtime_path()` (Task 1); `Engine::new_ephemeral_with_binary(outbound: OutboundRouter, binary_path: PathBuf) -> Self` (`engine.rs:223`, `pub(crate)`); `Engine::run_agent_turn(&self, msg: &IncomingMessage, scope: &str, default_wd: &str, prompt: &str) -> AgentTurnResult` (`engine.rs:1068`, `pub(crate)`); `AgentTurnResult { text: String, session_id: Option<String>, error: Option<String> }` (`engine.rs:42`); `IncomingMessage` field shape (see test at `engine.rs:1616`).
- Produces: nothing new.

- [ ] **Step 1: Write the failing E4 test**

Append inside the `#[cfg(test)]` tests module of `src-tauri/src/remote_im/engine.rs`:

```rust
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
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            engine.run_agent_turn(&msg, "e2e-scope", "/tmp/omp-e2e-engine", "hi"),
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine_full_turn_against_mock_runtime 2>&1 | tail -5`
Expected: FAIL — `turn error: Some(...)` with the fail-closed runtime-unavailable text… **unless** the mock bin is already built (Tasks 1–3), in which case this test passes immediately — that is the intended outcome (the engine path was already complete; the missing piece was a spawnable Runtime). Either way proceed to Step 3 after observing green.

- [ ] **Step 3: Run to green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine_full_turn 2>&1 | tail -3`
Expected: 1 passed. Then `cargo test --manifest-path src-tauri/Cargo.toml --lib remote_im 2>&1 | tail -3` → all remote_im tests green (no regression in the pooled-runtime tests).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(e2e): engine full-turn against spawned mock runtime (AC-2.9)"
```

---

### Task 5: mid-turn crash injection (E5) + journal interrupted marking (E6) — AC-7.1

**Files:**
- Modify: `src-tauri/src/bin/mock_acp_runtime.rs` (add `hang` branch)
- Modify: `src-tauri/src/e2e_runtime.rs` (append E5 + E6 tests + `test_home` helper)

**Interfaces:**
- Consumes: Task 1 harness; `AcpClient::kill(&self)`; `AcpClient::is_alive(&self) -> bool`; `AcpEvent::ProcessExited { code }`; `AcpEvent::Stderr { line }`; `crate::event_journal::EventJournal::new(session_id: String)`; `crate::event_journal::recovery::{append_turn_start_durable(&mut EventJournal) -> String, recover_session_journal(&str) -> Option<RecoveryReport>}`; `RecoveryReport::{Clean, Interrupted { turn_start_event_id, marker_message_id, content }}`; env pattern from `event_journal/recovery.rs:236-258` (module GUARD + `crate::paths::APP_HOME_ENV_LOCK` + `OMP_DESKTOP_HOME` temp dir).
- Produces: `hang` scenario (crash-injection target, spec D3/D6).

- [ ] **Step 1: Write the failing E5 test**

Append to `src-tauri/src/e2e_runtime.rs`:

```rust
#[tokio::test(flavor = "current_thread")]
async fn e2e_crash_mid_turn_fails_pending_without_replay() {
    let (client, mut rx) = spawn_mock();
    client
        .initialize_and_new_session()
        .await
        .expect("initialize + session/new against mock");

    let prompt = tokio::spawn({
        let c = Arc::clone(&client);
        async move { c.prompt("scenario:hang").await }
    });
    // Let the prompt reach the mock before the "crash".
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    client.kill().await;

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), prompt)
        .await
        .expect("prompt task hung after kill")
        .expect("prompt task join");
    assert!(outcome.is_err(), "killed mid-turn prompt must fail, got Ok");

    let events = collect_until(&mut rx, |e| matches!(e, AcpEvent::ProcessExited { .. })).await;
    let prompt_requests = events
        .iter()
        .filter(|e| matches!(e, AcpEvent::Stderr { line } if line.contains("method=session/prompt")))
        .count();
    assert_eq!(
        prompt_requests, 1,
        "host must never re-issue session/prompt after a crash: {events:?}"
    );
    assert!(!client.is_alive());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_crash_mid_turn 2>&1 | tail -5`
Expected: FAIL — `killed mid-turn prompt must fail, got Ok` (mock treats `scenario:hang` as happy and completes the turn before the kill lands).

- [ ] **Step 3: Implement the `hang` branch**

In `src-tauri/src/bin/mock_acp_runtime.rs`, add a `"hang"` arm inside the prompt `match scenario`, before the `_` arm:

```rust
                    // Crash-injection target (AC-7.1): never reply, never
                    // notify — the Host only unblocks when the process dies.
                    "hang" => {}
```

- [ ] **Step 4: Run E5 to green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_crash_mid_turn 2>&1 | tail -5`
Expected: PASS (1 passed).

- [ ] **Step 5: Write the failing E6 test (journal recovery composition)**

Append to `src-tauri/src/e2e_runtime.rs`:

```rust
/// Hold the module guard AND the shared app-home env lock for the whole
/// test, and point OMP_DESKTOP_HOME at a fresh temp dir (pattern copied from
/// `event_journal/recovery.rs` tests — env overrides race without it).
static HOME_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn test_home(tag: &str) -> (
    std::sync::MutexGuard<'static, ()>,
    std::sync::MutexGuard<'static, ()>,
    PathBuf,
) {
    let module = HOME_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let env = crate::paths::APP_HOME_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("omp-e2e-home-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("OMP_DESKTOP_HOME", &dir);
    (module, env, dir)
}

#[tokio::test(flavor = "current_thread")]
async fn e2e_crash_journal_marks_interrupted_no_replay() {
    let (_module, _env, _dir) = test_home("crash-journal");
    let sid = "e2e-crash-sess";

    // TurnStart write-ahead: the dangling boundary a crash leaves behind.
    {
        let mut journal = crate::event_journal::EventJournal::new(sid.to_string());
        crate::event_journal::recovery::append_turn_start_durable(&mut journal);
    } // journal dropped; write-ahead save already hit disk

    // Live sidecar crash mid-turn.
    let (client, mut rx) = spawn_mock();
    client
        .initialize_and_new_session()
        .await
        .expect("initialize + session/new against mock");
    let prompt = tokio::spawn({
        let c = Arc::clone(&client);
        async move { c.prompt("scenario:hang").await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    client.kill().await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), prompt).await;
    let _ = collect_until(&mut rx, |e| matches!(e, AcpEvent::ProcessExited { .. })).await;

    // Recovery closes the dangling turn honestly — exactly once.
    let report = crate::event_journal::recovery::recover_session_journal(sid);
    match report {
        Some(crate::event_journal::recovery::RecoveryReport::Interrupted { content, .. }) => {
            assert!(
                content.starts_with("turn_interrupted"),
                "marker content must be the turn_interrupted pipe, got: {content}"
            );
        }
        other => panic!("expected Interrupted recovery report, got {other:?}"),
    }
    // Idempotent: a second recovery finds a clean journal, no duplicate marker.
    assert_eq!(
        crate::event_journal::recovery::recover_session_journal(sid),
        Some(crate::event_journal::recovery::RecoveryReport::Clean)
    );
}
```

- [ ] **Step 6: Run E6**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_crash_journal 2>&1 | tail -5`
Expected: PASS (this composes already-tested pieces — AC-1.10 recovery + the Task 5 kill path — so it may pass on first run; if `RecoveryReport` has no `PartialEq`, replace the final `assert_eq!` with `assert!(matches!(…, Some(RecoveryReport::Clean)))`).

- [ ] **Step 7: Full suite + commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3`
Expected: `496 passed; 0 failed; 1 ignored` (490 baseline + E1, E2, E3, E4, E5, E6).

```bash
git add src-tauri/src/bin/mock_acp_runtime.rs src-tauri/src/e2e_runtime.rs
git commit -m "feat(e2e): mid-turn crash injection + journal interrupted marking (AC-7.1)"
```

---

### Task 6: real-Runtime gated tier (E7 handshake, E8 v1 probe)

**Files:**
- Modify: `src-tauri/src/e2e_runtime.rs` (append `real_omp_binary` gate + E7 + E8)

**Interfaces:**
- Consumes: `AcpClient::initialize_and_open_session(resume: Option<&str>) -> Result<(String, bool), AgentError>`; `AcpClient::initialize_result() -> Option<Value>`; `crate::omp_desktop_v1::transport::V1Transport::dispatch_v1(&self, full_method: &str, params: Value) -> Result<Value, String>` (implemented by `AcpClient`); `SpawnOptions { binary_path, agent_dir, .. }`.
- Produces: probe evidence consumed by Task 7 (matrix updates). Tests are **env-gated**: default run = skip-print + pass; `OMP_E2E_REAL=1` + an `omp` binary = real execution.

- [ ] **Step 1: Write the gated tests**

Append to `src-tauri/src/e2e_runtime.rs`:

```rust
/// Real-Runtime gate: only Some when OMP_E2E_REAL=1 and an omp binary exists.
/// Never a CI gate — the default path skips with a log line (spec D4).
fn real_omp_binary() -> Option<PathBuf> {
    if std::env::var("OMP_E2E_REAL").ok().as_deref() != Some("1") {
        return None;
    }
    if let Ok(p) = std::env::var("OMP_E2E_REAL_BINARY") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    for cand in ["/opt/homebrew/bin/omp", "/usr/local/bin/omp"] {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn spawn_real() -> Option<(Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>)> {
    let bin = real_omp_binary()?;
    let agent_dir = std::env::temp_dir().join(format!("omp-e2e-agent-{}", std::process::id()));
    std::fs::create_dir_all(&agent_dir).ok()?;
    let opts = SpawnOptions {
        binary_path: Some(bin),
        agent_dir: Some(agent_dir),
        ..Default::default()
    };
    AcpClient::spawn_with_options(PathBuf::new(), std::env::temp_dir(), opts).ok()
}

/// AC-1.1 end-to-end leg: real capability negotiation against a live
/// `omp acp --stdio` (no LLM — handshake + local session only).
#[tokio::test(flavor = "current_thread")]
async fn e2e_real_handshake_capabilities() {
    let Some((client, mut rx)) = spawn_real() else {
        eprintln!("SKIP e2e_real_handshake_capabilities: OMP_E2E_REAL!=1 or no omp binary");
        return;
    };
    // initialize runs first inside initialize_and_open_session; session/new
    // may legitimately fail on auth-less machines, so we assert on the
    // cached initialize result regardless of the session outcome.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.initialize_and_open_session(None),
    )
    .await;
    let init = client
        .initialize_result()
        .expect("initialize result cached even if session/new failed");
    assert_eq!(
        init.get("protocolVersion").and_then(|v| v.as_u64()),
        Some(1),
        "real Runtime protocolVersion mismatch: {init}"
    );
    assert!(
        init.get("agentCapabilities").is_some() || init.get("capabilities").is_some(),
        "real Runtime advertised no capabilities: {init}"
    );
    drop(collect_until(&mut rx, |_| false).await); // drain anything buffered
    client.kill().await;
}

/// AC-1.9 / AC-5.1 / AC-5.2 evidence probe: which `_omp/desktop/v1/*` methods
/// does the installed Runtime actually answer? Results are recorded into the
/// acceptance matrix by the docs task — the test itself only asserts the
/// probes terminate with a structured outcome (never hang/panic).
#[tokio::test(flavor = "current_thread")]
async fn e2e_real_v1_method_probe() {
    use crate::omp_desktop_v1::transport::V1Transport;
    let Some((client, _rx)) = spawn_real() else {
        eprintln!("SKIP e2e_real_v1_method_probe: OMP_E2E_REAL!=1 or no omp binary");
        return;
    };
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.initialize_and_open_session(None),
    )
    .await;
    for m in ["diagnostics.selfCheck", "providers.list", "sessionConfig.get"] {
        let full = format!("_omp/desktop/v1/{m}");
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client.dispatch_v1(&full, serde_json::json!({})),
        )
        .await;
        match &outcome {
            Ok(Ok(v)) => eprintln!("v1 probe {full}: OK {}", &v.to_string()[..v.to_string().len().min(200)]),
            Ok(Err(e)) => eprintln!("v1 probe {full}: ERR {e}"),
            Err(_) => panic!("v1 probe {full} timed out after 15s"),
        }
    }
    client.kill().await;
}
```

- [ ] **Step 2: Verify default-skip behavior (CI path unchanged)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_real 2>&1 | tail -6`
Expected: 2 passed (both skip-print and return — `OMP_E2E_REAL` unset).

- [ ] **Step 3: Run the real tier locally and capture evidence**

Run: `OMP_E2E_REAL=1 cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_real -- --nocapture 2>&1 | tail -20`
Expected (this machine has `/opt/homebrew/bin/omp` 17.1.3): E7 PASS with protocolVersion=1. E8 PASS; **record the three `v1 probe …` lines verbatim** — Task 7 needs them as matrix evidence. If E7 fails because `initialize` itself errors (not just session/new), STOP and investigate — do not weaken the assertion to force green.

- [ ] **Step 4: Full suite + commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3`
Expected: `498 passed; 0 failed; 1 ignored`.

```bash
git add src-tauri/src/e2e_runtime.rs
git commit -m "test(e2e): real-Runtime handshake + v1 probe, env-gated (AC-1.1)"
```

---

### Task 7: matrix flips + coverage-audit sync + memory + final gates

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (rows :31 AC-1.1, :61 AC-2.9, :129 AC-7.1, counts table, FAIL/BLOCKED narrative if present)
- Modify: `docs/release/test-coverage-audit.md` (counts line 15, gap table row 76)
- Modify: agent memory `omp-desktop-roadmap-status.md` + `MEMORY.md` (outside repo)

**Interfaces:**
- Consumes: E1–E6 green (Tasks 1–5), E7/E8 probe output captured in Task 6 Step 3.
- Produces: AC-2.9 → PASS, AC-7.1 → PASS; counts 39 PASS / 100 BLOCKED (grep -o convention incl. counts row).

- [ ] **Step 1: Flip AC-2.9 to PASS**

Replace the AC-2.9 row (:61) verdict `BLOCKED` → `PASS` and its evidence with:

`Mock-Runtime E2E green: e2e_mock_happy_path_turn (session create → prompt → streamed response → turn end), e2e_mock_permission_gate_turn (permission approval leg), e2e_mock_tool_call_lifecycle (tool exec leg), engine_full_turn_against_mock_runtime (Hub leg via remote_im Engine). Scriptable mock_acp_runtime bin spawned through the real AcpClient path; replies sourced from golden fixtures.`

- [ ] **Step 2: Flip AC-7.1 to PASS**

Replace the AC-7.1 row (:129) verdict `BLOCKED` → `PASS` and its evidence with:

`Automated crash injection: e2e_crash_mid_turn_fails_pending_without_replay (kill spawned sidecar mid-hang → pending prompt fails, ProcessExited, exactly one session/prompt on the wire — no auto-replay) + e2e_crash_journal_marks_interrupted_no_replay (write-ahead dangling TurnStart → recover_session_journal closes it as turn_interrupted, idempotent). Conservative-recovery invariant (journal never a replay source) asserted by the single-prompt wire count.`

- [ ] **Step 3: Append AC-1.1 end-to-end evidence (status stays PARTIAL)**

Append to the AC-1.1 row (:31) evidence cell:

` End-to-end leg added 2026-07-31: e2e_real_handshake_capabilities (env-gated OMP_E2E_REAL=1) negotiated protocolVersion=1 + agentCapabilities against live omp 17.1.3. Elicitation gap unchanged — PARTIAL stands.`

- [ ] **Step 4: Record v1 probe outcomes honestly (AC-1.9/5.1/5.2)**

Using the Task 6 Step 3 probe lines, append one sentence to each of AC-1.9 (:39), AC-5.1 (:100), AC-5.2 (:101) evidence cells, e.g. `v1 probe vs omp 17.1.3 (2026-07-31): diagnostics.selfCheck → <OK/ERR verbatim>`. **Flip a row to PASS only if its probe returned OK and the row's remaining gap is solely this verification; otherwise keep the verdict and record the probe result.** If a method returned method-not-found, that is honest evidence the Runtime does not implement it — record, do not flip.

- [ ] **Step 5: Update counts + coverage audit**

In `1.0-acceptance-matrix.md` counts table: `| PASS | 37 |` → `| PASS | 39 |`, `| BLOCKED | 102 |` → `| BLOCKED | 100 |`. Verify with `grep -o '| PASS |' … | wc -l` = 39 and `grep -o '| BLOCKED |' … | wc -l` = 100 (occurrence convention includes the counts row itself; if mismatch, investigate — do NOT hand-force).

In `docs/release/test-coverage-audit.md` line 15: `490 tests (490 pass + 1 ignored)` → `498 tests (498 pass + 1 ignored)`; append before the final ` |`: `; +8 AC-2.9/AC-7.1 E2E tests (5 AcpClient-level incl. crash injection, 1 engine full-turn, 2 env-gated real-Runtime) (2026-07-31 evening)`.

Update the gap-table row 76 (End-to-end capability negotiation): strike it as partially resolved — the handshake leg (AC-1.1) ran against omp 17.1.3 via `e2e_real_handshake_capabilities`; remaining gap = v1 method semantics (AC-1.2/1.3/1.8) per probe outcomes recorded in the matrix.

- [ ] **Step 6: Update project memory**

In `omp-desktop-roadmap-status.md`: add an AC-2.9/AC-7.1 entry (mock_acp_runtime bin, scenario prefix protocol, E1–E8, verdict counts 39 PASS / 16 PARTIAL / 100 BLOCKED / 3 FAIL grep 口径）; update the priority list: ① AC-10.9 更新渠道 ② AC-12.3 凭据管理文档 ③ 真机验收 + 外部安全审计. Update `MEMORY.md` hook line to match.

- [ ] **Step 7: Final gates**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3` → 498 passed.
Run: `pnpm test 2>&1 | tail -4` → 840 passed (no frontend change — if this drifts, investigate, don't assume).
Run: `pnpm typecheck && pnpm check:i18n && pnpm check:brand && pnpm check:provenance && pnpm check:legal` → all green.

- [ ] **Step 8: Commit**

```bash
git add docs/release/1.0-acceptance-matrix.md docs/release/test-coverage-audit.md
git commit -m "docs(release): AC-2.9 + AC-7.1 PASS — mock-Runtime E2E + crash injection (AC-2.9, AC-7.1)"
```

---

## Self-Review

1. **Spec coverage:** D1 (Task 1 bin, zero deps) ✓; D2 two levels (E1–E3 AcpClient, E4 engine; SessionManager excluded) ✓; D3 crash injection (E5/E6) ✓; D4 env-gated real tier, never a gate (E7/E8 skip path verified in Task 6 Step 2; LLM tier unimplemented flag-only per spec §6) ✓; D5 flip discipline (Task 7 Steps 1–4, probe-honesty rule) ✓; D6 three scenarios happy/permission/hang + toolcall as happy-extension (spec §4.3 lists tool_call under E3 — consistent) ✓.
2. **Placeholder scan:** none — every code step is complete; the only conditional (Task 7 Step 4 flip rule) is a decision procedure with verbatim evidence requirements, not a TODO.
3. **Type consistency:** `mock_runtime_path`/`spawn_mock`/`collect_until`/`joined_assistant` names identical across Tasks 1–6; `PermissionOutcome::Selected { option_id }` matches `acp_client.rs:1405`; `RecoveryReport::{Clean, Interrupted{…}}` matches `recovery.rs:68-78`; `AgentTurnResult.text/error` matches `engine.rs:42-46`; `new_ephemeral_with_binary` is `pub(crate)` and reachable from engine's own test module; `e2e_runtime` is `#[cfg(test)]` at crate root so `crate::e2e_runtime::…` resolves from engine tests; `pick_option_id(&Value, &str)` matches `permission.rs:576`.
4. **Known deviation notes:** Task 4 Step 2 documents that E4 may pass immediately (engine path was complete; the blocker was a spawnable Runtime — exactly what `spawn_lock_reclaimed_after_failed_spawn`'s comment predicted). Task 5 Step 6 documents the `PartialEq` fallback for `RecoveryReport`.
