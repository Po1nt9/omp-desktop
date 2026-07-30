//! ACP client — JSON-RPC framing and transport for the OMP Runtime.
//!
//! Plan 1: fail-closed shell. All spawn paths return `runtime_unavailable`.
//! Plan 2: `OmpExtension` client added for versioned `_omp/desktop/v1/*` protocol.
//! No private extension bindings (private rewind namespace) remain.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex as ParkingMutex;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use tracing::{debug, error, info, warn};

use crate::error::{AgentError, AgentErrorCode};

#[derive(Debug, Clone)]
pub enum AcpEvent {
    State {
        backend: String,
        agent_session_id: Option<String>,
        model_id: Option<String>,
    },
    Stream {
        kind: StreamKind,
        text: String,
        message_id: Option<String>,
        done: bool,
    },
    ToolCall {
        tool_call_id: String,
        title: String,
        kind: String,
        status: String,
        raw: Value,
    },
    /// Live plan entries notification (sessionUpdate plan).
    Plan {
        entries: Value,
        /// Markdown / text body when available (planContent).
        body: Option<String>,
        /// Reserved for plan-gate reverse-requests; always `None` in Plan 1
        /// (no private extension is wired up).
        rpc_id: Option<u64>,
        tool_call_id: Option<String>,
    },
    PermissionRequest {
        rpc_id: u64,
        tool_call_id: String,
        tool_name: String,
        title: String,
        options: Value,
        raw: Value,
    },
    PromptComplete {
        stop_reason: String,
        /// True only for the `session/prompt` RPC result — the real end of the
        /// turn, ordered after every chunk the agent sent.
        ///
        /// Early-completion notifications from private extensions are no longer
        /// wired up in Plan 1, so this is always `true` on the live path.
        authoritative: bool,
    },
    /// Provider/API retry loop (sessionUpdate = retry_state). Host caps retries.
    RetryState {
        attempt: u32,
        max_retries: u32,
        reason: String,
        status: String,
    },
    /// Context compaction (auto or manual `/compact`).
    ContextCompact {
        trigger: String,
        tokens_before: Option<u64>,
        tokens_after: Option<u64>,
        summary_preview: Option<String>,
        note: Option<String>,
    },
    /// Turn / context usage reported by the agent (when present).
    /// Prefer these over UI char heuristics.
    UsageReported {
        /// Total context tokens after the turn when known.
        total_tokens: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        /// Optional raw kind for debugging (not shown to users).
        source: String,
    },
    Error {
        error: AgentError,
    },
    Stderr {
        line: String,
    },
    ProcessExited {
        code: Option<i32>,
    },
}

/// Host circuit-breaker: after this many provider retries, cancel the turn
/// (Codex-like). Agent may advertise a higher max (e.g. 15); we still stop at 5.
pub const HOST_PROVIDER_MAX_RETRIES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Assistant,
    Thought,
}

struct Pending {
    method: String,
    tx: oneshot::Sender<Result<Value, String>>,
}

const HANDSHAKE_TIMEOUT_SECS: u64 = 45;
const AUTH_TIMEOUT_SECS: u64 = 12;
/// Max wait for a single stdin write (JSON-RPC line). A wedged agent with a full
/// pipe used to block forever here — which froze interject ("引导"), cancel, and
/// any other Host→agent RPC before the request-level timeout could start.
const STDIN_WRITE_TIMEOUT_SECS: u64 = 8;
/// Max **silence** (no `session/update`) while waiting for `session/prompt`.
/// Long tool chains that keep emitting updates re-arm this window.
const PROMPT_IDLE_TIMEOUT_SECS: u64 = 600;
/// Absolute ceiling for one `session/prompt` wait (wedged process / lost RPC).
const PROMPT_ABSOLUTE_TIMEOUT_SECS: u64 = 4 * 60 * 60;
/// Poll slice while waiting for the `session/prompt` oneshot.
const PROMPT_WAIT_SLICE_SECS: u64 = 5;
/// Legacy alias used in docs/comments — idle silence window for prompt RPC.
#[allow(dead_code)]
const PROMPT_TIMEOUT_SECS: u64 = PROMPT_IDLE_TIMEOUT_SECS;

/// Whether a `session/prompt` wait should fail for silence or absolute age.
///
/// - `last_update`: last inbound `session/update` (None → use wait start as baseline)
/// - `wait_started`: when the RPC was dispatched
/// - `now`: current time
fn prompt_wait_should_timeout(
    last_update: Option<Instant>,
    wait_started: Instant,
    now: Instant,
    idle_timeout: Duration,
    absolute_timeout: Duration,
) -> Option<&'static str> {
    if now.saturating_duration_since(wait_started) >= absolute_timeout {
        return Some("absolute");
    }
    let baseline = last_update.unwrap_or(wait_started);
    if now.saturating_duration_since(baseline) >= idle_timeout {
        return Some("idle");
    }
    None
}

pub struct AcpClient {
    /// The spawned CLI process, or `None` in API mode (connected to a remote
    /// ACP server over TCP instead of spawning `grok agent stdio`).
    child: AsyncMutex<Option<Child>>,
    /// Write half of the transport (child stdin, or the TCP write half). Both
    /// impl `AsyncWrite`, so the JSON-RPC line protocol is transport-agnostic.
    stdin: AsyncMutex<Option<Box<dyn AsyncWrite + Unpin + Send>>>,
    next_id: AtomicU64,
    pending: ParkingMutex<HashMap<u64, Pending>>,
    event_tx: mpsc::UnboundedSender<AcpEvent>,
    agent_session_id: ParkingMutex<Option<String>>,
    cli_path: PathBuf,
    cwd: PathBuf,
    stopped: AtomicBool,
    reader_alive: AtomicBool,
    /// Recent stderr lines for crash diagnostics (ring, newest last).
    stderr_tail: ParkingMutex<Vec<String>>,
    /// Last inbound `session/update` — proof the agent is still producing turn
    /// output. Re-arms the `prompt_complete` fallback window.
    last_update_at: ParkingMutex<Option<Instant>>,
    /// Cached ACP `initialize` result. Used by the host to negotiate the
    /// `_omp/desktop/v1` capability after the handshake succeeds.
    /// `None` until `initialize_and_open_session` (or a direct `initialize`)
    /// populates it.
    initialize_result: ParkingMutex<Option<Value>>,
}

/// Spawn options for the ACP transport.
///
/// Plan 3: `binary_path` and `agent_dir` are consumed by the real spawn
/// path. When `binary_path` is `None` (and no `cli_path` is provided),
/// spawn returns `runtime_unavailable` — preserving the Plan 1 fail-closed
/// behavior for environments without the OMP runtime.
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    pub model_id: Option<String>,
    pub effort: Option<String>,
    pub permission_policy: Option<String>,
    /// Absolute path to the `omp` binary. When set, takes precedence over
    /// the `cli_path` argument passed to [`AcpClient::spawn_with_options`].
    pub binary_path: Option<PathBuf>,
    /// Optional agent working directory injected as `PI_CODING_AGENT_DIR`.
    pub agent_dir: Option<PathBuf>,
}

/// Stable backend identifier for the fail-closed shell.
pub const BACKEND_ID: &str = "runtime_unavailable";

/// Stable runtime-unavailable error for every execution / spawn path.
pub fn runtime_unavailable_error() -> AgentError {
    AgentError::new(
        AgentErrorCode::RuntimeUnavailable,
        "Agent runtime is unavailable (fail-closed shell).",
    )
}

impl AcpClient {
    /// Spawn `omp acp --stdio` using default options.
    ///
    /// Delegates to [`Self::spawn_with_options`] with an empty
    /// [`SpawnOptions`]. Returns `runtime_unavailable` when no binary is
    /// configured (preserving Plan 1 fail-closed behavior).
    pub fn spawn(
        cli_path: PathBuf,
        cwd: PathBuf,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<AcpEvent>), AgentError> {
        Self::spawn_with_options(cli_path, cwd, SpawnOptions::default())
    }

