# Plan 3: Supervisor, Core ACP, Event Journal, Multi-Session

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the real OMP Runtime into the Desktop host by reactivating the ACP transport, implementing a Supervisor that manages the sidecar process lifecycle, wiring v1 protocol handlers to real OMP resources, and establishing the event journal and multi-session coordination.

**Architecture:** The Supervisor spawns and monitors the bundled `omp acp` sidecar. The existing `AcpClient` transport (currently fail-closed) is reactivated with real spawn/initialize/session paths. The v1 protocol handlers (stubbed in Plan 2) are wired to actual OMP `SessionManager`, `AuthStorage`, `ModelManager`, and extension loader. An event journal records durable commit points for crash recovery. Multi-session coordination maps Desktop UI sessions to OMP agent sessions via the existing `SessionManager` infrastructure.

**Tech Stack:** Rust, Tauri 2, tokio, TypeScript (OMP submodule patch), bun, Vitest, `cargo test`.

---

## Global Constraints

- The spec for this plan is `docs/superpowers/specs/2026-07-28-omp-desktop-design.md` §3 item 3 and §10 (process model).
- Plan 2 must be complete before starting — the v1 protocol surface is the contract layer.
- Default: single supervised sidecar. Per-session process mode is test-only (behind a flag), no UI.
- The OMP runtime binary is bundled in the app resources. The Supervisor discovers it via Tauri's resource resolver.
- `OMP_DESKTOP_V1_PROTOCOL=1` is now the default (not just a test flag) — the v1 protocol is always negotiated.
- Brand normalization: `agentInfo.title` from OMP ("Oh My Pi") is displayed as "OMP" or "OMP Runtime" in all user-visible surfaces.
- Do not change the application version from `0.1.9` in Plan 3.
- All existing tests must continue to pass.
- The mock_acp transport must remain functional for testing.

---

### Task 1: Implement the Supervisor Process Manager

**Files:**
- Create: `src-tauri/src/supervisor/mod.rs`
- Create: `src-tauri/src/supervisor/process.rs`
- Create: `src-tauri/src/supervisor/tests.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Tauri resource resolver (for finding the bundled `omp` binary); `AcpClient` from Plan 1.
- Produces: `Supervisor` struct that spawns, monitors, and recovers the OMP sidecar.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/supervisor/tests.rs`:

```rust
use super::*;

#[tokio::test]
async fn supervisor_returns_unavailable_when_binary_not_found() {
    let supervisor = Supervisor::new(SupervisorConfig {
        binary_path: None, // No binary configured
        ..Default::default()
    });
    let result = supervisor.start().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("runtime_unavailable"));
}

#[test]
fn supervisor_config_has_sensible_defaults() {
    let config = SupervisorConfig::default();
    assert_eq!(config.max_restarts, 3);
    assert_eq!(config.restart_delay_ms, 1000);
    assert_eq!(config.health_check_interval_ms, 5000);
}
```

- [ ] **Step 2: Implement the Supervisor**

Create `src-tauri/src/supervisor/mod.rs`:

```rust
pub mod process;
pub mod tests;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::process::Child;

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub binary_path: Option<PathBuf>,
    pub agent_dir: Option<PathBuf>,
    pub max_restarts: u32,
    pub restart_delay_ms: u64,
    pub health_check_interval_ms: u64,
    pub env_vars: Vec<(String, String)>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            agent_dir: None,
            max_restarts: 3,
            restart_delay_ms: 1000,
            health_check_interval_ms: 5000,
            env_vars: vec![("OMP_DESKTOP_V1_PROTOCOL".to_string(), "1".to_string())],
        }
    }
}

pub struct Supervisor {
    config: SupervisorConfig,
    child: Arc<RwLock<Option<Child>>>,
    restart_count: Arc<RwLock<u32>>,
}

impl Supervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            child: Arc::new(RwLock::new(None)),
            restart_count: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let binary = self.config.binary_path.as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "runtime_unavailable: OMP binary not configured"))?;
        if !binary.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "runtime_unavailable: OMP binary not found",
            )));
        }
        // Spawn omp acp --stdio
        let mut cmd = tokio::process::Command::new(binary);
        cmd.arg("acp").arg("--stdio");
        if let Some(dir) = &self.config.agent_dir {
            cmd.env("PI_CODING_AGENT_DIR", dir);
        }
        for (k, v) in &self.config.env_vars {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::piped())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());
        let child = cmd.spawn()?;
        *self.child.write().await = Some(child);
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        let mut guard = self.child.write().await;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    *guard = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut guard = self.child.write().await;
        if let Some(mut child) = guard.take() {
            // Try graceful shutdown first
            let _ = child.kill().await;
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Register module, run tests, commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml supervisor --locked
git add src-tauri/src/supervisor src-tauri/src/lib.rs
git commit -m "feat: add OMP Runtime Supervisor process manager"
```

