# Update Channels (stable/beta/nightly) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement build-time-baked update channel isolation (stable/beta/nightly) so AC-10.9 flips FAIL → PASS.

**Architecture:** Channel identity derives from the version string (prerelease suffix: `nightly`/`beta`/none). CI derives channel from the tag and bakes the per-channel feed endpoint into the build; each channel publishes to its own rolling GitHub release + manifest. The manual GitHub fallback path becomes channel-aware with prerelease-aware semver comparison. Frontend displays the baked channel identity (no runtime switcher — Chrome/VS Code model).

**Tech Stack:** Rust (semver crate, already in dep tree), bash + node (.mjs, node:test) for CI scripts, React/TS for display, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-07-31-update-channels-design.md` (decisions D1–D8 — read first)

## Global Constraints

- New dependency allowed: `semver = "1"` only (already in tree via tauri-plugin-updater — zero new compiled code). No other new deps.
- Scripts never print secret values; logs metadata only.
- User-facing copy goes through i18n `t()` — en + zh-CN (`src/i18n/messages.ts`) and zh-TW (`src/i18n/zh-tw.ts`); `pnpm check:i18n` must stay green.
- Product name **OMP Desktop**; run `pnpm check:brand` before committing.
- `cargo test --lib` does NOT rebuild bin targets — irrelevant here (no bin changes), but always run gates from repo root: `cd /Users/po1nt9/Github/grok-app-main` first (cwd resets between Bash calls).
- macOS has no `timeout` command.
- One commit per task, English message, tag `AC-10.9`.
- `DUAL_PUBLISH_LEGACY_LATEST` (D5 transition) applies to nightly only; stable/beta never dual-publish.
- Do not commit `tauri.release.conf.json` changes produced by local script runs (it is generated; check `git status` before each commit).

---

### Task 1: Rust `UpdateChannel` module + `updater_status.release_channel`

**Files:**
- Create: `src-tauri/src/update_channel.rs`
- Modify: `src-tauri/src/lib.rs:8` (add mod declaration after `mod updater;`)
- Modify: `src-tauri/src/updater.rs:61-98` (DTO + populate) and its test module
- Modify: `src-tauri/Cargo.toml:36` (add semver dep near serde)

**Interfaces:**
- Produces: `crate::update_channel::UpdateChannel` (`Stable`/`Beta`/`Nightly`), `UpdateChannel::from_version(&str) -> UpdateChannel`, `.as_str() -> &'static str`, `.owns_tag(&self, tag: &str) -> bool`. Task 2 consumes all three; Task 4 relies on `updater_status` JSON gaining `releaseChannel` (camelCase serde).

- [ ] **Step 1: Add semver dependency**

In `src-tauri/Cargo.toml`, after the `serde_json = "1"` line insert:

```toml
# Prerelease-aware version ordering for update channels (AC-10.9). Already in
# the tree via tauri-plugin-updater — no new compiled code.
semver = "1"
```

- [ ] **Step 2: Write the failing module (tests only reference missing impl)**

Create `src-tauri/src/update_channel.rs`:

```rust
//! Release channel identity (stable / beta / nightly) — build-time baked.
//!
//! The channel is a property of the installed build, derived from the version
//! string (`env!("CARGO_PKG_VERSION")`): prerelease containing `nightly` →
//! Nightly, `beta` → Beta, otherwise Stable. Users pick a channel by installing
//! that channel's build (Chrome/VS Code model); there is no runtime switch.
//! See docs/superpowers/specs/2026-07-31-update-channels-design.md (AC-10.9, D1).

/// Release channel this build tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_version_maps_prerelease_channels() {
        assert_eq!(UpdateChannel::from_version("1.0.0"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("v1.0.0"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("1.1.0-beta.1"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_version("v1.1.0-beta.2"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_version("0.3.1-nightly"), UpdateChannel::Nightly);
        assert_eq!(
            UpdateChannel::from_version("1.1.0-nightly.20260801"),
            UpdateChannel::Nightly
        );
        assert_eq!(UpdateChannel::from_version("0.0.0"), UpdateChannel::Stable);
    }

    #[test]
    fn from_version_falls_back_to_stable_on_garbage() {
        assert_eq!(UpdateChannel::from_version(""), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("nope"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("1.0"), UpdateChannel::Stable);
    }

    #[test]
    fn as_str_lowercase_ids() {
        assert_eq!(UpdateChannel::Stable.as_str(), "stable");
        assert_eq!(UpdateChannel::Beta.as_str(), "beta");
        assert_eq!(UpdateChannel::Nightly.as_str(), "nightly");
    }

    #[test]
    fn owns_tag_matches_channel_membership() {
        assert!(UpdateChannel::Nightly.owns_tag("v0.3.1-nightly"));
        assert!(!UpdateChannel::Nightly.owns_tag("v1.0.0"));
        assert!(UpdateChannel::Stable.owns_tag("1.0.0"));
        assert!(!UpdateChannel::Stable.owns_tag("1.1.0-beta.1"));
        assert!(UpdateChannel::Beta.owns_tag("v1.1.0-beta.1"));
    }
}
```

In `src-tauri/src/lib.rs`, after line 8 (`mod updater;`) insert:

```rust
mod update_channel;
```

- [ ] **Step 3: Run tests to verify they fail (compile error)**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib update_channel 2>&1 | tail -5`
Expected: FAIL — `no method named from_version` / `as_str` / `owns_tag` compile errors.

- [ ] **Step 4: Implement `UpdateChannel`**

In `src-tauri/src/update_channel.rs`, after the enum insert:

```rust
impl UpdateChannel {
    /// Lowercase channel id for DTOs / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }

    /// Derive the channel from a version string (`1.0.0`, `1.1.0-beta.1`,
    /// `0.3.1-nightly`, `1.1.0-nightly.20260801`; optional leading `v`).
    /// Unparseable versions fall back to Stable (most conservative feed).
    pub fn from_version(version: &str) -> Self {
        let v = version.trim().trim_start_matches(['v', 'V']);
        let Ok(parsed) = semver::Version::parse(v) else {
            return Self::Stable;
        };
        let pre = parsed.pre.as_str().to_ascii_lowercase();
        if pre.contains("nightly") {
            Self::Nightly
        } else if pre.contains("beta") {
            Self::Beta
        } else {
            Self::Stable
        }
    }