    /// Spawn `omp acp --stdio` with the given options.
    ///
    /// When `opts.binary_path` is set it takes precedence over `cli_path`.
    /// If neither is configured (or the binary does not exist on disk), the
    /// method returns `runtime_unavailable` — preserving the Plan 1
    /// fail-closed behavior for environments without the OMP runtime.
    pub fn spawn_with_options(
        cli_path: PathBuf,
        cwd: PathBuf,
        opts: SpawnOptions,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<AcpEvent>), AgentError> {
        // Prefer SpawnOptions::binary_path; fall back to the cli_path arg
        // when it is non-empty (back-compat with existing callers).
        let binary = opts
            .binary_path
            .clone()
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| {
                if !cli_path.as_os_str().is_empty() {
                    Some(cli_path.clone())
                } else {
                    None
                }
            });
        let binary = binary.ok_or_else(|| runtime_unavailable_error())?;
        if !binary.exists() {
            return Err(runtime_unavailable_error());
        }

        let mut cmd = tokio::process::Command::new(&binary);
        cmd.arg("acp").arg("--stdio");
        if let Some(dir) = &opts.agent_dir {
            cmd.env("PI_CODING_AGENT_DIR", dir);
        }
        cmd.env("OMP_DESKTOP_V1_PROTOCOL", "1");
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            AgentError::new(
                AgentErrorCode::CliNotFound,
                format!("failed to spawn {}: {e}", binary.display()),
            )
        })?;

        Self::from_child(child, binary, cwd)
    }

    /// Spawn with an explicit agent home directory.
    ///
    /// Sets `opts.agent_dir` (when not already set) and delegates to
    /// [`Self::spawn_with_options`]. The `session_data_mode` parameter is
    /// reserved for future use and currently ignored.
    pub fn spawn_with_home(
        cli_path: PathBuf,
        cwd: PathBuf,
        _session_data_mode: &str,
        opts: SpawnOptions,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<AcpEvent>), AgentError> {
        // `session_data_mode` is a legacy parameter; agent_dir is the
        // canonical way to set PI_CODING_AGENT_DIR. If the caller already
        // set agent_dir on opts, respect it.
        Self::spawn_with_options(cli_path, cwd, opts)
    }

    /// Plan 1 fail-closed: the runtime is unavailable. Returns the stable
    /// `RuntimeUnavailable` error without connecting to a remote server.
    /// TCP connect is for API mode (Plan 3+); stdio sidecar is the default.
    pub fn connect_tcp(
        _addr: &str,
        _cwd: PathBuf,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<AcpEvent>), AgentError> {
        Err(runtime_unavailable_error())
    }

    /// Build an [`AcpClient`] from a spawned child process, wiring the
    /// child's stdin/stdout/stderr pipes to the existing JSON-RPC framing.
    ///
    /// This is the single constructor that connects a `tokio::process::Child`
    /// to the read loop, pending-request map, and stdin writer. The caller
    /// must have spawned the child with `Stdio::piped()` on all three
    /// streams.
    fn from_child(
        mut child: tokio::process::Child,
        cli_path: PathBuf,
        cwd: PathBuf,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<AcpEvent>), AgentError> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::new(AgentErrorCode::AgentCrashed, "child stdin missing"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::new(AgentErrorCode::AgentCrashed, "child stdout missing")
        })?;
        let stderr = child.stderr.take();

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let client = Arc::new(Self {
            child: AsyncMutex::new(Some(child)),
            stdin: AsyncMutex::new(Some(Box::new(stdin))),
            next_id: AtomicU64::new(1),
            pending: ParkingMutex::new(HashMap::new()),
            event_tx: event_tx.clone(),
            agent_session_id: ParkingMutex::new(None),
            cli_path,
            cwd,
            stopped: AtomicBool::new(false),
            reader_alive: AtomicBool::new(true),
            stderr_tail: ParkingMutex::new(Vec::new()),
            last_update_at: ParkingMutex::new(None),
            initialize_result: ParkingMutex::new(None),
        });

        // stdout → JSON-RPC read loop (lines → handle_line).
        client.start_read_loop(Box::new(stdout));

        // stderr → best-effort log + AcpEvent::Stderr (no JSON-RPC framing).
        if let Some(stderr) = stderr {
            let c = Arc::clone(&client);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end();
                            if trimmed.is_empty() {
                                continue;
                            }
                            warn!("acp stderr: {trimmed}");
                            c.push_stderr(trimmed);
                            let _ = c.event_tx.send(AcpEvent::Stderr {
                                line: trimmed.to_string(),
                            });
                        }
                        Err(e) => {
                            warn!("acp stderr read error: {e}");
                            break;
                        }
                    }
                }
            });
        }

        Ok((client, event_rx))
    }

    /// Whether mock ACP mode is enabled via `GROK_APP_MOCK_ACP` env var.
    /// Mock mode bypasses the fail-closed spawn for local development/testing.
    pub fn use_mock() -> bool {
        std::env::var("GROK_APP_MOCK_ACP")
            .map(|v| !v.is_empty() && v != "0" && v != "false")
            .unwrap_or(false)
    }

    /// Spawn the transport read loop over any `AsyncRead` (child stdout or the
    /// TCP read half). Each newline-delimited JSON-RPC line is dispatched to
    /// [`handle_line`]; EOF fails pending requests and emits `ProcessExited`.
    fn start_read_loop(self: &Arc<Self>, reader: Box<dyn AsyncRead + Unpin + Send>) {
        let c = Arc::clone(self);
        tokio::spawn(async move {
            // Large session/update lines (available_commands) can be multi-MB.
            let mut reader = BufReader::with_capacity(8 * 1024 * 1024, reader);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        Arc::clone(&c).handle_line(trimmed).await;
                    }
                    Err(e) => {
                        error!("acp read error: {e}");
                        break;
                    }
                }
            }
            c.reader_alive.store(false, Ordering::SeqCst);
            let detail = c.format_exit_detail("Agent stream closed (EOF)");
            c.fail_all_pending(&detail);
            let _ = c.event_tx.send(AcpEvent::ProcessExited { code: None });
        });
    }

    fn push_stderr(&self, line: &str) {
        let mut buf = self.stderr_tail.lock();
        buf.push(line.to_string());
        const MAX: usize = 40;
        if buf.len() > MAX {
            let drain = buf.len() - MAX;
            buf.drain(0..drain);
        }
    }

    fn stderr_joined(&self) -> String {
        self.stderr_tail.lock().join(" | ")
    }

    fn format_exit_detail(&self, head: &str) -> String {
        let tail = self.stderr_joined();
        if tail.is_empty() {
            head.to_string()
        } else {
            // Cap length for UI
            let t = if tail.len() > 800 {
                format!("…{}", &tail[tail.len() - 800..])
            } else {
                tail
            };
            format!("{head}; stderr: {t}")
        }
    }

    fn fail_all_pending(&self, message: &str) {
        let pending: Vec<_> = self.pending.lock().drain().map(|(_, p)| p).collect();
        for p in pending {
            let _ = p.tx.send(Err(message.to_string()));
        }
    }

    async fn handle_line(self: Arc<Self>, line: &str) {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                warn!("acp non-json line: {e}: {}", &line[..line.len().min(200)]);
                return;
            }
        };

        // Response to our request (result or error present)
        if let Some(id) = json_id_u64(msg.get("id")) {
            if msg.get("result").is_some() || msg.get("error").is_some() {
                if let Some(p) = self.pending.lock().remove(&id) {
                    if let Some(err) = msg.get("error") {
                        let full = format_jsonrpc_error(err);
                        warn!("acp ← {} id={id} error: {}", p.method, full);
                        let _ = p.tx.send(Err(full));
                    } else {
                        let _ = p.tx.send(Ok(msg.get("result").cloned().unwrap_or(Value::Null)));
                    }
                } else if let Some(err) = msg.get("error") {
                    // Race: prompt_complete fallback already resolved pending, but the
                    // real RPC error arrived later (official subscription / provider fails).
                    // Must still surface the error — do not drop as "unknown id".
                    let full = format_jsonrpc_error(err);
                    warn!(
                        "acp late error response id={id} (pending already resolved): {full}"
                    );
                    let _ = self.event_tx.send(AcpEvent::Error {
                        error: classify_rpc_error(&full),
                    });
                } else {
                    debug!(
                        "acp late ok response id={id} (pending already resolved); keys={:?}",
                        msg.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
                    );
                }
                // also surface prompt complete via result stopReason
                if let Some(sr) = msg
                    .pointer("/result/stopReason")
                    .and_then(|v| v.as_str())
                {
                    let _ = self.event_tx.send(AcpEvent::PromptComplete {
                        stop_reason: sr.to_string(),
                        authoritative: true,
                    });
                }
                return;
            }
        }

        // Server request / notification
        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
            let req_id = json_id_u64(msg.get("id"));

            if method == "session/request_permission" {
                let rpc_id = req_id.unwrap_or(0);
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                let _ = self
                    .event_tx
                    .send(decode_permission_request(rpc_id, &params));
                return;
            }

            // True notifications: no id. (If id is present, must reply — never swallow.)
            if req_id.is_none() {
                // Standard ACP session updates (retry_state, chunks, tools, plan…).
                if method == "session/update" {
                    self.handle_session_update(msg.get("params").unwrap_or(&Value::Null));
                } else {
                    debug!("acp notification ignored method={method}");
                }
                return;
            }

            // Unhandled server→client request with id: reply so agent does not hang.
            let id = req_id.unwrap();
            warn!("acp unhandled server request method={method} id={id}");
            let err = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {method}"),
                }
            });
            if let Err(e) = self.write_line(&err).await {
                warn!("failed to reject unhandled method {method}: {e}");
            }
        }
    }

    /// Whether a `session/prompt` request is still awaiting its RPC result.
    fn has_pending_prompt(&self) -> bool {
        self.pending
            .lock()
            .values()
            .any(|p| p.method == "session/prompt")
    }

    fn handle_session_update(&self, params: &Value) {
        // Proof of life for the turn: re-arms the prompt idle timer.
        *self.last_update_at.lock() = Some(Instant::now());
        let events = decode_session_update(params);
        if events.is_empty() {
            let update = params.get("update").unwrap_or(params);
            let kind = update
                .get("sessionUpdate")
                .or_else(|| update.get("session_update"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            debug!("acp session/update ignored kind={kind}");
        }
        for ev in events {
            if let AcpEvent::RetryState {
                attempt,
                max_retries,
                reason,
                status,
            } = &ev
            {
                info!(
                    "acp retry_state attempt={attempt}/{max_retries} status={status} reason={}",
                    reason.chars().take(160).collect::<String>()
                );
            }
            if let AcpEvent::ContextCompact {
                trigger,
                tokens_before,
                tokens_after,
                ..
            } = &ev
            {
                info!(
                    "acp context compact trigger={trigger} before={tokens_before:?} after={tokens_after:?}"
                );
            }
            let _ = self.event_tx.send(ev);
        }
    }

    async fn write_line(&self, value: &Value) -> Result<(), String> {
        let mut line = serde_json::to_string(value).map_err(|e| e.to_string())?;
        line.push('\n');
        // Bound stdin lock+write: a dead/wedged agent must not pin the Host
        // forever (interject / cancel / prompt all go through here).
        let write_fut = async {
            let mut guard = self.stdin.lock().await;
            let stdin = guard.as_mut().ok_or_else(|| "stdin closed".to_string())?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(STDIN_WRITE_TIMEOUT_SECS),
            write_fut,
        )
        .await
        {
            Ok(r) => r,
            Err(_) => {
                // Wedged agent: free waiters, surface ProcessExited so Host ends
                // the turn, and kill the child so the pool slot is not held as
                // "still working" overnight.
                self.reader_alive.store(false, Ordering::SeqCst);
                let head = format!(
                    "stdin write timeout after {STDIN_WRITE_TIMEOUT_SECS}s (agent may be wedged)"
                );
                let detail = self.format_exit_detail(&head);
                error!("{detail}");
                self.fail_all_pending(&detail);
                let _ = self.event_tx.send(AcpEvent::ProcessExited { code: None });
                self.kill().await;
                Err(head)
            }
        }
    }
}

/// Pull a u64 from common token field names on a JSON object.
fn json_token_u64(obj: &Value, keys: &[&str]) -> Option<u64> {
    for k in keys {
        if let Some(n) = obj.get(*k).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().map(|i| i.max(0) as u64))
                .or_else(|| v.as_f64().map(|f| f.max(0.0) as u64))
        }) {
            return Some(n);
        }
    }
    None
}

