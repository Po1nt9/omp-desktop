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

    /// True when a previous run aborted on store-unavailable.
    pub fn is_store_unavailable(path: &Path) -> bool {
        MigrationLedger::load(path).store_unavailable
    }
}

/// App-wide ledger location: `<app_data_root>/credential-migration.json`.
pub fn ledger_path() -> PathBuf {
    crate::paths::app_data_root().join("credential-migration.json")
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::store::{MockStore, NS_PROVIDER};

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

    fn ledger_path_tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omp-mig-test-{}-{}.json",
            tag,
            std::process::id()
        ))
    }

    #[test]
    fn dry_run_enumerates_without_writing() {
        let store = MockStore::new();
        let path = ledger_path_tmp("dry");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
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
        let path = ledger_path_tmp("unavail");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
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
        let path = ledger_path_tmp("rerun");
        let mut ledger = MigrationLedger::default();
        ledger.upsert(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            MigrationState::Cleaned,
            None,
        );
        ledger.save(&path).unwrap();
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).expect("dry run");
        assert!(plan.entries.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn run_success_migrates_end_to_end() {
        let store = MockStore::new();
        let path = ledger_path_tmp("ok");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
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
        assert_eq!(
            ledger.state_of("provider:official_api_key"),
            MigrationState::Cleaned
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rerun_after_success_is_noop() {
        let store = MockStore::new();
        let path = ledger_path_tmp("noop");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        mig.run(&source, &plan);
        // Second run: plan empty (entry no longer enumerated after cleanup,
        // and the ledger would skip it anyway).
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
        let path = ledger_path_tmp("rb");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
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
        assert_eq!(
            ledger.state_of("provider:official_api_key"),
            MigrationState::Failed
        );
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
        let path = ledger_path_tmp("cl");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
        *source.fail_cleanup.lock().unwrap() = true;
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        let report = mig.run(&source, &plan);
        assert_eq!(report.pending, 1);
        assert_eq!(report.failed, 0);
        // Tombstone in place; value still resolvable from the store.
        assert_eq!(
            source
                .values
                .lock()
                .unwrap()
                .get("official_api_key")
                .map(|s| s.as_str()),
            Some(TOMBSTONE)
        );
        assert_eq!(
            store.get(NS_PROVIDER, "official_api_key").unwrap().as_deref(),
            Some("sk-live")
        );
        // Retry completes cleanup (resume: entry re-enters dry_run because
        // its ledger state is Tombstoned and the store resolves it).
        *source.fail_cleanup.lock().unwrap() = false;
        let plan2 = mig.dry_run(&source).unwrap();
        let report2 = mig.run(&source, &plan2);
        assert_eq!(report2.cleaned, 1);
        assert_eq!(
            MigrationLedger::load(&path).state_of("provider:official_api_key"),
            MigrationState::Cleaned
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reference_commit_failure_rolls_back_store() {
        let store = MockStore::new();
        let path = ledger_path_tmp("rb7");
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
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
        assert_eq!(
            MigrationLedger::load(&path).state_of("provider:official_api_key"),
            MigrationState::Failed
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupted_ledger_recovers_to_dry_run() {
        let store = MockStore::new();
        let path = ledger_path_tmp("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let source = MemSource::single(
            "provider:official_api_key",
            NS_PROVIDER,
            "official_api_key",
            "sk-live",
        );
        let mig = Migrator::new(&store, &path);
        let plan = mig.dry_run(&source).unwrap();
        assert_eq!(plan.entries.len(), 1);
        let report = mig.run(&source, &plan);
        assert_eq!(report.cleaned, 1);
        let _ = std::fs::remove_file(&path);
    }
}