    /// True when a release tag belongs to this channel.
    pub fn owns_tag(self, tag: &str) -> bool {
        Self::from_version(tag) == self
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib update_channel 2>&1 | tail -3`
Expected: `test result: ok. 4 passed`

- [ ] **Step 6: Add `release_channel` to `UpdaterStatusDto`**

In `src-tauri/src/updater.rs`, change the struct (lines 61-72):

```rust
/// Snapshot for About / Doctor: which update path this binary can use.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterStatusDto {
    /// Platform packaging supports silent install (e.g. not Linux .deb).
    pub platform_supported: bool,
    /// Release binary built with signing pubkey + endpoint.
    pub plugin_enabled: bool,
    /// `silent` when plugin path is live; otherwise `github_manual`.
    pub channel: String,
    /// Release channel baked into this build (`stable` / `beta` / `nightly`),
    /// derived from the version string (AC-10.9 — no runtime switch).
    pub release_channel: String,
    /// Compile-time endpoint (empty when plugin off).
    pub endpoint: String,
}
```

In `updater_status()`, before the `UpdaterStatusDto { … }` literal insert:

```rust
    let release_channel =
        crate::update_channel::UpdateChannel::from_version(env!("CARGO_PKG_VERSION"))
            .as_str()
            .to_string();
```

and add `release_channel,` to the struct literal (after `channel,`).

In the test module, add:

```rust
    #[test]
    fn updater_status_release_channel_matches_version() {
        let s = updater_status();
        let expect =
            crate::update_channel::UpdateChannel::from_version(env!("CARGO_PKG_VERSION"));
        assert_eq!(s.release_channel, expect.as_str());
        assert!(matches!(
            s.release_channel.as_str(),
            "stable" | "beta" | "nightly"
        ));
    }
```

- [ ] **Step 7: Run full Rust gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3`
Expected: `test result: ok. 503 passed` (498 + 5 new) + 1 ignored.

- [ ] **Step 8: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/update_channel.rs src-tauri/src/lib.rs src-tauri/src/updater.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(updater): UpdateChannel identity from version string + status DTO (AC-10.9)"
```

---

### Task 2: `app_update.rs` channel-aware manual path + prerelease-aware comparison

**Files:**
- Modify: `src-tauri/src/app_update.rs` (comparison :61-67, new selection fn, `check_app_update` :393-423, tests)

**Interfaces:**
- Consumes: `crate::update_channel::UpdateChannel` + `owns_tag` (Task 1).
- Produces: `select_release_for_channel(releases: &[Value], channel: UpdateChannel) -> Option<&Value>` (pub, test-covered). `is_remote_newer` keeps its signature but becomes prerelease-aware. `parse_semver` stays unchanged (sanity check for tag URLs only).

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/app_update.rs` test module, add:

```rust
    #[test]
    fn is_remote_newer_prerelease_aware() {
        // Channel-internal ordering (the old tuple parser called these equal).
        assert!(is_remote_newer(
            "0.3.1-nightly.20260730",
            "0.3.1-nightly.20260731"
        ));
        assert!(is_remote_newer("1.1.0-beta.1", "v1.1.0-beta.2"));
        // Cross-channel semver truth: stable outranks same-version prerelease.
        assert!(is_remote_newer("1.0.0-nightly.20260801", "1.0.0"));
        assert!(!is_remote_newer("1.0.0", "1.0.0-nightly.20260801"));
        // Same version on the same channel → no update.
        assert!(!is_remote_newer("0.3.1-nightly", "v0.3.1-nightly"));
    }

    #[test]
    fn select_release_for_channel_picks_newest_owned_tag() {
        let releases = json!([
            {"tag_name": "v1.0.0", "draft": false},
            {"tag_name": "v1.1.0-nightly.20260801", "draft": false},
            {"tag_name": "v1.1.0-nightly.20260730", "draft": false},
            {"tag_name": "v1.1.0-beta.1", "draft": false},
            {"tag_name": "v9.9.9-nightly.draft", "draft": true}
        ]);
        let arr = releases.as_array().unwrap();
        use crate::update_channel::UpdateChannel::*;
        let n = select_release_for_channel(arr, Nightly).unwrap();
        assert_eq!(
            n.get("tag_name").and_then(|t| t.as_str()),
            Some("v1.1.0-nightly.20260801")
        );
        let b = select_release_for_channel(arr, Beta).unwrap();
        assert_eq!(
            b.get("tag_name").and_then(|t| t.as_str()),
            Some("v1.1.0-beta.1")
        );
        let s = select_release_for_channel(arr, Stable).unwrap();
        assert_eq!(s.get("tag_name").and_then(|t| t.as_str()), Some("v1.0.0"));
    }

    #[test]
    fn select_release_for_channel_none_when_no_owned_tag() {
        let releases = json!([{"tag_name": "v1.0.0"}]);
        let arr = releases.as_array().unwrap();
        assert!(select_release_for_channel(arr, crate::update_channel::UpdateChannel::Nightly).is_none());
        assert!(select_release_for_channel(&[], crate::update_channel::UpdateChannel::Stable).is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib app_update 2>&1 | tail -8`
Expected: FAIL — `select_release_for_channel` unresolved; `is_remote_newer_prerelease_aware` fails (old tuple parser treats prereleases as equal).

- [ ] **Step 3: Implement prerelease-aware comparison + channel selection**

In `src-tauri/src/app_update.rs`, replace `is_remote_newer` (lines 61-67) with:

```rust
/// Full semver parse (prerelease-aware) for update comparison.
fn parse_full_semver(raw: &str) -> Option<semver::Version> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(s).ok()
}

/// True when `remote` is a higher semver than `current` (prerelease-aware:
/// `0.3.1-nightly.20260731` > `0.3.1-nightly.20260730`, `1.0.0` >
/// `1.0.0-nightly.1`). Unparseable input → false (never offer a bogus update).
pub fn is_remote_newer(current: &str, remote: &str) -> bool {
    match (parse_full_semver(current), parse_full_semver(remote)) {
        (Some(a), Some(b)) => b > a,
        _ => false,
    }
}

/// Pick the newest release belonging to `channel` from a `/releases` list
/// payload (drafts excluded). Channel membership is defined by the tag's
/// prerelease segment via [`crate::update_channel::UpdateChannel::owns_tag`];
/// ordering is semver, not list position (AC-10.9, D6).
pub fn select_release_for_channel<'a>(
    releases: &'a [Value],
    channel: crate::update_channel::UpdateChannel,
) -> Option<&'a Value> {
    releases
        .iter()
        .filter(|r| r.get("draft").and_then(|d| d.as_bool()) != Some(true))
        .filter(|r| {
            r.get("tag_name")
                .and_then(|t| t.as_str())
                .map(|t| channel.owns_tag(t))
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            let ta = a.get("tag_name").and_then(|t| t.as_str()).unwrap_or("");
            let tb = b.get("tag_name").and_then(|t| t.as_str()).unwrap_or("");
            parse_full_semver(ta).cmp(&parse_full_semver(tb))
        })
}
```

- [ ] **Step 4: Make `check_app_update` channel-aware**

Replace the body of `check_app_update` (lines 393-423) with:

```rust
/// Query GitHub for the latest release *on this build's channel* and compare.
///
/// Stable keeps `/releases/latest` + the HTML redirect fallback. Beta/nightly
/// list recent releases and pick the newest tag their channel owns — the
/// `/latest` endpoints are stable-only by GitHub semantics, and the HTML
/// redirect resolves to the newest stable tag (wrong channel), so non-stable
/// channels skip the HTML fallback and surface API errors directly (AC-10.9).
pub async fn check_app_update() -> Result<AppUpdateCheck, String> {
    let current = env!("CARGO_PKG_VERSION");
    let channel = crate::update_channel::UpdateChannel::from_version(current);
    let api_url = std::env::var("GROK_APP_RELEASES_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASES_API_URL.into());
    let html_url = std::env::var("GROK_APP_RELEASES_HTML_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASES_HTML_URL.into());

    if !is_allowed_update_url(&api_url) {
        return Err("update check URL must be https (or localhost for tests)".into());
    }
    if !is_allowed_update_url(&html_url) {
        return Err("update fallback URL must be https (or localhost for tests)".into());
    }

    let ua = format!(
        "OMP-Desktop/{current} (desktop; check-update; +https://github.com/Po1nt9/omp-desktop)"
    );
    let client = http_client(&ua)?;

    if channel == crate::update_channel::UpdateChannel::Stable {
        return match fetch_via_api(&client, &api_url).await {
            Ok(v) => parse_github_release(current, &v),
            Err(api_err) => {
                tracing::warn!(error = %api_err, "app update API failed; trying HTML redirect fallback");
                match fetch_via_html_redirect(&client, &html_url, current).await {
                    Ok(check) => Ok(check),
                    Err(fallback_err) => Err(format!("{api_err} | {fallback_err}")),
                }
            }
        };
    }

    let list_url = format!(
        "{}?per_page=30",
        api_url.strip_suffix("/latest").unwrap_or(&api_url)
    );
    let v = fetch_via_api(&client, &list_url).await?;
    let empty = Vec::new();
    let releases = v.as_array().unwrap_or(&empty);
    match select_release_for_channel(releases, channel) {
        Some(release) => parse_github_release(current, release),
        // No release published on this channel yet — report up-to-date,
        // pointing at the releases page (spec §5: never error-pop for this).
        None => Ok(AppUpdateCheck {
            current_version: current.to_string(),
            latest_version: current.to_string(),
            update_available: false,
            release_name: None,
            html_url: DEFAULT_RELEASES_PAGE.to_string(),
            published_at: None,
            body: None,
            asset_names: vec![],
            download_url: None,
            download_name: None,
        }),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib app_update 2>&1 | tail -3`
Expected: `test result: ok. 9 passed` (6 old + 3 new).

- [ ] **Step 6: Run full Rust gate**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3`
Expected: `test result: ok. 506 passed` + 1 ignored.

- [ ] **Step 7: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/app_update.rs
git commit -m "feat(updater): channel-aware manual update path + prerelease semver compare (AC-10.9)"
```

---

### Task 3: JS channel lib + CI wiring (release.yml, assemble, verify, release-config)

**Files:**
- Create: `scripts/update-channel-lib.mjs`
- Create: `scripts/update-channel-lib.test.mjs`
- Modify: `scripts/build-release-config.mjs`
- Modify: `scripts/assemble-updater-manifest.sh`
- Modify: `scripts/verify-updater-setup.sh:89-113`
- Modify: `.github/workflows/release.yml:146-179, 199-228`

**Interfaces:**
- Produces: `channelFromVersion(tag) -> "stable"|"beta"|"nightly"`, `endpointFor(repo, channel) -> url`, `isPrerelease(channel) -> bool`, `rollingTagFor(channel)`, `manifestNameFor(channel)` + CLI (`node scripts/update-channel-lib.mjs channel|endpoint|prerelease …`) consumed by release.yml. Bash side consumes `CHANNEL`, `DUAL_PUBLISH_LEGACY_LATEST`, `PRINT_DERIVED` env in assemble-updater-manifest.sh.

- [ ] **Step 1: Write the failing test**

Create `scripts/update-channel-lib.test.mjs`:

```js
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import {
  channelFromTag,
  channelFromVersion,
  endpointFor,
  isPrerelease,
  manifestNameFor,
  rollingTagFor,
} from "./update-channel-lib.mjs";

test("channelFromVersion maps prerelease suffixes", () => {
  assert.equal(channelFromVersion("1.0.0"), "stable");
  assert.equal(channelFromVersion("v1.0.0"), "stable");
  assert.equal(channelFromVersion("0.3.1-nightly"), "nightly");
  assert.equal(channelFromVersion("1.1.0-nightly.20260801"), "nightly");
  assert.equal(channelFromVersion("1.1.0-beta.1"), "beta");
  assert.equal(channelFromVersion("v1.1.0-beta.2"), "beta");
  assert.equal(channelFromVersion(""), "stable");
});

test("channelFromTag mirrors channelFromVersion", () => {
  assert.equal(channelFromTag("v0.3.1-nightly"), "nightly");
});

test("rolling tag + manifest per channel (stable keeps legacy feed)", () => {
  assert.equal(rollingTagFor("stable"), "omp-desktop-latest");
  assert.equal(manifestNameFor("stable"), "latest.json");
  assert.equal(rollingTagFor("beta"), "omp-desktop-beta");
  assert.equal(manifestNameFor("beta"), "beta.json");
  assert.equal(rollingTagFor("nightly"), "omp-desktop-nightly");
  assert.equal(manifestNameFor("nightly"), "nightly.json");
  assert.throws(() => rollingTagFor("canary"), /unknown update channel/);
});

test("endpointFor builds the rolling-release manifest URL", () => {
  assert.equal(
    endpointFor("owner/omp-desktop", "nightly"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-nightly/nightly.json",
  );
  assert.equal(
    endpointFor("owner/omp-desktop", "stable"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-latest/latest.json",
  );
});

test("isPrerelease: everything except stable", () => {
  assert.equal(isPrerelease("stable"), false);
  assert.equal(isPrerelease("beta"), true);
  assert.equal(isPrerelease("nightly"), true);
});

test("CLI prints channel / endpoint / prerelease for CI steps", () => {
  const run = (...args) =>
    execFileSync("node", ["scripts/update-channel-lib.mjs", ...args], {
      encoding: "utf8",
    }).trim();
  assert.equal(run("channel", "v1.1.0-beta.1"), "beta");
  assert.equal(
    run("endpoint", "owner/omp-desktop", "v1.1.0-beta.1"),
    "https://github.com/owner/omp-desktop/releases/download/omp-desktop-beta/beta.json",
  );
  assert.equal(run("prerelease", "v1.0.0"), "false");
  assert.equal(run("prerelease", "v0.3.1-nightly"), "true");
});

const runAssembleDerived = (extraEnv) =>
  execFileSync("bash", ["scripts/assemble-updater-manifest.sh"], {
    env: { ...process.env, PRINT_DERIVED: "1", ...extraEnv },
    encoding: "utf8",
  });

test("assemble-updater-manifest PRINT_DERIVED maps all three channels", () => {
  const stable = runAssembleDerived({ CHANNEL: "stable", ROLLING_TAG: "" });
  assert.match(stable, /CHANNEL=stable/);
  assert.match(stable, /ROLLING_TAG=omp-desktop-latest/);
  assert.match(stable, /MANIFEST_NAME=latest\.json/);
  const beta = runAssembleDerived({ CHANNEL: "beta", ROLLING_TAG: "" });
  assert.match(beta, /ROLLING_TAG=omp-desktop-beta/);
  assert.match(beta, /MANIFEST_NAME=beta\.json/);
  const nightly = runAssembleDerived({ CHANNEL: "nightly", ROLLING_TAG: "" });
  assert.match(nightly, /ROLLING_TAG=omp-desktop-nightly/);
  assert.match(nightly, /MANIFEST_NAME=nightly\.json/);
});

test("assemble-updater-manifest rejects unknown CHANNEL (fail-closed)", () => {
  assert.throws(
    () => runAssembleDerived({ CHANNEL: "canary", ROLLING_TAG: "" }),
    /unknown CHANNEL/,
  );
});
```

Note: `ROLLING_TAG: ""` overrides any leaked env so the default derivation is exercised (bash `${ROLLING_TAG:-…}` treats empty as unset).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/po1nt9/Github/grok-app-main && node --test scripts/update-channel-lib.test.mjs 2>&1 | tail -5`
Expected: FAIL — `Cannot find module './update-channel-lib.mjs'`.

- [ ] **Step 3: Create the channel lib**

Create `scripts/update-channel-lib.mjs`:

```js
// Single source of truth for update-channel derivation (AC-10.9).
// Channels are build-time identities derived from the version/tag string —
// see docs/superpowers/specs/2026-07-31-update-channels-design.md (D1/D2/D7).
//
// CLI (used by release.yml steps):
//   node scripts/update-channel-lib.mjs channel v1.1.0-beta.1
//   node scripts/update-channel-lib.mjs endpoint owner/repo v1.1.0-beta.1
//   node scripts/update-channel-lib.mjs prerelease v1.1.0-beta.1

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const CHANNELS = ["stable", "beta", "nightly"];

export const ROLLING_TAGS = {
  stable: "omp-desktop-latest",
  beta: "omp-desktop-beta",
  nightly: "omp-desktop-nightly",
};

export const MANIFEST_NAMES = {
  stable: "latest.json",
  beta: "beta.json",
  nightly: "nightly.json",
};

/** "v1.1.0-beta.1" → "beta"; "0.3.1-nightly" → "nightly"; "1.0.0" → "stable". */
export function channelFromVersion(version) {
  const v = String(version ?? "")
    .trim()
    .replace(/^[vV]/, "");
  const pre = v.includes("-") ? v.slice(v.indexOf("-") + 1).toLowerCase() : "";
  if (pre.includes("nightly")) return "nightly";
  if (pre.includes("beta")) return "beta";
  return "stable";
}

export function channelFromTag(tag) {
  return channelFromVersion(tag);
}

export function rollingTagFor(channel) {
  const tag = ROLLING_TAGS[channel];
  if (!tag) throw new Error(`unknown update channel: ${channel}`);
  return tag;
}

export function manifestNameFor(channel) {
  const name = MANIFEST_NAMES[channel];
  if (!name) throw new Error(`unknown update channel: ${channel}`);
  return name;
}

/** Feed endpoint for a repo + channel: the rolling-release manifest URL. */
export function endpointFor(repo, channel) {
  return `https://github.com/${repo}/releases/download/${rollingTagFor(channel)}/${manifestNameFor(channel)}`;
}

/** GitHub Release prerelease flag: everything except stable is a prerelease. */
export function isPrerelease(channel) {
  return channel !== "stable";
}

const isMain =
  process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);