---

### Task 2: Reactivate the ACP Transport (Remove Fail-Closed Stubs)

**Files:**
- Modify: `src-tauri/src/acp_client.rs`
- Modify: `src-tauri/src/runtime_availability.rs`
- Create: `src-tauri/src/acp_transport.rs`

**Interfaces:**
- Consumes: Supervisor from Task 1; existing JSON-RPC framing in `acp_client.rs`.
- Produces: `AcpClient` that can spawn a real OMP sidecar, perform `initialize`, and handle `session/*` methods.

- [ ] **Step 1: Write a transport reactivation test**

Create `src-tauri/src/acp_transport.rs` with a test that verifies the spawn path is no longer hard-coded to `runtime_unavailable` when a binary is configured:

```rust
#[cfg(test)]
mod tests {
    use crate::acp_client::AcpClient;

    #[test]
    fn spawn_options_are_consumed_when_binary_configured() {
        // Verify that AcpClient::spawn_with_options no longer unconditionally
        // returns runtime_unavailable — it should check for a configured binary
        // and only return runtime_unavailable if no binary is found.
        let opts = crate::acp_client::SpawnOptions::default();
        // Without a binary, this should still fail — but with a binary, it should try to spawn.
        // The actual spawn test requires a mock binary, tested in integration tests.
    }
}
```

- [ ] **Step 2: Reactivate spawn in acp_client.rs**

In `src-tauri/src/acp_client.rs`, modify the `spawn`, `spawn_with_options`, and `spawn_with_home` methods to:
1. Check for a configured binary path (via `SpawnOptions`)
2. If a binary is configured, spawn it via `tokio::process::Command`
3. Wire stdin/stdout/stderr to the existing JSON-RPC framing
4. If no binary is configured, return `runtime_unavailable` (preserving the Plan 1 behavior for environments without the runtime)

The key change is replacing:
```rust
pub fn spawn_with_options(_opts: SpawnOptions) -> Result<Self, AgentError> {
    Err(runtime_unavailable_error())
}
```
with:
```rust
pub fn spawn_with_options(opts: SpawnOptions) -> Result<Self, AgentError> {
    let binary = opts.binary_path.as_ref()
        .ok_or_else(|| runtime_unavailable_error())?;
    if !binary.exists() {
        return Err(runtime_unavailable_error());
    }
    // Spawn omp acp --stdio
    let mut cmd = std::process::Command::new(binary);
    cmd.arg("acp").arg("--stdio");
    if let Some(dir) = &opts.agent_dir {
        cmd.env("PI_CODING_AGENT_DIR", dir);
    }
    cmd.env("OMP_DESKTOP_V1_PROTOCOL", "1");
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| AgentError::new(AgentErrorCode::RuntimeUnavailable, e.to_string()))?;
    // Wire to existing framing...
    let (stdin, stdout, stderr) = take_child_pipes(&child);
    let client = AcpClient::from_streams(stdin, stdout, stderr);
    Ok(client)
}
```

- [ ] **Step 3: Update SpawnOptions to include binary_path**

Add `binary_path: Option<PathBuf>` and `agent_dir: Option<PathBuf>` to the `SpawnOptions` struct (currently marked `#[allow(dead_code)]`).

- [ ] **Step 4: Update runtime_availability to be dynamic**

Update `src-tauri/src/runtime_availability.rs` to read from a runtime state instead of a hardcoded constant. Add a `RuntimeState` that the Supervisor updates when the sidecar starts/stops.

