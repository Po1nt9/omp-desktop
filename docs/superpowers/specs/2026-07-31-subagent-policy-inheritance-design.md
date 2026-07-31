# AC-1.5 Subagent 策略继承 + Todo lifecycle transport test — 设计

- 日期：2026-07-31
- 矩阵锚点：`docs/release/1.0-acceptance-matrix.md:35`（FAIL）
- 设计锚点：`docs/superpowers/specs/2026-07-28-omp-desktop-design.md:91,226,232,351`
- 流程：superpowers brainstorming → 本 spec → writing-plans → TDD

## 1. 问题

矩阵 AC-1.5 一行的 FAIL 由两个独立缺口组成：

1. **Todo lifecycle**：`todo.list` 类型完备（`src/lib/ompDesktopV1/methods.ts:262`），但
   `OmpDesktopV1Client.call()` 仍 fail-closed（`index.ts:94-97`，注释明确 "Plan 3 will
   inject the transport here"），没有任何 transport 级测试。
2. **Subagent policy inheritance**：Desktop 只有 on/off ——
   `agent_subagents.rs` 只把 `[subagents] enabled = bool` 写进 agent-home
   `config.toml`（且仅 independent mode；shared mode 是 no-op）。
   grep `inherit/permission/policy/propagat` 全仓库零匹配。设计 L226 要求：
   > subagent | parent policy + 显式继承/收窄规则 | 不得比 parent 扩权；MCP/workspace 限制必须继承

## 2. 决策点（自主会话协议：呈现推荐默认值，用户未逐项答复即采纳并记录于此）

### D1 — Desktop 侧继承执行策略：**三层方案（推荐）**

探索发现（改变设计形状的事实）：

- **F1 · on/off kill switch 实际是断的**：`apply_subagents_to_command`
  （`agent_subagents.rs:107`）是死代码（编译器告警 never used）；
  `AcpClient::spawn_with_options`（`acp_client.rs:238-279`）只传 `acp --stdio` +
  `PI_CODING_AGENT_DIR` + `OMP_DESKTOP_V1_PROTOCOL`。**shared mode 下设置开关对
  Runtime 完全无效**（TOML 不写、CLI flag/env 不传）。
- **F2 · 子代理由 Runtime 内部 fork**：Desktop 永不直接 spawn 子代理进程；
  `runtime/oh-my-pi` 不在本仓库 git 跟踪内 → 本次只交付 Desktop 追踪面。
- **F3 · 权限请求携带 kind**：`session/request_permission` 的 `tool_name` =
  `toolCall.kind`（`acp_client.rs:1506-1510`），host 可识别 subagent-spawn 类工具
  （前端已有同类正则 `toolDisplay.ts:55`）。但**子代理自己的工具请求没有
  parent-link 元数据**，无法可靠区分——该区分是 Runtime 职责，不伪造。
- **F4 · v1 client 预留 transport 注入点**：`index.ts:94-97` 注释即 Plan 3 接缝。

三层：

1. **纯函数钳制**（`permission.rs`）：`subagent_effective_policy(parent, configured)`
   按宽松度阶梯取两者中更严格者；`configured = None` → 原样继承 parent
   （不扩权也不缩权）。宽松度阶梯（宽 → 严）：
   `AlwaysApprove > AllowForSession > AcceptEdits > Ask > DontAsk > Deny`。
   `AllowOnce` 是单次授权结果而非会话策略，映射为 `Ask` 后参与钳制。
2. **配置 + spawn 接线**：`[subagents]` TOML 扩展 `policy` / `inherit_mcp` /
   `inherit_workspace` 三个键（把 `set_table_bool` 泛化出 `set_table_string`）；
   **接通死代码** `apply_subagents_to_command` 进 `spawn_with_options`，并扩展其
   签名携带钳制后 policy（env `OMP_SUBAGENT_POLICY=<wire>`）。双通道同语义：
   shared mode 走 CLI flag/env；independent mode 走 TOML。
3. **Host 闸门**（`session_manager.rs` PermissionRequest 分支）：识别
   subagent-spawn kind（`is_subagent_spawn_tool(tool_name)`，镜像
   `toolDisplay.ts:55` 的模式集）——subagents disabled → 自动拒绝
   （TOML/flag 之外的纵深防御）；enabled → 按 parent policy 正常裁决 +
   `tracing::info!` 审计。

