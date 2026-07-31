# Event-Journal Recovery Wiring — Design Spec

**Date:** 2026-07-31
**Status:** Approved (user defaults — questions unanswered, recommendations adopted)
**Acceptance items:** AC-1.10 (primary), AC-7.3 (evidence strengthening); AC-7.1 stays BLOCKED (manual crash injection, out of scope)
**Master design refs:** §10 (会话、事件与恢复), §5.2 (capability baseline: stable event ID / replay cursor / journal commit point), §18.2.7 (pass condition 7)

## 1. Problem

`EventJournal::replay_from` and `EventJournal::load_from` have **zero production call sites** (only tests + portability export/import). On reconnect/reattach/app-restart, `connect_inner` (`session_manager.rs:2815`) always creates a fresh `EventJournal::new`, discarding the on-disk journal written at the last turn-end commit. Consequences:

- No stable event ID / sequence continuity across restarts (violates §5.2 baseline intent).
- A crash mid-turn is indistinguishable from a clean exit between turns: the disk journal only ever reflects the last cleanly-committed turn, because `save_to` runs only on authoritative `PromptComplete` (`session_manager.rs:3164`).
- The §10 conservative-recovery rule — *mark unfinished turns `unknown/interrupted`; never auto-replay* — has no implementation: nothing detects the unfinished turn, nothing marks it.

## 2. Decisions (user default = recommendation, 2026-07-31)

| # | Question | Decision | Rationale |
|---|---|---|---|
| D1 | How to detect a mid-turn crash reliably? | **TurnStart write-ahead**: `save_to` immediately after appending `TurnStart` | Only way to distinguish "crashed mid-turn" from "clean exit between turns". Cost: one small JSON write per turn (turn-boundary frequency, no throttle needed). |
| D2 | How to present the interrupted turn? | **`messages.json` marker message** (`marker: "turn_interrupted"`), idempotent | Reuses the existing `ChatMessageStored.marker` pipeline (precedent: `turn_cancelled`). No new IPC/frontend state. Permanently visible in history; satisfies "user reviews state, then explicitly starts a new turn". |
| D3 | Include AC-7.1 automated crash injection (kill real sidecar)? | **No** | Process orchestration across platforms is CI-fragile and belongs to the cross-platform acceptance phase. Recovery detection is fully testable by constructing on-disk journal states. AC-7.1 stays BLOCKED with an updated note. |

## 3. Architecture

### 3.1 New module: `src-tauri/src/event_journal/recovery.rs`

Pure, filesystem-only, no Runtime dependency:

```rust
pub enum RecoveryState {
    Clean,
    Interrupted { turn_start_event_id: String, sequence: u64 },
}

/// Assess recovery state from a loaded journal.
/// Uses replay_from(last_commit) — the first production consumer of replay_from.
/// Interrupted iff the uncommitted tail contains a TurnStart not closed by a TurnEnd.
/// Journals with no commit points: scan the whole event list with the same rule.
pub fn assess(journal: &EventJournal) -> RecoveryState;

/// Close a dangling turn honestly: append TurnEnd { stopReason: "interrupted",
/// interruptedAfter: <turn_start_event_id> } + commit. Makes recovery idempotent —
/// the next assess() sees paired turn boundaries and returns Clean.
pub fn close_interrupted_turn(journal: &mut EventJournal, turn_start_event_id: &str);

/// Full recovery for one session directory. Returns Some(RecoveryState) when a
/// journal existed and was assessed; None when no journal file exists.
/// - load_from(standard_path) — Err (corrupt): log warn, quarantine the file to
///   `event_journal.corrupt-<unixts>.json`, return Some(Clean) and let the caller
///   start a fresh journal. No marker without evidence of an in-flight turn.
/// - Interrupted: close_interrupted_turn + save_to + append marker message
///   (idempotency comes from the journal being closed — not from marker dedup).
pub fn recover_session_journal(app_session_id: &str) -> Option<RecoveryState>;
```

`assess` rule, precisely: let `tail` = `commit_points.last().map(|cp| replay_from(cp))` flattened (empty when no commits). Walk `tail` (or all events when no commit points) maintaining a turn-depth counter on TurnStart/TurnEnd; `Interrupted` iff depth > 0 at the end, anchored at the last unmatched TurnStart. This also future-proofs against nested/multi-turn tails.

### 3.2 Wiring (three cuts in `session_manager.rs`)

1. **TurnStart write-ahead** (~line 4605): after `journal.append(EventKind::TurnStart, …)`, immediately `journal.save_to(&EventJournal::standard_path(&s.app_session_id))` (best-effort, log on error — same pattern as the existing TurnEnd save).
2. **Recovery on connect** (`connect_inner`, before line 2815): call `recover_session_journal(&app_session_id)`. The marker append (if any) happens inside, via the same store helper the session manager already uses for assistant/user rows.
3. **Journal continuity** (line 2815): replace `EventJournal::new(...)` with `EventJournal::load_from(&standard_path).unwrap_or_else(|_| EventJournal::new(...))`. After recovery, a loaded journal is always Clean (paired boundaries); sequences and stable IDs continue across restarts.

### 3.3 Marker message

Copies the existing `turn_cancelled` precedent (`session_manager.rs:3580-3603`) exactly:

```rust
let content = "turn_interrupted|crash_recovery".to_string();
store::append_message(&app_session_id, ChatMessageStored {
    id: Uuid::new_v4().to_string(),
    role: "tool".into(),
    content: content.clone(),
    thought: None,
    created_at: chrono::Utc::now(),
    is_error: true,
    attachments: None,
    marker: Some("turn_interrupted".into()),
});
// + emit "session://turn_marker" { sessionId, messageId, marker: "turn_interrupted",
//   reason: "crash_recovery", content } so an open UI updates live.
```

Frontend pipeline (already generic, three small additions):

1. `ConversationThread.tsx:838-844` — extend the EndOfTurnChip branch condition with `m.marker === "turn_interrupted"` / `m.content?.startsWith("turn_interrupted")`.
2. `src/lib/endOfTurn.ts` `mapEndOfTurnReason` — add a `crash_recovery` case → new i18n key `endOfTurn.interrupted`, tone `error`. (`parseEndOfTurnContent` already handles the `marker|reason` pipe format; without the case it falls through to `endOfTurn.unknown`.)
3. `src/lib/sessionMessageNodes.ts:59` and `src/lib/session.ts:649-661` — verify the `turn_marker` event handler + node filter treat `turn_interrupted` like `turn_cancelled` (payload.marker is already forwarded; `isError` mapping may include it).

i18n key `endOfTurn.interrupted` (en / zh-CN / zh-TW), e.g.:
> "Turn interrupted — the app or runtime exited; final state unknown. Boundary side effects may be unknown. Review, then start a new turn."

Per §18.2.7 the copy must mark `unknown/interrupted` and must **not** promise absolute no-duplication.

## 4. Error handling

| Case | Behavior |
|---|---|
| No journal file | `recover_session_journal` → `None`; connect proceeds with fresh journal (current behavior). |
| Journal corrupt / unreadable | Warn + quarantine to `event_journal.corrupt-<unixts>.json`; fresh journal; **no marker** (no evidence of in-flight turn). |
| `save_to` fails on write-ahead | Log warn; turn proceeds (in-memory journal still correct; worst case we lose crash-detection for this turn, never lose messages). |
| Marker append fails | Log warn; journal is already closed + saved, so recovery stays idempotent; marker loss is cosmetic. |
| Legacy sessions (pre-feature, crashed mid-turn) | Disk journal ends at last commit → `Clean`, no marker. Retroactive detection is impossible; documented limitation. |
| Graceful quit mid-turn | TurnStart was persisted → next connect marks `Interrupted`. Correct: the turn genuinely did not finish. |

## 5. Non-goals (YAGNI)

- AC-7.1 automated real-sidecar crash injection (stays BLOCKED; note updated).
- Replaying/re-injecting prompt/tool/edit/shell events — forbidden by §10; the journal carries only turn boundaries and is never a re-injection source.
- Persisting MessageStart/ToolCall*/Usage/Compact events into the journal (not needed for recovery; separate work if ever).
- `JournalCommit` event variant appends (commit points already tracked in `commit_points`).
- AC-12.6 recovery-boundary guide doc (separate doc task, now unblocked).

## 6. Testing

`event_journal/recovery.rs` (new test module; all filesystem tests hold `APP_HOME_ENV_LOCK` + `OMP_DESKTOP_HOME` temp override, per the credential-migration test-infrastructure rule):

1. `assess_clean_when_tail_empty` — journal with committed TurnEnd → Clean.
2. `assess_interrupted_on_dangling_turn_start` — TurnStart after last commit → Interrupted with that event id.
3. `assess_interrupted_without_commit_points` — journal with only a TurnStart, no commits → Interrupted.
4. `close_interrupted_turn_makes_assess_clean` — close → assess → Clean; journal reloadable.
5. `recover_marks_interrupted_idempotently` — seed session dir with dangling-TurnStart journal; run `recover_session_journal` twice: first run → Interrupted + marker in messages.json + journal closed on disk; second run → Clean, no second marker.
6. `recover_corrupt_journal_quarantines_without_marker` — garbage bytes in event_journal.json → quarantined file exists, fresh start, no marker.
7. `recover_missing_journal_returns_none`.
8. Write-ahead: append TurnStart via the same code path shape → file exists on disk before TurnEnd.

Frontend (`src/lib/endOfTurn.test.ts`, extends existing cases):

9. `mapEndOfTurnReason("crash_recovery")` → `endOfTurn.interrupted` + error tone; `parseEndOfTurnContent("turn_interrupted|crash_recovery")` → reason `crash_recovery`.

Existing suites must stay green: event_journal 11, session_manager-related, portability 3 (they hold the env lock), full `cargo test --lib` (443 → ~452), `pnpm test`, typecheck, check:i18n (1884+1 keys ×3), check:brand, check:provenance, check:legal.

## 7. Acceptance-matrix updates (final task)

- AC-1.10: BLOCKED → **PASS** (recovery wired: load/replay_from/close/marker + write-ahead; test evidence).
- AC-7.3: evidence note updated (load_from now has production call sites).
- AC-7.1: stays BLOCKED; unblocker note updated (detection + marking now implemented and unit-tested; remaining gap is real-sidecar crash injection).
- TC-M.7 row in test-coverage-audit.md: recovery detection now automated; remaining manual = real crash injection.
- Verdict counts recomputed via grep; memory (`omp-desktop-roadmap-status`) updated: remaining FAILs after this = 5.

## 8. Commit plan

One branch, per-task commits (TDD): recovery module → write-ahead → connect wiring + marker → i18n → matrix/memory.