- [ ] **Step 5: Run tests, commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
git add src-tauri/src/acp_client.rs src-tauri/src/acp_transport.rs src-tauri/src/runtime_availability.rs src-tauri/src/lib.rs
git commit -m "feat: reactivate ACP transport with real sidecar spawn"
```

---

### Task 3: Wire v1 Protocol Handlers to Real OMP Resources

**Files:**
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts`
- Modify: `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/handlers/index.ts`

**Interfaces:**
- Consumes: Plan 2 v1 dispatcher and handlers; existing OMP `SessionManager`, `AuthStorage`, `ModelManager`, extension loader.
- Produces: v1 handlers that return real data instead of `runtime_unavailable`.

- [ ] **Step 1: Implement normalizers for OMP → v1 ID and shape conversion**

In the OMP submodule, create `packages/coding-agent/src/modes/acp/desktop-v1/normalizers.ts`:

```ts
import { randomBytes } from "node:crypto";

// OMP uses UUIDv7 for session IDs; v1 protocol uses sess_<base32>.
// Create a stable mapping from OMP session IDs to v1 protocol IDs.
const sessionMap = new Map<string, string>();

export function toV1SessionId(ompSessionId: string): string {
  let v1Id = sessionMap.get(ompSessionId);
  if (!v1Id) {
    v1Id = `sess_${randomBytes(16).toString("base32").toLowerCase().replace(/=/g, "").slice(0, 26)}`;
    sessionMap.set(ompSessionId, v1Id);
  }
  return v1Id;
}

export function fromV1SessionId(v1SessionId: string): string | undefined {
  for (const [ompId, v1] of sessionMap) {
    if (v1 === v1SessionId) return ompId;
  }
  return undefined;
}

export function normalizeSessionInfo(s: any) {
  return {
    id: toV1SessionId(s.id || s.sessionId),
    cwd: s.cwd,
    title: s.title ?? null,
    modified: typeof s.modified === "string" ? s.modified : s.modified?.toISOString() ?? new Date().toISOString(),
    parentSession: s.parentSession ? toV1SessionId(s.parentSession) : null,
  };
}

export function normalizeUsageReport(r: any) {
  return {
    providerId: r.provider || r.providerId || "unknown",
    modelId: r.model || r.modelId || "unknown",
    inputTokens: r.inputTokens || r.usage?.inputTokens || 0,
    outputTokens: r.outputTokens || r.usage?.outputTokens || 0,
    timestamp: r.fetchedAt || r.timestamp || new Date().toISOString(),
  };
}

export function normalizeExtension(e: any) {
  return {
    id: e.id || e.providerId,
    providerId: e.providerId || e.id,
    enabled: e.state === "active" || e.enabled === true,
  };
}

export function normalizeProvider(p: any) {
  return {
    id: p.id,
    name: p.name || p.title || p.id,
    authMethods: p.authMethods || [],
  };
}

export function normalizeModel(m: any) {
  return {
    id: m.id,
    providerId: m.providerId || m.provider,
    displayName: m.displayName || m.name || m.id,
    contextWindow: m.contextWindow ?? null,
  };
}
```

- [ ] **Step 2: Replace handler stubs with real wiring**

Update `acp-agent.ts`'s `buildDesktopV1HandlerDeps()` to wire real OMP resources:

```ts
function buildDesktopV1HandlerDeps(session: any, settings: any, authStorage: any): Record<string, unknown> {
  return {
    sessionManager: {
      listAll: async (limit: number) => {
        const result = await session.sessionManager.listAll(limit);
        return {
          sessions: result.sessions.map(normalizeSessionInfo),
          total: result.total,
        };
      },
      list: async (cwd: string, limit: number) => {
        const result = await session.sessionManager.list(cwd, limit);
        return { sessions: result.sessions.map(normalizeSessionInfo) };
      },
      listProjects: async () => session.sessionManager.listProjects(),
    },
    usageReports: () => session.fetchUsageReports().then((r: any[]) => r.map(normalizeUsageReport)),
    extensions: {
      loadAll: (cwd?: string) => loadAllExtensions(cwd, settings.disabledExtensions)
        .then((exts: any[]) => exts.map(normalizeExtension)),
      toggle: (id: string, enabled?: boolean) =>
        enabled !== false ? enableProvider(id) : disableProvider(id),
    },
    providers: {
      list: () => listProviders().then((ps: any[]) => ps.map(normalizeProvider)),
      listModels: (id?: string) => listModels(id).then((ms: any[]) => ms.map(normalizeModel)),
    },
    authStorage,
    mcp: {
      list: (cwd?: string) => listMcpSources(cwd),
      discover: (cwd: string) => discoverMcp(cwd),
    },
    sessionConfig: {
      get: (id?: string) => getSessionConfig(id),
      set: (id, config) => setSessionConfig(id, config),
    },
    diagnostics: { selfCheck: () => selfCheck() },
  };
}
```

