# Bundle OMP Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship OMP Desktop with a bundled `omp` Runtime (works out of the box), keep the manual-path override, and add an in-app "检查 omp 更新" upgrade flow.

**Architecture:** Three-tier binary resolution at spawn (manual override → in-app upgraded copy at `<app_data>/runtime/omp` → bundled sidecar next to the main exe). CI compiles omp from the `runtime/oh-my-pi` submodule inside the existing release matrix job and places it at `src-tauri/binaries/omp-<triple>` for Tauri `externalBin`. Upgrades download from upstream `can1357/oh-my-pi` releases (TLS trust, SHA256 recorded) and atomically replace the writable copy.

**Tech Stack:** Rust (Tauri 2), Bun (`--compile`) for the omp binary, GitHub Actions (release.yml), React + `createT(locale)` i18n.

**Spec:** [2026-08-01-bundle-omp-runtime-design.md](../specs/2026-08-01-bundle-omp-runtime-design.md) (authoritative; this plan implements it 1:1)

## Global Constraints

- Product name is **OMP Desktop**; the Runtime is **omp** (lowercase, it is a name).
- The upgrade button copy is **检查 omp 更新** — never "检查 Runtime 更新".
- No hardcoded user-facing English/Chinese — all UI strings via `createT(locale)` / `t()`, added to all 3 locales (`src/i18n/messages.ts` en + zh-CN blocks, `src/i18n/zh-tw.ts`).
- Never use `window.confirm` / `window.prompt` / `window.alert` in Tauri UI.
- Log metadata only — never prompt content, never secret values, never dump binary stdout to logs (SA-L.1 / AC-8.8).
- Do not commit binaries: `src-tauri/binaries/` is a build artifact, git-ignored.
- Do not commit secrets, `secrets.json`, or local configuration files.
- README.md is Chinese (primary); README_EN.md is English. Keep both in sync.
- Run `pnpm check:brand` before every commit that touches bundled artifacts or user-facing copy.
- Full gates before the final commit: `cd src-tauri && cargo test --lib`, `pnpm test`, `pnpm typecheck`, `pnpm check:i18n`, `pnpm check:brand`, `pnpm check:provenance`, `pnpm check:legal`.
- Rust env mutation in tests MUST hold `crate::paths::APP_HOME_ENV_LOCK` (parking_lot Mutex) and use `unsafe { std::env::set_var(...) }` (edition 2024).

---

### Task 1: Three-tier `omp_runtime` resolver + swap both spawn sites

**Files:**
- Create: `src-tauri/src/omp_runtime.rs`
- Modify: `src-tauri/src/lib.rs` (module declaration)
- Modify: `src-tauri/src/session_manager.rs:2647-2658`
- Modify: `src-tauri/src/remote_im/bridge.rs:168-176`

**Interfaces:**
- Consumes: `crate::store::AppSettings.manual_cli_path` (existing), `crate::paths::app_data_root()` (existing).
- Produces: `pub fn omp_binary_name() -> &'static str`, `pub fn upgraded_omp_path() -> PathBuf`, `pub fn bundled_omp_path() -> Option<PathBuf>`, `pub fn resolve_from_candidates(manual: Option<&str>, upgraded: &Path, bundled: Option<&Path>) -> Option<PathBuf>`, `pub fn resolve_omp_binary(settings: &crate::store::AppSettings) -> Option<PathBuf>`. Task 4 uses `omp_binary_name()` and `resolve_omp_binary()`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/omp_runtime.rs` with only the test module first (file won't compile until Step 3 adds the functions — that is the failing state):

```rust
//! Three-tier OMP Runtime binary resolution.
//! Spec: docs/superpowers/specs/2026-08-01-bundle-omp-runtime-design.md

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn manual_wins_over_upgraded_and_bundled() {
        let dir = tempfile::tempdir().unwrap();
        let manual = dir.path().join("manual-omp");
        let upgraded = dir.path().join("upgraded-omp");
        let bundled = dir.path().join("bundled-omp");
        for p in [&manual, &upgraded, &bundled] {
            std::fs::write(p, b"x").unwrap();
        }
        assert_eq!(
            resolve_from_candidates(
                Some(manual.to_str().unwrap()),
                &upgraded,
                Some(&bundled),
            ),
            Some(manual)
        );
    }

    #[test]
    fn missing_or_empty_manual_falls_through_to_upgraded() {
        let dir = tempfile::tempdir().unwrap();
        let upgraded = dir.path().join("omp");
        std::fs::write(&upgraded, b"x").unwrap();
        let missing = dir.path().join("nope");
        // manual path points at a nonexistent file
        assert_eq!(
            resolve_from_candidates(Some(missing.to_str().unwrap()), &upgraded, None),
            Some(upgraded.clone())
        );
        // manual is whitespace-only
        assert_eq!(
            resolve_from_candidates(Some("   "), &upgraded, None),
            Some(upgraded)
        );
    }

    #[test]
    fn upgraded_wins_over_bundled() {
        let dir = tempfile::tempdir().unwrap();
        let upgraded = dir.path().join("omp");
        let bundled = dir.path().join("bundled-omp");
        std::fs::write(&upgraded, b"x").unwrap();
        std::fs::write(&bundled, b"x").unwrap();
        assert_eq!(
            resolve_from_candidates(None, &upgraded, Some(&bundled)),
            Some(upgraded)
        );
    }

    #[test]
    fn bundled_is_last_resort_and_none_when_all_absent() {
        let dir = tempfile::tempdir().unwrap();
        let upgraded = dir.path().join("omp"); // does not exist
        let bundled = dir.path().join("bundled-omp");
        std::fs::write(&bundled, b"x").unwrap();
        assert_eq!(
            resolve_from_candidates(None, &upgraded, Some(&bundled)),
            Some(bundled)
        );
        let absent = dir.path().join("absent");
        assert_eq!(resolve_from_candidates(None, &upgraded, Some(&absent)), None);
        assert_eq!(resolve_from_candidates(None, &upgraded, None), None);
    }

    #[test]
    fn binary_name_matches_platform() {
        if cfg!(windows) {
            assert_eq!(omp_binary_name(), "omp.exe");
        } else {
            assert_eq!(omp_binary_name(), "omp");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib omp_runtime`
Expected: FAIL — `resolve_from_candidates`, `omp_binary_name` unresolved.

- [ ] **Step 3: Implement the resolver**

Add above the test module in `src-tauri/src/omp_runtime.rs`:

```rust
use std::path::{Path, PathBuf};

/// Platform binary name: `omp` (macOS/Linux) / `omp.exe` (Windows). The
/// upgraded copy and the bundled sidecar both use this single fixed name —
/// each package only ever contains its own platform's binary.
pub fn omp_binary_name() -> &'static str {
    if cfg!(windows) {
        "omp.exe"
    } else {
        "omp"
    }
}

/// `<app_data>/runtime/omp[.exe]` — the writable in-app upgraded copy.
pub fn upgraded_omp_path() -> PathBuf {
    crate::paths::app_data_root()
        .join("runtime")
        .join(omp_binary_name())
}

/// Bundled sidecar: Tauri `externalBin` bundles `binaries/omp-<triple>` as
/// plain `omp` next to the main executable. Sibling lookup (no
/// tauri-plugin-shell dependency, no AppHandle needed).
pub fn bundled_omp_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(omp_binary_name())))
}

