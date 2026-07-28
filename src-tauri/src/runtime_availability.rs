//! Stable fail-closed runtime availability contract (Plan 1).
//!
//! Until a later plan connects an OMP runtime, every Agent execution path
//! returns the same stable `runtime_unavailable` error. This module exposes
//! the read-only availability state to the frontend via a Tauri command.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAvailability {
    pub available: bool,
    pub reason: &'static str,
}

pub const RUNTIME_AVAILABILITY: RuntimeAvailability = RuntimeAvailability {
    available: false,
    reason: "runtime_unavailable",
};

/// Read-only Tauri command returning the stable availability state.
#[tauri::command]
pub fn runtime_availability() -> RuntimeAvailability {
    RUNTIME_AVAILABILITY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_unavailable_with_stable_reason() {
        assert_eq!(
            RUNTIME_AVAILABILITY,
            RuntimeAvailability {
                available: false,
                reason: "runtime_unavailable",
            }
        );
    }

    #[test]
    fn serializes_to_camel_case() {
        let json = serde_json::to_string(&RUNTIME_AVAILABILITY).unwrap();
        assert_eq!(
            json,
            r#"{"available":false,"reason":"runtime_unavailable"}"#
        );
    }
}
