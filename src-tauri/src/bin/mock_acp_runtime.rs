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
                let text = frame.get("params").map(prompt_text).unwrap_or_default();
                let scenario = scenario_of(&text).to_string();
                match scenario {
                    // permission/toolcall/hang scenarios added by later tasks;
                    // default = happy.
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
