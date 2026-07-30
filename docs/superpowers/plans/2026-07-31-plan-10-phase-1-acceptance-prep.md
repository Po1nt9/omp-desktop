# Plan 10 Phase 1: Acceptance Preparation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the three release documentation deliverables (acceptance matrix skeleton, security audit checklist, test coverage audit) that form the Phase 1 preparation work of Plan 10, then commit all pending changes.

**Architecture:** Three standalone markdown documents in `docs/release/`, cross-referenced. The acceptance matrix is the master sign-off document organized by master design §18.2's 12 Pass conditions. The security checklist and test audit are working documents referenced by the matrix. All items start as `PENDING` — execution is Phase 2+ work.

**Tech Stack:** Markdown. Source material: master design (`docs/superpowers/specs/2026-07-28-omp-desktop-design.md`) §5.2, §8, §11, §14, §15, §17, §18; design spec (`docs/superpowers/specs/2026-07-31-plan-10-phase-1-acceptance-prep-design.md`).

## Global Constraints

- All check items use status enum: `PASS` / `FAIL` / `BLOCKED` / `WAIVED` / `PENDING`. Initial state is `PENDING` for every item.
- Item IDs: `AC-<condition>.<seq>` (matrix), `SA-<domain>.<seq>` (security), `TC-<suite>.<seq>` (test audit).
- Documents are in English (matching existing `docs/release/signing-requirements.md`).
- No cartesian product of all dimensions (master design §18.1 prohibition).
- Performance targets: cold start < 3 s, idle RSS < 200 MB, TTFT < 2 s, 100-message load < 500 ms.
- 10 fixed official channels + 1 conditional (WeChat personal) per master design §14.1.

---

### Task 1: Acceptance Matrix Skeleton

**Files:**
- Create: `docs/release/1.0-acceptance-matrix.md`

**Interfaces:**
- Consumes: master design §5.2 (capability baseline), §14.1 (channel list), §18.1 (dimensions), §18.2 (12 Pass conditions); design spec §2.4 (capability decomposition), §2.7 (benchmark targets), §3 (outline)
- Produces: the master sign-off document referenced by Task 2 and Task 3

- [ ] **Step 1: Create the matrix document**

Write `docs/release/1.0-acceptance-matrix.md` with this structure:

1. **Header + meta table** — title, version (0.1.0-skeleton), date, link to master design §18 and design spec. Sign-off table with rows for: Project Lead, QA Lead, Security Auditor (all empty).

2. **§1–§12: One section per Pass condition** from master design §18.2. Each section contains a table with columns: ID | Item | Verification | Status | Evidence | Sign-off. Content per section:

   - **§1 Capability baseline 100% alignment** — decompose §5.2 into items per design spec §2.4 table (13 groups: Core ACP, Queue & Steer, Provider/Model/Credential, MCP, Todo/Subagent, Branch/Checkpoint/Rewind, Usage/Compaction, Attachment, Diagnostics, Event Replay/Recovery, Localization Envelope, Thinking Visibility, Trace Correlation). Each group = one item. Verification = "capability negotiation test + contract test". Note trace propagation end-to-end is optional (not release-blocking).
   - **§2 Release-blocking tests zero failures** — items for each gate: `pnpm test` (831 tests), `cargo test` (416 tests), 4 check scripts (brand/provenance/i18n/legal). Plus: protocol tests, Tauri command tests, E2E tests, migration tests, packaging tests, updater tests. Mark which exist and which are missing (reference test-coverage-audit.md).
   - **§3 Locale coverage** — 3 items: en/zh-CN/zh-TW message key coverage 100%, ICU/variable validation zero errors, critical flow screenshots. Verification = `node scripts/check-i18n-completeness.mjs` + manual screenshots.
   - **§4 Brand scan zero violations** — 1 item: `node scripts/check-brand-policy.mjs` zero violations with explicit OMP catalog allowlist (xAI Provider, Grok model names, Provider endpoints, auth methods, sanitized Provider errors).
   - **§5 Active Directory discovery CLI parity** — 1 item: Desktop discovery vs CLI `omp` discovery 100% match; Desktop does not bypass OMP writer/lock.
   - **§6 Credential migration all-pass** — 7 items from §18.2 condition 6: dry run, success, repeat execution, system secure storage unavailable, readback failure, cleanup failure, rollback. Plus: no plaintext secret fallback in release artifacts.
   - **§7 Crash recovery no auto-replay** — 2 items: Desktop does not auto-replay prompt/tool/edit/shell after crash; unknown boundaries marked `unknown/interrupted` with honest copy.
   - **§8 Remote channel acceptance** — 8 common mandatory items per §14.2 (credential validation, inbound/outbound, identity whitelist, approval expiry + anti-replay, dedup, rate limiting/backoff, revocation, log sanitization) × 10 fixed channels. Plus: applicability matrix items (edit, button, group, channel, thread) per declared capabilities. Plus: WeChat personal conditional item. Reference security-audit-checklist.md SA-R domain.
   - **§9 Workspace routing + tool containment** — 2 items: workspace whitelist is routing/cwd only (consistent messaging); OMP canonical path + shell containment + MCP/subagent inheritance capabilities all pass before remote tool enablement.
   - **§10 Install/sign/upgrade/rollback/SBOM** — 5 items per OS target: macOS Universal (ARM + Intel), Windows x64, Linux x64, Linux ARM64. Each: install, signature/checksum verification, upgrade, rollback, SBOM/license audit.
   - **§11 External OMP diagnostics-only + update channels** — 2 items: external OMP entry is diagnostics-only (no session/config-write/auto-update); three update channels (stable/beta/nightly) correctly isolated.
   - **§12 Documentation coverage** — 10 items from §18.2 condition 12: installation, Provider setup, credentials, permissions, remote risks, recovery boundaries, i18n, updates, licenses, contributing.

