//! Inbound media download + MIME inference, shared across channels.
use super::outbound::http_client;
use super::types::{Attachment, AttachmentSource};
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
            let mime = if resource_type == "image" {
                "image/png"
            } else {
                "application/octet-stream"
            };
            Ok(MediaBytes {
                data,
                mime_type: mime.to_string(),
            })
        }
        AttachmentSource::Telegram { file_id } => {
            let token = secrets
                .get("bot_token")
                .or_else(|| secrets.get("token"))
                .ok_or("missing telegram bot_token")?;
            let client = http_client()?;
            // Step 1: getFile → file_path
            let get_url = format!("https://api.telegram.org/bot{token}/getFile?file_id={file_id}");
            let resp: Value = client
                .get(&get_url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
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
            Ok(MediaBytes {
                data,
                mime_type: mime,
            })
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
            Ok(MediaBytes {
                data,
                mime_type: mime,
            })
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
        let mb = MediaBytes {
            data: vec![1, 2, 3],
            mime_type: "image/png".into(),
        };
        assert_eq!(mb.data, vec![1, 2, 3]);
        assert_eq!(mb.mime_type, "image/png");
    }
}
