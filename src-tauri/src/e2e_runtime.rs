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