/// Parse turn/context usage from a sessionUpdate payload.
/// Supports nested `usage` objects and flat camel/snake fields.
/// Returns None when no usage signal is present (do not invent zeros).
pub fn parse_usage_update(kind: &str, update: &Value) -> Option<AcpEvent> {
    let usage_obj = update
        .get("usage")
        .or_else(|| update.get("tokenUsage"))
        .or_else(|| update.get("token_usage"))
        .or_else(|| update.get("tokens"))
        .filter(|v| v.is_object());

    let root = usage_obj.unwrap_or(update);

    let input = json_token_u64(
        root,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
            "input",
        ],
    );
    let output = json_token_u64(
        root,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
            "output",
        ],
    );
    let total = json_token_u64(
        root,
        &[
            "totalTokens",
            "total_tokens",
            "contextTokens",
            "context_tokens",
            "usedTokens",
            "used_tokens",
            "tokens",
            "total",
        ],
    )
    .or_else(|| match (input, output) {
        (Some(i), Some(o)) => Some(i.saturating_add(o)),
        _ => None,
    });

    // Kind hints alone are not enough — need at least one number.
    if total.is_none() && input.is_none() && output.is_none() {
        return None;
    }

    // Avoid double-firing compact events that only carry tokens_before/after.
    if kind.contains("compact")
        && total.is_none()
        && (update.get("tokens_before").is_some()
            || update.get("tokensBefore").is_some()
            || update.get("tokens_after").is_some()
            || update.get("tokensAfter").is_some())
    {
        return None;
    }

    Some(AcpEvent::UsageReported {
        total_tokens: total,
        input_tokens: input,
        output_tokens: output,
        source: kind.to_string(),
    })
}

