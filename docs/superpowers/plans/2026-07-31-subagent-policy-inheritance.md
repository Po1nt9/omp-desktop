# AC-1.5 Subagent Policy Inheritance + Todo Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close AC-1.5 (matrix `docs/release/1.0-acceptance-matrix.md:35`, FAIL → PASS): subagent policy inheritance surface (clamp + config + spawn wiring + host gate) and todo lifecycle transport tests.

**Architecture:** Three Desktop-side layers per spec `docs/superpowers/specs/2026-07-31-subagent-policy-inheritance-design.md`: (1) pure policy clamp in `permission.rs` (never wider than parent); (2) `[subagents]` TOML keys + spawn flag/env wiring that finally connects the dead-code kill switch; (3) host permission-request gate denying subagent-spawn when disabled. Frontend: transport seam in `OmpDesktopV1Client` with mock-transport round-trip tests for `todo.list` / `subagents.*`.

**Tech Stack:** Rust (Tauri 2), parking_lot, tokio; React+TS, vitest.

## Global Constraints

- TDD per task: red → green → commit. One commit per task.
- Commit messages: English, `feat(<scope>): … (AC-1.5)` / `docs(release): … (AC-1.5)`.
- Design L226 verbatim: "subagent | parent policy + 显式继承/收窄规则 | 不得比 parent 扩权；MCP/workspace 限制必须继承".
- `runtime/oh-my-pi` is NOT git-tracked — never edit it; Runtime-side enforcement is out of scope (documented in matrix evidence).
- No new settings UI, no new i18n keys (D4). All gates must stay green: `cargo test --lib`, `pnpm test`, `pnpm typecheck`, `pnpm check:i18n`, `pnpm check:brand`, `pnpm check:provenance`, `pnpm check:legal`.
- `PermissionPolicy::parse` / `as_str` are the only wire conversions; new wire strings must round-trip through them.
- Do not commit secrets or `secrets.json`. Never log secret values.
- cwd resets between Bash calls — always `cd /Users/po1nt9/Github/grok-app-main` first.

---

### Task 1: Policy clamp pure function (permission.rs)

**Files:**
- Modify: `src-tauri/src/permission.rs` (insert after `effective_permission_policy`, line 539; tests in `mod tests` at line 596+)

**Interfaces:**
- Produces: `pub fn subagent_effective_policy(parent: PermissionPolicy, configured: Option<PermissionPolicy>) -> PermissionPolicy` — consumed by Task 4 (session_manager spawn site) and tested here.
- Produces (private): `fn permissiveness_rank(p: PermissionPolicy) -> u8`.

- [ ] **Step 1: Write the failing tests**

Append inside `#[cfg(test)] mod tests` in `src-tauri/src/permission.rs` (after the existing tests, before the closing brace):

```rust
    // ── AC-1.5: subagent policy clamp ────────────────────────────────────

    #[test]
    fn subagent_inherits_parent_when_unconfigured() {
        for p in [
            PermissionPolicy::Ask,
            PermissionPolicy::AllowOnce,
            PermissionPolicy::AllowForSession,
            PermissionPolicy::DontAsk,
            PermissionPolicy::AcceptEdits,
            PermissionPolicy::Deny,
            PermissionPolicy::AlwaysApprove,
        ] {
            assert_eq!(
                subagent_effective_policy(p, None),
                p,
                "unconfigured subagent must inherit parent unchanged"
            );
        }
    }

    #[test]
    fn subagent_clamp_never_wider_than_parent() {
        let all = [
            PermissionPolicy::Ask,
            PermissionPolicy::AllowOnce,
            PermissionPolicy::AllowForSession,
            PermissionPolicy::DontAsk,
            PermissionPolicy::AcceptEdits,
            PermissionPolicy::Deny,
            PermissionPolicy::AlwaysApprove,
        ];
        for p in all {
            for c in all {
                let eff = subagent_effective_policy(p, Some(c));
                assert!(
                    permissiveness_rank(eff) <= permissiveness_rank(p),
                    "parent={p:?} configured={c:?} → {eff:?} widens beyond parent"
                );
            }
        }
    }

    #[test]
    fn subagent_clamp_picks_narrower_of_the_two() {
        // Configured wider than parent → parent wins (narrowed to parent).
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::Ask,
                Some(PermissionPolicy::AcceptEdits)
            ),
            PermissionPolicy::Ask
        );
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AcceptEdits,
                Some(PermissionPolicy::AlwaysApprove)
            ),
            PermissionPolicy::AcceptEdits
        );
        // Configured narrower than parent → configured wins.
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AlwaysApprove,
                Some(PermissionPolicy::AllowForSession)
            ),
            PermissionPolicy::AllowForSession
        );
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AllowForSession,
                Some(PermissionPolicy::Ask)
            ),
            PermissionPolicy::Ask
        );
        // Equal → unchanged.
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AcceptEdits,
                Some(PermissionPolicy::AcceptEdits)
            ),
            PermissionPolicy::AcceptEdits
        );
    }

    #[test]
    fn subagent_clamp_treats_allow_once_as_ask() {
        // AllowOnce is a single-decision grant, not a session policy.
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AllowOnce,
                Some(PermissionPolicy::AcceptEdits)
            ),
            PermissionPolicy::AllowOnce,
            "AllowOnce parent caps at Ask-level permissiveness"
        );
        assert_eq!(
            subagent_effective_policy(
                PermissionPolicy::AllowForSession,
                Some(PermissionPolicy::AllowOnce)
            ),
            PermissionPolicy::AllowOnce,
            "AllowOnce configured narrows to Ask-level"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib permission::tests::subagent 2>&1 | tail -5`
