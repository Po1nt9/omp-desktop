# OMP Desktop 1.0 Security Audit Checklist

**Created:** 2026-07-31
**Authority:** [Master Design §8/§11/§14/§15/§17](../superpowers/specs/2026-07-28-omp-desktop-design.md) · [Design Spec §2.5](../superpowers/specs/2026-07-31-plan-10-phase-1-acceptance-prep-design.md)
**Referenced by:** [Acceptance Matrix §6/§8/§9](1.0-acceptance-matrix.md)
**Status:** Skeleton — all items `PENDING`. Audit execution is Plan 10 Phase 4 work.

> **Note:** Self-audit is insufficient for 1.0 (Plan 10 external dependency #4). This checklist structures both the preliminary self-audit and the external security auditor's review. Self-audit results are preliminary until external review confirms them.

**Status enum:** `PASS` / `FAIL` / `BLOCKED` / `WAIVED` / `PENDING` (same as acceptance matrix).

---

## SA-C: Credential Storage

Source: master design §8 (credential architecture and migration).

| ID | Check | Method | Status | Notes |
|---|---|---|---|---|
| SA-C.1 | SecretStore uses OS keychain: macOS Keychain, Windows Credential Manager, Linux Secret Service | Code audit: `src-tauri/src/secrets.rs` | PASS (2026-07-31) | `secrets/store.rs` `KeychainStore` wraps the `keyring` crate (macOS Keychain / Windows Credential Manager / Linux Secret Service) behind the `SecretStore` trait; `default_store()` is the process-wide handle with `install_test_store`/`MockStore` fault injection for tests. Keychain is the only credential backend (strict mode; on/off toggle retired). |
| SA-C.2 | `agent.db` stores only `keychain:v1:<opaque-id>` references, never plaintext secrets | Code audit + DB inspection | PENDING | |
| SA-C.3 | System secure storage unavailable → blocks save/refresh with actionable error; no silent plaintext fallback | Code audit + manual test (disable keychain) | PASS (2026-07-31) | Strict mode (§8.1): `save_secrets` / `remote_im::config::save_instance` return `StoreError` carrying i18n key `credentials.storeUnavailable` (actionable: start gnome-keyring/Secret Service + restart; existing credentials untouched) and write nothing to disk. `load_secrets`/`get_secrets` fail closed (None) once the migration ledger `store_unavailable` flag is set; plaintext pass-through only while the store is healthy. No plaintext fallback or reverse-copy path remains. Tests: `store_unavailable_blocks_save_and_fails_load_closed`, remote_im outage test (443 lib tests green). Manual disable-keychain run on Linux still owed before final sign-off. |
| SA-C.4 | Remote platform credentials use isolated namespace, ACL, and metadata type (separate from Agent Provider credentials) | Code audit: `src-tauri/src/secrets.rs` remote namespace | PENDING | |
| SA-C.5 | Migration 6-step idempotency: dry run → copy → readback (constant-time compare) → reference commit → tombstone → cleanup | `cargo test` migration suite | PASS (2026-07-31) | `secrets/migration.rs` `Migrator` implements all 6 steps against the `MigrationSource` trait; 16-test suite covers every §18.2.6 scenario: dry-run enumerate/skip/conflict, success, idempotent rerun, store-unavailable deferral, readback-failure rollback, cleanup-failure tombstone+retry (`__tombstoned_v1__`), reference-commit rollback. Startup integration test: both sources migrate on launch, second run no-op. |
| SA-C.6 | Neither Desktop nor CLI defines its own keychain service/account naming; all go through unified Rust SecretStore helper | Code audit | PENDING | |
| SA-C.7 | Release artifacts contain no plaintext secret fallback | Artifact inspection: grep built binaries for secret patterns | PENDING | |
| SA-C.8 | auth-broker is the sole writer for OAuth refresh (Desktop + CLI shared) | Code audit | PENDING | |
| SA-C.9 | `CredentialIndexStore` manages only metadata, Provider/account status, and opaque references — no secret access | Code audit | PENDING | |

---

## SA-P: Permission Model

Source: master design §11 (permission and security boundary).

| ID | Check | Method | Status | Notes |
|---|---|---|---|---|
| SA-P.1 | Per-path decision table enforced: bash (tool tier + policy + cwd/shell containment), edit (policy + canonical target path gate), delete (independent high-risk gate + canonical path), move (source + destination separately canonicalized/gated), elicitation (schema-allowed values only), plan approval (active plan/turn ID gate), subagent (parent policy + explicit inheritance/narrowing) | Code audit + contract tests | PENDING | Subagent component resolved 2026-07-31 (see SA-P.7); other paths pending. |
| SA-P.2 | Fail-closed on missing capability: no tool execution without approval; missing Runtime capability → read-only or disabled | Test suite + code audit | PENDING | |
| SA-P.3 | Request binding: every permission request bound to runtime instance + session + turn + request ID | Code audit | PENDING | |
| SA-P.4 | Timeout / restart / turn-end invalidates pending requests | Code audit + test | PENDING | |
| SA-P.5 | "First legal decision wins" applies only to the same pending request; other requests unaffected | Code audit | PENDING | |
| SA-P.6 | Process-level override (single sidecar) not disguised as per-session setting in UI | UI audit | PENDING | |
| SA-P.7 | Subagent cannot escalate beyond parent policy; MCP/workspace constraints inherited | Code audit + test | PASS | Desktop side (2026-07-31, AC-1.5): `permission::subagent_effective_policy` clamp — subagent ceiling never wider than the session's effective policy, proven by an exhaustive 7×8 matrix test; clamped policy reaches the Runtime via `[subagents] policy/inherit_mcp/inherit_workspace = true` TOML (independent mode) and `OMP_SUBAGENT_POLICY` env (both modes); host gate `subagent_spawn_gate_denies` rejects subagent-spawn permission requests when subagents are disabled even under AlwaysApprove (host test). Runtime-side honoring of the declared constraints remains Runtime responsibility — real-Runtime E2E pending. |
| SA-P.8 | Delete gate independent from edit/move approval — no inheritance | Code audit | PENDING | |
| SA-P.9 | Move: either source or destination out-of-bounds → reject | Code audit | PENDING | |
| SA-P.10 | `allow_always` / `reject_always` treated as managed-session memory scope only; UI does not describe them as cross-restart persistent rules (until versioned policy API negotiated) | UI audit + code audit | PENDING | |

---

## SA-R: Remote Access

Source: master design §14 (Remote Hub and channels).

| ID | Check | Method | Status | Notes |
|---|---|---|---|---|
| SA-R.1 | Channel credentials stored in SecretStore remote namespace (isolated from Agent Provider credentials) | Code audit | PASS (2026-07-31) | Channel secrets live under `NS_REMOTE = "remote"` with keys `<instance_id>:<field>`; provider keys under `NS_PROVIDER = "provider"` — no shared namespace. Disk holds only `keychain:v1:remote:<key>` references in `channel-secret-refs.json`; legacy `channel-secrets.json` is migrated at startup and securely deleted. `remote_im/config.rs` save/get/delete all route through `SecretStore`; 4 tests incl. refs-win dual-read, fail-closed outage, delete sweep. |
| SA-R.2 | All channels default-off | Config audit | PENDING | |
| SA-R.3 | Webhook default loopback; public ingress requires explicit user reverse-proxy/tunnel configuration | Code audit | PENDING | |
| SA-R.4 | Identity whitelist enforcement per channel | `cargo test` remote_im suite (59 tests) | PENDING | |
| SA-R.5 | Approval expiry + anti-replay | `cargo test` remote_im suite | PASS | 2026-07-31: `replay_guard.rs` ±300s freshness window + per-channel nonce cache (7 tests); engine gate drops Stale/Replayed before dedup (3 tests); TTL-bound in-memory approval (`DEFAULT_APPROVAL_TTL_SECS=3600`, never persisted) wired to `SpawnOptions.permission_policy="yolo"` (4 tests); bridge grant/revoke + status DTO (1 test). |
| SA-R.6 | Dedup: SQLite `seen_messages` table, `(channel,message_id)` composite PK, `INSERT OR IGNORE` atomic, 7-day TTL (cleanup every 1024 inserts), `dedup.sqlite` persistence | `cargo test` dedup_store suite | PENDING | |
| SA-R.7 | Rate limiting: fixed window, per-channel 60/min + per-scope 10/min, in-memory, lazy window reset | `cargo test` rate_limiter suite | PENDING | |
| SA-R.8 | Revocation handling per channel | `cargo test` remote_im suite | PENDING | |
| SA-R.9 | Log sanitization in remote layer (no prompt/reply/token in logs) | Code audit | PENDING | |
| SA-R.10 | Enterprise WeChat: default listen loopback (not `0.0.0.0`); reject unsigned/undecryptable requests; port conflict, restart occupation, reverse proxy source, key rotation tests pass | Code audit + `cargo test` protocol_start_tests | PENDING | |
| SA-R.11 | Remote approval does not require extra PIN; documentation recommends platform MFA + strict user whitelist + least privilege | Doc review | PENDING | |
| SA-R.12 | Channels do protocol translation only; do not directly manage OMP sessions | Code audit | PENDING | |
| SA-R.13 | Test builds with dangerous auto-permission mode: prominent "no sandbox, may access resources outside authorized directories" warning per occurrence; default off; does not claim security isolation | Code audit + UI audit | PENDING | |

---

## SA-L: Log Sanitization

Source: master design §15 (errors, diagnostics, and observability).

| ID | Check | Method | Status | Notes |
|---|---|---|---|---|
| SA-L.1 | Default logs do not contain prompt, reply, file content, or full tool output | Code audit across all 5 log layers (desktop, runtime, protocol, remote, updater) | PENDING | |
| SA-L.2 | key/token/cookie/header/suspected-secret sanitization in all log layers | Code audit | PENDING | |
| SA-L.3 | Crash reports saved locally by default; external submission requires user preview and confirmation | Code audit + UI audit | PENDING | |
| SA-L.4 | Diagnostics page self-check is read-only: no model requests, no project modifications | Code audit + test | PENDING | |
| SA-L.5 | Support bundle excludes secrets | `cargo test` support_bundle suite (2 tests: session_bundle_includes_messages_without_secrets, support_bundle_creates_zip_without_secrets) | PENDING | |
| SA-L.6 | Safe mode triggers correctly: sidecar missing/integrity failure, protocol incompatible, consecutive crashes, active directory unresolvable, credential/config migration failure | Code audit + manual test | PENDING | |
| SA-L.7 | Safe mode capabilities correctly scoped: allows viewing projections, exporting diagnostics, restoring backups, external OMP read-only diagnostics; prohibits Agent, tools, and shared config writes | Code audit | PENDING | |

---

## SA-B: Brand & Legal

Source: master design §17 (license and brand assets).

| ID | Check | Method | Status | Notes |
|---|---|---|---|---|
| SA-B.1 | MIT license preservation: RongleCat Grok App MIT notice + Mario Zechner and Can Bölük OMP/Pi MIT notice retained | `node scripts/check-legal-baseline.mjs` | PENDING | |
| SA-B.2 | OMP per-directory NOTICE files preserved | `node scripts/check-legal-baseline.mjs` | PENDING | |
| SA-B.3 | Silver font CC BY 4.0 attribution present | `node scripts/check-legal-baseline.mjs` | PENDING | |
| SA-B.4 | Highlight.js BSD-3-Clause attribution present | `node scripts/check-legal-baseline.mjs` | PENDING | |
| SA-B.5 | THIRD_PARTY_NOTICES in installer packages (all npm, Cargo, native, and resource dependency licenses) | Artifact inspection | PENDING | |
| SA-B.6 | In-app "About → Open Source Licenses" accessible | UI audit | PENDING | |
| SA-B.7 | No Grok/xAI name, icon, bundle identifier, or assets implying official affiliation | `node scripts/check-brand-policy.mjs` (9 tests) | PENDING | |
| SA-B.8 | Brand scan allowlist explicitly scoped to OMP catalog/runtime sources: xAI Provider, Grok model names, Provider endpoints, auth methods, sanitized original Provider errors; allowlist entries retain real names and pass functional regression | Code audit of scan config + functional regression | PENDING | |
| SA-B.9 | Provenance check passes | `node scripts/check-provenance.mjs` | PENDING | |

---

## Audit Execution Notes

### Self-Audit Scope (Phase 2–3, internal)

- Code audit items: review referenced source files, record findings in Notes column.
- Test items: run referenced test suites, record output as Evidence.
- Manual/UI audit items: follow described procedure, record screenshots or observations.
- Artifact inspection items: download release artifacts from latest nightly, inspect.

### External Auditor Scope (Phase 4, Plan 10 dependency #4)

- All SA-C and SA-P items require external confirmation.
- SA-R items involving cryptographic operations (HMAC, key rotation, anti-replay) require external review.
- SA-L sanitization items require external sampling verification.
- Self-audit results are **preliminary** until external auditor signs off.

### Cross-References

- Credential migration details → [Acceptance Matrix §6](1.0-acceptance-matrix.md#6-credential-migration-all-pass)
- Channel acceptance details → [Acceptance Matrix §8](1.0-acceptance-matrix.md#8-remote-channel-acceptance)
- Workspace/containment details → [Acceptance Matrix §9](1.0-acceptance-matrix.md#9-workspace-routing--tool-containment)
- Signing requirements → [signing-requirements.md](signing-requirements.md)