3. **Appendix A: Performance benchmark targets** — 4-row table from design spec §2.7 (metric | target | protocol | status=PENDING).

4. **Appendix B: OS × locale test matrix** — two compact tables per design spec §3: capability × OS (4 columns) and core flow × locale (3 columns). All cells `PENDING`.

- [ ] **Step 2: Verify document structure**

Run: `grep -c "^##" docs/release/1.0-acceptance-matrix.md`
Expected: 15+ (12 Pass condition sections + meta + 2 appendices)

Run: `grep -c "PENDING" docs/release/1.0-acceptance-matrix.md`
Expected: 40+ (all items start as PENDING)

- [ ] **Step 3: Commit**

```bash
git add docs/release/1.0-acceptance-matrix.md
git commit -m "docs(release): add 1.0 acceptance matrix skeleton (Plan 10 Phase 1)"
```

---

### Task 2: Security Audit Checklist

**Files:**
- Create: `docs/release/security-audit-checklist.md`

**Interfaces:**
- Consumes: master design §8 (credentials), §11 (permissions), §14 (remote), §15 (logs), §17 (brand/legal); design spec §2.5 (5 domains)
- Produces: security checklist referenced by matrix §6, §8, §9

- [ ] **Step 1: Create the checklist document**

Write `docs/release/security-audit-checklist.md` with:

1. **Header** — title, purpose, link to master design §11/§8/§14/§15/§17, link to acceptance matrix. Note: self-audit is insufficient for 1.0 (Plan 10 dep #4); this checklist structures both self-audit and external auditor review.

2. **Five domain sections**, each with a table (ID | Check | Method | Status | Notes):

   - **SA-C: Credential Storage** (from §8) — items:
     - SA-C.1: SecretStore uses OS keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service) — code audit `src-tauri/src/secrets.rs`
     - SA-C.2: `agent.db` stores only `keychain:v1:<opaque-id>` references, never plaintext — code audit + DB inspection
     - SA-C.3: No plaintext fallback when system secure storage unavailable — must block save/refresh with actionable error — code audit + manual test
     - SA-C.4: Remote platform credentials use isolated namespace/ACL/metadata — code audit
     - SA-C.5: Migration 6-step idempotency (dry run → copy → readback → reference commit → tombstone → cleanup) — test `cargo test` migration suite
     - SA-C.6: No Desktop/CLI-defined keychain service/account naming — code audit
     - SA-C.7: Release artifacts contain no plaintext secret fallback — artifact inspection

   - **SA-P: Permission Model** (from §11) — items:
     - SA-P.1: Per-path decision table enforced (bash/edit/delete/move/elicitation/plan approval/subagent) — code audit + contract tests
     - SA-P.2: Fail-closed on missing capability — no tool execution without approval — test suite
     - SA-P.3: Request binding (runtime instance + session + turn + request ID) — code audit
     - SA-P.4: Timeout/restart/turn-end invalidation of pending requests — code audit + test
     - SA-P.5: "First legal decision wins" applies only to same pending request — code audit
     - SA-P.6: Process-level override not disguised as per-session setting in UI — UI audit
     - SA-P.7: Subagent cannot escalate beyond parent policy; MCP/workspace constraints inherited — code audit + test
     - SA-P.8: Delete gate independent from edit/move approval — code audit

   - **SA-R: Remote Access** (from §14) — items:
     - SA-R.1: Channel credentials in SecretStore remote namespace (isolated from Agent Provider credentials) — code audit
     - SA-R.2: All channels default-off — config audit
     - SA-R.3: Webhook default loopback; public ingress requires explicit user reverse-proxy/tunnel config — code audit
     - SA-R.4: Identity whitelist enforcement per channel — test suite (53 remote_im tests)
     - SA-R.5: Approval expiry + anti-replay — test suite
     - SA-R.6: Dedup (SQLite `seen_messages`, `(channel,message_id)` PK, 7-day TTL) — test suite (DedupStore tests)
     - SA-R.7: Rate limiting (fixed window, per-channel 60/min + per-scope 10/min) — test suite (RateLimiter tests)
     - SA-R.8: Revocation handling — test suite
     - SA-R.9: Log sanitization in remote layer — code audit
     - SA-R.10: Enterprise WeChat: loopback bind (not 0.0.0.0), reject unsigned/undecryptable requests, port conflict/restart/proxy-source/key-rotation tests — code audit + test
     - SA-R.11: Remote approval does not require extra PIN; docs recommend platform MFA + strict user whitelist + least privilege — doc review

   - **SA-L: Log Sanitization** (from §15) — items:
     - SA-L.1: Default logs do not contain prompt/reply/file-content/full-tool-output — code audit
     - SA-L.2: key/token/cookie/header/suspected-secret sanitization in all log layers (desktop/runtime/protocol/remote/updater) — code audit
     - SA-L.3: Crash reports local-only until user previews and confirms — code audit + UI audit
     - SA-L.4: Diagnostics page self-check is read-only (no model requests, no project modifications) — code audit + test
     - SA-L.5: Support bundle excludes secrets — test (`support_bundle` tests)

   - **SA-B: Brand & Legal** (from §17) — items:
     - SA-B.1: MIT license preservation (RongleCat Grok App + Mario Zechner/Can Bölük OMP/Pi attributions) — `node scripts/check-legal-baseline.mjs`
     - SA-B.2: THIRD_PARTY_NOTICES in installer packages — artifact inspection
     - SA-B.3: In-app "About → Open Source Licenses" — UI audit
     - SA-B.4: No Grok/xAI brand assets (name, icon, bundle identifier) — `node scripts/check-brand-policy.mjs`
     - SA-B.5: Brand scan allowlist explicitly scoped to OMP catalog/runtime xAI Provider, Grok model names, Provider endpoints, auth methods, sanitized Provider errors — code audit of scan config
     - SA-B.6: Provenance check — `node scripts/check-provenance.mjs`

