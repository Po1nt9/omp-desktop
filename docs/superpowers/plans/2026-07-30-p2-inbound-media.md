# P2: 入站媒体接收 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Agent 通过 ACP image content block 接收并理解用户经飞书/Telegram/Discord 发来的图片。

**Architecture:** `IncomingMessage` 加 `attachments` 字段 → 3 个适配器入站解析提取图片元数据 → 新 `media.rs` 统一下载 → `AcpClient` 加 `prompt_with_blocks` 支持 image content block → `run_agent_turn` 在 prompt 前下载图片转 base64。

**Tech Stack:** Rust, reqwest (已依赖), base64 0.22 (已依赖, `STANDARD` engine), serde_json.

## Global Constraints

- 包名 `omp-desktop`，测试 `cargo test -p omp-desktop`（在 `src-tauri/` 下运行）
- base64 用法：`use base64::{engine::general_purpose::STANDARD as B64, Engine as _};` 然后 `B64.encode(bytes)`（参照 `editors.rs:9`）
- http client：`crate::remote_im::outbound::http_client()`（已存在）
- 20 个 `IncomingMessage` 构造点都要补 `attachments: vec![]`
- MIME 推断：从文件扩展名映射（.png/.jpg/.jpeg/.gif/.webp）

**Spec:** `docs/superpowers/specs/2026-07-30-p2-inbound-media-design.md`

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `src-tauri/src/remote_im/types.rs` | 修改 | 加 Attachment 类型 + IncomingMessage.attachments 字段 |
| 20 个构造点（见下） | 修改 | 补 `attachments: vec![]` |
| `src-tauri/src/remote_im/media.rs` | 创建 | 统一下载入口 + MIME 推断 |
| `src-tauri/src/remote_im/mod.rs` | 修改 | 注册 `mod media;` |
| `src-tauri/src/acp_client.rs` | 修改 | PromptBlock + prompt_with_blocks + wire_blocks |
| `src-tauri/src/remote_im/engine.rs` | 修改 | run_agent_turn 下载+构造 blocks |
| `src-tauri/src/remote_im/channels/telegram.rs` | 修改 | 提取 photo file_id |
| `src-tauri/src/remote_im/channels/discord.rs` | 修改 | 提取 attachments url |
| `src-tauri/src/remote_im/channels/feishu.rs` | 修改 | image_key 进 attachments |

---

### Task 1: 类型扩展 — Attachment + IncomingMessage.attachments

**Files:**
- Modify: `src-tauri/src/remote_im/types.rs`
- Modify: 20 个构造 IncomingMessage 的位置

**Interfaces:**
- Produces: `Attachment`, `AttachmentKind`, `AttachmentSource` (in `types.rs`); `IncomingMessage.attachments: Vec<Attachment>`

- [ ] **Step 1: Add Attachment types to types.rs**

In `src-tauri/src/remote_im/types.rs`, after the `IncomingMessage` struct (line 29), add:

```rust
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
```

Then add the `attachments` field to `IncomingMessage`:

```rust
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
}
```

- [ ] **Step 2: Fix all 20 construction sites — add `attachments: vec![]`**

These files construct `IncomingMessage { ... }` and now need the new field. Add `attachments: vec![],` to each (before the closing `}`):

| File | Line |
|------|------|
| `src-tauri/src/remote_im/channels/qq.rs` | ~123 |
| `src-tauri/src/remote_im/engine.rs` | ~1180 |
| `src-tauri/src/remote_im/engine.rs` | ~1202 |
| `src-tauri/src/remote_im/engine.rs` | ~1250 |
| `src-tauri/src/remote_im/engine.rs` | ~1275 |
| `src-tauri/src/remote_im/channels/line.rs` | ~121 |
| `src-tauri/src/remote_im/channels/telegram.rs` | ~127 |
| `src-tauri/src/remote_im/channels/discord.rs` | ~170 |
| `src-tauri/src/remote_im/channels/dingtalk.rs` | ~167 |
| `src-tauri/src/remote_im/channels/dingtalk.rs` | ~260 |
| `src-tauri/src/remote_im/channels/wecom.rs` | ~139 |
| `src-tauri/src/remote_im/channels/wecom.rs` | ~250 |
| `src-tauri/src/remote_im/channels/feishu.rs` | ~190 |
| `src-tauri/src/remote_im/channels/feishu.rs` | ~389 |
| `src-tauri/src/remote_im/channels/qqbot.rs` | ~161 |
| `src-tauri/src/remote_im/channels/matrix.rs` | ~77 |
| `src-tauri/src/remote_im/channels/slack.rs` | ~166 |
| `src-tauri/src/remote_im/channels/wps_xiezuo.rs` | ~166 |
| `src-tauri/src/remote_im/channels/weixin.rs` | ~253 |
| `src-tauri/src/remote_im/channels/weibo.rs` | ~83 |

