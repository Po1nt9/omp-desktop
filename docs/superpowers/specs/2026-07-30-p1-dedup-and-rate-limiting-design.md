# P1: 去重 + 限流 (Dedup + Rate Limiting) — Design Spec

- **日期**: 2026-07-30
- **工作包**: P1（Remote IM 入站消息去重与请求速率限流）
- **状态**: Draft
- **前置**: Remote IM Runtime Bridge 已合并 main（`65ba830..16f6aaf`），remote_im 引擎可驱动真实 OMP Runtime
- **关联**: Master design §14.2（渠道验收去重/限流必测项）、`2026-07-30-remote-im-runtime-bridge-design.md`（排除项）

## 1. 背景与问题

IM 平台的 webhook 常因网关超时而重试，多实例部署也会导致同一 `message_id` 被多个 bridge 实例收到。当前 remote_im 引擎：

- **无去重**: `IncomingMessage.message_id` 仅用于 reply_to 路由，不缓存。重复消息会触发重复 agent turn，浪费 Runtime 资源、对用户造成重复回复。
- **无限流**: `in_flight` 是 per-scope 串行化守卫（同 chat+sender 排队），不是限流器。无法防御高频轰炸、恶意刷量、失控循环。

Master design §14.2 将"去重、限流/退避"列为所有正式渠道的共同必测项。本 spec 实现 P1 工作包，解阻 Plan 8 渠道验收。

## 2. 需求（已确认）

| # | 需求 | 决策来源 |
|---|------|---------|
| R1 | 去重必须跨进程重启生效 | 用户确认（AskUserQuestion） |
| R2 | 限流采用请求速率限流（固定/滑动窗口），非并发上限 | 用户确认 |
| R3 | 限流阈值用内置默认值，不暴露到 Settings/前端 | 用户确认 |
| R4 | 范围仅 remote_im 引擎入站路径，不改 channel 适配器、不改前端 | YAGNI |
| R5 | 工作包独立 spec→plan→实现循环，不与 P2/跨设备同步合并 | 用户确认 |

## 3. 架构

两个新模块，各司其职、可独立测试，均接在 `Engine::handle()` 入口最前端，互不依赖：

```
IncomingMessage 流入
        │
        ▼
┌─ Engine::handle(msg) ────────────────────────────┐
│  1. dedup.check_and_mark(channel, message_id)     │  ← 重复则 return
│  2. rate_limiter.check(channel, scope_key)        │  ← 超限则 return
│  3. [现有逻辑: scope 解析 → 快捷命令 / agent turn] │
└───────────────────────────────────────────────────┘
```

### 模块划分

| 模块 | 文件 | 职责 | 持久化 |
|------|------|------|--------|
| `DedupStore` | `src-tauri/src/remote_im/dedup_store.rs` | 跨重启消息去重 | SQLite |
| `RateLimiter` | `src-tauri/src/remote_im/rate_limiter.rs` | 请求速率限流 | 纯内存 |

## 4. 组件设计

### 4.1 DedupStore（去重）

**存储**: SQLite，复用项目已有的 `rusqlite = { version = "0.32", features = ["bundled"] }` 依赖，零新增依赖。

```sql
CREATE TABLE IF NOT EXISTS seen_messages (
    channel    TEXT NOT NULL,
    message_id TEXT NOT NULL,
    seen_ts    INTEGER NOT NULL,          -- Unix 秒
    PRIMARY KEY (channel, message_id)
);
CREATE INDEX IF NOT EXISTS idx_seen_ts ON seen_messages(seen_ts);
```

- **Key**: 复合主键 `(channel, message_id)`。channel 字段隔离不同 IM 平台，避免跨平台 message_id 碰撞。
- **原子去重**: `INSERT OR IGNORE INTO seen_messages VALUES (?, ?, ?)`，通过 `Connection::changes()` 判断。`=0` 表示该 key 已存在（重复），`=1` 表示新消息。
- **路径**: `crate::paths::app_data_root().join("remote").join("dedup.sqlite")`，与现有 `scope-bindings.json` 同目录。
- **连接管理**: `rusqlite::Connection` 包在 `parking_lot::Mutex<Connection>` 内（rusqlite 非线程安全），外层 `Arc`，与 `SessionStore` 的 `Arc<Mutex<HashMap>>` 范式一致。
- **TTL**: 默认 7 天。`cleanup()` 执行 `DELETE FROM seen_messages WHERE seen_ts < ?`。触发策略：内部 `AtomicU64` 计数器，每 1024 次 insert 触发一次 cleanup（节流，避免每条消息都扫表）。
- **测试变体**: `ephemeral()` 使用 `:memory:` 数据库，零文件副作用。