Expected: FAIL — `E0425: cannot find function 'subagent_effective_policy' in this scope` (compile error).

- [ ] **Step 3: Implement the clamp**

Insert in `src-tauri/src/permission.rs` immediately after `effective_permission_policy`'s closing brace (line 539):

```rust
/// Permissiveness ladder for AC-1.5 clamping. Higher rank = more permissive.
/// `AllowOnce` is a single-decision grant, not a session policy — it
/// participates at `Ask` level.
fn permissiveness_rank(p: PermissionPolicy) -> u8 {
    match p {
        PermissionPolicy::Deny => 0,
        PermissionPolicy::DontAsk => 1,
        PermissionPolicy::Ask => 2,
        PermissionPolicy::AllowOnce => 2,
        PermissionPolicy::AcceptEdits => 3,
        PermissionPolicy::AllowForSession => 4,
        PermissionPolicy::AlwaysApprove => 5,
    }
}

/// AC-1.5: subagent effective policy = the narrower of the parent policy and
/// the configured subagent ceiling (design L226: 不得比 parent 扩权).
/// `None` → inherit the parent unchanged (neither widened nor narrowed).
pub fn subagent_effective_policy(
    parent: PermissionPolicy,
    configured: Option<PermissionPolicy>,
) -> PermissionPolicy {
    match configured {
        None => parent,
        Some(c) => {
            if permissiveness_rank(c) < permissiveness_rank(parent) {
                c
            } else {
                parent
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib permission:: 2>&1 | tail -3`
Expected: PASS — all permission tests including the 4 new ones.

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/permission.rs
git commit -m "feat(permission): subagent policy clamp — never wider than parent (AC-1.5)"
```

---

### Task 2: AppSettings.subagent_policy field (store.rs + frontend type)

**Files:**
- Modify: `src-tauri/src/store.rs` (field after `subagents_enabled` at line 224; factory default near line 350; tests near line 1713)
- Modify: frontend settings type — locate via grep (mirror of `subagentsEnabled`)

**Interfaces:**
- Produces: `AppSettings.subagent_policy: Option<String>` (serde camelCase → `subagentPolicy`), consumed by Task 4 spawn site.
- Consumes: nothing from Task 1 (independent).

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/store.rs` test module, next to `subagents_enabled_defaults_true_when_missing_from_json` (line 1713), add:

```rust
    #[test]
    fn subagent_policy_defaults_none_when_missing_from_json() {
        let s: AppSettings = serde_json::from_str("{}").unwrap_or_default();
        assert!(s.subagent_policy.is_none());
        assert!(s.subagents_enabled);
    }
```

