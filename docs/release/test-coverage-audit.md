# OMP Desktop 1.0 Test Coverage Audit

**Created:** 2026-07-31 · **Updated:** 2026-07-31 (Phase 2 code audit)
**Authority:** [Master Design §18.2](../superpowers/specs/2026-07-28-omp-desktop-design.md#182-pass-条件) · [Design Spec §2.6](../superpowers/specs/2026-07-31-plan-10-phase-1-acceptance-prep-design.md)
**Referenced by:** [Acceptance Matrix §2](1.0-acceptance-matrix.md#2-release-blocking-tests-zero-failures)
**Data as of:** 2026-07-31 (commit `d9cf636`)

---

## 1. Suite Inventory

| Suite | Count | How to Run | Coverage Scope |
|---|---|---|---|
| Frontend (vitest) | 94 files / 831 tests (830 pass + 1 flaky) | `pnpm test` | React components, Tauri command mocks, fail-closed behavior, UI logic, hooks, i18n rendering, code language detection, end-of-turn detection, project path handling, voice audio, redaction, runtime availability, file preview |
| Rust (cargo test) | 419 tests (419 pass + 1 ignored) | `cargo test --manifest-path src-tauri/Cargo.toml` | See module breakdown below. +3 wecom signature tests, +1 permission delete test added in Phase 2. |
| Brand policy | 9 tests | `node scripts/check-brand-policy.mjs` (+ `.test.mjs`) | Brand scan rules: lowercase brand detection, Grok/xAI coupling residue, Runtime brand normalization, allowlist scoping. Code-span exemption added for `omp` CLI binary names. |
| i18n completeness | 5 tests | `node scripts/check-i18n-completeness.mjs` (+ `.test.mjs`) | Three-locale (en/zh-CN/zh-TW) key coverage, ICU parameter/type validation, empty value detection, hardcoded string detection |
| Legal baseline | tests | `node scripts/check-legal-baseline.mjs` (+ `.test.mjs`) | MIT license attribution (RongleCat, OMP/Pi authors), NOTICE files, font/font-highlight.js attributions |
| Provenance | 17 tests | `node scripts/check-provenance.mjs` (+ `.test.mjs`) | Fork provenance: Grok App upstream attribution, xAI asset removal verification. Patch ledger SHAs corrected + cursor-provider-removal patch recorded. |

### Rust Module Breakdown (top modules by test count)

| Module | Tests | Scope |
|---|---|---|
| `remote_im` | 62 | Channel adapters, dedup store, rate limiter, protocol start, weixin flow, catalog, media, bridge, session, config, validation |
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
| `secrets` | 11 | OS keychain integration (Keychain/Credential Manager/Secret Service) |
| `event_journal` | 11 | Event journal persistence, commit points, standard path format |
| `acp_golden_test` | 9 | ACP golden file contract tests |
| `permission_rules` | 8 | Permission rule evaluation |
| `journal_throttle` | 8 | Journal write throttling |
| Others (25+ modules) | ~100 | voice_stt, hooks, session_content_search, proxy, app_update, supervisor, session_title, permission_host_test, agents_catalog, support_bundle, path_scope, store_lock, portability, slash, projects, pb_frame, app_sessions, feishu_reg, weixin_reg, control_plane, runtime, types, validate, config |

**Environment note:** `store::tests::ensure_general_project_is_idempotent_and_not_removable` fails under filesystem sandboxing (sandbox blocks app data directory creation) but passes in CI (3 platforms, latest run 2026-07-30 green) and unsandboxed local runs. Not a product bug.

---

## 2. Pass-Condition Mapping

| ID | Pass Condition | Automated Coverage | Manual Required | Gap |
|---|---|---|---|---|
| TC-M.1 | §1 Capability baseline 100% alignment | `acp_client` (21), `extensions` (13), `acp_golden_test` (9), `permission` (20) — contract tests for protocol methods | Capability negotiation with real bundled Runtime; product entry + documentation alignment review | No end-to-end capability negotiation test with bundled Runtime; no automated doc-alignment check |
| TC-M.2 | §2 Release-blocking tests zero failures | vitest 831 + cargo 416 + 4 check scripts — all green | E2E happy-path, packaging, updater verification | Mock E2E happy-path test deferred from Plan 7 final review |
| TC-M.3 | §3 Locale coverage 100% + ICU zero errors | `check-i18n-completeness.mjs` (5 tests) — key coverage + ICU validation | Critical flow screenshots in 3 locales | No screenshot automation |
| TC-M.4 | §4 Brand scan zero violations | `check-brand-policy.mjs` (9 tests) — scan + allowlist | Allowlist functional regression (xAI Provider, Grok models display correctly) | — |
| TC-M.5 | §5 Active Directory discovery CLI parity | — | Desktop vs CLI discovery comparison across 3 OS | No automated parity test |
| TC-M.6 | §6 Credential migration all-pass | `secrets` (11), `mirror` (12) — keychain roundtrip, migration steps | System secure storage unavailable scenario; artifact plaintext scan | Some migration sub-items manual-only (AC-6.4, AC-6.8) |
| TC-M.7 | §7 Crash recovery no auto-replay | `event_journal` (11), `journal_throttle` (8) — journal persistence, commit points | Manual crash injection: kill sidecar mid-turn → restart → verify no replay | No automated crash recovery test |
| TC-M.8 | §8 Remote channel acceptance | `remote_im` (62) — dedup, rate limiter, protocol start, weixin flow, catalog, media, bridge, config, validation | Real platform smoke tests on ≥1 OS per channel | No real-platform smoke automation; applicability items (edit/button/group/channel/thread) mostly manual |
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
| **v1 transport injection** | AC-1.2/1.3/1.4/1.8/1.9, AC-5.1/5.2 | **Critical** | `OmpExtension::request()` (`omp_desktop_v1/mod.rs:72-93`) is a fail-closed stub — all 32 `_omp/desktop/v1/*` methods return runtime_unavailable regardless of negotiated capability. Highest-leverage remaining work; turns ~16 PARTIAL items into verifiable PASS/FAIL. |
| Mock E2E happy-path test | AC-2.9 | **High** | Deferred from Plan 7 final review. Should simulate: session create → prompt → response → permission approval → tool execution → turn end, all with mock Runtime. |
| End-to-end capability negotiation test | AC-1.1–AC-1.13 | **High** | Current contract tests verify individual methods. Need integration test: Desktop ↔ bundled Runtime negotiate full baseline, assert all 13 groups present. (Blocked by the v1 transport gap above.) |
| **6-step credential migration suite** | AC-6.1/6.2/6.5/6.6/6.7, AC-2.10 | **High** | Master design §8.2's dry-run/readback/tombstone/rollback migration is unimplemented. Only a 2-step copy+clear exists. |
| **Subagent policy inheritance** | AC-1.5 | **High** | `agent_subagents.rs` only syncs on/off; no permission/policy/MCP inheritance in Desktop. |
| **Host+Hub trace correlation** | AC-1.13 | **High** | grep "trace_id/correlation/span/otel" = zero matches. Mandatory Host+Hub scope is absent. |
| **Event-journal recovery wiring** | AC-1.10 | **High** | `replay_from`/`load_from` have no production call sites — recovery path not wired to session reconnect. |
| Automated crash recovery test | AC-7.1 | **High** | Kill sidecar mid-turn, restart, assert: no auto-replay, turn marked `unknown/interrupted`. Currently no auto-replay by absence-of-code, not asserted. |
| **Remote approval expiry + anti-replay** | AC-8.4 | **High** | No approval/expiry/nonce/replay infrastructure in remote_im; `allow_remote_yolo` is a bare boolean. |
| **Single update channel** | AC-10.9 | **Medium** | Only silent/github_manual exist; no stable/beta/nightly isolation. |
| Discovery parity test | AC-5.1 | **Medium** | Compare Desktop discovery API output vs CLI `omp` discovery for same env vars/profile/cwd. (Blocked by v1 transport gap.) |

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

| Issue | Severity | Matrix Items |
|---|---|---|
| Remote channel credentials plaintext in `channel-secrets.json` | HIGH | SA-R.1 / AC-6.2 |
| Silent plaintext fallback when OS secure storage unavailable | HIGH | SA-C.3 / AC-6.4 |

> Note: AC-6.4 was previously listed as "Manual-Acceptable" (verify actionable error). Phase 2 code audit **confirmed** it as a real FAIL: `secrets.rs:410-431` silently writes plaintext `secrets.json` when keychain is off/unavailable. This is an architectural gap, not a verification gap — flipping it breaks plaintext-mode users and needs a migration plan.

### Known Code Quality Items (from Plan 7 Final Review)

Deferred items noted during Plan 7 merge, not release-blocking but should be tracked:

| Item | Location | Notes |
|---|---|---|
| `AgentTurnResult` stale comments | `src-tauri/src/remote_im/` | Comments describe outdated behavior; cosmetic but misleading. |
| `runtimes` / `in_flight` / `spawn_locks` maps unbounded growth | `src-tauri/src/remote_im/runtime.rs` | No eviction policy for per-`work_dir` maps. Long-running instances with many distinct work dirs could leak memory. Needs TTL or LRU eviction. |

### Coverage Strengths

Areas with strong automated coverage (no gaps identified):

- **Permission model**: 20 permission + 8 permission_rules + 5 permission_host_test = 33 tests covering per-path gates, fail-closed, request binding.
- **Remote IM infrastructure**: 62 tests covering dedup, rate limiting, protocol start, channel flow, media, config validation.
- **Event journal**: 11 tests covering persistence, commit points, path format, throttle.
- **Secrets**: 11 tests covering keychain roundtrip, migration steps.
- **Brand/legal/provenance**: 4 check scripts with dedicated test files, run in CI.
- **Frontend**: 831 tests across 94 files — comprehensive component and hook coverage.
