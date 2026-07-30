# In-app Auto-update Enablement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flip the in-app updater from `github_manual` (open browser) to `silent` (download → install → relaunch) by generating the minisign keypair, dry-running the never-exercised signing path, configuring GitHub Secrets, and cutting a signed prerelease that produces a valid `latest.json`.

**Architecture:** The auto-update code is already 100% implemented (Rust `updater.rs`, `useUpdater.ts`, `build.rs` gate, `release.yml` signing path, helper scripts). This plan only *enables* it: generate a free minisign keypair, locally dry-run the secrets-present build path to catch latent bugs, upload secrets to the repo, and cut `v0.3.1-nightly` so CI runs the real four-platform signing + `assemble-updater-manifest.sh`. No code is expected to change unless the dry-run surfaces a bug.

**Tech Stack:** Tauri 2 updater plugin (`tauri-plugin-updater`), minisign keypair via `pnpm tauri signer generate`, GitHub Actions (`release.yml`), `gh` CLI for secrets + release inspection.

## Global Constraints

- **No Apple/Windows code signing** — out of scope (Plan 9 paid certs). macOS Gatekeeper / Windows SmartScreen warnings persist on the updated binary; accepted.
- **Private key never enters git, never printed.** `.gitignore` already covers `*.key` (and `~/.tauri/` is outside the repo). The pubkey is embedded in-app via env var, not committed.
- **Debug builds never enable the updater** — enforced by `src-tauri/build.rs` (`!cfg!(debug_assertions)`); do not weaken.
- **Two hard PAUSE points:** before uploading GitHub Secrets (Task 5) and before pushing the release tag (Task 7). Wait for explicit user confirmation.
- **Target version:** `0.3.1-nightly` (bump in `package.json:4`, `src-tauri/tauri.conf.json:4`, CHANGELOG, commit, tag `v0.3.1-nightly`).
- Linux `.deb`/`.rpm` stay `manual-required` by Tauri design; only AppImage auto-updates. Do not attempt to change this.
- Rolling endpoint is fixed: `https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json`

**Branch:** `feat/auto-update-enablement` (already created and has the design spec commit).

---

## File Map

- **Read-only (verify behavior, do not modify unless buggy):**
  - `src-tauri/src/updater.rs` — Rust updater commands + `prepare_for_app_update` teardown ordering.
  - `src-tauri/build.rs` — `cfg(omp_desktop_updater_enabled)` gate (lines 7-26).
  - `src/hooks/useUpdater.ts` — frontend state machine.
  - `.github/workflows/release.yml` — signing detection + assemble-updater (lines 129-233).
  - `scripts/build-release-config.mjs`, `scripts/assemble-updater-manifest.sh`, `scripts/verify-updater-setup.sh`, `scripts/generate-latest-json.sh`.
- **Modify (only if dry-run or verification finds a bug):** the above scripts/CI as needed.
- **Modify (docs/version, always):** `package.json`, `src-tauri/tauri.conf.json`, `CHANGELOG.md`, `docs/desktop-auto-update.md`.
- **Generated (gitignored, never commit):** `~/.tauri/omp-desktop.key`, `~/.tauri/omp-desktop.key.pub`, `src-tauri/tauri.release.conf.json`.

---

### Task 1: Generate the minisign keypair

**Files:**
- Create: `~/.tauri/omp-desktop.key` (private — outside repo, never committed)
- Create: `~/.tauri/omp-desktop.key.pub` (public — contents go to a GitHub Secret, not committed)

