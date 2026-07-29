# Plan 9: OS Packaging and Updates — Implementation Plan Outline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]` syntax for tracking.

**Goal:** Produce signed, auto-updating installer packages for macOS (Universal), Windows (x64), and Linux (AppImage + .deb + .rpm), with the OMP Runtime binary bundled and co-signed, and three update channels (stable, beta, nightly).

**Architecture:** Tauri's build pipeline produces the base installers; `tauri-plugin-updater` handles auto-update checks against a manifest server. The OMP Runtime binary is bundled as a resource in the app bundle and co-signed alongside the main binary. Update channels are isolated by config/cache/signing — each channel has its own manifest URL, signing key, and update feed.

**Tech Stack:** Tauri CLI (`tauri build`), `tauri-plugin-updater`, platform signing tools (`codesign`/`notarytool` for macOS, `signtool` for Windows), `appimagetool`/`dpkg-deb`/`rpmbuild` for Linux.

---

## ⚠️ External Dependencies (BLOCKING)

This plan **cannot be executed** until the following external resources are available:

1. **Code signing certificates** —
   - macOS: Apple Developer ID Application certificate + notarization service account.
   - Windows: Authenticode code signing certificate (EV or OV).
   - Linux: no signing certificate needed (AppImage is unsigned; .deb/.rpm can be GPG-signed optionally).
2. **CI/CD infrastructure** — build runners for macOS (Universal), Windows x64, and Linux. GitHub Actions runners can cover all three, but macOS Universal requires a custom runner or two runners (Intel + ARM) with `lipo`.
3. **Update manifest hosting** — a static file server (or GitHub Releases) for `latest.json` update manifests per channel.
4. **OMP Runtime release binary** — a pre-built, versioned OMP Runtime binary for each target platform, downloadable at build time.

**Status:** 🚫 Blocked on signing certificates and CI/CD. Outline only.

---

## Scope

- macOS Universal build (Intel + ARM via `lipo`)
- Windows x64 build (ARM64 deferred post-1.0)
- Linux build (AppImage + .deb + .rpm)
- Code signing for macOS (notarization) and Windows (Authenticode)
- Auto-update channels: stable, beta, nightly
- OMP Runtime bundling and co-signing

## High-Level Tasks (detailed TDD steps deferred until deps available)

1. **Configure Tauri build for each platform** — `src-tauri/tauri.conf.json` per-platform settings (bundle targets, icons, publisher info).
2. **Bundle OMP Runtime binary** — download script (`scripts/fetch-omp-runtime.sh`) that fetches the versioned runtime binary for the target platform and places it in `src-tauri/resources/`.
3. **Set up macOS code signing** — `scripts/sign-macos.sh` using `codesign` + `notarytool`; configure `tauri.conf.json` with `macos.signingIdentity`.
4. **Set up Windows code signing** — `scripts/sign-windows.sh` using `signtool`; configure `tauri.conf.json` with `windows.certificateThumbprint`.
5. **Configure update channels** — `src-tauri/tauri.conf.json` `updater` section with per-channel manifest URLs; `scripts/generate-latest-json.sh` already exists and produces the manifest.
6. **Test update flow** — end-to-end: install stable → publish beta → verify auto-update notification → update to beta.
7. **Create installer packages** — `.dmg` (macOS), `.msi`/`.exe` (Windows), `.AppImage`/`.deb`/`.rpm` (Linux).

## Preparation Work (can be done NOW without external deps)

- Audit existing `scripts/build-local.sh`, `scripts/build-release-config.mjs`, `scripts/generate-latest-json.sh`, `scripts/assemble-updater-manifest.sh`, `scripts/verify-updater-setup.sh` — these already exist and handle parts of the build/release pipeline. Document what they do and identify gaps.
- Audit `src-tauri/tauri.conf.json` for current updater config.
- Write `scripts/fetch-omp-runtime.sh` (download stub — the runtime binary URL isn't available yet, but the script structure can be written).
- Document the signing certificate requirements in `docs/release/signing-requirements.md`.

## Existing Infrastructure to Reuse

- `scripts/build-local.sh` — local build helper for mac/win/linux
- `scripts/build-release-config.mjs` — release config
- `scripts/generate-latest-json.sh` — update manifest generator
- `scripts/assemble-updater-manifest.sh` — updater manifest assembler
- `scripts/verify-updater-setup.sh` — updater setup verifier
- `scripts/release-tag.sh` — release tag helper
- `scripts/setup-cross-compile.sh` — cross-compile setup
- `scripts/package-windows-portable.sh` — Windows portable packaging
- `tauri-plugin-updater` — already in dependencies (`package.json` line 46)
