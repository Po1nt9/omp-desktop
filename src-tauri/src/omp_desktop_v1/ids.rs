//! Stable ID validation patterns for the OMP Desktop v1 Extension Protocol.
//!
//! Each pattern mirrors `runtime/oh-my-pi/packages/coding-agent/src/modes/acp/desktop-v1/schema/ids.ts`.
//! Patterns are compiled once and reused via `id_patterns()`.

use regex::Regex;
use std::sync::OnceLock;

pub struct IdPatterns {
    pub session: Regex,
    pub turn: Regex,
    pub event: Regex,
    pub permission: Regex,
    pub queue_receipt: Regex,
    pub credential: Regex,
    pub project: Regex,
    pub model: Regex,
    pub mcp_source: Regex,
}

static ID_PATTERNS: OnceLock<IdPatterns> = OnceLock::new();

/// Returns the process-wide compiled ID pattern set.
///
/// The patterns are immutable for the lifetime of the process; callers may
/// cheaply borrow the `&'static IdPatterns` and run `.is_match(...)` without
/// any allocation.
pub fn id_patterns() -> &'static IdPatterns {
    ID_PATTERNS.get_or_init(|| IdPatterns {
        session: Regex::new(r"^sess_[a-z2-7]{26}$").unwrap(),
        turn: Regex::new(r"^turn_[a-z2-7]{26}$").unwrap(),
        event: Regex::new(r"^evt_[a-z2-7]{26}$").unwrap(),
        permission: Regex::new(r"^perm_[a-z2-7]{26}$").unwrap(),
        queue_receipt: Regex::new(r"^rcpt_[a-z2-7]{26}$").unwrap(),
        credential: Regex::new(r"^cred_[a-z2-7]{26}$").unwrap(),
        project: Regex::new(r"^proj_[a-f0-9]{40}$").unwrap(),
        model: Regex::new(r"^[a-z0-9-]+/[a-z0-9.-]+$").unwrap(),
        mcp_source: Regex::new(r"^mcp_[a-f0-9]{40}$").unwrap(),
    })
}
