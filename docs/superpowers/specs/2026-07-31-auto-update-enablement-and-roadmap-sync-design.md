# Auto-update enablement & roadmap sync — Design

**Date:** 2026-07-31
**Scope:** Two independent deliverables bundled because both finish the v0.3.0-nightly follow-up.
**Repo:** Po1nt9/omp-desktop (working dir `~/Github/grok-app-main`)

---

## Background & key finding

An audit of the auto-update stack (8 files) establishes that **in-app auto-update is fully implemented, not pending development.** Every layer is wired:

| Layer | Status | Evidence |
|---|---|---|
| Rust updater | ✅ complete | `src-tauri/src/updater.rs` — `is_auto_update_supported`, `is_updater_plugin_enabled`, `updater_status`, `prepare_for_app_update` (teardown only after successful `install()`), 5 unit tests |
| Frontend state machine | ✅ complete | `src/hooks/useUpdater.ts` (check→download→install→relaunch + GitHub fallback), `UpdaterProvider.tsx`, wired into `SettingsPage.tsx:3225-3327` |
| Build gate | ✅ | `src-tauri/build.rs` → `cfg(omp_desktop_updater_enabled)` when both `OMP_DESKTOP_UPDATER_*` env vars present; debug builds never enable |
| CI pipeline | ✅ | `.github/workflows/release.yml:129-233` — secret detection → signed build → `assemble-updater` merges `latest.json`; graceful degradation when secrets absent |
| Scripts | ✅ | `verify-updater-setup.sh`, `assemble-updater-manifest.sh`, `build-release-config.mjs`, `generate-latest-json.sh` |
| Docs | ✅ | `docs/desktop-auto-update.md` (154 lines), `docs/release/signing-requirements.md` (101 lines) |