**Interfaces:**
- Produces: a minisign keypair. The **public key** string becomes the value of the `OMP_DESKTOP_UPDATER_PUBLIC_KEY` secret (Task 5). The **private key file contents** become `TAURI_SIGNING_PRIVATE_KEY`. A chosen password becomes `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

- [ ] **Step 1: Ensure the target dir exists**

```bash
mkdir -p ~/.tauri
```

- [ ] **Step 2: Generate the keypair with the Tauri signer**

Run (interactive — it will prompt for a password; choose one and record it securely outside the repo):
```bash
cd ~/Github/grok-app-main
pnpm tauri signer generate -w ~/.tauri/omp-desktop.key
```
Expected output: prints a public key like `CWt...` (long base64-ish string) and writes the private key to `~/.tauri/omp-desktop.key`.

- [ ] **Step 3: Capture the public key into a temp env var (this shell only — never echo the value into a file in the repo)**

```bash
OMP_DESKTOP_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/omp-desktop.key.pub)"
# Sanity: show length only, NOT the value
echo "pubkey length: ${#OMP_DESKTOP_UPDATER_PUBLIC_KEY}"
```
Expected: a length around 60-80 chars. Keep this shell session for later tasks, or re-read the `.pub` file when needed.

- [ ] **Step 4: Verify the private key is gitignored / outside the repo**

```bash
cd ~/Github/grok-app-main
git check-ignore -v ~/.tauri/omp-desktop.key 2>/dev/null || echo "outside repo (safe): ~/.tauri is not under $(pwd)"
# Also confirm *.key pattern is ignored if a key ever lands in-repo:
grep -n '\*.key' .gitignore
```
Expected: either "outside repo" message (the common case) or a matching `.gitignore` line. The repo must never track the private key.

- [ ] **Step 5: No commit needed** (no repo files changed; keypair lives outside the repo).

---

### Task 2: Dry-run the release config generator

**Goal:** Confirm `build-release-config.mjs` produces a correct `tauri.release.conf.json` when the env vars are present. This is the first never-run secrets-present step.

**Files:**
- Modify (generated, gitignored): `src-tauri/tauri.release.conf.json`

- [ ] **Step 1: Run the config generator with the env vars set**

```bash
cd ~/Github/grok-app-main
OMP_DESKTOP_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/omp-desktop.key.pub)" \
OMP_DESKTOP_UPDATER_ENDPOINT="https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json" \
node scripts/build-release-config.mjs
```
Expected stdout: `Updater enabled -> https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json`, a pubkey prefix preview, and `Wrote src-tauri/tauri.release.conf.json`.

- [ ] **Step 2: Inspect the generated config (must contain pubkey + endpoint + createUpdaterArtifacts)**

```bash
cat src-tauri/tauri.release.conf.json
```
Expected JSON shape:
```json
{
  "bundle": {
    "macOS": { "minimumSystemVersion": "11.0" },
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "pubkey": "<your public key>",
      "endpoints": ["https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json"]
    }
  }
}
```

- [ ] **Step 3: Verify it is gitignored**

```bash
git status --porcelain src-tauri/tauri.release.conf.json
```
Expected: empty output (the file is ignored). If it shows as untracked, **stop** — `.gitignore` line 33 must be fixing this; investigate before continuing.

- [ ] **Step 4: No commit** (generated artifact).

---

### Task 3: Local signed build (macOS only) to verify `.sig` artifact emission

**Goal:** The most important dry-run — confirm a real signed build produces the updater archive + `.sig`. macOS only (fast, native on this arm64 host). This validates `createUpdaterArtifacts` + `TAURI_SIGNING_*` actually work together.

**Files:** none in repo (build outputs go to `src-tauri/target`, gitignored).

**Interfaces:** Produces confidence that CI's per-platform `.app.tar.gz` + `.sig` will emit. If this fails, the bug is in the toolchain/config, not CI.

- [ ] **Step 1: Run a release build for macOS with signing env vars**

This compiles Rust in release mode — expect several minutes.
```bash
cd ~/Github/grok-app-main
OMP_DESKTOP_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/omp-desktop.key.pub)" \
OMP_DESKTOP_UPDATER_ENDPOINT="https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json" \
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/omp-desktop.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
pnpm tauri build --config src-tauri/tauri.release.conf.json
```
> If you set a password on the key in Task 1, pass it via `TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<pw>"` instead of empty.
Expected: build succeeds, finishing with bundle output lines listing `*.dmg` AND `*.app.tar.gz` + `*.app.tar.gz.sig`.

