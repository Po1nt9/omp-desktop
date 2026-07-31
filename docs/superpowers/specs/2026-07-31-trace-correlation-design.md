# AC-1.13 Host+Hub Trace Correlation — 设计

日期：2026-07-31
状态：已批准（用户未答 → 采用推荐默认值，与 AC-8.4/AC-1.5 同一先例）
验收矩阵行：AC-1.13「Trace Correlation: Desktop Host + Remote Hub scope correlation (mandatory; end-to-end optional)」，验证方式 Contract tests (Host+Hub scope)。

## 1. 问题

设计 §13（specs/2026-07-28-omp-desktop-design.md:254-258）：`traceId` 需要在请求、事件边界传播；**若 Runtime 未提供，只保证 Desktop Host 与 Remote Hub 自身范围内关联**（必选），贯穿 Runtime/tool/subagent/MCP/Provider 的 propagation 是可选扩展（§91 明确不阻挡 1.0）。

现状：`grep -rn "trace_id|correlation|otel|opentelemetry" src-tauri/src` 零匹配。tracing 基建完整（`logging.rs:21-61`，EnvFilter + stderr/app.log 双 fmt layer，100+ 日志调用），但**没有任何 span/instrument**——一条入站消息或一个 prompt turn 的日志散落在 5+ 个 tokio 任务边界两侧，无法关联。矩阵据此判 FAIL。

「Remote Hub」按 [[omp-desktop-remote-hub-architecture]] 的定论映射为今日已落地的 `remote_im` Engine/Bridge（Hub 本体被判过度设计、从未建造）。

## 2. 调研结论（先调研再自建）

| 方向 | 事实 | 结论 |
|---|---|---|
| tracing span field + `Instrument` | tokio-rs/tracing 官方推荐；Vector（20k+★）用 span field 做组件级关联；tower-http TraceLayer 每请求一 span；`tracing`/`tracing-subscriber` 已在依赖中 | **采用**，零新运行时依赖；100+ 现有日志调用自动继承 span 字段，无需逐行改 |
| OpenTelemetry（tracing-opentelemetry） | +45 个 Cargo.lock 包（约 +7%）；无 collector 的桌面应用里管道空转 | 拒绝。W3C traceparent 是将来 ACP 协议加 trace context 时的事，届时换 output layer 即可，span 字段名不变 |
| 专用 correlation-id crate | crates.io 无活跃候选（tracing-request-id 不存在、correlation-id 不存在；axum/hyper-trace-id 太窄太新） | 拒绝。所谓「crate」就是 10 行 span 包装 |
| 测试断言库 | tracing-fluent-assertions 仅 7★ | 拒绝微依赖，自研 ~50 行 dev-only capture Layer（供应链卫生） |

## 3. 决策

- **D1 机制 = span field `trace_id` + Instrument 传播**。每个工作单元创建一个 `info_span!`，携带 `trace_id`（及既有上下文字段）；跨 `.await` 用 `.instrument(span)`，跨 `tokio::spawn` 用克隆的 span instrument 子 future。fmt layer 默认 full 格式会把 span 栈连同字段打印在每行日志前（`turn{trace_id=…}: …`），**日志输出零配置变化**。
- **D2 出生点（每 scope 一个）**：
  - **Hub（remote_im）**：`runtime.rs` pump `recv` 之后、`Engine::handle` 之前。所有渠道 connector 经 mpsc 单点汇聚于此，是唯一的全渠道出生点；控制消息（inline handle）与自由文本（detached spawn）两条路径同一 instrument 包装。
  - **Host（桌面）**：`SessionManager::send_message`（session_manager.rs:4597）每个用户 prompt turn 一个 trace_id；prompt 任务 spawn（:4802）用该 span instrument。connect/permission 等日志在 turn span 存活期间自动携带。
- **D3 id 与字段命名**：`Uuid::new_v4().to_string()`（uuid v4 已在 Cargo.toml:39，全仓库 id 生成惯例）；span 字段名 **`trace_id`**（对齐设计 §13 `traceId`）；span 名 `remote_msg`（Hub）/ `turn`（Host）。
- **D4 范围 = 日志关联 + contract tests**。交付：(a) 新模块 `src-tauri/src/trace.rs`（`new_trace_id()`、`turn_span()`、`remote_msg_span()`）；(b) 两 scope 出生点 + 传播点接线；(c) 自研 capture Layer 的 contract tests 证明同一工作单元跨 spawn/mpsc 边界的所有日志携带同一 `trace_id`。
- **D5 非目标**（写进矩阵证据，防误读）：Runtime 内部/tool/subagent/MCP/Provider 传播（设计标可选）；ACP 线上加 traceparent；诊断页 UI 展示 trace scope（§15 诊断页属 AC-1.9 范畴）；DesktopV1Error 加 traceId 字段（tracing 不支持从当前 span 反读字段值，需显式传参，超出本项验收方式）。

## 4. 设计细节

### 4.1 `trace.rs`（新模块，~60 行）

