# P2: 入站媒体接收 (Inbound Media) — Design Spec

- **日期**: 2026-07-30
- **工作包**: P2（Remote IM 入站图片接收）
- **状态**: Draft
- **前置**: P1 去重+限流已完成；Remote IM Runtime Bridge 已合并 main
- **关联**: Master design §2.24（attachment capability）、§5.2（1.0 必选能力含 attachment）、Remote IM Bridge spec（排除项）

## 1. 背景与可行性

当前 remote_im 引擎是纯文本管道：`IncomingMessage.content: String`，无媒体字段。用户发的图片消息要么被编码成 `[image:key]` 文本占位（飞书/钉钉/微信），要么被静默丢弃（Telegram/Discord/Slack 等）。Agent 无法实际"看到"图片内容。

**可行性已验证**（探索结论）：
- OMP Runtime 的 ACP `session/prompt` RPC 的 `prompt` 参数是 content blocks 数组，原生支持 `{type:"image", data:<base64>, mimeType}` block。
- Runtime 端 `AcpAgent.#convertPromptBlocks`（acp-agent.ts:2134）把 image blocks 提取为 `AgentImageContent[]`，传给 `session.prompt(text, {images})`。
- **结论**：只要 Rust 端在构造 `session/prompt` 参数时加入 image content block，Runtime 就能消费图片。

**缺口**：
- Rust `wire_session_prompt_params` 只构造 `[{type:"text"}]`。
- `IncomingMessage` 无 attachments 字段。
- 14 个适配器中仅飞书有 `download_message_resource`；其余无下载能力。

## 2. 需求

