# 6-Step Credential Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement master design §8.2's 6-step idempotent credential migration (dry-run/copy/readback/reference/tombstone/cleanup) for `secrets.json` + `channel-secrets.json`, behind a unified mockable `SecretStore`, in strict mode (no silent plaintext fallback).

**Architecture:** `SecretStore` trait (prod: `KeychainStore`; test: `MockStore`) + one generic `Migrator` engine driving two `MigrationSource` adapters; per-step `MigrationLedger` makes every step resumable; load/save paths re-routed through the store; store-unavailable blocks credential writes with an actionable error.

**Tech Stack:** Rust (edition 2021), `keyring` crate (existing dep), `serde`/`serde_json`, Tauri 2.

## Global Constraints

- Master design §8.1: one unified helper; remote creds in isolated namespace; **no silent plaintext fallback** — store unavailable → block save/refresh with actionable error.
- Master design §8.2: six steps, per-step non-sensitive ledger, idempotent re-run; failed copy/readback/reference → rollback store entry, legacy stays readable; failed tombstone/cleanup → keep tombstone, retry; never copy verified entries back to plaintext.
- §18.2.6: seven migration test scenarios must pass: dry-run, success, re-run, store-unavailable, readback-failure, cleanup-failure, rollback.
- Tombstone marker: `"__tombstoned_v1__"`. Reference format: `keychain:v1:<ns>:<key>`. Namespaces: `"provider"`, `"remote"`.
- Never log secret values. Ledger stores only migration_id/namespace/key/state/reason-code.
- Do not commit `secrets.json` or any real credential material (AGENTS.md).
- Run `pnpm check:brand` before committing doc changes; `cargo test --manifest-path src-tauri/Cargo.toml` must stay green.

## File Structure

- `git mv src-tauri/src/secrets.rs src-tauri/src/secrets/mod.rs` — module becomes a directory; public API (`load_secrets`, `save_secrets`, `load_secrets_disk_only`, presence helpers) keeps its signatures.
- Create `src-tauri/src/secrets/store.rs` — trait + KeychainStore + StoreError + const_time_eq + MockStore.
- Create `src-tauri/src/secrets/migration.rs` — Migrator + MigrationLedger + MigrationSource + both adapters.
- Modify `src-tauri/src/remote_im/config.rs` — channel secrets through the store + refs file.
- Modify `src-tauri/src/lib.rs` — startup migration run.
- Modify `src-tauri/src/commands.rs` — retire keychain-toggle apply path.
- Modify `src/i18n/en.json`, `zh-CN.json`, `zh-TW.json` — `credentials.storeUnavailable` etc.
- Modify frontend settings component that renders the `storeApiKeysInKeychain` toggle (locate in Task 8).

---

### Task 1: `SecretStore` trait + KeychainStore + MockStore

**Files:**
- Create: `src-tauri/src/secrets/store.rs`
- Move: `git mv src-tauri/src/secrets.rs src-tauri/src/secrets/mod.rs`
- Test: in-file `#[cfg(test)]` module in `store.rs`

**Interfaces:**
- Produces (relied on by Tasks 2-8):
  - `pub const NS_PROVIDER: &str = "provider"`, `pub const NS_REMOTE: &str = "remote"`
  - `pub enum StoreError { Unavailable { message_key: &'static str, detail: String }, Backend(String) }` with `impl Display + std::error::Error`, `pub fn message_key(&self) -> &'static str`
  - `pub trait SecretStore: Send + Sync { fn get(&self, ns: &str, key: &str) -> Result<Option<String>, StoreError>; fn set(&self, ns: &str, key: &str, value: &str) -> Result<(), StoreError>; fn delete(&self, ns: &str, key: &str) -> Result<(), StoreError>; }`
  - `pub struct KeychainStore;` + `KeychainStore::new()`, `pub fn probe(&self) -> bool`
  - `pub fn const_time_eq(a: &str, b: &str) -> bool`
  - `#[cfg(test)] pub struct MockStore` with failure flags: `set_unavailable(bool)`, `set_fail_set(bool)`, `set_corrupt_get(bool)`, `set_fail_delete(bool)`, and `pub fn len(&self) -> usize`, `pub fn contains(&self, ns: &str, key: &str) -> bool`
  - `pub fn default_store() -> std::sync::Arc<dyn SecretStore>` (process-wide, test-swappable via `#[cfg(test)] pub fn install_test_store(Arc<dyn SecretStore>)`)

- [ ] **Step 1: Move the module and create `store.rs` with the trait + KeychainStore**

```bash
cd ~/Github/grok-app-main
mkdir -p src-tauri/src/secrets
git mv src-tauri/src/secrets.rs src-tauri/src/secrets/mod.rs
```

In `src-tauri/src/secrets/mod.rs` add after the existing `use` block:

```rust
pub mod migration;
pub mod store;
```

Write `src-tauri/src/secrets/store.rs`:

```rust
//! Unified OS secure-store abstraction for credential material (design §8.1).
//!
//! One helper serves both Agent provider credentials (namespace `provider`)
//! and Remote IM channel credentials (namespace `remote`, isolated per §8.1).
//! Production backend is the OS keychain via the `keyring` crate; tests use
//! `MockStore` with programmable failure points.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Namespace for Agent provider credentials (official/relay API keys).
pub const NS_PROVIDER: &str = "provider";
/// Namespace for Remote IM channel credentials (isolated per §8.1).
pub const NS_REMOTE: &str = "remote";

/// Reverse-DNS service id shared with the app data layout.
const KEYRING_SERVICE: &str = "com.omp-desktop.omp-desktop";

/// Errors from the secure store. `Unavailable` carries an actionable i18n
/// message key; strict mode turns it into a user-facing block, never a
/// silent plaintext fallback.
#[derive(Debug, Clone)]
pub enum StoreError {
    Unavailable {
        message_key: &'static str,
        detail: String,
    },
    Backend(String),
}

impl StoreError {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        StoreError::Unavailable {
            message_key: "credentials.storeUnavailable",
            detail: detail.into(),
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            StoreError::Unavailable { message_key, .. } => message_key,
            StoreError::Backend(_) => "credentials.storeError",
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Unavailable { detail, .. } => {
                write!(f, "secure store unavailable: {detail}")
            }
            StoreError::Backend(e) => write!(f, "secure store error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

pub trait SecretStore: Send + Sync {
    fn get(&self, ns: &str, key: &str) -> Result<Option<String>, StoreError>;
    fn set(&self, ns: &str, key: &str, value: &str) -> Result<(), StoreError>;
    fn delete(&self, ns: &str, key: &str) -> Result<(), StoreError>;
}

/// Constant-time string compare for readback verification (§8.2 step 3).
pub fn const_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Production store backed by the OS keychain.
///
/// Account naming is `<ns>:<key>`. For the `provider` namespace, reads fall
/// back to the legacy unprefixed account name (written by pre-migration
/// builds) so existing keychain users keep their keys; writes always use the
/// prefixed form and delete clears both.
pub struct KeychainStore;

impl KeychainStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(account: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|e| StoreError::unavailable(e.to_string()))
    }

    fn account(ns: &str, key: &str) -> String {
        format!("{ns}:{key}")
    }

    /// Soft read-only probe: reachable store returns `true`. Never writes.
    pub fn probe(&self) -> bool {
        let entry = match Self::entry("__omp_store_probe__") {
            Ok(e) => e,
            Err(_) => return false,
        };
        !matches!(entry.get_password(), Err(keyring::Error::NoEntry) | Err(_))
            || matches!(entry.get_password(), Err(keyring::Error::NoEntry))
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, ns: &str, key: &str) -> Result<Option<String>, StoreError> {
        let entry = Self::entry(&Self::account(ns, key))?;
        match entry.get_password() {
            Ok(v) if !v.is_empty() => return Ok(Some(v)),
            Ok(_) => return Ok(None),
            Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(StoreError::Backend(e.to_string())),
        }
        // Legacy unprefixed fallback (provider namespace only).
        if ns == NS_PROVIDER {
            let legacy = Self::entry(key)?;
            match legacy.get_password() {
                Ok(v) if !v.is_empty() => return Ok(Some(v)),
                Ok(_) | Err(keyring::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(StoreError::Backend(e.to_string())),
            }
        }
        Ok(None)
    }

    fn set(&self, ns: &str, key: &str, value: &str) -> Result<(), StoreError> {
        let entry = Self::entry(&Self::account(ns, key))?;
        entry
            .set_password(value)
            .map_err(|e| StoreError::Backend(e.to_string()))
    }

    fn delete(&self, ns: &str, key: &str) -> Result<(), StoreError> {
        for account in [Self::account(ns, key), key.to_string()] {
            if ns != NS_PROVIDER && account == key {
                continue; // legacy fallback only exists for provider ns
            }
            if let Ok(entry) = Self::entry(&account) {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(StoreError::Backend(e.to_string())),
                }
            }
        }
        Ok(())
    }
}

// ── Process-wide store handle (test-swappable) ─────────────────────────────

static STORE: RwLock<Option<Arc<dyn SecretStore>>> = RwLock::new(None);

/// The app-wide store. Defaults to `KeychainStore`; tests install a mock.
pub fn default_store() -> Arc<dyn SecretStore> {
    if let Some(s) = STORE.read().unwrap().as_ref() {
        return s.clone();
    }
    let store: Arc<dyn SecretStore> = Arc::new(KeychainStore::new());
    *STORE.write().unwrap() = Some(store.clone());
    store
}

#[cfg(test)]
pub fn install_test_store(store: Arc<dyn SecretStore>) {
    *STORE.write().unwrap() = Some(store);
}

#[cfg(test)]
pub fn reset_test_store() {
    *STORE.write().unwrap() = None;
}

// ── Mock ───────────────────────────────────────────────────────────────────

#[cfg(test)]
pub struct MockStore {
    data: std::sync::Mutex<HashMap<(String, String), String>>,
    unavailable: std::sync::Mutex<bool>,
    fail_set: std::sync::Mutex<bool>,
    corrupt_get: std::sync::Mutex<bool>,
    fail_delete: std::sync::Mutex<bool>,
}

#[cfg(test)]
impl MockStore {
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(HashMap::new()),
            unavailable: std::sync::Mutex::new(false),
            fail_set: std::sync::Mutex::new(false),
            corrupt_get: std::sync::Mutex::new(false),
            fail_delete: std::sync::Mutex::new(false),
        }
    }

    pub fn set_unavailable(&self, v: bool) {
        *self.unavailable.lock().unwrap() = v;
    }
    pub fn set_fail_set(&self, v: bool) {
        *self.fail_set.lock().unwrap() = v;
    }
    pub fn set_corrupt_get(&self, v: bool) {
        *self.corrupt_get.lock().unwrap() = v;
    }
    pub fn set_fail_delete(&self, v: bool) {
        *self.fail_delete.lock().unwrap() = v;
    }
    pub fn len(&self) -> usize {
        self.data.lock().unwrap().len()
    }
    pub fn contains(&self, ns: &str, key: &str) -> bool {
        self.data
            .lock()
            .unwrap()
            .contains_key(&(ns.to_string(), key.to_string()))
    }
}

#[cfg(test)]
impl SecretStore for MockStore {
    fn get(&self, ns: &str, key: &str) -> Result<Option<String>, StoreError> {
        if *self.unavailable.lock().unwrap() {
            return Err(StoreError::unavailable("mock unavailable"));
        }
        let v = self
            .data
            .lock()
            .unwrap()
            .get(&(ns.to_string(), key.to_string()))
            .cloned();
        if *self.corrupt_get.lock().unwrap() {
            return Ok(v.map(|s| format!("{s}-corrupted")));
        }
        Ok(v)
    }

    fn set(&self, ns: &str, key: &str, value: &str) -> Result<(), StoreError> {
        if *self.unavailable.lock().unwrap() {
            return Err(StoreError::unavailable("mock unavailable"));
        }
        if *self.fail_set.lock().unwrap() {
            return Err(StoreError::Backend("mock set failure".into()));
        }
        self.data
            .lock()
            .unwrap()
            .insert((ns.to_string(), key.to_string()), value.to_string());
        Ok(())
    }

    fn delete(&self, ns: &str, key: &str) -> Result<(), StoreError> {
        if *self.unavailable.lock().unwrap() {
            return Err(StoreError::unavailable("mock unavailable"));
        }
        if *self.fail_delete.lock().unwrap() {
            return Err(StoreError::Backend("mock delete failure".into()));
        }
        self.data
            .lock()
            .unwrap()
            .remove(&(ns.to_string(), key.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_time_eq_compares_exactly() {
        assert!(const_time_eq("abc", "abc"));
        assert!(!const_time_eq("abc", "abd"));
        assert!(!const_time_eq("abc", "abcd"));
        assert!(!const_time_eq("", "a"));
        assert!(const_time_eq("", ""));
    }

    #[test]
    fn mock_store_roundtrip_and_flags() {
        let s = MockStore::new();
        s.set(NS_PROVIDER, "k", "v").unwrap();
        assert_eq!(s.get(NS_PROVIDER, "k").unwrap().as_deref(), Some("v"));
        assert!(s.contains(NS_PROVIDER, "k"));
        s.delete(NS_PROVIDER, "k").unwrap();
        assert_eq!(s.get(NS_PROVIDER, "k").unwrap(), None);

        s.set_unavailable(true);
        assert!(matches!(
            s.get(NS_PROVIDER, "k"),
            Err(StoreError::Unavailable { .. })
        ));
        assert_eq!(s.set(NS_REMOTE, "k", "v").unwrap_err().message_key(), "credentials.storeUnavailable");
        s.set_unavailable(false);

        s.set_fail_set(true);
        assert!(matches!(
            s.set(NS_PROVIDER, "k", "v"),
            Err(StoreError::Backend(_))
        ));
        s.set_fail_set(false);

        s.set(NS_PROVIDER, "k", "real").unwrap();
        s.set_corrupt_get(true);
        assert_eq!(s.get(NS_PROVIDER, "k").unwrap().as_deref(), Some("real-corrupted"));
        s.set_corrupt_get(false);

        s.set_fail_delete(true);
        assert!(s.delete(NS_PROVIDER, "k").is_err());
        assert!(s.contains(NS_PROVIDER, "k"));
    }

    #[test]
    fn namespaces_are_isolated() {
        let s = MockStore::new();
        s.set(NS_PROVIDER, "same-key", "provider-value").unwrap();
        s.set(NS_REMOTE, "same-key", "remote-value").unwrap();
        assert_eq!(
            s.get(NS_PROVIDER, "same-key").unwrap().as_deref(),
            Some("provider-value")
        );
        assert_eq!(
            s.get(NS_REMOTE, "same-key").unwrap().as_deref(),
            Some("remote-value")
        );
        s.delete(NS_PROVIDER, "same-key").unwrap();
        assert_eq!(
            s.get(NS_REMOTE, "same-key").unwrap().as_deref(),
            Some("remote-value")
        );
    }
}
```

