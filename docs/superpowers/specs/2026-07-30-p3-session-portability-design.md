# P3: 会话持久化 + 跨设备迁移 (Session Portability) — Design Spec

- **日期**: 2026-07-30
- **工作包**: P3（EventJournal 持久化 + 会话导出/导入迁移）
- **状态**: Draft
- **前置**: Plan 3 EventJournal 已实现（纯内存）、P1/P2 已完成
- **参考蓝本**: [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) 的 `hermes_state_portability.py`

## 1. 调研结论（为什么不自动同步）

本工作包原目标"跨设备同步"经实际 GitHub 调研后重新定位：

- **iroh + iroh-docs**（11.9k★，纯 Rust）：P2P set reconciliation，但 KV 模型需自维护因果顺序，iroh-docs 仍 0.x。
- **Matrix/Tuwunel**：协议原生支持 event log + 多设备 fan-out，但运维 homeserver 对桌面应用过重。
- **NousResearch/hermes-agent**（223k★）：**无自动同步**。单机 SQLite，靠 `hermes_state_portability.py` 手动导出/导入迁移会话。
- **chenhg5/cc-connect**（14.5k★）：**无自动同步**。单实例，靠 IM 多端登录实现"跨设备"。

**两个最成熟的同类产品都不做自动同步**，靠 IM 平台本身的多端访问（OMP 已有 remote_im）。结论：自动同步引擎是过早优化，OMP 采用 Hermes 的 portability 路线——**持久化 + 手动导出/导入迁移**，比任何同步引擎轻数个数量级。

详见 brainstorming 调研记录（本会话前述）。

## 2. 需求

| # | 需求 | 来源 |
|---|------|------|
| R1 | EventJournal 持久化到磁盘，会话断开/应用退出后可恢复 | Plan 3 推迟项（"later plans persist to disk"） |
| R2 | 会话导出命令：journal + messages 打包成 JSON，供跨设备迁移 | Hermes portability 模式 |
| R3 | 会话导入命令：幂等（skip-on-exists），不覆盖已存在会话 | Hermes import_sessions 语义 |
| R4 | 有大小/数量限制，防撑爆 | Hermes `_IMPORT_MAX_*` 常量 |
| R5 | 不做自动实时同步 | 调研结论 |

## 3. 架构

```
导出路径                              导入路径
EventJournal (内存)                   PortableSession JSON (设备 B)
  + messages.json            ──→        ↓
  + SessionMeta              导出     session_import_portable
        ↓                            ↓
session_export_portable            session_id 已存在? skip : 写入
        ↓                            messages.json + event_journal.json
PortableSession {meta, journal, messages}
        ↓
   JSON（跨设备传输：AirDrop/局域网/手动）
```

## 4. 组件设计

### 4.1 EventJournal 持久化 (`event_journal/mod.rs`)

EventJournal 当前是纯内存，字段私有（session_id/events/commit_points/sequence）。加派生 + 持久化方法：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventJournal {
    session_id: String,
    events: Vec<JournalEvent>,
    commit_points: Vec<CommitPoint>,
    sequence: u64,
}

impl EventJournal {
    /// `app_data_root/sessions/<session_id>/event_journal.json`
    pub fn standard_path(session_id: &str) -> PathBuf {
        crate::paths::session_dir(session_id).join("event_journal.json")
    }
    /// 序列化到文件（pretty JSON）。
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() { let _ = std::fs::create_dir_all(p); }
        std::fs::write(path, serde_json::to_vec_pretty(self).map_err(io_err)?)
    }
    /// 从文件反序列化。
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read(path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&raw).map_err(|e| e.to_string())
    }
}
```
（`io_err` 把 serde 错误转 io::Error，或方法返回 `Result<_, String>`。）

### 4.2 PortableSession 类型（新，`store.rs` 或新 `portability.rs`）

```rust
/// 跨设备迁移的可移植会话包（仿 Hermes export_session）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableSession {
    pub meta: SessionMeta,
    pub messages: Vec<ChatMessageStored>,
    pub journal: Option<SerializableEventJournal>,  // None 则无 journal
}
```
`SerializableEventJournal` 即加了 Serialize/Deserialize 的 EventJournal（4.1 已派生）。

### 4.3 导出/导入逻辑（新 `portability.rs`）

```rust
/// 限制（仿 Hermes _IMPORT_MAX_*）
const MAX_MESSAGES_PER_SESSION: usize = 10_000;
const MAX_SESSION_BYTES: usize = 5 * 1024 * 1024; // 5 MB