/// Parse compact-related sessionUpdate → (trigger, before, after, summary, note)
fn parse_context_compact_update(
    kind: &str,
    update: &Value,
) -> Option<(
    String,
    Option<u64>,
    Option<u64>,
    Option<String>,
    Option<String>,
)> {
    let tokens_before = update
        .get("tokens_before")
        .or_else(|| update.get("tokensBefore"))
        .and_then(|v| v.as_u64());
    let tokens_after = update
        .get("tokens_after")
        .or_else(|| update.get("tokensAfter"))
        .and_then(|v| v.as_u64());
    let summary_preview = update
        .get("summary_preview")
        .or_else(|| update.get("summaryPreview"))
        .or_else(|| update.get("summary"))
        .and_then(|v| v.as_str())
        .map(|s| s.chars().take(500).collect::<String>());
    let note = update
        .get("note")
        .or_else(|| update.get("message"))
        .or_else(|| update.get("reason"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let trigger_raw = update
        .get("trigger")
        .or_else(|| update.get("trigger_type"))
        .or_else(|| update.get("triggerType"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let trigger = if trigger_raw.eq_ignore_ascii_case("manual") || kind.contains("manual")
    {
        "manual".to_string()
    } else if trigger_raw.eq_ignore_ascii_case("auto")
        || kind.contains("auto")
        || kind == "tokens_used"
        || kind == "compaction_checkpoint"
    {
        "auto".to_string()
    } else if !trigger_raw.is_empty() {
        trigger_raw.to_string()
    } else {
        "auto".to_string()
    };

    if tokens_before.is_some()
        || tokens_after.is_some()
        || summary_preview.is_some()
        || kind.contains("compact")
        || kind == "tokens_used"
        || kind == "compaction_checkpoint"
    {
        Some((
            trigger,
            tokens_before,
            tokens_after,
            summary_preview,
            note,
        ))
    } else {
        None
    }
}

impl AcpClient {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_timeout(method, params, HANDSHAKE_TIMEOUT_SECS)
            .await
    }

    async fn request_timeout(
        &self,
        method: &str,
        params: Value,
        timeout_secs: u64,
    ) -> Result<Value, String> {
        if !self.reader_alive.load(Ordering::SeqCst) {
            return Err(format!(
                "agent stdout closed before {method}; {}",
                self.format_exit_detail("process dead")
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(
            id,
            Pending {
                method: method.to_string(),
                tx,
            },
        );
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        info!("acp → {method} id={id}");
        if let Err(e) = self.write_line(&msg).await {
            self.pending.lock().remove(&id);
            return Err(format!("write {method} failed: {e}"));
        }
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(r)) => match r {
                Ok(v) => {
                    info!("acp ← {method} id={id} ok");
                    Ok(v)
                }
                Err(e) => {
                    warn!("acp ← {method} id={id} error: {e}");
                    Err(e)
                }
            },
            Ok(Err(_)) => {
                let head =
                    format!("rpc channel closed while waiting for {method} (id={id})");
                error!("{}", self.format_exit_detail(&head));
                Err(head)
            }
            Err(_) => {
                self.pending.lock().remove(&id);
                // Keep user-facing error short; log stderr separately (MCP noise must not
                // surface as NETWORK_PROVIDER detail in the chat).
                let head = format!("rpc timeout on {method} (id={id}) after {timeout_secs}s");
                let logged = self.format_exit_detail(&head);
                error!("{logged}");
                Err(head)
            }
        }
    }

    /// `session/prompt` wait: idle-based silence timeout (re-armed by every
    /// `session/update`) plus an absolute ceiling. A fixed wall-clock timer
    /// killed multi-tool turns that were still healthy past 10 minutes.
    async fn request_prompt(&self, params: Value) -> Result<Value, String> {
        let method = "session/prompt";
        if !self.reader_alive.load(Ordering::SeqCst) {
            return Err(format!(
                "agent stdout closed before {method}; {}",
                self.format_exit_detail("process dead")
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(
            id,
            Pending {
                method: method.to_string(),
                tx,
            },
        );
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        info!("acp → {method} id={id}");
        if let Err(e) = self.write_line(&msg).await {
            self.pending.lock().remove(&id);
            return Err(format!("write {method} failed: {e}"));
        }

        let wait_started = Instant::now();
        // Mark activity at dispatch so pure silence is measured from send time.
        *self.last_update_at.lock() = Some(wait_started);
        let idle = Duration::from_secs(PROMPT_IDLE_TIMEOUT_SECS);
        let absolute = Duration::from_secs(PROMPT_ABSOLUTE_TIMEOUT_SECS);
        let slice = Duration::from_secs(PROMPT_WAIT_SLICE_SECS);
        let mut rx = rx;

        loop {
            tokio::select! {
                r = &mut rx => {
                    match r {
                        Ok(Ok(v)) => {
                            info!("acp ← {method} id={id} ok");
                            return Ok(v);
                        }
                        Ok(Err(e)) => {
                            warn!("acp ← {method} id={id} error: {e}");
                            return Err(e);
                        }
                        Err(_) => {
                            let head = format!(
                                "rpc channel closed while waiting for {method} (id={id})"
                            );
                            error!("{}", self.format_exit_detail(&head));
                            return Err(head);
                        }
                    }
                }
                _ = tokio::time::sleep(slice) => {
                    if !self.reader_alive.load(Ordering::SeqCst) {
                        self.pending.lock().remove(&id);
                        let head = format!(
                            "agent stdout closed while waiting for {method} (id={id})"
                        );
                        error!("{}", self.format_exit_detail(&head));
                        return Err(head);
                    }
                    let last = *self.last_update_at.lock();
                    let now = Instant::now();
                    if let Some(kind) =
                        prompt_wait_should_timeout(last, wait_started, now, idle, absolute)
                    {
                        self.pending.lock().remove(&id);
                        let idle_secs = last
                            .unwrap_or(wait_started)
                            .elapsed()
                            .as_secs();
                        let head = match kind {
                            "absolute" => format!(
                                "rpc timeout on {method} (id={id}) after {}s absolute (idle {idle_secs}s)",
                                wait_started.elapsed().as_secs()
                            ),
                            _ => format!(
                                "rpc timeout on {method} (id={id}) after {idle_secs}s idle (wall {}s)",
                                wait_started.elapsed().as_secs()
                            ),
                        };
                        let logged = self.format_exit_detail(&head);
                        error!("{logged}");
                        return Err(head);
                    }
                }
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_line(&msg).await
    }

    /// True while the agent stdout reader is still alive (process usable).
    pub fn is_alive(&self) -> bool {
        self.reader_alive.load(Ordering::SeqCst)
    }

    /// Working directory this process was spawned with.
    pub fn cwd(&self) -> &std::path::Path {
        &self.cwd
    }

    /// Returns a clone of the cached ACP `initialize` result, if the
    /// handshake has completed. Used by the host to negotiate the
    /// `_omp/desktop/v1` capability from the advertised `extensions` list.
    pub fn initialize_result(&self) -> Option<Value> {
        self.initialize_result.lock().clone()
    }

    /// Initialize + auth, then open a session.
    /// Prefer `session/load` when `resume_session_id` is set (runtime persists
    /// agent sessions under its home directory). Fall back to `session/new`.
    /// Returns `(session_id, resumed)`.
    pub async fn initialize_and_open_session(
        &self,
        resume_session_id: Option<&str>,
    ) -> Result<(String, bool), AgentError> {
        // Do not advertise client fs methods we do not implement — avoids agent
        // hanging on fs/readTextFile while we never reply.
        let init = self
            .request_timeout(
                "initialize",
                wire_initialize_params(),
                HANDSHAKE_TIMEOUT_SECS,
            )
            .await
            .map_err(|e| self.map_handshake_err("initialize", e))?;

        info!(
            "acp initialized agentVersion={:?} loadSession={:?}",
            init.pointer("/_meta/agentVersion")
                .or_else(|| init.pointer("/agentVersion")),
            init.pointer("/agentCapabilities/loadSession")
                .or_else(|| init.pointer("/capabilities/loadSession"))
        );

        // Cache the initialize result so the host can negotiate the
        // `_omp/desktop/v1` capability from the advertised `extensions` list
        // after the handshake succeeds.
        *self.initialize_result.lock() = Some(init.clone());

        // Best-effort cached auth — short timeout so a hung auth cannot burn 120s.
        match self
            .request_timeout(
                "authenticate",
                json!({ "methodId": "cached_token" }),
                AUTH_TIMEOUT_SECS,
            )
            .await
        {
            Ok(_) => info!("acp authenticate cached_token ok"),
            Err(e) => warn!("acp authenticate soft-fail (continuing): {e}"),
        }

        self.open_session(resume_session_id).await
    }

    /// Open or resume an ACP session on an already-initialized agent process.
    /// Used for cold connect after `initialize` and for warm process reuse when
    /// switching App sessions without respawning CLI.
    /// Returns `(session_id, resumed)`.
    ///
    /// Injects **enabled** MCP servers (App Extensions prefs + runtime MCP list)
    /// into `mcpServers` so independent and shared mode both see tools.
    pub async fn open_session(
        &self,
        resume_session_id: Option<&str>,
    ) -> Result<(String, bool), AgentError> {
        let cwd = self.cwd.to_string_lossy().to_string();
        if !self.cwd.is_dir() {
            return Err(AgentError::new(
                AgentErrorCode::AgentCrashed,
                format!("project cwd is not a directory: {cwd}"),
            ));
        }

        // Build ACP mcpServers off the async runtime (CLI list / file IO).
        let project_cwd = cwd.clone();
        let mcp_servers = tauri::async_runtime::spawn_blocking(move || {
            crate::extensions::build_session_mcp_servers(Some(project_cwd.as_str()))
        })
        .await
        .unwrap_or_else(|_| serde_json::json!([]));
        let mcp_count = mcp_servers.as_array().map(|a| a.len()).unwrap_or(0);
        info!("acp session open injecting mcpServers count={mcp_count}");

        // Prefer resuming the previous agent session for full native context.
        // Note: session/load replays history as ACP notifications; Host must
        // gate stream/tool side effects on `prompt_in_flight` (see session_manager).
        if let Some(rid) = resume_session_id.map(str::trim).filter(|s| !s.is_empty()) {
            info!("acp session/load begin sessionId={rid} cwd={cwd}");
            match self
                .request_timeout(
                    "session/load",
                    json!({
                        "sessionId": rid,
                        "cwd": cwd,
                        "mcpServers": mcp_servers.clone()
                    }),
                    HANDSHAKE_TIMEOUT_SECS,
                )
                .await
            {
                Ok(result) => {
                    let sid = result
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or(rid)
                        .to_string();
                    info!("acp session/load ok sessionId={sid}");
                    *self.agent_session_id.lock() = Some(sid.clone());
                    let model_id = result
                        .pointer("/models/currentModelId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let _ = self.event_tx.send(AcpEvent::State {
                        backend: BACKEND_ID.into(),
                        agent_session_id: Some(sid.clone()),
                        model_id,
                    });
                    return Ok((sid, true));
                }
                Err(e) => {
                    warn!("acp session/load fail ({e}); falling back to session/new");
                }
            }
        }

        let result = self
            .request_timeout(
                "session/new",
                json!({
                    "cwd": cwd,
                    "mcpServers": mcp_servers
                }),
                HANDSHAKE_TIMEOUT_SECS,
            )
            .await
            .map_err(|e| self.map_handshake_err("session/new", e))?;

        let sid = result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AgentError::new(
                    AgentErrorCode::AgentCrashed,
                    format!(
                        "session/new missing sessionId; keys={:?}",
                        result.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
                    ),
                )
            })?
            .to_string();

        *self.agent_session_id.lock() = Some(sid.clone());
        let model_id = result
            .pointer("/models/currentModelId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let _ = self.event_tx.send(AcpEvent::State {
            backend: BACKEND_ID.into(),
            agent_session_id: Some(sid.clone()),
            model_id,
        });

        Ok((sid, false))
    }

    /// Back-compat: always create a new session.
    pub async fn initialize_and_new_session(&self) -> Result<String, AgentError> {
        self.initialize_and_open_session(None)
            .await
            .map(|(sid, _)| sid)
    }

    /// Switch model on the live agent session (`session/set_model`).
    pub async fn set_model(&self, model_id: &str) -> Result<(), String> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err("model id empty".into());
        }
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| "no agent session".to_string())?;
        // ACP SetSessionModelRequest: sessionId + modelId (+ optional meta).
        let result = self
            .request(
                "session/set_model",
                json!({
                    "sessionId": sid,
                    "modelId": model_id,
                }),
            )
            .await
            .map_err(|e| format!("session/set_model: {e}"))?;
        // Best-effort: some agents echo currentModelId.
        if let Some(mid) = result
            .pointer("/models/currentModelId")
            .or_else(|| result.get("modelId"))
            .and_then(|v| v.as_str())
        {
            let _ = self.event_tx.send(AcpEvent::State {
                backend: BACKEND_ID.into(),
                agent_session_id: Some(sid),
                model_id: Some(mid.to_string()),
            });
        } else {
            let _ = self.event_tx.send(AcpEvent::State {
                backend: BACKEND_ID.into(),
                agent_session_id: Some(sid),
                model_id: Some(model_id.to_string()),
            });
        }
        Ok(())
    }

    /// Switch product session mode (`session/set_mode`). Tries candidate modeIds.
    pub async fn set_mode(&self, product_mode: &str) -> Result<String, String> {
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| "no agent session".to_string())?;
        let candidates = crate::agent_prefs::product_mode_candidates(product_mode);
        let mut last_err = String::from("no mode candidates");
        for mode_id in candidates {
            match self
                .request(
                    "session/set_mode",
                    json!({
                        "sessionId": sid,
                        "modeId": mode_id,
                    }),
                )
                .await
            {
                Ok(_) => {
                    tracing::info!("acp session/set_mode ok modeId={mode_id}");
                    return Ok(mode_id.to_string());
                }
                Err(e) => {
                    last_err = e;
                    tracing::debug!("acp session/set_mode {mode_id} soft-fail: {last_err}");
                }
            }
        }
        Err(format!("session/set_mode: {last_err}"))
    }

    fn map_handshake_err(&self, phase: &str, e: String) -> AgentError {
        let detail = self.format_exit_detail(&format!("{phase}: {e}"));
        let lower = detail.to_lowercase();
        if lower.contains("401")
            || lower.contains("auth")
            || lower.contains("unauthor")
            || lower.contains("login")
        {
            AgentError::new(AgentErrorCode::AuthFailed, detail)
        } else if lower.contains("network")
            || lower.contains("dns")
            || lower.contains("timeout")
            || lower.contains("5xx")
        {
            AgentError::new(AgentErrorCode::NetworkProvider, detail)
        } else {
            AgentError::new(AgentErrorCode::AgentCrashed, detail)
        }
    }

    pub async fn prompt(&self, text: &str) -> Result<(), AgentError> {
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| AgentError::new(AgentErrorCode::AgentCrashed, "no session"))?;

        self.stopped.store(false, Ordering::SeqCst);

        // Fire and wait for completion in background via request future
        let this_params = wire_session_prompt_params(&sid, text);

        let result = self
            .request_prompt(this_params)
            .await
            .map_err(|e| classify_rpc_error(&e))?;

        let stop = result
            .get("stopReason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn")
            .to_string();

        let _ = self.event_tx.send(AcpEvent::Stream {
            kind: StreamKind::Assistant,
            text: String::new(),
            message_id: None,
            done: true,
        });
        let _ = self.event_tx.send(AcpEvent::PromptComplete {
            stop_reason: stop,
            authoritative: true,
        });
        Ok(())
    }

    /// Like `prompt`, but supports mixed content blocks (text + images).
    pub async fn prompt_with_blocks(&self, blocks: &[PromptBlock]) -> Result<(), AgentError> {
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| AgentError::new(AgentErrorCode::AgentCrashed, "no session"))?;
        self.stopped.store(false, Ordering::SeqCst);
        let this_params = wire_session_prompt_params_blocks(&sid, blocks);
        let result = self
            .request_prompt(this_params)
            .await
            .map_err(|e| classify_rpc_error(&e))?;
        let stop = result
            .get("stopReason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn")
            .to_string();
        let _ = self.event_tx.send(AcpEvent::Stream {
            kind: StreamKind::Assistant,
            text: String::new(),
            message_id: None,
            done: true,
        });
        let _ = self.event_tx.send(AcpEvent::PromptComplete {
            stop_reason: stop,
            authoritative: true,
        });
        Ok(())
    }

    /// Cancel in-flight prompt (ACP notification — no id).
    pub async fn cancel(&self) -> Result<(), String> {
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| "no session".to_string())?;
        self.stopped.store(true, Ordering::SeqCst);
        self.notify("session/cancel", wire_session_cancel_params(&sid))
            .await
    }

    /// Unblock a waiting `session/prompt` RPC (e.g. after host circuit-breaker cancel).
    pub fn abort_pending_prompts(&self, message: &str) {
        let mut pending = self.pending.lock();
        let ids: Vec<u64> = pending
            .iter()
            .filter(|(_, p)| p.method == "session/prompt")
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(p) = pending.remove(&id) {
                let _ = p.tx.send(Err(message.to_string()));
            }
        }
    }

    pub async fn respond_permission(
        &self,
        rpc_id: u64,
        outcome: PermissionOutcome,
    ) -> Result<(), String> {
        let msg = wire_jsonrpc_result(rpc_id, wire_permission_result(&outcome));
        self.write_line(&msg).await
    }

    pub fn agent_session_id(&self) -> Option<String> {
        self.agent_session_id.lock().clone()
    }

    pub async fn kill(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
        *self.stdin.lock().await = None;
    }
}