**The only missing thing is the minisign keypair** (`OMP_DESKTOP_UPDATER_PUBLIC_KEY` + `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) in the repo's GitHub Secrets. This is free and immediately generatable.

**Therefore:** "completing auto-update" is an **enablement + verification** task, not a coding task. The real engineering risk is that the **"secrets-present" path has never run on real CI** — v0.3.0-nightly only exercised the "secrets-absent → graceful degradation" path.

The three untested risk points:
1. **Signed CI build**: `build-release-config.mjs` produces `tauri.release.conf.json` with `createUpdaterArtifacts: true`; Tauri emits `.app.tar.gz` / `.nsis.zip` / `.AppImage` + `.sig` per platform. Filename-arch matching in `map_platform()` is the fragile point.
2. **`assemble-updater-manifest.sh`**: never executed against a real release; downloads assets, pairs archive+sig, generates `latest.json`, clobbers upload to `omp-desktop-latest`.
3. **End-to-end**: an older build's `check()` hits `latest.json` → signature verifies → download → install → teardown → relaunch.

---

## Decision record

Three decisions captured during brainstorming:

1. **Update scope = updater signing only (minisign, free).** Apple Developer ID ($99/yr) and Windows Authenticode ($100-700/yr) are explicitly **out of scope** — they remain Plan 9's paid certificate work. macOS Gatekeeper / Windows SmartScreen warnings persist on the (correctly auto-updated) binary; that is accepted.
2. **Execution = agent does it end-to-end**, pausing before (a) writing GitHub Secrets, and (b) cutting the signed release. Local `gh` CLI must be authenticated with admin rights on the repo.
3. **Final verification = a real prerelease tag** (e.g. `v0.3.1-nightly`) so the full four-platform CI runs the real signing + `latest.json` path.

---

## Deliverable A: Enable in-app auto-update

### Goal
Settings → About "check for update" moves from `github_manual` (opens browser) to `silent` (in-app download → install → relaunch), verified end-to-end.

### Steps (pause before secret upload AND before tagging release)

1. **Generate keypair** — `pnpm tauri signer generate -w ~/.tauri/omp-desktop.key`. Private key file never enters git (already covered by `.gitignore` patterns; verify). Public key → `OMP_DESKTOP_UPDATER_PUBLIC_KEY`.
2. **Local dry-run (no CI, no repo writes)** — exposes bugs in the never-run secrets-present path before burning a release:
   - `node scripts/build-release-config.mjs` with the env vars set → inspect generated `src-tauri/tauri.release.conf.json`.
   - `pnpm tauri build --config src-tauri/tauri.release.conf.json` for **macOS only** (fast) → confirm it produces `*.app.tar.gz` + `*.sig`.
   - `./scripts/verify-updater-setup.sh` → confirm wiring.
3. **Fix any bugs surfaced by the dry-run** (in code / scripts / CI). Commit.
4. **Configure GitHub Secrets** — `gh secret set OMP_DESKTOP_UPDATER_PUBLIC_KEY`, `gh secret set TAURI_SIGNING_PRIVATE_KEY`, `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. **PAUSE for user confirmation before this step.**
5. **Cut signed release** — tag `v0.3.1-nightly` (bump `package.json` + `src-tauri/tauri.conf.json` + CHANGELOG first). Triggers four-platform CI + `assemble-updater-manifest.sh`. **PAUSE for user confirmation before pushing the tag.**
6. **End-to-end verification**:
   - `omp-desktop-latest` release contains `latest.json` + 4 platform archive+sig pairs.
   - `curl` the `latest.json`, validate JSON structure (version, platforms, signature urls).
   - Run `assemble-updater-manifest.sh` logic check mentally against actual asset names — confirm `map_platform` matched all four.
   - If a prior signed build is available: Settings → About shows `silent` channel → check → download → install → restart → version matches tag.
7. **Update docs** — `desktop-auto-update.md` "Current status" line flips to signed-and-live; CHANGELOG gains a 0.3.1 entry.

### Constraints
- No Apple/Windows code signing.
- Private key never in git, never printed.
- Debug builds never enable updater (`build.rs` already enforces).
- Linux non-AppImage (`.deb`/`.rpm`) stays `manual-required` by design.

### Out of scope
- Apple notarization, Windows Authenticode (Plan 9).
- Auto-update for `.deb`/`.rpm` (Tauri limitation).
- Changing the rolling endpoint URL or release-tagging scheme.

---

## Deliverable B: Sync the roadmap docs to reality

### Problem
`docs/superpowers/plans/2026-07-29-plans-4-10-roadmap.md` is stale: it marks Plan 7 (Remote Hub) and Plan 8 (Channels) as 🚫 Blocked, but P1–P3 + the Remote IM Runtime Bridge already delivered both — just **not** via the originally-envisioned Hub architecture. They landed as direct Runtime bridging + 14 channel adapters.

### Steps
1. **Update `plans-4-10-roadmap.md`**:
   - Plan 7 → ✅, with a note that the Hub architecture was dropped in favor of direct Runtime bridging (per-`work_dir` AcpClient pool + drain barrier + 3-layer concurrency locks). Reference the design decision already recorded in the Remote Hub architecture analysis.
   - Plan 8 → ✅, noting 14 adapters shipped (the original plan listed 11 platforms; some split by region, plus a generic adapter).
   - Plan 9 → partially done: updater signing done after Deliverable A; OS code-signing still Blocked on purchasing certificates.
   - Fix the summary table at the bottom.
2. **Update Plan 10 acceptance doc** if its blocker list references Plan 7-9 status (keep it Blocked, but reflect that 7/8 landed and only 9's paid certs + cross-platform test infra remain).
3. Keep edits factual and concise — document *what shipped vs. what was planned and why*, no puffery.

### Out of scope
- Rewriting other plan docs (verification docs, individual plan files) beyond status-line fixes.
- Changing the master design spec.

---

## Git management

- All work on a feature branch off `main` (e.g. `feat/auto-update-enablement`), not directly on `main`.
- Small, focused commits: dry-run fixes, doc updates, version bump, CHANGELOG as separate commits where sensible.
- Final merge to `main` after verification; tag `v0.3.1-nightly` from `main`.
- Private key, `tauri.release.conf.json` (gitignored), and any local build artifacts never committed.

---

## Success criteria

- [ ] `OMP_DESKTOP_UPDATER_*` secrets configured in repo.
- [ ] `v0.3.1-nightly` release built by CI with `.sig` artifacts on all 4 platforms.
- [ ] `omp-desktop-latest` release has a valid `latest.json` referencing all 4 platforms with signatures.
- [ ] `docs/desktop-auto-update.md` status line reflects "signed and live".
- [ ] `plans-4-10-roadmap.md` matches reality (Plan 7/8 ✅, Plan 9 partially).
- [ ] CHANGELOG has a 0.3.1 entry.
- [ ] No secrets in git history; branch merged to main; tag pushed.

## Risks

| Risk | Mitigation |
|---|---|
| `assemble-updater-manifest.sh` has a latent bug never hit before | Local dry-run + inspect; fail-soft in CI (`::warning::`, release installers still publish) |
| `map_platform` misses a platform due to asset naming | Inspect actual asset names in the dry-run; use `PLATFORM_HINTS` if heuristic fails |
| Tauri `createUpdaterArtifacts` format differs by platform | Dry-run on macOS first; CI matrix covers the rest |
| Secret misconfigured | `gh secret list` to confirm presence (never values); `verify-updater-setup.sh` |
| End-to-end check can't be fully run (no prior signed build) | At minimum validate `latest.json` structure + signature URLs; defer live update test to first user on a signed build |
