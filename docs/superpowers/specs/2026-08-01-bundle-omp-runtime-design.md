# Design: Bundle OMP Runtime into OMP Desktop (default-on, user-overridable)

**Date:** 2026-08-01
**Status:** Approved (design confirmed section-by-section with user)
**Scope:** OMP Desktop ships with a bundled `omp` binary so the app works out of
the box, while keeping the existing manual-path override and adding an in-app
independent omp upgrade path.

## Decisions (confirmed with user)

| Question | Decision |
|---|---|
| Bundling scope | **Default bundled, overridable** — app ships one omp; Settings keeps manual-path override. (Not "bundled-only", not "download-on-first-run".) |
| Binary source | **Compile from submodule source** in CI — version bound to `runtime/oh-my-pi` submodule pin. (Not "download from oh-my-pi Releases", not "store binary in git".) |
| Upgrade strategy | **In-app independent omp upgrade** — "Check omp update" button downloads latest omp to a writable data dir, decoupled from Desktop releases. (Not "tied to App version".) |
| Bundle placement | **Bundled = read-only sidecar; upgrade = writable data dir; upgrade wins** — two locations, upgrade copy preferred, bundled copy is permanent fallback. |

## Motivation

OMP Desktop currently ships as a shell: without a user-supplied `omp` CLI
(Settings → manual path), the app is fail-closed and the welcome screen cannot
advance. This is a poor out-of-box experience. Bundling the Runtime — the way
Codex bundles its engine — makes the app usable immediately, while the
manual-path escape hatch preserves flexibility for advanced users.

## Architecture

### Three-tier binary resolution

At spawn time, resolve the omp binary in priority order; first existing path
wins:

```
1. settings.manual_cli_path        ← user manual override (existing, highest)
2. <app_data>/runtime/omp[.exe]     ← in-app upgraded copy (writable, preferred over bundled)
3. <bundle>/omp[.exe]                 ← bundled sidecar copy next to the main exe (read-only, factory default, fallback)
```

- **Bundled copy:** compiled from submodule source in CI, named per Tauri
  sidecar convention, embedded read-only. Always present as the safety net.
- **Upgraded copy:** downloaded by "Check omp update" into
  `<app_data>/runtime/omp`. Writable, preferred when present.
- **Manual override:** existing `manual_cli_path`, unchanged, highest priority.

Deleting the upgraded copy automatically falls back to the bundled copy on next
spawn — this is the built-in one-step rollback.

### v1 protocol is already live

`acp_client.rs:236` and `supervisor/mod.rs:47` already inject
`OMP_DESKTOP_V1_PROTOCOL=1` at spawn. The bundled Runtime (submodule pin
`v17.1.3-21-g64db4c38a`, 21 commits past the 17.1.3 tag) carries the full
`_omp/desktop/v1/*` dispatcher + all 34 handlers
(`runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/`). The prior
acceptance probe failures were an artifact of probing the stale system-installed
17.1.3 without the env gate — not a missing implementation. Bundling the
pinned, newer Runtime removes that version-skew class of error entirely.

## Components

### 1. Binary resolution (Rust)

`session_manager.rs` — replace the inline `manual_cli_path` read with a new
`resolve_omp_binary(settings)`:

```rust
fn resolve_omp_binary(settings: &AppSettings) -> Option<PathBuf> {
    // 1. manual override (existing logic)
    if manual_cli_path.exists() { return Some(that) }
    // 2. upgraded copy
    let upgraded = paths::app_data_root().join("runtime").join(omp_binary_name());
    if upgraded.exists() { return Some(upgraded) }
    // 3. bundled sidecar — Tauri `externalBin` bundles it as plain `omp`
    //    next to the main executable
    std::env::current_exe().ok()
        .and_then(|exe| exe.parent().map(|d| d.join(omp_binary_name())))
        .filter(|p| p.exists())
}
```

- `omp_binary_name()` → `omp` (macOS/Linux) / `omp.exe` (Windows). The upgraded
  copy uses a single fixed name (no platform suffix) because each package only
  ever contains its own platform's binary.
