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
3. <bundle>/omp-<target>[.exe]      ← bundled sidecar copy (read-only, factory default, fallback)
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
`resolve_omp_binary(settings, app_handle)`:

```rust
fn resolve_omp_binary(settings, app_handle) -> Option<PathBuf> {
    // 1. manual override (existing logic)
    if manual_cli_path.exists() { return Some(that) }
    // 2. upgraded copy
    let upgraded = paths::app_data_root().join("runtime").join(omp_binary_name());
    if upgraded.exists() { return Some(upgraded) }
    // 3. bundled sidecar
    app_handle.sidecar("omp").ok().map(|cmd| cmd.path().into())
}
```

- `omp_binary_name()` → `omp` (macOS/Linux) / `omp.exe` (Windows). The upgraded
  copy uses a single fixed name (no platform suffix) because each package only
  ever contains its own platform's binary.
- Sidecar path via Tauri's `app_handle.sidecar("omp")` API — Tauri resolves the
  correct `omp-<target>` suffix and sandboxed absolute path.
- Change surface is this one function; `spawn_with_options` and the v1 env
  injection are untouched.

### 2. CI compile step (release.yml)

Each of the 4 existing build jobs (win-x64, mac-arm64, mac-x64, linux-x64)
gains two steps **before** the Tauri build:

```yaml
- name: Build omp binary (submodule)
  working-directory: runtime/oh-my-pi/packages/coding-agent
  run: |
    bun install --frozen-lockfile
    bun run build
  env:
    OMP_COMPILE_TARGET: ${{ matrix.omp_target }}

- name: Place omp binary at sidecar path
  run: |
    mkdir -p src-tauri/binaries
    cp runtime/oh-my-pi/packages/coding-agent/binaries/omp-* \
       src-tauri/binaries/omp-${{ matrix.sidecar_target }}
```

- Matrix gains two fields per job: `omp_target` (Bun compile target, e.g.
  `darwin-arm64`) and `sidecar_target` (Tauri target triple, e.g.
  `aarch64-apple-darwin`).
- Bun installed per-job via `oven-sh/setup-bun`.
- `build-binary.ts` outputs to `packages/coding-agent/binaries/omp-<target>`;
  copied to `src-tauri/binaries/omp-<sidecar_target>` where Tauri `externalBin`
  finds it.
- Current release already produces **separate single-arch** macOS packages
  (ARM64 DMG + x64 DMG as distinct artifacts), so sidecar per-arch naming works
  cleanly — no Universal-merge complication.
- No new CI jobs; two extra steps in existing jobs.

### 3. In-app omp upgrade ("检查 omp 更新")

Flow:

```
click "Check omp update"
  → GET oh-my-pi releases/latest (tag + assets)
  → compare with current omp version (spawn `--version`, cached in Settings)
  → newer? no → "already up to date"
    yes → download omp-<target> to <app_data>/runtime/omp.new
        → SHA256 vs release SHA256SUMS
        → match? no → delete .new, "download corrupted"
          yes → atomic rename .new → omp
              → "update complete, restart to apply"
```

- **Version detect:** spawn `<resolved omp> --version` once, cache result in
  Settings. No `--smoke-test` (too heavy).
- **Download source:** GitHub API `repos/Po1nt9/oh-my-pi/releases/latest`;
  pick `omp-<target>` asset + `SHA256SUMS`. SHA256 is sufficient today (omp
  releases are not minisign-signed); signature check can be added later if omp
  starts signing.
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
| SHA256 mismatch | Delete `.new`, "downloaded file corrupted", no replace |
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

[User] "Check omp update" --> oh-my-pi Releases --> download+SHA256 --> atomic rename
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
- **Upgrade flow:** mock GitHub release server (fixture assets + SHA256SUMS) —
  happy path download/replace; corrupt-hash rejection; rename-failure keeps old.
- **Rollback:** after placing an upgraded copy, delete it → next spawn resolves
  to bundled.
- **CI:** each of the 4 build jobs produces a package whose bundled omp runs
  `omp --version` (assert sidecar binary present in artifact).

## Out of scope

- Changing how `manual_cli_path` / Settings UI works today (kept as-is).
- Signing the omp upgrade download beyond SHA256 (omp releases unsigned today).
- Windows/macOS-Intel or IM-channel acceptance items (waived 2026-07-31, see
  `docs/release/1.0-acceptance-matrix.md` header).