/// Pure priority resolution — first existing candidate wins:
/// manual override → upgraded copy → bundled sidecar.
pub fn resolve_from_candidates(
    manual: Option<&str>,
    upgraded: &Path,
    bundled: Option<&Path>,
) -> Option<PathBuf> {
    manual
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| upgraded.exists().then(|| upgraded.to_path_buf()))
        .or_else(|| bundled.filter(|p| p.exists()).map(Path::to_path_buf))
}

/// Resolve the omp binary for spawn. `None` preserves the Plan 1 fail-closed
/// behavior when no binary exists at any tier.
pub fn resolve_omp_binary(settings: &crate::store::AppSettings) -> Option<PathBuf> {
    resolve_from_candidates(
        settings.manual_cli_path.as_deref(),
        &upgraded_omp_path(),
        bundled_omp_path().as_deref(),
    )
}
```

Register the module in `src-tauri/src/lib.rs` next to the other `mod` declarations (alphabetical neighborhood, e.g. after `mod models_catalog;` / wherever `mod` lines live):

```rust
mod omp_runtime;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib omp_runtime`
Expected: 5 PASS.

- [ ] **Step 5: Swap the session spawn site**

In `src-tauri/src/session_manager.rs:2647-2658`, replace:

```rust
        // Plan 3 Task 5: discover the OMP Runtime binary. Prefer the user's
        // manually-picked path (Settings → manual_cli_path); fall back to
        // `None` to preserve the Plan 1 fail-closed behavior for environments
        // without the runtime. The agent_dir is the independent runtime home
        // (PI_CODING_AGENT_DIR) so the sidecar shares the host's auth/profile.
        let binary_path = settings
            .manual_cli_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists());
```

with:

```rust
        // Three-tier resolution (bundle-omp spec): manual override →
        // in-app upgraded copy → bundled sidecar; None stays fail-closed.
        // The agent_dir is the independent runtime home
        // (PI_CODING_AGENT_DIR) so the sidecar shares the host's auth/profile.
        let binary_path = crate::omp_runtime::resolve_omp_binary(&settings);
```

- [ ] **Step 6: Swap the remote-IM bridge spawn site**

In `src-tauri/src/remote_im/bridge.rs:168-176`, replace:

```rust
        // Resolve binary path and agent dir from Settings (same source as SessionManager).
        let settings = crate::store::load_settings();
        let binary_path = settings
            .manual_cli_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists());
```

with:

```rust
        // Same three-tier resolution as SessionManager (bundle-omp spec).
        let settings = crate::store::load_settings();
        let binary_path = crate::omp_runtime::resolve_omp_binary(&settings);
```

If `use std::path::PathBuf;` in bridge.rs becomes unused (warning), remove it; keep it if other code still uses `PathBuf`.

- [ ] **Step 7: Run the full Rust suite**

Run: `cd src-tauri && cargo test --lib`
Expected: all green (previous count + 5 new).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/omp_runtime.rs src-tauri/src/lib.rs src-tauri/src/session_manager.rs src-tauri/src/remote_im/bridge.rs
git commit -m "feat(runtime): three-tier omp binary resolution (manual > upgraded > bundled)"
```

