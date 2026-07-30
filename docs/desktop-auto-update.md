# Desktop auto-update

OMP Desktop uses the **Tauri 2 updater**: signed release artifacts, a rolling
`latest.json` endpoint, in-app check/download/install, and a hard stop of
managed agent / Remote IM processes before the binary swap.

> **Current status:** the updater is **signed and live** as of `v0.3.1-nightly`.
> Settings → About "check for update" runs the silent download → install →
> relaunch path, and the rolling `omp-desktop-latest` release ships `latest.json`
> + per-platform `.sig` archives for all four targets. macOS Gatekeeper / Windows
> SmartScreen still warn because OS code-signing (Apple Developer ID /
> Authenticode) is not yet configured — see
> [Signing requirements](./release/signing-requirements.md) for that remaining
> Plan 9 work. The minisign keypair verifies *archive integrity*, independent of
> OS trust.

## Architecture

```
CI release (tag vX.Y.Z)
  ├── vX.Y.Z                 user-facing installers (DMG / AppImage / NSIS …)
  └── omp-desktop-latest     rolling updater release
        └── latest.json  + per-platform archive + .sig
                 ▲
                 │ check()
        Desktop  tauri-plugin-updater  (release builds only)
                 │ prepare_for_app_update → stop agents / Remote IM
                 │ install + relaunch
        UI: Settings → About
```

Unsigned / local builds keep the previous GitHub "open release page" path — the
`updater` crate is always linked (Tauri ACL requirement) but only **registered**
when the build-time cfg is set.

## App pieces

| Piece | Location |
|-------|----------|
| Build-time gate | `src-tauri/build.rs` → `cfg(omp_desktop_updater_enabled)` when both `OMP_DESKTOP_UPDATER_*` env vars are set (crate always linked for ACL) |
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
   ```
3. **Release cut:** tag `vX.Y.Z` so CI builds installers **and** refreshes `omp-desktop-latest` + `latest.json` + `.sig`.
4. **Smoke on a prior signed build:** Settings → About shows the in-app (signed release) channel → Check → Download → Install and restart → version matches tag.
5. **Failure path:** if install fails, agents / Remote IM must keep running (`prepare_for_app_update` only after successful `install()`).
6. **Unsigned / local builds:** About must show the GitHub download channel and still open Release / download installer (no crash).

## Rolling endpoint

```text
https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json
```

Publish two GitHub releases per cut:

1. **`vX.Y.Z`** — human installers + notes
2. **`omp-desktop-latest`** — updater archives + `latest.json` (clobber each release)

## Build steps (outline)

```sh
export OMP_DESKTOP_UPDATER_PUBLIC_KEY=...
export OMP_DESKTOP_UPDATER_ENDPOINT=https://github.com/Po1nt9/omp-desktop/releases/download/omp-desktop-latest/latest.json
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
TAG=v0.3.0 REPO=Po1nt9/omp-desktop bash scripts/assemble-updater-manifest.sh
```

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
