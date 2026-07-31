# Desktop auto-update

OMP Desktop uses the **Tauri 2 updater**: signed release artifacts, per-channel
rolling manifest endpoints, in-app check/download/install, and a hard stop of
managed agent / Remote IM processes before the binary swap.

> **Current status:** the updater is **signed and live** as of `v0.3.1-nightly`,
> with **three isolated update channels** (stable / beta / nightly, AC-10.9).
> Settings → About "check for update" runs the silent download → install →
> relaunch path, and each channel's rolling release ships its manifest +
> per-platform `.sig` archives for all four targets. macOS Gatekeeper / Windows
> SmartScreen still warn because OS code-signing (Apple Developer ID /
> Authenticode) is not yet configured — see
> [Signing requirements](./release/signing-requirements.md) for that remaining
> Plan 9 work. The minisign keypair verifies *archive integrity*, independent of
> OS trust.

## OS trust warnings while unsigned

Until OS code-signing lands (Plan 9), expect — and verify past — the platform
warnings:

| Platform | Unsigned consequence | User workaround |
|----------|---------------------|-----------------|
| macOS | Gatekeeper blocks launch ("unidentified developer") | Right-click → Open, or `xattr -dr com.apple.quarantine /Applications/OMP\ Desktop.app` |
| Windows | SmartScreen "Windows protected your PC" | "More info" → "Run anyway" |
| Linux | None (AppImage is unsigned by design) | — |

Signing costs (details + secret inventory in
[Signing requirements](./release/signing-requirements.md)): Apple Developer
Program **USD $99/year**; Windows Authenticode OV roughly **$100–300/year**,
EV roughly **$300–700/year**; the SignPath Foundation offers a free tier for
open-source projects.

## Update channels (AC-10.9)

The channel is a **build-time identity**, derived from the version string
(prerelease suffix): `1.1.0-nightly.20260801` → nightly, `1.1.0-beta.1` → beta,
`1.0.0` → stable. There is **no runtime switcher** — users pick a channel by
installing that channel's build (Chrome / VS Code model). The same derivation
drives CI (tag suffix → endpoint + GitHub prerelease flag), the baked updater
endpoint, and the manual GitHub fallback, so a build can never cross channels.

| Channel | Rolling release | Manifest | GitHub prerelease |
|---------|-----------------|----------|-------------------|
| stable | `omp-desktop-latest` | `latest.json` | no |
| beta | `omp-desktop-beta` | `beta.json` | yes |
| nightly | `omp-desktop-nightly` | `nightly.json` | yes |

Cut a channel release by tagging its version: `pnpm release:tag 1.1.0-beta.1`
or `…-nightly.YYYYMMDD`. CI derives the channel from the tag
(`scripts/update-channel-lib.mjs`), bakes the matching endpoint, and marks
beta/nightly releases as GitHub prereleases (keeps `/releases/latest`
stable-only).

**Transitional dual-publish (D5):** until the first stable release ships,
nightly tags also publish `latest.json` to `omp-desktop-latest`
(`DUAL_PUBLISH_LEGACY_LATEST=1` in `release.yml`) so installed
v0.3.x-nightly builds — whose baked endpoint is the legacy stable feed — keep
receiving updates. Delete that flag at the 1.0 stable cut.

All three channels share one minisign signing keypair (a channel switch always
means installing a different signed build anyway).

## Architecture

```
CI release (tag vX.Y.Z[-suffix])
  ├── vX.Y.Z[-suffix]        user-facing installers (DMG / AppImage / NSIS …)
  └── omp-desktop-{latest|beta|nightly}   rolling updater release, per channel
        └── {latest|beta|nightly}.json  + per-platform archive + .sig
                 ▲
                 │ check()   (endpoint baked per channel at build time)
        Desktop  tauri-plugin-updater  (release builds only)
                 │ prepare_for_app_update → stop agents / Remote IM
                 │ install + relaunch
        UI: Settings → About (shows delivery mode + release channel)
```

Unsigned / local builds keep the previous GitHub "open release page" path — the
`updater` crate is always linked (Tauri ACL requirement) but only **registered**
when the build-time cfg is set. The manual path is channel-aware: beta/nightly
builds list recent releases and pick the newest tag their channel owns with
prerelease-aware semver ordering (`select_release_for_channel`).

## App pieces

| Piece | Location |
|-------|----------|
| Build-time gate | `src-tauri/build.rs` → `cfg(omp_desktop_updater_enabled)` when both `OMP_DESKTOP_UPDATER_*` env vars are set (crate always linked for ACL) |
| Channel identity | `src-tauri/src/update_channel.rs` — `UpdateChannel::from_version(env!("CARGO_PKG_VERSION"))` |
| Channel derivation (CI) | `scripts/update-channel-lib.mjs` — tag → channel / endpoint / prerelease flag |
| Release conf delta | `scripts/build-release-config.mjs` → `src-tauri/tauri.release.conf.json` (gitignored — always regenerate) |
| Platform support | `is_auto_update_supported` — Linux requires AppImage (`APPIMAGE` env) |
| Pre-relaunch teardown | `prepare_for_app_update` — **only after** successful `install()`, never before |
| Capabilities | `updater:allow-*`, `process:allow-restart` |

