//! `DesktopV1Error` — the stable error envelope for `_omp/desktop/v1/*` requests.
//!
//! Mirrors `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/errors.ts`.
//! The code → metadata table is the authoritative Rust source for error codes
//! defined in the Plan 2 inventory.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopV1Error {
    pub code: String,
    pub message: String,
    pub message_key: String,
    pub args: serde_json::Value,
    pub recoverable: bool,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub details: Option<serde_json::Value>,
}

impl DesktopV1Error {
    /// Build an error from a stable code and opaque `args` payload.
    ///
    /// Unknown codes fall back to a generic `("unknown", false, false)` profile
    /// so the wire envelope stays forward-compatible with newer runtimes.
    pub fn new(code: &str, args: serde_json::Value) -> Self {
        let (message_key, recoverable, retryable) = match code {
            "runtime_unavailable" => ("runtime.unavailable", false, false),
            "invalid_params" => ("validation.invalidParams", true, false),
            "not_found" => ("state.notFound", true, false),
            "auth_failed" => ("auth.failed", true, false),
            "capability_missing" => ("capability.missing", false, false),
            "too_late" => ("timing.tooLate", false, false),
            "schema_digest_mismatch" => ("compat.schemaDigestMismatch", false, false),
            "unknown_method" => ("compat.unknownMethod", false, false),
            "journal_gap" => ("recovery.journalGap", true, true),
            _ => ("unknown", false, false),
        };
        Self {
            code: code.to_string(),
            message: message_key.to_string(),
            message_key: message_key.to_string(),
            args,
            recoverable,
            retryable,
            details: None,
        }
    }

    /// Convenience: `runtime_unavailable` with empty args — the Plan 2 fail-closed
    /// sentinel returned whenever no capability has been negotiated or the
    /// Plan 3 transport is not yet wired.
    pub fn runtime_unavailable() -> Self {
        Self::new("runtime_unavailable", serde_json::json!({}))
    }
}

impl std::fmt::Display for DesktopV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DesktopV1Error({}): {}", self.code, self.message_key)
    }
}

impl std::error::Error for DesktopV1Error {}
