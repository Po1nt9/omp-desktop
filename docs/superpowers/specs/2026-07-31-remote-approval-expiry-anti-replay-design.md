# AC-8.4 远程审批过期 + 防重放 设计

日期：2026-07-31 ｜ 状态：已批准（默认决策，见 §2）｜ 验收口径：`cargo test` remote_im suite

## 1. 背景与问题

验收矩阵 AC-8.4（§14.2 所有正式渠道共同必测项"审批过期与防重放"）判定 **FAIL**：

- `grep "approval/expire/expiry/nonce/replay"` 在 `remote_im/` 零命中——无任何审批过期/防重放基础设施。
- `allow_remote_yolo` 是持久化全局布尔（`config.rs:313`），但 `Engine` 只存不读（`engine.rs:118`）——**死开关**。
- 远程 turn 的 `SpawnOptions.permission_policy` 恒为 `None`（`engine.rs:901-907`），工具权限完全交给 Runtime 默认策略，IM 侧无审批生命周期。
- `DedupStore` 只做 `(channel, message_id)` 7 天去重，**不做新鲜度校验**：换一个 message_id 的重放 webhook 可绕过全部防护。
- webhook 验签覆盖不对称：仅 wecom（可选 SHA1，`wecom.rs:413-450`）和 LINE（强制 HMAC-SHA256，`line.rs:97-109`）有签名校验，且都**无时间戳新鲜度窗口**。

### 现成方案调研（先调研再自建）

- **Slack 标准模式**（docs.slack.dev/authentication/verifying-requests-from-slack）：签名串含 `X-Slack-Request-Timestamp`，验证方检查与本地时间偏差 ≤ **300s**，超窗视为重放丢弃。这是 webhook 防重放的业界基准。
- **审批 TTL 先例**：sudo `timestamp_timeout`（授权窗口 15min 后需重新认证）——内存态、到期失效、不跨会话。
- 无可采纳的现成 Rust crate/库：机制约 150 行，自建于 `remote_im/` 内即可。

### Master design 约束

- §14.2（`omp-desktop-design.md:288`）：所有正式渠道必测"审批过期与防重放"。
- §11（:212）：`allow_always`/`reject_always` 属受管会话内存态，**未协商持久 capability 时不跨重启保存** → 审批状态必须内存化、重启失效。
- §11.2（:236）：远程审批不要求 PIN，文档建议平台 MFA + 严格白名单（不在本工作包）。

## 2. 设计决策（用户未答 → 采用推荐默认值）

| # | 决策 | 采用 | 备选（未选原因） |
|---|---|---|---|
| D1 | 审批语义 | **全局 yolo 开关 + TTL 自动过期**：启用挂内存 `expires_at`（默认 3600s），到期/重启自动失效 | per-scope slash 授权（链路长、测试面大）；双轨（实现量最大，YAGNI） |
| D2 | 防重放范围 | **webhook 渠道强制**（wecom/LINE/generic 有 ts/nonce 时校验）；WS/长轮询渠道依赖平台认证传输 + 现有 dedup，文档化 | 全渠道统一（多数渠道消息体无可靠时间戳，假安全感） |
| D3 | 权限接线 | **接线**：审批有效时 `SpawnOptions.permission_policy = Some("yolo")`，否则 `None` | 仅 IM 侧 TTL 不接 Runtime（审批不影响实际权限，验收证据弱） |

## 3. 架构

```
IncomingMessage { …, timestamp: Option<i64>, nonce: Option<String> }   // types.rs，serde default

Engine::handle(msg)                                     // engine.rs:204
  ├─ entry log
  ├─ ★ ReplayGuard::check(channel, ts, nonce, now)      // 新增，replay_guard.rs
  │     Stale | Replayed → tracing::warn! + drop（不回复，避免 oracle）
  ├─ dedup.check_and_mark                               // 现有
  ├─ rate_limiter.check                                 // 现有
  └─ … run_agent_turn
        SpawnOptions {
          ★ permission_policy: self.effective_permission_policy(),  // Some("yolo") if approval active
          …
        }
```

### 3.1 ReplayGuard（`remote_im/replay_guard.rs`，新模块）

```rust
pub const DEFAULT_FRESHNESS_WINDOW_SECS: i64 = 300;   // Slack 标准
pub enum ReplayVerdict { Allow, Stale, Replayed }

pub struct ReplayGuard { inner: Mutex<HashMap<String, i64>>, window_secs: i64 }
// key = "channel|nonce"，value = 过期时刻（unix secs）

impl ReplayGuard {
    pub fn new(window_secs: i64) -> Self;
    pub fn check(&self, channel: &str, timestamp: Option<i64>,
                 nonce: Option<&str>, now: i64) -> ReplayVerdict;
}
```

语义（严格、可单测）：

1. `timestamp` 存在且 `|now - ts| > window` → **Stale**（未来超窗同样拒绝——时钟伪造）。
2. `nonce` 存在：缓存中有未过期条目 → **Replayed**；否则记录 `expires_at = now + window`。
3. 两者皆无 → **Allow**（WS/长轮询渠道；传输层已被平台认证，见 §5 文档化要求）。
4. ts+nonce 都有：先 freshness 后 nonce。
5. 惰性清理：每次插入后若 `len > 1024` 则全表扫除过期项。

### 3.2 渠道填充 ts/nonce

