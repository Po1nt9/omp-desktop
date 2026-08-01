//! In-app independent omp Runtime upgrade ("检查 omp 更新").
//! Spec: docs/superpowers/specs/2026-08-01-bundle-omp-runtime-design.md

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
    // Same path as omp_runtime::upgraded_omp_path(), but derived from the
    // single app_data_root() read above so a concurrent env override (test
    // suites mutate OMP_DESKTOP_HOME) cannot split dir and target apart.
    let target = dir.join(crate::omp_runtime::omp_binary_name());
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
        // Payload must exceed MIN_BINARY_BYTES (1 MiB error-page guard).
        let bytes = vec![b'x'; 1024 * 1024 + 1];
        let applied = apply_omp_bytes(&bytes).unwrap();
        // Derive from the temp home, not a fresh env read: other test modules
        // flip OMP_DESKTOP_HOME concurrently, so upgraded_omp_path() could
        // point at a different root mid-test.
        let target = home
            .path()
            .join("runtime")
            .join(crate::omp_runtime::omp_binary_name());
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
        let bytes2 = vec![b'y'; 1024 * 1024 + 1];
        apply_omp_bytes(&bytes2).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), bytes2);
    }

    #[test]
    fn apply_rejects_tiny_download() {
        let (_guard, home) = with_test_home();
        assert!(apply_omp_bytes(b"404: Not Found").is_err());
        assert!(!home
            .path()
            .join("runtime")
            .join(crate::omp_runtime::omp_binary_name())
            .exists());
    }
}