if (isMain) {
  const [cmd, a, b] = process.argv.slice(2);
  try {
    if (cmd === "channel" && a) {
      console.log(channelFromTag(a));
    } else if (cmd === "endpoint" && a && b) {
      console.log(endpointFor(a, channelFromTag(b)));
    } else if (cmd === "prerelease" && a) {
      console.log(String(isPrerelease(channelFromTag(a))));
    } else {
      console.error(
        "usage: update-channel-lib.mjs channel <tag> | endpoint <owner/repo> <tag> | prerelease <tag>",
      );
      process.exit(1);
    }
  } catch (err) {
    console.error(String(err?.message ?? err));
    process.exit(1);
  }
}
```

- [ ] **Step 4: Wire channel derivation into assemble-updater-manifest.sh**

In `scripts/assemble-updater-manifest.sh`:

Replace lines 14-18 (the env defaults block) with:

```bash
TAG="${TAG:-}"
REPO="${REPO:-${GITHUB_REPOSITORY:-}}"
CHANNEL="${CHANNEL:-stable}"
case "$CHANNEL" in
  stable)  DEFAULT_ROLLING="omp-desktop-latest";  MANIFEST_NAME="latest.json" ;;
  beta)    DEFAULT_ROLLING="omp-desktop-beta";    MANIFEST_NAME="beta.json" ;;
  nightly) DEFAULT_ROLLING="omp-desktop-nightly"; MANIFEST_NAME="nightly.json" *)
    echo "error: unknown CHANNEL='$CHANNEL' (stable|beta|nightly)" >&2
    exit 1
    ;;