- Bundled path via `std::env::current_exe()` sibling lookup, NOT
  `app_handle.sidecar()` — no `tauri-plugin-shell` dependency, and it works in
  both spawn sites: `session_manager.rs` (has `AppHandle`) and
  `remote_im/bridge.rs:170-176` (`start_async` has no `AppHandle`). Tauri
  `externalBin = ["binaries/omp"]` bundles the per-target
  `binaries/omp-<triple>` as plain `omp` (stripped suffix) next to the main
  executable, so the sibling lookup finds it on all four targets.
- Change surface is the new resolver plus swapping both binary-resolution
  sites (`session_manager.rs:2652-2658`, `remote_im/bridge.rs:170-176`);
  `spawn_with_options` and the v1 env injection are untouched.

### 2. CI compile step (release.yml)

The single `publish` matrix job (4 targets) in release.yml gains:

```yaml
- uses: actions/checkout@v4
  with:
    submodules: true        # currently absent — releases build WITHOUT the submodule today

- name: Setup Bun
  uses: oven-sh/setup-bun@v2

- name: Build omp binary (submodule)
  working-directory: runtime/oh-my-pi
  env:
    CROSS_TARGET: ${{ matrix.omp_target }}
  run: |
    bun install --frozen-lockfile
    bun --cwd packages/coding-agent run build

- name: Place omp binary at sidecar path
  shell: bash
  run: |
    mkdir -p src-tauri/binaries
    cp "runtime/oh-my-pi/packages/coding-agent/dist/${{ matrix.omp_artifact }}" \
       "src-tauri/binaries/${{ matrix.sidecar_name }}"
```

- Matrix gains three fields per job (CROSS_TARGET / dist artifact / sidecar
  name):
  - mac-arm64: `darwin-arm64` → `omp-darwin-arm64` → `omp-aarch64-apple-darwin`
  - mac-x64: `darwin-x64` → `omp-darwin-x64` → `omp-x86_64-apple-darwin`
  - win-x64: `win32-x64` → `omp-win32-x64.exe` → `omp-x86_64-pc-windows-msvc.exe`
  - linux-x64: `linux-x64` → `omp-linux-x64` → `omp-x86_64-unknown-linux-gnu`
- The build must run from the **workspace root** of the submodule:
  `build-binary.ts` shells out to sibling packages
  (`gen:stats` / `gen:tool-views` / `gen:native` / `gen:mupdf`), so
  `bun install --frozen-lockfile` runs in `runtime/oh-my-pi` and the build is
  invoked as `bun --cwd packages/coding-agent run build`.
- Output is `packages/coding-agent/dist/omp[-<CROSS_TARGET>]` (Bun appends
  `.exe` for the win32 target) — NOT `binaries/`.
- Each job compiles its own platform natively; `CROSS_TARGET` is still set so
  the output carries the target suffix and `gen:native` gets the right
  TARGET_PLATFORM/TARGET_ARCH.
- `tauri.conf.json` gains `"externalBin": ["binaries/omp"]` under `bundle` —
  Tauri picks up `binaries/omp-<triple>` per target and bundles it as plain
  `omp` next to the main executable.
- Current release already produces **separate single-arch** macOS packages
  (ARM64 DMG + x64 DMG as distinct artifacts), so sidecar per-arch naming works
  cleanly — no Universal-merge complication.
- No new CI jobs; extra steps inside the existing matrix job.

### 3. In-app omp upgrade ("检查 omp 更新")

Flow:

```
click "检查 omp 更新"
  → GET can1357/oh-my-pi releases/latest (tag + assets)
  → compare with current omp version (spawn `--version`, cached in Settings)
  → newer? no → "already up to date"
    yes → download omp-<target> to <app_data>/runtime/omp.new
        → executable-bit sanity check (+ SHA256 recorded to Settings)
        → atomic rename .new → omp
        → "update complete, restart to apply"
```

- **Version detect:** spawn `<resolved omp> --version` once, cache result in
  Settings. No `--smoke-test` (too heavy).
- **Download source (decided 2026-08-01):** GitHub API
  `repos/can1357/oh-my-pi/releases/latest` — the UPSTREAM repo; the
  `Po1nt9/oh-my-pi` fork publishes no releases. Asset names per target:
  `omp-darwin-arm64`, `omp-darwin-x64`, `omp-linux-x64`,
  `omp-windows-x64.exe` (note: `windows`, not `win32`).
