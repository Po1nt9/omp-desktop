//! Runtime availability state (Plan 1: fail-closed; Plan 3: dynamic).
//!
//! Plan 1 exposed a hardcoded `runtime_unavailable` state so every Agent
//! execution path could surface a stable contract to the frontend. Plan 3
//! makes the state dynamic: the Supervisor calls [`set_runtime_available`]
//! when the OMP sidecar starts/stops, and the [`runtime_availability`] Tauri
//! command reflects the live state.
//!
//! Default state (preserving Plan 1): `available: false, reason:
//! "runtime_unavailable"`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAvailability {
    pub available: bool,
    pub reason: String,
}

impl Default for RuntimeAvailability {
    fn default() -> Self {
        Self {
            available: false,
            reason: "runtime_unavailable".to_string(),
        }
    }
}

/// Dynamic runtime state. The Supervisor updates this when the sidecar
/// starts or stops. Defaults to unavailable (Plan 1 fail-closed behavior).
static RUNTIME_AVAILABLE: AtomicBool = AtomicBool::new(false);
static RUNTIME_REASON: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new("runtime_unavailable".to_string()));

/// Update the runtime availability state.
///
/// Called by the Supervisor when the OMP sidecar starts (`available = true`)
/// or stops/crashes (`available = false`). The `reason` is a short stable
/// identifier (e.g. `"omp_runtime"`, `"runtime_unavailable"`) suitable for
/// frontend display logic.
pub fn set_runtime_available(available: bool, reason: &str) {
    RUNTIME_AVAILABLE.store(available, Ordering::SeqCst);
    if let Ok(mut guard) = RUNTIME_REASON.write() {
        *guard = reason.to_string();
    }
}

/// Read-only Tauri command returning the live availability state.
#[tauri::command]
pub fn runtime_availability() -> RuntimeAvailability {
    RuntimeAvailability {
        available: RUNTIME_AVAILABLE.load(Ordering::SeqCst),
        reason: RUNTIME_REASON
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "runtime_unavailable".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize tests that mutate the global runtime state. Rust runs
    /// tests in parallel by default; without this guard, one test flipping
    /// the state to `available: true` can race with another that asserts
    /// the default `false`.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_to_default() {
        set_runtime_available(false, "runtime_unavailable");
    }

    #[test]
    fn defaults_to_unavailable_with_stable_reason() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_to_default();
        let avail = runtime_availability();
        assert!(!avail.available);
        assert_eq!(avail.reason, "runtime_unavailable");
    }

    #[test]
    fn serializes_to_camel_case() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_to_default();
        let avail = runtime_availability();
        let json = serde_json::to_string(&avail).unwrap();
        assert_eq!(
            json,
            r#"{"available":false,"reason":"runtime_unavailable"}"#
        );
    }

    #[test]
    fn set_runtime_available_changes_state() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_to_default();
        set_runtime_available(true, "omp_runtime");
        let avail = runtime_availability();
        assert!(avail.available);
        assert_eq!(avail.reason, "omp_runtime");
        // Reset back to default so other tests are not affected.
        reset_to_default();
    }

    #[test]
    fn runtime_availability_default_matches_plan1() {
        // This test does not touch global state — no lock needed.
        let default = RuntimeAvailability::default();
        assert!(!default.available);
        assert_eq!(default.reason, "runtime_unavailable");
    }
}
