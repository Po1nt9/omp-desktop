# Event-Journal Recovery Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire event-journal recovery into the session reconnect production path (AC-1.10): TurnStart write-ahead, pure recovery assessor (`replay_from`'s first production consumer), idempotent `turn_interrupted` marker, journal continuity across restarts.

**Architecture:** New pure-filesystem module `event_journal/recovery.rs` (assess / close_interrupted_turn / recover_session_journal / append_turn_start_durable) + three small cuts in `session_manager.rs` (write-ahead at TurnStart, recovery + journal continuity at journal-attach site) + frontend chip pipeline extension (endOfTurn.ts case, i18n ×3) reusing the `turn_cancelled` precedent.

**Tech Stack:** Rust (Tauri 2), React + TypeScript, vitest, cargo test.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-31-event-journal-recovery-design.md` — decisions D1/D2/D3 are binding.
- Master design §10: never auto-replay Prompt/tool/edit/shell; journal is never a re-injection source.
- Master design §18.2.7: marker copy must say unknown/interrupted and must NOT promise absolute no-duplication.
- Filesystem tests that touch the app data root MUST hold `crate::paths::APP_HOME_ENV_LOCK` for the entire test and set env override `OMP_DESKTOP_HOME` to a temp dir (pattern from `secrets/mod.rs` strict tests).
- No hardcoded user-facing strings in frontend — `createT(locale)` / i18n keys in all 3 locales (`src/i18n/messages.ts` en + zh-CN, `src/i18n/zh-tw.ts`).
- Never log secret values (n/a here, but journal data may contain stop reasons only).
- Gates before final commit: `cargo test --lib`, `pnpm test`, `pnpm typecheck`, `pnpm check:i18n`, `pnpm check:brand`, `pnpm check:provenance`, `pnpm check:legal`.
- Repo: `/Users/po1nt9/Github/grok-app-main`, branch `main`, commit per task.

---

### Task 1: EventJournal accessors + recovery assessor (assess / close_interrupted_turn)

**Files:**
- Modify: `src-tauri/src/event_journal/mod.rs` (add accessors after `events()` at :173-177; register `pub mod recovery;` near top)
- Create: `src-tauri/src/event_journal/recovery.rs`
- Test: `src-tauri/src/event_journal/recovery.rs` (`#[cfg(test)] mod tests`, in-file)

**Interfaces:**
- Consumes: existing `EventJournal::append/commit/replay_from/events/standard_path/save_to/load_from` (`event_journal/mod.rs:128-203`), `EventKind` (:19), `JournalEvent` (:34), `CommitPoint` (:46).
- Produces (used by Tasks 2-4):
  - `EventJournal::session_id(&self) -> &str`
  - `EventJournal::commit_points(&self) -> &[CommitPoint]`
  - `recovery::RecoveryState { Clean, Interrupted { turn_start_event_id: String, sequence: u64 } }` (derive Debug, Clone, PartialEq, Eq)
  - `recovery::assess(journal: &EventJournal) -> RecoveryState`
  - `recovery::close_interrupted_turn(journal: &mut EventJournal, turn_start_event_id: &str)`

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/event_journal/recovery.rs` with only the test module first (implementation stubs come in Step 3 — write the full test module now against the planned API):

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib event_journal::recovery 2>&1 | tail -5`
Expected: FAIL — `recovery` module / `assess` / `close_interrupted_turn` / `commit_points()` unresolved.

- [ ] **Step 3: Implement accessors + recovery module**

In `src-tauri/src/event_journal/mod.rs`:

a) Register the module. Find the top-of-file module docs/`use` block and add (after any existing `mod tests;`-style declarations, e.g. near line 17):

```rust
pub mod recovery;
```

b) Add the two accessors immediately after `events()` (:173-177):

```rust
    /// Session this journal belongs to.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Recorded commit points, oldest first.
    pub fn commit_points(&self) -> &[CommitPoint] {
        &self.commit_points
    }
```

Create the implementation in `src-tauri/src/event_journal/recovery.rs` **above** the test module:

```rust
//! Crash-recovery assessment for the durable event journal (AC-1.10).
//!
//! Master design §10 conservative recovery: a turn whose TurnStart has no
//! paired TurnEnd is marked `unknown/interrupted`; nothing is auto-replayed.
//! This module is pure filesystem/journal logic — no Runtime dependency.

use super::{CommitPoint, EventJournal, EventKind};

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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib event_journal 2>&1 | tail -5`
Expected: PASS — recovery 5 tests + existing 11 event_journal tests (16 total).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/event_journal/mod.rs src-tauri/src/event_journal/recovery.rs
git commit -m "feat(event_journal): recovery assessor — replay_from's first production consumer"
```

---

### Task 2: recover_session_journal (load / quarantine / close / save / marker)

**Files:**
- Modify: `src-tauri/src/event_journal/recovery.rs` (append implementation + extend test module)
- Test: same file

**Interfaces:**
- Consumes: Task 1 `assess`, `close_interrupted_turn`, `RecoveryState`; `crate::store::{append_message, ChatMessageStored}` (`store.rs:970`, `store.rs:401-418`); `crate::paths::APP_HOME_ENV_LOCK` (test infra); `EventJournal::standard_path/save_to/load_from`.
- Produces (used by Task 4):
  - `recovery::RecoveryReport { Clean, Interrupted { turn_start_event_id: String, marker_message_id: String, content: String } }` (derive Debug, Clone, PartialEq, Eq)
  - `recovery::recover_session_journal(app_session_id: &str) -> Option<RecoveryReport>`
  - Marker content constant shape: `"turn_interrupted|crash_recovery"` (string literal, mirrored by Task 5 frontend parsing).

- [ ] **Step 1: Write the failing tests**

Append to the test module in `recovery.rs`:

```rust
    // --- recover_session_journal (filesystem) ---

    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Hold the module guard AND the shared app-home env lock for the whole
    /// test, and point OMP_DESKTOP_HOME at a fresh temp dir.
    fn test_home(
        tag: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        std::sync::MutexGuard<'static, ()>,
        std::path::PathBuf,
    ) {
        let module = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let env = crate::paths::APP_HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
```

Note: `crate::store::load_messages` is `pub fn load_messages(session_id: &str) -> Vec<ChatMessageStored>` (`store.rs:962`) — returns an empty Vec when no file exists, no Result.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib event_journal::recovery 2>&1 | tail -5`
Expected: FAIL — `RecoveryReport` / `recover_session_journal` unresolved.

- [ ] **Step 3: Implement recover_session_journal**

Append to `recovery.rs` (before the test module):

```rust
/// Recovery outcome with everything the call site needs for the live
/// `session://turn_marker` emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryReport {
    Clean,
    Interrupted {
        turn_start_event_id: String,
        marker_message_id: String,
        content: String,
    },
}

/// Marker content, pipe-format `marker|reason` — mirrored by the frontend
/// `parseEndOfTurnContent` (`src/lib/endOfTurn.ts`).
pub const INTERRUPTED_MARKER_CONTENT: &str = "turn_interrupted|crash_recovery";