---

### Task 2: Bundle config (externalBin + gitignore) + local packaged verification

**Files:**
- Modify: `src-tauri/tauri.conf.json` (`bundle` section)
- Modify: `.gitignore`
- Create (git-ignored): `src-tauri/binaries/omp-aarch64-apple-darwin`

**Interfaces:**
- Consumes: Task 1's `bundled_omp_path()` sibling convention (bundled as plain `omp` next to main exe).
- Produces: packaged app containing `Contents/MacOS/omp`; CI (Task 3) reproduces this per-target.

- [ ] **Step 1: Add externalBin to tauri.conf.json**

In `src-tauri/tauri.conf.json`, inside the `"bundle"` object (e.g. right after `"active": true,`), add:

```json
    "externalBin": [
      "binaries/omp"
    ],
```

- [ ] **Step 2: Git-ignore the sidecar directory**

Append to `.gitignore` (root):

```
# Build artifact: bundled omp sidecar placed by CI / local build (Task 2/3 of bundle-omp plan)
src-tauri/binaries/
```

- [ ] **Step 3: Build omp from the submodule locally**

```bash
cd runtime/oh-my-pi
bun install --frozen-lockfile
bun --cwd packages/coding-agent run build
cd ../..
mkdir -p src-tauri/binaries
cp runtime/oh-my-pi/packages/coding-agent/dist/omp src-tauri/binaries/omp-aarch64-apple-darwin
```

Expected: `src-tauri/binaries/omp-aarch64-apple-darwin` exists and `src-tauri/binaries/omp-aarch64-apple-darwin --version` prints a version.
(If bun is not installed locally: `brew install oven-sh/bun/bun`. The submodule build shells out to sibling packages gen scripts — run exactly as shown from the workspace root.)

- [ ] **Step 4: Build the app bundle**

Run: `pnpm build` (full `tauri build`, several minutes).
Expected: build succeeds; `src-tauri/target/release/bundle/macos/OMP Desktop.app/Contents/MacOS/` contains BOTH `omp-desktop` and `omp` (sidecar renamed to plain `omp`).

- [ ] **Step 5: Verify the bundled sidecar runs**

```bash
"src-tauri/target/release/bundle/macos/OMP Desktop.app/Contents/MacOS/omp" --version
```

Expected: prints the omp version (e.g. `17.1.3` or newer). This proves the tier-3 fallback binary is present and executable at the exact path `bundled_omp_path()` computes.

- [ ] **Step 6: Brand check + commit**

```bash
pnpm check:brand
git add src-tauri/tauri.conf.json .gitignore
git commit -m "feat(bundle): externalBin config for bundled omp sidecar"
```

(Do NOT `git add src-tauri/binaries/` — it is git-ignored on purpose.)

---

### Task 3: CI compile steps in release.yml

**Files:**
- Modify: `.github/workflows/release.yml` (matrix + 3 steps)

**Interfaces:**
- Consumes: Task 2's `externalBin` config and `src-tauri/binaries/omp-<triple>` naming.
- Produces: every release artifact carries its platform's bundled omp.

- [ ] **Step 1: Extend the matrix**

In `.github/workflows/release.yml`, replace the four matrix entries:

```yaml
          - platform: macos-latest
            args: "--target aarch64-apple-darwin"
            name: macOS-ARM64
          - platform: macos-latest
            args: "--target x86_64-apple-darwin"
            name: macOS-x64
          - platform: windows-latest
            args: ""
            name: Windows-x64
          - platform: ubuntu-22.04
            args: ""
            name: Linux-x64
```

with:

```yaml
          - platform: macos-latest
            args: "--target aarch64-apple-darwin"
            name: macOS-ARM64
            omp_target: darwin-arm64
            omp_artifact: omp-darwin-arm64
            sidecar_name: omp-aarch64-apple-darwin
          - platform: macos-latest
            args: "--target x86_64-apple-darwin"
            name: macOS-x64
            omp_target: darwin-x64
            omp_artifact: omp-darwin-x64
            sidecar_name: omp-x86_64-apple-darwin
          - platform: windows-latest
            args: ""
            name: Windows-x64
            omp_target: win32-x64
            omp_artifact: omp-win32-x64.exe
            sidecar_name: omp-x86_64-pc-windows-msvc.exe
          - platform: ubuntu-22.04
            args: ""
            name: Linux-x64
            omp_target: linux-x64
            omp_artifact: omp-linux-x64
            sidecar_name: omp-x86_64-unknown-linux-gnu
```

- [ ] **Step 2: Fetch the submodule in checkout**

Replace:

```yaml
      - name: Checkout
        uses: actions/checkout@v4
```

with:

```yaml
      - name: Checkout
        uses: actions/checkout@v4
        with:
          submodules: true
```

- [ ] **Step 3: Add Bun + omp build + sidecar placement steps**

Insert immediately AFTER the `- name: Install frontend dependencies` step and BEFORE the `# Release description = CHANGELOG.md section` comment block:

```yaml
      - name: Setup Bun
        uses: oven-sh/setup-bun@v2

      # bundle-omp: compile the pinned submodule Runtime for this target.
      # build-binary.ts shells out to sibling workspace packages, so install
      # and build run from the submodule workspace root.
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
          set -euo pipefail
          mkdir -p src-tauri/binaries
          cp "runtime/oh-my-pi/packages/coding-agent/dist/${{ matrix.omp_artifact }}" \
             "src-tauri/binaries/${{ matrix.sidecar_name }}"
          "src-tauri/binaries/${{ matrix.sidecar_name }}" --version
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): compile omp from submodule and bundle as sidecar per target"
```

- [ ] **Step 5: Verify on CI (4 targets)**

```bash
git push
gh workflow run release.yml
gh run list --workflow=release.yml --limit 1
# wait for the run, then:
gh run watch
```

Expected: all 4 `Build *` jobs green. Then download the macOS ARM64 artifact from the produced draft/release and assert the sidecar is inside:

```bash
# after the run finishes, find the release assets:
gh release view --json assets --jq '.assets[].name'
# download the aarch64 app.tar.gz (updater artifact) or mount the DMG, then:
tar tzf <app.tar.gz> | grep -E 'MacOS/omp$'
```

Expected: `OMP Desktop.app/Contents/MacOS/omp` present. (Windows/Linux legs are verified by the `--version` echo in the placement step of each job's log.)

---

### Task 4: omp upgrade backend (`omp_update.rs` + commands)

**Files:**
- Create: `src-tauri/src/omp_update.rs`
- Modify: `src-tauri/src/app_update.rs:268,274,296` (widen visibility of 3 helpers)
- Modify: `src-tauri/src/commands.rs` (2 new commands, after `app_check_update` at :237)
- Modify: `src-tauri/src/lib.rs` (`mod omp_update;`, handler registration, `registered_command_names`)

**Interfaces:**
- Consumes: Task 1's `omp_runtime::{omp_binary_name, resolve_omp_binary}`; `app_update::{is_remote_newer, http_client, is_allowed_update_url, format_http_error}` (visibility widened below); `paths::{app_data_root, APP_HOME_ENV_LOCK}`.
- Produces: `pub struct OmpUpdateCheck { current_version, latest_version, update_available, download_url, release_url }` (serde camelCase), `pub struct OmpUpdateApplied { version, sha256, path }`, `pub async fn check_omp_update()`, `pub async fn download_and_apply(url)`, `pub fn asset_name_for(os, arch)`, `pub fn parse_omp_version(stdout)`, `pub fn detect_omp_version(binary)`, `pub fn parse_omp_release(json, current)`, `pub fn apply_omp_bytes(bytes)`. Task 5 calls the two Tauri commands.

- [ ] **Step 1: Widen helper visibility in app_update.rs**

Change three signatures (bodies unchanged):

```rust
pub(crate) fn is_allowed_update_url(url: &str) -> bool {
```

```rust
pub(crate) fn format_http_error(status: u16, body: &str) -> String {
```

```rust
pub(crate) fn http_client(user_agent: &str) -> Result<reqwest::Client, String> {
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/omp_update.rs` containing only:

```rust
//! In-app independent omp Runtime upgrade ("检查 omp 更新").
//! Spec: docs/superpowers/specs/2026-08-01-bundle-omp-runtime-design.md

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_semver_from_version_output() {
        assert_eq!(parse_omp_version("omp 17.2.2\n").as_deref(), Some("17.2.2"));
        assert_eq!(parse_omp_version("17.1.3").as_deref(), Some("17.1.3"));
        assert_eq!(parse_omp_version("oh-my-pi v17.2.2-beta.1").as_deref(), Some("17.2.2"));
        assert_eq!(parse_omp_version("no version here"), None);
        assert_eq!(parse_omp_version("1.2"), None);
    }

    #[test]
    fn asset_name_maps_ci_targets() {
        assert_eq!(asset_name_for("macos", "aarch64"), Some("omp-darwin-arm64"));
        assert_eq!(asset_name_for("macos", "x86_64"), Some("omp-darwin-x64"));
        assert_eq!(asset_name_for("windows", "x86_64"), Some("omp-windows-x64.exe"));
        assert_eq!(asset_name_for("linux", "x86_64"), Some("omp-linux-x64"));
        assert_eq!(asset_name_for("linux", "aarch64"), None);
    }

    #[test]
    fn parse_release_compares_and_picks_asset() {
        let v = serde_json::json!({
            "tag_name": "v17.2.2",
            "html_url": "https://github.com/can1357/oh-my-pi/releases/tag/v17.2.2",
            "assets": [
                { "name": "omp-linux-x64", "browser_download_url": "https://github.com/can1357/oh-my-pi/releases/download/v17.2.2/omp-linux-x64" },
                { "name": current_asset_name().unwrap(), "browser_download_url": "https://github.com/can1357/oh-my-pi/releases/download/v17.2.2/CURRENT" }
            ]
        });
        // older local → update available, our platform's asset picked
        let check = parse_omp_release(&v, Some("17.1.3")).unwrap();
        assert!(check.update_available);
        assert_eq!(check.latest_version, "17.2.2");
        assert!(check.download_url.unwrap().ends_with("/CURRENT"));
        // same version → no update
        let same = parse_omp_release(&v, Some("17.2.2")).unwrap();
        assert!(!same.update_available);
        // no local binary → offer download
        let fresh = parse_omp_release(&v, None).unwrap();
        assert!(fresh.update_available);
    }

    fn with_test_home() -> (parking_lot::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = crate::paths::APP_HOME_ENV_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: serialized by APP_HOME_ENV_LOCK (process-wide env mutation).
        unsafe { std::env::set_var("OMP_DESKTOP_HOME", dir.path()) };
        (guard, dir)
    }

    #[test]
    fn apply_writes_atomic_copy_and_sha_record() {
        let (_guard, home) = with_test_home();
        let _ = &home;
        let bytes = vec![b'x'; 2048];
        let applied = apply_omp_bytes(&bytes).unwrap();
        let target = crate::omp_runtime::upgraded_omp_path();
        assert!(target.exists());
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        // sha256 sidecar record
        let sha_file = target.with_file_name(format!(
            "{}.sha256",
            crate::omp_runtime::omp_binary_name()
        ));
        assert_eq!(std::fs::read_to_string(sha_file).unwrap(), applied.sha256);
        // unix executable bit
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o111, 0o111);
        }
        // second apply atomically replaces the first
        let bytes2 = vec![b'y'; 2048];
        apply_omp_bytes(&bytes2).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), bytes2);
    }

    #[test]
    fn apply_rejects_tiny_download() {
        let (_guard, _home) = with_test_home();
        assert!(apply_omp_bytes(b"404: Not Found").is_err());
        assert!(!crate::omp_runtime::upgraded_omp_path().exists());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib omp_update`
Expected: FAIL — unresolved functions.

- [ ] **Step 4: Implement omp_update.rs**

Add above the test module:

```rust
use serde::Serialize;
use sha2::Digest;
use std::path::Path;

const RELEASE_API: &str = "https://api.github.com/repos/can1357/oh-my-pi/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/can1357/oh-my-pi/releases/latest";
/// Real omp binaries are tens of MB; anything smaller is an error page.
const MIN_BINARY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpUpdateCheck {
    pub current_version: Option<String>,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: Option<String>,
    pub release_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmpUpdateApplied {
    pub version: Option<String>,
    pub sha256: String,
    pub path: String,
}

/// Map (target_os, target_arch) to the upstream release asset name.
/// Upstream naming: omp-darwin-arm64 / omp-darwin-x64 / omp-linux-x64 /
/// omp-windows-x64.exe (no Linux ARM64 desktop asset).
pub fn asset_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("omp-darwin-arm64"),
        ("macos", "x86_64") => Some("omp-darwin-x64"),
        ("windows", "x86_64") => Some("omp-windows-x64.exe"),
        ("linux", "x86_64") => Some("omp-linux-x64"),
        _ => None,
    }
}

pub fn current_asset_name() -> Option<&'static str> {
    asset_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// First `X.Y.Z` in `omp --version` output (tolerates prefixes/suffixes).
pub fn parse_omp_version(stdout: &str) -> Option<String> {
    let bytes = stdout.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let cand = &stdout[start..i];
            let parts: Vec<&str> = cand.split('.').collect();
            if parts.len() == 3 && parts.iter().all(|p| !p.is_empty()) {
                return Some(cand.to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Run `<binary> --version` and parse it. Metadata only — stdout is parsed
/// in memory, never logged (SA-L.1).
pub fn detect_omp_version(binary: &Path) -> Option<String> {
    let out = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_omp_version(&String::from_utf8_lossy(&out.stdout))
}

fn current_omp_version() -> Option<String> {
    let settings = crate::store::load_settings();
    let binary = crate::omp_runtime::resolve_omp_binary(&settings)?;
    detect_omp_version(&binary)
}

/// Pure release-JSON → check result (testable without network).
pub fn parse_omp_release(
    v: &serde_json::Value,
    current: Option<&str>,
) -> Result<OmpUpdateCheck, String> {
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| "release JSON missing tag_name".to_string())?;
    let latest = tag.trim_start_matches('v').to_string();
    let update_available = match current {
        Some(cur) => crate::app_update::is_remote_newer(cur, &latest),
        None => true,
    };
    let wanted = current_asset_name();
    let download_url = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|a| {
                let name = a.get("name")?.as_str()?;
                if Some(name) != wanted {
                    return None;
                }
                a.get("browser_download_url")?
                    .as_str()
                    .map(str::to_string)
            })
        });
    Ok(OmpUpdateCheck {
        current_version: current.map(str::to_string),
        latest_version: latest,
        update_available,
        download_url,
        release_url: RELEASE_PAGE.to_string(),
    })
}

pub async fn check_omp_update() -> Result<OmpUpdateCheck, String> {
    let client = crate::app_update::http_client("omp-desktop-omp-update")?;
    let resp = client
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("network error: {e}"))?;
    if !status.is_success() {
        return Err(crate::app_update::format_http_error(status.as_u16(), &body));
    }
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("bad release JSON: {e}"))?;
    parse_omp_release(&v, current_omp_version().as_deref())
}

pub async fn download_and_apply(url: &str) -> Result<OmpUpdateApplied, String> {
    if !crate::app_update::is_allowed_update_url(url) {
        return Err("download URL not allowed".to_string());
    }
    let client = crate::app_update::http_client("omp-desktop-omp-update")?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::app_update::format_http_error(status.as_u16(), &body));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    apply_omp_bytes(&bytes)
}

/// Write `<app_data>/runtime/omp.new`, then atomically rename over the old
/// upgraded copy. On any failure the previous copy (or the bundled sidecar
/// fallback) is untouched — an upgrade never breaks availability.
pub fn apply_omp_bytes(bytes: &[u8]) -> Result<OmpUpdateApplied, String> {
    if bytes.len() < MIN_BINARY_BYTES {
        return Err("download too small — refusing to install".to_string());
    }
    let dir = crate::paths::app_data_root().join("runtime");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create runtime dir: {e}"))?;
    let target = crate::omp_runtime::upgraded_omp_path();
    let tmp = dir.join(format!("{}.new", crate::omp_runtime::omp_binary_name()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("write download: {e}"))?;
    let sha256 = hex::encode(sha2::Sha256::digest(bytes));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "replace failed — close all sessions and retry ({e})"
        ));
    }
    // Audit record (spec: TLS trust today; SHA kept for later verification).
    let _ = std::fs::write(
        dir.join(format!("{}.sha256", crate::omp_runtime::omp_binary_name())),
        &sha256,
    );
    let version = detect_omp_version(&target);
    Ok(OmpUpdateApplied {
        version,
        sha256,
        path: target.display().to_string(),
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib omp_update`
Expected: 5 PASS. Also run `cargo test --lib app_update` — visibility change must not break existing tests.

- [ ] **Step 6: Register the Tauri commands**

In `src-tauri/src/commands.rs`, immediately after the `app_check_update` command (:237-240), add:

```rust
/// Check upstream omp Runtime for a newer version (Settings → About).
#[tauri::command]
pub async fn omp_check_update() -> Result<crate::omp_update::OmpUpdateCheck, String> {
    crate::omp_update::check_omp_update().await
}

/// Download + apply the newer omp Runtime into <app_data>/runtime/.
#[tauri::command]
pub async fn omp_apply_update(url: String) -> Result<crate::omp_update::OmpUpdateApplied, String> {
    crate::omp_update::download_and_apply(url.trim()).await
}
```

In `src-tauri/src/lib.rs`:
1. Add `mod omp_update;` next to the other module declarations.
2. In the `tauri::generate_handler![` list, add after `commands::app_check_update,`:

```rust
            commands::omp_check_update,
            commands::omp_apply_update,
```

3. In `registered_command_names()`, add after `"app_check_update",`:

```rust
        "omp_check_update",
        "omp_apply_update",
```

- [ ] **Step 7: Full Rust suite + commit**

```bash
cd src-tauri && cargo test --lib && cd ..
git add src-tauri/src/omp_update.rs src-tauri/src/app_update.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(update): in-app omp Runtime upgrade (check + TLS download + atomic apply)"
```

---

### Task 5: Settings UI ("检查 omp 更新") + i18n

**Files:**
- Modify: `src/lib/api.ts` (near :324 `app_check_update` wrapper)
- Modify: `src/components/SettingsPage.tsx` (About section, after the app-update row ending :3366)
- Modify: `src/i18n/messages.ts` (en block + zh-CN block), `src/i18n/zh-tw.ts`

**Interfaces:**
- Consumes: Task 4 commands `omp_check_update` / `omp_apply_update` with camelCase DTOs; existing `t()` from `createT(locale)`; existing CSS classes `settings-row settings-row--stack`, `settings-about-update*`.
- Produces: `ompCheckUpdate()`, `ompApplyUpdate(url)` in api.ts; i18n keys `settings.ompUpdate*` in 3 locales.

- [ ] **Step 1: Add the api wrappers**

In `src/lib/api.ts`, after the existing `app_check_update` wrapper (:324), add:

```ts
export interface OmpUpdateCheck {
  currentVersion: string | null;
  latestVersion: string;
  updateAvailable: boolean;
  downloadUrl: string | null;
  releaseUrl: string;
}

export interface OmpUpdateApplied {
  version: string | null;
  sha256: string;
  path: string;
}

export function ompCheckUpdate(): Promise<OmpUpdateCheck> {
  return invoke<OmpUpdateCheck>("omp_check_update");
}

export function ompApplyUpdate(url: string): Promise<OmpUpdateApplied> {
  return invoke<OmpUpdateApplied>("omp_apply_update", { url });
}
```

- [ ] **Step 2: Add the i18n keys (all 3 locales)**

In `src/i18n/messages.ts`, en block (next to the `settings.releaseChannel*` keys ~:1003):

```ts
  "settings.ompUpdate": "Check omp update",
  "settings.ompUpdateDesc":
    "Update the built-in omp engine independently of the app.",
  "settings.ompUpdateChecking": "Checking…",
  "settings.ompUpdateAvailable": "omp {version} available",
  "settings.ompUpdateLatest": "omp is up to date ({version})",
  "settings.ompUpdateDownload": "Download & install",
  "settings.ompUpdateDownloading": "Downloading…",
  "settings.ompUpdateDone": "Updated to omp {version} — restart the app to apply",
  "settings.ompUpdateError": "omp update failed: {error}",
  "settings.ompUpdateUnavailable":
    "No prebuilt omp for this platform on the latest release",
```

zh-CN block (same key order):

```ts
  "settings.ompUpdate": "检查 omp 更新",
  "settings.ompUpdateDesc": "独立于 App 更新内置的 omp 引擎。",
  "settings.ompUpdateChecking": "检查中…",
  "settings.ompUpdateAvailable": "发现新版 omp {version}",
  "settings.ompUpdateLatest": "omp 已是最新（{version}）",
  "settings.ompUpdateDownload": "下载并安装",
  "settings.ompUpdateDownloading": "下载中…",
  "settings.ompUpdateDone": "已更新到 omp {version}——重启 App 后生效",
  "settings.ompUpdateError": "omp 更新失败：{error}",
  "settings.ompUpdateUnavailable": "最新 release 没有本平台的 omp 预编译包",
```

In `src/i18n/zh-tw.ts`, same keys, zh-TW copy:

```ts
  "settings.ompUpdate": "檢查 omp 更新",
  "settings.ompUpdateDesc": "獨立於 App 更新內建的 omp 引擎。",
  "settings.ompUpdateChecking": "檢查中…",
  "settings.ompUpdateAvailable": "發現新版 omp {version}",
  "settings.ompUpdateLatest": "omp 已是最新（{version}）",
  "settings.ompUpdateDownload": "下載並安裝",
  "settings.ompUpdateDownloading": "下載中…",
  "settings.ompUpdateDone": "已更新到 omp {version}——重啟 App 後生效",
  "settings.ompUpdateError": "omp 更新失敗：{error}",
  "settings.ompUpdateUnavailable": "最新 release 沒有本平台的 omp 預編譯包",
```

- [ ] **Step 3: i18n gate**

Run: `pnpm check:i18n`
Expected: PASS — key counts identical across the 3 locales (previous 1889 → 1900 per locale), zero placeholder mismatches (`{version}` / `{error}` consistent).

- [ ] **Step 4: Add the UI row**

In `src/components/SettingsPage.tsx`, after the closing of the app-update row component (:3366 `}`), add a new component:

```tsx
function OmpUpdateRow({ t }: { t: ReturnType<typeof createT> }) {
  const [phase, setPhase] = useState<
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "checked"; check: OmpUpdateCheck }
    | { kind: "downloading" }
    | { kind: "done"; applied: OmpUpdateApplied }
    | { kind: "error"; message: string }
  >({ kind: "idle" });

  const check = async () => {
    setPhase({ kind: "checking" });
    try {
      setPhase({ kind: "checked", check: await ompCheckUpdate() });
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  const apply = async (url: string) => {
    setPhase({ kind: "downloading" });
    try {
      setPhase({ kind: "done", applied: await ompApplyUpdate(url) });
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  const busy = phase.kind === "checking" || phase.kind === "downloading";

  return (
    <div className="settings-row settings-row--stack">
      <div className="settings-row__text">
        <div className="settings-row__label">{t("settings.ompUpdate")}</div>
        <div className="settings-row__desc">{t("settings.ompUpdateDesc")}</div>
      </div>
      <div className="settings-about-update">
        <div className="settings-about-update__actions">
          <button
            type="button"
            className="btn btn--solid"
            disabled={busy}
            onClick={() => void check()}
          >
            {phase.kind === "checking"
              ? t("settings.ompUpdateChecking")
              : t("settings.ompUpdate")}
          </button>
          {phase.kind === "checked" &&
          phase.check.updateAvailable &&
          phase.check.downloadUrl ? (
            <button
              type="button"
              className="btn btn--solid"
              disabled={busy}
              onClick={() => void apply(phase.kind === "checked" ? phase.check.downloadUrl! : "")}
            >
              {phase.kind === "downloading"
                ? t("settings.ompUpdateDownloading")
                : t("settings.ompUpdateDownload")}
            </button>
          ) : null}
        </div>
        {phase.kind === "checked" ? (
          <div
            className={
              "settings-about-update__status" +
              (phase.check.updateAvailable ? " is-available" : "")
            }
            role="status"
          >
            {phase.check.updateAvailable
              ? phase.check.downloadUrl
                ? t("settings.ompUpdateAvailable", {
                    version: phase.check.latestVersion,
                  })
                : t("settings.ompUpdateUnavailable")
              : t("settings.ompUpdateLatest", {
                  version: phase.check.latestVersion,
                })}
          </div>
        ) : null}
        {phase.kind === "done" ? (
          <div className="settings-about-update__status is-available" role="status">
            {t("settings.ompUpdateDone", {
              version: phase.applied.version ?? "",
            })}
          </div>
        ) : null}
        {phase.kind === "error" ? (
          <div className="settings-about-update__err" role="alert">
            {t("settings.ompUpdateError", { error: phase.message })}
          </div>
        ) : null}
      </div>
    </div>
  );
}
```

Then render it in the About section immediately below the existing app-update row (find where the update row component is invoked — the JSX sibling of the row ending at :3366 — and add `<OmpUpdateRow t={t} />` after it).

Notes for the implementer:
- Match the existing row component's prop pattern: if the neighboring row takes `t` (or uses a hook), mirror it exactly; import `createT` type / `useState` from the same modules already imported at the top of the file, and import `ompCheckUpdate, ompApplyUpdate, OmpUpdateCheck, OmpUpdateApplied` from `../lib/api`.
- Narrowing: the `apply(phase.check.downloadUrl!)` call is safe — the button only renders when `downloadUrl` is non-null; keep the guard as written and let TS narrowing do the rest (adjust to satisfy `tsc` if it complains, e.g. hoist `const dl = phase.check.downloadUrl` inside the `checked` branch).

- [ ] **Step 5: Typecheck + vitest**

Run: `pnpm typecheck && pnpm test`
Expected: PASS (no type errors; existing 843 vitest unaffected — this task adds no test obligations, UI verified manually in Task 6).

- [ ] **Step 6: Brand check + commit**

```bash
pnpm check:brand
git add src/lib/api.ts src/components/SettingsPage.tsx src/i18n/messages.ts src/i18n/zh-tw.ts
git commit -m "feat(settings): 检查 omp 更新 row in About (3-locale i18n)"
```

---

### Task 6: Docs + full gates + memory

**Files:**
- Modify: `README.md` (zh), `README_EN.md` (en)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above (behavior: bundled default, manual override, in-app upgrade).
- Produces: user-facing docs consistent with the new behavior.

- [ ] **Step 1: Update the Runtime-install story in both READMEs**

Locate the stale lines:

```bash
grep -n -i "runtime" README.md README_EN.md | grep -i -E "install|安装|manual|手动|指定|CLI"
```

Replace the "user must install omp first / point at CLI path" guidance with (keep the surrounding format/heading level):

README.md (zh):

```markdown
OMP Desktop 自带 omp 引擎，开箱即用，无需单独安装。高级用户仍可在 设置 → 手动指定 CLI 路径 覆盖内置版本；设置 → 关于 里的「检查 omp 更新」可独立于 App 升级 omp。
```

README_EN.md (en):

```markdown
OMP Desktop ships with the omp engine built in — no separate install needed. Advanced users can still override it via Settings → Manual CLI path; the "Check omp update" button under Settings → About upgrades omp independently of the app.
```

- [ ] **Step 2: CHANGELOG entry**

At the top of `CHANGELOG.md` (after the title/intro, before the first existing version section), add:

```markdown
## [Unreleased]

- **Bundled omp Runtime:** the app now ships with the omp engine compiled from
  the pinned `runtime/oh-my-pi` submodule — works out of the box, no manual CLI
  install. Resolution order: manual override → in-app upgraded copy → bundled
  sidecar. Settings → About gains "检查 omp 更新" to upgrade omp independently
  from upstream `can1357/oh-my-pi` releases.
- **内置 omp 引擎：** App 现在自带由 `runtime/oh-my-pi` submodule 编译的 omp——开箱即用。
  解析顺序：手动指定 → App 内升级副本 → 内置副本。设置 → 关于 新增「检查 omp 更新」，
  可独立于 App 从上游 `can1357/oh-my-pi` 升级 omp。
```

- [ ] **Step 3: Full gates**

```bash
cd src-tauri && cargo test --lib && cd ..
pnpm test
pnpm typecheck
pnpm check:i18n
pnpm check:brand
pnpm check:provenance
pnpm check:legal
```

Expected: all green. (If the known flake `store::tests::ensure_general_project_is_idempotent_and_not_removable` or a sandbox storm appears, re-run once before investigating.)

- [ ] **Step 4: Manual smoke (dev build)**

```bash
pnpm dev
```

In the app: Settings → About → click 检查 omp 更新 → expect "发现新版 omp …" (bundled pin is behind upstream) → 下载并安装 → expect "已更新到 omp …——重启 App 后生效". Restart the app; welcome screen should advance past the CLI gate with zero manual config (bundled/upgraded binary resolves).

- [ ] **Step 5: Commit + push**

```bash
git add README.md README_EN.md CHANGELOG.md
git commit -m "docs: bundled omp Runtime (README bilingual + changelog)"
git push
```

- [ ] **Step 6: Update agent memory**

Update `omp-desktop-roadmap-status.md` and the bundling-interest memory: bundled-omp implemented (tasks 1-6), `omp-desktop-runtime-bundling-interest.md` → mark brainstorming finished + implemented, link the spec/plan paths.

---

## Self-Review Notes (already applied)

- **Spec coverage:** resolver (Task 1) ✓, bundle config + local verify (Task 2) ✓, CI (Task 3) ✓, upgrade backend (Task 4) ✓, upgrade UI + i18n (Task 5) ✓, error-boundary behaviors covered by Task 4 implementation (tiny-download rejection, rename-failure keeps old, fail-closed resolution) ✓, Testing section of spec → Task 1/4 unit tests + Task 3 CI `--version` echo + Task 6 manual smoke ✓.
- **Deviations from spec (deliberate):** (a) version cache in Settings dropped — live `--version` detect per check is cheap and has no invalidation failure mode (YAGNI); SHA256 audit record lives in `<app_data>/runtime/omp.sha256` sidecar file instead of Settings for the same reason. (b) No mock GitHub server test — `parse_omp_release` is pure-tested with fixture JSON and `apply_omp_bytes` with in-memory bytes; the network seam is 6 thin lines.
- **Type consistency:** `OmpUpdateCheck`/`OmpUpdateApplied` field names (Rust snake → serde camelCase) match the TS interfaces in Task 5 Step 1 (`currentVersion`, `latestVersion`, `updateAvailable`, `downloadUrl`, `releaseUrl`; `version`, `sha256`, `path`). Command names `omp_check_update`/`omp_apply_update` identical in commands.rs, lib.rs handler list, `registered_command_names`, and api.ts.
