//! Process-level helpers for the OMP Runtime Supervisor.
//!
//! The core spawn / monitor / stop logic lives in [`super::Supervisor`].
//! This module is reserved for future process-level concerns
//! (resource limits, signal forwarding, crash introspection) that
//! Plan 3 does not yet exercise.

use std::path::PathBuf;

/// Reserved handle for process-level configuration that the
/// `Supervisor` may consume in later plans.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ProcessHandle {
    pub binary_path: Option<PathBuf>,
}
