//! Shared types for the in-process Remote IM runtime.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ChannelInstance {
    pub id: String,
    pub channel: String,
    pub name: String,
    pub enabled: bool,
    pub secrets: HashMap<String, String>,
    pub options: Value,
    pub acl: Value,
    pub project_scope: Value,
}

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub channel: String,
    pub instance_id: String,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String, // p2p | group
    pub sender_id: String,
    pub content: String,
    pub mentioned_bot: bool,
    pub attachments: Vec<Attachment>,
    /// AC-8.4 anti-replay: webhook-supplied unix timestamp (seconds).
    /// `None` for WS / long-poll channels (platform-authenticated transport).
    pub timestamp: Option<i64>,
    /// AC-8.4 anti-replay: webhook-supplied nonce (or derived replay key,
    /// e.g. LINE uses "sig:replyToken"). `None` for WS / long-poll channels.
    pub nonce: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub source: AttachmentSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    File,
}

/// Platform-specific download credentials. The channel set is finite and known,
/// so an enum (not a trait object) is the idiomatic choice with zero dyn-dispatch.
#[derive(Debug, Clone)]
pub enum AttachmentSource {
    /// Feishu: download_message_resource(message_id, file_key, resource_type)
    Feishu {
        message_id: String,
        file_key: String,
        resource_type: String,
    },
    /// Telegram: file_id → getFile API → file_path → download
    Telegram { file_id: String },
    /// Discord: CDN url, fetched directly (public)
    Discord { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedChannel {
    pub channel: String,
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub work_dir: String,
    pub agent_session_id: Option<String>,
    pub turn_count: u32,
}

impl SessionRecord {
    pub fn new(work_dir: &str) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            work_dir: work_dir.to_string(),
            agent_session_id: None,
            turn_count: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrustedProject {
    pub id: String,
    pub name: String,
    pub path: String,
}