For each, find the `IncomingMessage { ... mentioned_bot: ..., }` literal and add `attachments: vec![],` as the last field. Example for discord.rs:

```rust
    Some(IncomingMessage {
        channel: inst.channel.clone(),
        instance_id: inst.id.clone(),
        message_id,
        chat_id,
        chat_type: chat_type.into(),
        sender_id,
        content,
        mentioned_bot,
        attachments: vec![],   // ← add this line
    })
```

- [ ] **Step 3: Run build to verify all sites compile**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "error\[|missing field" | head
```
Expected: no `missing field attachments` errors. If any remain, the grep above lists them — fix and re-run.

- [ ] **Step 4: Run existing tests to confirm no regression**

```bash
cargo test -p omp-desktop remote_im 2>&1 | grep "test result" | tail -2
```
Expected: all existing remote_im tests still pass (53 + 12 P1 = ~65).

- [ ] **Step 5: Commit**

```bash
git add -A src-tauri/src/remote_im/
git commit -m "feat(remote_im): add Attachment type and attachments field to IncomingMessage"
```

---

### Task 2: media.rs — 统一下载 + MIME 推断

**Files:**
- Create: `src-tauri/src/remote_im/media.rs`
- Modify: `src-tauri/src/remote_im/mod.rs` (register `mod media;`)

**Interfaces:**
- Consumes: `super::types::{Attachment, AttachmentKind, AttachmentSource}`, `super::outbound::http_client`, `super::channels::feishu::download_message_resource`
- Produces: `MediaBytes { data, mime_type }`, `fetch_attachment(channel, secrets, options, att) -> Result<MediaBytes>`, `mime_from_extension(path) -> &'static str`

- [ ] **Step 1: Write media.rs with tests**

Create `src-tauri/src/remote_im/media.rs`:

```rust
//! Inbound media download + MIME inference, shared across channels.
use super::outbound::http_client;
use super::types::{Attachment, AttachmentKind, AttachmentSource};
use serde_json::Value;
use std::collections::HashMap;

pub struct MediaBytes {
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Infer a MIME type from a filename/url extension. Defaults to image/png.
pub fn mime_from_extension(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    }
}

/// Download attachment bytes for the given platform source.
pub async fn fetch_attachment(
    channel: &str,
    secrets: &HashMap<String, String>,
    options: &Value,
    att: &Attachment,
) -> Result<MediaBytes, String> {
    match &att.source {
        AttachmentSource::Feishu {
            message_id,
            file_key,
            resource_type,
        } => {
            let data = super::channels::feishu::download_message_resource(
                channel,
                secrets,
                options,
                message_id,
                file_key,
                resource_type,
            )
            .await?;
            let mime = if resource_type == "image" { "image/png" } else { "application/octet-stream" };
            Ok(MediaBytes { data, mime_type: mime.to_string() })
        }
        AttachmentSource::Telegram { file_id } => {
            let token = secrets
                .get("bot_token")
                .or_else(|| secrets.get("token"))
                .ok_or("missing telegram bot_token")?;
            let client = http_client()?;
            // Step 1: getFile
            let get_url = format!(
                "https://api.telegram.org/bot{token}/getFile?file_id={file_id}"
            );
            let resp: Value = client.get(&get_url).send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
            let file_path = resp
                .pointer("/result/file_path")
                .and_then(|v| v.as_str())
                .ok_or("telegram getFile: no file_path")?;
            let mime = mime_from_extension(file_path).to_string();
            // Step 2: download file content
            let dl_url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
            let data = client
                .get(&dl_url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .bytes()
                .await
                .map_err(|e| e.to_string())?
                .to_vec();
            Ok(MediaBytes { data, mime_type: mime })
        }
        AttachmentSource::Discord { url } => {
            let client = http_client()?;
            let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
            let mime = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(';').next().unwrap_or(s).to_string())
                .unwrap_or_else(|| mime_from_extension(url).to_string());
            let data = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
            Ok(MediaBytes { data, mime_type: mime })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_from_extension() {
        assert_eq!(mime_from_extension("photo.png"), "image/png");
        assert_eq!(mime_from_extension("a.JPG"), "image/jpeg");
        assert_eq!(mime_from_extension("x.jpeg"), "image/jpeg");
        assert_eq!(mime_from_extension("anim.gif"), "image/gif");
        assert_eq!(mime_from_extension("art.webp"), "image/webp");
        assert_eq!(mime_from_extension("noext"), "image/png"); // default
    }

    #[test]
    fn test_media_bytes_construct() {
        let mb = MediaBytes { data: vec![1, 2, 3], mime_type: "image/png".into() };
        assert_eq!(mb.data, vec![1, 2, 3]);
        assert_eq!(mb.mime_type, "image/png");
    }
}
```

Register in `src-tauri/src/remote_im/mod.rs`: add `mod media;` (after `mod rate_limiter;`).

- [ ] **Step 2: Run media tests**

```bash
cargo test -p omp-desktop remote_im::media 2>&1 | tail -15
```
Expected: 2 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/media.rs src-tauri/src/remote_im/mod.rs
git commit -m "feat(remote_im): add media download module with MIME inference"
```

---

### Task 3: ACP 扩展 — PromptBlock + prompt_with_blocks

**Files:**
- Modify: `src-tauri/src/acp_client.rs` (add PromptBlock, wire_session_prompt_params_blocks, prompt_with_blocks)

**Interfaces:**
- Produces: `PromptBlock` enum, `wire_session_prompt_params_blocks(session_id, blocks) -> Value`, `AcpClient::prompt_with_blocks(&self, blocks: &[PromptBlock]) -> Result<(), AgentError>`

- [ ] **Step 1: Add PromptBlock + wire function**

In `src-tauri/src/acp_client.rs`, near `wire_session_prompt_params` (line ~1357), add:

```rust
/// A content block for the ACP `session/prompt` `prompt` array.
#[derive(Debug, Clone)]
pub enum PromptBlock {
    Text { text: String },
    Image { data: String, mime_type: String }, // data = base64-encoded bytes
}

/// Host -> agent `session/prompt` params with mixed content blocks.
pub fn wire_session_prompt_params_blocks(session_id: &str, blocks: &[PromptBlock]) -> Value {
    let prompt: Vec<Value> = blocks
        .iter()
        .map(|b| match b {
            PromptBlock::Text { text } => json!({ "type": "text", "text": text }),
            PromptBlock::Image { data, mime_type } => {
                json!({ "type": "image", "data": data, "mimeType": mime_type })
            }
        })
        .collect();
    json!({ "sessionId": session_id, "prompt": prompt })
}

#[cfg(test)]
mod prompt_block_tests {
    use super::*;

    #[test]
    fn test_wire_blocks_text_only() {
        let v = wire_session_prompt_params_blocks(
            "s1",
            &[PromptBlock::Text { text: "hi".into() }],
        );
        assert_eq!(v["prompt"][0]["type"], "text");
        assert_eq!(v["prompt"][0]["text"], "hi");
    }