esac
ROLLING_TAG="${ROLLING_TAG:-$DEFAULT_ROLLING}"
# Transitional (AC-10.9 D5): nightly also feeds the legacy omp-desktop-latest
# manifest until the first stable release ships; then delete this flag.
DUAL_PUBLISH_LEGACY_LATEST="${DUAL_PUBLISH_LEGACY_LATEST:-0}"
WORK="${WORK:-/tmp/omp-desktop-updater-assets}"
PLATFORM_HINTS="${PLATFORM_HINTS:-}"

# Dry-run for tests: print derived routing values and exit before any network.
if [[ "${PRINT_DERIVED:-0}" == "1" ]]; then
  echo "CHANNEL=$CHANNEL"
  echo "ROLLING_TAG=$ROLLING_TAG"
  echo "MANIFEST_NAME=$MANIFEST_NAME"
  exit 0
fi
```

Careful — the `*)` case above must be on its own lines (fix the snippet when applying):

```bash
case "$CHANNEL" in
  stable)  DEFAULT_ROLLING="omp-desktop-latest";  MANIFEST_NAME="latest.json" ;;
  beta)    DEFAULT_ROLLING="omp-desktop-beta";    MANIFEST_NAME="beta.json" ;;
  nightly) DEFAULT_ROLLING="omp-desktop-nightly"; MANIFEST_NAME="nightly.json" ;;
  *)
    echo "error: unknown CHANNEL='$CHANNEL' (stable|beta|nightly)" >&2
    exit 1
    ;;
esac
```

Update the header comment (lines 2-3) to mention channels:

```bash
# Download versioned release assets, map platform archives + .sig files, write
# the channel manifest, and clobber-upload to the per-channel rolling release
# (stable → omp-desktop-latest/latest.json, beta → omp-desktop-beta/beta.json,
# nightly → omp-desktop-nightly/nightly.json; AC-10.9).
```

Replace the manifest generation + upload section (lines 186-211) with:

```bash
bash "$SCRIPT_DIR/generate-latest-json.sh" "$VERSION" "${TRIPLES[@]}" > "$MANIFEST_NAME"
echo "==> $MANIFEST_NAME"
cat "$MANIFEST_NAME"