#[derive(Debug, Clone)]
pub enum PermissionOutcome {
    Selected { option_id: String },
    Cancelled,
}

// ── Wire builders / pure decoders (locked by tests/fixtures/acp/) ────────────

/// Host → agent `initialize` params. Golden: `handshake_initialize.json`.
pub fn wire_initialize_params() -> Value {
    json!({
        "protocolVersion": 1,
        "clientInfo": { "name": "grok-app", "version": "0.1.0" },
        "capabilities": {}
    })
}

/// Host → agent `session/prompt` params.
pub fn wire_session_prompt_params(session_id: &str, text: &str) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": [{ "type": "text", "text": text }]
    })
}

/// A content block for the ACP `session/prompt` `prompt` array.
#[derive(Debug, Clone)]
pub enum PromptBlock {
    Text { text: String },
    /// `data` is base64-encoded image bytes; `mime_type` e.g. "image/png".
    Image { data: String, mime_type: String },
}

/// Host → agent `session/prompt` params with mixed content blocks.
pub fn wire_session_prompt_params_blocks(session_id: &str, blocks: &[PromptBlock]) -> Value {
    let prompt: Vec<Value> = blocks
        .iter()
        .map(|b| match b {
            PromptBlock::Text { text } => json!({ "type": "text", "text": text }),
            PromptBlock::Image { data, mime_type } => {
                json!({ "type": "image", "data": data, "mimeType": mime_type })
            }
        })
        .collect();
    json!({ "sessionId": session_id, "prompt": prompt })
}