- [ ] **Step 2: Confirm the updater artifacts exist**

```bash
ls -la src-tauri/target/release/bundle/macos/*.app.tar.gz* 2>/dev/null || \
ls -la src-tauri/target/*/release/bundle/macos/*.app.tar.gz* 2>/dev/null || \
echo "NO .app.tar.gz FOUND — build did not emit updater artifacts"
```
Expected: both `OMP.Desktop_0.3.0-nightly_<arch>.app.tar.gz` and `OMP.Desktop_0.3.0-nightly_<arch>.app.tar.gz.sig` present.

- [ ] **Step 3: Sanity-check the .sig is non-empty and the pubkey matches**

```bash
SIGFILE="$(find src-tauri/target -name '*.app.tar.gz.sig' | head -1)"
echo "sig file: $SIGFILE"
wc -c "$SIGFILE"
# The .sig is a minisign signature block; confirm it references the pubkey alg
head -c 40 "$SIGFILE" | xxd | head -3
```
Expected: non-empty file; minisign header bytes present.

- [ ] **Step 4: If the build failed or no .sig was emitted — debug now**

Common causes and fixes:
- "failed to read signing key" → `TAURI_SIGNING_PRIVATE_KEY` env not set / empty; re-export from the `.key` file.
- No `.app.tar.gz` → `tauri.release.conf.json` missing `createUpdaterArtifacts: true` (revisit Task 2).
- Wrong arch → fine for a dry-run; CI covers all arches.

Fix the script/config in-repo and re-run Step 1. Commit any real fix:
```bash
git add <fixed file>
git commit -m "fix(updater): <what was wrong in the signing path>"
```

- [ ] **Step 5: Clean up local build artifacts (optional, frees disk)**

```bash
# Keep target/ if you'll reuse; otherwise:
# rm -rf src-tauri/target  # heavy — only if disk-constrained
```

---

### Task 4: Run the updater wiring verification script

**Goal:** `verify-updater-setup.sh` is the maintainer pre-flight check; run it locally with keys present.

- [ ] **Step 1: Run the verifier with keys in env**

```bash
cd ~/Github/grok-app-main
OMP_DESKTOP_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/omp-desktop.key.pub)" \
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/omp-desktop.key)" \
./scripts/verify-updater-setup.sh
```
Expected: `OK` for workflow references, scripts present, docs present, and "local env has OMP_DESKTOP_UPDATER_PUBLIC_KEY + TAURI_SIGNING_PRIVATE_KEY". `Summary: ok=N warn=* fail=0`, exit 0.

- [ ] **Step 2: If any FAIL — fix and re-run**

A `FAIL` here means a wiring gap (missing script, broken workflow reference). Fix in-repo, commit:
```bash
git add <file> && git commit -m "fix(updater): <gap found by verify-updater-setup>"
```

- [ ] **Step 3: No new commit if all OK.**

---

### Task 5: Configure GitHub Secrets

> ⛔ **HARD PAUSE — do not run until the user explicitly confirms.** This writes to the repo's Actions secrets.

**Goal:** Upload the three secrets so CI's "Detect updater secrets" step (release.yml:130) flips to `enabled=true`.

**Files:** none in repo (secrets live in GitHub).

- [ ] **Step 1: Confirm user authorization to upload secrets**

Ask the user explicitly: "Ready to upload the 3 minisign secrets to Po1nt9/omp-desktop? (OMP_DESKTOP_UPDATER_PUBLIC_KEY, TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD)". **Wait for yes.**

- [ ] **Step 2: Upload the public key secret**

```bash
gh secret set OMP_DESKTOP_UPDATER_PUBLIC_KEY --repo Po1nt9/omp-desktop < ~/.tauri/omp-desktop.key.pub
```
Expected: `✓ Set Actions secret OMP_DESKTOP_UPDATER_PUBLIC_KEY for Po1nt9/omp-desktop`

