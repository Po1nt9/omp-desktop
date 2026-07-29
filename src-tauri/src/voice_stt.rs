//! Speech-to-text for Composer dictation.
//!
//! Plan 1 fail-closed shell: direct xAI STT network/credential code has been
//! removed. Every transcription path returns `runtime_unavailable` until an
//! OMP Runtime integration supplies live speech support. Pure parsers and DTOs
//! remain as the stable contract for a later plan.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AgentError, AgentErrorCode};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatusDto {
    pub available: bool,
    /// Stable class when unavailable: `runtime_unavailable`.
    pub reason: Option<String>,
    pub auth_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTranscribeResult {
    pub ok: bool,
    pub text: Option<String>,
    pub error: Option<String>,
    pub error_class: Option<String>,
}

/// Stable runtime-unavailable error for every speech path.
pub fn runtime_unavailable_error() -> AgentError {
    AgentError::new(
        AgentErrorCode::RuntimeUnavailable,
        "Speech runtime is unavailable (fail-closed shell).",
    )
}

/// Plan 1 fail-closed: voice is unavailable until runtime integration.
pub fn voice_status() -> VoiceStatusDto {
    VoiceStatusDto {
        available: false,
        reason: Some("runtime_unavailable".into()),
        auth_source: None,
    }
}

/// Plan 1 fail-closed: transcription is unavailable until runtime integration.
pub async fn voice_transcribe(
    _audio_base64: String,
    _filename: Option<String>,
    _mime: Option<String>,
) -> VoiceTranscribeResult {
    VoiceTranscribeResult {
        ok: false,
        text: None,
        error: Some("Speech transcription is unavailable in this build.".into()),
        error_class: Some("runtime_unavailable".into()),
    }
}

fn guess_mime(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else if lower.ends_with(".mp4") || lower.ends_with(".m4a") {
        "audio/mp4"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else {
        "audio/webm"
    }
}

/// Parse STT JSON body for transcript text (several plausible shapes).
/// Pure parser retained for a later runtime integration.
pub fn extract_transcript(body: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        // Plain text response
        return body.trim().to_string();
    };
    if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
        return t.to_string();
    }
    if let Some(t) = v.get("transcript").and_then(|x| x.as_str()) {
        return t.to_string();
    }
    if let Some(t) = v
        .pointer("/results/0/alternatives/0/transcript")
        .and_then(|x| x.as_str())
    {
        return t.to_string();
    }
    if let Some(arr) = v.get("segments").and_then(|x| x.as_array()) {
        let mut parts = Vec::new();
        for seg in arr {
            if let Some(t) = seg.get("text").and_then(|x| x.as_str()) {
                parts.push(t.trim());
            }
        }
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }
    String::new()
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttResult {
    pub text: String,
    pub duration: Option<f64>,
    pub language: Option<String>,
}

/// Plan 1 fail-closed: transcription is unavailable until runtime integration.
pub async fn transcribe_base64(
    _audio_b64: &str,
    _mime: Option<&str>,
    _language: Option<&str>,
) -> Result<SttResult, String> {
    Err("runtime_unavailable: speech transcription is unavailable in this build".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_transcript_text_field() {
        assert_eq!(
            extract_transcript(r#"{"text":"hello world"}"#),
            "hello world"
        );
    }

    #[test]
    fn extract_transcript_segments() {
        let body = r#"{"segments":[{"text":"foo"},{"text":"bar"}]}"#;
        assert_eq!(extract_transcript(body), "foo bar");
    }

    #[test]
    fn extract_empty_object() {
        assert!(extract_transcript("{}").is_empty());
    }

    #[test]
    fn guess_mime_webm() {
        assert_eq!(guess_mime("a.webm"), "audio/webm");
        assert_eq!(guess_mime("a.mp3"), "audio/mpeg");
    }

    #[tokio::test]
    async fn transcribe_returns_runtime_unavailable() {
        let r = voice_transcribe("AAAA".into(), Some("t.webm".into()), None).await;
        assert!(!r.ok);
        assert_eq!(r.error_class.as_deref(), Some("runtime_unavailable"));
    }

    #[tokio::test]
    async fn transcribe_base64_returns_runtime_unavailable() {
        let r = transcribe_base64("AAAA", Some("audio/wav"), Some("en")).await;
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("runtime_unavailable"));
    }

    #[test]
    fn voice_status_is_unavailable() {
        let s = voice_status();
        assert!(!s.available);
        assert_eq!(s.reason.as_deref(), Some("runtime_unavailable"));
    }
}