- [ ] **Step 3: Run tests, commit**

```bash
cd runtime/oh-my-pi && OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/
git add runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/normalizers.ts runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: wire v1 handlers to real OMP resources"
```

---

### Task 4: Implement Event Journal with Stable Event IDs

**Files:**
- Create: `src-tauri/src/event_journal/mod.rs`
- Create: `src-tauri/src/event_journal/tests.rs`
- Modify: `src-tauri/src/session_manager.rs`

**Interfaces:**
- Consumes: ACP event stream from `acp_client.rs`; session manager state.
- Produces: Durable event journal with stable event IDs, commit points, and replay support.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/event_journal/tests.rs`:

```rust
use super::*;

#[test]
fn journal_generates_stable_event_ids() {
    let mut journal = EventJournal::new("sess_test".to_string());
    let id1 = journal.append(EventKind::TurnStart, serde_json::json!({}));
    let id2 = journal.append(EventKind::TurnEnd, serde_json::json!({}));
    assert!(id1.starts_with("evt_"));
    assert!(id2.starts_with("evt_"));
    assert_ne!(id1, id2);
}

#[test]
fn journal_tracks_commit_points() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({}));
    let commit1 = journal.commit();
    journal.append(EventKind::TurnEnd, serde_json::json!({}));
    let commit2 = journal.commit();
    assert_ne!(commit1, commit2);
}

#[test]
fn journal_replay_from_commit_point() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({}));
    let commit = journal.commit();
    journal.append(EventKind::TurnEnd, serde_json::json!({}));
    let replayed = journal.replay_from(&commit).unwrap();
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].kind, EventKind::TurnEnd);
}
```

- [ ] **Step 2: Implement the EventJournal**

Create `src-tauri/src/event_journal/mod.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventKind {
    TurnStart,
    TurnEnd,
    MessageStart,
    MessageEnd,
    ToolCallStart,
    ToolCallEnd,
    UsageReported,
    ContextCompact,
    JournalCommit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEvent {
    pub id: String,
    pub session_id: String,
    pub kind: EventKind,
    pub data: serde_json::Value,
    pub sequence: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPoint {
    pub session_id: String,
    pub commit_token: String,
    pub stable_event_id: String,
    pub sequence: u64,
}

pub struct EventJournal {
    session_id: String,
    events: Vec<JournalEvent>,
    commit_points: Vec<CommitPoint>,
    sequence: u64,
}

impl EventJournal {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            events: Vec::new(),
            commit_points: Vec::new(),
            sequence: 0,
        }
    }

    pub fn append(&mut self, kind: EventKind, data: serde_json::Value) -> String {
        let id = generate_event_id();
        let event = JournalEvent {
            id: id.clone(),
            session_id: self.session_id.clone(),
            kind,
            data,
            sequence: self.sequence,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.sequence += 1;
        self.events.push(event);
        id
    }

    pub fn commit(&mut self) -> CommitPoint {
        let last_event = self.events.last().expect("cannot commit empty journal");
        let commit = CommitPoint {
            session_id: self.session_id.clone(),
            commit_token: generate_commit_token(),
            stable_event_id: last_event.id.clone(),
            sequence: last_event.sequence,
        };
        self.commit_points.push(commit.clone());
        commit
    }

    pub fn replay_from(&self, commit: &CommitPoint) -> Option<Vec<&JournalEvent>> {
        let idx = self.events.iter().position(|e| e.sequence > commit.sequence)?;
        Some(self.events[idx..].iter().collect())
    }

    pub fn events(&self) -> &[JournalEvent] {
        &self.events
    }
}

fn generate_event_id() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    let base32: Vec<char> = "abcdefghijklmnopqrstuvwxyz234567".chars().collect();
    let mut result = String::from("evt_");
    for b in bytes.iter() {
        result.push(base32[(*b as usize) % 32]);
    }
    result.truncate(30); // "evt_" + 26 chars
    result
}

