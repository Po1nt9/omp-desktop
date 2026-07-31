# Mock/Real-Runtime E2E 设计（Plan 10 · AC-2.9 / AC-7.1 + 真机探测层）

- **日期**：2026-07-31
- **状态**：已批准（用户缺席先例：决策采用推荐默认值，记录于此）
- **范围**：OMP Desktop Plan 10 验收 —— 解除 v1 transport 注入后标注 "只需 real-Runtime E2E 即可翻转" 的矩阵项中**可在无真机/无 LLM 条件下自动化**的部分
- **上游**：`docs/release/1.0-acceptance-matrix.md`（AC-2.9、AC-7.1 unblocker 原文）、`docs/release/test-coverage-audit.md:76`（端到端能力协商 gap）、Plan 7 final review deferred item（mock E2E happy-path）

---

## 1. 背景与问题陈述

v1 transport 注入（2026-07-31）后，矩阵审计结论（`1.0-acceptance-matrix.md:325`）：
AC-1.2/1.3/1.8/1.9、AC-5.1/5.2 等项 "now need a real-Runtime E2E to flip, not new plumbing"。
两个显式 BLOCKED 项：

- **AC-2.9**（E2E happy-path, mock Runtime）：unblocker 原文 "author a mock-Runtime E2E: session create → prompt → response → permission approval → tool exec → turn end"。自 Plan 7 final review 起 deferred。
- **AC-7.1**（崩溃后不 auto-replay）：unblocker 原文 "real-sidecar crash injection test (detection + unknown/interrupted marking now implemented and unit-tested via AC-1.10 wiring; the remaining gap is killing a live Runtime process mid-turn in an automated harness)"。

现状约束：

1. `AcpClient::spawn_with_options` 固定 spawn `<binary> acp --stdio`（`acp_client.rs:230 build_spawn_command`），任何替代 Runtime 必须容忍这两个 argv。
2. wire shape 由 hand-rolled builders + golden fixtures（`tests/fixtures/acp/` 7 个文件）钉住，`acp_golden_test.rs` 8 测试守护。
3. 现有 `mock_acp.rs` 是 PR2 时代的**进程内 UI stub**（connect/stream/stop 假流），不是 ACP 协议 server，无法走真实 spawn 路径。
4. remote_im `Engine::with_binary_path`（`engine.rs:223`）已是现成的测试注入点。
5. SessionManager 回合路径依赖 `AppHandle` emit，headless 单测不可驱动（permission host-gate 测试先例）。
6. 本机装有真实 Runtime：`omp 17.1.3`（/opt/homebrew/bin/omp）；CI 无。

## 2. 调研结论（先调研再自建）

委托 Librarian 调研 ACP 生态（agentclientprotocol 组织）：

| 候选 | 结论 |
|---|---|
| 官方 `testy`（`agent-client-protocol-test` crate 的 bin） | 功能匹配（initialize/session/new/prompt/cancel + request_permission + 场景脚本），但 `publish = false` 只能 git 依赖；把整个 acp crate v2 依赖树拉进 src-tauri；**argv 不兼容**（不认识 `acp --stdio`，需 wrapper）；与 golden fixtures 形状有漂移风险 |
| 社区 mock/echo agent | **不存在**。搜索全部指向 testy 或 TS `TestAgent` |
| TS SDK `@agentclientprotocol/sdk` `TestAgent` | ~50-80 行可搭，但把 Node 运行时引入 Rust 测试链路；argv 问题同样存在 |
| **自研 scriptable stdio mock** | ~200 行 Rust bin，仅 serde_json（已有依赖）；回复直接取 golden fixtures（single source of truth）；天然容忍 argv |

协议版本：stable = v1（`protocolVersion: 1`），与本项目 wire 一致。

## 3. 决策

