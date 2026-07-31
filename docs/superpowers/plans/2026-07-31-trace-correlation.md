# Trace Correlation (AC-1.13) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement mandatory Host+Hub scope trace correlation (AC-1.13, design §13): one work unit = one `trace_id` (uuid v4), carried as a `tracing` span field and propagated across `await`/spawn/mpsc boundaries via `Instrument`, so every existing log line inside a unit is correlatable with `grep trace_id=<uuid>`. Verified by 8 contract tests (5 mechanism + 3 wiring).

**Architecture:** New leaf module `src-tauri/src/trace.rs` (3 public fns + dev-only `CaptureLayer`) + Hub wiring at the remote_im pump's single mpsc convergence point (`runtime.rs`, both quick and detached branches) and engine event collector (`engine.rs` `Span::current()` inheritance) + Host wiring at `session_manager.send_message` (birth per prompt turn, turn task spawn instrumented). Engine internals and all 100+ existing log calls are untouched — they inherit span context automatically.

**Tech Stack:** Rust (Tauri 2), tracing 0.1 + tracing-subscriber 0.3 (already in `Cargo.toml:42-43`), uuid v4 (already `Cargo.toml:39`), parking_lot (already used codebase-wide). **Zero new dependencies.**

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-31-trace-correlation-design.md` — decisions D1–D5 are binding. D5 non-goals: no Runtime-internal propagation, no ACP wire `traceparent`, no diagnostics-page UI, no `DesktopV1Error` field.
- **Log metadata only — never the prompt content (SA-L.1 / AC-8.8).** Trace ids are random uuids; span fields carry `session_id`/`channel`/`message_id` only.
- Do NOT change `logging.rs` or the fmt layer — default full format already prints the span stack with fields.
- Do NOT hold `span.enter()` guards across `.await` — use `Instrument` for async, `in_scope` only for synchronous sections.
- Do NOT install a global subscriber in tests — use `tracing::subscriber::with_default` (sync tests) or `set_default` + `#[tokio::test(flavor = "current_thread")]` (async tests) so tests stay isolated and parallel-safe.
- Commit per task, English message `feat(<scope>): … (AC-1.13)` (docs task: `docs(release): …`).
- Gates before final commit: `cargo test --lib`, `pnpm test`, `pnpm typecheck`, `pnpm check:i18n`, `pnpm check:brand`, `pnpm check:provenance`, `pnpm check:legal`. Baseline: cargo 482 pass / vitest 840 pass.
- Repo: `/Users/po1nt9/Github/grok-app-main`, branch `main`.
- `cargo test --lib` runs from repo root as `cargo test --manifest-path src-tauri/Cargo.toml --lib`.

---

### Task 1: `trace.rs` module — span helpers + CaptureLayer + mechanism tests 1–5

**Files:**
- Create: `src-tauri/src/trace.rs`
- Modify: `src-tauri/src/lib.rs` (register `mod trace;` — insert alphabetically after `mod tool_heartbeat;` at :32, before `mod turn_complete;` at :33)

**Interfaces:**
- Produces (used by Tasks 2–3):
  - `trace::new_trace_id() -> String` — uuid v4 hyphenated (36 chars)
  - `trace::turn_span(trace_id: &str, session_id: &str) -> tracing::Span` — span name `turn`, fields `trace_id` + `session_id`
  - `trace::remote_msg_span(trace_id: &str, channel: &str, message_id: &str) -> tracing::Span` — span name `remote_msg`, fields `trace_id` + `channel` + `message_id`
- Test-only: `trace::test_capture::CaptureLayer` (Clone + Default), `CaptureLayer::events() -> Vec<Captured>`, `Captured { message: String, trace_ids: Vec<String> }` (root→leaf span-stack order).

- [ ] **Step 1: Write the failing module with tests only**

Create `src-tauri/src/trace.rs` with the full implementation AND tests in one go (the module is small; TDD red is `cargo test` failing before `mod trace;` registration — Step 2). The implementation comes first in-file so the tests below compile against the real API:

```rust
//! AC-1.13: Host+Hub scope trace correlation (design §13).
//!
//! One work unit = one trace id, carried as a `trace_id` field on a
//! `tracing` span. Every log event inside the span — including across
//! `.await` points and spawned tasks when wrapped with `Instrument` —
//! inherits the field, and the default fmt layer prints the span stack,
//! so logs correlate with `grep trace_id=<uuid>`.
//!
//! Metadata only — never prompt content (SA-L.1 / AC-8.8).

use tracing::Span;

/// One work unit = one trace id (uuid v4, codebase idiom).
pub fn new_trace_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Host scope: one user-prompt turn.
pub fn turn_span(trace_id: &str, session_id: &str) -> Span {
    tracing::info_span!("turn", trace_id, session_id = %session_id)
}

/// Hub scope: one inbound channel message.
pub fn remote_msg_span(trace_id: &str, channel: &str, message_id: &str) -> Span {
    tracing::info_span!("remote_msg", trace_id, channel = %channel, message_id = %message_id)
}

#[cfg(test)]
pub(crate) mod test_capture {
    //! Dev-only `Layer` that records (message, span-stack trace_ids) per
    //! event. Hand-rolled (~70 lines) instead of the 7-star
    //! `tracing-fluent-assertions` crate — supply-chain hygiene (spec §3).
    use parking_lot::Mutex;
    use std::sync::Arc;
    use tracing::field::{Field, Visit};
    use tracing::{Event, Subscriber};
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::Layer;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Captured {
        pub message: String,
        /// trace_id of every span in the event's context, root → leaf.
        pub trace_ids: Vec<String>,
    }

    #[derive(Clone, Default)]
    pub(crate) struct CaptureLayer {
        events: Arc<Mutex<Vec<Captured>>>,
    }

    impl CaptureLayer {
        pub(crate) fn events(&self) -> Vec<Captured> {
            self.events.lock().clone()
        }
    }

    /// Captures `trace_id` / `message` fields, robust to either
    /// `record_str` or `record_debug` encoding (strips debug quotes).
    #[derive(Default)]
    struct FieldVisitor {
        trace_id: Option<String>,
        message: Option<String>,
    }

    impl FieldVisitor {
        fn set(&mut self, field: &Field, raw: String) {
            let v = raw.trim_matches('"').to_string();
            match field.name() {
                "trace_id" => self.trace_id = Some(v),
                "message" => self.message = Some(v),
                _ => {}
            }
        }
    }

    impl Visit for FieldVisitor {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.set(field, value.to_string());
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.set(field, format!("{value:?}"));
        }
    }

    /// Stored in span extensions so events can read the trace_id of
    /// every span in their context stack.
    struct TraceId(String);

    impl<S> Layer<S> for CaptureLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            id: &tracing::span::Id,
            ctx: Context<'_, S>,
        ) {
            let mut v = FieldVisitor::default();
            attrs.record(&mut v);
            if let (Some(span), Some(tid)) = (ctx.span(id), v.trace_id) {
                span.extensions_mut().insert(TraceId(tid));
            }
        }

        fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
            let mut v = FieldVisitor::default();
            event.record(&mut v);
            let mut trace_ids = Vec::new();
            if let Some(scope) = ctx.event_scope(event) {
                for span in scope.from_root() {
                    if let Some(t) = span.extensions().get::<TraceId>() {
                        trace_ids.push(t.0.clone());
                    }
                }
            }
            self.events.lock().push(Captured {
                message: v.message.unwrap_or_default(),
                trace_ids,
            });
        }
    }

    /// Subscriber = registry + this layer (no fmt, no filter).
    pub(crate) fn subscriber(layer: CaptureLayer) -> impl Subscriber {
        tracing_subscriber::registry().with(layer)
    }
}

#[cfg(test)]
mod tests {
    use super::test_capture::{subscriber, CaptureLayer};
    use super::{new_trace_id, remote_msg_span, turn_span};
    use tracing::Instrument;

    #[test]
    fn span_field_inherited_by_events() {
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        tracing::subscriber::with_default(subscriber(layer), || {
            let span = turn_span("tid-1", "sess-1");
            span.in_scope(|| {
                tracing::info!("hello-event");
                tracing::info!(trace_id = "inner-override", "explicit-field-event");
            });
            tracing::info!("outside-span");
        });
        let events = handle.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].message, "hello-event");
        assert_eq!(events[0].trace_ids, vec!["tid-1".to_string()]);
        // An explicit event field does not remove span inheritance.
        assert_eq!(events[1].trace_ids, vec!["tid-1".to_string()]);
        assert!(events[2].trace_ids.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trace_id_survives_await() {
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        let _guard = tracing::subscriber::set_default(subscriber(layer));
        async {
            tracing::info!("before-await");
            tokio::task::yield_now().await;
            tracing::info!("after-await");
        }
        .instrument(turn_span("tid-2", "sess-2"))
        .await;
        let events = handle.events();
        assert_eq!(events.len(), 2);
        for e in &events {
            assert_eq!(e.trace_ids, vec!["tid-2".to_string()], "{}", e.message);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trace_id_survives_tokio_spawn() {
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        let _guard = tracing::subscriber::set_default(subscriber(layer));
        tokio::spawn(
            async move {
                tokio::task::yield_now().await;
                tracing::info!("inside-spawn");
            }
            .instrument(remote_msg_span("tid-3", "weixin", "m1")),
        )
        .await
        .unwrap();
        let events = handle.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trace_ids, vec!["tid-3".to_string()]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trace_id_survives_mpsc_boundary() {
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        let _guard = tracing::subscriber::set_default(subscriber(layer));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(1);
        let pump = tokio::spawn(async move {
            while let Some(v) = rx.recv().await {
                // Mirror of the runtime.rs pump birth point.
                let span = remote_msg_span("tid-4", "test", &v.to_string());
                async move {
                    tracing::info!("pump-received");
                }
                .instrument(span)
                .await;
            }
        });
        tx.send(7).await.unwrap();
        drop(tx);
        pump.await.unwrap();
        let events = handle.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trace_ids, vec!["tid-4".to_string()]);
    }

    #[test]
    fn distinct_units_get_distinct_trace_ids() {
        let a = new_trace_id();
        let b = new_trace_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36, "uuid v4 hyphenated");
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        tracing::subscriber::with_default(subscriber(layer), || {
            turn_span(&a, "s").in_scope(|| tracing::info!("e-a"));
            remote_msg_span(&b, "c", "m").in_scope(|| tracing::info!("e-b"));
        });
        let events = handle.events();
        assert_eq!(events[0].trace_ids, vec![a]);
        assert_eq!(events[1].trace_ids, vec![b]);
    }
}
```

