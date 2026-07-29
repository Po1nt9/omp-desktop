//! OMP Runtime Supervisor — manages the bundled `omp acp --stdio` sidecar.
//!
//! The Supervisor owns the child process handle, exposes lifecycle methods
//! (`start`, `is_running`, `stop`), and is the single place where the
//! `OMP_DESKTOP_V1_PROTOCOL=1` default environment is injected.
//!
//! See `docs/superpowers/plans/2026-07-29-plan-3-supervisor-core-acp.md`
//! Task 1 for the contract.

pub mod process;
pub mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::process::Child;
use tokio::sync::RwLock;

/// Configuration for [`Supervisor::new`].
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Absolute path to the bundled `omp` binary. `None` means the runtime
    /// is not available — [`Supervisor::start`] returns `runtime_unavailable`.
    pub binary_path: Option<PathBuf>,
    /// Optional agent working directory injected as `PI_CODING_AGENT_DIR`.
    pub agent_dir: Option<PathBuf>,
    /// Maximum restart attempts before giving up (enforced by future
    /// supervision loops; the field is part of the stable config surface).
    pub max_restarts: u32,
    /// Delay between restart attempts in milliseconds.
    pub restart_delay_ms: u64,
    /// Interval between health checks in milliseconds.
    pub health_check_interval_ms: u64,
    /// Extra environment variables applied to the child process.
    /// Always includes `OMP_DESKTOP_V1_PROTOCOL=1` by default.
    pub env_vars: Vec<(String, String)>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            agent_dir: None,
            max_restarts: 3,
            restart_delay_ms: 1000,
            health_check_interval_ms: 5000,
            env_vars: vec![("OMP_DESKTOP_V1_PROTOCOL".to_string(), "1".to_string())],
        }
    }
}

/// Process manager for the OMP Runtime sidecar.
pub struct Supervisor {
    config: SupervisorConfig,
    child: Arc<RwLock<Option<Child>>>,
    restart_count: Arc<RwLock<u32>>,
}

impl Supervisor {
    /// Create a new Supervisor that is not yet running.
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            child: Arc::new(RwLock::new(None)),
            restart_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Spawn `omp acp --stdio` if a binary is configured and exists.
    ///
    /// Returns `runtime_unavailable` (as an [`std::io::Error`] with
    /// [`std::io::ErrorKind::NotFound`]) when the binary is missing.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let binary = self.config.binary_path.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "runtime_unavailable: OMP binary not configured",
            )
        })?;
        if !binary.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "runtime_unavailable: OMP binary not found",
            )));
        }
        // Spawn `omp acp --stdio`
        let mut cmd = tokio::process::Command::new(binary);
        cmd.arg("acp").arg("--stdio");
        if let Some(dir) = &self.config.agent_dir {
            cmd.env("PI_CODING_AGENT_DIR", dir);
        }
        for (k, v) in &self.config.env_vars {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let child = cmd.spawn()?;
        *self.child.write().await = Some(child);
        Ok(())
    }

    /// Returns `true` if the child process is still running.
    ///
    /// A `None` child (never started, already stopped, or already exited)
    /// is treated as not running. Calling this clears a finished child
    /// handle so subsequent calls are cheap.
    pub async fn is_running(&self) -> bool {
        let mut guard = self.child.write().await;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    *guard = None;
                    false
                }
            }
        } else {
            false
        }
    }

    /// Stop the child process if any. Safe to call when not running.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut guard = self.child.write().await;
        if let Some(mut child) = guard.take() {
            // Try graceful shutdown first; kill() sends SIGKILL on Unix /
            // TerminateProcess on Windows. Higher-level graceful close
            // (stdin EOF → wait) lands in Plan 3 Task 2.
            let _ = child.kill().await;
        }
        Ok(())
    }
}
