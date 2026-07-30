# Plan 10 Phase 1: Acceptance Preparation — Design Spec

**Date:** 2026-07-31
**Status:** Approved (autonomous design per user directive)
**Scope:** Plan 10 "Preparation Work (can be done NOW)" — the three documentation deliverables that require no external dependencies.

---

## 1. Background

Plan 10 (1.0 Acceptance Matrix) is unblocked: Plans 1–9 are complete, OS code-signing is deferred to optional. The Plan 10 document defines four preparation items executable without cross-platform test infrastructure, performance tooling, or an external security auditor:

1. Acceptance matrix skeleton (`docs/release/1.0-acceptance-matrix.md`)
2. Performance benchmark target definitions (already decided: cold start < 3 s, idle RSS < 200 MB, TTFT < 2 s, 100-message session load < 500 ms)
3. Security audit checklist (`docs/release/security-audit-checklist.md`)
4. Test coverage audit (`docs/release/test-coverage-audit.md`)

This spec defines the structure, item format, and cross-references for these three documents. It does **not** execute the acceptance tests, benchmarks, or audit — that is Phase 2+.

## 2. Design Decisions

### 2.1 Matrix structure: Pass-condition-driven (Approach A)

The acceptance matrix is organized by the **12 Pass conditions** from master design §18.2. Each condition is a top-level section containing concrete check items grouped by dimension (capability / OS / locale / runtime mode / channel) where applicable.

**Rationale:**
- The 12 Pass conditions are the release sign-off criteria; organizing by them lets a reviewer go condition-by-condition and reach a ship/no-ship conclusion directly.
- Master design §18.1 explicitly says "不要求对所有维度做无意义的全笛卡尔积" — a full cartesian matrix (Approach C) is ruled out.
- Dimension grouping (Approach B) is used *within* each condition where it aids clarity, combining the strengths of both approaches.

### 2.2 Three-document architecture

```
docs/release/
├── 1.0-acceptance-matrix.md      ← master document
├── security-audit-checklist.md   ← referenced by matrix §4 (security conditions)
└── test-coverage-audit.md        ← referenced by matrix §2 (test gate conditions)
```

- The matrix is the single source of truth for release sign-off.
- The security checklist and test audit are standalone working documents; their summary conclusions are reflected back into the matrix item statuses.
- Performance benchmark targets live inside the matrix (§3 below), not a separate document — they are four numbers with measurement protocols, not a long-form doc.

### 2.3 Item format and status enum

Every check item in all three documents uses a consistent table row format:

| Column | Description |
|---|---|
| **ID** | Stable identifier: `AC-<condition>.<seq>` (matrix), `SA-<domain>.<seq>` (security), `TC-<suite>.<seq>` (test audit) |
| **Item** | What is being verified |
| **Verification** | How: automated test name, manual procedure, or audit step |
| **Status** | One of: `PASS` / `FAIL` / `BLOCKED` / `WAIVED` / `PENDING` |
| **Evidence** | Link or inline excerpt: test output, screenshot, commit hash, audit report section |
| **Sign-off** | Initials + date of verifier (empty until executed) |

Status semantics:
- `PASS` — verified with evidence.
- `FAIL` — verified, did not pass; blocks release unless waived.
- `BLOCKED` — cannot verify yet (missing infrastructure, dependency); must list what unblocks it.
- `WAIVED` — explicitly exempted with documented justification; requires sign-off.
- `PENDING` — not yet attempted (initial state for all items in the skeleton).

### 2.4 Capability baseline decomposition

Master design §5.2 lists the 1.0 mandatory capability baseline as a prose paragraph. The matrix decomposes it into discrete check items:

| Group | Items |
|---|---|
| Core ACP | session, prompt, cancel, tool, permission, elicitation |
| Queue & Steer | queue receipt lifecycle, steer target/ack/ordering, restart receipt query |
| Provider/Model/Credential | provider CRUD + status, model catalog, credential lifecycle (no secret to React), session config |
| MCP | config/discovery via versioned API, no Desktop second-source |
| Todo / Subagent | todo lifecycle, subagent policy inheritance (no privilege escalation) |
| Branch / Checkpoint / Rewind | branch create/switch, checkpoint, rewind |
| Usage / Compaction | usage reporting, compaction |
| Attachment | attachment send/receive in session |
| Diagnostics | diagnostics page, one-click self-check |
| Event Replay / Recovery | stable event ID, replay cursor, journal commit point, conservative recovery |
| Message Localization Envelope | messageKey + typed args for Runtime user-visible strings |
| Thinking Visibility | per-event visibility classification |
| Trace Correlation | Desktop Host + Remote Hub scope (mandatory); end-to-end propagation (optional, not release-blocking) |

Each group becomes one or more `AC-1.x` items under Pass condition 1 (capability baseline 100% alignment).

### 2.5 Security audit checklist domains

Five domains derived from master design sections:

