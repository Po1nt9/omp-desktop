//! Telegram Bot API long-polling (getUpdates).

use super::super::outbound::{http_client, secret_or_opt};
use super::super::types::{ChannelInstance, IncomingMessage};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

pub async fn run(
    inst: ChannelInstance,
    tx: mpsc::Sender<IncomingMessage>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let token = secret_or_opt(&inst.secrets, &inst.options, "bot_token")
        .or_else(|| secret_or_opt(&inst.secrets, &inst.options, "token"))
        .ok_or_else(|| "missing bot_token".to_string())?;
    let client = http_client()?;
    let mut offset: i64 = 0;

    tracing::info!(instance = %inst.id, "telegram long-poll starting");

    // drop pending webhook if any
    let _ = client
        .post(format!("https://api.telegram.org/bot{token}/deleteWebhook"))
        .json(&json!({ "drop_pending_updates": false }))
        .send()
        .await;

    loop {
        if *cancel.borrow() {
            return Ok(());
        }
        let url = format!(
            "https://api.telegram.org/bot{token}/getUpdates?timeout=25&offset={offset}"
        );
        let fut = client.get(&url).send();
        let res = tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() { return Ok(()); }
                continue;
            }
            r = fut => r,
        };
        let res = match res {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(instance = %inst.id, "telegram poll error: {e}");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        let body: Value = match res.json().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if body.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let Some(arr) = body.get("result").and_then(|r| r.as_array()) else {
            continue;
        };
        for upd in arr {
            if let Some(id) = upd.get("update_id").and_then(|x| x.as_i64()) {
                offset = id + 1;
            }
            let msg = upd.get("message").or_else(|| upd.get("edited_message"));
            let Some(msg) = msg else { continue };
            let text = msg
                .get("text")
                .or_else(|| msg.get("caption"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            // Extract the highest-resolution photo file_id (if any).
            let mut attachments: Vec<super::super::types::Attachment> = Vec::new();
            if let Some(photos) = msg.get("photo").and_then(|p| p.as_array()) {
                if let Some(largest) = photos.last() {
                    if let Some(file_id) = largest.get("file_id").and_then(|f| f.as_str()) {
                        attachments.push(super::super::types::Attachment {
                            kind: super::super::types::AttachmentKind::Image,
                            source: super::super::types::AttachmentSource::Telegram {
                                file_id: file_id.to_string(),
                            },
                        });
                    }
                }
            }
            // Drop only if there's no text AND no attachments.
            if text.is_empty() && attachments.is_empty() {
                continue;
            }
            let chat_id = msg
                .pointer("/chat/id")
                .map(|x| match x {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let chat_type = msg
                .pointer("/chat/type")
                .and_then(|x| x.as_str())
                .map(|t| {
                    if t == "private" {
                        "p2p"
                    } else {
                        "group"
                    }
                })
                .unwrap_or("p2p")
                .to_string();
            let sender_id = msg
                .pointer("/from/id")
                .map(|x| match x {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let message_id = msg
                .get("message_id")
                .map(|x| match x {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let entities = msg.get("entities").and_then(|e| e.as_array());
            let mentioned_bot = chat_type == "p2p"
                || entities
                    .map(|e| {
                        e.iter().any(|ent| {
                            ent.get("type").and_then(|t| t.as_str()) == Some("mention")
                                || ent.get("type").and_then(|t| t.as_str())
                                    == Some("bot_command")
                        })
                    })
                    .unwrap_or(false);

            let _ = tx
                .send(IncomingMessage {
                    channel: inst.channel.clone(),
                    instance_id: inst.id.clone(),
                    message_id,
                    chat_id,
                    chat_type,
                    sender_id,
                    content: text,
                    mentioned_bot,
                    attachments,
                    timestamp: None,
                    nonce: None,
                })
                .await;
        }
    }
}

pub async fn send_text(
    secrets: &std::collections::HashMap<String, String>,
    chat_id: &str,
    text: &str,
) -> Result<(), String> {
    let token = secrets
        .get("bot_token")
        .or_else(|| secrets.get("token"))
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing bot_token".to_string())?;
    let client = http_client()?;
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let res = client
        .post(url)
        .json(&json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown",
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        // retry without markdown
        let res2 = client
            .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
            .json(&json!({ "chat_id": chat_id, "text": text }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res2.status().is_success() {
            return Err(format!("telegram send: {}", res2.status()));
        }
    }
    Ok(())
}
