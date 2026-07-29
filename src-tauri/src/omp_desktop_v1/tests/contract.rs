//! Contract tests for the `OmpExtension` client.
//!
//! These tests assert the Plan 2 fail-closed contract:
//! - No capability negotiated → every request returns `runtime_unavailable`.
//! - Capability present but method not in the method list → `unknown_method`.
//! - Stable ID patterns compile and match the inventory examples.

use crate::omp_desktop_v1::generated::DesktopV1Capability;
use crate::omp_desktop_v1::ids::id_patterns;
use crate::omp_desktop_v1::OmpExtension;

#[tokio::test]
async fn extension_client_returns_unavailable_when_capability_absent() {
    let client = OmpExtension::new();
    let result = client
        .request("sessions.listAll", serde_json::json!({}))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, "runtime_unavailable");
}

#[tokio::test]
async fn extension_client_validates_method_name() {
    let client = OmpExtension::new();
    // Negotiate a capability that does NOT advertise `nonexistent.method`.
    // The client must reject the call with `unknown_method` instead of
    // attempting a real request.
    let cap = DesktopV1Capability {
        schema_version: 1,
        schema_digest: "test-digest".to_string(),
        methods: vec!["_omp/desktop/v1/sessions.listAll".to_string()],
        notifications: vec![],
        optional_features: vec![],
    };
    client.negotiate_capability(Some(cap)).await;
    let result = client
        .request("nonexistent.method", serde_json::json!({}))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "unknown_method");
}

#[tokio::test]
async fn extension_client_returns_unavailable_for_advertised_method_in_plan_2() {
    // Even when the method IS in the capability list, Plan 2 has no real
    // transport wired, so the request must still fail with `runtime_unavailable`.
    // Plan 3 will inject the AcpClient and make this succeed.
    let client = OmpExtension::new();
    let cap = DesktopV1Capability {
        schema_version: 1,
        schema_digest: "test-digest".to_string(),
        methods: vec!["_omp/desktop/v1/sessions.listAll".to_string()],
        notifications: vec![],
        optional_features: vec![],
    };
    client.negotiate_capability(Some(cap)).await;
    let result = client
        .request("sessions.listAll", serde_json::json!({}))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "runtime_unavailable");
}

#[tokio::test]
async fn negotiate_capability_round_trips() {
    let client = OmpExtension::new();
    assert!(!client.has_capability().await);
    assert!(client.capability().await.is_none());

    let cap = DesktopV1Capability {
        schema_version: 1,
        schema_digest: "abc".to_string(),
        methods: vec!["_omp/desktop/v1/usage.reports".to_string()],
        notifications: vec!["_omp/desktop/v1/turn.status".to_string()],
        optional_features: vec!["queue".to_string()],
    };
    client.negotiate_capability(Some(cap.clone())).await;
    assert!(client.has_capability().await);
    let observed = client.capability().await;
    assert!(observed.is_some());
    assert_eq!(observed.unwrap().schema_digest, "abc");

    // Clearing the capability restores the fail-closed state.
    client.negotiate_capability(None).await;
    assert!(!client.has_capability().await);
}

#[test]
fn stable_id_patterns_match_valid_examples() {
    let p = id_patterns();
    // Base32 alphabet is `a-z` + `2-7` (RFC 4648). Exactly 26 chars after the prefix.
    // `abcdefghijklmnopqrstuvwxyz` is exactly 26 lowercase letters (all in base32).
    assert!(p.session.is_match("sess_abcdefghijklmnopqrstuvwxyz"));
    assert!(p.turn.is_match("turn_abcdefghijklmnopqrstuvwxyz"));
    assert!(p.event.is_match("evt_abcdefghijklmnopqrstuvwxyz"));
    assert!(p.permission.is_match("perm_abcdefghijklmnopqrstuvwxyz"));
    assert!(p.queue_receipt.is_match("rcpt_abcdefghijklmnopqrstuvwxyz"));
    assert!(p.credential.is_match("cred_abcdefghijklmnopqrstuvwxyz"));
    // Hex SHA-1 (40 chars).
    assert!(p.project.is_match("proj_a1b2c3d4e5f6789012345678901234567890abcd"));
    // `<providerId>/<modelId>`.
    assert!(p.model.is_match("xai/grok-4.5"));
    assert!(p.model.is_match("anthropic/claude-opus-4"));
    // Hex SHA-1 (40 chars).
    assert!(p.mcp_source.is_match("mcp_f1e2d3c4b5a697887766554433221100ffeeddcc"));
}

#[test]
fn stable_id_patterns_reject_invalid_examples() {
    let p = id_patterns();
    assert!(!p.session.is_match("invalid"));
    assert!(!p.session.is_match("sess_abc")); // too short
    assert!(!p.session.is_match("sess_ABCDEFGHIJKLMNOPQRSTUVWXYZ")); // uppercase not in base32
    assert!(!p.session.is_match("sess_abcdefghijklmnopqrstuvwxyza")); // 27 chars — too long
    assert!(!p.session.is_match("sess_abcdefghij0klmnopqrstuv1xyz")); // contains 0/1 — not in base32
    assert!(!p.turn.is_match("sess_abcdefghijklmnopqrstuvwxyz")); // wrong prefix
    assert!(!p.project.is_match("proj_short"));
    assert!(!p.project.is_match("proj_A1B2C3D4E5F6789012345678901234567890ABCD")); // uppercase
    assert!(!p.model.is_match("xai")); // missing slash
    assert!(!p.model.is_match("xai/")); // missing model id
    assert!(!p.model.is_match("XAI/grok-4.5")); // uppercase provider
    assert!(!p.mcp_source.is_match("mcp_short"));
}

#[test]
fn error_metadata_is_stable_for_known_codes() {
    use crate::omp_desktop_v1::errors::DesktopV1Error;

    let ru = DesktopV1Error::runtime_unavailable();
    assert_eq!(ru.code, "runtime_unavailable");
    assert_eq!(ru.message_key, "runtime.unavailable");
    assert!(!ru.recoverable);
    assert!(!ru.retryable);

    let um = DesktopV1Error::new("unknown_method", serde_json::json!({ "method": "foo" }));
    assert_eq!(um.code, "unknown_method");
    assert_eq!(um.message_key, "compat.unknownMethod");
    assert!(!um.recoverable);
    assert!(!um.retryable);

    let ip = DesktopV1Error::new("invalid_params", serde_json::json!({}));
    assert_eq!(ip.message_key, "validation.invalidParams");
    assert!(ip.recoverable);
    assert!(!ip.retryable);

    let jg = DesktopV1Error::new("journal_gap", serde_json::json!({}));
    assert_eq!(jg.message_key, "recovery.journalGap");
    assert!(jg.recoverable);
    assert!(jg.retryable);
}