否决的备选：
- (b) 仅 TOML 配置同步：太弱，完全依赖 Runtime 自觉，host 无任何保证。
- (c) 拦截所有子代理内部请求：不可行（F3，ACP 请求无 parent-link 元数据，
  需要 Runtime 侧改动，超出本仓库追踪面）。

### D2 — 范围：**整行（推荐）**

todo transport test + subagent 继承一起做，否则矩阵行无法翻转。todo 侧 =
给 `OmpDesktopV1Client` 加 transport seam（构造注入 `(method, params) =>
Promise<unknown>`，默认 None 保持 fail-closed），加 mock-transport round-trip
测试：`todo.list`（五态 lifecycle：pending/in_progress/completed/abandoned/blocked +
sessionId 转发）+ `subagents.status` / `subagents.setEnabled`（Plan 5 已广告的
方法，与 subagent 范围天然协同）。

### D3 — 测试策略：**host 侧全层（推荐）**

- Rust 单元：钳制函数全组合矩阵（7 变体 × 有/无 configured）、TOML 三键 upsert、
  spawn flag/env 接线断言（含"死代码已接通"的回归断言）。
- Rust 集成：`permission_host_test.rs` 模式 —— disabled 时 subagent-spawn 权限
  请求被自动拒绝；enabled 时裁决路径不变。
- 前端 vitest：transport seam round-trip（todo.list 五态、sessionId 转发、
  subagents.status/setEnabled）。
- 不依赖真实 Runtime；Runtime 侧 E2E 仍归 BLOCKED umbrella（矩阵 evidence 注明）。

### D4 — UI：**不新增（推荐）**

行验收只要求 "Contract tests + permission inheritance tests"。
`AppSettings.subagent_policy: Option<String>` 落盘（默认 `None` = 继承 parent），
设置 UI 留给后续 pass；i18n 不动，所有门保持绿。

## 3. 设计细节

### 3.1 策略钳制（permission.rs）

```rust
/// 宽松度阶梯（宽 → 严）。AllowOnce 映射为 Ask。
fn permissiveness_rank(p: PermissionPolicy) -> u8 { ... }

/// AC-1.5：子代理有效策略 = min(parent, configured)。
/// configured=None → 继承 parent（相等，不扩权）。
pub fn subagent_effective_policy(
    parent: PermissionPolicy,
    configured: Option<PermissionPolicy>,
) -> PermissionPolicy { ... }
```

语义保证（全部单测覆盖）：
- `subagent_effective_policy(P, None) == P`（继承）。
- `subagent_effective_policy(P, Some(C))` 的 rank `>= max(rank(P), rank(C))`
  （rank 越大越严格）——**永不比 parent 宽**。
- 交换律：min 与顺序无关。

### 3.2 TOML 配置面（agent_subagents.rs）

```toml
[subagents]
enabled = true
policy = "accept_edits"        # 钳制后上限；缺省 = 继承 parent
inherit_mcp = true             # 子代理继承 parent 的 MCP allowlist
inherit_workspace = true       # 子代理继承 parent 的 workspace/cwd 约束
```

- `set_table_bool` 泛化出 `set_table_string`（同手法：行扫描、表内替换、
  表尾/文末插入）。
- `sync_subagents_to_agent_profile` 扩展签名或新增
  `sync_subagent_policy_to_agent_profile(mode, policy, inherit_mcp, inherit_workspace)`，
  仍 shared mode no-op。
- MCP/workspace 继承键是**声明式约束**：Desktop 保证写出正确值，Runtime 负责
  执行（设计 L226/L232 分工）。`inherit_* = true` 是默认且唯一语义——Desktop
  不提供让子代理脱离 parent MCP/workspace 约束的开关。

### 3.3 Spawn 接线（acp_client.rs + agent_subagents.rs）

- `SpawnOptions` 新增：
  ```rust
  pub subagents_enabled: Option<bool>,   // None = 不动 CLI（默认 enabled）
  pub subagent_policy: Option<String>,   // 钳制后 wire form
  ```
- `spawn_with_options`：调用（现已死代码的）`apply_subagents_to_command` 扩展版：
  ```rust
  pub fn apply_subagents_to_command(
      cmd: &mut tokio::process::Command,
      enabled: bool,
      policy: Option<&str>,
  )
  ```
  disabled → `--no-subagents` + `GROK_SUBAGENTS=0`（现状语义）；
  `policy = Some(p)` → env `OMP_SUBAGENT_POLICY=<p>`。