**API**:
```rust
pub struct DedupStore {
    conn: Arc<Mutex<Connection>>,
    insert_count: AtomicU64,
}

impl DedupStore {
    pub fn open_default() -> Self;            // app_data_root/remote/dedup.sqlite
    pub fn open(path: PathBuf) -> Self;       // 建表 + 建索引
    pub fn ephemeral() -> Self;               // :memory:，测试用
    /// true = 新消息放行; false = 重复丢弃
    pub fn check_and_mark(&self, channel: &str, message_id: &str) -> bool;
}
```

### 4.2 RateLimiter（限流）

**算法**: 固定窗口计数器（fixed window）。简单、低开销、内存常量、对 IM 场景精度足够。

**两层维度**:
- **per-channel**: 防止单个 IM 平台整体轰炸。默认 60 条/分钟。
- **per-scope**: 防止单聊高频轮次。默认 10 轮/分钟。scope_key 沿用 `SessionStore::scope_key(channel, instance, chat, sender)`。

**数据结构**（纯内存）:
```rust
pub struct RateLimiter {
    inner: Mutex<HashMap<String, WindowCounter>>,
    window_secs: u64,       // 默认 60
    channel_limit: u32,     // 默认 60
    scope_limit: u32,       // 默认 10
}

struct WindowCounter {
    window_start: Instant,
    count: u32,
}
```

- **检查**: `check(channel, scope_key) -> bool`。对 channel 和 scope_key 各维护一个 counter key（前缀区分：`"ch:{channel}"` / `"sc:{scope_key}"`）。窗口过期（`now - window_start >= window_secs`）则重置计数。两层都通过才放行。
- **超限动作**: 记录 `tracing::warn!(target: "remote_im::rate_limit", ...)` + **静默丢弃**（直接 return，不回消息）。理由：给被限流方回消息会制造回声/放大效应，且被限流方往往是恶意或失控来源。
- **无持久化**: 限流状态重启清空。可接受——重启罕见，且 per-scope 串行化（`in_flight`）仍在兜底。HashMap 无界增长风险由窗口过期重置缓解：过期窗口的 key 在 cleanup 时惰性移除（见 §4.3）。

**API**:
```rust
impl RateLimiter {
    pub fn new_default() -> Self;
    pub fn new(window_secs: u64, channel_limit: u32, scope_limit: u32) -> Self;
    /// true = 放行; false = 超限丢弃
    pub fn check(&self, channel: &str, scope_key: &str) -> bool;
}
```

### 4.3 过期窗口重置（RateLimiter）

`check()` 的核心逻辑天然处理过期：取目标 key 的 counter，若 `now - window_start >= window_secs`，直接重置 `window_start = now; count = 0`，然后递增。**无需单独的清理遍历**——不活跃的 channel/scope 的 counter key 会留在 map 里，但其内存有界（bounded by `活跃channel数 × 活跃scope数`），不会无限增长。这符合 YAGNI，且避免了持锁全量遍历的性能开销。

## 5. Engine 集成

### 5.1 新增字段

```rust
pub struct Engine {
    // ... 现有字段不变 ...
    dedup: DedupStore,
    rate_limiter: RateLimiter,
}
```

### 5.2 构造函数

- `Engine::new(...)`: 初始化 `DedupStore::open_default()` + `RateLimiter::new_default()`
- `Engine::new_ephemeral(...)`: 初始化 `DedupStore::ephemeral()` + `RateLimiter::new_default()`

### 5.3 handle() 入口改动

在 `handle(&self, msg: IncomingMessage)` 最前端（第 139 行，日志之后、scope 解析之前）插入：

```rust
pub async fn handle(&self, msg: IncomingMessage) {
    tracing::info!(...);  // 现有日志

    // ── P1: 去重 ──
    if !self.dedup.check_and_mark(&msg.channel, &msg.message_id) {
        tracing::debug!(
            target: "remote_im::dedup",
            channel = %msg.channel, message_id = %msg.message_id,
            "duplicate message dropped"
        );
        return;
    }

    // ── P1: 限流 ──
    let scope_key = SessionStore::scope_key(
        &msg.channel, &msg.instance_id, &msg.chat_id, &msg.sender_id
    );
    if !self.rate_limiter.check(&msg.channel, &scope_key) {
        tracing::warn!(
            target: "remote_im::rate_limit",
            channel = %msg.channel, scope = %scope_key,
            "rate limit exceeded, message dropped"
        );
        return;
    }

    // ── 现有逻辑不变 ──
    // ... scope 解析 → 快捷命令 / agent turn ...
}
```