# Ensure rolling release exists.
if ! gh release view "$ROLLING_TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "==> Creating rolling release $ROLLING_TAG"
  gh release create "$ROLLING_TAG" \
    --repo "$REPO" \
    --title "OMP Desktop auto-updater (rolling, $CHANNEL)" \
    --notes "Rolling $CHANNEL feed for the Tauri auto-updater. Prefer versioned vX.Y.Z releases for first-time installs." \
    --latest=false || true
fi

echo "==> Uploading archives + $MANIFEST_NAME to $ROLLING_TAG"
declare -A SEEN=()
for f in "${UPLOAD_FILES[@]}" "$MANIFEST_NAME"; do
  [[ -n "${SEEN[$f]:-}" ]] && continue
  SEEN[$f]=1
  gh release upload "$ROLLING_TAG" "$f" --repo "$REPO" --clobber
done

# Also attach the manifest to the versioned release for humans / debugging.
gh release upload "$TAG" "$MANIFEST_NAME" --repo "$REPO" --clobber || true

# Transitional dual-publish (D5): installed v0.3.x-nightly builds still poll
# omp-desktop-latest/latest.json. Feed it from nightly tags until the first
# stable release ships. Manifest URLs keep pointing at the nightly rolling
# release (public); archives are mirrored so both locations serve. Nightly
# only — stable/beta must never touch another channel's feed.
if [[ "$CHANNEL" == "nightly" && "$DUAL_PUBLISH_LEGACY_LATEST" == "1" ]]; then
  echo "==> Transitional dual-publish: latest.json → omp-desktop-latest"
  cp "$MANIFEST_NAME" latest.json
  if ! gh release view "omp-desktop-latest" --repo "$REPO" >/dev/null 2>&1; then
    gh release create "omp-desktop-latest" \
      --repo "$REPO" \
      --title "OMP Desktop auto-updater (rolling, stable)" \
      --notes "Rolling stable feed for the Tauri auto-updater." \
      --latest=false || true
  fi
  declare -A SEEN_LEGACY=()
  for f in "${UPLOAD_FILES[@]}" latest.json; do
    [[ -n "${SEEN_LEGACY[$f]:-}" ]] && continue
    SEEN_LEGACY[$f]=1
    gh release upload "omp-desktop-latest" "$f" --repo "$REPO" --clobber
  done
fi

echo "==> Done"
```

- [ ] **Step 5: Channel-aware endpoint in build-release-config.mjs**

In `scripts/build-release-config.mjs`, replace the env block (lines 30-41) with:

```js
import { channelFromVersion, endpointFor } from "./update-channel-lib.mjs";

const updaterPubkey = process.env.OMP_DESKTOP_UPDATER_PUBLIC_KEY;
// Channel from the release version being built (tag); endpoint precedence:
// explicit OMP_DESKTOP_UPDATER_ENDPOINT override → channel-derived from repo.
const releaseVersion = process.env.OMP_DESKTOP_RELEASE_VERSION ?? "";
const channel = channelFromVersion(releaseVersion);
const updaterEndpoint =
  process.env.OMP_DESKTOP_UPDATER_ENDPOINT ??
  (process.env.GITHUB_REPOSITORY
    ? endpointFor(process.env.GITHUB_REPOSITORY, channel)
    : undefined);

const missing = [];
if (!updaterPubkey) missing.push("OMP_DESKTOP_UPDATER_PUBLIC_KEY");
if (!updaterEndpoint)
  missing.push("OMP_DESKTOP_UPDATER_ENDPOINT (or GITHUB_REPOSITORY to derive)");
if (missing.length > 0) {
  console.error(
    `Error: required environment variable(s) missing: ${missing.join(", ")}`,
  );
  process.exit(1);
}
```

Note the existing `import` lines at the top stay; add the new import below them (or merge into this replacement — when applying, ensure only one import block results). Update the log line:

```js
console.log(`Update channel  -> ${channel} (version ${releaseVersion || "unknown"})`);
console.log(`Updater enabled -> ${updaterEndpoint}`);
```

- [ ] **Step 6: Channel-aware --fetch-latest in verify-updater-setup.sh**

In `scripts/verify-updater-setup.sh`, replace the section-5 header lines (89-91) with:

```bash
# 5) Optional live channel manifest (OMP_DESKTOP_UPDATE_CHANNEL=stable|beta|nightly)
REPO="${GITHUB_REPOSITORY:-Po1nt9/omp-desktop}"
CHANNEL="${OMP_DESKTOP_UPDATE_CHANNEL:-stable}"
case "$CHANNEL" in
  stable)  ROLLING="omp-desktop-latest";  MANIFEST="latest.json" ;;
  beta)    ROLLING="omp-desktop-beta";    MANIFEST="beta.json" ;;
  nightly) ROLLING="omp-desktop-nightly"; MANIFEST="nightly.json" ;;
  *)
    echo "unknown OMP_DESKTOP_UPDATE_CHANNEL='$CHANNEL' (stable|beta|nightly)" >&2
    exit 1
    ;;
esac
LATEST_URL="https://github.com/${REPO}/releases/download/${ROLLING}/${MANIFEST}"
```

In the same section, replace `/tmp/omp-desktop-latest.json` with `/tmp/omp-desktop-${CHANNEL}.json` (3 occurrences) and the final `note` line's wording `$LATEST_URL` stays as-is (already parameterized).

- [ ] **Step 7: Wire release.yml**

In `.github/workflows/release.yml`:

(a) After the "Detect updater secrets" step (ends line 144), insert:

```yaml
      # AC-10.9: channel identity comes from the tag/version suffix.
      - name: Derive update channel + endpoint
        id: updchan
        shell: bash
        run: |
          set -euo pipefail
          if [[ "${{ github.ref_type }}" == "tag" ]]; then
            VER="${{ github.ref_name }}"
          else
            VER="v$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')"
          fi
          echo "version=$VER" >> "$GITHUB_OUTPUT"
          echo "channel=$(node scripts/update-channel-lib.mjs channel "$VER")" >> "$GITHUB_OUTPUT"
          echo "endpoint=$(node scripts/update-channel-lib.mjs endpoint "$GITHUB_REPOSITORY" "$VER")" >> "$GITHUB_OUTPUT"
          echo "prerelease=$(node scripts/update-channel-lib.mjs prerelease "$VER")" >> "$GITHUB_OUTPUT"
```

(b) In "Write release config (updater)" (line 146-152), replace the env block with:

```yaml
        env:
          OMP_DESKTOP_UPDATER_PUBLIC_KEY: ${{ secrets.OMP_DESKTOP_UPDATER_PUBLIC_KEY }}
          OMP_DESKTOP_UPDATER_ENDPOINT: ${{ steps.updchan.outputs.endpoint }}
          OMP_DESKTOP_RELEASE_VERSION: ${{ steps.updchan.outputs.version }}
