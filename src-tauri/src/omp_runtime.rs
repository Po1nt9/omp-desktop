//! Three-tier OMP Runtime binary resolution.
//! Spec: docs/superpowers/specs/2026-08-01-bundle-omp-runtime-design.md

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

#[cfg(test)]
mod tests {
    use super::*;

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