#[cfg(test)]
mod prompt_block_tests {
    use super::*;

    #[test]
    fn test_wire_blocks_text_only() {
        let v = wire_session_prompt_params_blocks("s1", &[PromptBlock::Text { text: "hi".into() }]);
        assert_eq!(v["prompt"][0]["type"], "text");
        assert_eq!(v["prompt"][0]["text"], "hi");
    }

    #[test]
    fn test_wire_blocks_with_image() {
        let v = wire_session_prompt_params_blocks(
            "s1",
            &[
                PromptBlock::Text {
                    text: "describe this".into(),
                },
                PromptBlock::Image {
                    data: "BASE64".into(),
                    mime_type: "image/png".into(),
                },
            ],
        );
        assert_eq!(v["prompt"].as_array().unwrap().len(), 2);
        assert_eq!(v["prompt"][1]["type"], "image");
        assert_eq!(v["prompt"][1]["data"], "BASE64");
        assert_eq!(v["prompt"][1]["mimeType"], "image/png");
    }
}

/// Host → agent `session/cancel` notification params.
pub fn wire_session_cancel_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

/// Permission RPC result body (inside JSON-RPC `result`).
pub fn wire_permission_result(outcome: &PermissionOutcome) -> Value {
    match outcome {
        PermissionOutcome::Selected { option_id } => json!({
            "outcome": {
                "outcome": "selected",
                "optionId": option_id
            }
        }),
        PermissionOutcome::Cancelled => json!({
            "outcome": { "outcome": "cancelled" }
        }),
    }
}

/// Full JSON-RPC success envelope for a server→client request reply.
pub fn wire_jsonrpc_result(rpc_id: u64, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": rpc_id,
        "result": result,
    })
}