- [ ] **Step 3: Upload the private key secret**

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo Po1nt9/omp-desktop < ~/.tauri/omp-desktop.key
```
Expected: `✓ Set Actions secret TAURI_SIGNING_PRIVATE_KEY for Po1nt9/omp-desktop`

- [ ] **Step 4: Upload the password secret**

If you used an empty password in Task 1:
```bash
printf '' | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo Po1nt9/omp-desktop
```
If you used a real password, substitute it (do not let it hit shell history — prefer a prompt):
```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo Po1nt9/omp-desktop
# then paste the password when prompted, Ctrl-D to finish
```
Expected: `✓ Set Actions secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD for Po1nt9/omp-desktop`

- [ ] **Step 5: Confirm all three secrets exist (names only — gh never shows values)**

```bash
gh secret list --repo Po1nt9/omp-desktop
```
Expected: the three names appear. (Other pre-existing secrets may also show.)

- [ ] **Step 6: No commit** (no repo files changed).

---

### Task 6: Bump version to 0.3.1-nightly + CHANGELOG

**Files:**
- Modify: `package.json` (line 4 `"version"`)
- Modify: `src-tauri/tauri.conf.json` (line 4 `"version"`)
- Modify: `CHANGELOG.md` (add new section above `[0.3.0-nightly]`)

**Interfaces:** Produces version `0.3.1-nightly` so Task 7's tag matches what `release.yml` expects (`changelog-for-release.py` fails the build if `## [0.3.1-nightly]` is missing from CHANGELOG).

- [ ] **Step 1: Bump package.json**

Change `"version": "0.3.0-nightly",` → `"version": "0.3.1-nightly",` at `package.json:4`.

- [ ] **Step 2: Bump tauri.conf.json**

Change `"version": "0.3.0-nightly",` → `"version": "0.3.1-nightly",` at `src-tauri/tauri.conf.json:4`.

- [ ] **Step 3: Add CHANGELOG section**

Insert immediately after the header block (before `## [0.3.0-nightly]`):
```markdown
## [0.3.1-nightly] - 2026-07-31

### Changed / 变更

- **In-app auto-update enabled:** builds are now signed with a minisign
  updater key, so Settings → About "check for update" runs the silent
  download → install → relaunch path (was: open GitHub release page).
  The rolling `omp-desktop-latest` release now ships `latest.json` +
  per-platform `.sig` archives. 应用内自动更新已启用（minisign 签名）。

### Known Limitations / 已知限制

- Builds remain **not code-signed** by Apple / Windows (Plan 9, paid certs).
  The updater verifies archive integrity, but macOS Gatekeeper / Windows
  SmartScreen still warn on the updated binary. 更新包已签名校验，但 OS 代码签名仍未启用。
```

- [ ] **Step 4: Verify the changelog extractor finds the new section**

```bash
python3 scripts/changelog-for-release.py v0.3.1-nightly | head -5
```
Expected: prints the new section's first lines (no "section not found" error).

- [ ] **Step 5: Commit**

```bash
git add package.json src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: bump to 0.3.1-nightly + enable signed updater changelog"
```

---

### Task 7: Cut the signed release (v0.3.1-nightly)

> ⛔ **HARD PAUSE — do not push the tag until the user explicitly confirms.** This triggers a full four-platform CI build (~15-30 min) that publishes a public release.

**Goal:** Merge the branch to main and push `v0.3.1-nightly` so `release.yml` runs the real signing path end-to-end.

- [ ] **Step 1: Confirm authorization**

Tell the user: "About to merge `feat/auto-update-enablement` to main and push tag `v0.3.1-nightly`. This triggers four-platform CI (~15-30 min) and publishes a public GitHub release. Proceed?" **Wait for yes.**

- [ ] **Step 2: Ensure the branch is up to date and pushed**