/// 导出单会话为 PortableSession。
pub fn export_session(session_id: &str) -> Result<PortableSession, String> {
    let meta = store::load_sessions_index()
        .into_iter()
        .find(|m| m.id == session_id)
        .ok_or_else(|| format!("session {session_id} not found"))?;
    let messages = store::load_messages(session_id);
    let journal = EventJournal::load_from(&EventJournal::standard_path(session_id)).ok();
    Ok(PortableSession { meta, messages, journal })
}

/// 幂等导入：session_id 已存在则 skip（仿 Hermes import_sessions 语义）。
pub fn import_session(data: &PortableSession) -> Result<ImportResult, String> {
    // 大小/数量校验
    if data.messages.len() > MAX_MESSAGES_PER_SESSION { return Err("too many messages".into()); }
    let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SESSION_BYTES { return Err("session too large".into()); }
    // 幂等检查：查 sessions index
    let exists = store::load_sessions_index().iter().any(|m| m.id == data.meta.id);
    if exists { return Ok(ImportResult::skipped()); }
    // 写入：消息落 messages.json + meta 加入 index
    store::save_messages(&data.meta.id, &data.messages)?;
    store::update_session_meta(&data.meta)?;
    if let Some(j) = &data.journal {
        let _ = j.save_to(&EventJournal::standard_path(&data.meta.id));
    }
    Ok(ImportResult::imported())
}
```

### 4.4 Tauri 命令 (`commands.rs`)

```rust
#[tauri::command]
pub fn session_export_portable(session_id: String) -> Result<PortableSession, String> {
    crate::portability::export_session(&session_id)
}

#[tauri::command]
pub fn session_import_portable(data: PortableSession) -> Result<ImportResult, String> {
    crate::portability::import_session(&data)
}
```

### 4.5 session_manager 集成（commit 后持久化）

在 `session_manager.rs:3146-3151`（TurnEnd commit 后）追加：
```rust
if let Some(journal) = s.event_journal.as_ref() {
    let _ = journal.save_to(&EventJournal::standard_path(&s.app_session_id));
}
```

## 5. 测试策略（TDD）

### 5.1 event_journal 持久化测试
- `test_save_load_roundtrip`: new → append → commit → save → load → 字段一致
- `test_load_missing_file_returns_err`: 不存在路径报错
- `test_save_creates_parent_dir`: 父目录不存在时自动创建
- `test_standard_path_format`: 路径格式正确

### 5.2 portability 测试
- `test_export_includes_messages`: 导出含 messages
- `test_import_new_session`: 导入新 session_id，写入成功
- `test_import_skip_existing`: 已存在 session_id → skipped，不覆盖
- `test_import_too_many_messages`: 超限报错
- `test_import_too_large`: 超字节限制报错
- `test_roundtrip_export_import`: 导出 → 导入到新 id → messages 一致

## 6. 范围与非目标

- ✅ EventJournal 磁盘持久化（解决重启丢失）
- ✅ 会话手动导出/导入（跨设备迁移，仿 Hermes）
- ❌ 自动实时同步（iroh/Matrix，调研判定不做）
- ❌ journal 富集消息内容（独立工作包，当前 TurnStart/TurnEnd 元事件不变）
- ❌ 前端 UI（仅 Rust 命令 + 类型，前端接入后续）

## 7. 验收标准

- [ ] `cargo test -p omp-desktop event_journal` 含新增持久化测试全绿
- [ ] `cargo test -p omp-desktop portability` 全绿
- [ ] `cargo build` + `cargo clippy` 无新 warning
- [ ] 持久化验证：journal save → load roundtrip 字段一致（测试覆盖）
- [ ] 导入幂等验证：重复导入同一 session 不覆盖（测试覆盖）