| 渠道 | timestamp | nonce | 说明 |
|---|---|---|---|
| wecom（webhook） | query `timestamp` | query `nonce` | `parse_wecom_sig`（wecom.rs:369-390）已解析，接进 IncomingMessage |
| LINE（webhook） | 无 | `X-Line-Signature` 头 + 每事件 `replyToken`（`"{sig}:{reply_token}"`） | 签名 = HMAC(body)；同一 POST 可含多事件（共享签名），故 nonce 必须含事件级判别符；重放 POST → 同 sig+token 对 → 缓存拦截（无 ts 无法做 freshness，文档化） |
| generic | 无 | 无 | 放行（v1 不伪造能力） |
| feishu/telegram/discord/slack/dingtalk/qq/qqbot/matrix/weixin/weibo/wps | 无 | 无 | WS/长轮询，平台认证传输 + dedup 覆盖 |

### 3.3 审批过期（engine/bridge/config/mod）

```rust
pub const DEFAULT_APPROVAL_TTL_SECS: i64 = 3600;

// Engine 内
approval: Mutex<Option<i64>>,            // expires_at；None = 未授权/已过期
fn grant_approval(&self, ttl_secs: i64); // expires_at = now + ttl
fn revoke_approval(&self);               // expires_at = None
fn approval_active(&self) -> bool;       // expires_at.is_some_and(|t| now < t)
fn effective_permission_policy(&self) -> Option<String>;  // active → Some("yolo")
```

接线点：

- `BridgeRuntime::set_config(allow_remote_yolo: Some(true))` → 持久化 + 经 runtime handle（`runtime.rs:21` 的 `engine: Arc<Engine>`，BridgeRuntime 在 start 后持有）调 `engine.grant_approval(ttl)`；`Some(false)` → 持久化 + `engine.revoke_approval()`。runtime 未运行时 grant 无处落地——语义即"须在运行中开启"（与重启失效一致）。
- `Engine::new` / `new_ephemeral`：**一律以 inactive 启动**，无论持久化 flag（§11：审批不跨重启；重启/重启 bridge 后用户须重新开启）。持久化 bool 保留仅作 schema 兼容与"上次配置"展示。
- `BridgeStatusDto`（mod.rs:47）新增 `approval_active: bool` + `approval_expires_at: Option<i64>`，供 UI 后续展示"已过期需重新开启"。
- `run_agent_turn`（engine.rs:901-907）：`permission_policy: self.effective_permission_policy()`。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| Stale（超窗/未来伪造） | drop + `tracing::warn!`（channel, instance, reason）；不回复发送者 |
| Replayed（nonce 窗口内重复） | 同上 |
| 审批过期 | 不报错；spawn 不再带 yolo（Runtime 回落默认策略）；DTO `approval_active=false` |
| ReplayGuard Mutex poison | `unwrap_or_else(|e| e.into_inner())`（与既有 APP_HOME_ENV_LOCK 模式一致） |

## 5. 文档化要求（spec 内承诺，非本包交付）

- WS/长轮询渠道的防重放依赖平台认证传输 + `DedupStore`，在矩阵 AC-8.4 证据与 per-channel 表中逐渠道说明。
- LINE 无时间戳 → 仅精确重放拦截，无 freshness 窗口。

## 6. 非目标（YAGNI）

- per-channel 独立审批配置/TTL（矩阵的 "per channel" 指机制覆盖每渠道，非每渠道独立配置）。
- nonce 持久化（窗口仅 300s，落盘无意义且违背 §11 精神）。
- 前端"审批已过期"提示 UI（DTO 字段已备，后续工作包）。
- 给其他渠道补 HMAC 验签（feishu/telegram 等属 AC-8.1/平台配置范畴）。
- IM 侧 PIN（§11.2 明确不要求）。

## 7. 测试清单（TDD，全部 `cargo test` remote_im）

**replay_guard.rs（7）**：①新鲜 ts 放行；②超窗旧 ts → Stale；③超窗未来 ts → Stale；④nonce 窗口内重复 → Replayed；⑤nonce 过期后重用 → Allow；⑥无 ts/nonce → Allow；⑦渠道隔离（同 nonce 不同 channel 各自计数）。

**engine.rs（3）**：⑧webhook 消息 stale ts 在 dedup 之前被丢弃（dedup 不含该 message_id）；⑨replayed nonce 被丢弃；⑩合法消息正常通过 guard 进入后续流水线。

**审批（5）**：⑪grant 后 active 且 `effective_permission_policy() == Some("yolo")`；⑫TTL 过期后 inactive 且 policy None；⑬revoke 立即 inactive；⑭新 Engine（模拟重启）即使 persisted yolo=true 也 inactive；⑮`set_config(false)` 撤销 + `set_config(true)` 重新授权（bridge 层）。

共 15 个新测试。测试构造复用 `Engine::new_ephemeral` / `OutboundRouter::new` / 手写 IncomingMessage 字面量模式（`engine.rs:1301-1321`）。

## 8. 矩阵更新计划

- AC-8.4 → PASS：证据 = replay_guard 7 测试 + engine 接线 3 测试 + 审批 5 测试；`allow_remote_yolo` 经 TTL 授权接线到 `SpawnOptions.permission_policy`。
- per-channel 表（matrix.md:154-168）AC-8.4 列：webhook 渠道（wecom/LINE/generic）证据 freshness+nonce；其余渠道证据 transport-auth+dedup+审批门。
- SA-R.5（security-audit-checklist.md:61）→ PASS。
- test-coverage-audit.md：remote_im 计数 +15；gap 表"Remote approval expiry + anti-replay"行标 Resolved。