| # | 需求 | 来源 |
|---|------|------|
| R1 | 用户通过 IM 发送图片，Agent 能接收并理解图片内容 | Master design §5.2 attachment 必选能力 |
| R2 | 先支持 3 个渠道：飞书、Telegram、Discord（下载 API 最简单/已存在） | YAGNI，可验证增量 |
| R3 | 仅图片（image/* MIME）；文件/语音/视频为非目标 | YAGNI |
| R4 | 出站媒体发送为非目标（独立工作包） | 范围控制 |

## 3. 架构

```
适配器入站解析                Engine (run_agent_turn)           ACP
┌─────────────┐   IncomingMessage    ┌──────────────────┐   session/prompt
│ feishu.rs   │──┐  + attachments   │ 1.下载图片字节   │──▶ {prompt: [
│ telegram.rs │──┤  : Vec<Attachment>│ 2.base64 编码    │      {type:text,...},
│ discord.rs  │──┘                   │ 3.构造 image block│     {type:image,...}
└─────────────┘                      └──────────────────┘   ]}
```

## 4. 组件设计

### 4.1 类型扩展 (`types.rs`)

```rust
#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub source: AttachmentSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind { Image, File }

/// 平台特定的下载凭证。渠道数有限且已知，用 enum 而非 trait object。
#[derive(Debug, Clone)]
pub enum AttachmentSource {
    /// 飞书: download_message_resource(message_id, file_key, resource_type)
    Feishu { message_id: String, file_key: String, resource_type: String },
    /// Telegram: file_id → getFile API → file_path → 下载
    Telegram { file_id: String },
    /// Discord: CDN url 直接 GET
    Discord { url: String },
}

pub struct IncomingMessage {
    // ... 现有字段不变 ...
    pub content: String,            // 纯文本（飞书不再注入占位标记）
    pub attachments: Vec<Attachment>, // 图片等附件，默认空 vec
}
```

`IncomingMessage` 加 `attachments: Vec<Attachment>` 字段。现有构造点全部补 `attachments: vec![]`。

### 4.2 媒体下载 (`media.rs`，新文件)

平台无关下载入口：
```rust
pub struct MediaBytes { pub data: Vec<u8>, pub mime_type: String }

/// 下载附件字节。mime_type 由来源推断（飞书 resource_type / Telegram photo / Discord url 扩展名）。
pub async fn fetch_attachment(
    channel: &str,
    secrets: &HashMap<String, String>,
    options: &Value,
    att: &Attachment,
) -> Result<MediaBytes, String>;
```

- **飞书**：调 `feishu::download_message_resource(channel, secrets, options, message_id, file_key, resource_type)`。mime_type 由 resource_type 映射（image→image/png）。
- **Telegram**：GET `https://api.telegram.org/bot{token}/getFile?file_id={file_id}` → 取 `result.file_path` → GET `https://api.telegram.org/file/bot{token}/{file_path}`。mime_type 从 file_path 扩展名推断。
- **Discord**：直接 GET `url`（CDN 公开，无需鉴权）。mime_type 从 url 扩展名或 content-type 推断。

### 4.3 ACP 扩展 (`acp_client.rs`)

新增 content block 类型和构造函数（不破坏现有 `prompt(&str)`）：
```rust
pub enum PromptBlock {
    Text { text: String },
    Image { data: String, mime_type: String }, // data = base64 编码
}

pub fn wire_session_prompt_params_blocks(session_id: &str, blocks: &[PromptBlock]) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": blocks.iter().map(|b| match b {
            PromptBlock::Text { text } => json!({ "type": "text", "text": text }),
            PromptBlock::Image { data, mime_type } =>
                json!({ "type": "image", "data": data, "mimeType": mime_type }),
        }).collect::<Vec<_>>()
    })
}
```

`AcpClient` 加 `prompt_with_blocks(&self, blocks: &[PromptBlock])` 方法，内部调 `wire_session_prompt_params_blocks`。现有 `prompt(&str)` 保持不变（委托给单文本 block）。

### 4.4 Engine 集成 (`engine.rs`)

在 `run_agent_turn` 中，构造 prompt blocks：
```rust
let mut blocks = vec![PromptBlock::Text { text: prompt.to_string() }];
// 从 instances 取该渠道的 secrets + options（下载需要平台凭证）
let inst = self.instances.lock().get(&msg.instance_id).cloned();
if let Some(inst) = inst {
    for att in msg.attachments.iter().filter(|a| a.kind == AttachmentKind::Image) {
        match media::fetch_attachment(&msg.channel, &inst.secrets, &inst.options, att).await {
            Ok(media) => {
                let b64 = base64::encode(&media.data);
                blocks.push(PromptBlock::Image { data: b64, mime_type: media.mime_type });
            }
            Err(e) => tracing::warn!(target: "remote_im::media", "download failed: {e}"),
        }
    }
}
runtime.acp.prompt_with_blocks(&blocks).await;
```

下载失败不阻断 turn——图片下载失败时，文本 prompt 仍正常发送，Agent 收到的是无图的文本。

### 4.5 适配器入站改动

**飞书 (`feishu.rs`)**：现有逻辑在第 322-336 行把 `[image:key]` 塞进 content。改为：提取到 `msg.attachments`，content 不再追加占位标记。

**Telegram (`telegram.rs`)**：第 75-77 行 `if text.is_empty() { continue; }` 丢弃纯图片。改为：若 `msg.photo` 数组非空，取最后一个（最高分辨率）的 `file_id`，构造 `AttachmentSource::Telegram { file_id }` 加入 attachments。caption 仍进 content。仅当 text 和 attachments 都空时才 continue。

**Discord (`discord.rs`)**：第 150 行只取 content。新增：检查 `d["attachments"]` 数组，对每个 `content_type` 以 `image/` 开头的附件，取 `url` 构造 `AttachmentSource::Discord { url }`。

## 5. 测试策略（TDD）

### 5.1 media.rs 测试
- `test_discord_url_download`：mock HTTP，验证 GET url 返回字节 + mime 推断
- `test_telegram_getfile_then_download`：mock getFile + 文件下载两步
- `test_mime_from_extension`：.png→image/png, .jpg→image/jpeg, .webp→image/webp

### 5.2 acp_client.rs 测试
- `test_wire_blocks_text_only`：单文本 block，JSON 与现有格式一致
- `test_wire_blocks_with_image`：含 image block，验证 `type/data/mimeType` 字段

### 5.3 适配器入站测试
- Telegram：构造含 `photo` 数组的 webhook payload，验证 attachments 被填充
- Discord：构造含 `attachments` 数组的 gateway event，验证 url 被提取
- 飞书：验证 image_key 进入 attachments 而非 content

### 5.4 base64 依赖
项目需确认 `base64` crate 已在 Cargo.toml；若无则新增（轻量依赖）。

## 6. 配置与非目标

**非目标**：
- ❌ 出站媒体发送（`OutboundRouter::reply_media`、各适配器 `send_image`）
- ❌ Slack/QQ/QQBot/微信/钉钉/WeCom/Matrix/LINE/微博/WPS 等其他渠道
- ❌ 文件/语音/视频附件
- ❌ 媒体持久化（下载即用即弃，不落盘）
- ❌ 图片大小限制/压缩（Runtime 端有 `autoResizeImages`，Rust 端不做）

## 7. 验收标准

- [ ] `cargo test -p omp-desktop remote_im::media` 全绿
- [ ] `cargo test -p omp-desktop remote_im::acp` image block 构造测试全绿
- [ ] Telegram/Discord/飞书入站解析测试全绿（attachments 正确填充）
- [ ] `cargo build` + `cargo clippy` 无新 warning
- [ ] 端到端（手动）：发图片给 Telegram bot，Runtime 收到 image content block（日志验证）
