# Recovery boundary

What OMP Desktop guarantees after a crash — and what it explicitly does not.

> **Current status (2026-07-31):** conservative recovery is implemented
> (AC-1.10 / AC-7.1). Desktop **never auto-replays** a prompt, tool call,
> edit, or shell command after a crash. Interrupted turns are honestly closed
> and marked; corrupt journals are quarantined, not repaired.

## 1. Source of truth

The **OMP session** (Runtime-side) is the source of truth. The Desktop event
journal stores UI projections — drafts, bindings, audit index — only. UI
history is **never re-injected as a prompt** (design §10).

## 2. Crash mid-turn

1. Every turn starts with a **write-ahead** `TurnStart`
   (`append_turn_start_durable`) — if the app dies mid-turn, the journal
   proves the turn was open.
2. Next launch, `assess` replays from the last commit and checks turn depth →
   `RecoveryState::Interrupted`.
3. Recovery closes the turn honestly: appends
   `TurnEnd{stopReason:"interrupted"}` (idempotent — a second assess returns
   `Clean`) plus a marker message (`turn_interrupted|crash_recovery`).
4. The UI renders an **interrupted** chip (`endOfTurn.interrupted`, error
   tone) at the end of the affected history.
5. You decide what to do next — Desktop does not resume or replay anything on
   its own.

## 3. Corrupt journal

An unparseable journal is renamed to `event_journal.corrupt-<timestamp>.json`
and a fresh journal starts; the state reports `Clean` and **no marker is
fabricated** — recovery acts only on evidence.

## 4. Guarantees vs non-guarantees

| Guaranteed | Not guaranteed |
|---|---|
| Desktop never actively replays a prompt/tool/edit/shell after a crash | Runtime/tool **side effects** across the crash boundary (a tool may have run partially) — unknown, marked `unknown/interrupted` |
| Interrupted turns are closed + marked, idempotently | **No absolute no-duplication promise** (§18.2.7) — an in-flight Runtime action may or may not have completed |
| Corrupt journals are preserved (quarantined) for inspection | Cross-restart `epoch+sequence` event dedup — **roadmap**, needs stable event ids + replay cursors from the Runtime |

## 5. What to do as a user

After the interrupted chip: start a new turn explicitly. If the turn had side
effects (file edits, shell), check the workspace before re-issuing the
instruction — the model will see the current file state, not a replay.

## 6. File index

| Area | File |
|---|---|
| Recovery assess/close | `src-tauri/src/event_journal/recovery.rs` |
| Write-ahead TurnStart | `src-tauri/src/event_journal/` |
| Session wiring | `src-tauri/src/session_manager.rs` |
| UI end-of-turn reason | `src/lib/endOfTurn.ts` + i18n key `endOfTurn.interrupted` |