- **D1：mock Runtime 自研**（方案 B）。`src-tauri` 新增 `[[bin]] mock-acp-runtime`，回复取自 `tests/fixtures/acp/` golden fixtures。拒绝 testy（git dep + argv + 漂移）与 TS 路径（Node 入链）。
- **D2：E2E 两层**。①AcpClient 级：真实 spawn mock → initialize → session/new → prompt → 流式 session/update（含 tool_call 变体）→ permission 请求/批准 → done，覆盖 AC-2.9 核心回路。②remote_im Engine 级：`with_binary_path(mock)` 注入后 `engine.handle` 驱动完整 Hub 回合（入站消息 → Runtime 回合 → 出站回复），即 Plan 7 deferred 的 mock E2E 本意。SessionManager 级排除（AppHandle 限制，先例）。
- **D3：AC-7.1 崩溃注入以 mock 进程为靶**。spawn mock → hang 场景 prompt → 杀掉子进程 → 断言：pending 全部失败排空（`fail_all_pending`）、Host 不 auto-replay、AC-1.10 接线的恢复路径将回合标记为 interrupted。对 Host 而言 mock 是真实 stdio sidecar 进程，kill 路径与真 Runtime 完全一致。
- **D4：真机层 env-gated 自动跳过，永不为 CI 门禁**。`OMP_E2E_REAL=1`（或自动探测 omp binary）才运行：真 `omp acp --stdio` initialize 握手（无需 LLM）断言 protocolVersion/capabilities；诚实探测 v1 扩展方法应答并记录证据。LLM prompt 回合层用第二个 flag（`OMP_E2E_LIVE=1`），仅手动/本地，不进任何门禁。
- **D5：矩阵翻转纪律**。确定翻转：AC-2.9 → PASS、AC-7.1 → PASS。证据升级（不翻状态）：AC-1.1（能力协商端到端腿）。探测后诚实记录：AC-1.9/AC-5.1/AC-5.2（取决于真 Runtime 对 v1 方法的应答；Runtime 未实现则保持原判定并写明探测结果）。AC-1.2/1.3/1.8 不在本包（queue/steer/credential/resolveMedia 的 v1 语义验证超出 happy-path 范围）。
- **D6：mock 场景集 = 3 个**。`happy`（fixture chunks + tool_call update + done）、`permission`（发 session/request_permission，等 Host 结果后继续）、`hang`（永不回复）。malformed/robustness 场景 YAGNI 砍掉（golden 套件已守 decode 健壮性）。

## 4. 架构

```
src-tauri/
  src/bin/mock_acp_runtime.rs     # [[bin]] mock-acp-runtime（D1）
  src/e2e_runtime.rs              # #[cfg(test)] E2E harness 辅助（spawn mock、收集事件、kill 辅助）
  tests/fixtures/acp/             # 现有 fixtures（回复源）+ 可能新增 tool_call 场景 fixture
```

### 4.1 mock-acp-runtime 行为

- 启动忽略全部 argv（容忍 `acp --stdio`）。
- 逐行读 stdin（JSON-RPC 2.0，newline-delimited，与 AcpClient 传输一致）。
- 方法分派：
  - `initialize` → 回 `handshake_initialize.json` 的 agentResponse（原样）。
  - `session/new` → 回新 sessionId（uuid 或固定串，按 fixture 形状）。
  - `session/prompt` → 按场景（env `MOCK_ACP_SCENARIO` 或 prompt 文本前缀 `scenario:` 选择，默认 happy）：
    - `happy`：按 `stream_chunks.json`/`mock_stream.json` 顺序发 `session/update` 通知（含 tool_call 生命周期变体），最后回 prompt result（stopReason end_turn）。
    - `permission`：先发 `session/request_permission`（`permission_request.json` 形状），阻塞等 Host 的 JSON-RPC result，批准后发剩余 chunks + done。
    - `hang`：不回复、不发通知，直到被杀。
  - `session/cancel` → 回 ack（`stop_cancel.json` 形状），终止当前流。
- 未知方法 → JSON-RPC method_not_found。
- 一切输出走 stdout 仅协议帧；诊断走 stderr（不污染协议）。

### 4.2 E2E harness（`src/e2e_runtime.rs`，cfg(test)）

