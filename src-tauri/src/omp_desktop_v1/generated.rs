//! Generated `serde` types mirroring the OMP Desktop v1 schema bundle.
//!
//! These structs are hand-verified to match the JSON Schemas in
//! `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/`.
//! A future `build.rs` step will regenerate them from `schema-bundle.json`,
//! but for Plan 2 the committed file builds without a codegen dependency.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub modified: String,
    pub parent_session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub cwd: String,
    pub session_count: u32,
    pub last_activity_at: String,
    pub last_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub auth_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub context_window: Option<u32>,
}

/// Credential metadata only — never includes the secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMetadata {
    pub id: String,
    pub provider_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSourceInfo {
    pub id: String,
    pub name: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub timestamp: String,
}

/// Capability descriptor advertised by the OMP Runtime during ACP `initialize`.
///
/// When `None`, the `OmpExtension` client is fail-closed and every request
/// returns `runtime_unavailable`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopV1Capability {
    pub schema_version: u32,
    pub schema_digest: String,
    pub methods: Vec<String>,
    pub notifications: Vec<String>,
    pub optional_features: Vec<String>,
}
