//! Event Journal — durable record of ACP events with stable event IDs and
//! commit points for crash recovery.
//!
//! Each appended event receives a stable `evt_<base32>` identifier that
//! survives across replays. `commit()` snapshots the latest event as a
//! `CommitPoint` carrying a `cp_<hex>` token; `replay_from()` returns the
//! tail of the journal that arrived *after* a given commit point.
//!
//! See `docs/superpowers/plans/2026-07-29-plan-3-supervisor-core-acp.md`
//! Task 4 for the contract.

pub mod tests;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Kind of journal entry. Mirrors the v1 protocol event taxonomy plus an
/// internal `JournalCommit` marker for commit points.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventKind {
    TurnStart,
    TurnEnd,
    MessageStart,
    MessageEnd,
    ToolCallStart,
    ToolCallEnd,
    UsageReported,
    ContextCompact,
    JournalCommit,
}

/// A single journal entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEvent {
    pub id: String,
    pub session_id: String,
    pub kind: EventKind,
    pub data: serde_json::Value,
    pub sequence: u64,
    pub timestamp: String,
}

/// A durable marker pointing at the last stable event in the journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitPoint {
    pub session_id: String,
    pub commit_token: String,
    pub stable_event_id: String,
    pub sequence: u64,
}

/// In-memory event journal for a single session.
///
/// Plan 3 Task 4 keeps the journal in memory; later plans persist it to
/// disk and feed it from the live ACP event stream.
pub struct EventJournal {
    session_id: String,
    events: Vec<JournalEvent>,
    commit_points: Vec<CommitPoint>,
    sequence: u64,
}

impl std::fmt::Debug for EventJournal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventJournal")
            .field("session_id", &self.session_id)
            .field("events", &self.events.len())
            .field("commit_points", &self.commit_points.len())
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

impl Clone for EventJournal {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            events: self.events.clone(),
            commit_points: self.commit_points.clone(),
            sequence: self.sequence,
        }
    }
}
impl serde::Serialize for EventJournal {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("EventJournal", 4)?;
        st.serialize_field("sessionId", &self.session_id)?;
        st.serialize_field("events", &self.events)?;
        st.serialize_field("commitPoints", &self.commit_points)?;
        st.serialize_field("sequence", &self.sequence)?;
        st.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventJournal {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            session_id: String,
            events: Vec<JournalEvent>,
            commit_points: Vec<CommitPoint>,
            sequence: u64,
        }
        let r = Raw::deserialize(d)?;
        Ok(EventJournal {
            session_id: r.session_id,
            events: r.events,
            commit_points: r.commit_points,
            sequence: r.sequence,
        })
    }
}

impl EventJournal {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            events: Vec::new(),
            commit_points: Vec::new(),
            sequence: 0,
        }
    }

    /// Append an event and return its stable event ID.
    pub fn append(&mut self, kind: EventKind, data: serde_json::Value) -> String {
        let id = generate_event_id();
        let event = JournalEvent {
            id: id.clone(),
            session_id: self.session_id.clone(),
            kind,
            data,
            sequence: self.sequence,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.sequence += 1;
        self.events.push(event);
        id
    }

    /// Snapshot the latest event as a commit point. Panics if the journal
    /// is empty — a commit must anchor on a real event.
    pub fn commit(&mut self) -> CommitPoint {
        let last_event = self
            .events
            .last()
            .expect("cannot commit empty journal");
        let commit = CommitPoint {
            session_id: self.session_id.clone(),
            commit_token: generate_commit_token(),
            stable_event_id: last_event.id.clone(),
            sequence: last_event.sequence,
        };
        self.commit_points.push(commit.clone());
        commit
    }

    /// Return all events that arrived strictly after the given commit point.
    ///
    /// Returns `None` if no event follows the commit point's sequence
    /// (i.e. there is nothing to replay).
    pub fn replay_from(&self, commit: &CommitPoint) -> Option<Vec<&JournalEvent>> {
        let idx = self
            .events
            .iter()
            .position(|e| e.sequence > commit.sequence)?;
        Some(self.events[idx..].iter().collect())
    }

    /// Read-only access to the full event list.
    pub fn events(&self) -> &[JournalEvent] {
        &self.events
    }

    /// Standard on-disk path: `<session_dir>/event_journal.json`.
    pub fn standard_path(session_id: &str) -> PathBuf {
        crate::paths::session_dir(session_id).join("event_journal.json")
    }

    /// Serialize to a file (pretty JSON). Creates parent dirs as needed.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        std::fs::write(path, bytes)
    }

    /// Deserialize from a file.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read(path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&raw).map_err(|e| e.to_string())
    }
}

/// Generate a stable event ID: `evt_` followed by 26 chars drawn from
/// the lowercase base32 alphabet `abcdefghijklmnopqrstuvwxyz234567`.
///
/// We sample 26 bytes and reduce each modulo 32. Because 256 is divisible
/// by 32, the reduction is unbiased. The output length is always
/// `4 (prefix) + 26 = 30` ASCII chars.
fn generate_event_id() -> String {
    use rand::Rng;
    let bytes: [u8; 26] = rand::thread_rng().gen();
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut result = String::with_capacity(30);
    result.push_str("evt_");
    for b in bytes.iter() {
        // 256 % 32 == 0 → unbiased reduction.
        result.push(ALPHABET[(*b as usize) % 32] as char);
    }
    result
}

/// Generate a commit token: `cp_` followed by 32 lowercase hex chars
/// (16 random bytes encoded as hex).
fn generate_commit_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    format!("cp_{}", hex::encode(&bytes))
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn test_save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "omp-journal-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("event_journal.json");
        let mut journal = EventJournal::new("sess-rt".into());
        let _ = journal.append(EventKind::TurnStart, serde_json::json!({}));
        let _ = journal.commit();
        journal.save_to(&path).expect("save");

        let loaded = EventJournal::load_from(&path).expect("load");
        assert_eq!(loaded.events().len(), 1);
        assert_eq!(loaded.events()[0].id, journal.events()[0].id);
        assert_eq!(loaded.events()[0].kind, EventKind::TurnStart);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file_returns_err() {
        let path = Path::new("/nonexistent/omp-test-journal-xyz.json");
        assert!(EventJournal::load_from(path).is_err());
    }

    #[test]
    fn test_save_creates_parent_dir() {
        let base = std::env::temp_dir().join(format!(
            "omp-journal-parent-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = base.join("nested/deep/event_journal.json");
        let journal = EventJournal::new("sess-pd".into());
        journal.save_to(&path).expect("save creates parents");
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_standard_path_format() {
        let p = EventJournal::standard_path("abc-123");
        assert!(p
            .to_string_lossy()
            .ends_with("sessions/abc-123/event_journal.json"));
    }
}