/// Decode `session/request_permission` params into a host event.
pub fn decode_permission_request(rpc_id: u64, params: &Value) -> AcpEvent {
    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let tool_call_id = tool_call
        .get("toolCallId")
        .or_else(|| tool_call.get("tool_call_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let title = tool_call
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Tool permission")
        .to_string();
    let tool_name = tool_call
        .get("kind")
        .or_else(|| tool_call.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("tool")
        .to_string();
    let options = params.get("options").cloned().unwrap_or(json!([]));
    AcpEvent::PermissionRequest {
        rpc_id,
        tool_call_id,
        tool_name,
        title,
        options,
        raw: params.clone(),
    }
}

/// Pure decode of `session/update` params → host events (no I/O).
/// Used by the live client and golden fixture tests.
pub fn decode_session_update(params: &Value) -> Vec<AcpEvent> {
    let update = params.get("update").unwrap_or(params);
    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut out = Vec::new();
    match kind {
        "agent_message_chunk" => {
            let text = update
                .pointer("/content/text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let message_id = update
                .get("messageId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            out.push(AcpEvent::Stream {
                kind: StreamKind::Assistant,
                text,
                message_id,
                done: false,
            });
        }
        // Grok/Gemini emit agent_thought_chunk; some paths also use "thought".
        "agent_thought_chunk" | "thought" => {
            let text = update
                .pointer("/content/text")
                .and_then(|v| v.as_str())
                .or_else(|| update.get("text").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            let message_id = update
                .get("messageId")
                .or_else(|| update.get("message_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if !text.is_empty() {
                out.push(AcpEvent::Stream {
                    kind: StreamKind::Thought,
                    text,
                    message_id,
                    done: false,
                });
            }
        }
        "plan" => {
            let entries = update.get("entries").cloned().unwrap_or(json!([]));
            let body = update
                .get("planContent")
                .or_else(|| update.get("plan_content"))
                .or_else(|| update.get("content"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            out.push(AcpEvent::Plan {
                entries,
                body,
                rpc_id: None,
                tool_call_id: None,
            });
        }
        "tool_call" | "tool_call_update" => {
            let tool_call_id = update
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = update
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let k = update
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = update
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title_l = title.to_ascii_lowercase();
            let kind_l = k.to_ascii_lowercase();
            if status == "completed"
                && (title_l.contains("compact")
                    || kind_l.contains("compact")
                    || tool_call_id.to_ascii_lowercase().contains("compact"))
            {
                out.push(AcpEvent::ContextCompact {
                    trigger: "manual".into(),
                    tokens_before: None,
                    tokens_after: None,
                    summary_preview: None,
                    note: Some(title.clone()),
                });
            }
            out.push(AcpEvent::ToolCall {
                tool_call_id,
                title,
                kind: k,
                status,
                raw: update.clone(),
            });
        }
        "retry_state" => {
            let attempt = update
                .get("attempt")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let max_retries = update
                .get("max_retries")
                .or_else(|| update.get("maxRetries"))
                .and_then(|v| v.as_u64())
                .unwrap_or(HOST_PROVIDER_MAX_RETRIES as u64) as u32;
            let reason = update
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = update
                .get("type")
                .or_else(|| update.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("retrying")
                .to_string();
            out.push(AcpEvent::RetryState {
                attempt,
                max_retries,
                reason,
                status,
            });
        }
        "tokens_used"
        | "compaction"
        | "compaction_completed"
        | "context_compact"
        | "auto_compact"
        | "compaction_checkpoint" => {
            if let Some((trigger, before, after, summary, note)) =
                parse_context_compact_update(kind, update)
            {
                out.push(AcpEvent::ContextCompact {
                    trigger,
                    tokens_before: before,
                    tokens_after: after,
                    summary_preview: summary,
                    note,
                });
            }
            // tokens_used often also carries a usage object — surface it.
            if let Some(ev) = parse_usage_update(kind, update) {
                out.push(ev);
            }
        }
        "usage"
        | "token_usage"
        | "tokenUsage"
        | "context_usage"
        | "contextUsage"
        | "turn_usage"
        | "turnUsage" => {
            if let Some(ev) = parse_usage_update(kind, update) {
                out.push(ev);
            }
        }
        _ => {
            if let Some(ev) = parse_usage_update(kind, update) {
                out.push(ev);
            }
            if update.get("tokens_before").is_some()
                || update.get("tokensBefore").is_some()
                || update.get("tokens_after").is_some()
                || update.get("tokensAfter").is_some()
            {
                if let Some((trigger, before, after, summary, note)) =
                    parse_context_compact_update(kind, update)
                {
                    out.push(AcpEvent::ContextCompact {
                        trigger,
                        tokens_before: before,
                        tokens_after: after,
                        summary_preview: summary,
                        note,
                    });
                    return out;
                }
            }
            let title = update
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if title.contains("compact") {
                out.push(AcpEvent::ContextCompact {
                    trigger: if title.contains("auto") {
                        "auto".into()
                    } else {
                        "manual".into()
                    },
                    tokens_before: None,
                    tokens_after: None,
                    summary_preview: None,
                    note: update
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod usage_parse_tests {
    use super::*;

    #[test]
    fn parse_nested_usage_object() {
        let update = json!({
            "usage": {
                "inputTokens": 1200,
                "outputTokens": 340,
                "totalTokens": 1540
            }
        });
        let ev = parse_usage_update("usage", &update).expect("usage");
        match ev {
            AcpEvent::UsageReported {
                total_tokens,
                input_tokens,
                output_tokens,
                ..
            } => {
                assert_eq!(total_tokens, Some(1540));
                assert_eq!(input_tokens, Some(1200));
                assert_eq!(output_tokens, Some(340));
            }
            _ => panic!("expected UsageReported"),
        }
    }

    #[test]
    fn parse_flat_snake_case_sums_total() {
        let update = json!({
            "input_tokens": 10,
            "output_tokens": 5
        });
        let ev = parse_usage_update("turn_usage", &update).expect("usage");
        match ev {
            AcpEvent::UsageReported {
                total_tokens,
                input_tokens,
                output_tokens,
                ..
            } => {
                assert_eq!(input_tokens, Some(10));
                assert_eq!(output_tokens, Some(5));
                assert_eq!(total_tokens, Some(15));
            }
            _ => panic!("expected UsageReported"),
        }
    }

    #[test]
    fn parse_usage_empty_returns_none() {
        assert!(parse_usage_update("usage", &json!({})).is_none());
        assert!(parse_usage_update("other", &json!({ "title": "hi" })).is_none());
    }
}

fn format_jsonrpc_error(err: &Value) -> String {
    let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
    let message = err
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("rpc error")
        .to_string();
    let data = err
        .get("data")
        .map(|d| d.to_string())
        .filter(|s| s != "null" && !s.is_empty());
    match data {
        Some(d) => format!("{message} (code {code}, data: {d})"),
        None => format!("{message} (code {code})"),
    }
}

fn classify_rpc_error(e: &str) -> AgentError {
    let lower = e.to_lowercase();
    if lower.contains("quota")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("429")
        || lower.contains("not entitled")
        || lower.contains("insufficient credit")
        || lower.contains("out of credits")
        || lower.contains("usage limit")
    {
        return AgentError::new(AgentErrorCode::QuotaExceeded, e);
    }
    if lower.contains("could not connect")
        || lower.contains("edit aborted")
        || lower.contains("no active session")
        || lower.contains("acp client missing")
        || lower.contains("no session")
    {
        return AgentError::new(AgentErrorCode::ConnectFailed, e);
    }
    if lower.contains("401")
        || lower.contains("auth")
        || lower.contains("unauthor")
        || lower.contains("login")
        || lower.contains("access denied")
        || lower.contains("authentication code")
    {
        return AgentError::new(AgentErrorCode::AuthFailed, e);
    }
    if lower.contains("subscription") || lower.contains("billing") || lower.contains("payment") {
        // Subscription/billing without explicit quota → treat as auth/entitlement.
        return AgentError::new(AgentErrorCode::AuthFailed, e);
    }
    if lower.contains("dns")
        || lower.contains("timeout")
        || lower.contains("network")
        || lower.contains("5xx")
        || lower.contains("503")
        || lower.contains("rpc channel closed")
        || lower.contains("shell_api_error")
        || lower.contains("no available channels")
        || lower.contains("provider retries")
        || lower.contains("service unavailable")
    {
        // Timeouts / provider 503 / channel-empty are network-provider, not process crash.
        AgentError::new(AgentErrorCode::NetworkProvider, e)
    } else if lower.contains("not found") && lower.contains("cli") {
        AgentError::new(AgentErrorCode::CliNotFound, e)
    } else {
        AgentError::new(AgentErrorCode::AgentCrashed, e)
    }
}

/// Whether host should stop waiting and fail the turn (Codex-like 5-retry cap).
pub fn should_abort_provider_retry(attempt: u32, max_retries: u32, status: &str) -> bool {
    let status = status.to_lowercase();
    if status.contains("fail")
        || status.contains("exhaust")
        || status.contains("gave_up")
        || status.contains("give_up")
        || status == "error"
    {
        return true;
    }
    let cap = max_retries.min(HOST_PROVIDER_MAX_RETRIES).max(1);
    attempt >= cap
}

/// Result of an API-mode connectivity probe (see [`probe_acp_server`]).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpProbeResult {
    /// The server accepted a TCP connection and returned a valid ACP
    /// `initialize` result.
    pub ok: bool,
    pub agent_version: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
}

impl AcpProbeResult {
    fn fail(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            agent_version: None,
            model: None,
            error: Some(error.into()),
        }
    }
}

/// Lightweight connectivity check for **API mode**: TCP-connect to an ACP
/// server (`host:port`), perform the `initialize` handshake, and report the
/// agent version / current model. Creates no client and no session — just
/// confirms the address is reachable and speaks ACP. Bounded by timeouts so
/// a wrong address / silent port fails fast.
pub async fn probe_acp_server(addr: &str) -> AcpProbeResult {
    use tokio::time::{timeout, Duration};

    let stream = match timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return AcpProbeResult::fail(format!("connect failed: {e}")),
        Err(_) => return AcpProbeResult::fail("connect timed out (5s)"),
    };
    let _ = stream.set_nodelay(true);
    let (rd, mut wr) = stream.into_split();

    let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientInfo":{"name":"grok-app-probe","version":"0"},"capabilities":{}}}"#;
    let write = async {
        wr.write_all(req.as_bytes()).await?;
        wr.write_all(b"\n").await?;
        wr.flush().await
    };
    if let Err(e) = write.await {
        return AcpProbeResult::fail(format!("write failed: {e}"));
    }

    let mut reader = BufReader::new(rd);
    let mut line = String::new();
    match timeout(Duration::from_secs(20), reader.read_line(&mut line)).await {
        Ok(Ok(n)) if n > 0 => {
            let v: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
            let result = &v["result"];
            if !result.is_object() {
                return AcpProbeResult::fail(
                    "connected, but no ACP initialize result in response",
                );
            }
            let meta = &result["_meta"];
            AcpProbeResult {
                ok: true,
                agent_version: meta["agentVersion"].as_str().map(String::from),
                model: meta["modelState"]["currentModelId"]
                    .as_str()
                    .map(String::from),
                error: None,
            }
        }
        Ok(Ok(_)) => {
            AcpProbeResult::fail("server closed the connection (EOF) before responding")
        }
        Ok(Err(e)) => AcpProbeResult::fail(format!("read failed: {e}")),
        Err(_) => AcpProbeResult::fail("connected, but no ACP response within 20s"),
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn abort_at_host_cap_even_if_agent_allows_more() {
        assert!(!should_abort_provider_retry(1, 15, "retrying"));
        assert!(!should_abort_provider_retry(4, 15, "retrying"));
        assert!(should_abort_provider_retry(5, 15, "retrying"));
        assert!(should_abort_provider_retry(6, 15, "retrying"));
    }

    #[test]
    fn abort_on_failed_status() {
        assert!(should_abort_provider_retry(1, 15, "failed"));
        assert!(should_abort_provider_retry(1, 15, "exhausted"));
    }

    #[test]
    fn respect_lower_agent_max() {
        assert!(!should_abort_provider_retry(1, 3, "retrying"));
        assert!(should_abort_provider_retry(3, 3, "retrying"));
    }
}

#[cfg(test)]
mod prompt_wait_timeout_tests {
    use super::*;

    fn idle() -> Duration {
        Duration::from_secs(PROMPT_IDLE_TIMEOUT_SECS)
    }
    fn absolute() -> Duration {
        Duration::from_secs(PROMPT_ABSOLUTE_TIMEOUT_SECS)
    }

    #[test]
    fn healthy_activity_never_times_out() {
        let started = Instant::now();
        let last = started + Duration::from_secs(30 * 60);
        let now = last + Duration::from_secs(60);
        // 30+ min wall clock, but last update 60s ago — under 600s idle.
        assert_eq!(
            prompt_wait_should_timeout(Some(last), started, now, idle(), absolute()),
            None
        );
    }

    #[test]
    fn pure_silence_hits_idle() {
        let started = Instant::now();
        let now = started + idle();
        assert_eq!(
            prompt_wait_should_timeout(None, started, now, idle(), absolute()),
            Some("idle")
        );
    }

    #[test]
    fn stale_last_update_hits_idle() {
        let started = Instant::now();
        let last = started + Duration::from_secs(10);
        let now = last + idle();
        assert_eq!(
            prompt_wait_should_timeout(Some(last), started, now, idle(), absolute()),
            Some("idle")
        );
    }

    #[test]
    fn absolute_ceiling_even_with_fresh_updates() {
        let started = Instant::now();
        let now = started + absolute();
        let last = now - Duration::from_secs(1);
        assert_eq!(
            prompt_wait_should_timeout(Some(last), started, now, idle(), absolute()),
            Some("absolute")
        );
    }
}

fn json_id_u64(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    if let Some(u) = v.as_u64() {
        return Some(u);
    }
    if let Some(i) = v.as_i64() {
        if i >= 0 {
            return Some(i as u64);
        }
    }
    if let Some(s) = v.as_str() {
        return s.parse().ok();
    }
    None
}

#[cfg(test)]
mod live_handshake_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn live_initialize_session_new_under_30s() {
        if std::env::var("GROK_APP_LIVE_ACP").ok().as_deref() != Some("1") {
            eprintln!("skip live ACP (set GROK_APP_LIVE_ACP=1)");
            return;
        }
        let cli = which::which("grok").or_else(|_| {
            let p = crate::process_util::user_home().join(".grok/bin/grok");
            if p.exists() {
                Ok(p)
            } else {
                let p2 = crate::process_util::user_home().join(r".grok\bin\grok.exe");
                if p2.exists() {
                    Ok(p2)
                } else {
                    Err(which::Error::CannotFindBinaryPath)
                }
            }
        }).expect("runtime");
        let cwd = std::env::current_dir().unwrap();
        let t0 = std::time::Instant::now();
        let (client, mut events) = AcpClient::spawn(cli, cwd).expect("spawn");
        // drain events in bg
        tokio::spawn(async move {
            while let Some(ev) = events.recv().await {
                eprintln!("ev: {:?}", std::mem::discriminant(&ev));
            }
        });
        let sid = tokio::time::timeout(Duration::from_secs(45), client.initialize_and_new_session())
            .await
            .expect("overall timeout")
            .expect("handshake");
        eprintln!("OK session={} in {:?}", sid, t0.elapsed());
        client.kill().await;
        assert!(!sid.is_empty());
    }
}

#[cfg(test)]
mod private_bindings_regression_tests {
    use super::*;

    /// Plan 1 audit + Plan 2 Task 7: the dead private rewind bindings must not
    /// return. The binding code was unreachable (fail-closed spawn) but still
    /// shipped in the binary; removing the methods entirely and asserting via
    /// `include_str!` keeps future drift from reintroducing them.
    ///
    /// The forbidden strings are assembled from parts so the test source itself
    /// does not contain them as contiguous literals (which would make
    /// `contains` always return true and the assertion always fail).
    #[test]
    fn no_private_xai_bindings_remain() {
        let source = include_str!("acp_client.rs");
        let ns: String = ["x", ".ai/", "rewind", "/"].concat();
        let points: String = format!("{ns}{}", "points");
        let execute: String = format!("{ns}{}", "execute");
        assert!(
            !source.contains(&points),
            "forbidden points binding must be removed"
        );
        assert!(
            !source.contains(&execute),
            "forbidden execute binding must be removed"
        );
    }
}

#[cfg(test)]
mod spawn_reactivation_tests {
    use super::*;

    fn assert_runtime_unavailable(
        result: Result<(Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>), AgentError>,
    ) {
        match result {
            Ok(_) => panic!("expected runtime_unavailable error, got Ok"),
            Err(err) => {
                assert_eq!(
                    err.code,
                    AgentErrorCode::RuntimeUnavailable,
                    "expected RuntimeUnavailable, got {:?}: {}",
                    err.code,
                    err.message
                );
            }
        }
    }

    #[test]
    fn spawn_returns_runtime_unavailable_when_no_binary_configured() {
        // Neither SpawnOptions::binary_path nor the cli_path arg is set —
        // must preserve Plan 1 fail-closed behavior.
        let opts = SpawnOptions::default();
        let cli_path = PathBuf::new();
        let cwd = std::env::temp_dir();
        let result = AcpClient::spawn_with_options(cli_path, cwd, opts);
        assert_runtime_unavailable(result);
    }

    #[test]
    fn spawn_returns_runtime_unavailable_when_binary_does_not_exist() {
        // A binary_path is configured but the file does not exist on disk.
        let opts = SpawnOptions {
            binary_path: Some(PathBuf::from("/nonexistent/omp-binary-12345")),
            ..Default::default()
        };
        let cli_path = PathBuf::new();
        let cwd = std::env::temp_dir();
        let result = AcpClient::spawn_with_options(cli_path, cwd, opts);
        assert_runtime_unavailable(result);
    }

    #[test]
    fn spawn_returns_runtime_unavailable_when_cli_path_only_is_empty() {
        // SpawnOptions has no binary_path and cli_path is empty — must fail.
        let opts = SpawnOptions {
            model_id: Some("test-model".to_string()),
            ..Default::default()
        };
        let result = AcpClient::spawn_with_options(PathBuf::new(), std::env::temp_dir(), opts);
        assert_runtime_unavailable(result);
    }

    #[test]
    fn spawn_options_has_binary_path_and_agent_dir_fields() {
        // Verify the fields exist and round-trip through Default + setters.
        let opts = SpawnOptions {
            binary_path: Some(PathBuf::from("/usr/local/bin/omp")),
            agent_dir: Some(PathBuf::from("/tmp/agent")),
            model_id: Some("gpt-4".to_string()),
            effort: Some("high".to_string()),
            permission_policy: Some("yolo".to_string()),
        };
        assert_eq!(opts.binary_path, Some(PathBuf::from("/usr/local/bin/omp")));
        assert_eq!(opts.agent_dir, Some(PathBuf::from("/tmp/agent")));
        assert_eq!(opts.model_id.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn spawn_delegates_to_spawn_with_options() {
        // spawn() with an empty cli_path must also return runtime_unavailable.
        let result = AcpClient::spawn(PathBuf::new(), std::env::temp_dir());
        assert_runtime_unavailable(result);
    }

    #[test]
    fn connect_tcp_still_returns_runtime_unavailable() {
        // Plan 3 Task 2 Step 4: TCP connect remains fail-closed.
        let result = AcpClient::connect_tcp("127.0.0.1:0", std::env::temp_dir());
        assert_runtime_unavailable(result);
    }

    #[test]
    fn spawn_options_default_has_none_for_binary_and_agent_dir() {
        let opts = SpawnOptions::default();
        assert!(opts.binary_path.is_none());
        assert!(opts.agent_dir.is_none());
        assert!(opts.model_id.is_none());
        assert!(opts.effort.is_none());
        assert!(opts.permission_policy.is_none());
    }
}