注意：去重在限流之前。理由——重复消息是最廉价的拦截（一条 SQL），应在限流计数之前剔除，避免重复消息也消耗限流配额。

## 6. 测试策略（TDD）

实现前先写测试。测试与实现同文件内嵌 `#[cfg(test)] mod tests`。

### 6.1 DedupStore 测试

- `test_first_message_passes`: 首次 `check_and_mark` 返回 true
- `test_duplicate_dropped`: 同 `(channel, message_id)` 第二次返回 false
- `test_different_channel_no_collision`: 同 message_id 不同 channel 互不影响
- `test_ephemeral_no_file`: `ephemeral()` 不创建文件
- `test_ttl_cleanup`: 手动插入过期记录后触发 cleanup，记录被删除
- `test_persistence_across_reopen`: 写入后重新 open 同路径，仍能识别重复（跨重启）

### 6.2 RateLimiter 测试

- `test_under_limit_passes`: 窗口内未达限，连续 check 全部 true
- `test_over_limit_dropped`: 达限后 check 返回 false
- `test_window_reset`: 模拟时间前进超过 window_secs，计数重置，再次放行
- `test_channel_and_scope_independent`: channel 达限不影响其他 channel；scope 同理
- `test_expired_window_reset`: 过期窗口的 counter 被重置而非保留旧计数（验证 §4.3 行为）

### 6.3 集成测试（Engine 层）

- `test_duplicate_msg_no_agent_turn`: 经 `handle()` 发两条同 message_id，mock outbound 只收到一条回复（验证去重生效于 turn 层）
- `test_rate_limited_msg_no_agent_turn`: 触发限流后 `handle()` 不产生 outbound（验证限流生效于 turn 层）

测试中模拟时间前进：RateLimiter 内部时间通过**可注入的 `now()` 闭包**获取（`now: Box<dyn Fn() -> Instant + Send + Sync>`），生产用默认 `Instant::now`，测试用受控时钟替换。这避免依赖真实 `tokio::time` 且不污染公共 API（注入通过 `new` 的非公开重载完成）。

## 7. 配置常量

全部内置为常量，集中定义，便于未来提升到 Settings：

```rust
// dedup_store.rs
const DEDUP_TTL_SECS: u64 = 7 * 24 * 3600;   // 7 天
const DEDUP_CLEANUP_INTERVAL: u64 = 1024;      // 每 1024 次 insert 触发一次 cleanup

// rate_limiter.rs
const WINDOW_SECS: u64 = 60;
const CHANNEL_LIMIT: u32 = 60;   // 条/分钟
const SCOPE_LIMIT: u32 = 10;     // 轮/分钟
```

## 8. 范围与非目标

**本 spec 包含**:
- ✅ remote_im engine 入站消息去重（跨重启持久化）
- ✅ per-channel + per-scope 请求速率限流（固定窗口）
- ✅ 内置默认阈值

**本 spec 不包含（非目标）**:
- ❌ 出站消息的去重/限流
- ❌ 基于成本/延迟的自适应动态限流（YAGNI）
- ❌ 滑动窗口/令牌桶（固定窗口对当前场景足够）
- ❌ 修改任何 channel 适配器（适配器照常 send，engine 层统一拦截）
- ❌ Settings/前端配置 UI（阈值内置）
- ❌ P2 媒体收发、跨设备同步（各自独立 spec）

## 9. 验收标准

- [ ] `cargo test -p omp-desktop remote_im::dedup_store` 全绿
- [ ] `cargo test -p omp-desktop remote_im::rate_limiter` 全绿
- [ ] `cargo test -p omp-desktop remote_im::engine` 中新增集成测试全绿
- [ ] `cargo clippy -p omp-desktop` 无新 warning
- [ ] `cargo build -p omp-desktop` 成功
- [ ] 跨重启去重验证：写入 → 重开 → 仍识别重复（测试覆盖）
- [ ] 限流验证：达限丢弃、窗口重置放行（测试覆盖）