- **Integrity:** upstream publishes no SHA256SUMS today (7 binary assets,
  changelog-only body). Trust model is TLS + GitHub, same as installing omp
  from npm/brew. The downloaded file's computed SHA256 is recorded in Settings
  for audit; if a checksums asset ever appears on upstream releases, the
  downloader verifies against it automatically (fail-closed on mismatch).
- **Atomic replace:** write `.new`, verify, then `rename` (POSIX atomic;
  Windows `MoveFileEx` + `MOVEFILE_REPLACE_EXISTING`). On replace failure, keep
  the old copy — a failed upgrade never breaks availability.
- **Rollback:** no version history. User rolls back by deleting the upgraded
  copy (auto-fallback to bundled) or pointing manual path elsewhere. Avoids
  multi-version accumulation in the data dir.
- **UI:** Settings → About, a "检查 omp 更新" button beside the existing
  Desktop-update button; independent flows. i18n via existing `createT(locale)`
  (zh + en).

## Error boundaries & degradation

| Failure | Behavior |
|---|---|
| Bundled copy missing (packaging error) | Fall through to upgraded copy, then manual path, then fail-closed (existing "Runtime not configured") |
| Upgraded copy corrupt (interrupted download / bad hash) | Delete `.new`, silently fall back to bundled; user unaffected |
| Upgrade download network failure | "Network error, retry later"; bundled copy keeps working |
| Checksums asset present but hash mismatch | Delete `.new`, "downloaded file corrupted", no replace |
| Replace permission denied (Windows file lock) | Keep old copy, "close all sessions and retry" |
| omp crashes on launch (version incompat) | Existing supervisor crash detection flips `unavailable` (`session_manager.rs:3636`); user reverts to bundled by deleting upgraded copy |
| GitHub API rate limit | "update check failed, retry later"; App unaffected |

**Core principles:**

1. Upgrade never breaks availability — any upgrade failure falls back to the
   bundled copy; the bundled copy is the untouchable safety net.
2. The bundled copy is read-only and never mutated by upgrades — upgrades only
   touch `<app_data>/runtime/omp`. Deleting that file is the one-step rollback.

**Non-goals (YAGNI):**
- No multi-version history / version switcher
- No automatic background update checks (user-initiated button only)
- No post-crash downgrade prompts (supervisor logs suffice)

## Data flow

```
[CI] submodule source --bun compile--> omp-<target> --cp--> src-tauri/binaries/
                                                                | (tauri externalBin)
                                                                v
[Package] omp-<target> embedded read-only ---------------> spawn fallback (tier 3)

[User] "检查 omp 更新" --> can1357/oh-my-pi Releases --> download (TLS) --> atomic rename
                                                                |
                                                                v
                                              <app_data>/runtime/omp (tier 2)

[Spawn] manual_path? -> upgraded? -> bundled?  (first hit wins)
```

## Testing

- **Unit (Rust):** `resolve_omp_binary` priority order — manual > upgraded >
  bundled > none; each tier present/absent combination.
- **Unit (Rust):** `omp_binary_name()` per-platform.
- **Integration:** spawn resolves to bundled copy in a clean env (no manual
  path, no upgraded copy) — proves out-of-box path.
- **Upgrade flow:** mock GitHub release server (fixture assets, optional
  checksums) — happy path download/replace; checksum-mismatch rejection when a
  checksums fixture is present; rename-failure keeps old.
- **Rollback:** after placing an upgraded copy, delete it → next spawn resolves
  to bundled.
- **CI:** each of the 4 build jobs produces a package whose bundled omp runs
  `omp --version` (assert sidecar binary present in artifact).

## Out of scope

- Changing how `manual_cli_path` / Settings UI works today (kept as-is).
- Release-signing the omp upgrade download (upstream ships unsigned binaries;
  integrity is TLS + GitHub, checksum auto-verify when upstream adds them).
- Windows/macOS-Intel or IM-channel acceptance items (waived 2026-07-31, see
  `docs/release/1.0-acceptance-matrix.md` header).
