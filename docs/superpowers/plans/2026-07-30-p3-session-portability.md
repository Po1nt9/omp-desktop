# P3: 会话持久化 + 跨设备迁移 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** EventJournal 持久化到磁盘 + 会话导出/导入（幂等，仿 Hermes portability），让会话可跨设备手动迁移。

**Architecture:** 给 EventJournal 加 Serialize + save/load 纯方法；新建 `portability.rs` 做 PortableSession 导出（messages.json + journal + meta）和幂等导入（skip-on-exists）；session_manager 在 commit 后持久化 journal。

**Tech Stack:** Rust, serde_json, std::fs, chrono.

## Global Constraints

- 包名 `omp-desktop`，测试在 `src-tauri/` 下跑 `cargo test -p omp-desktop`
- 现有 store API：`load_sessions_index() -> Vec<SessionMeta>`、`update_session_meta(&meta)`、`load_messages(id) -> Vec<ChatMessageStored>`、`save_messages(id, &msgs)`
- session 目录：`crate::paths::session_dir(id)`（返回 `app_data_root/sessions/<id>`）
- EventJournal 字段当前私有（session_id/events/commit_points/sequence）

**Spec:** `docs/superpowers/specs/2026-07-30-p3-session-portability-design.md`

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `src-tauri/src/event_journal/mod.rs` | 修改 | EventJournal 加 Serialize/Deserialize + save_to/load_from/standard_path |
| `src-tauri/src/portability.rs` | 创建 | PortableSession + export/import 逻辑 |
| `src-tauri/src/lib.rs` | 修改 | 注册 `mod portability;` |
| `src-tauri/src/commands.rs` | 修改 | 加 2 个 Tauri 命令 |
| `src-tauri/src/session_manager.rs` | 修改 | commit 后持久化 journal |

---

### Task 1: EventJournal 持久化

**Files:**
- Modify: `src-tauri/src/event_journal/mod.rs`

**Interfaces:**
- Produces: `EventJournal` now `Serialize+Deserialize`; `EventJournal::standard_path(id) -> PathBuf`, `save_to(&self, &Path) -> io::Result<()>`, `load_from(&Path) -> Result<Self, String>`

- [ ] **Step 1: Add derives + persistence methods**

In `src-tauri/src/event_journal/mod.rs`:

Add `Serialize, Deserialize` to the `EventJournal` struct derive (currently has no derives):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventJournal {
    session_id: String,
    events: Vec<JournalEvent>,
    commit_points: Vec<CommitPoint>,
    sequence: u64,
}
```

Add `use std::path::Path;` and `use std::path::PathBuf;` to the imports (top of file, after `use serde...`). Then add these methods to `impl EventJournal` (after the existing `events()` method):

```rust
    /// Standard on-disk path: `<session_dir>/event_journal.json`.
    pub fn standard_path(session_id: &str) -> PathBuf {
        crate::paths::session_dir(session_id).join("event_journal.json")
    }

    /// Serialize to a file (pretty JSON). Creates parent dirs as needed.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        use std::io::ErrorKind;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(ErrorKind::Other, e.to_string()))?;
        std::fs::write(path, bytes)
    }

    /// Deserialize from a file.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read(path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&raw).map_err(|e| e.to_string())
    }