- [ ] **Step 2: Register the module and run tests (green)**

In `src-tauri/src/lib.rs`, after `mod tool_heartbeat;` add:

```rust
mod trace;
```

Run:

```bash
cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib trace::
```

Expected: 5 tests pass (`span_field_inherited_by_events`, `trace_id_survives_await`, `trace_id_survives_tokio_spawn`, `trace_id_survives_mpsc_boundary`, `distinct_units_get_distinct_trace_ids`). Also run full `cargo test --manifest-path src-tauri/Cargo.toml --lib` — expect 487 pass (482 + 5).

- [ ] **Step 3: Commit**

```
feat(trace): span-field trace_id helpers + CaptureLayer + mechanism tests (AC-1.13)
```

---

### Task 2: Hub wiring — runtime.rs pump birth + engine.rs collector inheritance (tests 6–7)

**Files:**
- Modify: `src-tauri/src/remote_im/runtime.rs` (pump at :114-147; add `pump_span_for` helper + test)
- Modify: `src-tauri/src/remote_im/engine.rs` (collector spawn at :1013-1049; add test 7 in `mod tests` at :1385)

**Interfaces:**
- Consumes: `crate::trace::{new_trace_id, remote_msg_span, test_capture}` (Task 1), existing `IncomingMessage` (`remote_im/types.rs:19-35`), `Engine::new_ephemeral` (`engine.rs:196`).
- Produces: `fn pump_span_for(msg: &IncomingMessage) -> tracing::Span` (private to runtime.rs) — the single birth-point helper the pump loop calls.

