//! Crash-recovery assessment for the durable event journal (AC-1.10).
//!
//! Master design §10 conservative recovery: a turn whose TurnStart has no
//! paired TurnEnd is marked `unknown/interrupted`; nothing is auto-replayed.
//! This module is pure filesystem/journal logic — no Runtime dependency.

use super::{EventJournal, EventKind};

/// Recovery verdict for a loaded journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryState {
    /// All recorded turns are closed (or the journal is empty).
    Clean,
    /// A TurnStart after the last commit point has no paired TurnEnd.
    Interrupted { turn_start_event_id: String, sequence: u64 },
}

/// Assess recovery state. Uses `replay_from(last_commit)` — the first
/// production consumer of `replay_from`. With no commit points, scans the
/// whole event list. A turn-depth counter over TurnStart/TurnEnd keeps the
/// rule correct for any future multi-event tails.
pub fn assess(journal: &EventJournal) -> RecoveryState {
    let tail: Vec<&super::JournalEvent> = match journal.commit_points().last() {
        Some(cp) => journal.replay_from(cp).unwrap_or_default(),
        None => journal.events().iter().collect(),
    };
    let mut depth = 0u32;
    let mut open: Option<(&str, u64)> = None;
    for ev in tail {
        match ev.kind {
            EventKind::TurnStart => {
                depth += 1;
                open = Some((ev.id.as_str(), ev.sequence));
            }
            EventKind::TurnEnd => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    open = None;
                }
            }
            _ => {}
        }
    }
    match open {
        Some((id, seq)) => RecoveryState::Interrupted {
            turn_start_event_id: id.to_string(),
            sequence: seq,
        },
        None => RecoveryState::Clean,
    }
}

/// Close a dangling turn honestly: append TurnEnd with provenance and commit.
/// After this, `assess` returns Clean, making recovery idempotent.
pub fn close_interrupted_turn(journal: &mut EventJournal, turn_start_event_id: &str) {
    journal.append(
        EventKind::TurnEnd,
        serde_json::json!({
            "stopReason": "interrupted",
            "interruptedAfter": turn_start_event_id,
        }),
    );
    let _ = journal.commit();
}

/// Outcome of recovering one session's journal from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryReport {
    /// Journal exists (or was corrupt and quarantined) and no dangling turn
    /// needed closing — nothing for the UI to show.
    Clean,
    /// A dangling turn was found and closed; a marker message was appended.
    Interrupted {
        turn_start_event_id: String,
        marker_message_id: String,
        content: String,
    },
}

/// Marker message content (pipe format, mirrors `turn_cancelled|<reason>`).
/// Parsed by the frontend end-of-turn chip pipeline.
pub const INTERRUPTED_MARKER_CONTENT: &str = "turn_interrupted|crash_recovery";

/// Recover one session's journal after a restart.
///
/// - No journal on disk → `None` (nothing to recover, no UI work).
/// - Corrupt journal → quarantine to `event_journal.corrupt-<unixts>.json`,
///   return `Some(Clean)` (no marker without evidence; a fresh journal will
///   be started).
/// - Dangling TurnStart after the last commit → close the turn honestly in
///   the journal (making recovery idempotent), persist, and append an
///   idempotent `turn_interrupted` marker to messages.json following the
///   `turn_cancelled` precedent (role "tool", is_error, marker field).
pub fn recover_session_journal(app_session_id: &str) -> Option<RecoveryReport> {
    let path = EventJournal::standard_path(app_session_id);
    if !path.exists() {
        return None;
    }
    let mut journal = match EventJournal::load_from(&path) {
        Ok(j) => j,
        Err(err) => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let quarantine = path.with_file_name(format!("event_journal.corrupt-{ts}.json"));
            tracing::warn!(
                "event journal corrupt for {app_session_id}: {err}; quarantining to {}",
                quarantine.display()
            );
            if let Err(rename_err) = std::fs::rename(&path, &quarantine) {
                tracing::warn!("failed to quarantine corrupt journal: {rename_err}");
            }
            return Some(RecoveryReport::Clean);
        }
    };
    match assess(&journal) {
        RecoveryState::Clean => Some(RecoveryReport::Clean),
        RecoveryState::Interrupted {
            turn_start_event_id,
            ..
        } => {
            close_interrupted_turn(&mut journal, &turn_start_event_id);
            if let Err(err) = journal.save_to(&path) {
                tracing::warn!("failed to persist recovered journal for {app_session_id}: {err}");
            }
            let marker_message_id = uuid::Uuid::new_v4().to_string();
            let content = INTERRUPTED_MARKER_CONTENT.to_string();
            if let Err(err) = crate::store::append_message(
                app_session_id,
                crate::store::ChatMessageStored {
                    id: marker_message_id.clone(),
                    role: "tool".into(),
                    content: content.clone(),
                    thought: None,
                    created_at: chrono::Utc::now(),
                    is_error: true,
                    attachments: None,
                    marker: Some("turn_interrupted".into()),
                },
            ) {
                tracing::warn!("failed to append turn_interrupted marker: {err}");
            }
            Some(RecoveryReport::Interrupted {
                turn_start_event_id,
                marker_message_id,
                content,
            })
        }
    }
}

