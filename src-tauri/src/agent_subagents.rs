//! Subagent spawning — spawn flags, env, config.
//!
//! CLI: `--no-subagents`, `GROK_SUBAGENTS`, `[subagents] enabled`.
//! Enabled by default; when App setting is off, force-disable at spawn.

use std::fs;

use crate::paths::{agent_config_toml, ensure_app_dirs};

/// Top-level CLI flags (before `agent`) for the subagents_enabled setting.
/// Empty when enabled (CLI default on); `["--no-subagents"]` when disabled.
pub fn subagents_spawn_flags(enabled: bool) -> Vec<&'static str> {
    if enabled {
        vec![]
    } else {
        vec!["--no-subagents"]
    }
}

/// `GROK_SUBAGENTS` env value when force-disabling. `None` when enabled.
pub fn subagents_spawn_env_value(enabled: bool) -> Option<&'static str> {
    if enabled {
        None
    } else {
        Some("0")
    }
}

/// When off, always force-disable so config cannot re-enable subagents.
pub fn should_force_disable_subagents(subagents_enabled: bool) -> bool {
    !subagents_enabled
}

/// Upsert `[subagents] enabled = bool` in a TOML-ish text blob.
pub fn set_subagents_enabled_in_toml(text: &str, enabled: bool) -> String {
    set_table_bool(text, "subagents", "enabled", enabled)
}

/// Upsert `[subagents] policy = "<wire>"` — the clamped subagent ceiling
/// (AC-1.5, design L226). Written in independent mode only via
/// [`sync_subagent_policy_to_agent_profile`].
pub fn set_subagent_policy_in_toml(text: &str, policy_wire: &str) -> String {
    set_table_string(text, "subagents", "policy", policy_wire)
}

/// Upsert `[subagents] inherit_mcp / inherit_workspace` — declared
/// inheritance of the parent's MCP allowlist and workspace constraints
/// (AC-1.5, design L226/L232). Desktop always writes `true`: there is no
/// supported mode where a subagent escapes the parent's MCP/workspace
/// constraints.
pub fn set_subagent_inherit_flags_in_toml(
    text: &str,
    inherit_mcp: bool,
    inherit_workspace: bool,
) -> String {
    let text = set_table_bool(text, "subagents", "inherit_mcp", inherit_mcp);
    set_table_bool(&text, "subagents", "inherit_workspace", inherit_workspace)
}

fn set_table_bool(text: &str, table: &str, key: &str, value: bool) -> String {
    set_table_line(text, table, key, &value.to_string())
}

fn set_table_string(text: &str, table: &str, key: &str, value: &str) -> String {
    set_table_line(text, table, key, &format!("\"{value}\""))
}

fn set_table_line(text: &str, table: &str, key: &str, rendered_value: &str) -> String {
    let header = format!("[{table}]");
    let line_val = format!("{key} = {rendered_value}");
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

/// Write `[subagents] enabled` into App agent-home (independent mode only).
pub fn sync_subagents_to_agent_profile(
    session_data_mode: &str,
    subagents_enabled: bool,
) -> Result<(), String> {
    if session_data_mode == "shared" {
        // Never rewrite the user's personal runtime config from the App.
        return Ok(());
    }
    let _ = ensure_app_dirs();
    let path = agent_config_toml();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let next = set_subagents_enabled_in_toml(&existing, subagents_enabled);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, next).map_err(|e| e.to_string())?;
    tracing::info!(
        "agent_subagents: synced [subagents] enabled={} → {}",
        subagents_enabled,
        path.display()
    );
    Ok(())
}

/// Apply spawn flag + env on a tokio Command (top-level, before `agent`).
/// When enabled, leaves CLI defaults alone; when disabled, force-disables.
/// AC-1.5: `policy` (the clamped subagent ceiling, wire form) is exported as
/// `OMP_SUBAGENT_POLICY` so the runtime applies the same cap in shared mode,
/// where the App must not rewrite the user's config.toml.
pub fn apply_subagents_to_command(
    cmd: &mut tokio::process::Command,
    enabled: bool,
    policy: Option<&str>,
) {
    for flag in subagents_spawn_flags(enabled) {
        cmd.arg(flag);
    }
    if let Some(v) = subagents_spawn_env_value(enabled) {
        cmd.env("GROK_SUBAGENTS", v);
    }
    if let Some(p) = policy {
        cmd.env("OMP_SUBAGENT_POLICY", p);
    }
}

/// Write the clamped subagent policy + inheritance flags into App agent-home
/// (independent mode only — shared mode never rewrites the user's config;
/// the same values reach the runtime via spawn env in that mode).
pub fn sync_subagent_policy_to_agent_profile(
    session_data_mode: &str,
    effective_policy_wire: &str,
) -> Result<(), String> {
    if session_data_mode == "shared" {
        return Ok(());
    }
    let _ = ensure_app_dirs();
    let path = agent_config_toml();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let next = set_subagent_policy_in_toml(&existing, effective_policy_wire);
    let next = set_subagent_inherit_flags_in_toml(&next, true, true);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, next).map_err(|e| e.to_string())?;
    tracing::info!(
        "agent_subagents: synced [subagents] policy={} inherit_mcp=true inherit_workspace=true → {}",
        effective_policy_wire,
        path.display()
    );
    Ok(())
}

