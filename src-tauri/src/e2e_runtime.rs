//! E2E harness + tests driving a real spawned mock ACP Runtime process
//! (AC-2.9 happy-path, AC-7.1 crash injection). The mock binary
//! (`src/bin/mock_acp_runtime.rs`) speaks newline-delimited JSON-RPC over
//! stdio and sources replies from the golden fixtures.

use crate::acp_client::{AcpClient, AcpEvent, SpawnOptions, StreamKind};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Absolute path to the built mock binary. `CARGO_BIN_EXE_*` is only set for
/// integration tests, so unit (lib) tests rely on the target-dir fallback —
/// CI builds the bin explicitly before `cargo test` for that reason.
pub(crate) fn mock_runtime_path() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_mock_acp_runtime") {
        return PathBuf::from(p);
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/mock_acp_runtime");
    let p = if base.exists() {
        base
    } else {
        base.with_extension("exe") // Windows: mock_acp_runtime.exe
    };
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

/// Hold the module guard AND the shared app-home env lock for the whole
/// test, and point OMP_DESKTOP_HOME at a fresh temp dir (pattern copied from
/// `event_journal/recovery.rs` tests — env overrides race without it).
static HOME_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn test_home(tag: &str) -> (
    std::sync::MutexGuard<'static, ()>,
    parking_lot::MutexGuard<'static, ()>,
    PathBuf,
) {
    let module = HOME_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let env = crate::paths::APP_HOME_ENV_LOCK.lock();
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
    let Some((client, _rx)) = spawn_real() else {
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
            Ok(Ok(v)) => {
                let s = v.to_string();
                eprintln!("v1 probe {full}: OK {}", &s[..s.len().min(200)]);
            }
            Ok(Err(e)) => eprintln!("v1 probe {full}: ERR {e}"),
            Err(_) => panic!("v1 probe {full} timed out after 15s"),
        }
    }
    client.kill().await;
}