```

(c) In the tauri-action step, replace the `OMP_DESKTOP_UPDATER_ENDPOINT` env (line 163) with:

```yaml
          OMP_DESKTOP_UPDATER_ENDPOINT: ${{ steps.updchan.outputs.endpoint }}
```

and replace `prerelease: false` (line 174) with:

```yaml
          prerelease: ${{ steps.updchan.outputs.prerelease }}
```

(d) In the `assemble-updater` job, replace the run block (lines 213-228) with:

```yaml
        run: |
          set -euo pipefail
          if [[ -z "${OMP_DESKTOP_UPDATER_PUBLIC_KEY:-}" || -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
            echo "Updater secrets not configured — skip assemble-updater-manifest"
            exit 0
          fi
          if [[ "${{ github.ref_type }}" == "tag" ]]; then
            TAG="${{ github.ref_name }}"
          else
            TAG="v$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')"
          fi
          CHANNEL="$(node scripts/update-channel-lib.mjs channel "$TAG")"
          echo "Update channel: $CHANNEL (tag $TAG)"
          # DUAL_PUBLISH_LEGACY_LATEST: transitional nightly → omp-desktop-latest
          # dual-publish (AC-10.9 D5). Delete after the first stable release.
          # Fail soft if this tag has no updater archives (partial rebuild).
          if ! TAG="$TAG" REPO="$GITHUB_REPOSITORY" CHANNEL="$CHANNEL" DUAL_PUBLISH_LEGACY_LATEST=1 bash scripts/assemble-updater-manifest.sh; then
            echo "::warning::assemble-updater-manifest failed — release installers are still published"
            exit 0
          fi
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd /Users/po1nt9/Github/grok-app-main && node --test scripts/update-channel-lib.test.mjs 2>&1 | tail -5`
Expected: `pass 8` / `fail 0`.

Also smoke the config generator (endpoint derivation without secrets):
Run: `cd /Users/po1nt9/Github/grok-app-main && OMP_DESKTOP_UPDATER_PUBLIC_KEY=dummy GITHUB_REPOSITORY=owner/omp-desktop OMP_DESKTOP_RELEASE_VERSION=v1.1.0-beta.1 node scripts/build-release-config.mjs`
Expected: prints `Update channel  -> beta` and `…/omp-desktop-beta/beta.json`. Then `git checkout -- src-tauri/tauri.release.conf.json` (generated file — do not commit the local diff).

- [ ] **Step 9: YAML sanity + commit**

Run: `cd /Users/po1nt9/Github/grok-app-main && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && bash -n scripts/assemble-updater-manifest.sh && bash -n scripts/verify-updater-setup.sh && echo OK`
Expected: `OK`

```bash
cd /Users/po1nt9/Github/grok-app-main
git add scripts/update-channel-lib.mjs scripts/update-channel-lib.test.mjs scripts/build-release-config.mjs scripts/assemble-updater-manifest.sh scripts/verify-updater-setup.sh .github/workflows/release.yml
git commit -m "ci(release): per-channel feeds (stable/beta/nightly) from tag suffix (AC-10.9)"
```

---

### Task 4: Frontend channel display (type, normalizer, About badge, i18n ×3)

**Files:**
- Create: `src/lib/updateChannelInfo.ts`
- Create: `src/lib/updateChannelInfo.test.ts`
- Modify: `src/hooks/useUpdater.ts:104-110` (type) and `:414-433` (use normalizer)
- Modify: `src/lib/api.ts:337-343` (UpdaterStatus type)
- Modify: `src/components/SettingsPage.tsx:3270-3277` (badge line)
- Modify: `src/i18n/messages.ts` (en ~:999, zh-CN ~:3096) + `src/i18n/zh-tw.ts` (~:958)

**Interfaces:**
- Consumes: `updater_status` JSON `releaseChannel` (Task 1).
- Produces: `normalizeUpdaterStatus(raw: {channel?: string; releaseChannel?: string; pluginEnabled?: boolean; platformSupported?: boolean; endpoint?: string}) -> UpdaterChannelInfo`; `UpdaterChannelInfo.releaseChannel: "stable" | "beta" | "nightly" | "unknown"`.

- [ ] **Step 1: Write the failing test**

Create `src/lib/updateChannelInfo.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { normalizeUpdaterStatus } from "./updateChannelInfo";

describe("normalizeUpdaterStatus", () => {
  it("passes through known delivery modes and release channels", () => {
    expect(
      normalizeUpdaterStatus({
        channel: "silent",
        releaseChannel: "nightly",
        pluginEnabled: true,
        platformSupported: true,
        endpoint: "https://example/nightly.json",
      }),
    ).toEqual({
      channel: "silent",
      releaseChannel: "nightly",
      pluginEnabled: true,
      platformSupported: true,
      endpoint: "https://example/nightly.json",
    });
  });

  it("normalizes unknown / missing values defensively", () => {
    expect(normalizeUpdaterStatus({})).toEqual({
      channel: "unknown",
      releaseChannel: "unknown",
      pluginEnabled: false,
      platformSupported: false,
      endpoint: "",
    });
    expect(
      normalizeUpdaterStatus({ channel: "weird", releaseChannel: "canary" })
        .releaseChannel,
    ).toBe("unknown");
    expect(
      normalizeUpdaterStatus({ channel: "github_manual", releaseChannel: "beta" })
        .channel,
    ).toBe("github_manual");
  });

  it("accepts stable and beta release channels", () => {
    expect(normalizeUpdaterStatus({ releaseChannel: "stable" }).releaseChannel).toBe("stable");
    expect(normalizeUpdaterStatus({ releaseChannel: "beta" }).releaseChannel).toBe("beta");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm vitest run src/lib/updateChannelInfo.test.ts 2>&1 | tail -4`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the normalizer**

Create `src/lib/updateChannelInfo.ts`:

```ts
/**
 * Normalized view of the Rust `updater_status` DTO (Settings → About).
 * `channel` is the delivery mode (signed plugin vs GitHub manual);
 * `releaseChannel` is the build-baked feed identity (AC-10.9).
 */
export type ReleaseChannel = "stable" | "beta" | "nightly" | "unknown";

export type UpdaterChannelInfo = {
  /** `silent` when signed release plugin path is live; else `github_manual`. */
  channel: "silent" | "github_manual" | "unknown";
  releaseChannel: ReleaseChannel;
  pluginEnabled: boolean;
  platformSupported: boolean;
  endpoint: string;
};

export function normalizeUpdaterStatus(raw: {
  channel?: string;
  releaseChannel?: string;
  pluginEnabled?: boolean;
  platformSupported?: boolean;
  endpoint?: string;
}): UpdaterChannelInfo {
  const channel =
    raw.channel === "silent"
      ? "silent"
      : raw.channel === "github_manual"
        ? "github_manual"
        : "unknown";
  const releaseChannel: ReleaseChannel =
    raw.releaseChannel === "stable" ||
    raw.releaseChannel === "beta" ||
    raw.releaseChannel === "nightly"
      ? raw.releaseChannel
      : "unknown";
  return {
    channel,
    releaseChannel,
    pluginEnabled: !!raw.pluginEnabled,
    platformSupported: !!raw.platformSupported,
    endpoint: raw.endpoint || "",
  };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm vitest run src/lib/updateChannelInfo.test.ts 2>&1 | tail -3`
Expected: `3 passed`.

- [ ] **Step 5: Wire the normalizer into useUpdater + api type**

In `src/hooks/useUpdater.ts`, replace the local type (lines 104-110) with:

```ts
export type { UpdaterChannelInfo, ReleaseChannel } from "../lib/updateChannelInfo";
import {
  normalizeUpdaterStatus,
  type UpdaterChannelInfo,
} from "../lib/updateChannelInfo";
```

(Match the file's existing import style — if imports live at top, put the import with them and keep only the `export type` re-export at the old location.)

Replace the initial state (lines 114-119) with:

```ts
  const [channelInfo, setChannelInfo] = useState<UpdaterChannelInfo>(
    normalizeUpdaterStatus({}),
  );
```

Replace the invoke block (lines 416-433) with:

```ts
        const s = await invoke<{
          platformSupported: boolean;
          pluginEnabled: boolean;
          channel: string;
          releaseChannel: string;
          endpoint: string;
        }>("updater_status");
        if (!aliveRef.current || generationRef.current !== gen) return;
        setChannelInfo(normalizeUpdaterStatus(s));
```

In `src/lib/api.ts`, update `UpdaterStatus` (lines 337-343):

```ts
export type UpdaterStatus = {
  platformSupported: boolean;
  pluginEnabled: boolean;
  /** Delivery mode: `silent` | `github_manual` */
  channel: string;
  /** Build-baked release channel: `stable` | `beta` | `nightly` (AC-10.9). */
  releaseChannel: string;
  endpoint: string;
};
```

- [ ] **Step 6: About badge + i18n keys**

In `src/components/SettingsPage.tsx`, replace the hint div (lines 3270-3277) with:

```tsx
        <div className="settings-row__hint" data-updater-channel={channelInfo.channel}>
          {channelInfo.channel === "silent"
            ? t("settings.autoUpdateChannelSilent")
            : t("settings.autoUpdateChannelManual")}
          {channelInfo.endpoint
            ? ` · ${channelInfo.endpoint.replace(/^https:\/\//, "")}`
            : ""}
        </div>
        {channelInfo.releaseChannel !== "unknown" && (
          <div
            className="settings-row__hint"
            data-release-channel={channelInfo.releaseChannel}
          >
            {t(`settings.releaseChannel.${channelInfo.releaseChannel}`)}
            {" · "}
            {t("settings.releaseChannelDesc")}
          </div>
        )}
```

i18n keys — `src/i18n/messages.ts` en section, after `"settings.autoUpdateChannelManual"` entry:

```ts
  "settings.releaseChannel.stable": "Release channel: stable",
  "settings.releaseChannel.beta": "Release channel: beta",
  "settings.releaseChannel.nightly": "Release channel: nightly",
  "settings.releaseChannelDesc":
    "Channels are tied to the installed build — install a beta/nightly build to switch.",
```

zh-CN section (same anchor, after the translated `autoUpdateChannelManual`):

```ts
  "settings.releaseChannel.stable": "更新渠道：stable（稳定版）",
  "settings.releaseChannel.beta": "更新渠道：beta（测试版）",
  "settings.releaseChannel.nightly": "更新渠道：nightly（每日构建）",
  "settings.releaseChannelDesc": "渠道随安装包固定——安装 beta/nightly 安装包即可切换。",
```

`src/i18n/zh-tw.ts` (same anchor):

```ts
  "settings.releaseChannel.stable": "更新渠道：stable（穩定版）",
  "settings.releaseChannel.beta": "更新渠道：beta（測試版）",
  "settings.releaseChannel.nightly": "更新渠道：nightly（每日建構）",
  "settings.releaseChannelDesc": "渠道隨安裝包固定——安裝 beta/nightly 安裝包即可切換。",
```

Check whether `t()` supports dynamic template keys in this codebase — it does (i18n key lookup by string); the ICU checker validates key existence per locale. If `t()` is typed to a key union, cast: `t(\`settings.releaseChannel.${channelInfo.releaseChannel}\` as const)` may fail — in that case use a lookup map:

```tsx
          {t(
            channelInfo.releaseChannel === "beta"
              ? "settings.releaseChannel.beta"
              : channelInfo.releaseChannel === "nightly"
                ? "settings.releaseChannel.nightly"
                : "settings.releaseChannel.stable",
          )}
```

Prefer the lookup-map form if typecheck complains about the template key.

- [ ] **Step 7: Frontend gates**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm test 2>&1 | tail -3 && pnpm typecheck && pnpm check:i18n && pnpm check:brand`
Expected: vitest `841 passed` (840 + 1 file/3 tests — report shows Tests line), typecheck clean, i18n 3 locales × 1889 keys, brand clean.

- [ ] **Step 8: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src/lib/updateChannelInfo.ts src/lib/updateChannelInfo.test.ts src/hooks/useUpdater.ts src/lib/api.ts src/components/SettingsPage.tsx src/i18n/messages.ts src/i18n/zh-tw.ts
git commit -m "feat(settings): show build-baked release channel in About (AC-10.9)"
```

---

### Task 5: Version sync + docs + matrix/audit/memory + full gates

**Files:**
- Modify: `src-tauri/Cargo.toml:2` (`0.3.0-nightly` → `0.3.1-nightly`), `src-tauri/Cargo.lock` (via cargo), i18n footers `src/i18n/messages.ts:13,:2144`, `src/i18n/zh-tw.ts:7`
- Modify: `CHANGELOG.md` (add entry to `[0.3.1-nightly]`)
- Modify: `docs/desktop-auto-update.md` (multi-channel architecture)
- Modify: `docs/release/1.0-acceptance-matrix.md:227` (AC-10.9 flip), counts table, FAIL list
- Modify: `docs/release/test-coverage-audit.md:15,:61,:83` (counts, TC-M.11, gap row)
- Modify: memory `omp-desktop-roadmap-status.md` + `MEMORY.md`

**Interfaces:**
- Consumes: everything above (final evidence for the matrix flip).

- [ ] **Step 1: Sync versions**

- `src-tauri/Cargo.toml`: `version = "0.3.0-nightly"` → `"0.3.1-nightly"`.
- `src/i18n/messages.ts` both footers: `OMP v0.3.0-nightly` → `OMP v0.3.1-nightly`.
- `src/i18n/zh-tw.ts` footer: same.
- Regenerate lockfile: `cd /Users/po1nt9/Github/grok-app-main && cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -2` (Cargo.lock updates).

- [ ] **Step 2: CHANGELOG entry**

In `CHANGELOG.md`, inside the `## [0.3.1-nightly]` section after the existing `### Changed / 变更` block, add:

```markdown
### Added / 新增

- **Update channels (stable/beta/nightly):** the update feed is now isolated
  per channel. Channel identity is baked into each build from its version
  suffix; CI publishes per-channel rolling manifests
  (`omp-desktop-latest`/`latest.json`, `omp-desktop-beta`/`beta.json`,
  `omp-desktop-nightly/nightly.json`) and marks beta/nightly releases as
  GitHub prereleases. Settings → About shows the build's channel.
  更新渠道（stable/beta/nightly）隔离：渠道身份随构建固定，CI 按渠道发布
  滚动 manifest，关于页显示当前渠道。
```

- [ ] **Step 3: Rewrite docs/desktop-auto-update.md channel sections**

Read the current doc first, then rewrite the architecture/endpoint sections to describe: three channels, build-time baking (D1), per-channel rolling releases + manifest names (D2), single signing keypair (D3 deviation), transitional dual-publish (D5), channel-aware manual fallback (D6), prerelease flagging (D7), how to cut a beta/nightly release (`pnpm release:tag 1.1.0-beta.1` / `…-nightly.YYYYMMDD`). Keep the doc's existing language/structure conventions.

- [ ] **Step 4: Flip AC-10.9 in the matrix**

In `docs/release/1.0-acceptance-matrix.md:227`, replace the AC-10.9 row with:

```markdown
| AC-10.9 | Three update channels (stable/beta/nightly) correctly isolated | Config audit + manual channel switch test | PASS | Channel isolation implemented 2026-07-31 ([design](../superpowers/specs/2026-07-31-update-channels-design.md), build-time baking per D1): `update_channel.rs` derives channel from the version string (4 tests); CI derives channel from tag suffix and bakes per-channel endpoints (`update-channel-lib.mjs`, 8 node tests); per-channel rolling releases + manifests (`omp-desktop-latest`/`latest.json`, `omp-desktop-beta`/`beta.json`, `omp-desktop-nightly`/`nightly.json`); beta/nightly releases now marked GitHub prereleases; manual fallback path is channel-aware with prerelease semver ordering (3 tests); About shows the baked channel. Channel switch = install the other channel's build (Chrome/VS Code model). Live three-feed publish/pull/signature verification rides with AC-10.8/AC-2.12 (BLOCKED, cross-platform acceptance run). Transitional nightly→`omp-desktop-latest` dual-publish (D5) protects installed v0.3.x-nightly builds until the first stable release. |
```

Update the counts table: `| PASS | 40 |` / `| FAIL | 2 |` and adjust the parenthetical date note if needed. Verify with:

Run: `cd /Users/po1nt9/Github/grok-app-main && for s in PASS PARTIAL BLOCKED FAIL; do printf "%s " "$s"; grep -o "| $s |" docs/release/1.0-acceptance-matrix.md | wc -l; done`
Expected: PASS 40 / PARTIAL 16 / BLOCKED 100 / FAIL 2 — set counts-table values to exactly these grep totals.

In the FAIL list section (:353+), remove the AC-10.9 entry (keep AC-12.3 + the other).

- [ ] **Step 5: Sync test-coverage-audit.md**

- Line 15 Rust count: `506 tests (505 pass + 1 ignored)`… verify actual total from the gate run and use that; append `; +8 AC-10.9 update-channel tests (5 UpdateChannel/status, 3 channel-aware manual path) (2026-07-31 evening)`.
- Gap row "Single update channel" (:83): strike and mark **Resolved 2026-07-31** with one-line evidence (mechanism + tests + CI wiring; live feed verification → AC-10.8/AC-2.12).
- TC-M.11 (:61): note channel isolation now automated at mechanism level (config audit + derivation tests); real-platform smoke remains manual.

- [ ] **Step 6: Memory update**

Update `~/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`: add AC-10.9 entry (design/plan commits, key decisions D1/D5, stale-binary pitfall N/A here, counts 40/16/100/2); How-to-apply priorities now ① AC-12.3 凭据管理文档 → ② 跨平台真机验收 + 外部安全审计. Update `MEMORY.md` index line accordingly.

- [ ] **Step 7: Full gates**

Run: `cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -3 && pnpm test 2>&1 | tail -3 && pnpm typecheck && pnpm check:i18n && pnpm check:brand && pnpm check:provenance && pnpm check:legal && node --test scripts/update-channel-lib.test.mjs 2>&1 | tail -3`
Expected: cargo 506 pass + 1 ignored (verify actual); vitest 843 (840+3); all checks green; node test pass 8 fail 0. (If the sandboxed `store::tests::ensure_general_project…` flake appears, re-run — documented non-product-bug in audit :43.)

- [ ] **Step 8: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src/i18n/messages.ts src/i18n/zh-tw.ts CHANGELOG.md docs/desktop-auto-update.md docs/release/1.0-acceptance-matrix.md docs/release/test-coverage-audit.md
git commit -m "docs(release): AC-10.9 PASS — update channel isolation (stable/beta/nightly) + version sync"
```

---

## Self-Review

**Spec coverage:** D1 (build-time baking) → T1+T3 ✅ · D2 (3 rolling releases/manifests) → T3 ✅ · D3 (single keypair deviation) → no code, documented in spec + doc rewrite T5 ✅ · D4 (app-data isolation deferred) → no code ✅ · D5 (dual-publish) → T3 step 4 ✅ · D6 (manual path) → T2 ✅ · D7 (CI derivation + prerelease) → T3 step 7 ✅ · D8 (frontend display) → T4 ✅ · version drift fix → T5 step 1 ✅ · AC-10.9 flip criteria (§7) → T5 steps 4-5 ✅.

**Placeholder scan:** none — all steps carry exact code/commands. T5 step 3 doc rewrite intentionally says "read first, rewrite sections" (prose task, no code block possible); its required content outline is enumerated.

**Type consistency:** Rust `UpdateChannel::{from_version, as_str, owns_tag}` identical in T1/T2 ✅ · `release_channel` Rust field → camelCase `releaseChannel` consumed by T4 normalizer ✅ · JS lib exports (`channelFromVersion/channelFromTag/endpointFor/isPrerelease/rollingTagFor/manifestNameFor`) identical in lib, test, build-release-config, release.yml CLI calls ✅ · bash `CHANNEL`/`MANIFEST_NAME`/`ROLLING_TAG`/`PRINT_DERIVED`/`DUAL_PUBLISH_LEGACY_LATEST` consistent between script and tests ✅ · `normalizeUpdaterStatus` name identical in T4 steps 1/3/5 ✅.

**Deviation notes:** T3 step 4 shows the `case` block twice (second is the corrected form) — apply the second. T4 step 6 offers a fallback if `t()` rejects template keys — prefer lookup-map form on typecheck error.
