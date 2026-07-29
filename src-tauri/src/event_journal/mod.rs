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