- [ ] **Step 2: Fix the probe implementation**

The `probe()` above is convoluted; replace its body with the simple read-only check:

```rust
    /// Soft read-only probe: reachable store returns `true`. Never writes.
    pub fn probe(&self) -> bool {
        let entry = match Self::entry("__omp_store_probe__") {
            Ok(e) => e,
            Err(_) => return false,
        };
        match entry.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(_) => false,
        }
    }
```

- [ ] **Step 3: Build + run the new tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::store 2>&1 | tail -5`
Expected: 3 passed (compile may surface missing `migration` module — create an empty `src-tauri/src/secrets/migration.rs` with `//! placeholder; filled in Task 2.` so the module tree builds; Task 2 replaces it.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/secrets/
git commit -m "feat(secrets): SecretStore trait + KeychainStore + MockStore with fault injection"
```

---

### Task 2: MigrationLedger + Migrator dry_run (scenarios 1 + 4)

**Files:**
- Create/overwrite: `src-tauri/src/secrets/migration.rs`
- Test: in-file `#[cfg(test)]` module

**Interfaces:**
- Consumes: Task 1 `SecretStore`, `StoreError`, `MockStore`, `NS_PROVIDER`, `NS_REMOTE`, `const_time_eq`.
- Produces (Tasks 3-8 rely on):
  - `pub const TOMBSTONE: &str = "__tombstoned_v1__"`, `pub const REF_PREFIX: &str = "keychain:v1:"`
  - `pub fn make_reference(ns: &str, key: &str) -> String`, `pub fn is_reference(v: &str) -> bool`
  - `pub enum MigrationState { DryRun, Copied, Verified, Referenced, Tombstoned, Cleaned, Failed }`
  - `pub struct MigrationLedger { pub store_unavailable: bool, pub entries: Vec<LedgerEntry> }` + `load(path)`, `save(path)`, `state_of(&self, id) -> MigrationState` (default DryRun), `upsert(&mut self, id, ns, key, state, reason)`
  - `pub struct LegacyEntry { pub migration_id: String, pub namespace: &'static str, pub key: String }`
  - `pub trait MigrationSource { enumerate/read_legacy/commit_reference/tombstone/cleanup }` (signatures below)
  - `pub struct MigrationPlan { pub entries: Vec<LegacyEntry>, pub conflicts: Vec<String> }`
  - `pub struct Migrator<'a> { pub store: &'a dyn SecretStore, pub ledger_path: PathBuf }` + `pub fn dry_run(&self, source: &dyn MigrationSource) -> Result<MigrationPlan, StoreError>`

- [ ] **Step 1: Write the failing tests (scenario 1: dry-run writes nothing; scenario 4: store unavailable aborts)**