    #[test]
    fn test_wire_blocks_with_image() {
        let v = wire_session_prompt_params_blocks(
            "s1",
            &[
                PromptBlock::Text { text: "describe this".into() },
                PromptBlock::Image { data: "BASE64".into(), mime_type: "image/png".into() },
            ],
        );
        assert_eq!(v["prompt"].as_array().unwrap().len(), 2);
        assert_eq!(v["prompt"][1]["type"], "image");
        assert_eq!(v["prompt"][1]["data"], "BASE64");
        assert_eq!(v["prompt"][1]["mimeType"], "image/png");
    }
}
```

- [ ] **Step 2: Add prompt_with_blocks method to AcpClient**

Right after the existing `prompt(&self, text: &str)` method (line ~1289), add:

```rust
    /// Like `prompt`, but supports mixed content blocks (text + images).
    pub async fn prompt_with_blocks(&self, blocks: &[PromptBlock]) -> Result<(), AgentError> {
        let sid = self
            .agent_session_id
            .lock()
            .clone()
            .ok_or_else(|| AgentError::new(AgentErrorCode::AgentCrashed, "no session"))?;
        self.stopped.store(false, Ordering::SeqCst);
        let this_params = wire_session_prompt_params_blocks(&sid, blocks);
        let result = self
            .request_prompt(this_params)
            .await
            .map_err(|e| classify_rpc_error(&e))?;
        let stop = result
            .get("stopReason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn")
            .to_string();
        let _ = self.event_tx.send(AcpEvent::Stream {
            kind: StreamKind::Assistant,
            text: String::new(),
            message_id: None,
            done: true,
        });
        let _ = self.event_tx.send(AcpEvent::PromptComplete {
            stop_reason: stop,
            authoritative: true,
        });
        Ok(())
    }
```

(Note: verify `AgentErrorCode`, `classify_rpc_error`, `request_prompt`, `AcpEvent`, `StreamKind`, `Ordering` are all in scope at the top of acp_client.rs — they are used by the existing `prompt` method, so they are.)

- [ ] **Step 3: Run ACP block tests**

```bash
cargo test -p omp-desktop prompt_block_tests 2>&1 | tail -12
```
Expected: 2 tests PASS.

- [ ] **Step 4: Run build**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/acp_client.rs
git commit -m "feat(acp): add PromptBlock and prompt_with_blocks for image input"
```

---

### Task 4: Engine 集成 — run_agent_turn 下载图片构造 blocks

**Files:**
- Modify: `src-tauri/src/remote_im/engine.rs` (run_agent_turn, ~line 960-975)

**Interfaces:**
- Consumes: `crate::acp_client::PromptBlock`, `super::media::{fetch_attachment, MediaBytes}`, `base64` engine

- [ ] **Step 1: Add base64 import + rewrite prompt call in run_agent_turn**

At the top of `src-tauri/src/remote_im/engine.rs`, add to imports:

```rust
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use crate::acp_client::PromptBlock;
```

In `run_agent_turn`, find the prompt call (~line 970):

```rust
        // Run the prompt (blocks until the turn completes).
        if let Err(e) = runtime.acp.prompt(prompt).await {
```

Replace with block-construction logic:

```rust
        // Construct prompt blocks: text first, then any image attachments.
        let mut blocks = vec![PromptBlock::Text { text: prompt.to_string() }];
        if !msg.attachments.is_empty() {
            if let Some(inst) = self.instances.lock().get(&msg.instance_id).cloned() {
                for att in msg.attachments.iter().filter(|a| a.kind == super::types::AttachmentKind::Image) {
                    match super::media::fetch_attachment(
                        &msg.channel, &inst.secrets, &inst.options, att,
                    ).await {
                        Ok(media) => {
                            let b64 = B64.encode(&media.data);
                            blocks.push(PromptBlock::Image {
                                data: b64,
                                mime_type: media.mime_type,
                            });
                        }
                        Err(e) => tracing::warn!(
                            target: "remote_im::media",
                            channel = %msg.channel,
                            "image download failed: {e}"
                        ),
                    }
                }
            }
        }
        // Run the prompt (blocks until the turn completes).
        if let Err(e) = runtime.acp.prompt_with_blocks(&blocks).await {
```

(The `if let Err(e) = ...` body below stays unchanged.)