```bash
cd ~/Github/grok-app-main
git status -sb        # confirm clean working tree on feat/auto-update-enablement
git log --oneline main..HEAD   # show what will merge
git push -u origin feat/auto-update-enablement
```

- [ ] **Step 3: Merge to main**

```bash
git checkout main
git pull --ff-only origin main
git merge --no-ff feat/auto-update-enablement -m "feat: enable signed in-app auto-update (0.3.1-nightly)"
git push origin main
```

- [ ] **Step 4: Tag and push**

```bash
git tag v0.3.1-nightly
git push origin v0.3.1-nightly
```
Expected: the push triggers the `release` workflow (visible at the repo's Actions tab).

- [ ] **Step 5: Watch the workflow start**

```bash
sleep 10
gh run list --repo Po1nt9/omp-desktop --workflow=release.yml --limit 3
```
Expected: a run for `v0.3.1-nightly` shows as `in_progress` / `queued`.

- [ ] **Step 6: Monitor to completion**

```bash
# Poll until done (non-blocking watch; ~15-30 min). Watch the most recent release run:
gh run watch --repo Po1nt9/omp-desktop $(gh run list --repo Po1nt9/omp-desktop --workflow=release.yml --limit 1 --json databaseId -q '.[0].databaseId') --exit-status
```
Expected: conclusion `success`. If any of the 4 build jobs or `assemble-updater` fails, see Task 8 debugging; the `checksums` job may still succeed.

> If a build job fails, do NOT delete the release yet — diagnose first (Task 8). A failed `assemble-updater` fails soft (installers still published).

---

### Task 8: Verify the release artifacts + latest.json

**Goal:** Confirm the signed release produced everything the updater needs.

- [ ] **Step 1: List the versioned release assets**

```bash
gh release view v0.3.1-nightly --repo Po1nt9/omp-desktop --json assets -q '.assets[].name'
```
Expected: 4 installers (`.dmg` ×2, `.exe`, `.AppImage`, `.deb`, `.rpm`, portable zip), `SHA256SUMS`, `latest.json`, **and** updater archives with `.sig`:
`OMP.Desktop_0.3.1-nightly_aarch64.app.tar.gz`, `OMP.Desktop_0.3.1-nightly_aarch64.app.tar.gz.sig`, `..._x64.app.tar.gz(.sig)`, `..._x64-setup.nsis.zip(.sig)`, `..._amd64.AppImage.tar.gz(.sig)` (exact names may vary — the point is `.sig` files exist).

- [ ] **Step 2: Confirm the rolling release has latest.json + archives**

```bash
gh release view omp-desktop-latest --repo Po1nt9/omp-desktop --json assets -q '.assets[].name'
```
Expected: `latest.json` plus the archive+sig pairs (clobbered onto this rolling release).

- [ ] **Step 3: Fetch and validate latest.json structure**

```bash
curl -sSL https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json -o /tmp/omp-latest.json
python3 -c '
import json, sys
d = json.load(open("/tmp/omp-latest.json"))
assert d["version"] == "0.3.1-nightly", f"version: {d.get(\"version\")}"
assert "notes" in d and d["notes"], "missing notes"
pub = d.get("pubkey", "")
assert pub and len(pub) > 20, "missing/short pubkey"
for plat, entry in d.get("platforms", {}).items():
    assert "url" in entry and "signature" in entry, f"{plat} missing url/signature"
    assert entry["url"].endswith((".tar.gz",".zip")) or "AppImage" in entry["url"], f"{plat} odd url"
print("OK platforms:", ", ".join(sorted(d["platforms"].keys())))
print("version:", d["version"])
'
```
Expected: `OK platforms: darwin-aarch64, darwin-x86_64, linux-x86_64, windows-x86_64` (subset acceptable if a platform failed, but all 4 is the goal) and `version: 0.3.1-nightly`.

- [ ] **Step 4: If a platform is missing or latest.json malformed — diagnose**

Likely cause: `assemble-updater-manifest.sh`'s `map_platform` didn't match an asset name (it keys off `aarch64`/`x64`/`x86_64`/`amd64` in the filename). Inspect:
```bash
gh release view v0.3.1-nightly --repo Po1nt9/omp-desktop --json assets -q '.assets[].name' | grep -E '\.sig$'
```
Compare names against the `map_platform` cases in `scripts/assemble-updater-manifest.sh:43-74`. If an arch token is absent/misnamed, the fix is either renaming in CI or extending `map_platform` + re-running assemble locally:
```bash
TAG=v0.3.1-nightly REPO=Po1nt9/omp-desktop bash scripts/assemble-updater-manifest.sh
```
Commit any `map_platform` fix and note it may require a re-tag or manual re-assemble (no need to rebuild installers).

- [ ] **Step 5: Confirm the live verifier now passes with --fetch-latest**

```bash
cd ~/Github/grok-app-main
OMP_DESKTOP_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/omp-desktop.key.pub)" \
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/omp-desktop.key)" \
./scripts/verify-updater-setup.sh --fetch-latest
```
Expected: `OK latest.json fetchable and valid JSON`, `fail=0`.

---

### Task 9: Update the auto-update doc status line

**Files:**
- Modify: `docs/desktop-auto-update.md` (lines 7-11, the "Current status" block)

- [ ] **Step 1: Replace the status block**

Find (lines 7-11):
```markdown
> **Current status:** the updater **plumbing is wired but not yet signed**.
> Builds produced without the `OMP_DESKTOP_UPDATER_*` signing secrets fall back
> to the GitHub "open release page" path — no in-app auto-update. See
> [Signing requirements](./release/signing-requirements.md) for the certificate
> blocker.
```
Replace with:
```markdown
> **Current status:** the updater is **signed and live** as of `v0.3.1-nightly`.
> Settings → About "check for update" runs the silent download → install →
> relaunch path. macOS Gatekeeper / Windows SmartScreen still warn because OS
> code-signing (Apple Developer ID / Authenticode) is not yet configured — see
> [Signing requirements](./release/signing-requirements.md) for that remaining
> Plan 9 work. The minisign keypair verifies *archive integrity*, independent of
> OS trust.
```

- [ ] **Step 2: Commit on main (doc-only follow-up)**

```bash
git checkout main
git pull --ff-only origin main
git add docs/desktop-auto-update.md
git commit -m "docs: mark auto-update signed and live as of v0.3.1-nightly"
git push origin main
```

- [ ] **Step 3: Verify**

```bash
git show --stat HEAD
```
Expected: single file changed, `docs/desktop-auto-update.md`.

---

## Verification (whole plan)

After Task 9, the end state:
- [ ] Three minisign secrets configured on `Po1nt9/omp-desktop`.
- [ ] `v0.3.1-nightly` released with `.sig` artifacts on all 4 platforms.
- [ ] `omp-desktop-latest` has a valid `latest.json` (4 platforms, version `0.3.1-nightly`).
- [ ] `verify-updater-setup.sh --fetch-latest` passes.
- [ ] `docs/desktop-auto-update.md` reflects "signed and live".
- [ ] CHANGELOG has the `0.3.1-nightly` section.
- [ ] No private key material in git history (`git log --all -p | grep -F "$(cat ~/.tauri/omp-desktop.key | head -1)"` returns nothing).

## Risks recap

| Risk | Mitigation (task) |
|---|---|
| `build-release-config.mjs` / signing path has a latent bug | Dry-run Tasks 2-4 before touching the repo |
| `assemble-updater-manifest.sh` `map_platform` misses an arch | Task 8 Step 4 diagnosis + local re-assemble |
| Tauri `createUpdaterArtifacts` format quirk per platform | macOS dry-run (Task 3) + CI matrix covers the rest |
| Secret misconfigured | Task 5 Step 5 `gh secret list` |
| Live update can't be fully tested (no prior signed build) | `latest.json` structural validation (Task 8 Step 3) is the floor; live update proven on the first real user upgrade from 0.3.1 → next |