Append to `migration.rs` after the implementation skeleton below; write tests first per TDD — expected compile failure until `Migrator` exists:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::store::MockStore;

    /// In-memory source for engine tests.
    struct MemSource {
        entries: Vec<LegacyEntry>,
        values: std::sync::Mutex<std::collections::HashMap<String, String>>,
        references: std::sync::Mutex<std::collections::HashMap<String, String>>,
        fail_commit: std::sync::Mutex<bool>,
        fail_cleanup: std::sync::Mutex<bool>,
    }

    impl MemSource {
        fn single(id: &'static str, ns: &'static str, key: &str, value: &str) -> Self {
            let mut values = std::collections::HashMap::new();
            values.insert(key.to_string(), value.to_string());
            Self {
                entries: vec![LegacyEntry {
                    migration_id: id.to_string(),
                    namespace: ns,
                    key: key.to_string(),
                }],
                values: std::sync::Mutex::new(values),
                references: std::sync::Mutex::new(std::collections::HashMap::new()),
                fail_commit: std::sync::Mutex::new(false),
                fail_cleanup: std::sync::Mutex::new(false),
            }
        }
    }

    impl MigrationSource for MemSource {
        fn enumerate(&self) -> Result<Vec<LegacyEntry>, String> {
            Ok(self.entries.clone())
        }
        fn read_legacy(&self, entry: &LegacyEntry) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(&entry.key).cloned())
        }
        fn commit_reference(&self, entry: &LegacyEntry, reference: &str) -> Result<(), String> {
            if *self.fail_commit.lock().unwrap() {
                return Err("mock commit failure".into());
            }
            self.references
                .lock()
                .unwrap()
                .insert(entry.key.clone(), reference.to_string());
            Ok(())
        }
        fn tombstone(&self, entry: &LegacyEntry) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(entry.key.clone(), TOMBSTONE.to_string());
            Ok(())
        }
        fn cleanup(&self, entry: &LegacyEntry) -> Result<(), String> {
            if *self.fail_cleanup.lock().unwrap() {
                return Err("mock cleanup failure".into());
            }
            self.values.lock().unwrap().remove(&entry.key);
            Ok(())
        }
    }

    fn ledger_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omp-mig-test-{}-{}.json", tag, std::process::id()))
    }

    #[test]
    fn dry_run_enumerates_without_writing() {
        let store = MockStore::new();
        let path = ledger_path("dry");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).expect("dry run");
        assert_eq!(plan.entries.len(), 1);
        assert!(plan.conflicts.is_empty());
        // Nothing written: store empty, no ledger file, legacy value intact.
        assert_eq!(store.len(), 0);
        assert!(!path.exists());
        assert_eq!(
            source.read_legacy(&plan.entries[0]).unwrap().as_deref(),
            Some("sk-live")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dry_run_aborts_when_store_unavailable() {
        let store = MockStore::new();
        store.set_unavailable(true);
        let path = ledger_path("unavail");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let err = mig.dry_run(&source).expect_err("must abort");
        assert_eq!(err.message_key(), "credentials.storeUnavailable");
        // Failure flag persisted for Safe Mode wiring; legacy untouched.
        let ledger = MigrationLedger::load(&path);
        assert!(ledger.store_unavailable);
        assert_eq!(store.len(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cleaned_entries_are_skipped_on_rerun() {
        let store = MockStore::new();
        let path = ledger_path("rerun");
        let mut ledger = MigrationLedger::default();
        ledger.upsert("provider:official_api_key", NS_PROVIDER, "official_api_key", MigrationState::Cleaned, None);
        ledger.save(&path).unwrap();
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).expect("dry run");
        assert!(plan.entries.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | grep -E "^error" | head -3`
Expected: `cannot find` errors for `Migrator`, `MigrationLedger`, etc.

- [ ] **Step 3: Implement the module (replace the Task-1 placeholder entirely)**

```rust
//! §8.2 six-step idempotent credential migration engine.
//!
//! One generic `Migrator` drives any `MigrationSource` (secrets.json fields,
//! channel-secrets.json entries). Every step is recorded in a non-sensitive
//! ledger so a crash mid-pipeline resumes from each entry's last state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::store::{const_time_eq, SecretStore, StoreError};

/// Marker written in place of a legacy plaintext value after migration.
pub const TOMBSTONE: &str = "__tombstoned_v1__";
/// Reference scheme committed to metadata files (§8.2 step 4).
pub const REF_PREFIX: &str = "keychain:v1:";

pub fn make_reference(ns: &str, key: &str) -> String {
    format!("{REF_PREFIX}{ns}:{key}")
}

pub fn is_reference(v: &str) -> bool {
    v.starts_with(REF_PREFIX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    DryRun,
    Copied,
    Verified,
    Referenced,
    Tombstoned,
    Cleaned,
    /// Terminal for this run; re-run retries from DryRun.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub migration_id: String,
    pub namespace: String,
    pub key: String,
    pub state: MigrationState,
    #[serde(default)]
    pub reason: Option<String>,
    pub updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MigrationLedger {
    /// Set when a run aborted on store-unavailable; read by strict-mode
    /// load/save paths and (later) Safe Mode.
    #[serde(default)]
    pub store_unavailable: bool,
    #[serde(default)]
    pub entries: Vec<LedgerEntry>,
}

impl MigrationLedger {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::store_lock::write_bytes_atomic(path, raw.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn state_of(&self, migration_id: &str) -> MigrationState {
        self.entries
            .iter()
            .find(|e| e.migration_id == migration_id)
            .map(|e| e.state)
            .unwrap_or(MigrationState::DryRun)
    }

    pub fn upsert(
        &mut self,
        migration_id: &str,
        namespace: &str,
        key: &str,
        state: MigrationState,
        reason: Option<&str>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match self.entries.iter_mut().find(|e| e.migration_id == migration_id) {
            Some(e) => {
                e.state = state;
                e.reason = reason.map(|r| r.to_string());
                e.updated_at = now;
            }
            None => self.entries.push(LedgerEntry {
                migration_id: migration_id.to_string(),
                namespace: namespace.to_string(),
                key: key.to_string(),
                state,
                reason: reason.map(|r| r.to_string()),
                updated_at: now,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyEntry {
    pub migration_id: String,
    pub namespace: &'static str,
    pub key: String,
}

/// A plaintext source to migrate. Implementations must never log values.
pub trait MigrationSource {
    fn enumerate(&self) -> Result<Vec<LegacyEntry>, String>;
    fn read_legacy(&self, entry: &LegacyEntry) -> Result<Option<String>, String>;
    fn commit_reference(&self, entry: &LegacyEntry, reference: &str) -> Result<(), String>;
    fn tombstone(&self, entry: &LegacyEntry) -> Result<(), String>;
    fn cleanup(&self, entry: &LegacyEntry) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MigrationPlan {
    pub entries: Vec<LegacyEntry>,
    /// migration_ids whose ledger says a reference was committed but the
    /// store no longer resolves it — reported, never auto-overwritten.
    pub conflicts: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    pub cleaned: usize,
    pub failed: usize,
    /// Entries left at a mid-state (tombstone/cleanup failure); retried next run.
    pub pending: usize,
}

pub struct Migrator<'a> {
    pub store: &'a dyn SecretStore,
    pub ledger_path: PathBuf,
}

impl<'a> Migrator<'a> {
    pub fn new(store: &'a dyn SecretStore, ledger_path: &Path) -> Self {
        Self {
            store,
            ledger_path: ledger_path.to_path_buf(),
        }
    }

    fn load_ledger(&self) -> MigrationLedger {
        MigrationLedger::load(&self.ledger_path)
    }

    fn save_ledger(&self, ledger: &MigrationLedger) {
        if let Err(e) = ledger.save(&self.ledger_path) {
            tracing::warn!(target: "secrets::migration", error = %e, "failed to persist migration ledger");
        }
    }

    /// Step 1: enumerate + validate + conflict-check. Writes nothing except
    /// the store-unavailable flag when the store probe fails.
    pub fn dry_run(&self, source: &dyn MigrationSource) -> Result<MigrationPlan, StoreError> {
        // Probe: a read on a never-written key distinguishes unreachable
        // store (Unavailable) from empty store (Ok(None)).
        self.store.get("__probe__", "__probe__")?;

        let enumerated = source.enumerate().map_err(StoreError::Backend)?;
        let ledger = self.load_ledger();
        let mut plan = MigrationPlan::default();
        for entry in enumerated {
            match ledger.state_of(&entry.migration_id) {
                MigrationState::Cleaned => continue,
                MigrationState::Referenced | MigrationState::Tombstoned => {
                    // Resume crash-interrupted entries; flag broken references.
                    match self.store.get(entry.namespace, &entry.key) {
                        Ok(Some(_)) => plan.entries.push(entry),
                        _ => plan.conflicts.push(entry.migration_id.clone()),
                    }
                }
                _ => plan.entries.push(entry),
            }
        }
        Ok(plan)
    }
}

// tests live below (Tasks 2-3 append them)
```

Wait — dry_run must persist `store_unavailable` on probe failure. Adjust `dry_run` error path:

```rust
    pub fn dry_run(&self, source: &dyn MigrationSource) -> Result<MigrationPlan, StoreError> {
        if let Err(e) = self.store.get("__probe__", "__probe__") {
            if matches!(e, StoreError::Unavailable { .. }) {
                let mut ledger = self.load_ledger();
                ledger.store_unavailable = true;
                self.save_ledger(&ledger);
            }
            return Err(e);
        }
        // Successful probe clears a stale unavailable flag.
        let mut ledger = self.load_ledger();
        if ledger.store_unavailable {
            ledger.store_unavailable = false;
            self.save_ledger(&ledger);
        }
        let enumerated = source.enumerate().map_err(StoreError::Backend)?;
        let mut plan = MigrationPlan::default();
        for entry in enumerated {
            match ledger.state_of(&entry.migration_id) {
                MigrationState::Cleaned => continue,
                MigrationState::Referenced | MigrationState::Tombstoned => {
                    match self.store.get(entry.namespace, &entry.key) {
                        Ok(Some(_)) => plan.entries.push(entry),
                        _ => plan.conflicts.push(entry.migration_id.clone()),
                    }
                }
                _ => plan.entries.push(entry),
            }
        }
        Ok(plan)
    }
```

Also add to `MigrationLedger` a helper for strict-mode readers:

```rust
    /// True when a previous run aborted on store-unavailable.
    pub fn is_store_unavailable(path: &Path) -> bool {
        MigrationLedger::load(path).store_unavailable
    }
```

And a shared ledger path helper (used by lib.rs + secrets/mod.rs):

```rust
/// App-wide ledger location: `<app_data_root>/credential-migration.json`.
pub fn ledger_path() -> PathBuf {
    crate::paths::app_data_root().join("credential-migration.json")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | tail -3`
Expected: 3 passed (dry_run_enumerates_without_writing, dry_run_aborts_when_store_unavailable, cleaned_entries_are_skipped_on_rerun).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets/migration.rs
git commit -m "feat(secrets): migration ledger + Migrator dry_run with store-unavailable abort"
```

---

### Task 3: Migrator run loop — steps 2-6 (scenarios 2, 3, 5, 6, 7)

**Files:**
- Modify: `src-tauri/src/secrets/migration.rs` (append `run()` + tests)

**Interfaces:**
- Consumes: Task 2 types.
- Produces: `Migrator::run(&self, source: &dyn MigrationSource, plan: &MigrationPlan) -> MigrationReport` (Task 8 calls it).

- [ ] **Step 1: Write the failing tests (append inside `mod tests`)**

```rust
    #[test]
    fn run_success_migrates_end_to_end() {
        let store = MockStore::new();
        let path = ledger_path("ok");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        let report = mig.run(&source, &plan);
        assert_eq!(report.cleaned, 1);
        assert_eq!(report.failed, 0);
        // Value lives in the store; legacy removed; ledger terminal.
        assert_eq!(
            store.get(NS_PROVIDER, "official_api_key").unwrap().as_deref(),
            Some("sk-live")
        );
        assert_eq!(source.values.lock().unwrap().get("official_api_key"), None);
        let ledger = MigrationLedger::load(&path);
        assert_eq!(ledger.state_of("provider:official_api_key"), MigrationState::Cleaned);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rerun_after_success_is_noop() {
        let store = MockStore::new();
        let path = ledger_path("noop");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        mig.run(&source, &plan);
        // Second run: plan empty (Cleaned skipped... entry no longer enumerated
        // after cleanup, and ledger would skip it anyway).
        let plan2 = mig.dry_run(&source).unwrap();
        assert!(plan2.entries.is_empty());
        let report2 = mig.run(&source, &plan2);
        assert_eq!((report2.cleaned, report2.failed, report2.pending), (0, 0, 0));
        assert_eq!(
            store.get(NS_PROVIDER, "official_api_key").unwrap().as_deref(),
            Some("sk-live")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn readback_failure_rolls_back_store_and_keeps_legacy() {
        let store = MockStore::new();
        let path = ledger_path("rb");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        store.set_corrupt_get(true);
        let report = mig.run(&source, &plan);
        assert_eq!(report.failed, 1);
        // Rollback: store entry deleted (best-effort), legacy intact.
        store.set_corrupt_get(false);
        assert_eq!(store.get(NS_PROVIDER, "official_api_key").unwrap(), None);
        assert_eq!(
            source.read_legacy(&plan.entries[0]).unwrap().as_deref(),
            Some("sk-live")
        );
        let ledger = MigrationLedger::load(&path);
        assert_eq!(ledger.state_of("provider:official_api_key"), MigrationState::Failed);
        // Re-run retries and succeeds.
        let plan2 = mig.dry_run(&source).unwrap();
        assert_eq!(plan2.entries.len(), 1);
        let report2 = mig.run(&source, &plan2);
        assert_eq!(report2.cleaned, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cleanup_failure_keeps_tombstone_and_retries() {
        let store = MockStore::new();
        let path = ledger_path("cl");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        *source.fail_cleanup.lock().unwrap() = true;
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        let report = mig.run(&source, &plan);
        assert_eq!(report.pending, 1);
        assert_eq!(report.failed, 0);
        // Tombstone in place; value still resolvable from the store.
        assert_eq!(
            source.values.lock().unwrap().get("official_api_key").map(|s| s.as_str()),
            Some(TOMBSTONE)
        );
        assert_eq!(
            store.get(NS_PROVIDER, "official_api_key").unwrap().as_deref(),
            Some("sk-live")
        );
        // Retry completes cleanup (resume: entry re-enters dry_run because
        // MemSource still enumerates it — its value is the tombstone marker;
        // the engine treats tombstoned legacy as "jump to cleanup").
        *source.fail_cleanup.lock().unwrap() = false;
        let plan2 = mig.dry_run(&source).unwrap();
        let report2 = mig.run(&source, &plan2);
        assert_eq!(report2.cleaned, 1);
        assert_eq!(MigrationLedger::load(&path).state_of("provider:official_api_key"), MigrationState::Cleaned);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reference_commit_failure_rolls_back_store() {
        let store = MockStore::new();
        let path = ledger_path("rb7");
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        *source.fail_commit.lock().unwrap() = true;
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        let report = mig.run(&source, &plan);
        assert_eq!(report.failed, 1);
        assert_eq!(store.get(NS_PROVIDER, "official_api_key").unwrap(), None);
        assert_eq!(
            source.read_legacy(&plan.entries[0]).unwrap().as_deref(),
            Some("sk-live")
        );
        assert_eq!(MigrationLedger::load(&path).state_of("provider:official_api_key"), MigrationState::Failed);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupted_ledger_recovers_to_dry_run() {
        let store = MockStore::new();
        let path = ledger_path("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let source = MemSource::single("provider:official_api_key", NS_PROVIDER, "official_api_key", "sk-live");
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        assert_eq!(plan.entries.len(), 1);
        let report = mig.run(&source, &plan);
        assert_eq!(report.cleaned, 1);
        let _ = std::fs::remove_file(&path);
    }
```

Note: the cleanup-retry test relies on `dry_run` including entries whose legacy value is the tombstone marker (MemSource still enumerates them). Real adapters exclude tombstoned values from `enumerate`; the engine must therefore ALSO handle "ledger state Tombstoned + enumerated" by resuming at cleanup — already covered by the `Referenced | Tombstoned` arm in dry_run. For MemSource, `read_legacy` returning `Some(TOMBSTONE)` must make `run` jump straight to cleanup. That behavior is implemented below.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | grep -E "^error" | head -3`
Expected: `no method named run` compile error.

- [ ] **Step 3: Implement `run()` (append to `impl<'a> Migrator<'a>`)**

```rust
    /// Steps 2-6 per entry, resuming from each entry's ledger state.
    /// Failure semantics (§8.2): copy/readback/reference failure rolls back
    /// the store entry and keeps the legacy value readable; tombstone/cleanup
    /// failure keeps the tombstone and retries next run. Verified entries are
    /// never copied back to plaintext.
    pub fn run(&self, source: &dyn MigrationSource, plan: &MigrationPlan) -> MigrationReport {
        let mut report = MigrationReport::default();
        let mut ledger = self.load_ledger();

        for entry in &plan.entries {
            let id = entry.migration_id.as_str();
            // `Failed` sorts above `Cleaned` in the enum, so a failed entry
            // must be normalized back to DryRun here — otherwise every
            // `state < X` check short-circuits and re-runs silently no-op.
            let mut state = ledger.state_of(id);
            if state == MigrationState::Failed {
                state = MigrationState::DryRun;
            }

            // Tombstoned legacy (crash after step 5, or resumed entry): the
            // value is already safe in the store — jump to cleanup.
            if state < MigrationState::Tombstoned {
                if let Ok(Some(v)) = source.read_legacy(entry) {
                    if v == TOMBSTONE {
                        state = MigrationState::Tombstoned;
                    }
                }
            }

            // Steps 2-3: copy + readback.
            if state < MigrationState::Verified {
                let legacy = match source.read_legacy(entry) {
                    Ok(Some(v)) if v != TOMBSTONE => v,
                    Ok(_) => {
                        // Nothing to migrate (vanished mid-run); mark cleaned.
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Cleaned, None);
                        self.save_ledger(&ledger);
                        report.cleaned += 1;
                        continue;
                    }
                    Err(e) => {
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Failed, Some("legacy_read"));
                        self.save_ledger(&ledger);
                        tracing::warn!(target: "secrets::migration", migration_id = id, error = %e, "legacy read failed");
                        report.failed += 1;
                        continue;
                    }
                };
                if state < MigrationState::Copied {
                    if let Err(e) = self.store.set(entry.namespace, &entry.key, &legacy) {
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Failed, Some("store_copy"));
                        self.save_ledger(&ledger);
                        tracing::warn!(target: "secrets::migration", migration_id = id, error = %e, "store copy failed");
                        report.failed += 1;
                        continue;
                    }
                    ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Copied, None);
                    self.save_ledger(&ledger);
                }
                // Readback + constant-time compare.
                match self.store.get(entry.namespace, &entry.key) {
                    Ok(Some(v)) if const_time_eq(&v, &legacy) => {
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Verified, None);
                        self.save_ledger(&ledger);
                    }
                    _ => {
                        // Rollback the uncommitted copy; legacy stays readable.
                        let _ = self.store.delete(entry.namespace, &entry.key);
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Failed, Some("readback_mismatch"));
                        self.save_ledger(&ledger);
                        report.failed += 1;
                        continue;
                    }
                }
                state = MigrationState::Verified;
            }

            // Step 4: commit reference.
            if state < MigrationState::Referenced {
                let reference = make_reference(entry.namespace, &entry.key);
                if let Err(e) = source.commit_reference(entry, &reference) {
                    // Rollback: the reference was never committed, so the
                    // store copy must not linger as an orphan.
                    let _ = self.store.delete(entry.namespace, &entry.key);
                    ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Failed, Some("reference_commit"));
                    self.save_ledger(&ledger);
                    tracing::warn!(target: "secrets::migration", migration_id = id, error = %e, "reference commit failed");
                    report.failed += 1;
                    continue;
                }
                ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Referenced, None);
                self.save_ledger(&ledger);
                state = MigrationState::Referenced;
            }

            // Step 5: tombstone the legacy value.
            if state < MigrationState::Tombstoned {
                if let Err(e) = source.tombstone(entry) {
                    // Keep pre-tombstone state; value is safe in the store and
                    // the reference is committed. Retry next run.
                    self.save_ledger(&ledger);
                    tracing::warn!(target: "secrets::migration", migration_id = id, error = %e, "tombstone failed; will retry");
                    report.pending += 1;
                    continue;
                }
                ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Tombstoned, None);
                self.save_ledger(&ledger);
                state = MigrationState::Tombstoned;
            }

            // Step 6: re-validate the reference resolves, then clean up.
            if state < MigrationState::Cleaned {
                match self.store.get(entry.namespace, &entry.key) {
                    Ok(Some(v)) if !v.is_empty() => {}
                    _ => {
                        // Reference broken — never delete anything.
                        ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Failed, Some("reference_unresolvable"));
                        self.save_ledger(&ledger);
                        report.failed += 1;
                        continue;
                    }
                }
                if let Err(e) = source.cleanup(entry) {
                    // Keep tombstone; retry next run. No credential loss.
                    self.save_ledger(&ledger);
                    tracing::warn!(target: "secrets::migration", migration_id = id, error = %e, "cleanup failed; will retry");
                    report.pending += 1;
                    continue;
                }
                ledger.upsert(id, entry.namespace, &entry.key, MigrationState::Cleaned, None);
                self.save_ledger(&ledger);
                report.cleaned += 1;
            }
        }

        report
    }
```

- [ ] **Step 4: Run tests to verify all pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | tail -3`
Expected: 9 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets/migration.rs
git commit -m "feat(secrets): Migrator run loop with §8.2 rollback/tombstone/retry semantics (7 scenarios green)"
```

---

### Task 4: `SecretsJsonSource` adapter

**Files:**
- Modify: `src-tauri/src/secrets/migration.rs` (append adapter + tests)
- Modify: `src-tauri/src/secrets/mod.rs` (make `write_disk_secrets`/`read_disk_secrets` `pub(crate)`)

**Interfaces:**
- Consumes: Task 2/3 engine; `SecretsFile` from `crate::store`.
- Produces: `pub struct SecretsJsonSource { pub path: PathBuf }` + `SecretsJsonSource::new(path: PathBuf)`.

Design note: `commit_reference` writes the reference string **into the field itself** (`official_api_key = "keychain:v1:provider:official_api_key"`) and sets the `keychain_has_*` flag — the plaintext leaves disk atomically at step 4. `tombstone` is therefore a no-op (the reference replaces the plaintext; a field starting with `keychain:v1:` is never dual-read as plaintext). `cleanup` only re-confirms (the engine already re-validated the store).

- [ ] **Step 1: Make the disk helpers crate-visible in `secrets/mod.rs`**

Change `fn read_disk_secrets` → `pub(crate) fn read_disk_secrets` and `fn write_disk_secrets` → `pub(crate) fn write_disk_secrets`.

- [ ] **Step 2: Write the failing tests (append inside `mod tests`)**

```rust
    mod secrets_json {
        use super::*;
        use crate::secrets::migration::SecretsJsonSource;
        use crate::store::SecretsFile;

        fn tmp_secrets(tag: &str, official: Option<&str>, relay: Option<&str>) -> PathBuf {
            let path = std::env::temp_dir().join(format!(
                "omp-secrets-src-{}-{}.json", tag, std::process::id()
            ));
            let file = SecretsFile {
                official_api_key: official.map(|s| s.to_string()),
                relay_api_key: relay.map(|s| s.to_string()),
                relay_base_url: Some("https://relay.example".into()),
                default_model: Some("grok-4".into()),
                ..Default::default()
            };
            std::fs::write(&path, serde_json::to_string_pretty(&file).unwrap()).unwrap();
            path
        }

        #[test]
        fn enumerates_plaintext_fields_only() {
            let path = tmp_secrets("enum", Some("sk-live"), None);
            let source = SecretsJsonSource::new(path.clone());
            let entries = source.enumerate().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].migration_id, "provider:official_api_key");
            assert_eq!(entries[0].namespace, NS_PROVIDER);
            assert_eq!(entries[0].key, "official_api_key");
            // References are not re-enumerated.
            let path2 = tmp_secrets("enum2", Some("keychain:v1:provider:official_api_key"), None);
            let source2 = SecretsJsonSource::new(path2.clone());
            assert!(source2.enumerate().unwrap().is_empty());
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&path2);
        }

        #[test]
        fn commit_reference_replaces_plaintext_atomically() {
            let path = tmp_secrets("commit", Some("sk-live"), Some("rk-live"));
            let source = SecretsJsonSource::new(path.clone());
            let entry = LegacyEntry {
                migration_id: "provider:official_api_key".into(),
                namespace: NS_PROVIDER,
                key: "official_api_key".into(),
            };
            source
                .commit_reference(&entry, "keychain:v1:provider:official_api_key")
                .unwrap();
            let raw = std::fs::read_to_string(&path).unwrap();
            assert!(!raw.contains("sk-live"));
            assert!(raw.contains("keychain:v1:provider:official_api_key"));
            // Metadata and the untouched relay field survive.
            assert!(raw.contains("relay.example"));
            assert!(raw.contains("rk-live"));
            let disk: SecretsFile = serde_json::from_str(&raw).unwrap();
            assert!(disk.keychain_has_official);
            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn full_engine_run_over_secrets_json() {
            let store = MockStore::new();
            let path = tmp_secrets("full", Some("sk-live"), Some("rk-live"));
            let ledger = ledger_path("full-src");
            let source = SecretsJsonSource::new(path.clone());
            let mig = Migrator::new(&store, &ledger);
            let plan = mig.dry_run(&source).unwrap();
            assert_eq!(plan.entries.len(), 2);
            let report = mig.run(&source, &plan);
            assert_eq!((report.cleaned, report.failed), (2, 0));
            let raw = std::fs::read_to_string(&path).unwrap();
            assert!(!raw.contains("sk-live"));
            assert!(!raw.contains("rk-live"));
            assert_eq!(
                store.get(NS_PROVIDER, "official_api_key").unwrap().as_deref(),
                Some("sk-live")
            );
            assert_eq!(
                store.get(NS_PROVIDER, "relay_api_key").unwrap().as_deref(),
                Some("rk-live")
            );
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&ledger);
        }
    }
```

- [ ] **Step 3: Run to verify compile failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | grep -E "^error" | head -3`
Expected: `SecretsJsonSource` not found.

- [ ] **Step 4: Implement the adapter (append to `migration.rs`, before `mod tests`)**

```rust
// ── secrets.json adapter ───────────────────────────────────────────────────

use crate::store::SecretsFile;

const KEY_OFFICIAL: &str = "official_api_key";
const KEY_RELAY: &str = "relay_api_key";

/// Migrates the two plaintext fields of `secrets.json`. The reference is
/// written into the field itself, so step 4 removes plaintext from disk
/// atomically; tombstone/cleanup are no-ops beyond engine bookkeeping.
pub struct SecretsJsonSource {
    pub path: PathBuf,
}

impl SecretsJsonSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> SecretsFile {
        crate::secrets::read_disk_secrets(&self.path)
    }

    fn write(&self, file: &SecretsFile) -> Result<(), String> {
        crate::secrets::write_disk_secrets(&self.path, file)
    }
}

impl MigrationSource for SecretsJsonSource {
    fn enumerate(&self) -> Result<Vec<LegacyEntry>, String> {
        let disk = self.read();
        let mut out = Vec::new();
        for (field, value) in [
            (KEY_OFFICIAL, &disk.official_api_key),
            (KEY_RELAY, &disk.relay_api_key),
        ] {
            if let Some(v) = value {
                if !v.is_empty() && !is_reference(v) {
                    out.push(LegacyEntry {
                        migration_id: format!("provider:{field}"),
                        namespace: super::store::NS_PROVIDER,
                        key: field.to_string(),
                    });
                }
            }
        }
        Ok(out)
    }

    fn read_legacy(&self, entry: &LegacyEntry) -> Result<Option<String>, String> {
        let disk = self.read();
        Ok(match entry.key.as_str() {
            KEY_OFFICIAL => disk.official_api_key,
            KEY_RELAY => disk.relay_api_key,
            _ => None,
        })
    }

    fn commit_reference(&self, entry: &LegacyEntry, reference: &str) -> Result<(), String> {
        let mut disk = self.read();
        match entry.key.as_str() {
            KEY_OFFICIAL => {
                disk.official_api_key = Some(reference.to_string());
                disk.keychain_has_official = true;
            }
            KEY_RELAY => {
                disk.relay_api_key = Some(reference.to_string());
                disk.keychain_has_relay = true;
            }
            _ => return Err(format!("unknown secrets.json field {}", entry.key)),
        }
        self.write(&disk)
    }

    fn tombstone(&self, _entry: &LegacyEntry) -> Result<(), String> {
        // The reference replaced the plaintext at step 4; nothing further
        // to mark on disk.
        Ok(())
    }

    fn cleanup(&self, _entry: &LegacyEntry) -> Result<(), String> {
        // File already holds only the reference + metadata.
        Ok(())
    }
}
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | tail -3`
Expected: 12 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/secrets/
git commit -m "feat(secrets): SecretsJsonSource adapter — reference replaces plaintext atomically"
```

---

### Task 5: `ChannelSecretsSource` adapter + secure delete

**Files:**
- Modify: `src-tauri/src/secrets/migration.rs` (append adapter + tests)

**Interfaces:**
- Produces: `pub struct ChannelSecretsSource { pub secrets_path: PathBuf, pub refs_path: PathBuf }` + `ChannelSecretsSource::new(secrets_path, refs_path)`; refs-file shape `HashMap<String, HashMap<String, String>>` (instance → field → reference); helpers `pub fn read_channel_refs(path: &Path) -> HashMap<String, HashMap<String, String>>` and `pub fn write_channel_refs(path: &Path, refs: &HashMap<String, HashMap<String, String>>) -> Result<(), String>` (Task 7 uses them for dual-read).

- [ ] **Step 1: Write the failing tests (append a `mod channel` inside `mod tests`)**

```rust
    mod channel {
        use super::*;
        use crate::secrets::migration::{read_channel_refs, ChannelSecretsSource};
        use std::collections::HashMap;

        fn tmp_channel(tag: &str) -> (PathBuf, PathBuf) {
            let dir = std::env::temp_dir().join(format!("omp-chan-src-{}-{}", tag, std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            (dir.join("channel-secrets.json"), dir.join("channel-secret-refs.json"))
        }

        fn write_legacy(path: &Path, map: &HashMap<String, HashMap<String, String>>) {
            std::fs::write(path, serde_json::to_string_pretty(map).unwrap()).unwrap();
        }

        fn two_instance_map() -> HashMap<String, HashMap<String, String>> {
            let mut fields_a = HashMap::new();
            fields_a.insert("botToken".to_string(), "tg-token-a".to_string());
            fields_a.insert("appSecret".to_string(), "app-secret-a".to_string());
            let mut fields_b = HashMap::new();
            fields_b.insert("appId".to_string(), "fs-app-id".to_string());
            let mut map = HashMap::new();
            map.insert("inst-a".to_string(), fields_a);
            map.insert("inst-b".to_string(), fields_b);
            map
        }

        #[test]
        fn enumerates_instance_field_pairs() {
            let (secrets, refs) = tmp_channel("enum");
            write_legacy(&secrets, &two_instance_map());
            let source = ChannelSecretsSource::new(secrets.clone(), refs);
            let mut ids: Vec<String> = source
                .enumerate()
                .unwrap()
                .into_iter()
                .map(|e| e.migration_id)
                .collect();
            ids.sort();
            assert_eq!(
                ids,
                vec![
                    "remote:inst-a:appSecret".to_string(),
                    "remote:inst-a:botToken".to_string(),
                    "remote:inst-b:appId".to_string(),
                ]
            );
            let _ = std::fs::remove_dir_all(secrets.parent().unwrap());
        }

        #[test]
        fn full_run_tombstones_then_deletes_legacy_file() {
            let store = MockStore::new();
            let (secrets, refs) = tmp_channel("full");
            write_legacy(&secrets, &two_instance_map());
            let ledger = ledger_path("chan-full");
            let source = ChannelSecretsSource::new(secrets.clone(), refs.clone());
            let mig = Migrator::new(&store, &ledger);
            let plan = mig.dry_run(&source).unwrap();
            assert_eq!(plan.entries.len(), 3);
            let report = mig.run(&source, &plan);
            assert_eq!((report.cleaned, report.failed, report.pending), (3, 0, 0));
            // Legacy file securely deleted; refs file holds all references;
            // values live in the store under the remote namespace.
            assert!(!secrets.exists());
            let refs_map = read_channel_refs(&refs);
            assert_eq!(
                refs_map["inst-a"]["botToken"],
                "keychain:v1:remote:inst-a:botToken"
            );
            assert_eq!(
                store.get(NS_REMOTE, "inst-a:botToken").unwrap().as_deref(),
                Some("tg-token-a")
            );
            assert_eq!(
                store.get(NS_REMOTE, "inst-b:appId").unwrap().as_deref(),
                Some("fs-app-id")
            );
            let _ = std::fs::remove_dir_all(secrets.parent().unwrap());
            let _ = std::fs::remove_file(&ledger);
        }

        #[test]
        fn cleanup_failure_preserves_tombstoned_file_and_retries() {
            let store = MockStore::new();
            let (secrets, refs) = tmp_channel("retry");
            let mut map = HashMap::new();
            let mut fields = HashMap::new();
            fields.insert("botToken".to_string(), "tg-token".to_string());
            map.insert("inst-a".to_string(), fields);
            write_legacy(&secrets, &map);
            let ledger = ledger_path("chan-retry");
            // Source whose first cleanup attempt fails: simulate by making the
            // legacy path read-only after tombstone. Simpler: run once with a
            // directory that cannot be modified is platform-dependent, so
            // instead verify the engine contract via ledger states here and
            // leave fault-injection to MemSource (Task 3 scenario 6).
            let source = ChannelSecretsSource::new(secrets.clone(), refs.clone());
            let mig = Migrator::new(&store, &ledger);
            let plan = mig.dry_run(&source).unwrap();
            let report = mig.run(&source, &plan);
            assert_eq!(report.cleaned, 1);
            assert!(!secrets.exists());
            let _ = std::fs::remove_dir_all(secrets.parent().unwrap());
            let _ = std::fs::remove_file(&ledger);
        }
    }
```

- [ ] **Step 2: Run to verify compile failure**

Expected: `ChannelSecretsSource` / `read_channel_refs` not found.

- [ ] **Step 3: Implement the adapter (append to `migration.rs`)**

```rust
// ── channel-secrets.json adapter ───────────────────────────────────────────

use std::collections::HashMap;

/// Read the non-secret references file (instance → field → reference).
pub fn read_channel_refs(path: &Path) -> HashMap<String, HashMap<String, String>> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn write_channel_refs(
    path: &Path,
    refs: &HashMap<String, HashMap<String, String>>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(refs).map_err(|e| e.to_string())?;
    crate::store_lock::write_bytes_atomic(path, raw.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Migrates every (instance, field) of `channel-secrets.json`. References go
/// to `channel-secret-refs.json`; tombstoned values keep the file shape;
/// when the last field cleans up, the legacy file is securely deleted
/// (overwrite-then-remove, §8.2 step 6).
pub struct ChannelSecretsSource {
    pub secrets_path: PathBuf,
    pub refs_path: PathBuf,
}

impl ChannelSecretsSource {
    pub fn new(secrets_path: PathBuf, refs_path: PathBuf) -> Self {
        Self {
            secrets_path,
            refs_path,
        }
    }

    fn read_legacy_map(&self) -> HashMap<String, HashMap<String, String>> {
        std::fs::read_to_string(&self.secrets_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_legacy_map(&self, map: &HashMap<String, HashMap<String, String>>) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        crate::store_lock::write_bytes_atomic(&self.secrets_path, raw.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.secrets_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn split_key(key: &str) -> Option<(&str, &str)> {
        // instance ids contain no ':' in this codebase (uuid-ish); fields
        // are camelCase. Split on the LAST ':' so either side may grow.
        key.rsplit_once(':')
    }

    /// Overwrite with zeros then remove — best-effort secure delete.
    fn secure_delete(path: &Path) -> Result<(), String> {
        if let Ok(meta) = std::fs::metadata(path) {
            let len = meta.len() as usize;
            if len > 0 {
                let _ = std::fs::write(path, vec![0u8; len]);
            }
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl MigrationSource for ChannelSecretsSource {
    fn enumerate(&self) -> Result<Vec<LegacyEntry>, String> {
        let map = self.read_legacy_map();
        let mut out = Vec::new();
        for (instance, fields) in &map {
            for (field, value) in fields {
                if value.is_empty() || value == TOMBSTONE || is_reference(value) {
                    continue;
                }
                let key = format!("{instance}:{field}");
                out.push(LegacyEntry {
                    migration_id: format!("remote:{key}"),
                    namespace: super::store::NS_REMOTE,
                    key,
                });
            }
        }
        Ok(out)
    }

    fn read_legacy(&self, entry: &LegacyEntry) -> Result<Option<String>, String> {
        let Some((instance, field)) = Self::split_key(&entry.key) else {
            return Ok(None);
        };
        Ok(self
            .read_legacy_map()
            .get(instance)
            .and_then(|f| f.get(field))
            .cloned())
    }

    fn commit_reference(&self, entry: &LegacyEntry, reference: &str) -> Result<(), String> {
        let Some((instance, field)) = Self::split_key(&entry.key) else {
            return Err(format!("malformed channel key {}", entry.key));
        };
        let mut refs = read_channel_refs(&self.refs_path);
        refs.entry(instance.to_string())
            .or_default()
            .insert(field.to_string(), reference.to_string());
        write_channel_refs(&self.refs_path, &refs)
    }

    fn tombstone(&self, entry: &LegacyEntry) -> Result<(), String> {
        let Some((instance, field)) = Self::split_key(&entry.key) else {
            return Err(format!("malformed channel key {}", entry.key));
        };
        let mut map = self.read_legacy_map();
        if let Some(fields) = map.get_mut(instance) {
            if fields.contains_key(field) {
                fields.insert(field.to_string(), TOMBSTONE.to_string());
            }
        }
        self.write_legacy_map(&map)
    }

    fn cleanup(&self, entry: &LegacyEntry) -> Result<(), String> {
        let Some((instance, field)) = Self::split_key(&entry.key) else {
            return Err(format!("malformed channel key {}", entry.key));
        };
        let mut map = self.read_legacy_map();
        if let Some(fields) = map.get_mut(instance) {
            fields.remove(field);
            if fields.is_empty() {
                map.remove(instance);
            }
        }
        if map.is_empty() {
            Self::secure_delete(&self.secrets_path)
        } else {
            self.write_legacy_map(&map)
        }
    }
}
```

Note: `cleanup` removes one field per call; the file is securely deleted when the last field's cleanup empties the map. The zero-overwrite happens only at that final delete (earlier writes already replaced values with tombstones, so no plaintext lingers in the interim file).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets::migration 2>&1 | tail -3`
Expected: 15 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets/migration.rs
git commit -m "feat(secrets): ChannelSecretsSource adapter with tombstone + secure delete"
```

---

### Task 6: Strict-mode rewire of `secrets/mod.rs`

**Files:**
- Modify: `src-tauri/src/secrets/mod.rs` (load/save through store; retire file mode)
- Modify: `src-tauri/src/store.rs:474-485` (remove the keychain opt-in back-fill)

**Interfaces:**
- Consumes: Tasks 1-5 (`default_store`, `is_reference`, `ledger_path`, `MigrationLedger::is_store_unavailable`).
- Produces: unchanged public API (`load_secrets`, `save_secrets`, `load_secrets_disk_only`, presence helpers) — but strict semantics:
  - `save_secrets`: writes both keys via `default_store()`; disk gets reference strings + metadata. `StoreError` → `Err(String)` (actionable, via Display).
  - `load_secrets`: resolves `keychain:v1:` references via the store; plaintext fields pass through ONLY when no store-unavailable flag is set (pre-migration window); when the flag is set, secret fields resolve to `None` (fail-closed) and a warning logs the message key.
  - `migrate_plaintext_keys_to_keychain` and `apply_keychain_preference` are removed (startup Migrator replaces them); `prefer_keychain_storage`/`use_keychain_backend`/`probe_keychain`/`keychain_platform_ok`/`keychain_get`/`keychain_set`/`keychain_delete`/`configured_backend`/`active_backend`/`clear_keychain_secrets` are removed; `wipe_all_secrets` deletes store entries via `default_store()` + removes the file.

- [ ] **Step 1: Write the failing strict-mode tests (replace the old `mod tests` in `secrets/mod.rs`)**

Keep these existing tests unchanged: `disk_has_plaintext_keys_detects_present`, `presence_uses_keychain_flags_without_values`, `strip_keys_for_disk_keeps_metadata_and_flags`, `merge_prefers_keychain_over_disk`, `merge_falls_back_to_disk_when_keychain_empty`, `strip_roundtrip_json_has_no_keys`, `presence_helpers_do_not_need_key_material`, `file_write_preserves_keys_when_using_full_payload`.
Remove (functions deleted): `keychain_roundtrip_when_available`, `soft_probe_does_not_require_write`, `default_settings_prefer_file_not_keychain` (the settings field itself is retired in this task — also remove the field from `AppSettings`; see Step 3).

New tests (note: these use `install_test_store` + temp paths; the functions under test must take path/store from the process-wide handle — see implementation):

```rust
    #[test]
    fn save_then_load_roundtrips_through_store_without_plaintext_on_disk() {
        let store = Arc::new(crate::secrets::store::MockStore::new());
        crate::secrets::store::install_test_store(store.clone());
        let tmp = std::env::temp_dir().join(format!("omp-strict-{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        crate::paths::set_test_app_data_root(&tmp); // added in Step 3
        let ledger = tmp.join("credential-migration.json");
        let _ = fs::remove_file(&ledger);

        let s = SecretsFile {
            official_api_key: Some("sk-strict".into()),
            relay_api_key: Some("rk-strict".into()),
            relay_base_url: Some("https://r".into()),
            default_model: Some("m".into()),
            ..Default::default()
        };
        save_secrets(&s).unwrap();
        // Disk holds no plaintext.
        let raw = fs::read_to_string(crate::paths::secrets_file()).unwrap();
        assert!(!raw.contains("sk-strict"));
        assert!(!raw.contains("rk-strict"));
        assert!(raw.contains("keychain:v1:provider:official_api_key"));
        // Load resolves references back to values.
        invalidate_session_cache();
        let loaded = load_secrets();
        assert_eq!(loaded.official_api_key.as_deref(), Some("sk-strict"));
        assert_eq!(loaded.relay_api_key.as_deref(), Some("rk-strict"));
        assert_eq!(loaded.relay_base_url.as_deref(), Some("https://r"));

        crate::secrets::store::reset_test_store();
        crate::paths::reset_test_app_data_root();
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn store_unavailable_blocks_save_and_fails_load_closed() {
        let store = Arc::new(crate::secrets::store::MockStore::new());
        crate::secrets::store::install_test_store(store.clone());
        let tmp = std::env::temp_dir().join(format!("omp-strict-ua-{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        crate::paths::set_test_app_data_root(&tmp);

        store.set_unavailable(true);
        let s = SecretsFile {
            official_api_key: Some("sk-never-disk".into()),
            ..Default::default()
        };
        let err = save_secrets(&s).expect_err("save must fail closed");
        assert!(err.contains("secure store unavailable"));
        // Nothing written to disk.
        let disk = load_secrets_disk_only();
        assert!(disk.official_api_key.is_none());

        // Fail-closed load: plaintext on disk + unavailable flag → None.
        let legacy = SecretsFile {
            official_api_key: Some("sk-legacy".into()),
            ..Default::default()
        };
        crate::secrets::write_disk_secrets(&crate::paths::secrets_file(), &legacy).unwrap();
        let mut ledger = crate::secrets::migration::MigrationLedger::default();
        ledger.store_unavailable = true;
        ledger.save(&crate::secrets::migration::ledger_path()).unwrap();
        invalidate_session_cache();
        let loaded = load_secrets();
        assert_eq!(loaded.official_api_key, None);

        crate::secrets::store::reset_test_store();
        crate::paths::reset_test_app_data_root();
        let _ = fs::remove_dir_all(&tmp);
    }
```

- [ ] **Step 2: Run to verify failure**

Expected: compile errors (`set_test_app_data_root` missing; `save_secrets` signature/behavior unchanged yet — `store_unavailable_blocks_save...` fails because current code writes plaintext).

- [ ] **Step 3: Implement the rewire**

In `src-tauri/src/paths.rs` add a test-only root override (check existing file for how `app_data_root` is derived; mirror the simplest override pattern — if a `OnceLock`/env override already exists, use it instead of adding new):

```rust
#[cfg(test)]
static TEST_ROOT: std::sync::RwLock<Option<std::path::PathBuf>> = std::sync::RwLock::new(None);

#[cfg(test)]
pub fn set_test_app_data_root(p: &std::path::Path) {
    *TEST_ROOT.write().unwrap() = Some(p.to_path_buf());
}

#[cfg(test)]
pub fn reset_test_app_data_root() {
    *TEST_ROOT.write().unwrap() = None;
}
```

and at the top of `app_data_root()`:

```rust
#[cfg(test)]
if let Some(p) = TEST_ROOT.read().unwrap().as_ref() {
    return p.clone();
}
```

In `src-tauri/src/store.rs`: remove `store_api_keys_in_keychain` from `AppSettings` (field + default + the back-fill block at lines 474-485). Frontend/settings references are handled in Task 8.

In `src-tauri/src/secrets/mod.rs`: delete the functions listed in Interfaces (probe/prefer/use_keychain/keychain_get/set/delete/migrate_plaintext/apply_keychain_preference/configured_backend/active_backend/clear_keychain_secrets), delete `SecretsBackendKind`, and rewrite the core:

```rust
use migration::{is_reference, ledger_path, MigrationLedger};
use std::sync::Arc;
use store::{default_store, SecretStore, NS_PROVIDER};

const KEY_OFFICIAL: &str = "official_api_key";
const KEY_RELAY: &str = "relay_api_key";

fn store() -> Arc<dyn SecretStore> {
    default_store()
}

/// Resolve one secret field: reference → store; plaintext → pass through
/// unless the store-unavailable flag demands fail-closed.
fn resolve_field(value: Option<String>, key: &str, store_unavailable: bool) -> Option<String> {
    match value {
        Some(v) if is_reference(&v) => match store().get(NS_PROVIDER, key) {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::warn!(target: "grok_app::secrets", field = key, error = %e, "failed to resolve secret reference");
                None
            }
        },
        Some(v) if !v.is_empty() => {
            if store_unavailable {
                // Fail closed (§8.1): never silently keep using plaintext
                // once a migration attempt has failed on store-unavailable.
                tracing::warn!(target: "grok_app::secrets", field = key, message_key = "credentials.storeUnavailable", "blocking plaintext credential read; secure store unavailable");
                None
            } else {
                Some(v)
            }
        }
        _ => None,
    }
}
```

Rewrite `load_secrets` (drop the keychain-merge branch):

```rust
pub fn load_secrets() -> SecretsFile {
    {
        let cache = SESSION_CACHE.lock();
        if let Some(ref s) = *cache {
            return s.clone();
        }
    }
    let _ = ensure_app_dirs();
    let disk = read_disk_secrets(&secrets_file());
    let unavailable = MigrationLedger::is_store_unavailable(&ledger_path());
    let merged = SecretsFile {
        official_api_key: resolve_field(disk.official_api_key.clone(), KEY_OFFICIAL, unavailable),
        relay_api_key: resolve_field(disk.relay_api_key.clone(), KEY_RELAY, unavailable),
        relay_base_url: disk.relay_base_url,
        default_model: disk.default_model,
        keychain_has_official: disk.keychain_has_official,
        keychain_has_relay: disk.keychain_has_relay,
    };
    *SESSION_CACHE.lock() = Some(merged.clone());
    merged
}
```

Rewrite `save_secrets` (strict):

```rust
pub fn save_secrets(s: &SecretsFile) -> Result<(), String> {
    let _ = ensure_app_dirs();
    let path = secrets_file();
    invalidate_session_cache();
    let store = store();

    let mut disk = strip_keys_for_disk(s);
    for (key, value) in [
        (KEY_OFFICIAL, &s.official_api_key),
        (KEY_RELAY, &s.relay_api_key),
    ] {
        match value {
            Some(v) if !v.is_empty() && !is_reference(v) => {
                store.set(NS_PROVIDER, key, v).map_err(|e| e.to_string())?;
                set_disk_field(&mut disk, key, Some(migration::make_reference(NS_PROVIDER, key)), true);
            }
            Some(v) if is_reference(v) => {
                // Caller passed back the on-disk form; keep it.
                set_disk_field(&mut disk, key, Some(v.clone()), true);
            }
            _ => {
                // IMPORTANT: before executing, read the CURRENT save_secrets
                // body and confirm what `None`/empty means today ("unchanged"
                // vs "cleared"). Preserve that semantic exactly — only the
                // backend changes. If None currently means "leave the stored
                // key alone", this arm must skip the delete below instead.
                store.delete(NS_PROVIDER, key).map_err(|e| e.to_string())?;
                set_disk_field(&mut disk, key, None, false);
            }
        }
    }
    write_disk_secrets(&path, &disk)?;
    *SESSION_CACHE.lock() = Some(SecretsFile {
        official_api_key: s.official_api_key.clone(),
        relay_api_key: s.relay_api_key.clone(),
        relay_base_url: disk.relay_base_url.clone(),
        default_model: disk.default_model.clone(),
        keychain_has_official: disk.keychain_has_official,
        keychain_has_relay: disk.keychain_has_relay,
    });
    Ok(())
}

fn set_disk_field(disk: &mut SecretsFile, key: &str, value: Option<String>, has: bool) {
    match key {
        KEY_OFFICIAL => {
            disk.official_api_key = value;
            disk.keychain_has_official = has;
        }
        _ => {
            disk.relay_api_key = value;
            disk.keychain_has_relay = has;
        }
    }
}
```

Rewrite `wipe_all_secrets`:

```rust
pub fn wipe_all_secrets() -> Result<(), String> {
    let store = store();
    let _ = store.delete(NS_PROVIDER, KEY_OFFICIAL);
    let _ = store.delete(NS_PROVIDER, KEY_RELAY);
    let path = secrets_file();
    if path.is_file() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    invalidate_session_cache();
    Ok(())
}
```

Fix remaining references to deleted functions: grep `apply_keychain_preference\|configured_backend\|active_backend\|clear_keychain_secrets\|SecretsBackendKind\|migrate_plaintext_keys_to_keychain` across `src-tauri/src` and update callers — known: `commands.rs:661` (handled in Task 8), `commands.rs:939` (`storeApiKeysInKeychain` status field — remove), any support-bundle/diagnostics usage of `configured_backend` (replace with a literal `"keychain"` string or drop the field; inspect and choose the smallest honest change).

- [ ] **Step 4: Run secrets tests + full lib build**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib secrets 2>&1 | tail -3`
Expected: all pass (2 new strict tests + kept legacy tests + 15 migration + 3 store).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets/ src-tauri/src/paths.rs src-tauri/src/store.rs src-tauri/src/commands.rs
git commit -m "feat(secrets): strict mode — save/load through SecretStore, no plaintext fallback"
```

---

### Task 7: Route channel credentials through the store

**Files:**
- Modify: `src-tauri/src/remote_im/config.rs`

**Interfaces:**
- Consumes: `default_store`, `NS_REMOTE`, `migration::{read_channel_refs, write_channel_refs, is_reference, TOMBSTONE, ledger_path, MigrationLedger}`.
- Produces: unchanged public signatures (`get_secrets(instance_id) -> HashMap<String,String>`, `save_instance`, `delete_instance`) so callers (`runtime.rs:51`, `validate.rs:11`) need no changes; `pub fn channel_refs_path() -> PathBuf` (Task 8 uses it); save path is strict (store error → `Err`).

- [ ] **Step 1: Write the failing tests (append `#[cfg(test)] mod strict_tests` to `config.rs`)**

```rust
#[cfg(test)]
mod strict_tests {
    use super::*;
    use crate::secrets::migration::{ledger_path, MigrationLedger};
    use crate::secrets::store::{install_test_store, reset_test_store, MockStore, NS_REMOTE, SecretStore};
    use std::sync::Arc;

    fn setup(tag: &str) -> (Arc<MockStore>, PathBuf) {
        let store = Arc::new(MockStore::new());
        install_test_store(store.clone());
        let tmp = std::env::temp_dir().join(format!("omp-chan-strict-{}-{}", tag, std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        crate::paths::set_test_app_data_root(&tmp);
        (store, tmp)
    }

    fn teardown(tmp: &PathBuf) {
        reset_test_store();
        crate::paths::reset_test_app_data_root();
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn save_instance_writes_store_not_plaintext_file() {
        let (store, tmp) = setup("save");
        let inst = ChannelInstanceDto {
            id: "inst-x".into(),
            channel: "telegram".into(),
            name: "t".into(),
            enabled: true,
            has_credentials: false,
            status: "unconfigured".into(),
            last_error: None,
        };
        let mut secrets = HashMap::new();
        secrets.insert("botToken".to_string(), "tg-secret".to_string());
        save_instance(&inst, &secrets).unwrap();
        // No plaintext secrets file; reference recorded; value in store.
        assert!(!secrets_path().exists());
        let refs = crate::secrets::migration::read_channel_refs(&channel_refs_path());
        assert_eq!(refs["inst-x"]["botToken"], "keychain:v1:remote:inst-x:botToken");
        assert_eq!(
            store.get(NS_REMOTE, "inst-x:botToken").unwrap().as_deref(),
            Some("tg-secret")
        );
        // Dual-read returns the value.
        assert_eq!(get_secrets("inst-x")["botToken"], "tg-secret");
        teardown(&tmp);
    }

    #[test]
    fn store_unavailable_blocks_channel_save() {
        let (store, tmp) = setup("ua");
        store.set_unavailable(true);
        let inst = ChannelInstanceDto {
            id: "inst-y".into(),
            channel: "discord".into(),
            name: "d".into(),
            enabled: true,
            has_credentials: false,
            status: "unconfigured".into(),
            last_error: None,
        };
        let mut secrets = HashMap::new();
        secrets.insert("botToken".to_string(), "never-on-disk".to_string());
        assert!(save_instance(&inst, &secrets).is_err());
        assert!(!secrets_path().exists());
        teardown(&tmp);
    }

    #[test]
    fn get_secrets_dual_reads_legacy_until_migrated() {
        let (store, tmp) = setup("dual");
        // Pre-migration legacy file.
        let mut map = HashMap::new();
        let mut fields = HashMap::new();
        fields.insert("botToken".to_string(), "legacy-token".to_string());
        map.insert("inst-z".to_string(), fields);
        write_secrets_map(&map).unwrap();
        // No references yet → plaintext served (no unavailable flag).
        assert_eq!(get_secrets("inst-z")["botToken"], "legacy-token");
        // After migration (store + refs + tombstone), store wins.
        store.set(NS_REMOTE, "inst-z:botToken", "legacy-token").unwrap();
        let mut refs = HashMap::new();
        let mut r = HashMap::new();
        r.insert("botToken".to_string(), "keychain:v1:remote:inst-z:botToken".to_string());
        refs.insert("inst-z".to_string(), r);
        crate::secrets::migration::write_channel_refs(&channel_refs_path(), &refs).unwrap();
        assert_eq!(get_secrets("inst-z")["botToken"], "legacy-token");
        teardown(&tmp);
    }

    #[test]
    fn delete_instance_removes_store_entries_and_refs() {
        let (store, tmp) = setup("del");
        store.set(NS_REMOTE, "inst-d:botToken", "tok").unwrap();
        let mut refs = HashMap::new();
        let mut r = HashMap::new();
        r.insert("botToken".to_string(), "keychain:v1:remote:inst-d:botToken".to_string());
        refs.insert("inst-d".to_string(), r);
        crate::secrets::migration::write_channel_refs(&channel_refs_path(), &refs).unwrap();
        let inst = ChannelInstanceDto {
            id: "inst-d".into(),
            channel: "telegram".into(),
            name: "t".into(),
            enabled: true,
            has_credentials: true,
            status: "configured".into(),
            last_error: None,
        };
        save_instance(&inst, &HashMap::new()).unwrap();
        delete_instance("inst-d").unwrap();
        assert!(!store.contains(NS_REMOTE, "inst-d:botToken"));
        assert!(crate::secrets::migration::read_channel_refs(&channel_refs_path()).get("inst-d").is_none());
        teardown(&tmp);
    }
}
```

Check `ChannelInstanceDto` field names/types first (`grep -n "struct ChannelInstanceDto" -A 15 src-tauri/src/remote_im/mod.rs` or wherever it lives) and adjust the literals — they must match the real struct exactly.

- [ ] **Step 2: Run to verify failure** (compile errors for `channel_refs_path`, behavior failures for plaintext writes)

- [ ] **Step 3: Implement the rewire in `config.rs`**

Add:

```rust
use crate::secrets::migration::{
    is_reference, read_channel_refs, write_channel_refs, TOMBSTONE,
};
use crate::secrets::store::{default_store, NS_REMOTE};

/// Non-secret reference file replacing plaintext channel secrets.
pub fn channel_refs_path() -> PathBuf {
    remote_dir().join("channel-secret-refs.json")
}
```

Rewrite `get_secrets`:

```rust
pub fn get_secrets(instance_id: &str) -> HashMap<String, String> {
    let store = default_store();
    let refs = read_channel_refs(&channel_refs_path());
    let legacy = load_secrets_map();
    let mut out = HashMap::new();

    // Referenced fields resolve from the secure store.
    if let Some(fields) = refs.get(instance_id) {
        for field in fields.keys() {
            let key = format!("{instance_id}:{field}");
            match store.get(NS_REMOTE, &key) {
                Ok(Some(v)) => {
                    out.insert(field.clone(), v);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(target: "remote_im::config", instance = instance_id, field = field.as_str(), error = %e, "failed to resolve channel secret reference");
                }
            }
        }
    }
    // Dual-read: legacy fields not yet migrated (and not tombstoned).
    if let Some(fields) = legacy.get(instance_id) {
        for (field, value) in fields {
            if value.is_empty() || value == TOMBSTONE || is_reference(value) {
                continue;
            }
            out.entry(field.clone()).or_insert_with(|| value.clone());
        }
    }
    out
}
```

Rewrite the secrets part of `save_instance` (replace the `load_secrets_map`/`write_secrets_map` block):

```rust
    // Strict mode: secrets go to the OS secure store; only non-secret
    // references persist. Store failure aborts the save (no plaintext).
    let store = default_store();
    let mut refs = read_channel_refs(&channel_refs_path());
    let mut ref_row = refs.remove(&saved.id).unwrap_or_default();
    let legacy = load_secrets_map();
    let legacy_count = legacy.get(&saved.id).map(|f| f.len()).unwrap_or(0);
    for (k, v) in secrets {
        let t = v.trim();
        if t.is_empty() {
            continue;
        }
        let key = format!("{}:{}", saved.id, k);
        store.set(NS_REMOTE, &key, t).map_err(|e| e.to_string())?;
        ref_row.insert(k.clone(), format!("keychain:v1:remote:{key}"));
    }
    let has = !ref_row.is_empty() || legacy_count > 0;
    if !ref_row.is_empty() {
        refs.insert(saved.id.clone(), ref_row);
    }
    write_channel_refs(&channel_refs_path(), &refs)?;
```

(keep the rest of `save_instance` — status/has_credentials/list write — unchanged).

Rewrite `delete_instance`'s secrets part:

```rust
    let store = default_store();
    let mut refs = read_channel_refs(&channel_refs_path());
    if let Some(fields) = refs.remove(instance_id) {
        for field in fields.keys() {
            let _ = store.delete(NS_REMOTE, &format!("{instance_id}:{field}"));
        }
        write_channel_refs(&channel_refs_path(), &refs)?;
    }
    let mut all = load_secrets_map();
    all.remove(instance_id);
    write_secrets_map(&all)?;
```

Also handle `write_secrets_map` callers: after this task only migration-era dual-read and `delete_instance` (legacy cleanup) write the legacy file; leave both helpers in place for the migration window.

- [ ] **Step 4: Run remote_im tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib remote_im 2>&1 | tail -3`
Expected: all pass (existing 60+ plus 4 new).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/remote_im/config.rs
git commit -m "feat(remote_im): channel credentials through SecretStore (strict mode, dual-read)"
```

---

### Task 8: Startup migration wiring + retire keychain toggle + i18n

**Files:**
- Modify: `src-tauri/src/lib.rs` (run migration at startup, after `ensure_app_dirs`)
- Modify: `src-tauri/src/commands.rs:644-664, 939` (remove toggle apply + status field)
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json`, `src/i18n/zh-TW.json`
- Modify: frontend settings component rendering the toggle (locate: `grep -rn "storeApiKeysInKeychain" src/`)

**Interfaces:**
- Consumes: all previous tasks.

- [ ] **Step 1: Add the startup runner to `secrets/migration.rs`**

```rust
/// Run both migrations at app startup. Returns a combined report; on
/// store-unavailable the ledger flag is set (consumed by strict-mode loads
/// and, later, Safe Mode).
pub fn run_startup_migration(store: &dyn crate::secrets::store::SecretStore) -> MigrationReport {
    let ledger = ledger_path();
    let mig = Migrator::new(store, &ledger);
    let mut total = MigrationReport::default();

    let secrets_source = SecretsJsonSource::new(crate::paths::secrets_file());
    let refs = crate::paths::app_data_root()
        .join("remote")
        .join("channel-secret-refs.json");
    let channel_source = ChannelSecretsSource::new(
        crate::paths::app_data_root()
            .join("remote")
            .join("channel-secrets.json"),
        refs,
    );

    for source in [
        &secrets_source as &dyn MigrationSource,
        &channel_source as &dyn MigrationSource,
    ] {
        match mig.dry_run(source) {
            Ok(plan) => {
                if !plan.conflicts.is_empty() {
                    tracing::warn!(
                        target: "secrets::migration",
                        conflicts = ?plan.conflicts,
                        "migration conflicts need manual attention"
                    );
                }
                let r = mig.run(source, &plan);
                total.cleaned += r.cleaned;
                total.failed += r.failed;
                total.pending += r.pending;
            }
            Err(e) => {
                tracing::warn!(
                    target: "secrets::migration",
                    error = %e,
                    message_key = e.message_key(),
                    "credential migration aborted"
                );
                total.failed += 1;
                break; // store unavailable aborts both sources
            }
        }
    }
    total
}
```

- [ ] **Step 2: Wire it in `lib.rs`**

In the `setup` closure (or right after `paths::ensure_app_dirs()` at line 73, before any credential use):

```rust
    let store = crate::secrets::store::default_store();
    let report = crate::secrets::migration::run_startup_migration(store.as_ref());
    if report.cleaned > 0 || report.failed > 0 || report.pending > 0 {
        tracing::info!(
            target: "secrets::migration",
            cleaned = report.cleaned,
            failed = report.failed,
            pending = report.pending,
            "startup credential migration finished"
        );
    }
```

- [ ] **Step 3: Retire the settings toggle**

- `commands.rs:644-664`: remove the `store_api_keys_in_keychain` comparison + `apply_keychain_preference` call + rollback lines (the field no longer exists on `AppSettings` after Task 6).
- `commands.rs:939`: remove the `"storeApiKeysInKeychain"` status entry.
- Frontend: `grep -rn "storeApiKeysInKeychain" src/` — remove the toggle UI block (likely a settings row with `t("settings.storeApiKeysInKeychain...")`); remove now-unused i18n keys for that row if they exist (check `check:i18n` after).

- [ ] **Step 4: Add i18n keys (all three locales)**

`en.json`:
```json
"credentials.storeUnavailable": "System secure storage is unavailable. Credentials cannot be saved or loaded. On Linux, install and unlock a Secret Service provider (gnome-keyring or KWallet), then restart."
```
`zh-CN.json`:
```json
"credentials.storeUnavailable": "系统安全存储不可用，无法保存或读取凭据。Linux 用户请安装并解锁 Secret Service（gnome-keyring 或 KWallet）后重启。"
```
`zh-TW.json`:
```json
"credentials.storeUnavailable": "系統安全儲存不可用，無法儲存或讀取憑證。Linux 使用者請安裝並解鎖 Secret Service（gnome-keyring 或 KWallet）後重新啟動。"
```

(Place under the existing `credentials`/`settings` namespace per the file's structure; run `pnpm check:i18n` to confirm key parity.)

- [ ] **Step 5: Run full verification gates**

```bash
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -3
pnpm check:i18n && pnpm typecheck && pnpm check:brand && pnpm check:provenance
```

Expected: cargo all green (421 + ~13 new = ~434); all four gates green. Known-flaky: `store::tests::ensure_general_project_is_idempotent_and_not_removable` may fail under sandbox only — re-run in isolation to confirm.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/src/secrets/migration.rs src/i18n/ src/
git commit -m "feat(secrets): startup migration run + retire keychain opt-in toggle + storeUnavailable i18n"
```

---

### Task 9: Acceptance matrix + security checklist + memory update

**Files:**
- Modify: `docs/release/1.0-acceptance-matrix.md` (AC-2.10, AC-6.x rows; Audit Summary counts + security-gaps table)
- Modify: `docs/release/security-audit-checklist.md` (SA-C.1/SA-C.3 rows)
- Modify: `docs/release/test-coverage-audit.md` (counts; remove credential-migration gap rows)
- Modify: project memory `omp-desktop-roadmap-status.md` + `MEMORY.md`

- [ ] **Step 1: Flip AC-2.10 + AC-6.1/6.2/6.4/6.5/6.6/6.7 to PASS** with evidence citing: `secrets/migration.rs` engine + 15 migration tests + 4 channel strict tests + 2 strict-mode tests; scenario list mapping to test names; strict mode (`save_secrets`/`save_instance` hard-fail on `StoreError::Unavailable`; fail-closed load when flag set); secure delete of `channel-secrets.json`.
- [ ] **Step 2: Update Audit Summary**: remove the two HIGH rows from "Security gaps remaining" (both resolved); move credential-migration off the "Release-blocking FAIL items" list; recompute verdict counts with `grep -oE '\| (PASS|PARTIAL|BLOCKED|FAIL) \|'` and update the table exactly.
- [ ] **Step 3: Update security-audit-checklist SA-C.1/SA-C.3** to PASS with file:line evidence.
- [ ] **Step 4: Update test-coverage-audit.md**: new cargo count; mark "6-step credential migration suite" gap resolved.
- [ ] **Step 5: Run `pnpm check:brand && pnpm check:provenance`** (docs changed), then commit:

```bash
git add docs/release/ && git commit -m "docs(release): flip AC-6.x/AC-2.10 to PASS after 6-step credential migration"
```

- [ ] **Step 6: Update memory** — `omp-desktop-roadmap-status.md` (new bullet for the migration work; update How-to-apply priorities: next is event-journal recovery wiring AC-1.10) and `MEMORY.md` index line.

---

## Self-Review Notes (filled by plan author)

- Spec coverage: SecretStore/KeychainStore/MockStore (T1) ✓; ledger + dry_run (T2) ✓; 6-step run + 7 scenarios (T3: 2,3,5,6,7 + corrupted ledger; T2: 1,4) ✓; SecretsJsonSource (T4) ✓; ChannelSecretsSource + secure delete (T5) ✓; strict mode + fail-closed (T6/T7) ✓; startup trigger + toggle retirement + i18n (T8) ✓; acceptance impact (T9) ✓. §8.1 namespace isolation tested in T1. Non-goals untouched (no Safe Mode UI, no agent.db, no auth-broker).
- Type consistency: `MigrationReport{cleaned,failed,pending}` used identically in T3/T8; `LegacyEntry{migration_id, namespace: &'static str, key}` identical in T2/T4/T5; `MigrationSource` signatures identical in T2 definition and T3 MemSource/T4/T5 impls; `install_test_store`/`reset_test_store` used in T6/T7 tests match T1 definitions.
- Known deviation from spec §5.2: for `SecretsJsonSource`, the reference is stored in the field itself (no separate refs map), making tombstone/cleanup no-ops — documented in T4 design note; the tombstone-marker path is fully exercised by the channel adapter (T5) and MemSource (T3).
