use super::*;

#[test]
fn journal_generates_stable_event_ids() {
    let mut journal = EventJournal::new("sess_test".to_string());
    let id1 = journal.append(EventKind::TurnStart, serde_json::json!({}));
    let id2 = journal.append(EventKind::TurnEnd, serde_json::json!({}));
    assert!(id1.starts_with("evt_"));
    assert!(id2.starts_with("evt_"));
    assert_ne!(id1, id2);
}

#[test]
fn journal_tracks_commit_points() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({}));
    let commit1 = journal.commit();
    journal.append(EventKind::TurnEnd, serde_json::json!({}));
    let commit2 = journal.commit();
    assert_ne!(commit1, commit2);
}

#[test]
fn journal_replay_from_commit_point() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({}));
    let commit = journal.commit();
    journal.append(EventKind::TurnEnd, serde_json::json!({}));
    let replayed = journal.replay_from(&commit).unwrap();
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].kind, EventKind::TurnEnd);
}

#[test]
fn event_id_has_exact_shape() {
    // evt_ + 26 chars from [a-z2-7] = 30 chars total.
    let mut journal = EventJournal::new("sess_test".to_string());
    let id = journal.append(EventKind::MessageStart, serde_json::json!({}));
    assert_eq!(id.len(), 30, "event id must be 30 chars: evt_ + 26");
    assert!(id.starts_with("evt_"));
    let tail = &id[4..];
    assert_eq!(tail.len(), 26);
    for c in tail.chars() {
        assert!(
            ('a'..='z').contains(&c) || ('2'..='7').contains(&c),
            "char {c} outside base32 alphabet"
        );
    }
}

#[test]
fn commit_token_has_exact_shape() {
    // cp_ + 32 hex chars = 35 chars total.
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::MessageStart, serde_json::json!({}));
    let cp = journal.commit();
    assert!(cp.commit_token.starts_with("cp_"));
    assert_eq!(cp.commit_token.len(), 35, "commit token must be 35 chars: cp_ + 32 hex");
    let tail = &cp.commit_token[3..];
    assert_eq!(tail.len(), 32);
    for c in tail.chars() {
        assert!(c.is_ascii_hexdigit(), "char {c} is not hex");
    }
}

#[test]
fn replay_returns_none_when_nothing_after_commit() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({}));
    let commit = journal.commit();
    // Nothing was appended after the commit.
    assert!(journal.replay_from(&commit).is_none());
}

#[test]
fn events_are_recorded_in_order() {
    let mut journal = EventJournal::new("sess_test".to_string());
    journal.append(EventKind::TurnStart, serde_json::json!({"n": 1}));
    journal.append(EventKind::MessageStart, serde_json::json!({"n": 2}));
    journal.append(EventKind::MessageEnd, serde_json::json!({"n": 3}));
    let events = journal.events();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 0);
    assert_eq!(events[1].sequence, 1);
    assert_eq!(events[2].sequence, 2);
}