- `mock_runtime_path()` → `env!("CARGO_BIN_EXE_mock-acp-runtime")`。
- `spawn_mock(scenario) -> AcpClient`（`spawn_with_options`，binary_path=mock，env 注入场景）。
- 事件收集器：drain `AcpEvent` 流为 Vec 供断言。
- `kill_child(&mut AcpClient)`：杀掉子进程（AcpClient 需暴露 test-only 的 child handle 访问——若已有 ProcessExited 路径测试辅助则复用）。

### 4.3 测试清单（TDD 任务分解在 plan 中细化）

| # | 测试 | 覆盖 |
|---|---|---|
| E1 | mock happy：initialize→new→prompt→chunks→done，事件序列与 fixtures 一致 | AC-2.9 核心 |
| E2 | mock permission：request_permission 到达 Host permission 管道，批准后回合完成 | AC-2.9 permission 腿 |
| E3 | mock tool_call：tool_call update 变体（pending/running/completed）decode | AC-2.9 tool exec 腿 |
| E4 | engine 级全回合：`with_binary_path(mock)` + handle 入站消息 → 出站回复文本聚合 | AC-2.9 Hub 腿（Plan 7 deferred） |
| E5 | 崩溃注入：hang 场景 prompt 中 kill → pending 失败排空、无 auto-replay | AC-7.1 |
| E6 | 崩溃后恢复：journal write-ahead + recover → turn_interrupted 标记（复用 AC-1.10 断言路径） | AC-7.1 |
| E7 | 真机握手（`OMP_E2E_REAL=1` gate）：真 omp initialize → protocolVersion=1 + capabilities 非空 | AC-1.1 证据 |
| E8 | 真机 v1 探测（同 gate）：v1 方法逐一 probe，应答记录为矩阵证据 | AC-1.9/5.1/5.2 证据 |

E7/E8 默认 skip（env 未设即 pass-with-skip 日志），CI 行为不变。

## 5. 错误处理与边界

- mock 进程异常退出（非 hang 场景）→ harness panic 并带 stderr 捕获内容（可诊断）。
- E2E 测试超时：每测 `tokio::time::timeout`（10s 上限，hang 场景 kill 后 5s），防 CI 挂死。
- trace-capture 类共享 callsite 风险：本包不新增 trace 捕获测试；如需日志断言，遵守 trace.rs `global_events()` 规则（共享 callsite 禁 scoped capture）。
- `[[bin]]` 不进 Tauri bundle：tauri.conf.json bundle 仅打包主 binary，实现时验证 `bundler` 配置不受影响（externalBin 不变更）。
- Windows 兼容：mock 纯 stdio/serde_json，无平台 API；kill 用 tokio Child::kill 跨平台。

## 6. 非目标

- LLM 真机回合（`OMP_E2E_LIVE` 仅留 flag 位，本包不实现测试）。
- AC-1.2/1.3/1.8 的 v1 语义 E2E（queue/steer/credential/resolveMedia）。
- SessionManager/UI 层 E2E（AppHandle 限制）。
- malformed/协议模糊性测试（golden 套件已覆盖 decode 面）。
- Playwright/webdriver 类 UI 自动化。

## 7. 成功标准

1. AC-2.9、AC-7.1 矩阵行 → PASS，证据指向本包测试。
2. `cargo test --lib` 全绿含 E1-E6；E7/E8 默认 skip；`OMP_E2E_REAL=1 cargo test` 在本机真跑 E7/E8。
3. 零新 runtime 依赖（dev 也不加 git dep）。
4. pnpm test / typecheck / 4 check 门不因本包漂移。
5. test-coverage-audit 计数与 gap 表同步。

## 8. 自审记录

- 占位符扫描：无 TBD/TODO。
- 一致性：D2 两层与 §4.3 E1-E6 一一对应；D4 gate 与 E7/E8 一致；D5 翻转纪律与 §7.1 一致。
- 范围：单 plan 可承载（预估 5-6 个 TDD 任务）。
- 歧义消解：①"real-sidecar crash injection"（AC-7.1 原文）解释为"真实 spawn 的 sidecar 进程"，mock 二进制满足（D3 已论证）；②CARGO_BIN_EXE 在 lib 单测可用性——实现任务首个 checkpoint 验证，不可用则回退 `cargo build` 后定位 target/debug/mock-acp-runtime。
