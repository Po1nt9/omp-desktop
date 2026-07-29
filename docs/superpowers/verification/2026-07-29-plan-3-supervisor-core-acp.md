# Plan 3 Supervisor, Core ACP, Event Journal, Multi-Session Verification

- OMP submodule base: 667111575ebba136dadfd6989379e7f67e0d40d9
- OMP submodule patched: 238d64a2432f8f4b5acc3c0c4636e617106c6363
- Patch branch: desktop-v1-protocol
- Supervisor: implemented (spawn, monitor, stop)
- ACP transport: reactivated (real spawn with pipe wiring)
- v1 handlers: wired to real OMP resources (sessions, usage, extensions, providers, sessionConfig)
- v1 handlers stubbed: mcp, diagnostics (Plan 4)
- Event journal: implemented (stable event IDs, commit points, replay)
- Multi-session: wired (Supervisor → AcpClient → SessionManager)
- Runtime availability: dynamic (updated on spawn/crash)
- Brand policy: zero violations
- Provenance policy: passed
- Legal/SBOM input policy: passed
- OMP submodule tests: passed (92 desktop-v1 + 54 acp-agent)
- Frontend typecheck/tests: passed
- Rust tests: passed (380+)

## Verification environment

- Platform: macOS (Darwin)
- Date: 2026-07-29

## Step 1: Submodule pointer and provenance

- `runtime/oh-my-pi` submodule HEAD: `238d64a2432f8f4b5acc3c0c4636e617106c6363` (Plan 3 Task 3 commit)
- `provenance/omp-patches.json`: added `plan-3-v1-handler-wiring` patch entry on top of the Plan 2 `desktop-v1-protocol` patch
- Superproject gitlink matches the new patched commit
- Submodule HEAD matches the gitlink
- `cd runtime/oh-my-pi && git log --oneline -1` → `238d64a24 feat: wire v1 handlers to real OMP resources (Plan 3)`

## Step 2: Custom policy gates

- `pnpm check:provenance`: passed — frozen records, remotes, gitlink, and submodule checkout match; publication verified
- `pnpm check:brand`: passed — zero violations
- `pnpm check:legal`: passed — inventory, policy, and notice coverage verified

## Step 3: OMP submodule tests

- `OMP_DESKTOP_V1_PROTOCOL=1 bun test packages/coding-agent/test/desktop-v1/`: 92 tests passed, 0 failed (12 files)
- `bun test packages/coding-agent/test/acp-agent.test.ts`: 54 tests passed, 0 failed

## Step 4: Frontend verification

- `pnpm typecheck`: zero TypeScript errors
- `pnpm test`: 94 test files, 829 tests passed, 0 failed

## Step 5: Rust verification

- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 381 tests passed, 0 failed, 1 ignored

## Step 6: Plan 3 task coverage

### Task 1 — Supervisor process manager

- `src-tauri/src/supervisor/mod.rs`: `Supervisor` with `start`, `is_running`, `stop`
- `SupervisorConfig` defaults: `max_restarts=3`, `restart_delay_ms=1000`, `health_check_interval_ms=5000`, env always includes `OMP_DESKTOP_V1_PROTOCOL=1`
- `supervisor_returns_unavailable_when_binary_not_found` test passes
- `supervisor_config_has_sensible_defaults` test passes

### Task 2 — ACP transport reactivation

- `src-tauri/src/acp_client.rs`: `spawn_with_options` consumes `binary_path` and `agent_dir`; falls back to `runtime_unavailable` when no binary is configured (Plan 1 fail-closed preserved)
- `from_child` wires stdin/stdout/stderr to the existing JSON-RPC framing
- `connect_tcp` remains fail-closed (API mode reserved for Plan 3+)
- `SpawnOptions` carries `binary_path: Option<PathBuf>` and `agent_dir: Option<PathBuf>`
- `runtime_availability.rs` is dynamic: `set_runtime_available(available, reason)` updates an `AtomicBool` + `RwLock<String>`; `runtime_availability` Tauri command returns the live state

### Task 3 — v1 handlers wired to real OMP resources

- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/normalizers.ts`: session ID mapping (`toV1SessionId` / `fromV1SessionId`), `normalizeSessionInfo`, `normalizeUsageReport`, `normalizeExtension`, `normalizeProvider`, `normalizeModel`
- `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/acp-agent.ts`: `buildDesktopV1HandlerDeps` wires real OMP `SessionManager`, `AuthStorage`, `ModelManager`, extension loader, MCP, sessionConfig, diagnostics
- mcp and diagnostics handlers remain stubbed (Plan 4 dependency)

### Task 4 — Event journal

- `src-tauri/src/event_journal/mod.rs`: `EventJournal` with `append`, `commit`, `replay_from`, `events`
- `EventKind` covers turn boundaries, message/tool/usage/compact events, and a `JournalCommit` marker
- `JournalEvent` carries stable `evt_<base32>` IDs (30 chars) and an `sequence` counter
- `CommitPoint` carries `cp_<hex>` tokens anchored on the last event's stable ID
- `replay_from(commit)` returns the tail of events strictly after the commit's sequence

### Task 5 — Multi-session coordination

- `src-tauri/src/session_manager.rs::connect_inner` cold spawn path now:
  1. Discovers the OMP binary via `AppSettings::manual_cli_path` (must exist on disk)
  2. Injects `agent_dir` from `agent_grok_home(session_data_mode)` so the sidecar shares the host's auth/profile
  3. Calls `AcpClient::spawn_with_options(opts)` (reactivated in Task 2)
  4. On spawn success, calls `set_runtime_available(true, "omp_runtime")`
  5. On spawn failure, preserves the Plan 1 fail-closed behavior (`connect_failed`)
- `LiveSession` carries `event_journal: Option<EventJournal>`, attached after `initialize_and_open_session` succeeds
- `EventKind::TurnStart` is appended when a prompt is dispatched (`send_message` path)
- `EventKind::TurnEnd` plus `journal.commit()` are appended on the authoritative `PromptComplete` (live session only)
- `set_runtime_available(false, "runtime_crashed")` is called from `AcpEvent::ProcessExited`
- The event journal is left attached after a crash so a future replay pass can read the tail
- Fail-closed behavior is preserved when `manual_cli_path` is unset or missing on disk

### Task 6 — Provenance and verification

- Submodule pointer committed at `238d64a2432f8f4b5acc3c0c4636e617106c6363`
- `provenance/omp-patches.json` records the `plan-3-v1-handler-wiring` patch with the full 40-char SHA
- All policy gates, OMP submodule tests, frontend typecheck/tests, and Rust tests pass

## Plan 3 completion

Plan 3 is complete. The Supervisor manages the OMP sidecar lifecycle, the ACP transport performs real `initialize` and `session/new` / `session/load`, the v1 protocol handlers return real data from OMP resources (sessions, usage, extensions, providers, sessionConfig), the event journal records durable turn boundaries with stable IDs and commit points, and multi-session coordination maps Desktop UI sessions to OMP agent sessions via the existing `SessionManager` infrastructure. Runtime availability is dynamic and reflects the live sidecar state. The fail-closed behavior is preserved for environments without a configured OMP binary. Crash recovery via event-journal replay and the remaining v1 handler wiring (mcp, diagnostics) are deferred to Plan 4.
