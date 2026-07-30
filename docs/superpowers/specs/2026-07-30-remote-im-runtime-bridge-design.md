# Remote IM Runtime Bridge — Design

- 日期：2026-07-30
- 状态：**设计已确认，待写实现计划**
- 产品名称：OMP Desktop
- 关联文档：[`2026-07-28-omp-desktop-design.md`](./2026-07-28-omp-desktop-design.md) §11.2、§14

## 1. 问题与目标

`remote_im` 模块已是成熟的 outbound gateway（14 渠道适配器、4028 行 Rust、33 个测试、零 Hub 代码），其架构与 Hermes Gateway / cc-connect 同构。但存在一个硬功能缺口：`src-tauri/src/remote_im/engine.rs:699-708` 的 fail-closed 栅栏让所有远程 agent turn 直接返回 `runtime_unavailable`。

**结果**：IM 消息能进出，但无法真正驱动 OMP Runtime 执行 agent turn。"远程 IM 驱动本机 Agent" 这个核心卖点当前不可用。

**目标**：在**不重写**现有架构的前提下，替换 fail-closed 栅栏，让远程 IM 消息能真正调用 OMP Runtime 完成一个完整的 agent turn（open/resume session → prompt → 收集流式回复 → 回写 session_id），并补齐生产必需的去重/限流保护。

**成功标准**：
1. 从任意已配置渠道发一条文本消息 → 本机 OMP Runtime 执行 → agent 回复文本回到 IM
2. 同一会话连续发消息 → 第二条复用同一 agent session（resume，不新建）
3. 无 Runtime binary 时 → 优雅降级返回 `runtime_unavailable`（保持现有行为）
4. 现有 384 个 Rust 测试 + 33 个 remote_im 测试全过，新增测试全过

## 2. 明确不做（排除项）

- ❌ 不推倒重写 14 个渠道适配器
- ❌ 不引入 Hub 中继（现有 outbound 架构已验证可行）
- ❌ 不做跨设备 session 同步
- ❌ 不改会话绑定到 work_dir 的设计
- ❌ 媒体收发、去重/限流是后续工作包，本 spec 仅定义 Runtime 接通

## 3. 关键技术约束（已验证）

调研已确认以下硬约束，设计必须遵守：

| 约束 | 证据 | 设计影响 |
|---|---|---|
| `AcpClient` 是 per-cwd、per-session 的 | `acp_client.rs:172` `cwd: PathBuf` 字段；一个 client = 一个子进程 = 一个 cwd | remote_im 需按 work_dir 管理 AcpClient 池 |
| `AcpClient::prompt()` 在当前 session 上操作 | `acp_client.rs:1255`；`spawn_with_options` 返回 `(client, events_rx)` | 每个 client 有独立事件流需收集 |
| `SessionManager` 不跨 App session 偷进程 | `session_manager.rs:2468` 注释 | remote_im 独立管理自己的进程池，不与 UI 的 SessionManager 共享 |
| `Supervisor`（Plan 3）只管进程生命周期，不暴露通信通道 | `supervisor/mod.rs:53` `child` 藏在 RwLock 里 | remote_im 用 `AcpClient::spawn_with_options` 直接 spawn，不走 Supervisor |
| remote_im 消息泵把自由文本消息 `tokio::spawn` 成独立 task | `runtime.rs:129` | 多个 IM scope 可能并发；需 per-scope 并发保护 |
| `Engine` 当前完全独立，无 `AcpClient` | `engine.rs:39-48` | 需注入 Runtime 依赖 |

## 4. 接口契约（已验证的调用路径）

```rust
// 1. Spawn（参考 session_manager.rs:2661, acp_client.rs:235）
//    binary_path 来自 Settings.manual_cli_path
AcpClient::spawn_with_options(
    cli_path,                          // binary_path 或返回 runtime_unavailable
    cwd,                               // work_dir（PathBuf）
    SpawnOptions { model_id, effort, permission_policy, binary_path, agent_dir }
) -> Result<(Arc<AcpClient>, mpsc::UnboundedReceiver<AcpEvent>)>

// 2. 开/恢复会话（acp_client.rs:996）
acp.initialize_and_open_session(resume_session_id: Option<&str>)
    -> Result<(session_id: String, resumed: bool)>

// 3. 执行 turn（acp_client.rs:1255）
acp.prompt(text: &str) -> Result<(), AgentError>

// 4. 取当前 session id
acp.agent_session_id() -> Option<String>

// 5. 收集流式回复
//    spawn 时返回的 events_rx 收到 AcpEvent::Stream { kind: StreamKind::Assistant, text, .. }
//    累积 text 即 agent 回复
```

## 5. 架构设计

### 5.1 AcpClient 池（核心新增）

`Engine` 持有一个 per-work_dir 的 AcpClient 池。每个 work_dir 对应一个独立的 Runtime 子进程，可被该 work_dir 下的所有 IM scope 复用（不同 IM 会话操作同一项目 = 同一进程的不同 agent session）。

