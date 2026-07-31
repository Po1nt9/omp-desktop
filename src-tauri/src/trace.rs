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
        /// Global mode: only record events inside a traced span, so a
        /// process-wide install ignores unrelated tests' noise.
        traced_only: bool,
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
            if self.traced_only && trace_ids.is_empty() {
                return;
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

    /// Process-wide capture for tests that exercise *shared* callsites
    /// (e.g. `Engine::handle` logs, also emitted by other tests).
    ///
    /// Why not scoped `set_default` there: tracing caches each callsite's
    /// subscriber interest process-wide, and a concurrent test emitting the
    /// same callsite with no subscriber can bake `Interest::never()` into
    /// that cache — racing with (and starving) a scoped install. A global
    /// default participates in the interest union deterministically, and
    /// `traced_only` keeps foreign no-span events out of the buffer.
    pub(crate) fn global_events() -> Arc<Mutex<Vec<Captured>>> {
        static GLOBAL: std::sync::OnceLock<Arc<Mutex<Vec<Captured>>>> = std::sync::OnceLock::new();
        GLOBAL
            .get_or_init(|| {
                let events = Arc::new(Mutex::new(Vec::new()));
                let layer = CaptureLayer {
                    events: events.clone(),
                    traced_only: true,
                };
                // Err means another global default exists (none in this
                // codebase); events still accumulate via the shared Arc.
                let _ = tracing::subscriber::set_global_default(subscriber(layer));
                events
            })
            .clone()
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
