//! In-process multi-channel Remote IM runtime (Rust only).

use super::channels;
use super::config;
use super::engine::Engine;
use super::outbound::OutboundRouter;
use super::types::{ChannelInstance, ConnectedChannel, IncomingMessage};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::Instrument;

pub struct RuntimeHandle {
    cancel_tx: watch::Sender<bool>,
    pump: JoinHandle<()>,
    connectors: Vec<JoinHandle<()>>,
    /// Kept alive so the pump channel never closes while connectors restart.
    _keepalive_tx: mpsc::Sender<IncomingMessage>,
    outbound: OutboundRouter,
    engine: Arc<Engine>,
}

impl RuntimeHandle {
    /// AC-8.4: bridge-side access for approval grant/revoke + status.
    pub(crate) fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub async fn stop(self) {
        let _ = self.cancel_tx.send(true);
        // Give connectors a moment to exit long-poll / WS loops via cancel.
        tokio::time::sleep(Duration::from_millis(200)).await;
        for h in self.connectors {
            h.abort();
        }
        self.pump.abort();
        // Do NOT clear outbound here: in-flight handle tasks may still reply.
        // A subsequent start_runtime builds a fresh OutboundRouter.
    }
}

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

pub async fn start_runtime(
    allow_remote_yolo: bool,
    binary_path: Option<PathBuf>,
    agent_dir: Option<PathBuf>,
) -> Result<(RuntimeHandle, Vec<ConnectedChannel>), String> {
    let list = config::list_instances();
    let mut active = Vec::new();
    let mut instances = Vec::new();

    for dto in list {
        if !dto.enabled || !dto.has_credentials {
            continue;
        }
        let secrets = config::get_secrets(&dto.id);
        if secrets.is_empty() {
            continue;
        }
        let inst = ChannelInstance {
            id: dto.id.clone(),
            channel: dto.channel.clone(),
            name: dto.name.clone(),
            enabled: true,
            secrets,
            options: dto.options.clone(),
            acl: dto.acl.clone(),
            project_scope: dto.project_scope.clone(),
        };
        active.push(ConnectedChannel {
            channel: dto.channel.clone(),
            instance_id: dto.id.clone(),
            name: dto.name.clone(),
        });
        instances.push(inst);
    }

    if instances.is_empty() {
        return Err("no enabled channel with credentials".into());
    }

    let outbound = OutboundRouter::new();
    let engine = Arc::new(Engine::new(
        outbound.clone(),
        allow_remote_yolo,
        binary_path,
        agent_dir,
    ));
    for inst in &instances {
        // Must inject _instance_id so weixin context_token / dingtalk webhooks resolve.
        let mut secrets = inst.secrets.clone();
        secrets.insert("_instance_id".into(), inst.id.clone());
        // Non-secret bind fields (feishu app_id, domain) live in options — also mirror
        // string options into secrets so send helpers that only read secrets still work.
        if let Some(obj) = inst.options.as_object() {
            for (k, v) in obj {
                if secrets.contains_key(k) {
                    continue;
                }
                if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    secrets.insert(k.clone(), s.to_string());
                }
            }
        }
        outbound.register(
            &inst.id,
            &inst.channel,
            secrets,
            inst.options.clone(),
        );
        engine.upsert_instance(inst.clone());
    }

    let (msg_tx, mut msg_rx) = mpsc::channel::<IncomingMessage>(256);
    let (cancel_tx, cancel_rx) = watch::channel(false);

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

    let mut connectors = Vec::new();
    for inst in instances {
        let h = channels::spawn_instance(inst, msg_tx.clone(), cancel_rx.clone());
        connectors.push(h);
    }
    // Keep one sender so pump does not exit if a connector task ends/restarts.
    let keepalive_tx = msg_tx;

    Ok((
        RuntimeHandle {
            cancel_tx,
            pump,
            connectors,
            _keepalive_tx: keepalive_tx,
            outbound,
            engine,
        },
        active,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-8.4: the handle exposes the engine so the bridge can grant/revoke
    /// approval; a fresh engine starts inactive even with persisted yolo.
    #[tokio::test]
    async fn handle_engine_accessor_exposes_approval_state() {
        let (cancel_tx, _rx) = watch::channel(false);
        let (keepalive_tx, _keepalive_rx) = mpsc::channel::<IncomingMessage>(1);
        let engine = Arc::new(Engine::new_ephemeral(OutboundRouter::new(), true));
        let h = RuntimeHandle {
            cancel_tx,
            pump: tokio::spawn(async {}),
            connectors: vec![],
            _keepalive_tx: keepalive_tx,
            outbound: OutboundRouter::new(),
            engine,
        };
        assert!(!h.engine().approval_active());
        h.engine()
            .grant_approval(crate::remote_im::engine::DEFAULT_APPROVAL_TTL_SECS);
        assert!(h.engine().approval_active());
        assert!(h.engine().approval_expires_at().is_some());
        h.engine().revoke_approval();
        assert!(!h.engine().approval_active());
    }

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

    /// AC-1.13: the pump births a distinct trace_id per inbound message.
    #[test]
    fn runtime_pump_births_trace_per_message() {
        use crate::trace::test_capture::{subscriber, CaptureLayer};

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