```

Verify `crate::paths::session_dir` is `pub` (it is — used by store.rs).

- [ ] **Step 2: Add persistence tests**

Add a new test module at the end of `mod.rs` (the file currently has `pub mod tests;` as a separate file, so add an inline module here for the new tests):

```rust
#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn test_save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "omp-journal-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("event_journal.json");
        let mut journal = EventJournal::new("sess-rt".into());
        let _ = journal.append(EventKind::TurnStart, serde_json::json!({}));
        let cp = journal.commit();
        journal.save_to(&path).expect("save");

        let loaded = EventJournal::load_from(&path).expect("load");
        assert_eq!(loaded.events().len(), 1);
        assert_eq!(loaded.events()[0].id, journal.events()[0].id);
        assert_eq!(loaded.events()[0].kind, EventKind::TurnStart);
        // commit points aren't exposed by accessor; verify via events + sequence
        let _ = cp;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file_returns_err() {
        let path = std::path::Path::new("/nonexistent/omp-test-journal-xyz.json");
        assert!(EventJournal::load_from(path).is_err());
    }

    #[test]
    fn test_save_creates_parent_dir() {
        let dir = std::env::temp_dir().join(format!(
            "omp-journal-parent-{}/nested/deep",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("event_journal.json");
        let journal = EventJournal::new("sess-pd".into());
        journal.save_to(&path).expect("save creates parents");
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(
            path.parent().unwrap().parent().unwrap().parent().unwrap(),
        );
    }

    #[test]
    fn test_standard_path_format() {
        let p = EventJournal::standard_path("abc-123");
        assert!(p.to_string_lossy().ends_with("sessions/abc-123/event_journal.json"));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p omp-desktop event_journal::persistence_tests 2>&1 | tail -15
```
Expected: 4 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/event_journal/mod.rs
git commit -m "feat(event_journal): add disk persistence (save_to/load_from/standard_path)"
```

---

### Task 2: portability.rs — 导出/导入模块

**Files:**
- Create: `src-tauri/src/portability.rs`
- Modify: `src-tauri/src/lib.rs` (register `mod portability;`)

**Interfaces:**
- Consumes: `crate::event_journal::EventJournal`, `crate::store::{SessionMeta, ChatMessageStored, load_sessions_index, update_session_meta, load_messages, save_messages}`
- Produces: `PortableSession`, `ImportResult`, `export_session(id) -> Result<PortableSession>`, `import_session(&PortableSession) -> Result<ImportResult>`

- [ ] **Step 1: Create portability.rs with types + logic + tests**

Create `src-tauri/src/portability.rs`:

```rust
//! Session portability: export/import a session (messages + journal + meta)
//! for cross-device migration. Idempotent import (skip-on-exists), modeled
//! after Hermes Agent's `hermes_state_portability.py`.
use crate::event_journal::EventJournal;
use crate::store::{self, ChatMessageStored, SessionMeta};
use serde::{Deserialize, Serialize};

const MAX_MESSAGES_PER_SESSION: usize = 10_000;
const MAX_SESSION_BYTES: usize = 5 * 1024 * 1024; // 5 MB

/// A portable session bundle (journal + messages + meta).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableSession {
    pub meta: SessionMeta,
    pub messages: Vec<ChatMessageStored>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<EventJournal>,
}

/// Result of an import attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: bool,
    pub session_id: String,
    pub reason: String,
}

impl ImportResult {
    fn imported(id: &str) -> Self {
        Self {
            imported: true,
            session_id: id.into(),
            reason: "imported".into(),
        }
    }
    fn skipped(id: &str) -> Self {
        Self {
            imported: false,
            session_id: id.into(),
            reason: "session already exists".into(),
        }
    }
}

/// Export a single session as a `PortableSession`.
pub fn export_session(session_id: &str) -> Result<PortableSession, String> {
    let meta = store::load_sessions_index()
        .into_iter()
        .find(|m| m.id == session_id)
        .ok_or_else(|| format!("session {session_id} not found"))?;
    let messages = store::load_messages(session_id);
    let journal = EventJournal::load_from(&EventJournal::standard_path(session_id)).ok();
    Ok(PortableSession {
        meta,
        messages,
        journal,
    })
}

/// Idempotent import: if `session_id` already exists, skip (do not overwrite).
pub fn import_session(data: &PortableSession) -> Result<ImportResult, String> {
    if data.messages.len() > MAX_MESSAGES_PER_SESSION {
        return Err(format!(
            "too many messages: {} > {MAX_MESSAGES_PER_SESSION}",
            data.messages.len()
        ));
    }
    let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SESSION_BYTES {
        return Err(format!(
            "session too large: {} > {MAX_SESSION_BYTES}",
            bytes.len()
        ));
    }
    let exists = store::load_sessions_index()
        .iter()
        .any(|m| m.id == data.meta.id);
    if exists {
        return Ok(ImportResult::skipped(&data.meta.id));
    }
    store::save_messages(&data.meta.id, &data.messages)?;
    store::update_session_meta(&data.meta)?;
    if let Some(j) = &data.journal {
        let _ = j.save_to(&EventJournal::standard_path(&data.meta.id));
    }
    Ok(ImportResult::imported(&data.meta.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_too_many_messages_rejected() {
        let mut data = PortableSession {
            meta: test_meta("toomany"),
            messages: vec![ChatMessageStored::default(); MAX_MESSAGES_PER_SESSION + 1],
            journal: None,
        };
        let _ = &mut data; // silence unused mut lint paths
        let res = import_session(&data);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("too many messages"));
    }

    #[test]
    fn test_import_skips_existing_session() {
        // A session that already exists in the index must be skipped.
        // We rely on the "general" session not existing in the temp test env;
        // instead test the skip logic directly by pre-inserting.
        let id = format!("skip-test-{}", unique_suffix());
        // Pre-create via store so it exists in index + on disk.
        let meta = test_meta(&id);
        store::save_messages(&id, &[]).unwrap();
        store::update_session_meta(&meta).unwrap();
        // Now import the same id → must skip.
        let data = PortableSession {
            meta,
            messages: vec![],
            journal: None,
        };
        let res = import_session(&data).unwrap();
        assert!(!res.imported);
        assert_eq!(res.reason, "session already exists");
        // cleanup
        let _ = std::fs::remove_dir_all(crate::paths::session_dir(&id));
        let mut list = store::load_sessions_index();
        list.retain(|m| m.id != id);
        let _ = store::save_sessions_index(&list);
    }

    #[test]
    fn test_roundtrip_export_import() {
        let id = format!("rt-test-{}", unique_suffix());
        // Seed: create session with one message + export + import under a fresh id.
        let msg = ChatMessageStored {
            role: "user".into(),
            content: "hello portability".into(),
            ..Default::default()
        };
        let meta = test_meta(&id);
        store::save_messages(&id, &[msg.clone()]).unwrap();
        store::update_session_meta(&meta).unwrap();
        // Export
        let exported = export_session(&id).unwrap();
        assert_eq!(exported.messages.len(), 1);
        assert_eq!(exported.messages[0].content, "hello portability");
        // Cleanup source
        let _ = std::fs::remove_dir_all(crate::paths::session_dir(&id));
        let mut list = store::load_sessions_index();
        list.retain(|m| m.id != id);
        let _ = store::save_sessions_index(&list);
        // Re-import (now id no longer exists) → imported
        let res = import_session(&exported).unwrap();
        assert!(res.imported, "should import after source removed");
        // Verify content persisted
        let loaded = store::load_messages(&id);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "hello portability");
        // cleanup
        let _ = std::fs::remove_dir_all(crate::paths::session_dir(&id));
        let mut list = store::load_sessions_index();
        list.retain(|m| m.id != id);
        let _ = store::save_sessions_index(&list);
    }

    fn test_meta(id: &str) -> SessionMeta {
        use chrono::Utc;
        SessionMeta {
            id: id.into(),
            project_id: None,
            title: "portability test".into(),
            agent_session_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            model_id: None,
            archived: false,
            pinned: false,
            effort: None,
            mode: None,
            permission_policy: None,
            scheduled: false,
        }
    }

    fn unique_suffix() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string()
    }
}
```

Note: verify `ChatMessageStored` derives `Default` — if not, construct it with required fields instead of `..Default::default()`. Check `src-tauri/src/store.rs` around line 409. Also verify `store::save_sessions_index` is accessible (it is `pub fn`, line 812).

Register in `src-tauri/src/lib.rs`: add `mod portability;` near other module declarations.

- [ ] **Step 2: Run portability tests**

```bash
cargo test -p omp-desktop portability 2>&1 | tail -20
```
Expected: 3 tests PASS. If `ChatMessageStored` lacks `Default`, adjust construction.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/portability.rs src-tauri/src/lib.rs
git commit -m "feat: add session portability module (export/import, idempotent)"
```

---

### Task 3: session_manager 集成 + Tauri 命令

**Files:**
- Modify: `src-tauri/src/session_manager.rs` (commit 后持久化 journal)
- Modify: `src-tauri/src/commands.rs` (加 2 命令)

- [ ] **Step 1: Persist journal after commit in session_manager**

In `src-tauri/src/session_manager.rs`, find the TurnEnd commit block (~line 3146-3151):

```rust
                            if let Some(journal) = s.event_journal.as_mut() {
                                journal.append(
                                    ...
                                );
                                let _ = journal.commit();
                            }
```

After this block (still inside the same scope where `s` is available), add persistence:

```rust
                            if let Some(journal) = s.event_journal.as_ref() {
                                let _ = journal.save_to(
                                    &crate::event_journal::EventJournal::standard_path(
                                        &s.app_session_id,
                                    ),
                                );
                            }
```

Note: `s.app_session_id` — verify this is the correct field name on the session struct (it was used at line 2807: `EventJournal::new(s.app_session_id.clone())`).

- [ ] **Step 2: Add Tauri commands in commands.rs**

In `src-tauri/src/commands.rs`, add (near the existing `session_import_transcript` ~line 4576):

```rust
#[tauri::command]
pub fn session_export_portable(session_id: String) -> Result<crate::portability::PortableSession, String> {
    crate::portability::export_session(&session_id)
}

#[tauri::command]
pub fn session_import_portable(
    data: crate::portability::PortableSession,
) -> Result<crate::portability::ImportResult, String> {
    crate::portability::import_session(&data)
}
```

- [ ] **Step 3: Register commands in the invoke_handler**

Find the `tauri::generate_handler![...]` list in `src-tauri/src/lib.rs` (or wherever the handler is registered) and add `commands::session_export_portable` and `commands::session_import_portable`. Search for `generate_handler` to locate it.

- [ ] **Step 4: Build + run tests**

```bash
cargo build -p omp-desktop 2>&1 | grep -E "^error" | head
cargo test -p omp-desktop event_journal portability 2>&1 | grep "test result" | tail -3
```
Expected: no errors; all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/session_manager.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: persist journal after commit + expose portability commands"
```

---

### Task 4: 全量验证

- [ ] **Step 1: Full build + clippy**

```bash
cargo build -p omp-desktop 2>&1 | grep "Finished"
cargo clippy -p omp-desktop 2>&1 | grep -iE "portability|event_journal::persistence" | head
```
Expected: build succeeds; no new warnings in P3 files.

- [ ] **Step 2: All affected tests**

```bash
cargo test -p omp-desktop event_journal portability 2>&1 | grep -E "test result|FAILED" | tail
```
Expected: all green.

---

## Self-Review

**Spec coverage:**
- §4.1 EventJournal 持久化 → Task 1 ✓
- §4.2/4.3 PortableSession + export/import → Task 2 ✓
- §4.4 Tauri 命令 → Task 3 ✓
- §4.5 session_manager 集成 → Task 3 ✓
- §5 测试 → Tasks 1,2 内嵌 ✓
- §7 验收 → Task 4 ✓

**Placeholder scan:** 无 TBD。Task 2 Step 1 注释提示核实 ChatMessageStored::Default，已在步骤内说明。

**Type consistency:** `PortableSession`/`ImportResult`/`export_session`/`import_session` 在 Task 2 定义，Task 3 引用一致。`EventJournal::standard_path`/`save_to`/`load_from` 在 Task 1 定义，Task 2/3 引用一致。