- [ ] **Step 2: Run build + existing engine tests**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
cargo test -p omp-desktop remote_im::engine::tests 2>&1 | grep "test result"
```
Expected: no errors; existing 5 engine tests still pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/engine.rs
git commit -m "feat(remote_im): download image attachments and send as ACP image blocks"
```

---

### Task 5: 适配器入站 — Telegram photo 提取

**Files:**
- Modify: `src-tauri/src/remote_im/channels/telegram.rs` (~line 63-127)

- [ ] **Step 1: Extract photo file_id into attachments**

In `src-tauri/src/remote_im/channels/telegram.rs`, find the message parsing loop (~line 63-77):

```rust
            let msg = upd.get("message").or_else(|| upd.get("edited_message"));
            let Some(msg) = msg else { continue };
            let text = msg
                .get("text")
                .or_else(|| msg.get("caption"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                continue;
            }
```

Replace with attachment extraction:

```rust
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
```

Then find the `IncomingMessage { ... attachments: vec![] }` literal for telegram (~line 127) and change `attachments: vec![]` to `attachments,`.

- [ ] **Step 2: Run build + telegram tests**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
cargo test -p omp-desktop remote_im 2>&1 | grep "test result" | tail -2
```
Expected: no errors; all remote_im tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/channels/telegram.rs
git commit -m "feat(remote_im/telegram): extract photo file_id into attachments"
```

---

### Task 6: 适配器入站 — Discord attachments 提取

**Files:**
- Modify: `src-tauri/src/remote_im/channels/discord.rs` (`parse_message` fn, ~line 146-180)

- [ ] **Step 1: Extract image attachment URLs**

In `src-tauri/src/remote_im/channels/discord.rs`, find `parse_message` (~line 146). Current:

```rust
fn parse_message(inst: &ChannelInstance, d: &Value) -> Option<IncomingMessage> {
    if d.get("author").and_then(|a| a.get("bot")).and_then(|b| b.as_bool()) == Some(true) {
        return None;
    }
    let content = d.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
    if content.is_empty() {
        return None;
    }
```

Replace the `if content.is_empty()` guard with attachment extraction (`Attachment` types are fully-qualified since only `IncomingMessage`/`ChannelInstance` are imported at top):

```rust
fn parse_message(inst: &ChannelInstance, d: &Value) -> Option<IncomingMessage> {
    if d.get("author").and_then(|a| a.get("bot")).and_then(|b| b.as_bool()) == Some(true) {
        return None;
    }
    let content = d.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
    // Extract image attachments (Discord CDN urls are public, no auth needed).
    let mut attachments: Vec<super::super::types::Attachment> = Vec::new();
    if let Some(arr) = d.get("attachments").and_then(|a| a.as_array()) {
        for att in arr {
            let is_image = att
                .get("content_type")
                .and_then(|ct| ct.as_str())
                .map(|ct| ct.starts_with("image/"))
                .unwrap_or(false);
            if is_image {
                if let Some(url) = att.get("url").and_then(|u| u.as_str()) {
                    attachments.push(super::super::types::Attachment {
                        kind: super::super::types::AttachmentKind::Image,
                        source: super::super::types::AttachmentSource::Discord {
                            url: url.to_string(),
                        },
                    });
                }
            }
        }
    }
    // Drop only if there's no text AND no image attachments.
    if content.is_empty() && attachments.is_empty() {
        return None;
    }
```

Then in the `IncomingMessage { ... attachments: vec![] }` literal (~line 170), change `attachments: vec![]` to `attachments,`.

- [ ] **Step 2: Run build + discord tests**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
cargo test -p omp-desktop remote_im 2>&1 | grep "test result" | tail -2
```
Expected: no errors; all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/channels/discord.rs
git commit -m "feat(remote_im/discord): extract image attachment urls into attachments"
```

---

### Task 7: 适配器入站 — 飞书 image_key 进 attachments

**Files:**
- Modify: `src-tauri/src/remote_im/channels/feishu.rs` (~line 322-340, ~line 389)

- [ ] **Step 1: Route image_key to attachments instead of content text**

