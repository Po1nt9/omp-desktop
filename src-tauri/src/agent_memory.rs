//! Cross-session memory — spawn flags, env, config.
//!
//! Plan 1 fail-closed shell: the `grok memory clear` CLI path is unavailable
//! until an OMP Runtime integration supplies a live agent runtime. Pure
//! helpers (spawn flag/env strings, TOML upsert, profile sync) remain as the
//! stable contract for a later plan.

use std::fs;

use crate::paths::{agent_config_toml, ensure_app_dirs};

/// Top-level CLI flag (before `agent`) for the experimental_memory setting.
pub fn memory_spawn_flag(enabled: bool) -> &'static str {
    if enabled {
        "--experimental-memory"
    } else {
        "--no-memory"
    }
}

/// `GROK_MEMORY` env value for the agent process.
pub fn memory_spawn_env_value(enabled: bool) -> &'static str {
    if enabled {
        "1"
    } else {
        "0"
    }
}

/// When off, always force-disable so config cannot leak memory on.
pub fn should_force_disable_memory(experimental_memory: bool) -> bool {
    !experimental_memory
}

/// Upsert `[memory] enabled = bool` in a TOML-ish text blob.
pub fn set_memory_enabled_in_toml(text: &str, enabled: bool) -> String {
    set_table_bool(text, "memory", "enabled", enabled)
}

fn set_table_bool(text: &str, table: &str, key: &str, value: bool) -> String {
    let header = format!("[{table}]");
    let line_val = format!("{key} = {value}");
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut in_table = false;
    let mut table_start: Option<usize> = None;
    for i in 0..lines.len() {
        let trimmed = lines[i].trim().to_string();
        if trimmed.starts_with('[') {
            if trimmed == header {
                in_table = true;
                table_start = Some(i);
            } else if in_table {
                lines.insert(i, line_val);
                return lines.join("\n") + "\n";
            } else {
                in_table = false;
            }
            continue;
        }
        if in_table {
            let key_part = trimmed.split('=').next().map(str::trim).unwrap_or("");
            if key_part == key {
                lines[i] = line_val;
                return lines.join("\n") + "\n";
            }
        }
    }
    if let Some(start) = table_start {
        lines.insert(start + 1, line_val);
        return lines.join("\n") + "\n";
    }
    let block = format!("\n{header}\n{line_val}\n");
    let base = text.trim_end();
    if base.is_empty() {
        format!("{header}\n{line_val}\n")
    } else {
        format!("{base}{block}")
    }
}

/// Write `[memory] enabled` into App agent-home (independent GROK_HOME only).
pub fn sync_memory_to_agent_profile(
    session_data_mode: &str,
    experimental_memory: bool,
) -> Result<(), String> {
    if session_data_mode == "shared" {
        // Never rewrite the user's personal ~/.grok/config.toml from the App.
        return Ok(());
    }
    let _ = ensure_app_dirs();
    let path = agent_config_toml();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let next = set_memory_enabled_in_toml(&existing, experimental_memory);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, next).map_err(|e| e.to_string())?;
    tracing::info!(
        "agent_memory: synced [memory] enabled={} → {}",
        experimental_memory,
        path.display()
    );
    Ok(())
}

/// Args for `grok memory clear` (workspace scope = product default).
pub fn memory_clear_cli_args(scope: &str) -> Vec<&'static str> {
    match scope.trim().to_ascii_lowercase().as_str() {
        "global" => vec!["memory", "clear", "-y", "--global"],
        "all" => vec!["memory", "clear", "-y", "--all"],
        _ => vec!["memory", "clear", "-y", "--workspace"],
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryClearResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub cwd: String,
}

/// Run memory clear scoped to `cwd` (project path when available).
///
/// Plan 1 fail-closed: the agent runtime is unavailable, so the `memory clear`
/// CLI path cannot run. Returns `runtime_unavailable` until an OMP Runtime
/// integration supplies live memory management.
pub fn clear_workspace_memory(
    _cwd: Option<&std::path::Path>,
    _session_data_mode: &str,
    _manual_cli_path: Option<&str>,
    _scope: &str,
) -> Result<MemoryClearResult, String> {
    Err("runtime_unavailable: memory clear is unavailable in this build".into())
}

/// Apply spawn flag + env on a tokio Command (top-level, before `agent`).
pub fn apply_memory_to_command(cmd: &mut tokio::process::Command, enabled: bool) {
    cmd.arg(memory_spawn_flag(enabled));
    cmd.env("GROK_MEMORY", memory_spawn_env_value(enabled));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_and_env() {
        assert_eq!(memory_spawn_flag(true), "--experimental-memory");
        assert_eq!(memory_spawn_flag(false), "--no-memory");
        assert_eq!(memory_spawn_env_value(true), "1");
        assert_eq!(memory_spawn_env_value(false), "0");
        assert!(should_force_disable_memory(false));
        assert!(!should_force_disable_memory(true));
    }

    #[test]
    fn upserts_memory_table() {
        let t = set_memory_enabled_in_toml("", true);
        assert!(t.contains("[memory]"));
        assert!(t.contains("enabled = true"));
        let t2 = set_memory_enabled_in_toml(&t, false);
        assert!(t2.contains("enabled = false"));
        assert_eq!(t2.matches("enabled").count(), 1);

        let existing = "[ui]\nyolo = false\n\n[memory]\nenabled = true\n";
        let next = set_memory_enabled_in_toml(existing, false);
        assert!(next.contains("[memory]"));
        assert!(next.contains("enabled = false"));
        assert!(next.contains("[ui]"));
    }

    #[test]
    fn clear_args() {
        assert_eq!(
            memory_clear_cli_args("workspace"),
            vec!["memory", "clear", "-y", "--workspace"]
        );
        assert_eq!(
            memory_clear_cli_args("global"),
            vec!["memory", "clear", "-y", "--global"]
        );
        assert_eq!(
            memory_clear_cli_args("all"),
            vec!["memory", "clear", "-y", "--all"]
        );
        assert_eq!(
            memory_clear_cli_args(""),
            vec!["memory", "clear", "-y", "--workspace"]
        );
    }
}
