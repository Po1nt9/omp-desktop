# Roadmap Doc Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md` (and the Plan 10 acceptance doc's blocker list) into agreement with what actually shipped, so the roadmap is a source of truth rather than a lie.

**Architecture:** Pure documentation edits — no code, no tests beyond reading the result. The roadmap currently marks Plan 7 (Remote Hub) and Plan 8 (Channels) as 🚫 Blocked, but P1–P3 + the Remote IM Runtime Bridge delivered both, just not via the originally-envisioned Hub architecture. Plan 7 landed as a local gateway (Runtime Bridge: per-`work_dir` AcpClient pool + drain barrier + 3-layer concurrency locks) rather than a remote Hub server. Plan 8 shipped 14 adapters (the plan listed 11 platforms; some split by region, plus a generic adapter). Plan 9's updater-signing sub-item completes via the companion auto-update plan; OS code-signing (Apple/Windows paid certs) remains blocked.

**Tech Stack:** Markdown. No build, no runtime.

## Global Constraints

- **Factual, concise edits.** Document *what shipped vs. what was planned and why the architecture differs*. No puffery, no rewriting history.
- **Preserve the existing document structure** (Status / Depends on / Spec / Scope / Key Tasks / Complexity / External Deps per plan). Only update Status lines, add a "How it actually shipped" note where the architecture diverged, and fix the summary table.
- **Plan 9 nuance:** updater signing = done (after the companion plan); OS code-signing = still blocked on certificates. Represent both.
- **Do not** rewrite individual plan files (plan-7-remote-hub.md, plan-8-channels.md, plan-9-os-packaging.md) beyond a one-line status header if they have one. The roadmap is the index; the per-plan docs keep their original scope for history.
- Branch: do this on `main` as a doc-only commit (or on the same `feat/auto-update-enablement` branch if it hasn't merged yet). Coordinate with the auto-update plan's Task 9 so docs don't diverge.

**Branch:** `docs/roadmap-sync` (new) off latest `main`.

---

## File Map

- **Modify:**
  - `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md` — the index (Status lines + summary table + per-plan "shipped as" notes for 7/8/9).
  - `docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md` — update the "External Dependencies (BLOCKING)" section to reflect that 7/8 landed and only 9's certs + cross-platform test infra remain.
- **Read-only references (cite, don't edit):**
  - `CHANGELOG.md` — the `0.3.0-nightly` section enumerates what shipped (Remote IM Runtime Bridge = Plan 7; 14 adapters = Plan 8; P1/P2/P3).
  - The companion design spec `docs/superpowers/specs/2026-07-31-auto-update-enablement-and-roadmap-sync-design.md`.

---

### Task 1: Create the doc-sync branch and confirm current roadmap text

- [ ] **Step 1: Branch off main**

```bash
cd ~/Github/grok-app-main
git checkout main
git pull --ff-only origin main
git checkout -b docs/roadmap-sync
```

- [ ] **Step 2: Re-read the exact current roadmap text to edit precisely**

```bash
sed -n '1,40p' docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md
```
Confirm the plan-list block (lines ~9-16) and the summary table (near the bottom) match what the design spec quoted. If line numbers drifted, locate by content.

- [ ] **Step 3: No commit yet.**

---

### Task 2: Update Plan 7 status (Remote Hub → shipped as local Runtime Bridge)

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md`

- [ ] **Step 1: Update the plan-list status line for Plan 7**

Find:
```
- Plan 7: [`2026-07-29-plan-7-remote-hub.md`](./2026-07-29-plan-7-remote-hub.md) — 🚫 Blocked (external deps)
```
Replace with:
```
- Plan 7: [`2026-07-29-plan-7-remote-hub.md`](./2026-07-29-plan-7-remote-hub.md) — ✅ Shipped (as local Runtime Bridge, not a remote Hub)
```

- [ ] **Step 2: Update the Plan 7 section Status line and add a "How it shipped" note**

In the `## Plan 7: Remote Hub` section, change:
```
**Status:** 🚫 Blocked (external deps)
```
to:
```
**Status:** ✅ Shipped as the **Remote IM Runtime Bridge** (`v0.3.0-nightly`).
```

Then immediately after the **External Dependencies** line of that section, insert a new subsection:
```markdown
### How it actually shipped (deviation from original design)

The Remote Hub server was **not built**. A pre-implementation review concluded the
Hub bundled three concerns, two of which (network reachability, multi-channel
orchestration) do not need a central server — the 14 channel adapters use
outbound long connections (WebSocket / long-poll / Socket Mode), proven by
[cc-connect](https://github.com/chenhg5/cc-connect) and
[hermes-agent](https://github.com/NousResearch/hermes-agent). The third concern,
cross-device session sync, is not a 1.0 requirement and was deferred.

What shipped instead (the `remote_im` engine's fail-closed gates replaced with
real OMP Runtime calls):
- **Per-`work_dir` AcpClient pool** — one runtime client per agent working dir.
- **Drain barrier** — in-flight turns complete before shutdown.
- **3-layer concurrency locks** — per-channel, per-session, per-runtime.
- Inbound IM messages drive real agent turns (end-to-end in `v0.3.0-nightly`).

See `CHANGELOG.md` `[0.3.0-nightly]` → "Remote IM Runtime Bridge (Plan 7)".
The originally-scoped **cross-device session sync** remains a possible future
plan (a lightweight journal relay over Plan 3's event journal), not blocking 1.0.
```

- [ ] **Step 3: No commit yet (batch with Plan 8/9).**

---

### Task 3: Update Plan 8 status (Channels → 14 adapters shipped)

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md`

- [ ] **Step 1: Update the plan-list status line**

Find:
```
- Plan 8: [`2026-07-29-plan-8-channels.md`](./2026-07-29-plan-8-channels.md) — 🚫 Blocked (external deps)
```
Replace with:
```
- Plan 8: [`2026-07-29-plan-8-channels.md`](./2026-07-29-plan-8-channels.md) — ✅ Shipped (14 adapters)
```

- [ ] **Step 2: Update the Plan 8 section Status + add a note**

Change:
```
**Status:** 🚫 Blocked (external deps)
**Depends on:** Plan 7 (for Remote Hub)
```
to:
```
**Status:** ✅ Shipped as **14 channel adapters** (`v0.3.0-nightly`).
**Depends on:** Plan 7 (shipped — the Runtime Bridge, not the Hub)
```

After the **External Dependencies** line, insert:
```markdown
### How it actually shipped (deviation from original design)

14 adapters shipped in `v0.3.0-nightly`: feishu, telegram, discord, slack,
dingtalk, wecom, weixin, qq, qqbot, matrix, line, weibo, wpc_xiezuo, generic.

The original plan listed 11 platforms; the delta comes from regional splits
(e.g. Feishu vs Lark share an implementation but were enumerated separately)
and the addition of a generic adapter. WeChat Personal and Email/Webhook were
**not** in the shipped set; they remain possible future additions. Because the
Hub was dropped (see Plan 7), these adapters connect via outbound long
connections with zero server infrastructure.
```

- [ ] **Step 3: No commit yet.**

---

### Task 4: Update Plan 9 status (partial: updater done, OS codesign blocked)

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md`

> This task must reflect the auto-update plan's outcome. Run it after the auto-update plan ships `v0.3.1-nightly`, OR mark Plan 9 as "🟡 Partial" if the doc-sync ships first.

- [ ] **Step 1: Update the plan-list status line**

Find:
```
- Plan 9: [`2026-07-29-plan-9-os-packaging.md`](./2026-07-29-plan-9-os-packaging.md) — 🚫 Blocked (external deps)
```
Replace with:
```
- Plan 9: [`2026-07-29-plan-9-os-packaging.md`](./2026-07-29-plan-9-os-packaging.md) — 🟡 Partial (updater signed; OS codesign blocked)
```

- [ ] **Step 2: Update the Plan 9 section Status + add a note**

Change:
```
**Status:** 🚫 Blocked (external deps)
```
to:
```
**Status:** 🟡 Partial. Packaging pipeline builds all four targets and publishes
installers + SHA256SUMS. **Updater signing is enabled** (minisign keypair,
`v0.3.1-nightly`) so in-app silent update works. **OS code-signing remains
blocked**: macOS Developer ID notarization and Windows Authenticode require
purchasing certificates (Apple $99/yr; Windows OV/EV $100-700/yr).
```

After the **External Dependencies** line, insert:
```markdown
### What's done vs. what's left

- ✅ Cross-platform build pipeline (macOS ARM/x64, Windows x64, Linux x64).
- ✅ Installer formats (DMG, NSIS + portable zip, AppImage, .deb, .rpm) + SHA256SUMS.
- ✅ Updater artifacts + `latest.json` (signed, `v0.3.1-nightly`).
- ✅ Graceful degradation when signing secrets are absent.
- 🚫 macOS notarization (needs Apple Developer ID cert — `APPLE_*` secrets).
- 🚫 Windows Authenticode (needs OV/EV cert — `signtool` step not yet wired).
```

- [ ] **Step 3: No commit yet.**

---

### Task 5: Update the summary table

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md`

- [ ] **Step 1: Replace the summary table**

Find the `| Plan | Status | Complexity | External Deps |` table and replace its rows for 7/8/9 (and the trailing summary paragraph). Replace:

```
| 7. Remote Hub | 🚫 Blocked | High | Server infra |
| 8. Channels | 🚫 Blocked | Very High | Platform APIs |
| 9. OS Packaging | 🚫 Blocked | Very High | Signing certs |
| 10. 1.0 Acceptance | 🚫 Blocked | Medium | All platforms |

Plans 4-6 can be executed immediately with no external dependencies. Plans 7-10 require external infrastructure and should be prioritized based on available resources.
```

with:

```
| 7. Remote Hub | ✅ Shipped (Runtime Bridge) | High | None (Hub dropped) |
| 8. Channels | ✅ Shipped (14 adapters) | Very High | None |
| 9. OS Packaging | 🟡 Partial (updater signed; codesign pending) | Very High | Signing certs (codesign only) |
| 10. 1.0 Acceptance | 🚫 Blocked | Medium | Plan 9 codesign + test infra |

Plans 1-8 are complete. Plan 9's remaining work is OS code-signing certificates
(Apple/Windows). Plan 10 (1.0 acceptance) is blocked on Plan 9's code-signing
plus cross-platform testing infrastructure.
```

- [ ] **Step 2: No commit yet.**

---

### Task 6: Update the Plan 10 acceptance doc blocker list

**Files:**
- Modify: `docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md`

- [ ] **Step 1: Update the "External Dependencies (BLOCKING)" item 1**

Find (in the `## ⚠️ External Dependencies (BLOCKING)` section):
```
1. **All prior plans complete** — Plans 1-9 must be done. Plans 1-6 are ✅ Complete; Plans 7-9 are 🚫 Blocked on external deps.
```
Replace with:
```
1. **All prior plans complete** — Plans 1-8 are ✅ Complete (Plan 7 shipped as the Runtime Bridge, not a Hub; Plan 8 shipped 14 adapters). Plan 9 is 🟡 Partial: the updater is signed but OS code-signing (Apple/Windows certs) is still pending.
```

- [ ] **Step 2: Update the Status line under that section if present**

Find:
```
**Status:** 🚫 Blocked on Plans 7-9 + testing infrastructure. Outline only.
```
Replace with:
```
**Status:** 🚫 Blocked on Plan 9 OS code-signing + cross-platform testing infrastructure. Outline only.
```

- [ ] **Step 3: No commit yet.**

---

### Task 7: Commit the roadmap sync

- [ ] **Step 1: Stage the two changed files**

```bash
cd ~/Github/grok-app-main
git add docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md docs/superpowers/plans/2026-07-29-plan-10-1.0-acceptance.md
git status -s
```
Expected: two modified files staged.

- [ ] **Step 2: Commit**

```bash
git commit -m "docs: sync roadmap to shipped reality (Plan 7/8 done, Plan 9 partial)

Plan 7 shipped as the Remote IM Runtime Bridge (local gateway, not a Hub);
Plan 8 shipped 14 channel adapters; Plan 9 updater signing is enabled, OS
code-signing still blocked on certificates. Plan 10 blocker list updated."
```

- [ ] **Step 3: Merge to main and push**

```bash
git checkout main
git pull --ff-only origin main
git merge --no-ff docs/roadmap-sync -m "docs: sync roadmap to shipped reality"
git push origin main
git branch -d docs/roadmap-sync
```

- [ ] **Step 4: Verify the result reads cleanly**

```bash
sed -n '/^## Plan 7/,/^## Plan 8/p' docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md
grep -n 'Shipped\|🟡 Partial\|Runtime Bridge' docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md
```
Expected: Plan 7 section shows "✅ Shipped as the Remote IM Runtime Bridge", the grep lists the updated status markers, and no leftover `🚫 Blocked` for Plans 7/8.

---

## Verification (whole plan)

- [ ] Roadmap plan-list block: Plans 7/8 = ✅, Plan 9 = 🟡, Plan 10 = 🚫.
- [ ] Each of Plan 7/8/9 has a "How it shipped" / "What's done vs left" note explaining the deviation.
- [ ] Summary table rows for 7/8/9/10 match the plan-list statuses.
- [ ] Plan 10 acceptance doc's blocker item 1 and Status line reference Plan 9 codesign (not "Plans 7-9 blocked").
- [ ] Both files committed and merged to main; no code changed.