```
Engine
├── runtimes: Mutex<HashMap<PathBuf, Arc<RuntimeEntry>>>
│   └── RuntimeEntry { acp: Arc<AcpClient>, text_buf: Arc<Mutex<String>> }
├── binary_path: Option<PathBuf>      // 来自 Settings.manual_cli_path
├── in_flight: Mutex<HashSet<String>> // per scope_key 并发保护
├── store: SessionStore               // 不变
├── outbound: OutboundRouter          // 不变
└── ... (其余字段不变)
```

### 5.2 run_agent_turn 替换后的流程

替换 `engine.rs:699-708`（fail-closed 栅栏）为：

```
1. 并发检查：scope_key ∈ in_flight? → 回复 "上条消息还在处理中"，return
2. 取 Runtime：get_or_spawn_runtime(work_dir)
   - 无 binary_path → 返回 runtime_unavailable（保持降级）
   - 池命中 → 复用
   - 池未命中 → spawn_with_options + 启动事件收集 task
3. 清空 text_buf
4. open/resume session：acp.initialize_and_open_session(resume_id)
   - resume_id 来自 binding.agent_session_id（现有 resolve_turn_intent 已准备好）
5. 执行 prompt：acp.prompt(content)
6. 收集回复：读取 text_buf → AgentTurnResult.text
7. 回写 session_id：acp.agent_session_id() → AgentTurnResult.session_id
8. （现有逻辑不变）binding_after_agent_turn + sync_turn_to_app + reply
```

### 5.3 事件收集

`spawn_with_options` 返回的 `events_rx` 必须被消费（否则背压会阻塞 Runtime）。为每个 RuntimeEntry 启动一个后台 task：

```
loop {
    match events_rx.recv().await {
        Some(AcpEvent::Stream { kind: Assistant, text, .. }) => text_buf.push_str(&text)
        Some(AcpEvent::Stream { kind: _, .. }) => {} // 忽略非 Assistant 流
        Some(_) => {}                                 // 其他事件忽略
        None => break                                 // client 关闭
    }
}
```

`text_buf` 在每个 turn 开始时清空，prompt 返回后读取。

### 5.4 并发安全

- **per-scope 串行**：同一 `{channel}:{instance}:{chat_id}:{sender_id}` 同时只允许一个 prompt。用 `in_flight` HashSet 保护，进入时插入，退出时移除（含 panic 安全的 guard）。
- **多 scope 并行**：不同 scope 可并行（各自 spawn 或复用不同 work_dir 的 Runtime）。注意同一 work_dir 的多个并发 scope 共享同一 AcpClient——但 AcpClient 内部有 stdin 写锁（`AsyncMutex`），prompt 是串行的。这意味着同 work_dir 的并发 scope 会串行排队。**这是可接受的**：同一个项目同时跑两个 agent turn 本就是异常用法。

### 5.5 降级行为

- 无 `binary_path`（Runtime 未配置）：`get_or_spawn_runtime` 返回 `runtime_unavailable`，`AgentTurnResult.error` 填入，现有第 724-739 行的错误处理路径正常工作（回复 "Error: runtime_unavailable..."）。
- spawn 失败（binary 不存在/无权限）：同上。
- prompt 超时/进程崩溃：`AgentError` → `AgentTurnResult.error`，scope 的 `agent_session_id` 不变（下次重试可 resume）。

## 6. 改动清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/remote_im/engine.rs` | Engine 结构体加 `runtimes`/`binary_path`/`in_flight`；替换 `run_agent_turn` 栅栏；新增 `get_or_spawn_runtime`；事件收集 task；现有 fail-closed 测试改为降级测试 |
| `src-tauri/src/remote_im/runtime.rs` | `start_runtime` 加 `binary_path` 参数，传入 Engine |
| `src-tauri/src/remote_im/bridge.rs` | 从 Settings 读 `manual_cli_path` 传入 `start_runtime` |
| 测试 | 池命中/未命中测试；并发拒绝测试；降级测试 |

## 7. 测试策略

1. **降级测试**（现有第 904-935 行改造）：无 binary → 返回 `runtime_unavailable`
2. **池测试**：同 work_dir 二次调用复用同一 RuntimeEntry；不同 work_dir 各自独立
3. **并发测试**：同 scope 第二个 turn 被拒绝并收到友好提示
4. **集成测试（mock）**：mock AcpClient 返回预设文本，验证 handle() → run_agent_turn → reply 全链路
5. **回归**：`cargo test` 全过，`cargo clippy` 0 warnings

## 8. 风险

| 风险 | 缓解 |
|---|---|
| AcpClient 事件流收集与 prompt 的时序 | prompt 是同步等待完成的 RPC，事件在 prompt 期间由后台 task 累积，prompt 返回后读取——与 SessionManager 的模式一致 |
| Runtime 进程泄漏（IM scope 不再活跃但进程未清理） | 接受短期泄漏（与 SessionManager 不主动清理 parked 进程一致）；后续工作包可加 LRU 淘汰 |
| 同 work_dir 并发 scope 串行排队导致延迟 | 文档化为已知限制；实际场景中同项目并发远程 turn 罕见 |