/// AC-1.5: does this ACP `tool_call.kind` spawn a subagent?
/// Mirrors the frontend pattern set (`src/lib/toolDisplay.ts:55`:
/// `/subagent|spawn_agent|spawn_subagent|\bagent\b/`).
pub fn is_subagent_spawn_tool(tool_name: &str) -> bool {
    let k = tool_name.to_lowercase();
    if k.contains("subagent") || k.contains("spawn_agent") {
        return true;
    }
    // `\bagent\b` — a standalone word; `_` is a word char, so "agent_task"
    // must NOT match (regex parity).
    k.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w == "agent")
}

/// AC-1.5 host gate verdict: `true` → the permission request must be
/// auto-denied (a subagent spawn was attempted while the kill switch is
/// off). Defense in depth beyond the TOML/flag channel — the host never
/// relies on the runtime honoring config alone.
pub fn subagent_spawn_gate_denies(tool_name: &str, subagents_enabled: bool) -> bool {
    is_subagent_spawn_tool(tool_name) && !subagents_enabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_and_env() {
        assert!(subagents_spawn_flags(true).is_empty());
        assert_eq!(subagents_spawn_flags(false), vec!["--no-subagents"]);
        assert_eq!(subagents_spawn_env_value(true), None);
        assert_eq!(subagents_spawn_env_value(false), Some("0"));
        assert!(should_force_disable_subagents(false));
        assert!(!should_force_disable_subagents(true));
    }

    #[test]
    fn upserts_subagents_table() {
        let t = set_subagents_enabled_in_toml("", false);
        assert!(t.contains("[subagents]"));
        assert!(t.contains("enabled = false"));
        let t2 = set_subagents_enabled_in_toml(&t, true);
        assert!(t2.contains("enabled = true"));
        assert_eq!(t2.matches("enabled").count(), 1);

        let existing = "[ui]\nyolo = false\n\n[subagents]\nenabled = true\n";
        let next = set_subagents_enabled_in_toml(existing, false);
        assert!(next.contains("[subagents]"));
        assert!(next.contains("enabled = false"));
        assert!(next.contains("[ui]"));
    }

    // ── AC-1.5: policy/inherit TOML surface + spawn-kind detection ───────

    #[test]
    fn upserts_subagent_policy_and_inherit_flags() {
        let base = "[ui]\npermission_mode = \"default\"\n";
        let out = set_subagent_policy_in_toml(base, "accept_edits");
        let out = set_subagent_inherit_flags_in_toml(&out, true, true);
        assert!(out.contains("[subagents]"), "table created: {out}");
        assert!(
            out.contains("policy = \"accept_edits\""),
            "policy key: {out}"
        );
        assert!(out.contains("inherit_mcp = true"), "mcp flag: {out}");
        assert!(out.contains("inherit_workspace = true"), "ws flag: {out}");
        // Existing tables preserved.
        assert!(out.contains("[ui]"), "ui table preserved: {out}");
        // Re-upsert with a different policy replaces in place (no dup keys).
        let out2 = set_subagent_policy_in_toml(&out, "ask");
        assert!(out2.contains("policy = \"ask\""), "policy replaced: {out2}");
        assert!(!out2.contains("accept_edits"), "old value gone: {out2}");
        assert_eq!(out2.matches("policy =").count(), 1, "no dup: {out2}");
    }

    #[test]
    fn is_subagent_spawn_tool_matches_frontend_pattern_set() {
        // Mirrors src/lib/toolDisplay.ts:55 —
        // /subagent|spawn_agent|spawn_subagent|\bagent\b/
        for hit in [
            "subagent",
            "spawn_subagent",
            "spawn_agent",
            "SubagentTask",
            "agent",
            "Agent",
            "task/agent",
        ] {
            assert!(is_subagent_spawn_tool(hit), "should match: {hit}");
        }
        for miss in [
            "execute_command",
            "write",
            "read_file",
            "agent_task", // \bagent\b must not match inside a word
        ] {
            assert!(!is_subagent_spawn_tool(miss), "should not match: {miss}");
        }
    }

    #[test]
    fn spawn_gate_denies_only_when_disabled_and_spawn_kind() {
        assert!(subagent_spawn_gate_denies("spawn_subagent", false));
        assert!(subagent_spawn_gate_denies("agent", false));
        assert!(!subagent_spawn_gate_denies("spawn_subagent", true));
        assert!(!subagent_spawn_gate_denies("write", false));
        assert!(!subagent_spawn_gate_denies("execute_command", false));
    }

    #[test]
    fn apply_command_carries_kill_switch_and_policy_env() {
        let mut cmd = tokio::process::Command::new("true");
        apply_subagents_to_command(&mut cmd, false, Some("ask"));
        let std = cmd.as_std();
        let args: Vec<_> = std.get_args().collect();
        assert!(
            args.iter().any(|a| *a == "--no-subagents"),
            "kill-switch flag present: {args:?}"
        );
        let envs: Vec<_> = std.get_envs().collect();
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "GROK_SUBAGENTS" && *v == Some(std::ffi::OsStr::new("0"))),
            "GROK_SUBAGENTS=0 present: {envs:?}"
        );
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "OMP_SUBAGENT_POLICY" && *v == Some(std::ffi::OsStr::new("ask"))),
            "OMP_SUBAGENT_POLICY=ask present: {envs:?}"
        );
    }

    #[test]
    fn apply_command_enabled_no_policy_leaves_defaults() {
        let mut cmd = tokio::process::Command::new("true");
        apply_subagents_to_command(&mut cmd, true, None);
        let std = cmd.as_std();
        assert_eq!(std.get_args().count(), 0, "no flags when enabled");
        assert_eq!(
            std.get_envs().count(),
            0,
            "no env when enabled+no policy"
        );
    }
}
