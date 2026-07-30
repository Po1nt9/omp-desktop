//! Fixed-window request rate limiting (per-channel + per-scope), in-memory.
//!
//! Two independent fixed windows are maintained: one keyed by channel (bounds
//! overall traffic from a single IM platform) and one keyed by scope (bounds
//! turns within a single chat+sender). A request is admitted only when both
//! windows are under their limits. Windows reset lazily when a check observes
//! that [`window`](Self::window) has elapsed since the window started.
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const WINDOW_SECS: u64 = 60;
const CHANNEL_LIMIT: u32 = 60;
const SCOPE_LIMIT: u32 = 10;

struct WindowCounter {
    window_start: Instant,
    count: u32,
}

pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, WindowCounter>>>,
    window: Duration,
    channel_limit: u32,
    scope_limit: u32,
}

impl RateLimiter {
    pub fn new_default() -> Self {
        Self::new(WINDOW_SECS, CHANNEL_LIMIT, SCOPE_LIMIT)
    }

    pub fn new(window_secs: u64, channel_limit: u32, scope_limit: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            window: Duration::from_secs(window_secs),
            channel_limit,
            scope_limit,
        }
    }

    /// Returns `true` if the request is admitted, `false` if rate-limited.
    /// Checks per-channel and per-scope windows; both must pass.
    pub fn check(&self, channel: &str, scope_key: &str) -> bool {
        let now = Instant::now();
        let mut g = self.inner.lock();
        let ch_ok = bump(
            &mut g,
            format!("ch:{channel}"),
            now,
            self.window,
            self.channel_limit,
        );
        let sc_ok = bump(
            &mut g,
            format!("sc:{scope_key}"),
            now,
            self.window,
            self.scope_limit,
        );
        ch_ok && sc_ok
    }
}

/// Increments the counter for `key`. Resets the window if expired.
/// Returns `true` if under limit (allowed), `false` if over (denied).
fn bump(
    map: &mut HashMap<String, WindowCounter>,
    key: String,
    now: Instant,
    window: Duration,
    limit: u32,
) -> bool {
    let entry = map.entry(key).or_insert(WindowCounter {
        window_start: now,
        count: 0,
    });
    if now.duration_since(entry.window_start) >= window {
        entry.window_start = now;
        entry.count = 0;
    }
    if entry.count >= limit {
        false
    } else {
        entry.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_under_limit_passes() {
        let rl = RateLimiter::new(60, 5, 5);
        for _ in 0..5 {
            assert!(rl.check("telegram", "sc:1"));
        }
    }

    #[test]
    fn test_over_limit_dropped() {
        // scope limit = 1, so second call in same window is denied.
        let rl = RateLimiter::new(60, 100, 1);
        assert!(rl.check("telegram", "sc:1"));
        assert!(!rl.check("telegram", "sc:1"));
    }

    #[test]
    fn test_channel_and_scope_independent() {
        // channel limit 1, but scope budget large.
        let rl = RateLimiter::new(60, 1, 100);
        assert!(rl.check("telegram", "sc:1"));
        // Same channel different scope: channel limit hit -> denied.
        assert!(!rl.check("telegram", "sc:2"));
        // Different channel: fresh.
        assert!(rl.check("discord", "sc:1"));
    }

    #[test]
    fn test_expired_window_reset() {
        // window of 0 secs: every check sees an expired window -> always resets.
        let rl = RateLimiter::new(0, 1, 1);
        assert!(rl.check("telegram", "sc:1"));
        // window is 0 => next call's now.duration_since(start) >= 0 always true => reset.
        assert!(rl.check("telegram", "sc:1"));
    }
}