fn generate_commit_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    format!("cp_{}", hex::encode(&bytes))
}
```

- [ ] **Step 3: Register, run tests, commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml event_journal --locked
git add src-tauri/src/event_journal src-tauri/src/lib.rs
git commit -m "feat: add event journal with stable event IDs and commit points"
```

---

### Task 5: Wire Multi-Session Coordination

**Files:**
- Modify: `src-tauri/src/session_manager.rs`
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: Supervisor from Task 1; ACP transport from Task 2; event journal from Task 4.
- Produces: Multi-session support — Desktop UI sessions mapped to OMP agent sessions with crash recovery.

- [ ] **Step 1: Reactivate the connect path**

In `session_manager.rs`, the `connect_inner` method (around line 2629) currently hits the fail-closed spawn path. Reactivate it to:
1. Use the Supervisor to get or start the sidecar
2. Create an `AcpClient` from the sidecar's stdin/stdout
3. Call `initialize_and_open_session` (already implemented, just dormant)
4. Wire ACP events to the event journal

The key change is in the cold spawn path:
```rust
// Before (Plan 1 fail-closed):
let client = AcpClient::spawn_with_options(spawn_opts);

// After (Plan 3):
let client = state.supervisor.get_or_start_client(&spawn_opts).await;
```

- [ ] **Step 2: Wire event journal to ACP events**

When ACP events arrive (`AcpEvent::Stream`, `AcpEvent::ToolCall`, etc.), append them to the session's event journal. This provides durable event IDs for the v1 protocol's `journal.commit` notification.

- [ ] **Step 3: Update runtime_availability to reflect real state**

When the Supervisor starts the sidecar successfully, update `RuntimeState` to `available: true`. When it crashes, update to `unavailable` and trigger recovery.

- [ ] **Step 4: Run tests, commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
git add src-tauri/src/session_manager.rs src-tauri/src/commands.rs src-tauri/src/runtime_availability.rs
git commit -m "feat: wire multi-session coordination with Supervisor and event journal"
```

---

### Task 6: Update Provenance and Run Full Verification

**Files:**
- Modify: `provenance/omp-patches.json`
- Create: `docs/superpowers/verification/2026-07-29-plan-3-supervisor-core-acp.md`

- [ ] **Step 1: Commit OMP submodule changes**

```bash
cd runtime/oh-my-pi
git add packages/coding-agent/src/modes/acp/desktop-v1/normalizers.ts packages/coding-agent/src/modes/acp/acp-agent.ts
git commit -m "feat: wire v1 handlers to real OMP resources (Plan 3)"
git log --oneline -1  # Record SHA
```

- [ ] **Step 2: Update omp-patches.json**

Add the Plan 3 patch entry.

- [ ] **Step 3: Update submodule pointer**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add runtime/oh-my-pi provenance/omp-patches.json
```

- [ ] **Step 4: Run full verification**

```bash
pnpm check:provenance
pnpm check:brand
pnpm check:legal
cd runtime/oh-my-pi && OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/
pnpm typecheck && pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

- [ ] **Step 5: Write verification record and commit**

```bash
git add docs/superpowers/verification/2026-07-29-plan-3-supervisor-core-acp.md
git commit -m "test: verify Plan 3 Supervisor, core ACP, event journal, multi-session"
```

---

## Plan 3 Completion Boundary

Plan 3 is complete when:
1. The Supervisor can spawn and manage the OMP sidecar process
2. The ACP transport is reactivated (real spawn, initialize, session/prompt)
3. v1 protocol handlers return real data from OMP resources
4. Event journal records durable events with stable IDs and commit points
5. Multi-session coordination maps Desktop UI sessions to OMP agent sessions
6. Crash recovery restores session state from the event journal
7. All policy gates, tests, and verification pass