In `src-tauri/src/remote_im/channels/feishu.rs`, find the media-tag injection (~line 322-336):

```rust
    if text.is_empty() || msg_type == "image" || msg_type == "file" || msg_type == "audio" {
        if let Some(key) = content_json
            .get("image_key")
            .or_else(|| content_json.get("file_key"))
            .and_then(|x| x.as_str())
        {
            let tag = format!("[{msg_type}:{key}]");
            if text.is_empty() {
                text = format!("请查看附件 {tag}");
            } else {
                text = format!("{text}\n{tag}");
            }
        }
    }
```

Replace with attachment collection (keep the `text` fallback only for audio/non-image):

```rust
    let mut attachments: Vec<super::super::types::Attachment> = Vec::new();
    if msg_type == "image" || msg_type == "file" {
        let key = content_json
            .get("image_key")
            .or_else(|| content_json.get("file_key"))
            .and_then(|x| x.as_str());
        if let Some(key) = key {
            let resource_type = if msg_type == "image" { "image" } else { "file" };
            let kind = if msg_type == "image" {
                super::super::types::AttachmentKind::Image
            } else {
                super::super::types::AttachmentKind::File
            };
            attachments.push(super::super::types::Attachment {
                kind,
                source: super::super::types::AttachmentSource::Feishu {
                    message_id: message_id.clone(),
                    file_key: key.to_string(),
                    resource_type: resource_type.to_string(),
                },
            });
        }
    } else if msg_type == "audio" {
        // Audio not supported in P2; surface a placeholder so the user knows.
        if let Some(key) = content_json.get("file_key").and_then(|x| x.as_str()) {
            if text.is_empty() {
                text = format!("[audio:{key}]");
            }
        }
    }
```

Note: `message_id` must be available at this point — verify it is extracted before this block (it is, at ~line 345 in the original; if not, move its extraction earlier). Then in the `IncomingMessage { ... attachments: vec![] }` literal (~line 389), change to `attachments,`.

- [ ] **Step 2: Run build + feishu tests**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
cargo test -p omp-desktop remote_im 2>&1 | grep "test result" | tail -2
```
Expected: no errors; all tests pass. If a feishu test asserts `[image:key]` in content, it will fail — update that test to assert the attachment is populated instead.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote_im/channels/feishu.rs
git commit -m "feat(remote_im/feishu): route image_key into attachments instead of content tag"
```

---

### Task 8: 全量验证

- [ ] **Step 1: Full remote_im test suite**

```bash
cargo test -p omp-desktop remote_im 2>&1 | grep -E "test result|FAILED|error\[" | tail -10
```
Expected: all pass (existing + P1 + P2 new).

- [ ] **Step 2: clippy**

```bash
cargo clippy -p omp-desktop 2>&1 | grep -iE "media\.rs|prompt_block|Attachment|attachments" | head
```
Expected: empty (no new warnings in P2 files).

- [ ] **Step 3: build**

```bash
cargo build -p omp-desktop 2>&1 | grep "Finished"
```
Expected: Finished successfully.

---

## Self-Review

**Spec coverage:**
- §4.1 类型扩展 → Task 1 ✓
- §4.2 media.rs 下载 + MIME → Task 2 ✓
- §4.3 ACP PromptBlock + prompt_with_blocks → Task 3 ✓
- §4.4 Engine run_agent_turn 集成 → Task 4 ✓
- §4.5 适配器改动（Telegram/Discord/飞书）→ Tasks 5,6,7 ✓
- §5 测试 → 内嵌于各 Task ✓
- §7 验收 → Task 8 ✓

**Placeholder scan:** Task 6 Step 1 有一个"Wait —"的思考过程残留，已修正为正确代码。其余无占位符。

**Type consistency:** `Attachment`/`AttachmentKind`/`AttachmentSource` 在 Task 1 定义，Tasks 5/6/7 引用时命名一致。`PromptBlock`/`prompt_with_blocks` 在 Task 3 定义，Task 4 引用一致。`fetch_attachment`/`MediaBytes` 在 Task 2 定义，Task 4 引用一致。