Local `pnpm dev` / debug builds **never** enable the updater plugin (no
feature, no env), so dev binaries never hit a production endpoint.

### Install / teardown order (P0)

```
download → install() → prepare_for_app_update() → relaunch()
```

If `install()` fails, agents / Remote IM stay running.

## Secrets (GitHub Actions)

| Secret / variable | Purpose |
|-------------------|---------|
| `OMP_DESKTOP_UPDATER_PUBLIC_KEY` | minisign public key embedded in the app |
| `TAURI_SIGNING_PRIVATE_KEY` | minisign private key for signing updater archives |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password for the private key (empty string OK) |
| Apple signing / notarize secrets | codesign + notarize DMG / .app (recommended for macOS Gatekeeper) — see [signing requirements](./release/signing-requirements.md) |

Generate a keypair once (see [Tauri updater](https://v2.tauri.app/plugin/updater/)):

```sh
pnpm tauri signer generate -w ~/.tauri/omp-desktop.key
# public key → OMP_DESKTOP_UPDATER_PUBLIC_KEY
# private key file contents → TAURI_SIGNING_PRIVATE_KEY
```

### Maintainer production checklist

Before treating silent update as "on" for users:

1. **Secrets in the shipping repo** (Settings → Secrets and variables → Actions): all rows above that you use. Empty `TAURI_SIGNING_PRIVATE_KEY` must not be set — omit or set a real key.
2. **Local dry-run (no secret values printed):**
   ```sh
   ./scripts/verify-updater-setup.sh
   ./scripts/verify-updater-setup.sh --fetch-latest
   OMP_DESKTOP_UPDATE_CHANNEL=nightly ./scripts/verify-updater-setup.sh --fetch-latest
   ```
3. **Release cut:** tag `vX.Y.Z[-suffix]` so CI builds installers **and** refreshes the channel's rolling release + manifest + `.sig` (`-beta.N` / `-nightly.*` tags land on their own channels and are flagged prerelease).
4. **Smoke on a prior signed build:** Settings → About shows the in-app (signed release) channel → Check → Download → Install and restart → version matches tag.
5. **Failure path:** if install fails, agents / Remote IM must keep running (`prepare_for_app_update` only after successful `install()`).
6. **Unsigned / local builds:** About must show the GitHub download channel and still open Release / download installer (no crash).

## Rolling endpoints

```text
stable:  https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json
beta:    https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-beta/beta.json
nightly: https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-nightly/nightly.json
```

Publish two GitHub releases per cut:

1. **`vX.Y.Z[-suffix]`** — human installers + notes (prerelease flag derived from the suffix)
2. **the channel's rolling release** — updater archives + manifest (clobber each release)

## Build steps (outline)

```sh
export OMP_DESKTOP_UPDATER_PUBLIC_KEY=...
# Endpoint: set OMP_DESKTOP_UPDATER_ENDPOINT explicitly, or let it derive from
# GITHUB_REPOSITORY + OMP_DESKTOP_RELEASE_VERSION (channel from version suffix).
export OMP_DESKTOP_UPDATER_ENDPOINT=https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-nightly/nightly.json
export TAURI_SIGNING_PRIVATE_KEY=...
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=...

# 1) Write tauri.release.conf.json (gitignored — required before --config)
node scripts/build-release-config.mjs

# 2) Same OMP_DESKTOP_UPDATER_* env must still be set so build.rs enables registration
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

Without step 1, `tauri build --config src-tauri/tauri.release.conf.json` fails with
file not found. The crate is always a hard dependency (Tauri ACL); only
**registration** is gated by the env cfg.

After all platforms upload assets to `vX.Y.Z`:

```sh
TAG=v0.3.0 REPO=Po1nt9/omp-desktop CHANNEL=stable bash scripts/assemble-updater-manifest.sh
```

`CHANNEL` defaults to `stable`; CI passes the tag-derived channel. Nightly runs
also set `DUAL_PUBLISH_LEGACY_LATEST=1` (transitional, see above).

Platform keys: `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `windows-x86_64`.

## Unsigned / community builds (fallback)

The official repo ships signed (see status above). For community forks or
builds where the updater secrets are **absent**, `release.yml` detects this via
the `Detect updater secrets` step and:

- Does **not** write `tauri.release.conf.json`.
- Passes no `--config` to `tauri-action` → no `createUpdaterArtifacts`.
- Skips `assemble-updater` (no `.sig` files to assemble).

The build still produces all platform installers and attaches them to the
`vX.Y.Z` release. In-app update stays on the "open GitHub release page" path.
This graceful degradation keeps unsigned community builds working.

## Linux note

Only **AppImage** supports in-app update. `.deb` / `.rpm` installs see
`manual-required` and open the GitHub releases page.

## macOS note

Codesign + notarize the `.app` / DMG in CI when Apple secrets are present.
After notarization, rebuild the updater `.tar.gz` from the signed app and
re-sign with the Tauri updater key if you notarize post-build.

## Manual verification

1. `pnpm typecheck` / `pnpm test` — UI unit tests
2. `cargo test --manifest-path src-tauri/Cargo.toml` — Rust tests
3. `./scripts/verify-updater-setup.sh` — updater wiring checks (no secret values)
4. Release smoke: build with both env vars, confirm `omp_desktop_updater_enabled`
   cfg is set in a release binary, and that check hits `latest.json`
