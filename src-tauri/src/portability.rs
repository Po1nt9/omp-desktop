//! Session portability: export/import a session (messages + journal + meta)
//! for cross-device migration. Idempotent import (skip-on-exists), modeled
//! after Hermes Agent's `hermes_state_portability.py`.
use crate::event_journal::EventJournal;
use crate::store::{self, ChatMessageStored, SessionMeta};
use serde::{Deserialize, Serialize};

const MAX_MESSAGES_PER_SESSION: usize = 10_000;
const MAX_SESSION_BYTES: usize = 5 * 1024 * 1024; // 5 MB

/// A portable session bundle (journal + messages + meta).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableSession {
    pub meta: SessionMeta,
    pub messages: Vec<ChatMessageStored>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<EventJournal>,
}

/// Result of an import attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: bool,
    pub session_id: String,
    pub reason: String,
}

impl ImportResult {
    fn imported(id: &str) -> Self {
        Self {
            imported: true,
            session_id: id.into(),
            reason: "imported".into(),
        }
    }
    fn skipped(id: &str) -> Self {
        Self {
            imported: false,
            session_id: id.into(),
            reason: "session already exists".into(),
        }
    }
}

/// Export a single session as a `PortableSession`.
pub fn export_session(session_id: &str) -> Result<PortableSession, String> {
    let meta = store::load_sessions_index()
        .into_iter()
        .find(|m| m.id == session_id)
        .ok_or_else(|| format!("session {session_id} not found"))?;
    let messages = store::load_messages(session_id);
    let journal = EventJournal::load_from(&EventJournal::standard_path(session_id)).ok();
    Ok(PortableSession {
        meta,
        messages,
        journal,
    })
}

/// Idempotent import: if `session_id` already exists, skip (do not overwrite).
pub fn import_session(data: &PortableSession) -> Result<ImportResult, String> {
    if data.messages.len() > MAX_MESSAGES_PER_SESSION {
        return Err(format!(
            "too many messages: {} > {MAX_MESSAGES_PER_SESSION}",
            data.messages.len()
        ));
    }
    let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SESSION_BYTES {
        return Err(format!(
            "session too large: {} > {MAX_SESSION_BYTES}",
            bytes.len()
        ));
    }
    let exists = store::load_sessions_index()
        .iter()
        .any(|m| m.id == data.meta.id);
    if exists {
        return Ok(ImportResult::skipped(&data.meta.id));
    }
    store::save_messages(&data.meta.id, &data.messages)?;
    store::update_session_meta(&data.meta)?;
    if let Some(j) = &data.journal {
        let _ = j.save_to(&EventJournal::standard_path(&data.meta.id));
    }
    Ok(ImportResult::imported(&data.meta.id))
}

/// Helper for tests: construct a `ChatMessageStored` without `Default`.
#[cfg(test)]
fn make_msg(role: &str, content: &str) -> ChatMessageStored {
    use chrono::Utc;
    ChatMessageStored {
        id: uuid::Uuid::new_v4().to_string(),
        role: role.into(),
        content: content.into(),
        thought: None,
        created_at: Utc::now(),
        is_error: false,
        attachments: None,
        marker: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_suffix() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string()
    }

    fn test_meta(id: &str) -> SessionMeta {
        use chrono::Utc;
        SessionMeta {
            id: id.into(),
            project_id: None,
            title: "portability test".into(),
            agent_session_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            model_id: None,
            archived: false,
            pinned: false,
            effort: None,
            mode: None,
            permission_policy: None,
            scheduled: false,
        }
    }

    fn cleanup(id: &str) {
        let _ = std::fs::remove_dir_all(crate::paths::session_dir(id));
        let mut list = store::load_sessions_index();
        list.retain(|m| m.id != id);
        let _ = store::save_sessions_index(&list);
    }

    /// These tests use the real app data root; serialize against tests that
    /// temporarily point `OMP_DESKTOP_HOME` elsewhere.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::paths::APP_HOME_ENV_LOCK.lock().unwrap()
    }

    #[test]
    fn test_import_too_many_messages_rejected() {
        let _env = env_lock();
        let data = PortableSession {
            meta: test_meta("toomany"),
            messages: (0..MAX_MESSAGES_PER_SESSION + 1)
                .map(|_| make_msg("user", "x"))
                .collect(),
            journal: None,
        };
        let res = import_session(&data);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("too many messages"));
        cleanup("toomany");
    }

    #[test]
    fn test_import_skips_existing_session() {
        let _env = env_lock();
        let id = format!("skip-test-{}", unique_suffix());
        let meta = test_meta(&id);
        store::save_messages(&id, &[]).unwrap();
        store::update_session_meta(&meta).unwrap();
        let data = PortableSession {
            meta,
            messages: vec![],
            journal: None,
        };
        let res = import_session(&data).unwrap();
        assert!(!res.imported);
        assert_eq!(res.reason, "session already exists");
        cleanup(&id);
    }

    #[test]
    fn test_roundtrip_export_import() {
        let _env = env_lock();
        let id = format!("rt-test-{}", unique_suffix());
        let msg = make_msg("user", "hello portability");
        let meta = test_meta(&id);
        store::save_messages(&id, &[msg.clone()]).unwrap();
        store::update_session_meta(&meta).unwrap();

        let exported = export_session(&id).unwrap();
        assert_eq!(exported.messages.len(), 1);
        assert_eq!(exported.messages[0].content, "hello portability");

        // Remove the source so re-import is a genuine import.
        cleanup(&id);

        let res = import_session(&exported).unwrap();
        assert!(res.imported, "should import after source removed");

        let loaded = store::load_messages(&id);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "hello portability");
        cleanup(&id);
    }
}