3. **Audit execution notes** — self-audit vs external auditor scope; external auditor is a Plan 10 dependency (dep #4); self-audit results are preliminary until external review.

- [ ] **Step 2: Verify document structure**

Run: `grep -c "^##" docs/release/security-audit-checklist.md`
Expected: 7+ (5 domains + header + notes)

Run: `grep -c "SA-" docs/release/security-audit-checklist.md`
Expected: 30+ (all item IDs)

- [ ] **Step 3: Commit**

```bash
git add docs/release/security-audit-checklist.md
git commit -m "docs(release): add security audit checklist (Plan 10 Phase 1)"
```

---

### Task 3: Test Coverage Audit

**Files:**
- Create: `docs/release/test-coverage-audit.md`

**Interfaces:**
- Consumes: actual test counts (vitest 831 / cargo 416 / 4 check scripts), master design §18.2 (12 Pass conditions), Plan 7 final review deferred items
- Produces: test audit referenced by matrix §2

- [ ] **Step 1: Create the audit document**

Write `docs/release/test-coverage-audit.md` with three sections:

1. **Suite Inventory** — table with columns: Suite | Count | How to Run | Coverage Scope:
   - Frontend (vitest): 94 files / 831 tests | `pnpm test` | React components, Tauri command mocks, fail-closed behavior, UI logic
   - Rust (cargo test): 416 tests (414 pass + 1 ignored + 1 env-dependent) | `cargo test --manifest-path src-tauri/Cargo.toml` | Tauri commands, store, secrets, remote_im (59 tests), session manager, event journal, portability, support bundle, path scope, fs browser
   - Brand policy: 9 tests | `node scripts/check-brand-policy.mjs` (+ `.test.mjs`) | Brand scan rules
   - i18n completeness: 5 tests | `node scripts/check-i18n-completeness.mjs` (+ `.test.mjs`) | Three-locale key coverage, ICU params
   - Legal baseline: tests | `node scripts/check-legal-baseline.mjs` (+ `.test.mjs`) | License attribution checks
   - Provenance: tests | `node scripts/check-provenance.mjs` (+ `.test.mjs`) | Fork provenance checks

   Note: cargo test `store::tests::ensure_general_project_is_idempotent_and_not_removable` fails in sandboxed environments (filesystem restriction) but passes in CI and unsandboxed local runs — not a product bug.

2. **Pass-Condition Mapping** — for each of the 12 Pass conditions, a row: ID | Condition | Automated Coverage | Manual Required | Gap. IDs use `TC-M.<seq>` format:
   - §1 Capability baseline: contract tests (partial) | capability negotiation with real Runtime | no end-to-end capability negotiation test with bundled Runtime
   - §2 Release-blocking tests: vitest 831 + cargo 416 + 4 check scripts | E2E, packaging, updater tests | mock E2E happy-path test (deferred from Plan 7)
   - §3 Locale: check-i18n-completeness.mjs | critical flow screenshots in 3 locales | screenshot automation
   - §4 Brand: check-brand-policy.mjs (9 tests) | — | —
   - §5 Discovery parity: — | Desktop vs CLI discovery comparison | no automated parity test
   - §6 Credential migration: cargo migration tests | system secure storage unavailable scenario | partial (7 sub-items, some manual)
   - §7 Crash recovery: — | manual crash injection | no automated crash recovery test
   - §8 Remote channels: 59 remote_im tests (dedup, rate limiter, protocol start, weixin flow, catalog) | real platform smoke tests on ≥1 OS | no real-platform smoke automation
   - §9 Workspace/containment: path scope tests | OMP canonical path + shell containment verification | depends on Runtime capabilities
   - §10 Install/upgrade: release.yml CI (4 targets) | manual install/upgrade/rollback per OS | no automated install test
   - §11 External OMP + channels: — | manual verification | no automated test
   - §12 Documentation: check scripts (links) | manual review | —

3. **Gap Analysis** — prioritized list of missing coverage:
   - **Release-blocking gaps:**
     - Mock E2E happy-path test (deferred from Plan 7 final review)
     - End-to-end capability negotiation test with bundled Runtime
     - Automated crash recovery test (conservative recovery path)
     - Discovery parity test (Desktop vs CLI)
   - **Important but manual-acceptable:**
     - Critical flow locale screenshots (3 locales)
     - Real-platform channel smoke tests
     - Install/upgrade/rollback per OS
   - **Known code quality items (from Plan 7 final review):**
     - `AgentTurnResult` stale comments
     - `runtimes`/`in_flight`/`spawn_locks` maps unbounded growth — no eviction policy
   - **Environment note:** cargo test `ensure_general_project_is_idempotent_and_not_removable` fails under filesystem sandboxing; passes in CI (3 platforms) and unsandboxed runs.

- [ ] **Step 2: Verify document structure**

Run: `grep -c "^##" docs/release/test-coverage-audit.md`
Expected: 3+ (inventory + mapping + gaps)

- [ ] **Step 3: Commit**

```bash
git add docs/release/test-coverage-audit.md
git commit -m "docs(release): add test coverage audit (Plan 10 Phase 1)"
```

---

### Task 4: Wrap-Up — Commit Pending Changes + Update Statuses

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md` (mark Preparation Work done)
- Modify: `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md` (already has uncommitted status updates)
- Commit: design spec (already created)

- [ ] **Step 1: Update Plan 10 document**

In `docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md`, update the Preparation Work section to mark items as done:

```markdown
## Preparation Work (can be done NOW without external deps)

- ✅ Write the acceptance matrix skeleton (`docs/release/1.0-acceptance-matrix.md`) with all capability items from the master design, even though many are currently BLOCKED.
- ✅ Define performance benchmark targets (startup, memory, latency) — these are product decisions, not implementation.
- ✅ Write a security audit checklist (`docs/release/security-audit-checklist.md`) covering credential storage, permission model, remote access, brand policy, legal baseline.
- ✅ Audit existing test coverage: `pnpm test` (831 tests), `cargo test` (416 tests), `node scripts/check-*.mjs` (brand/provenance/i18n/legal). Document what's covered and what's missing.
```

Also update the test coverage table:

```markdown
## Current Test Coverage (as of Plan 10 Phase 1)

| Suite | Count | Status |
|---|---|---|
| Frontend (vitest) | 831 tests (94 files) | ✅ All pass |
| Rust (cargo test) | 416 tests (414 pass + 1 ignored + 1 env-dependent) | ✅ All pass (CI green; 1 sandbox-only failure) |
| Brand policy | 9 tests | ✅ All pass |
| i18n completeness | 5 tests | ✅ All pass |
| Standalone checks | brand/provenance/i18n/legal | ✅ All pass |
```

- [ ] **Step 2: Commit all pending changes**

```bash
git add docs/superpowers/specs/2026-07-31-plan-10-phase-1-acceptance-prep-design.md \
       docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md \
       docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md
git commit -m "docs: Plan 10 Phase 1 design spec + status sync (Preparation Work complete)"
```

- [ ] **Step 3: Verify all deliverables exist**

Run: `ls -la docs/release/`
Expected: 4 files (signing-requirements.md + 3 new documents)

Run: `git log --oneline -5`
Expected: 4 commits from this plan (matrix, security checklist, test audit, wrap-up)

Run: `git status --short`
Expected: only `m runtime/oh-my-pi` (pre-existing submodule change, not ours)