| Domain | Source | Key checks |
|---|---|---|
| **Credential storage** | §8 | SecretStore uses OS keychain (Keychain/Credential Manager/Secret Service); `agent.db` stores only `keychain:v1:<opaque-id>`; no plaintext fallback; remote namespace isolated; migration 6-step idempotency (dry run → copy → readback → reference commit → tombstone → cleanup) |
| **Permission model** | §11 | Per-path decision table enforced (bash/edit/delete/move/elicitation/plan/subagent); fail-closed on missing capability; request binding (runtime+session+turn+request ID); timeout/restart invalidation; no per-session disguise of process-level override |
| **Remote access** | §14 | Channel credentials in SecretStore remote namespace; all channels default-off; webhook default loopback; identity whitelist; approval expiry + anti-replay; dedup; rate limiting/backoff; revocation; log sanitization; enterprise WeChat loopback + unsigned-request rejection |
| **Log sanitization** | §15 | No prompt/reply/file-content/full-tool-output in default logs; key/token/cookie/header/suspected-secret sanitization; crash report local-only until user confirms; diagnostics page read-only self-check |
| **Brand & legal** | §17 | MIT license preservation (RongleCat + OMP/Pi attributions); THIRD_PARTY_NOTICES in installers; in-app "About → Open Source Licenses"; no Grok/xAI brand assets; brand policy scan zero violations (with explicit OMP catalog allowlist) |

### 2.6 Test coverage audit structure

Three sections:

1. **Suite inventory** — current counts per suite (vitest 831, cargo 416, 4 check scripts with tests), what each covers, how to run.
2. **Pass-condition mapping** — for each of the 12 Pass conditions: which automated tests cover it, what requires manual verification, what has no coverage yet.
3. **Gap analysis** — items with no automated coverage, prioritized by release-blocking severity. Known gaps from Plan 7 final review (mock E2E happy-path, AgentTurnResult stale comments, unbounded map eviction) are listed here.

### 2.7 Performance benchmark targets

Already decided (Plan 10 document + master design). Listed in the matrix under Pass condition 2 with measurement protocols:

| Metric | Target | Protocol |
|---|---|---|
| Cold start to interactive UI | < 3 s | Kill app → launch → first interactive frame; median of 5 runs per OS |
| Idle RSS | < 200 MB | App open, no active session, 60 s after startup; `ps`/Task Manager |
| Time-to-first-token | < 2 s | Simple prompt ("Hello") → first streamed token; requires configured Provider |
| 100-message session load | < 500 ms | Open session with 100 messages → fully rendered; median of 5 runs |

These are `PENDING` in the skeleton — actual measurement is Phase 2 work requiring cross-platform infrastructure.

## 3. Acceptance Matrix Outline

The matrix document (`docs/release/1.0-acceptance-matrix.md`) follows this structure:

```
# OMP Desktop 1.0 Acceptance Matrix
## Meta (version, date, sign-off table)
## §1  Capability baseline 100% alignment        ← Pass condition 1
## §2  Release-blocking tests zero failures       ← Pass condition 2
## §3  Locale coverage 100% + ICU zero errors     ← Pass condition 3
## §4  Brand scan zero violations                 ← Pass condition 4
## §5  Active Directory discovery CLI parity      ← Pass condition 5
## §6  Credential migration all-pass              ← Pass condition 6
## §7  Crash recovery no auto-replay              ← Pass condition 7
## §8  Remote channel common + applicability      ← Pass condition 8
## §9  Workspace routing + tool containment       ← Pass condition 9
## §10 Install/sign/upgrade/rollback/SBOM         ← Pass condition 10
## §11 External OMP diagnostics-only + channels   ← Pass condition 11
## §12 Documentation coverage                     ← Pass condition 12
## Appendix A: Performance benchmark targets
## Appendix B: OS × locale test matrix (non-cartesian summary)
```

Each § contains a table of `AC-<n>.<seq>` items. Appendix B follows master design §18.1's rule — "每个 baseline capability 必须在所有支持 OS 上通过；每个用户可见核心流程必须在三 locale 通过" — and therefore contains two compact tables instead of a full cartesian product:

1. **Capability × OS** — each §5.2 baseline capability row × 4 OS columns (macOS ARM, macOS Intel, Windows x64, Linux x64/ARM64).
2. **Core flow × locale** — each user-visible core flow row × 3 locale columns (en, zh-CN, zh-TW).

## 4. Out of Scope (Phase 1)

- Executing any acceptance test, benchmark, or audit (Phase 2+).
- Cross-platform test infrastructure setup.
- External security auditor engagement.
- Performance tooling selection and CI integration.
- OS code-signing (optional, user-initiated).

## 5. Relationship to Later Phases

| Phase | Work | Depends on Phase 1 |
|---|---|---|
| 2 | Performance benchmark measurement | Matrix Appendix A targets + protocols |
| 3 | Cross-platform acceptance execution | Matrix items + OS × locale appendix |
| 4 | Security audit execution | Security checklist domains + items |
| 5 | Release readiness sign-off | All matrix items resolved (PASS/WAIVED) |

Phase 1 deliverables are the **skeleton and criteria**; later phases fill in statuses and evidence.