(Check the exact deserialization pattern of the neighboring test and mirror it — if it uses `serde_json::from_str::<AppSettings>("{}").unwrap()`, use that form instead of `unwrap_or_default`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib store:: 2>&1 | tail -5`
Expected: FAIL — `E0609: no field 'subagent_policy' on type 'AppSettings'`.

- [ ] **Step 3: Add the field + factory default + frontend type**

In `src-tauri/src/store.rs`, immediately after `pub subagents_enabled: bool,` (line 224):

```rust
    /// AC-1.5: configured subagent policy ceiling (wire form). `None` =
    /// inherit the parent session policy unchanged (clamped, never widened).
    #[serde(default)]
    pub subagent_policy: Option<String>,
```

In the factory `Default` impl (near line 350, next to `subagents_enabled: true,`):

```rust
            subagent_policy: None,
```

Frontend: find the TS settings type mirroring `subagentsEnabled`:

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -rn "subagentsEnabled" src/lib/ src/components/ | grep -i "type\|interface\|?" | head -5`

In the located interface (likely `src/lib/api.ts` or a settings types file), add next to `subagentsEnabled`:

```ts
  /** AC-1.5: subagent policy ceiling (wire form). null/undefined = inherit parent. */
  subagentPolicy?: string | null;
```

- [ ] **Step 4: Run tests + typecheck**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib store:: 2>&1 | tail -3`
Expected: PASS.
Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm typecheck 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/store.rs src/lib/
git commit -m "feat(store): AppSettings.subagent_policy ceiling field (AC-1.5)"
```

---

### Task 3: Subagent TOML surface + spawn-kind detection + policy env (agent_subagents.rs)

**Files:**
- Modify: `src-tauri/src/agent_subagents.rs` (refactor `set_table_bool` at lines 38-77; extend `apply_subagents_to_command` at line 107; add new fns; extend tests at line 119+)

**Interfaces:**
- Produces: `pub fn set_subagent_policy_in_toml(text: &str, policy_wire: &str) -> String`
- Produces: `pub fn set_subagent_inherit_flags_in_toml(text: &str, inherit_mcp: bool, inherit_workspace: bool) -> String`
- Produces: `pub fn sync_subagent_policy_to_agent_profile(session_data_mode: &str, effective_policy_wire: &str) -> Result<(), String>`
- Produces: `pub fn is_subagent_spawn_tool(tool_name: &str) -> bool`
- Produces: `pub fn subagent_spawn_gate_denies(tool_name: &str, subagents_enabled: bool) -> bool` — consumed by Task 5 (session_manager gate) and `permission_host_test.rs`.
- Produces (changed signature): `pub fn apply_subagents_to_command(cmd: &mut tokio::process::Command, enabled: bool, policy: Option<&str>)` — consumed by Task 4 (`acp_client.rs`). Note: currently dead code, so the signature change breaks no caller.

- [ ] **Step 1: Write the failing tests**

Append inside `#[cfg(test)] mod tests` in `src-tauri/src/agent_subagents.rs`:

```rust
    // ── AC-1.5: policy/inherit TOML surface + spawn-kind detection ───────

    #[test]
    fn upserts_subagent_policy_and_inherit_flags() {
        let base = "[ui]\npermission_mode = \"default\"\n";
        let out = set_subagent_policy_in_toml(base, "accept_edits");
        let out = set_subagent_inherit_flags_in_toml(&out, true, true);
        assert!(out.contains("[subagents]"), "table created: {out}");
        assert!(out.contains("policy = \"accept_edits\""), "policy key: {out}");
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
        // Mirrors src/lib/toolDisplay.ts:55 — /subagent|spawn_agent|spawn_subagent|\bagent\b/
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
            envs.iter().any(|(k, v)| *k == "GROK_SUBAGENTS" && *v == "0"),
            "GROK_SUBAGENTS=0 present: {envs:?}"
        );
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "OMP_SUBAGENT_POLICY" && *v == "ask"),
            "OMP_SUBAGENT_POLICY=ask present: {envs:?}"
        );
    }

    #[test]
    fn apply_command_enabled_no_policy_leaves_defaults() {
        let mut cmd = tokio::process::Command::new("true");
        apply_subagents_to_command(&mut cmd, true, None);
        let std = cmd.as_std();
        assert_eq!(std.get_args().count(), 0, "no flags when enabled");
        assert_eq!(std.get_envs().count(), 0, "no env when enabled+no policy");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib agent_subagents 2>&1 | tail -5`
Expected: FAIL — compile errors (`cannot find function 'set_subagent_policy_in_toml' …`, `this function takes 2 arguments but 3 arguments were supplied` for `apply_subagents_to_command`).

- [ ] **Step 3: Implement**

In `src-tauri/src/agent_subagents.rs`:

**3a.** Refactor `set_table_bool` (lines 38-77) to share a line-renderer core — replace the whole `fn set_table_bool` body with:

```rust
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
```

(The body of `set_table_line` is verbatim the old `set_table_bool` body with `line_val` built from the parameter — pure refactor, existing tests must stay green.)

**3b.** Add the new public functions after `set_subagents_enabled_in_toml` (line 36):

```rust
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
```

**3c.** Add the policy sync after `sync_subagents_to_agent_profile` (ends ~line 104):

```rust
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
```

**3d.** Add spawn-kind detection + gate verdict (place before `apply_subagents_to_command`):

```rust
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
```

**3e.** Extend `apply_subagents_to_command` (line 107) — replace whole fn:

```rust
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
```

**3f.** Fix the existing test that calls the old 2-arg signature (in `flags_and_env` test if present): change `apply_subagents_to_command(&mut cmd, false)` → `apply_subagents_to_command(&mut cmd, false, None)`. (Check with grep; the existing tests at lines 120-144 may not call it at all — if unused there, skip.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib agent_subagents 2>&1 | tail -3`
Expected: PASS (2 old + 5 new).

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/agent_subagents.rs
git commit -m "feat(agent_subagents): policy/inherit TOML surface + spawn-kind gate + policy env (AC-1.5)"
```

---

### Task 4: Spawn wiring — connect the dead-code kill switch (acp_client.rs + session_manager.rs)

**Files:**
- Modify: `src-tauri/src/acp_client.rs` (`SpawnOptions` lines 194-204; `spawn_with_options` lines 261-269; test literals at lines 2167/2180/2191 if they enumerate all fields)
- Modify: `src-tauri/src/session_manager.rs` (spawn_opts construction at line 2653; TOML policy sync near the existing `sync_permission_to_agent_profile` call in connect — locate via grep)

**Interfaces:**
- Consumes: `subagent_effective_policy` (Task 1), `AppSettings.subagent_policy` (Task 2), `apply_subagents_to_command(cmd, enabled, policy)` + `sync_subagent_policy_to_agent_profile` (Task 3).
- Produces: `SpawnOptions.subagents_enabled: Option<bool>`, `SpawnOptions.subagent_policy: Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/acp_client.rs` test module (near the spawn fail-closed tests at lines 2032-2225), add:

```rust
    // ── AC-1.5: spawn wiring — kill switch + clamped policy reach the process ──

    #[test]
    fn spawn_command_carries_subagent_kill_switch_and_policy() {
        let opts = SpawnOptions {
            subagents_enabled: Some(false),
            subagent_policy: Some("ask".to_string()),
            ..Default::default()
        };
        let cmd = build_spawn_command(Path::new("/nonexistent/omp"), &opts);
        let std = cmd.as_std();
        let args: Vec<_> = std.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert!(args.iter().any(|a| a == "--no-subagents"), "args: {args:?}");
        let envs: Vec<_> = std
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect();
        assert!(
            envs.iter().any(|(k, v)| k == "GROK_SUBAGENTS" && v.as_deref() == Some("0")),
            "envs: {envs:?}"
        );
        assert!(
            envs.iter().any(|(k, v)| k == "OMP_SUBAGENT_POLICY" && v.as_deref() == Some("ask")),
            "envs: {envs:?}"
        );
        // Pre-existing wiring intact.
        assert!(
            envs.iter().any(|(k, v)| k == "OMP_DESKTOP_V1_PROTOCOL" && v.as_deref() == Some("1")),
            "envs: {envs:?}"
        );
    }

    #[test]
    fn spawn_command_default_opts_leave_subagents_alone() {
        let opts = SpawnOptions::default();
        let cmd = build_spawn_command(Path::new("/nonexistent/omp"), &opts);
        let std = cmd.as_std();
        assert!(
            !std.get_args().any(|a| a == "--no-subagents"),
            "default must not force-disable"
        );
        assert!(
            !std.get_envs().any(|(k, _)| k == "GROK_SUBAGENTS" || k == "OMP_SUBAGENT_POLICY"),
            "default must not set subagent env"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib acp_client 2>&1 | tail -5`
Expected: FAIL — `E0425: cannot find function 'build_spawn_command'` and `E0560: struct 'SpawnOptions' has no field named 'subagents_enabled'`.

- [ ] **Step 3: Implement**

**3a.** Extend `SpawnOptions` (lines 194-204) — add two fields before the closing brace:

```rust
    /// AC-1.5: subagent kill switch. `None` = leave CLI defaults (enabled).
    /// `Some(false)` = force-disable via `--no-subagents` + `GROK_SUBAGENTS=0`.
    pub subagents_enabled: Option<bool>,
    /// AC-1.5: clamped subagent policy ceiling (wire form, from
    /// `permission::subagent_effective_policy`). Exported as
    /// `OMP_SUBAGENT_POLICY` so shared mode gets the same cap the TOML
    /// channel delivers in independent mode.
    pub subagent_policy: Option<String>,
```

**3b.** Extract `build_spawn_command` and call it from `spawn_with_options`. Replace lines 261-269 (`let mut cmd = ... stderr(...)`):

```rust
        let mut cmd = build_spawn_command(&binary, &opts);

        let child = cmd.spawn().map_err(|e| {
```

Add the new free function immediately before `impl AcpClient` (before line 219):

```rust
/// Assemble the `omp acp --stdio` spawn command (AC-1.5: extracted so the
/// subagent kill switch / policy env wiring is unit-testable without
/// spawning a process).
fn build_spawn_command(binary: &Path, opts: &SpawnOptions) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.arg("acp").arg("--stdio");
    if let Some(dir) = &opts.agent_dir {
        cmd.env("PI_CODING_AGENT_DIR", dir);
    }
    cmd.env("OMP_DESKTOP_V1_PROTOCOL", "1");
    crate::agent_subagents::apply_subagents_to_command(
        &mut cmd,
        opts.subagents_enabled.unwrap_or(true),
        opts.subagent_policy.as_deref(),
    );
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd
}
```

(Check `Path` is imported in acp_client.rs — `PathBuf` is; add `Path` to the `use std::path::…` import if missing.)

**3c.** Fix any `SpawnOptions` literals that enumerate all fields (compile errors will point at them — candidates: test sites at lines 2167/2180/2191). Add `..Default::default()` or the two new fields explicitly.

**3d.** Wire the spawn site in `src-tauri/src/session_manager.rs` (line 2653). Replace the `spawn_opts` literal:

```rust
        // AC-1.5: clamped subagent ceiling — never wider than the session's
        // own effective policy (design L226). Reaches the runtime via TOML
        // (independent) + OMP_SUBAGENT_POLICY env (both modes).
        let subagent_policy = crate::permission::subagent_effective_policy(
            crate::permission::PermissionPolicy::parse(&prefs.permission_policy),
            settings
                .subagent_policy
                .as_deref()
                .map(crate::permission::PermissionPolicy::parse),
        )
        .as_str()
        .to_string();
        let _ = crate::agent_subagents::sync_subagent_policy_to_agent_profile(
            &settings.session_data_mode,
            &subagent_policy,
        );
        let spawn_opts = crate::acp_client::SpawnOptions {
            model_id: Some(agent_model.clone()),
            effort: Some(prefs.effort.clone()),
            permission_policy: Some(prefs.permission_policy.clone()),
            binary_path,
            agent_dir,
            subagents_enabled: Some(settings.subagents_enabled),
            subagent_policy: Some(subagent_policy),
        };
```

**3e.** Run a repo-wide check for other `SpawnOptions` literals:

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -rn "SpawnOptions {" src-tauri/src/ | grep -v "pub struct"`
Expected: only session_manager.rs:2653 and acp_client.rs test sites — all handled above. If `remote_im/runtime.rs` or others appear, add the two fields as `subagents_enabled: None, subagent_policy: None` there (the remote engine path manages its own policy via AC-8.4 approval).

- [ ] **Step 4: Run tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib acp_client 2>&1 | tail -3`
Expected: PASS.
Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: PASS — full suite green (no regressions from the struct/signature changes).

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/acp_client.rs src-tauri/src/session_manager.rs
git commit -m "feat(acp_client): wire subagent kill switch + clamped policy into spawn (AC-1.5)"
```

---

### Task 5: Host permission gate (session_manager.rs PermissionRequest arm)

**Files:**
- Modify: `src-tauri/src/session_manager.rs` (`LiveSession` struct at line 190 — add field after `policy` at line 222; constructor sites at lines 1795, 2545, 2942, 5568, 5733, 5829; PermissionRequest arm at lines 3268-3298)
- Modify: `src-tauri/src/permission_host_test.rs` (new test)

**Interfaces:**
- Consumes: `subagent_spawn_gate_denies` (Task 3).
- Produces: `LiveSession.subagents_enabled: bool` (private).

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/permission_host_test.rs` inside `mod host_permission_e2e`:

```rust
    #[test]
    fn ac15_subagent_spawn_gate_denies_even_under_yolo() {
        use crate::agent_subagents::subagent_spawn_gate_denies;
        // Under AlwaysApprove a spawn request would otherwise be auto-allowed…
        let cache = SessionAllowCache::default();
        let would_auto = may_auto_allow(
            PermissionPolicy::AlwaysApprove,
            &cache,
            "spawn_subagent:",
            None,
            "",
            "spawn_subagent",
            "",
        );
        assert!(would_auto, "yolo auto-allows without the gate");
        // …but the AC-1.5 kill-switch gate overrides it when disabled.
        assert!(subagent_spawn_gate_denies("spawn_subagent", false));
        assert!(!subagent_spawn_gate_denies("spawn_subagent", true));
        // Non-spawn tools are never gate-denied.
        assert!(!subagent_spawn_gate_denies("write", false));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib permission_host_test 2>&1 | tail -5`
Expected: compiles (Task 3 landed the fn) but if run before the session_manager wiring it still passes — the RED for this task is the wiring regression test below. To keep honest TDD, first add the session_manager wiring test:

In `src-tauri/src/session_manager.rs` test module, add:

```rust
    #[test]
    fn ac15_live_session_snapshots_subagents_enabled() {
        // Compile-level guarantee: LiveSession carries the kill-switch
        // snapshot used by the PermissionRequest gate.
        let s = sample_live_for_empty_run("body", "", 0, "agent");
        assert!(s.subagents_enabled, "test fixtures default to enabled");
    }
```

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib ac15 2>&1 | tail -5`
Expected: FAIL — `E0609: no field 'subagents_enabled' on type 'LiveSession'`.

- [ ] **Step 3: Implement**

**3a.** Add the field to `LiveSession` after `policy: PermissionPolicy,` (line 222):

```rust
    /// AC-1.5: subagent kill-switch snapshot at connect/respawn. The
    /// PermissionRequest gate denies subagent-spawn tool calls when false —
    /// even under AlwaysApprove (defense in depth beyond TOML/CLI flags).
    subagents_enabled: bool,
```

**3b.** Populate at every constructor site. Sites and values:

- Line 2545 (connect — `settings` in scope): `subagents_enabled: settings.subagents_enabled,`
- Lines 1795 and 2942 (load/resume paths — check whether a settings value is in scope; if not, use `subagents_enabled: crate::store::load_settings().subagents_enabled,`)
- Test fixtures at 5568, 5733, 5829: `subagents_enabled: true,`

The compiler will enumerate any missed site (`E0063: missing field 'subagents_enabled'`).

**3c.** The gate in the PermissionRequest arm. In `src-tauri/src/session_manager.rs` at lines 3279-3288, replace the `let auto = …` / `let auto_deny = …` block inside the `if let Some(s) = guard.as_mut()`:

```rust
                        // AC-1.5: subagent-spawn kill switch — even a yolo
                        // parent must not auto-allow spawning when subagents
                        // are disabled. Gate wins over may_auto_allow.
                        let subagent_gate = crate::agent_subagents::subagent_spawn_gate_denies(
                            &tool_name,
                            s.subagents_enabled,
                        );
                        if subagent_gate {
                            tracing::warn!(
                                target: "permission",
                                tool = %tool_name,
                                "subagent-spawn permission denied: subagents disabled"
                            );
                        }
                        let auto = !subagent_gate
                            && may_auto_allow(
                                s.policy,
                                &s.allow_cache,
                                &sk,
                                root.as_deref(),
                                &path_target,
                                &tool_name,
                                &shell_command,
                            );
                        let auto_deny = subagent_gate || (!auto && may_auto_deny(s.policy));
```

(The existing auto_deny branch below then responds `reject_once` — no new response path needed.)

- [ ] **Step 4: Run tests**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: PASS — full suite including the two new tests.

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src-tauri/src/session_manager.rs src-tauri/src/permission_host_test.rs
git commit -m "feat(session_manager): host gate denies subagent-spawn permission when disabled (AC-1.5)"
```

---

### Task 6: Frontend transport seam + todo/subagents round-trip tests

**Files:**
- Modify: `src/lib/ompDesktopV1/index.ts` (transport seam)
- Modify: `src/lib/ompDesktopV1/contract.test.ts` (new describe block)

**Interfaces:**
- Produces: `export type DesktopV1Transport = (method: string, params: unknown) => Promise<unknown>`; `OmpDesktopV1Client.setTransport(t: DesktopV1Transport | null): void`.

- [ ] **Step 1: Write the failing tests**

Append to `src/lib/ompDesktopV1/contract.test.ts`:

```ts
describe("transport seam (AC-1.5)", () => {
  const capAll: DesktopV1Capability = {
    schemaVersion: 1,
    schemaDigest: "test-digest",
    methods: [
      "_omp/desktop/v1/todo.list",
      "_omp/desktop/v1/subagents.status",
      "_omp/desktop/v1/subagents.setEnabled",
    ],
    notifications: [],
    optionalFeatures: [],
  };

  it("todo.list round-trips phases through the injected transport", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const seen: Array<{ method: string; params: unknown }> = [];
    client.setTransport(async (method, params) => {
      seen.push({ method, params });
      return {
        phases: [
          {
            name: "phase-1",
            tasks: [
              { content: "design", status: "completed" },
              { content: "implement", status: "in_progress" },
              { content: "test", status: "pending" },
              { content: "drop", status: "abandoned" },
              { content: "wait", status: "blocked" },
            ],
          },
        ],
      };
    });

    const result = await client.call("todo.list", { sessionId: "s-1" });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.phases).toHaveLength(1);
      expect(result.value.phases[0].tasks.map((t) => t.status)).toEqual([
        "completed",
        "in_progress",
        "pending",
        "abandoned",
        "blocked",
      ]);
    }
    expect(seen).toEqual([
      { method: "_omp/desktop/v1/todo.list", params: { sessionId: "s-1" } },
    ]);
  });

  it("subagents.status round-trips", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    client.setTransport(async () => ({ enabled: true, activeCount: 2 }));
    const result = await client.call("subagents.status", {});
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value).toEqual({ enabled: true, activeCount: 2 });
    }
  });

  it("subagents.setEnabled forwards params and echoes result", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const seen: unknown[] = [];
    client.setTransport(async (_method, params) => {
      seen.push(params);
      return { enabled: false };
    });
    const result = await client.call("subagents.setEnabled", { enabled: false });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value).toEqual({ enabled: false });
    expect(seen).toEqual([{ enabled: false }]);
  });

  it("transport rejection maps to runtime_unavailable", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    client.setTransport(async () => {
      throw new Error("boom");
    });
    const result = await client.call("todo.list", {});
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.code).toBe("runtime_unavailable");
  });

  it("capability without transport stays fail-closed", async () => {
    const client = new OmpDesktopV1Client();
    client.setCapability(capAll);
    const result = await client.call("todo.list", {});
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.code).toBe("runtime_unavailable");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm vitest run src/lib/ompDesktopV1/contract.test.ts 2>&1 | tail -6`
Expected: FAIL — `client.setTransport is not a function` (TypeError in each new test).

- [ ] **Step 3: Implement the transport seam**

In `src/lib/ompDesktopV1/index.ts`:

**3a.** Add the transport type after the `CallResult` definition (line 40):

```ts
/**
 * Injected transport for `_omp/desktop/v1/*` requests (Plan 3 seam, AC-1.5).
 * Receives the fully-namespaced method and the raw params; resolves with the
 * JSON-RPC result payload. Rejections map to `runtime_unavailable`.
 */
export type DesktopV1Transport = (
  method: string,
  params: unknown,
) => Promise<unknown>;
```

**3b.** The type is declared `export type` inline, so no extra re-export line is needed.

**3c.** Add the field + setter to `OmpDesktopV1Client` (after `private capability` at line 53):

```ts
  private transport: DesktopV1Transport | null = null;

  /**
   * Inject (or clear with `null`) the request transport. Until a transport
   * is set the client stays fail-closed even with a negotiated capability.
   */
  setTransport(t: DesktopV1Transport | null): void {
    this.transport = t;
  }
```

**3d.** Replace the fail-closed tail of `call()` (lines 94-97):

```ts
    if (!this.transport) {
      // Plan 2 fail-closed: no transport injected yet.
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
    try {
      const value = (await this.transport(fullMethod, params)) as MethodMap[K]["result"];
      return { ok: true, value };
    } catch (e) {
      if (isDesktopV1Error(e)) {
        return { ok: false, error: e };
      }
      return { ok: false, error: RUNTIME_UNAVAILABLE };
    }
```

(`isDesktopV1Error` is already imported at line 17.)

- [ ] **Step 4: Run tests + typecheck**

Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm vitest run src/lib/ompDesktopV1/contract.test.ts 2>&1 | tail -4`
Expected: PASS — all tests (old + 5 new).
Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm typecheck 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add src/lib/ompDesktopV1/index.ts src/lib/ompDesktopV1/contract.test.ts
git commit -m "feat(ompDesktopV1): transport seam + todo.list/subagents round-trip tests (AC-1.5)"
```

---

### Task 7: Docs — flip AC-1.5 to PASS + counts + audits + memory + final gates

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (row at line 35; counts at lines 344-347; FAIL list at lines 353-364; check appendix Todo/Subagent row — grep first)
- Modify: `docs/release/test-coverage-audit.md` (test counts; gap table — check for AC-1.5 row)
- Modify: `docs/release/security-audit-checklist.md` (grep for subagent/AC-1.5 rows — e.g. SA-P.* )
- Modify: memory files `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md` + `MEMORY.md`

**Interfaces:**
- Consumes: all previous tasks (evidence strings below).

- [ ] **Step 1: Verify the new test inventory**

Run: `cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib 2>&1 | grep "test result"`
Expected: 468 + 10 = **478 passed** (4 clamp + 1 store + 5 agent_subagents — recount precisely; `permission_host_test` adds 1, `session_manager` adds 1, `acp_client` adds 2 → actual number from the run wins).
Run: `cd /Users/po1nt9/Github/grok-app-main && pnpm test 2>&1 | grep -E "Test Files|Tests "`
Expected: 835 + 5 = **840 passed**.

- [ ] **Step 2: Flip the matrix row**

Replace the AC-1.5 row (line 35) verdict `FAIL` → `PASS` with evidence:

```
| AC-1.5 | Todo / Subagent: todo lifecycle; subagent policy inheritance (no privilege escalation, MCP/workspace constraints inherited) | Contract tests + permission inheritance tests | PASS | Todo: transport seam + mock round-trip (`ompDesktopV1/contract.test.ts` "transport seam (AC-1.5)": todo.list five-state lifecycle + sessionId forwarding). Subagent inheritance: `permission::subagent_effective_policy` clamp (never wider than parent, exhaustive matrix test); `[subagents] policy/inherit_mcp/inherit_workspace` TOML (independent) + `OMP_SUBAGENT_POLICY` env + `--no-subagents`/`GROK_SUBAGENTS=0` spawn wiring (dead code connected — kill switch now real in shared mode); host gate denies subagent-spawn permission when disabled even under yolo (`subagent_spawn_gate_denies`, host test). Runtime-side enforcement of the declared constraints remains its own responsibility (design L226) — real-Runtime E2E stays BLOCKED under the E2E umbrella. |
```

Check the appendix Todo/Subagent row mentioned in earlier audits:

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -n "Todo.*Subagent\|Subagent.*Todo" docs/release/1.0-acceptance-matrix.md`

If an appendix row tracks the same item as BLOCKED with a Desktop-side remedy, update its cell per the same evidence (leave genuinely Runtime-E2E cells BLOCKED).

- [ ] **Step 3: Recompute counts + prune the FAIL list**

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -c "| PASS |" docs/release/1.0-acceptance-matrix.md && grep -c "| FAIL |" docs/release/1.0-acceptance-matrix.md`
Update the counts table (lines 344-347) with the grep totals (expect PASS 36, FAIL 4 — verify by grep, the file's own numbers win). Remove item 1 (AC-1.5) from the release-blocking FAIL list (lines 353-364) and renumber.

- [ ] **Step 4: Update coverage + security audits**

In `docs/release/test-coverage-audit.md`: bump the cargo total (line 15) and frontend total, note the AC-1.5 additions; if a gap-table row names AC-1.5 subagent/todo, strike it through with "**Resolved 2026-07-31**" like the AC-8.4 row (line 82 pattern — wait, line 82 is AC-8.4's row which should already be struck; grep for the AC-1.5 row).

In `docs/release/security-audit-checklist.md`:

Run: `cd /Users/po1nt9/Github/grok-app-main && grep -n "subagent\|AC-1.5" docs/release/security-audit-checklist.md`
Update matching rows (e.g. SA-P.1/SA-P.7) with PASS + the test evidence.

- [ ] **Step 5: Run all gates**

```bash
cd /Users/po1nt9/Github/grok-app-main/src-tauri && cargo test --lib 2>&1 | tail -2
cd /Users/po1nt9/Github/grok-app-main && pnpm test 2>&1 | tail -3
cd /Users/po1nt9/Github/grok-app-main && pnpm typecheck 2>&1 | tail -2
cd /Users/po1nt9/Github/grok-app-main && pnpm check:i18n 2>&1 | tail -2
cd /Users/po1nt9/Github/grok-app-main && pnpm check:brand 2>&1 | tail -2
cd /Users/po1nt9/Github/grok-app-main && pnpm check:provenance 2>&1 | tail -2
cd /Users/po1nt9/Github/grok-app-main && pnpm check:legal 2>&1 | tail -2
```
Expected: all green (cargo 478+, vitest 840+, i18n 1885 keys ×3 unchanged, brand/provenance/legal pass).

- [ ] **Step 6: Commit docs**

```bash
cd /Users/po1nt9/Github/grok-app-main
git add docs/release/1.0-acceptance-matrix.md docs/release/test-coverage-audit.md docs/release/security-audit-checklist.md
git commit -m "docs(release): flip AC-1.5 subagent policy inheritance + todo lifecycle to PASS"
```

- [ ] **Step 7: Update memory**

Update `/Users/po1nt9/.zcode/cli/memories/projects/github-858e378dd021e1c0/memory/omp-desktop-roadmap-status.md`: description → 剩余 3 FAIL; add an AC-1.5 bullet (decisions D1-D4, commit range, counts, the dead-code kill-switch fix); priorities → ①AC-1.13 → ②mock/real-Runtime E2E → ③AC-10.9 → ④AC-12.3 → ⑤真机验收. Update the `MEMORY.md` index line hook.

---

## Self-Review Notes

- **Spec coverage:** §3.1→Task 1; §3.6→Task 2; §3.2+§3.3(partial)+§3.4(detection)→Task 3; §3.3(wiring)→Task 4; §3.4(gate)→Task 5; §3.5→Task 6; §6→Task 7. §4 non-goals respected (no Runtime edits, no UI, no i18n).
- **Type consistency:** `subagent_effective_policy(parent, configured)` — Task 1 def = Task 4 call. `apply_subagents_to_command(cmd, enabled, policy)` — Task 3 def = Task 4 call. `subagent_spawn_gate_denies(tool_name, subagents_enabled)` — Task 3 def = Task 5 call + host test. `SpawnOptions.{subagents_enabled, subagent_policy}` — Task 4 def = Task 4 use. `DesktopV1Transport`/`setTransport` — Task 6 def = Task 6 tests. `subagent_policy` field — Task 2 def = Task 4 use.
- **Sequencing:** Task 2 (store field) precedes Task 4 (its consumer); Task 3 precedes Tasks 4-5; Task 1 precedes Task 4. Tasks 2-3 are independent of Task 1.
- **Placeholder scan:** every code step shows complete code; the two "locate via grep" instructions name exact patterns and fallback actions.
