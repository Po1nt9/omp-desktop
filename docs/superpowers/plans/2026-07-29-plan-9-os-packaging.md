# Plan 9: OS Packaging and Updates — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use `- [ ]` checkbox syntax for tracking.

**Goal:** Produce signed, auto-updating installer packages for macOS (Universal), Windows (x64), and Linux (AppImage + .deb + .rpm), with in-app auto-update over signed release artifacts.

**Status:** 🟡 **CI scaffolding complete; unsigned pipeline verified; signing blocked on certificates.** The release infrastructure (`.github/workflows/release.yml`, 14 scripts, updater plumbing) is in place and produces installers for all four targets on a `v*` tag. The **only** remaining blocker is code-signing certificates (Apple Developer ID + Windows Authenticode) — a purchase/approval dependency, not an engineering one.

---

## Architecture

OMP Desktop ships **without a bundled Runtime**. The app is a desktop shell; the
OMP Runtime CLI is supplied by the user (Settings → Manual CLI path, read at
`session_manager.rs:2642` from `settings.manual_cli_path`). The `runtime/oh-my-pi`
submodule exists for **development-time** code sharing only — it is not packaged
into the bundle. This keeps the app and Runtime independently upgradeable.

Tauri's build pipeline (`tauri build`) produces the base installers;
`tauri-plugin-updater` handles auto-update checks against a rolling
`latest.json` manifest hosted on GitHub Releases. Update archives are signed
with a minisign keypair; the OS code-signing layer (macOS codesign/notarytool,
Windows signtool) satisfies Gatekeeper/SmartScreen separately.

See [`docs/desktop-auto-update.md`](../../desktop-auto-update.md) for the full
updater architecture, and [`docs/release/signing-requirements.md`](../../release/signing-requirements.md)
for the certificate inventory.

---

## ⚠️ External Dependencies (BLOCKING — signing only)

The original "outline only" status was driven by four listed dependencies. Three
of them are **already resolved**:

1. ~~**Code signing certificates**~~ — ✅ **STILL THE ONLY BLOCKER.**
   - macOS: Apple Developer ID Application certificate + notarization (USD $99/yr).
   - Windows: Authenticode OV/EV certificate (~$100–$700/yr).
   - Linux: none needed.
2. ~~**CI/CD infrastructure**~~ — ✅ **Resolved.** `.github/workflows/release.yml` runs on GitHub Actions runners for all four targets (macOS ARM, macOS x64, Windows x64, Linux x64).
3. ~~**Update manifest hosting**~~ — ✅ **Resolved.** GitHub Releases hosts `latest.json` under the rolling `omp-desktop-latest` tag (wired in `assemble-updater-manifest.sh`).
4. ~~**OMP Runtime release binary**~~ — ✅ **N/A.** The app does not bundle the Runtime (see Architecture above). Users supply their own CLI.

---

## Scope

- ✅ macOS Universal build (Intel + ARM) — matrix builds both targets
- ✅ Windows x64 build — NSIS installer + portable zip (绿色版)
- ✅ Linux build — AppImage + .deb + .rpm
- ✅ Updater plumbing — `build-release-config.mjs`, `assemble-updater-manifest.sh`, `generate-latest-json.sh`, `verify-updater-setup.sh`
- ✅ Supply-chain floor — SHA256SUMS over all published assets, brand-policy & provenance checks
- 🚫 Code signing — macOS (notarization) and Windows (Authenticode): **blocked on certificates**
- 🚫 In-app auto-update activation — requires updater signing secrets; **blocked on keypair generation** (free, but deferred until signing is holistic)
- 🚫 Update channels (stable/beta/nightly) — deferred post-1.0; single channel first

## Completed Tasks

- [x] Configure Tauri build for each platform — `src-tauri/tauri.conf.json` + per-platform overrides (`tauri.macos.conf.json`, `tauri.windows.conf.json`).
- [x] CI release workflow — `.github/workflows/release.yml` (4-target matrix, tauri-action, changelog-driven release body, graceful updater degradation when secrets absent).
- [x] CI smoke workflow — `.github/workflows/ci.yml` (frontend typecheck/test/build + Rust cargo test on 3 platforms).
- [x] Updater manifest pipeline — `generate-latest-json.sh` + `assemble-updater-manifest.sh` produce rolling `omp-desktop-latest/latest.json`.
- [x] Updater verification — `verify-updater-setup.sh` checks wiring without printing secrets.
- [x] Auto-update documentation — `docs/desktop-auto-update.md`.
- [x] Signing requirements documentation — `docs/release/signing-requirements.md`.
- [x] README install & first-run guide (bilingual EN/中文) — `README.md`, `README_ZH.md`.
- [x] Supply-chain checksums — `SHA256SUMS` job publishes a hash over every asset.
- [x] Windows portable zip — `scripts/package-windows-portable.sh`.

## Remaining Tasks (blocked on certificates)

- [ ] **Set up macOS code signing** — wire `APPLE_*` secrets into `release.yml`; configure `tauri.conf.json` `macOS.signingIdentity`. Unsigned path already degrades gracefully (do **not** pass empty `APPLE_CERTIFICATE`).
- [ ] **Set up Windows code signing** — obtain Authenticode cert; add `sign-windows.sh` using `signtool`; wire `windows.certificateThumbprint` into `tauri.conf.json`. (Not yet scripted — the release pipeline only wires Tauri updater signing today.)
- [ ] **Generate updater keypair** — `pnpm tauri signer generate`; set `OMP_DESKTOP_UPDATER_PUBLIC_KEY` + `TAURI_SIGNING_PRIVATE_KEY` as repo secrets.
- [ ] **Test update flow** — end-to-end: install stable → publish new version → verify in-app auto-update notification → update + relaunch.
- [ ] **Activate update channels** — stable/beta/nightly manifest URLs + per-channel signing keys.

## Existing Infrastructure to Reuse

- `.github/workflows/ci.yml` — 3-platform CI smoke
- `.github/workflows/release.yml` — 4-target release build + publish
- `scripts/build-local.sh` — local build helper (mac-arm/mac-intel/win/linux)
- `scripts/build-release-config.mjs` — writes `tauri.release.conf.json` (updater delta)
- `scripts/generate-latest-json.sh` — update manifest generator (single-platform)
- `scripts/assemble-updater-manifest.sh` — multi-platform manifest assembler (downloads assets, maps platforms, writes rolling `latest.json`)
- `scripts/verify-updater-setup.sh` — updater wiring verifier
- `scripts/release-tag.sh` — version-sync + annotated tag helper
- `scripts/setup-cross-compile.sh` — cross-compile toolchain setup
- `scripts/package-windows-portable.sh` — Windows green/绿色版 packaging
- `scripts/changelog-for-release.py` — extracts CHANGELOG section for Release body
- `scripts/check-brand-policy.mjs`, `check-provenance.mjs`, `check-i18n-completeness.mjs` — supply-chain gates
- `tauri-plugin-updater` — dependency
