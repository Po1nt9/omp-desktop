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
}
