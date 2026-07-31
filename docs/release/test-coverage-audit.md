# OMP Desktop 1.0 Test Coverage Audit

**Created:** 2026-07-31 · **Updated:** 2026-07-31 (Phase 2 code audit)
**Authority:** [Master Design §18.2](../superpowers/specs/2026-07-28-omp-desktop-design.md#182-pass-条件) · [Design Spec §2.6](../superpowers/specs/2026-07-31-plan-10-phase-1-acceptance-prep-design.md)
**Referenced by:** [Acceptance Matrix §2](1.0-acceptance-matrix.md#2-release-blocking-tests-zero-failures)
**Data as of:** 2026-07-31 (commit `d9cf636`)

---

## 1. Suite Inventory

| Suite | Count | How to Run | Coverage Scope |
|---|---|---|---|
| Frontend (vitest) | 94 files / 840 tests (840 pass) | `pnpm test` | React components, Tauri command mocks, fail-closed behavior, UI logic, hooks, i18n rendering, code language detection, end-of-turn detection, project path handling, voice audio, redaction, runtime availability, file preview. +5 AC-1.5 transport-seam round-trip tests (todo.list five-state, subagents.status/setEnabled, rejection→runtime_unavailable, fail-closed without transport) (2026-07-31 evening). |
| Rust (cargo test) | 498 tests (498 pass + 1 ignored) | `cargo test --manifest-path src-tauri/Cargo.toml` | See module breakdown below. +3 wecom signature tests, +1 permission delete test added in Phase 2; +2 v1 mock-transport contract tests added with transport injection (2026-07-31); +18 §8.2 credential migration/strict-mode tests (16 migration incl. startup integration, 2 strict secrets, 4 remote_im config, −3 retired plaintext-fallback tests, −1 net remote_im dedup) (2026-07-31 evening); +10 event-journal recovery tests (assess/close/idempotent marker/corrupt quarantine/write-ahead, AC-1.10) (2026-07-31 evening); +15 AC-8.4 tests (7 ReplayGuard freshness/nonce, 3 engine anti-replay gate, 4 approval TTL/policy, 1 runtime handle accessor) (2026-07-31 evening); +14 AC-1.5 tests (4 subagent-policy clamp, 1 store default, 5 agent_subagents TOML/gate/env, 2 spawn wiring, 1 LiveSession snapshot, 1 host gate) (2026-07-31 evening); +8 AC-1.13 trace-correlation tests (5 mechanism, 3 wiring) (2026-07-31 evening); +8 AC-2.9/AC-7.1 E2E tests (5 AcpClient-level incl. crash injection, 1 engine full-turn, 2 env-gated real-Runtime) (2026-07-31 evening). |
| Brand policy | 9 tests | `node scripts/check-brand-policy.mjs` (+ `.test.mjs`) | Brand scan rules: lowercase brand detection, Grok/xAI coupling residue, Runtime brand normalization, allowlist scoping. Code-span exemption added for `omp` CLI binary names. |
| i18n completeness | 5 tests | `node scripts/check-i18n-completeness.mjs` (+ `.test.mjs`) | Three-locale (en/zh-CN/zh-TW) key coverage, ICU parameter/type validation, empty value detection, hardcoded string detection |
| Legal baseline | tests | `node scripts/check-legal-baseline.mjs` (+ `.test.mjs`) | MIT license attribution (RongleCat, OMP/Pi authors), NOTICE files, font/font-highlight.js attributions |
| Provenance | 17 tests | `node scripts/check-provenance.mjs` (+ `.test.mjs`) | Fork provenance: Grok App upstream attribution, xAI asset removal verification. Patch ledger SHAs corrected + cursor-provider-removal patch recorded. |

### Rust Module Breakdown (top modules by test count)

| Module | Tests | Scope |
|---|---|---|
| `remote_im` | 84 | Channel adapters, dedup store, rate limiter, replay guard, approval TTL, protocol start, weixin flow, catalog, media, bridge, session, config (incl. 4 SecretStore credential tests), validation |
| `commands` | 24 | Tauri command handlers |
| `acp_client` | 21 | ACP protocol client, prompt blocks |
| `permission` | 20 | Permission model, per-path gates |
| `fs_browser` | 19 | Filesystem browser, path disambiguation |
| `store` | 17 | Session/project store, CRUD, sorting |
| `session_manager` | 14 | Session lifecycle, turn management |
| `process_limits` | 13 | Process resource limits |
| `extensions` | 13 | Extension protocol v1 |
| `mirror` | 12 | Auth mirror |
| `stream_stall` | 11 | Stream stall detection |
| `secrets` | 29 | SecretStore (Keychain/Credential Manager/Secret Service) + MockStore fault injection, 6-step §8.2 migration engine (16 tests incl. startup integration), strict-mode save/load |
| `event_journal` | 21 | Event journal persistence, commit points, standard path format + recovery (assess/close/marker/quarantine/write-ahead) |
| `acp_golden_test` | 9 | ACP golden file contract tests |
| `permission_rules` | 8 | Permission rule evaluation |
| `journal_throttle` | 8 | Journal write throttling |
| Others (25+ modules) | ~100 | voice_stt, hooks, session_content_search, proxy, app_update, supervisor, session_title, permission_host_test, agents_catalog, support_bundle, path_scope, store_lock, portability, slash, projects, pb_frame, app_sessions, feishu_reg, weixin_reg, control_plane, runtime, types, validate, config |

**Environment note:** `store::tests::ensure_general_project_is_idempotent_and_not_removable` fails under filesystem sandboxing (sandbox blocks app data directory creation) but passes in CI (3 platforms, latest run 2026-07-30 green) and unsandboxed local runs. Not a product bug.

---

## 2. Pass-Condition Mapping

| ID | Pass Condition | Automated Coverage | Manual Required | Gap |
|---|---|---|---|---|
| TC-M.1 | §1 Capability baseline 100% alignment | `acp_client` (21), `extensions` (13), `acp_golden_test` (9), `permission` (20) — contract tests for protocol methods | Capability negotiation with real bundled Runtime; product entry + documentation alignment review | Handshake leg automated 2026-07-31 (`e2e_real_handshake_capabilities` vs omp 17.1.3: protocolVersion=1 + agentCapabilities); remaining: v1 method semantics legs (blocked Runtime-side per `e2e_real_v1_method_probe`) + no automated doc-alignment check |
| TC-M.2 | §2 Release-blocking tests zero failures | vitest 840 + cargo 498 + 4 check scripts — all green | E2E happy-path, packaging, updater verification | Mock E2E happy-path resolved 2026-07-31 (AC-2.9 PASS); packaging/updater still manual |
| TC-M.3 | §3 Locale coverage 100% + ICU zero errors | `check-i18n-completeness.mjs` (5 tests) — key coverage + ICU validation | Critical flow screenshots in 3 locales | No screenshot automation |
| TC-M.4 | §4 Brand scan zero violations | `check-brand-policy.mjs` (9 tests) — scan + allowlist | Allowlist functional regression (xAI Provider, Grok models display correctly) | — |
| TC-M.5 | §5 Active Directory discovery CLI parity | — | Desktop vs CLI discovery comparison across 3 OS | No automated parity test |
| TC-M.6 | §6 Credential migration all-pass | `secrets` (29, incl. 16-test migration suite covering all §18.2.6 scenarios + startup integration), `mirror` (12) — keychain roundtrip, migration steps, strict-mode fail-closed | Artifact plaintext scan (AC-6.8) | AC-6.4 store-unavailable now automated via MockStore fault injection; AC-6.8 still needs built-binary grep |
| TC-M.7 | §7 Crash recovery no auto-replay | `event_journal` (21, incl. 10 recovery tests: assess/close/idempotent marker/corrupt quarantine/write-ahead), `journal_throttle` (8), `e2e_runtime` crash-injection tests (2) — journal persistence, commit points, recovery wiring, asserted no-replay | Manual crash injection: kill sidecar mid-turn → restart → verify no replay | Automated crash injection green 2026-07-31 (AC-7.1 PASS); manual restart leg still available for acceptance run |
| TC-M.8 | §8 Remote channel acceptance | `remote_im` (84) — dedup, rate limiter, replay guard, approval TTL, protocol start, weixin flow, catalog, media, bridge, config, validation | Real platform smoke tests on ≥1 OS per channel | No real-platform smoke automation; applicability items (edit/button/group/channel/thread) mostly manual |
| TC-M.9 | §9 Workspace routing + tool containment | `path_scope`, `fs_browser` (19) — path canonicalization, scope enforcement | OMP canonical path + shell containment + MCP/subagent inheritance verification | Depends on Runtime capabilities; no automated containment test |
| TC-M.10 | §10 Install/sign/upgrade/rollback/SBOM | release.yml CI (4 targets build + SHA256SUMS + updater manifest) | Manual install/upgrade/rollback per OS (5 platform variants) | No automated install/upgrade test |
| TC-M.11 | §11 External OMP diagnostics-only + update channels | — | Manual verification: external OMP read-only; channel isolation | No automated test |
| TC-M.12 | §12 Documentation coverage | Check scripts verify link integrity | Manual content review (10 doc areas) | — |

---

## 3. Gap Analysis

### Release-Blocking Gaps

These gaps correspond to acceptance matrix items that cannot reach `PASS` without new tests or manual execution:

| Gap | Matrix Items | Priority | Notes |
|---|---|---|---|
| ~~**v1 transport injection**~~ | AC-1.2/1.3/1.4/1.8/1.9, AC-5.1/5.2 | ~~Critical~~ **Resolved 2026-07-31** | `OmpExtension::request()` now dispatches through an injected `V1Transport` trait object (`omp_desktop_v1/transport.rs`); `AcpClient` implements it via generic JSON-RPC forwarding, and the session manager installs it on capability negotiation. Mock-transport contract tests verify live dispatch + error→`runtime_unavailable` mapping. Remaining work on these items is real-Runtime E2E, not plumbing. |
| ~~**Mock E2E happy-path test**~~ | AC-2.9 | ~~**High**~~ **Resolved 2026-07-31** | Scriptable `mock_acp_runtime` bin (stdio JSON-RPC, replies from golden fixtures, scenario prefix per prompt) per `docs/superpowers/plans/2026-07-31-mock-runtime-e2e.md`: `e2e_mock_happy_path_turn` (create → prompt → response → turn end), `e2e_mock_permission_gate_turn` (permission approval leg), `e2e_mock_tool_call_lifecycle` (tool exec leg), `engine_full_turn_against_mock_runtime` (Hub leg). |
| End-to-end capability negotiation test | AC-1.1–AC-1.13 | **High → Partially resolved 2026-07-31** | Handshake leg done: `e2e_real_handshake_capabilities` (env-gated) negotiated protocolVersion=1 + agentCapabilities against live omp 17.1.3. Remaining gap: v1 method semantics (AC-1.2/1.3/1.8 etc.) — `e2e_real_v1_method_probe` shows the installed Runtime answers none of `diagnostics.selfCheck`/`providers.list`/`sessionConfig.get` (-32603 Unknown ACP ext method), so those legs stay blocked on the Runtime side. |
| ~~**6-step credential migration suite**~~ | AC-6.1/6.2/6.5/6.6/6.7, AC-2.10 | ~~**High**~~ **Resolved 2026-07-31** | §8.2 implemented per `docs/superpowers/plans/2026-07-31-credential-migration.md`: `SecretStore` trait + generic `Migrator` (dry-run/copy/readback/reference-commit/cleanup/rollback-tombstone) + `SecretsJsonSource`/`ChannelSecretsSource` adapters + idempotent startup auto-migration. 16-test migration suite green; strict mode removed the plaintext fallback (AC-6.4) and the keychain toggle. |
| ~~**Subagent policy inheritance**~~ | AC-1.5 | ~~**High**~~ **Resolved 2026-07-31** | Three-layer Desktop enforcement per `docs/superpowers/plans/2026-07-31-subagent-policy-inheritance.md`: `subagent_effective_policy` clamp (never wider than parent, exhaustive matrix), `[subagents] policy/inherit_mcp/inherit_workspace` TOML sync (independent mode) + `OMP_SUBAGENT_POLICY` env (both modes) + `--no-subagents`/`GROK_SUBAGENTS=0` spawn wiring (previously dead code — kill switch now real), host gate denying subagent-spawn permission even under yolo. Todo lifecycle covered by the v1 transport-seam round-trip tests (five-state todo.list + subagents.status/setEnabled). |
| ~~**Host+Hub trace correlation**~~ | AC-1.13 | ~~**High**~~ **Resolved 2026-07-31** | tracing span-field `trace_id` + `Instrument` per `docs/superpowers/plans/2026-07-31-trace-correlation.md`: birth at remote_im pump recv (per message, both quick/detached branches) and `send_message` (per turn); engine event collector inherits via `Span::current()`. 8 contract tests (5 mechanism in `trace.rs`, 3 wiring — incl. process-wide capture layer for shared-callsite stability). End-to-end Runtime propagation optional per design §13. |
| ~~**Event-journal recovery wiring**~~ | AC-1.10 | ~~**High**~~ **Resolved 2026-07-31** | Recovery wired into session connect per `docs/superpowers/plans/2026-07-31-event-journal-recovery.md`: TurnStart write-ahead (durable dangling boundary on crash), `recover_session_journal` at connect (load → `replay_from` assess → honest close → save), idempotent `turn_interrupted` marker, journal continuity across restarts. 10 recovery tests green. |
| ~~**Automated crash recovery test**~~ | AC-7.1 | ~~**High**~~ **Resolved 2026-07-31** | `e2e_crash_mid_turn_fails_pending_without_replay` (kill spawned sidecar mid-hang → pending fails, ProcessExited, exactly one session/prompt on the wire) + `e2e_crash_journal_marks_interrupted_no_replay` (dangling TurnStart → recover closes as `turn_interrupted`, idempotent). No-replay now asserted, not absence-of-code. |
| ~~**Remote approval expiry + anti-replay**~~ | AC-8.4 | ~~**High**~~ **Resolved 2026-07-31** | ReplayGuard (Slack-standard ±300s freshness window + per-channel nonce cache, 7 tests) gated before dedup in `Engine::handle` (3 tests); wecom populates query timestamp+nonce, LINE derives `sig:replyToken` nonce; TTL-bound in-memory approval (3600s, never persisted, dies on restart) wired to `SpawnOptions.permission_policy="yolo"` (4 tests); bridge grant/revoke + status DTO (1 test). |
| **Single update channel** | AC-10.9 | **Medium** | Only silent/github_manual exist; no stable/beta/nightly isolation. |
| Discovery parity test | AC-5.1 | **Medium** | Compare Desktop discovery API output vs CLI `omp` discovery for same env vars/profile/cwd. (Executable now that the v1 transport is live.) |

### Important but Manual-Acceptable

These items are feasible as manual acceptance procedures (no automation required for 1.0):

| Item | Matrix Items | Notes |
|---|---|---|
| Critical flow locale screenshots | AC-3.5 | 10 flows × 3 locales = 30 screenshots. Manual capture during cross-platform acceptance (Phase 3). |
| Real-platform channel smoke tests | AC-8.2 | 10 channels × inbound/outbound. Requires real platform accounts. Run on ≥1 OS; remaining OS use protocol simulation (already covered by remote_im tests). |
| Install/upgrade/rollback per OS | AC-10.1–AC-10.5 | 5 platform variants × 4 operations. Manual during cross-platform acceptance (Phase 3). |
| External OMP diagnostics-only | AC-11.1–AC-11.2 | UI audit + code audit. |

### Security Issues Found in Phase 2 Audit

Code audit (SA-C/SA-P/SA-R/SA-L domains) found issues beyond test coverage:

**Fixed in this pass:**

| Issue | Severity | Fix |
|---|---|---|
| WeCom webhook `0.0.0.0` + no signature check | HIGH | loopback default + `allow_external` + `msg_signature` (SHA1) validation (`wecom.rs`) |
| Remote IM log leaked prompt preview + feishu app_id | MEDIUM | dropped preview (content_len only) + dropped app_id (`runtime.rs`, `feishu.rs`) |
| `delete_file` auto-approved under `AcceptEdits` | MEDIUM | carved delete out of AcceptEdits auto-approval (`permission.rs`) |

**Remaining (architectural, need migration plans):**

None open. Both credential gaps below were resolved 2026-07-31 by the §8.2
migration plan — kept for the record:

| Issue | Severity | Matrix Items | Resolution |
|---|---|---|---|
| ~~Remote channel credentials plaintext in `channel-secrets.json`~~ | HIGH | SA-R.1 / AC-6.2 | Migrated to SecretStore `remote` namespace at startup; file securely deleted; disk holds references only. |
| ~~Silent plaintext fallback when OS secure storage unavailable~~ | HIGH | SA-C.3 / AC-6.4 | Strict mode: blocked save with actionable i18n error + fail-closed load; no fallback path remains. |

> Historical note: AC-6.4 was previously listed as "Manual-Acceptable" (verify actionable error). Phase 2 code audit confirmed it as a real FAIL (`save_secrets` silently wrote plaintext when keychain was off/unavailable). Resolved 2026-07-31: strict mode + MockStore fault-injection tests made it automated, not manual.

### Known Code Quality Items (from Plan 7 Final Review)

Deferred items noted during Plan 7 merge, not release-blocking but should be tracked:

| Item | Location | Notes |
|---|---|---|
| `AgentTurnResult` stale comments | `src-tauri/src/remote_im/` | Comments describe outdated behavior; cosmetic but misleading. |
| `runtimes` / `in_flight` / `spawn_locks` maps unbounded growth | `src-tauri/src/remote_im/runtime.rs` | No eviction policy for per-`work_dir` maps. Long-running instances with many distinct work dirs could leak memory. Needs TTL or LRU eviction. |

### Coverage Strengths

Areas with strong automated coverage (no gaps identified):

- **Permission model**: 20 permission + 8 permission_rules + 5 permission_host_test = 33 tests covering per-path gates, fail-closed, request binding.
- **Remote IM infrastructure**: 69 tests covering dedup, rate limiting, protocol start, channel flow, media, config validation, SecretStore credential refs.
- **Event journal**: 11 tests covering persistence, commit points, path format, throttle.
- **Secrets**: 29 tests covering keychain roundtrip, MockStore fault injection, 6-step migration engine (dry-run/readback/tombstone/rollback/idempotency/startup), strict-mode fail-closed.
- **Brand/legal/provenance**: 4 check scripts with dedicated test files, run in CI.
- **Frontend**: 831 tests across 94 files — comprehensive component and hook coverage.
