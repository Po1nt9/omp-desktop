//! Anti-replay guard for webhook-sourced remote IM messages (AC-8.4).
//!
//! Slack-standard freshness window (signed timestamp within ±300s) plus an
//! in-memory nonce cache rejecting exact replays inside the window. Messages
//! without timestamp/nonce (WS / long-poll channels) pass through — their
//! transport is platform-authenticated and DedupStore covers redelivery.
//! Pure + clock-injected: `now` is always a parameter.

use parking_lot::Mutex;
use std::collections::HashMap;

/// Slack-standard webhook freshness window (seconds).
pub const DEFAULT_FRESHNESS_WINDOW_SECS: i64 = 300;

/// Lazy-sweep trigger for the nonce cache.
const SWEEP_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayVerdict {
    Allow,
    /// Timestamp outside the freshness window (old replay OR forged future).
    Stale,
    /// Nonce seen before and still inside its window.
    Replayed,
}

pub struct ReplayGuard {
    /// "channel|nonce" → expiry (unix secs).
    inner: Mutex<HashMap<String, i64>>,
    window_secs: i64,
}

impl ReplayGuard {
    pub fn new(window_secs: i64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            window_secs,
        }
    }

    /// Check one inbound message. Order: freshness first, then nonce.
    pub fn check(
        &self,
        channel: &str,
        timestamp: Option<i64>,
        nonce: Option<&str>,
        now: i64,
    ) -> ReplayVerdict {
        if let Some(ts) = timestamp {
            if (now - ts).abs() > self.window_secs {
                return ReplayVerdict::Stale;
            }
        }
        if let Some(nonce) = nonce {
            if nonce.is_empty() {
                return ReplayVerdict::Allow;
            }
            let key = format!("{channel}|{nonce}");
            let mut map = self.inner.lock();
            if let Some(exp) = map.get(&key) {
                if *exp > now {
                    return ReplayVerdict::Replayed;
                }
            }
            map.insert(key, now + self.window_secs);
            if map.len() > SWEEP_THRESHOLD {
                map.retain(|_, exp| *exp > now);
            }
        }
        ReplayVerdict::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_timestamp_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), None, 1000), ReplayVerdict::Allow);
        // Exactly at the window edge is still allowed (not > window).
        assert_eq!(g.check("wecom", Some(700), None, 1000), ReplayVerdict::Allow);
    }

    #[test]
    fn stale_old_timestamp_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(600), None, 1000), ReplayVerdict::Stale);
    }

    #[test]
    fn forged_future_timestamp_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1400), None, 1000), ReplayVerdict::Stale);
    }

    #[test]
    fn nonce_replay_within_window_rejected() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), Some("n1"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", Some(1001), Some("n1"), 1001), ReplayVerdict::Replayed);
    }

    #[test]
    fn nonce_reuse_after_expiry_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", Some(1000), Some("n1"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", Some(1301), Some("n1"), 1301), ReplayVerdict::Allow);
    }

    #[test]
    fn no_timestamp_no_nonce_allowed() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("telegram", None, None, 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("line", None, Some(""), 1000), ReplayVerdict::Allow);
    }

    #[test]
    fn nonce_cache_isolated_per_channel() {
        let g = ReplayGuard::new(300);
        assert_eq!(g.check("wecom", None, Some("shared"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("line", None, Some("shared"), 1000), ReplayVerdict::Allow);
        assert_eq!(g.check("wecom", None, Some("shared"), 1001), ReplayVerdict::Replayed);
    }
}