/// Full recovery for one session directory. `None` when no journal exists.
/// Filesystem-only; emitting is the caller's job (Task 4).
pub fn recover_session_journal(app_session_id: &str) -> Option<RecoveryReport> {
    let path = EventJournal::standard_path(app_session_id);
    if !path.exists() {
        return None;
    }
    let mut journal = match EventJournal::load_from(&path) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(
                "event journal corrupt for session {app_session_id}: {e}; quarantining"
            );
            let quarantine = path.with_file_name(format!(
                "event_journal.corrupt-{}.json",
                chrono::Utc::now().timestamp()
            ));
            let _ = std::fs::rename(&path, &quarantine);
            // No marker: no evidence of an in-flight turn.
            return Some(RecoveryReport::Clean);
        }
    };
    match assess(&journal) {
        RecoveryState::Clean => Some(RecoveryReport::Clean),
        RecoveryState::Interrupted {
            turn_start_event_id, ..
        } => {
            close_interrupted_turn(&mut journal, &turn_start_event_id);
            if let Err(e) = journal.save_to(&path) {
                tracing::warn!("failed to persist closed journal for {app_session_id}: {e}");
            }
            let marker_message_id = uuid::Uuid::new_v4().to_string();
            let content = INTERRUPTED_MARKER_CONTENT.to_string();
            if let Err(e) = crate::store::append_message(
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
                tracing::warn!("failed to append turn_interrupted marker for {app_session_id}: {e}");
            }
            Some(RecoveryReport::Interrupted {
                turn_start_event_id,
                marker_message_id,
                content,
            })
        }
    }
}
```

Check imports at the top of `recovery.rs`: `chrono` and `uuid` are already crate dependencies (used in `session_manager.rs`); fully-qualified paths above avoid new `use` lines except what Task 1 added.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib event_journal 2>&1 | tail -5`
Expected: PASS — 9 recovery tests + 11 existing = 20 event_journal tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/event_journal/recovery.rs
git commit -m "feat(event_journal): recover_session_journal — load/close/marker + corrupt quarantine"
```

---

### Task 3: TurnStart write-ahead (append_turn_start_durable + session_manager cut 1)

**Files:**
- Modify: `src-tauri/src/event_journal/recovery.rs` (helper + 1 test)
- Modify: `src-tauri/src/session_manager.rs:4604-4607` (TurnStart append site)

**Interfaces:**
- Consumes: `EventJournal::append/save_to/standard_path/session_id` (Task 1 accessor).
- Produces: `recovery::append_turn_start_durable(journal: &mut EventJournal) -> String` (returns the event id; used by session_manager).

- [ ] **Step 1: Write the failing test**

Append to the recovery test module:

```rust
    #[test]
    fn append_turn_start_durable_persists_immediately() {
        let (_m, _e, _dir) = test_home("writeahead");
        let sid = "sess-wa";
        let mut j = EventJournal::new(sid.into());
        let id = append_turn_start_durable(&mut j);
        // File exists before any TurnEnd/commit — this is the crash window.
        let on_disk = EventJournal::load_from(&EventJournal::standard_path(sid)).unwrap();
        assert_eq!(on_disk.events().len(), 1);
        assert!(matches!(on_disk.events()[0].kind, EventKind::TurnStart));
        assert_eq!(on_disk.events()[0].id, id);
        // And a crash right now would be detected as Interrupted.
        assert!(matches!(assess(&on_disk), RecoveryState::Interrupted { .. }));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib event_journal::recovery::tests::append_turn_start_durable 2>&1 | tail -3`
Expected: FAIL — function unresolved.

- [ ] **Step 3: Implement helper + wire into session_manager**

Append to `recovery.rs`:

```rust
/// TurnStart write-ahead (spec D1): append and immediately persist, so a
/// crash mid-turn leaves a dangling TurnStart on disk for `assess` to find.
/// Best-effort: a save error logs and the turn proceeds — worst case we lose
/// crash-detection for this one turn, never messages.
pub fn append_turn_start_durable(journal: &mut EventJournal) -> String {
    let id = journal.append(EventKind::TurnStart, serde_json::json!({}));
    let path = EventJournal::standard_path(journal.session_id());
    if let Err(e) = journal.save_to(&path) {
        tracing::warn!("event journal write-ahead failed for {}: {e}", journal.session_id());
    }
    id
}
```

In `src-tauri/src/session_manager.rs:4604-4607` replace:

```rust
            if let Some(journal) = s.event_journal.as_mut() {
                journal.append(EventKind::TurnStart, serde_json::json!({}));
            }
```

with:

```rust
            if let Some(journal) = s.event_journal.as_mut() {
                crate::event_journal::recovery::append_turn_start_durable(journal);
            }
```

Also update the comment directly above it (:4603-4605) to:

```rust
            // Plan 3 Task 5 + AC-1.10: record TurnStart in the event journal
            // and write it ahead to disk, so a crash mid-turn leaves a
            // dangling TurnStart for recovery to mark unknown/interrupted.
            // Best-effort — a missing journal (mock / pre-spawn shell) skips.
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: PASS — full suite green (≈452 tests), including session_manager tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/event_journal/recovery.rs src-tauri/src/session_manager.rs
git commit -m "feat(session_manager): TurnStart write-ahead for crash detection (AC-1.10 D1)"
```

---

### Task 4: connect_inner wiring — recovery + journal continuity + live emit

**Files:**
- Modify: `src-tauri/src/session_manager.rs:2814-2816` (journal attach site, post-spawn success)

**Interfaces:**
- Consumes: `recovery::{recover_session_journal, RecoveryReport}` (Task 2), `EventJournal::load_from/standard_path`.
- Produces: no new API. Behavior: on connect, interrupted turns from a prior run are closed + marked, and the in-memory journal continues from disk.

- [ ] **Step 1: Locate the attach site and confirm emit handle**

The journal attach site is inside a locked block after spawn success (`session_manager.rs:2815`); `app` is in scope (used by `Self::emit_state(&app, &snap)` at :2821) and the `turn_cancelled` precedent emits while holding the guard (`session_manager.rs:3594`). Read :2790-2825 to confirm exact surrounding code before editing.

- [ ] **Step 2: Wire recovery + continuity**

Replace:

```rust
                        // Plan 3 Task 5: attach a fresh event journal for this
                        // session so turn boundaries are durable for replay.
                        s.event_journal = Some(EventJournal::new(s.app_session_id.clone()));
```

with:

```rust
                        // Plan 3 Task 5 + AC-1.10: recover any interrupted turn
                        // from the on-disk journal (close + marker, idempotent),
                        // then continue the persisted journal so event IDs and
                        // sequences stay stable across restarts (§5.2 baseline).
                        let recovery =
                            crate::event_journal::recovery::recover_session_journal(
                                &s.app_session_id,
                            );
                        if let Some(
                            crate::event_journal::recovery::RecoveryReport::Interrupted {
                                marker_message_id,
                                content,
                                ..
                            },
                        ) = &recovery
                        {
                            let _ = app.emit(
                                "session://turn_marker",
                                serde_json::json!({
                                    "sessionId": s.app_session_id,
                                    "messageId": marker_message_id,
                                    "marker": "turn_interrupted",
                                    "reason": "crash_recovery",
                                    "content": content,
                                }),
                            );
                        }
                        let journal_path =
                            EventJournal::standard_path(&s.app_session_id);
                        s.event_journal = Some(
                            EventJournal::load_from(&journal_path).unwrap_or_else(|_| {
                                EventJournal::new(s.app_session_id.clone())
                            }),
                        );
```

Note: recovery runs at the post-spawn-success attach site; if spawn fails, recovery (and the marker) defer to the next successful connect. This is documented in the spec §4.

- [ ] **Step 3: Verify compile + full suite**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: PASS (no new tests in this task — wiring is covered by Task 2/3 tests + compile).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/session_manager.rs
git commit -m "feat(session_manager): journal recovery + continuity on connect (AC-1.10)"
```

---

### Task 5: Frontend chip pipeline + i18n (turn_interrupted / crash_recovery)

**Files:**
- Modify: `src/lib/endOfTurn.ts` (type, messageKey union, new case, isEndOfTurnMarker, parseEndOfTurnContent)
- Modify: `src/lib/session.ts:661` (applyTurnMarker isError)
- Modify: `src/components/lobe-chat/ConversationThread.tsx:841-842` (content-prefix parity)
- Modify: `src/lib/endOfTurn.test.ts` (new cases)
- Modify: `src/i18n/messages.ts` (en ~:1458 area + zh-CN ~:3542 area), `src/i18n/zh-tw.ts` (~:1407 area)

**Interfaces:**
- Consumes: backend marker `"turn_interrupted"` + content `"turn_interrupted|crash_recovery"` (Task 2 constant `INTERRUPTED_MARKER_CONTENT`); existing `EndOfTurnChip` render path (`ConversationThread.tsx:838-846`).
- Produces: i18n key `endOfTurn.interrupted` (×3 locales); `EndOfTurnReason` gains `"interrupted"`.

- [ ] **Step 1: Write the failing tests**

Append to `src/lib/endOfTurn.test.ts` (follow the file's existing test style):

```ts
import { describe, expect, it } from "vitest";
import {
  isEndOfTurnMarker,
  mapEndOfTurnReason,
  parseEndOfTurnContent,
} from "./endOfTurn";

describe("turn_interrupted / crash_recovery", () => {
  it("maps crash_recovery to interrupted chip model", () => {
    const m = mapEndOfTurnReason("crash_recovery");
    expect(m.reason).toBe("interrupted");
    expect(m.messageKey).toBe("endOfTurn.interrupted");
    expect(m.tone).toBe("error");
  });

  it("parses turn_interrupted|crash_recovery content", () => {
    expect(parseEndOfTurnContent("turn_interrupted|crash_recovery")).toBe(
      "interrupted",
    );
  });

  it("parses bare turn_interrupted content", () => {
    expect(parseEndOfTurnContent("turn_interrupted")).toBe("interrupted");
  });

  it("treats turn_interrupted as an end-of-turn marker", () => {
    expect(isEndOfTurnMarker("turn_interrupted")).toBe(true);
  });
});
```

Run: `pnpm vitest run src/lib/endOfTurn.test.ts 2>&1 | tail -5`
Expected: FAIL — `endOfTurn.interrupted` not in the union / `interrupted` reason unknown.

- [ ] **Step 2: Implement endOfTurn.ts changes**

a) Extend the reason type (:6-14):

```ts
export type EndOfTurnReason =
  | "user_stop"
  | "agent_exit"
  | "stall"
  | "permission_denied"
  | "error"
  | "cancelled"
  | "interrupted"
  | "unknown";
```

b) Extend the messageKey union (:17-25) with:

```ts
    | "endOfTurn.interrupted"
```

c) Add the mapping case in `mapEndOfTurnReason`, immediately before the final `return { reason: "unknown", ... }`:

```ts
  if (
    r === "crash_recovery" ||
    r === "interrupted" ||
    r === "turn_interrupted"
  ) {
    return {
      reason: "interrupted",
      messageKey: "endOfTurn.interrupted",
      tone: "error",
    };
  }
```

d) `isEndOfTurnMarker` (:94-103) — add `m === "turn_interrupted"` to the return chain:

```ts
  return (
    m === "turn_cancelled" ||
    m === "turn_interrupted" ||
    m === "turn_end" ||
    m === "stream_stall" ||
    m === "end_of_turn"
  );
```

e) `parseEndOfTurnContent` (:110-122) — add before the final `return null`:

```ts
  if (content.startsWith("turn_interrupted|")) {
    return mapEndOfTurnReason(content.slice("turn_interrupted|".length)).reason;
  }
  if (content.startsWith("turn_interrupted")) {
    return "interrupted";
  }
```

- [ ] **Step 3: session.ts + ConversationThread.tsx parity**

In `src/lib/session.ts:661` (inside `applyTurnMarker`) replace:

```ts
      isError: marker === "turn_cancelled",
```

with:

```ts
      isError: marker === "turn_cancelled" || marker === "turn_interrupted",
```

In `src/components/lobe-chat/ConversationThread.tsx:838-844` replace the branch condition:

```tsx
            if (
              isEndOfTurnMarker(m.marker) ||
              m.marker === "turn_cancelled" ||
              (m.role === "tool" &&
                (m.content?.startsWith("turn_cancelled") ||
                  m.content?.startsWith("turn_end|")))
            ) {
```

with:

```tsx
            if (
              isEndOfTurnMarker(m.marker) ||
              m.marker === "turn_cancelled" ||
              (m.role === "tool" &&
                (m.content?.startsWith("turn_cancelled") ||
                  m.content?.startsWith("turn_interrupted") ||
                  m.content?.startsWith("turn_end|")))
            ) {
```

(`isEndOfTurnMarker` now covers the marker itself; the content-prefix branch is parity with the `turn_cancelled` legacy-row handling.)

- [ ] **Step 4: i18n keys (3 locales)**

Add `endOfTurn.interrupted` next to the existing `endOfTurn.error`/`endOfTurn.unknown` entries. Copy rule (§18.2.7): states unknown/interrupted, does NOT promise absolute no-duplication.

`src/i18n/messages.ts` en block (after `"endOfTurn.error": "Turn ended with an error",` ~:1458):

```ts
  "endOfTurn.interrupted":
    "Turn interrupted — final state unknown; boundary side effects may be unknown. Review, then start a new turn.",
```

`src/i18n/messages.ts` zh-CN block (after `"endOfTurn.error": "本轮以错误结束",` ~:3542):

```ts
  "endOfTurn.interrupted":
    "本轮已中断——最终状态未知；退出边界的副作用状态可能未知。请检查状态后发起新回合。",
```

`src/i18n/zh-tw.ts` (after `"endOfTurn.error": "本輪以錯誤結束",` ~:1407):

```ts
  "endOfTurn.interrupted":
    "本輪已中斷——最終狀態未知；退出邊界的副作用狀態可能未知。請檢查狀態後發起新回合。",
```

- [ ] **Step 5: Run frontend gates**

Run: `pnpm vitest run src/lib/endOfTurn.test.ts 2>&1 | tail -3 && pnpm test 2>&1 | tail -3 && pnpm typecheck 2>&1 | tail -2 && pnpm check:i18n 2>&1 | tail -2`
Expected: endOfTurn tests PASS; full vitest 831+ PASS; typecheck clean; `check:i18n: OK (3 locales, 1885 keys each)`.

- [ ] **Step 6: Commit**

```bash
git add src/lib/endOfTurn.ts src/lib/endOfTurn.test.ts src/lib/session.ts src/components/lobe-chat/ConversationThread.tsx src/i18n/messages.ts src/i18n/zh-tw.ts
git commit -m "feat(frontend): turn_interrupted chip + i18n (AC-1.10 D2)"
```