- `session_manager.rs:2653` spawn_opts 构建点（`prefs.permission_policy` 与
  `settings` 均在场）：计算
  `subagent_effective_policy(parse(prefs.permission_policy), settings.subagent_policy.map(parse))`
  → `as_str()` 填入 `subagent_policy`；`settings.subagents_enabled` 填入
  `subagents_enabled`。
- 这同时修复 F1（shared mode 开关无效）。

### 3.4 Host 闸门（session_manager.rs PermissionRequest 分支）

- 新增 `is_subagent_spawn_tool(tool_name: &str) -> bool`（小写化后匹配
  `subagent` / `spawn_agent` / `spawn_subagent` / 精确词 `agent`，镜像
  `toolDisplay.ts:55`）。放在 `agent_subagents.rs`（Rust 侧单一归属）。
- PermissionRequest 分支在 replay 短路之后、`may_auto_allow` 之前：
  - `is_subagent_spawn_tool(&tool_name) && !settings.subagents_enabled`
    → `respond_permission(reject_once)` + `tracing::warn!` 审计（纵深防御：
    即使 Runtime 忽略 TOML/flag，host 也不放行 spawn）。
  - `is_subagent_spawn_tool && enabled` → `tracing::info!` 审计后走正常裁决。
- LiveSession 需能拿到 `subagents_enabled`：裁决点读 `store::load_settings()`
  太重；在 `LiveSession` 构建时快照一个 `subagents_enabled: bool` 字段
  （与 `policy` 字段同手法）。

### 3.5 前端 transport seam（src/lib/ompDesktopV1/index.ts）

```ts
export type DesktopV1Transport = (
  method: string,
  params: unknown,
) => Promise<unknown>;

class OmpDesktopV1Client {
  private transport: DesktopV1Transport | null = null;
  setTransport(t: DesktopV1Transport | null): void { this.transport = t; }
  // call(): capability + allow-list 检查不变；
  //   transport == null → RUNTIME_UNAVAILABLE（现状 fail-closed 保持）；
  //   否则 const value = await transport(fullMethod, params) 并包装 ok:true。
}
```

测试（`contract.test.ts` 内新增 describe 块，mock transport）：
- `todo.list`：五态 phases round-trip；`sessionId` 参数原样转发到 transport。
- `subagents.status` → `{ enabled, activeCount }` round-trip。
- `subagents.setEnabled({enabled:false})` → 转发参数 + 回显结果。
- transport 抛错 → 映射为 error CallResult（不抛出）。
- 未设 transport + 有 capability → 仍 `runtime_unavailable`（fail-closed 回归）。

### 3.6 设置存储（store.rs）

- `AppSettings.subagent_policy: Option<String>`，`#[serde(default)]`，缺省 None。
- 前端 `AppSettings` 类型同步加可选字段（不设 UI）。

## 4. 非目标

- Runtime 侧继承执行（`runtime/oh-my-pi` 不在 git 追踪面；矩阵 evidence 注明
  real-Runtime E2E 仍 BLOCKED）。
- 子代理内部工具请求的 parent-link 区分（F3，Runtime 职责）。
- 设置 UI / i18n 新键。
- `session/new` 注入 per-subagent MCP allowlist（超出 ACP 现有表面；
  继承语义由 3.2 声明键承载）。

## 5. 测试策略汇总

| 层 | 文件 | 内容 |
|---|---|---|
| Rust 单元 | `permission.rs` | 钳制全组合矩阵、继承恒等、AllowOnce→Ask |
| Rust 单元 | `agent_subagents.rs` | `set_table_string` 三键 upsert、`is_subagent_spawn_tool` 模式集、扩展版 `apply_subagents_to_command` flag/env |
| Rust 单元 | `acp_client.rs` | SpawnOptions 新字段接线到 Command（死代码接通回归） |
| Rust 集成 | `permission_host_test.rs` | disabled → subagent-spawn 请求 reject；enabled → 路径不变 |
| 前端 vitest | `contract.test.ts` | transport seam 五个用例（3.5） |
| 文档 | 矩阵 :35 / test-coverage-audit / security-audit | 翻转 + 计数 + 证据 |

## 6. 验收

- 矩阵 AC-1.5 FAIL → PASS，evidence 记录：钳制函数 + spawn 接线 + host 闸门 +
  TOML 声明键 + transport seam 测试；Runtime 侧 E2E 注明归 real-Runtime BLOCKED
  umbrella。
- 计数重算（grep 总数），SA checklist / coverage audit 同步。
- 全门绿：`cargo test --lib`、`pnpm test`、`pnpm typecheck`、`check:i18n`、
  `check:brand`、`check:provenance`、`check:legal`。