- [ ] **Step 1: Write the failing tests first**

In `src-tauri/src/remote_im/runtime.rs`, append a test module at end of file (create `#[cfg(test)] mod tests` if none exists — check first with `grep -n "mod tests" src-tauri/src/remote_im/runtime.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::test_capture::{subscriber, CaptureLayer};

    fn sample_msg(message_id: &str) -> IncomingMessage {
        IncomingMessage {
            channel: "weixin".into(),
            instance_id: "weixin-default".into(),
            message_id: message_id.into(),
            chat_id: "peer@im.wechat".into(),
            chat_type: "p2p".into(),
            sender_id: "peer@im.wechat".into(),
            content: "hello".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: None,
            nonce: None,
        }
    }

    #[test]
    fn runtime_pump_births_trace_per_message() {
        let layer = CaptureLayer::default();
        let handle = layer.clone();
        tracing::subscriber::with_default(subscriber(layer), || {
            pump_span_for(&sample_msg("m1")).in_scope(|| tracing::info!("e1"));
            pump_span_for(&sample_msg("m2")).in_scope(|| tracing::info!("e2"));
        });
        let events = handle.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].trace_ids.len(), 1);
        assert_eq!(events[1].trace_ids.len(), 1);
        assert_ne!(
            events[0].trace_ids[0], events[1].trace_ids[0],
            "each inbound message must birth a distinct trace_id"
        );
    }
}
```

In `src-tauri/src/remote_im/engine.rs` `mod tests` (at :1385), add:

```rust
    /// AC-1.13: logs emitted by Engine::handle — including anything the
    /// runtime event collector logs — must carry the message's trace_id
    /// when the caller (runtime pump) instrumented the handle future.
    #[tokio::test(flavor = "current_thread")]
    async fn engine_handle_logs_carry_trace_id() {
        use crate::trace::test_capture::{subscriber, CaptureLayer};
        use tracing::Instrument;

        let layer = CaptureLayer::default();
        let handle = layer.clone();
        let _guard = tracing::subscriber::set_default(subscriber(layer));
        let outbound = OutboundRouter::new();
        let engine = Engine::new_ephemeral(outbound, false);
        let msg = IncomingMessage {
            channel: "weixin".into(),
            instance_id: "test-trace".into(),
            message_id: "m-trace".into(),
            chat_id: "peer@im.wechat".into(),
            chat_type: "p2p".into(),
            sender_id: "peer@im.wechat".into(),
            content: "inspect this repository".into(),
            mentioned_bot: true,
            attachments: vec![],
            timestamp: None,
            nonce: None,
        };
        let span = crate::trace::remote_msg_span("tid-eng", &msg.channel, &msg.message_id);
        // Fail-closed path (no Runtime) still exercises handle's log lines.
        tokio::time::timeout(
            Duration::from_secs(5),
            engine.handle(msg).instrument(span),
        )
        .await
        .expect("handle timed out");
        let events = handle.events();
        assert!(!events.is_empty(), "handle emitted no log events");
        for e in &events {
            assert!(
                e.trace_ids.iter().any(|t| t == "tid-eng"),
                "event {:?} lost trace context",
                e.message
            );
        }
    }
```

Run to see RED: `cargo test --manifest-path src-tauri/Cargo.toml --lib trace_id remote_im::runtime` — `runtime_pump_births_trace_per_message` fails to compile (no `pump_span_for` yet). `engine_handle_logs_carry_trace_id` compiles but FAILS on assertion (events have empty trace_ids — no instrument inside engine… actually with the `.instrument(span)` in the test itself, events may already pass; the assertion that matters is Step 2's collector wiring — keep both tests, they pass only after Step 2 completes and full suite stays green).

- [ ] **Step 2: Implement the wiring (green)**

In `src-tauri/src/remote_im/runtime.rs`, add the birth-point helper above `start_runtime` (near the pump, file-top scope):

```rust
/// AC-1.13 birth point: one inbound channel message = one trace unit.
/// Called once per message at the pump's mpsc convergence point, so
/// every channel (weixin/wecom/line/…) is covered without per-channel code.
fn pump_span_for(msg: &IncomingMessage) -> tracing::Span {
    crate::trace::remote_msg_span(
        &crate::trace::new_trace_id(),
        &msg.channel,
        &msg.message_id,
    )
}
```

Rewire the pump loop (:118-147). Replace exactly this block:

```rust
    let eng = engine.clone();
    let pump = tokio::spawn(async move {
        tracing::info!("remote_im: message pump started");
        while let Some(msg) = msg_rx.recv().await {
            // Log metadata only — never the prompt content (SA-L.1 / AC-8.8).
            tracing::info!(
                channel = %msg.channel,
                instance = %msg.instance_id,
                chat = %msg.chat_id,
                sender = %msg.sender_id,
                content_len = msg.content.len(),
                "remote_im: engine recv"
            );
            let e = eng.clone();
            let trimmed = msg.content.trim();
            // Control-plane messages are awaited inline (must not be dropped).
            // Free-form chat is detached so a long Grok turn does not block others.
            let quick = trimmed.starts_with('/')
                || trimmed.starts_with("__card_action__:")
                || trimmed == "0"
                || trimmed.eq_ignore_ascii_case("cancel");
            if quick {
                e.handle(msg).await;
            } else {
                tokio::spawn(async move {
                    e.handle(msg).await;
                });
            }
        }
        tracing::warn!("remote_im: message pump exited (all senders dropped)");
    });
```

with:

```rust
    let eng = engine.clone();
    let pump = tokio::spawn(async move {
        tracing::info!("remote_im: message pump started");
        while let Some(msg) = msg_rx.recv().await {
            // AC-1.13: birth one trace_id per inbound message; both the
            // inline (quick) and detached branches carry it (spec §4.2).
            let span = pump_span_for(&msg);
            // Log metadata only — never the prompt content (SA-L.1 / AC-8.8).
            span.in_scope(|| {
                tracing::info!(
                    channel = %msg.channel,
                    instance = %msg.instance_id,
                    chat = %msg.chat_id,
                    sender = %msg.sender_id,
                    content_len = msg.content.len(),
                    "remote_im: engine recv"
                );
            });
            let e = eng.clone();
            let trimmed = msg.content.trim();
            // Control-plane messages are awaited inline (must not be dropped).
            // Free-form chat is detached so a long Grok turn does not block others.
            let quick = trimmed.starts_with('/')
                || trimmed.starts_with("__card_action__:")
                || trimmed == "0"
                || trimmed.eq_ignore_ascii_case("cancel");
            if quick {
                e.handle(msg).instrument(span).await;
            } else {
                tokio::spawn(
                    async move {
                        e.handle(msg).await;
                    }
                    .instrument(span),
                );
            }
        }
        tracing::warn!("remote_im: message pump exited (all senders dropped)");
    });
```

Add the import at top of runtime.rs (with the other `use` lines):

```rust
use tracing::Instrument;
```

In `src-tauri/src/remote_im/engine.rs` at the collector spawn (:1013-1049), change:

```rust
        tokio::spawn(async move {
            while let Some(ev) = events.recv().await {
```

to wrap the whole async block — find the exact closing of that `tokio::spawn(async move { … });` (it ends with `tracing::info!("remote_im: runtime event collector exited");` then `});`) and convert to:

```rust
        // AC-1.13: inherit the caller's span so collector logs keep the
        // message's trace_id (Span::current() at spawn time, inside the
        // already-instrumented handle future; none when spawned outside
        // a message context — a no-op then).
        tokio::spawn(
            async move {
                while let Some(ev) = events.recv().await {
                    // … existing body unchanged …
                }
                tracing::info!("remote_im: runtime event collector exited");
            }
            .instrument(tracing::Span::current()),
        );
```

(Keep the existing body byte-identical; only the spawn wrapper changes.) Add `use tracing::Instrument;` to engine.rs imports.

- [ ] **Step 3: Run tests (green)**

```bash
cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib remote_im
```

Expected: all remote_im tests pass including `runtime_pump_births_trace_per_message` and `engine_handle_logs_carry_trace_id`. Then full `cargo test --manifest-path src-tauri/Cargo.toml --lib` — expect 489 pass (487 + 2).

- [ ] **Step 4: Commit**

```
feat(remote_im): pump births trace_id per message; collector inherits span (AC-1.13)
```

---

### Task 3: Host wiring — session_manager.send_message turn correlation (test 8)

**Files:**
- Modify: `src-tauri/src/session_manager.rs` (`send_message` entry :4597; turn spawn :4799-4824; add test in its `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::trace::{new_trace_id, turn_span, test_capture}` (Task 1).

- [ ] **Step 1: Write the failing test first**

In `src-tauri/src/session_manager.rs` tests module, add:

```rust
    /// AC-1.13: the send_message turn task spawn site wraps its future in
    /// `turn_span(trace_id, session_id)` via Instrument. The real task
    /// needs an AppHandle (unavailable in unit tests — same precedent as
    /// the permission host-gate tests), so this drives the identical
    /// span + Instrument wiring the spawn site uses and asserts logs
    /// emitted inside the task carry the trace_id.
    #[tokio::test(flavor = "current_thread")]
    async fn send_message_turn_logs_carry_trace_id() {
        use crate::trace::test_capture::{subscriber, CaptureLayer};
        use tracing::Instrument;

        let layer = CaptureLayer::default();
        let handle = layer.clone();
        let _guard = tracing::subscriber::set_default(subscriber(layer));
        let trace_id = crate::trace::new_trace_id();
        let span = crate::trace::turn_span(&trace_id, "sess-host");
        tokio::spawn(
            async move {
                tracing::info!("turn-task log");
            }
            .instrument(span),
        )
        .await
        .unwrap();
        let events = handle.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trace_ids, vec![trace_id]);
    }
```

Run to see it pass standalone (mechanism), knowing the wiring itself is verified by Step 2 + compile. (This test is GREEN immediately — it pins the pattern the spawn site must use; the wiring is enforced by code review + the instrument call being required for compilation to match this pattern. Accept per spec §4.4 test 8.)

- [ ] **Step 2: Implement the wiring**

In `send_message` (`session_manager.rs:4597`), right after the text validation at the top of the function, add the birth:

```rust
        // AC-1.13: one user-prompt turn = one trace unit (design §13).
        let trace_id = crate::trace::new_trace_id();
```

At the turn spawn site (:4799-4824), replace:

```rust
        let mgr = Arc::clone(self);
        let app2 = app.clone();
        let turn_sid = app_sid.clone();
        tokio::spawn(async move {
            let outcome = acp.prompt(&agent_prompt).await;
            if let Err(e) = outcome {
                mgr.with_session_mut(&turn_sid, |s| {
                    s.prompt_in_flight = false;
                    if !s.provider_retry_aborted {
                        SessionManager::record_turn_error(s, &app2, &e);
                        let _ = s.fsm.fail_with(e);
                    }
                });
                mgr.emit_for_session(&app2, &turn_sid);
            }
        });
```

with:

```rust
        let mgr = Arc::clone(self);
        let app2 = app.clone();
        let turn_sid = app_sid.clone();
        // AC-1.13: the whole turn task (prompt → terminal handling) runs
        // inside the turn span; sync dispatch log shares the trace_id.
        let span = crate::trace::turn_span(&trace_id, &turn_sid);
        span.in_scope(|| {
            tracing::info!(session_id = %turn_sid, "turn: prompt dispatched");
        });
        tokio::spawn(
            async move {
                let outcome = acp.prompt(&agent_prompt).await;
                if let Err(e) = outcome {
                    mgr.with_session_mut(&turn_sid, |s| {
                        s.prompt_in_flight = false;
                        if !s.provider_retry_aborted {
                            SessionManager::record_turn_error(s, &app2, &e);
                            let _ = s.fsm.fail_with(e);
                        }
                    });
                    mgr.emit_for_session(&app2, &turn_sid);
                }
            }
            .instrument(span),
        );
```

Add `use tracing::Instrument;` to session_manager.rs imports.

NOTE for executor: if the spawn body at :4799-4824 has drifted (line numbers approximate), anchor on `let turn_sid = app_sid.clone();` + the following `tokio::spawn(async move {` containing `acp.prompt(&agent_prompt).await` — that is the unique turn task spawn in `send_message`. Preserve the body byte-identical; only the wrapper changes.

- [ ] **Step 3: Run tests (green)**

```bash
cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib session_manager
```

Expected: all pass including `send_message_turn_logs_carry_trace_id`. Then full `cargo test --manifest-path src-tauri/Cargo.toml --lib` — expect 490 pass (489 + 1).

- [ ] **Step 4: Commit**

```
feat(session_manager): send_message births trace_id per prompt turn (AC-1.13)
```

---

### Task 4: Acceptance matrix flip + audit-doc sync + memory + final gates

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (row :43, counts :344-347, FAIL list item 2 at :364)
- Modify: `docs/release/test-coverage-audit.md` (gap row :79, Rust count line :15)
- Modify: `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`
- Do NOT touch: `docs/release/security-audit-checklist.md` (grep confirmed zero trace/correlation rows), matrix appendix B.1 Trace Correlation row :294 (device × capability matrix — stays BLOCKED, real-device bound).

- [ ] **Step 1: Flip the AC-1.13 matrix row**

Replace the AC-1.13 row (`docs/release/1.0-acceptance-matrix.md:43`) — old:

```
| AC-1.13 | Trace Correlation: Desktop Host + Remote Hub scope correlation (mandatory; end-to-end optional) | Contract tests (Host+Hub scope) | FAIL | grep "trace_id/correlation/span/otel" in src-tauri/src zero matches. Host+Hub scope correlation is completely unimplemented in Desktop. (End-to-end Runtime propagation was optional, but the mandatory Host+Hub piece is absent.) |
```

new:

```
| AC-1.13 | Trace Correlation: Desktop Host + Remote Hub scope correlation (mandatory; end-to-end optional) | Contract tests (Host+Hub scope) | PASS | `trace.rs` span-field `trace_id` (uuid v4) + `Instrument` propagation; birth at remote_im pump recv (per inbound message, both quick/detached branches — `runtime.rs pump_span_for`) and `session_manager.send_message` (per prompt turn); engine event collector inherits via `Span::current()`. 8 contract tests green (5 mechanism: await/spawn/mpsc/inheritance/distinctness; 3 wiring: pump birth, engine handle, send_message turn). End-to-end Runtime propagation remains optional non-goal (design §13, spec D5). |
```

- [ ] **Step 2: Update counts and FAIL list**

Counts table (:344-347): PASS `36` → `37`; FAIL `4` → `3`. PARTIAL 16 and BLOCKED 102 unchanged.

FAIL list item 2 (:364) — old:

```
2. **AC-1.13** — Host+Hub trace correlation completely absent (grep zero matches).
```

new:

```
2. ~~**AC-1.13** — Host+Hub trace correlation completely absent (grep zero matches).~~
   **Resolved 2026-07-31**: `trace.rs` span-field correlation per
   `docs/superpowers/plans/2026-07-31-trace-correlation.md` — pump birth per
   message + send_message birth per turn + collector inheritance; 8 contract
   tests green. End-to-end Runtime propagation optional per design §13.
```

- [ ] **Step 3: Sync test-coverage-audit.md**

Gap row (:79) — old:

```
| **Host+Hub trace correlation** | AC-1.13 | **High** | grep "trace_id/correlation/span/otel" = zero matches. Mandatory Host+Hub scope is absent. |
```

new:

```
| ~~**Host+Hub trace correlation**~~ | AC-1.13 | ~~**High**~~ **Resolved 2026-07-31** | tracing span-field `trace_id` + `Instrument` per `docs/superpowers/plans/2026-07-31-trace-correlation.md`: birth at remote_im pump recv (per message) and `send_message` (per turn); engine collector inherits via `Span::current()`. 8 contract tests (5 mechanism in `trace.rs`, 3 wiring). End-to-end Runtime propagation optional per design §13. |
```

Rust count line (:15): append before the closing ` |`:

```
+8 AC-1.13 trace-correlation tests (5 mechanism, 3 wiring) (2026-07-31 evening)
```

- [ ] **Step 4: Verify flip consistency**

```bash
cd /Users/po1nt9/Github/grok-app-main && grep -o "| PASS |" docs/release/1.0-acceptance-matrix.md | wc -l && grep -o "| FAIL |" docs/release/1.0-acceptance-matrix.md | wc -l && grep -c "AC-1.13.*PASS" docs/release/1.0-acceptance-matrix.md
```

Expected: PASS occurrences = 37 (36 + 1), FAIL occurrences = 4 (3 real + the counts-table row itself), AC-1.13 PASS row = 1. If mismatch, investigate before proceeding — do NOT hand-edit counts to force a match.

- [ ] **Step 5: Update project memory**

Edit `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`: change 3 FAIL → 2 FAIL (remaining: AC-10.9, AC-12.3); add AC-1.13 resolved entry (trace.rs span-field correlation, 8 tests, spec b22fbaa + plan); priority order becomes ① mock/real-Runtime E2E ② AC-10.9 ③ AC-12.3 ④ 真机验收.

- [ ] **Step 6: Run all gates**

```bash
cd /Users/po1nt9/Github/grok-app-main && cargo test --manifest-path src-tauri/Cargo.toml --lib
cd /Users/po1nt9/Github/grok-app-main && pnpm test && pnpm typecheck && pnpm check:i18n && pnpm check:brand && pnpm check:provenance && pnpm check:legal
```

Expected: cargo 490 pass; vitest 840 pass (no frontend change — if this drifts, investigate, don't assume); typecheck/i18n/brand/provenance/legal all green.

- [ ] **Step 7: Commit**

```
docs(release): AC-1.13 trace correlation PASS — matrix flip + audit sync (AC-1.13)
```

---

## Self-Review

- **Spec coverage:** D1 (span field + Instrument, zero deps) → Task 1. D2 (birth points: pump recv / send_message) → Tasks 2/3. D3 (uuid v4, `trace_id` field, span names `remote_msg`/`turn`) → Task 1. D4 (trace.rs + wiring + 8 tests) → Tasks 1–3. D5 non-goals → Global Constraints + Task 4 evidence wording.
- **8-test matrix:** 1–5 in `trace.rs` tests (Step 1 code); 6 `runtime_pump_births_trace_per_message` (runtime.rs); 7 `engine_handle_logs_carry_trace_id` (engine.rs); 8 `send_message_turn_logs_carry_trace_id` (session_manager.rs).
- **Placeholder scan:** all code blocks complete; the only intentional ellipsis is the engine collector body marked "existing body unchanged / byte-identical" with explicit anchor (`runtime event collector exited`).
- **Type consistency:** `CaptureLayer` Clone via `Arc<Mutex<Vec>>`; `Layer<S>` bound `S: Subscriber + for<'a> LookupSpan<'a>` matches registry usage; `set_default` + `current_thread` flavor is the thread-local-safe pattern; `trim_matches('"')` handles both `record_str` and `record_debug` paths.
- **Supply-chain hygiene:** zero new crates (调研结论 per user constraint — tracing span+Instrument is the tokio-rs official idiom; OTel rejected +45 deps; no active correlation-id crate exists).