---

### Task 6: Acceptance matrix / coverage audit / memory updates + final gates

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (AC-1.10 :40, AC-7.1 :129, AC-7.3 :131, verdict counts, FAIL-items narrative if it lists AC-1.10)
- Modify: `docs/release/test-coverage-audit.md` (TC-M.7 row, cargo count, event_journal module count, gap table row)
- Modify: memory `omp-desktop-roadmap-status.md` + `MEMORY.md`

- [ ] **Step 1: Flip AC-1.10 to PASS**

Replace the AC-1.10 row (`1.0-acceptance-matrix.md:40`) Status/Evidence with:

```
| AC-1.10 | Event Replay / Recovery: stable event ID, replay cursor, journal commit, conservative recovery (no auto-replay) | Contract tests + crash recovery test | PASS | Recovery wired into session connect (2026-07-31, spec `docs/superpowers/specs/2026-07-31-event-journal-recovery-design.md`): TurnStart write-ahead leaves a durable dangling boundary on crash; `recover_session_journal` (load → assess via `replay_from` → close interrupted turn honestly → save) runs at connect; journal continues from disk so event IDs/sequences stay stable across restarts. Conservative recovery per §10: unfinished turns marked `unknown/interrupted` via idempotent `turn_interrupted` marker; zero auto-replay (journal carries turn boundaries only, never a re-injection source). 10 recovery tests incl. idempotency + corrupt quarantine. |
```

- [ ] **Step 2: Update AC-7.1 / AC-7.3 notes**

AC-7.1 (:129) stays BLOCKED; replace its Unblocker with: `**Unblocker:** real-sidecar crash injection test (detection + unknown/interrupted marking now implemented and unit-tested via AC-1.10 wiring; the remaining gap is killing a live Runtime process mid-turn in an automated harness).`

AC-7.3 (:131) evidence note: append `load_from/replay_from now have production call sites (connect-path recovery, AC-1.10 PASS).`

- [ ] **Step 3: Recompute verdict counts + update summary**

Run: `for v in PASS PARTIAL BLOCKED FAIL; do printf "%s: " "$v"; grep -oE "\| $v \|" docs/release/1.0-acceptance-matrix.md | wc -l | tr -d ' '; done`
Expected: PASS 34, PARTIAL 16, BLOCKED 102, FAIL 6. Update the counts table + add a follow-up paragraph to the Audit Summary (same pattern as the §8.2 follow-up). If the "Release-blocking FAIL items" list or "Top unblockers" mentions event-journal recovery, mark it resolved the same way as the v1-transport entry.

- [ ] **Step 4: test-coverage-audit.md**

- Rust suite count → current `cargo test --lib` total (≈453).
- Module breakdown: `event_journal` 11 → 21 (recovery 10); add recovery scope to the module's Scope column.
- TC-M.7 row: change Evidence to `event_journal (21, incl. 10 recovery tests: assess/close/idempotent marker/corrupt quarantine/write-ahead)` and Remaining-gap to `Manual crash injection only (AC-7.1)`.
- Gap table: mark the "Event-journal recovery wiring" row Resolved 2026-07-31 (same ~~strikethrough~~ pattern as the v1 transport row).

- [ ] **Step 5: Memory**

Update `omp-desktop-roadmap-status.md`: add § AC-1.10 completion bullet (commits, design decisions, test counts); update description + "How to apply" priority list (next highest-leverage: AC-8.4 remote approval expiry+anti-replay → AC-1.5 subagent policy → AC-1.13 trace correlation → mock/real-Runtime E2E). Update the `MEMORY.md` index line (6 FAIL → 6 FAIL with AC-1.10 removed from BLOCKED-priority wording; FAIL count stays 6 since AC-1.10 was BLOCKED, not FAIL — verify against matrix and phrase accordingly).

- [ ] **Step 6: Final gates + commit**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3 && cd .. && pnpm test 2>&1 | tail -3 && pnpm typecheck 2>&1 | tail -2 && pnpm check:i18n 2>&1 | tail -2 && pnpm check:brand 2>&1 | tail -2 && pnpm check:provenance 2>&1 | tail -2 && pnpm check:legal 2>&1 | tail -2`
Expected: all green.

```bash
git add docs/release/1.0-acceptance-matrix.md docs/release/test-coverage-audit.md
git commit -m "docs(release): flip AC-1.10 event-journal recovery to PASS"
```

(Memory files live outside the repo — no commit.)