```rust
//! AC-1.13: Host+Hub scope trace correlation (design §13).
use tracing::Span;

/// One work unit = one trace id (uuid v4, codebase idiom).
pub fn new_trace_id() -> String { uuid::Uuid::new_v4().to_string() }

/// Host scope: one user-prompt turn.
pub fn turn_span(trace_id: &str, session_id: &str) -> Span {
    tracing::info_span!("turn", trace_id, session_id = %session_id)
}

/// Hub scope: one inbound channel message.
pub fn remote_msg_span(trace_id: &str, channel: &str, message_id: &str) -> Span {
    tracing::info_span!("remote_msg", trace_id, channel = %channel, message_id = %message_id)
}
```

### 4.2 Hub 接线（remote_im）

- `runtime.rs` pump（:114-147）：recv 后生成 `trace_id`，构造 `remote_msg_span(trace_id, msg.channel, msg.message_id)`；quick 分支（:139 `e.handle(msg).await`）与自由文本分支（:141-143 detached spawn）都以 `.instrument(span)` 包装。Engine 不变——handle 内既有日志（replay/dedup/rate/reply，engine.rs:279-443）自动落在 span 上下文里。
- `engine.rs` `run_agent_turn` 的 ACP event collector spawn（:1017-1049）：`tokio::spawn(async move {…}.instrument(Span::current()))`——`Span::current()` 在 spawn 调用点对已 instrument 的 handle future 求值，即携带 trace_id 的 span。

### 4.3 Host 接线（session_manager）

- `send_message`（:4597）：生成 `trace_id` + `turn_span(trace_id, session_id)`；turn 任务 spawn（:4799-4824，:4802）以该 span instrument。spawn 之前的同步段（journal append :4701-4714 等）用 `span.in_scope(|| …)` 或 `_enter` 守卫覆盖。
- ACP 层（acp_client prompt/event pump）不改：Host 侧 ACP 事件经 session_manager 既有事件循环回流，处于同一 turn 任务上下文；Runtime 进程内部无 trace（D5 非目标）。

### 4.4 Contract tests（自研 capture Layer，dev-only）

`trace.rs` 的 `#[cfg(test)]` 内实现 `CaptureLayer`（`tracing_subscriber::Layer`：`on_event` 记录 `(event level, 当前 span 栈的 trace_id 字段值)` 到 `Mutex<Vec<…>>`；`on_new_span` 捕获 span 字段）。用 `tracing::subscriber::with_default` 局部安装，不碰全局 subscriber。测试矩阵：

| # | 测试 | 证明 |
|---|---|---|
| 1 | `span_field_inherited_by_events`：span 内 `tracing::info!` | 事件携带 trace_id |
| 2 | `trace_id_survives_await`：`.instrument(span)` 的 future `.await` 跨 yield 点 | await 传播 |
| 3 | `trace_id_survives_tokio_spawn`：spawn 克隆 span instrument 的子任务 | spawn 传播（Hub collector / Host turn 任务同构） |
| 4 | `trace_id_survives_mpsc_boundary`：producer 生成 id 经 mpsc 发送、consumer 构造 span 再打日志 | Hub pump 出生点同构 |
| 5 | `distinct_units_get_distinct_trace_ids`：两个单元 | 不串号 |
| 6 | `runtime_pump_births_trace_per_message`（remote_im/runtime 测试模块）：capture Layer + 直接驱动 pump 分支逻辑（或抽出 `spawn_handle_task(msg, engine, trace_id)` helper 后测 helper） | Hub 出生点真实接线 |
| 7 | `engine_handle_logs_carry_trace_id`（remote_im/engine 测试）：capture Layer 包住一次 `Engine::handle`（复用现有 mock channel 基建），断言 begin/reply 日志携带同一 trace_id | Hub scope 端到端 |
| 8 | `send_message_turn_logs_carry_trace_id`（session_manager 测试）：对 turn-span helper 级别的接线断言（mock_acp 路径或 helper 抽取） | Host scope 接线 |

测试 1-5 证机制，6-8 证接线。6-8 的具体形态在 plan 阶段按现有测试基建（mock channel / sample_live 夹具）确定最小改动的接法——若 handle/send_message 无法在无 AppHandle 环境直接驱动，则把「span 构造 + instrument 包装」抽成可测 helper，production 调用点保持一行包装。

## 5. 验收

- `cargo test --lib` 全绿（预计 +8~10）；`pnpm test`、typecheck、4 check 门不回归。
- `grep -c "trace_id" src-tauri/src` 从 0 → 实质非零（机制 + 接线 + 测试）。
- 矩阵 AC-1.13 FAIL→PASS，证据引用本 spec 与测试清单；SA-* 相关行（若有）同步。
- 非目标（D5）逐项写进矩阵证据，end-to-end Runtime propagation 保持可选、不宣称。

## 6. 风险与回退

- **fmt 输出体积**：span 前缀让每行日志变长（`turn{trace_id=…session_id=…}:`）。既有 `session = %meta.id` 内联字段与 span 字段会重复出现——可接受（冗余利于 grep）；如嫌吵，后续可把重复的内联 session 字段删掉（非本项范围）。
- **risk: pump 双分支漏 instrument 其一** → 控制消息日志无 trace_id。测试 6 覆盖两分支。
- **回退**：单模块 + 调用点包装，git revert 即回到现状，无数据/协议迁移。