/// Write-ahead TurnStart (design D1): append the boundary AND persist the
/// journal immediately, so a crash mid-turn leaves a dangling TurnStart on
/// disk that recovery can detect. Without this, the journal only reflects
/// the last clean TurnEnd save and crash-vs-clean-exit is indistinguishable.
/// Best-effort persistence — a save failure only loses crash detectability,
/// never blocks the turn. Returns the new event's stable id.
pub fn append_turn_start_durable(journal: &mut EventJournal) -> String {
    let id = journal.append(EventKind::TurnStart, serde_json::json!({}));
    if let Err(err) = journal.save_to(&EventJournal::standard_path(journal.session_id())) {
        tracing::warn!(
            "event journal write-ahead save failed for {}: {err}",
            journal.session_id()
        );
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assess_clean_when_tail_empty() {
        let mut j = EventJournal::new("s1".into());
        j.append(EventKind::TurnStart, serde_json::json!({}));
        j.append(EventKind::TurnEnd, serde_json::json!({ "stopReason": "end_turn" }));
        let _ = j.commit();
        assert_eq!(assess(&j), RecoveryState::Clean);
    }

    #[test]
    fn assess_interrupted_on_dangling_turn_start() {
        let mut j = EventJournal::new("s1".into());
        j.append(EventKind::TurnStart, serde_json::json!({}));
        j.append(EventKind::TurnEnd, serde_json::json!({ "stopReason": "end_turn" }));
        let _ = j.commit();
        let dangling = j.append(EventKind::TurnStart, serde_json::json!({}));
        match assess(&j) {
            RecoveryState::Interrupted { turn_start_event_id, .. } => {
                assert_eq!(turn_start_event_id, dangling);
            }
            other => panic!("expected Interrupted, got {other:?}"),
        }
    }

    #[test]
    fn assess_interrupted_without_commit_points() {
        let mut j = EventJournal::new("s1".into());
        let dangling = j.append(EventKind::TurnStart, serde_json::json!({}));
        match assess(&j) {
            RecoveryState::Interrupted { turn_start_event_id, .. } => {
                assert_eq!(turn_start_event_id, dangling);
            }
            other => panic!("expected Interrupted, got {other:?}"),
        }
    }

    #[test]
    fn assess_clean_on_empty_journal() {
        let j = EventJournal::new("s1".into());
        assert_eq!(assess(&j), RecoveryState::Clean);
    }

    #[test]
    fn close_interrupted_turn_makes_assess_clean() {
        let mut j = EventJournal::new("s1".into());
        let dangling = j.append(EventKind::TurnStart, serde_json::json!({}));
        close_interrupted_turn(&mut j, &dangling);
        assert_eq!(assess(&j), RecoveryState::Clean);
        // The closing TurnEnd carries honest provenance.
        let last = j.events().last().unwrap();
        assert!(matches!(last.kind, EventKind::TurnEnd));
        assert_eq!(last.data["stopReason"], "interrupted");
        assert_eq!(last.data["interruptedAfter"], dangling.as_str());
        // Commit point was advanced so the next replay tail is empty.
        let cp = j.commit_points().last().unwrap();
        assert!(j.replay_from(cp).is_none());
    }

    // --- recover_session_journal (filesystem) ---

    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Hold the module guard AND the shared app-home env lock for the whole
    /// test, and point OMP_DESKTOP_HOME at a fresh temp dir.
    fn test_home(
        tag: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        parking_lot::MutexGuard<'static, ()>,
        std::path::PathBuf,
    ) {
        let module = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let dir = std::env::temp_dir().join(format!(
            "omp-recovery-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("OMP_DESKTOP_HOME", &dir);
        (module, env, dir)
    }

    fn seed_dangling_journal(app_session_id: &str) -> String {
        let mut j = EventJournal::new(app_session_id.into());
        j.append(EventKind::TurnStart, serde_json::json!({}));
        j.append(EventKind::TurnEnd, serde_json::json!({ "stopReason": "end_turn" }));
        let _ = j.commit();
        let dangling = j.append(EventKind::TurnStart, serde_json::json!({}));
        j.save_to(&EventJournal::standard_path(app_session_id)).unwrap();
        dangling
    }

    #[test]
    fn recover_marks_interrupted_idempotently() {
        let (_m, _e, _dir) = test_home("idem");
        let sid = "sess-idem";
        let dangling = seed_dangling_journal(sid);

        let first = recover_session_journal(sid);
        match &first {
            Some(RecoveryReport::Interrupted {
                turn_start_event_id,
                marker_message_id,
                content,
            }) => {
                assert_eq!(turn_start_event_id, &dangling);
                assert!(!marker_message_id.is_empty());
                assert_eq!(content, "turn_interrupted|crash_recovery");
            }
            other => panic!("expected Interrupted, got {other:?}"),
        }

        // Marker landed in messages.json exactly once.
        let msgs = crate::store::load_messages(sid);
        let markers: Vec<_> = msgs
            .iter()
            .filter(|m| m.marker.as_deref() == Some("turn_interrupted"))
            .collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].role, "tool");
        assert!(markers[0].is_error);

        // Journal on disk is now closed (paired boundaries).
        let j = EventJournal::load_from(&EventJournal::standard_path(sid)).unwrap();
        assert_eq!(assess(&j), RecoveryState::Clean);

        // Second run: Clean, no duplicate marker.
        assert_eq!(recover_session_journal(sid), Some(RecoveryReport::Clean));
        let msgs = crate::store::load_messages(sid);
        assert_eq!(
            msgs.iter()
                .filter(|m| m.marker.as_deref() == Some("turn_interrupted"))
                .count(),
            1
        );
    }

    #[test]
    fn recover_corrupt_journal_quarantines_without_marker() {
        let (_m, _e, _dir) = test_home("corrupt");
        let sid = "sess-corrupt";
        let path = EventJournal::standard_path(sid);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json !!!").unwrap();

        assert_eq!(recover_session_journal(sid), Some(RecoveryReport::Clean));
        assert!(!path.exists(), "corrupt journal must be moved aside");
        let quarantined = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("event_journal.corrupt-")
            });
        assert!(quarantined, "quarantine file must exist");
        let msgs = crate::store::load_messages(sid);
        assert!(msgs.is_empty(), "no marker without evidence");
    }

    #[test]
    fn recover_missing_journal_returns_none() {
        let (_m, _e, _dir) = test_home("missing");
        assert_eq!(recover_session_journal("sess-missing"), None);
    }

    #[test]
    fn recover_clean_journal_stays_clean() {
        let (_m, _e, _dir) = test_home("clean");
        let sid = "sess-clean";
        let mut j = EventJournal::new(sid.into());
        j.append(EventKind::TurnStart, serde_json::json!({}));
        j.append(EventKind::TurnEnd, serde_json::json!({ "stopReason": "end_turn" }));
        let _ = j.commit();
        j.save_to(&EventJournal::standard_path(sid)).unwrap();
        assert_eq!(recover_session_journal(sid), Some(RecoveryReport::Clean));
        assert!(crate::store::load_messages(sid).is_empty());
    }

    // --- append_turn_start_durable ---

    #[test]
    fn append_turn_start_durable_persists_immediately() {
        let (_m, _e, _dir) = test_home("wal");
        let sid = "sess-wal";
        let mut j = EventJournal::new(sid.into());

        let event_id = append_turn_start_durable(&mut j);
        assert!(event_id.starts_with("evt_"));

        // The TurnStart is on disk WITHOUT waiting for TurnEnd save — this is
        // the write-ahead property that makes mid-turn crashes detectable.
        let on_disk = EventJournal::load_from(&EventJournal::standard_path(sid)).unwrap();
        assert_eq!(on_disk.events().len(), 1);
        assert_eq!(on_disk.events()[0].kind, EventKind::TurnStart);
        assert_eq!(on_disk.events()[0].id, event_id);
    }
}
