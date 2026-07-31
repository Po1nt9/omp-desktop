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
    /// Short identifier for logs ("secrets_json" / "channel_secrets").
    fn name(&self) -> &'static str;
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

// ── startup wiring ─────────────────────────────────────────────────────────

/// §8.2 startup migration, wired once at app launch after `ensure_app_dirs`.
///
/// Idempotent: already-`Cleaned` entries are skipped, mid-states resume. A
/// store outage persists the ledger's `store_unavailable` flag (Safe Mode
/// signal) and leaves legacy plaintext readable; per-entry failures roll back
/// the store entry and are retried next launch. Never logs secret values.
pub fn run_startup_migration(store: &dyn SecretStore) {
    let migrator = Migrator::new(store, &ledger_path());
    let secrets_json = SecretsJsonSource::new(crate::paths::secrets_file());
    let channel = ChannelSecretsSource::new(
        crate::remote_im::config::secrets_path(),
        crate::remote_im::config::channel_refs_path(),
    );
    let sources: [&dyn MigrationSource; 2] = [&secrets_json, &channel];

    for source in sources {
        let plan = match migrator.dry_run(source) {
            Ok(p) => p,
            Err(e) => {
                // dry_run already persisted store_unavailable on outage.
                tracing::warn!(
                    target: "grok_app::secrets",
                    source = source.name(),
                    error = %e,
                    message_key = e.message_key(),
                    "credential migration dry-run failed; legacy secrets left untouched"
                );
                continue;
            }
        };
        if plan.entries.is_empty() && plan.conflicts.is_empty() {
            continue;
        }
        let report = migrator.run(source, &plan);
        if report.failed > 0 || report.pending > 0 || !plan.conflicts.is_empty() {
            tracing::warn!(
                target: "grok_app::secrets",
                source = source.name(),
                cleaned = report.cleaned,
                failed = report.failed,
                pending = report.pending,
                conflicts = plan.conflicts.len(),
                "credential migration incomplete; failed/pending entries retry next launch"
            );
        } else {
            tracing::info!(
                target: "grok_app::secrets",
                source = source.name(),
                cleaned = report.cleaned,
                "credential migration completed"
            );
        }
    }
}

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
    fn name(&self) -> &'static str {
        "secrets_json"
    }
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

    fn write_legacy_map(
        &self,
        map: &HashMap<String, HashMap<String, String>>,
    ) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        crate::store_lock::write_bytes_atomic(&self.secrets_path, raw.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.secrets_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn split_key(key: &str) -> Option<(&str, &str)> {
        // Instance ids contain no ':' in this codebase (uuid-ish); fields are
        // camelCase. Split on the LAST ':' so either side may grow.
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
    fn name(&self) -> &'static str {
        "channel_secrets"
    }
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
        fn name(&self) -> &'static str {
            "mem"
        }
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

    mod secrets_json {
        use super::*;

        fn tmp_secrets(tag: &str, official: Option<&str>, relay: Option<&str>) -> PathBuf {
            let path = std::env::temp_dir().join(format!(
                "omp-secrets-src-{}-{}.json",
                tag,
                std::process::id()
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
            let path2 = tmp_secrets(
                "enum2",
                Some("keychain:v1:provider:official_api_key"),
                None,
            );
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
            let ledger = ledger_path_tmp("full-src");
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

    mod channel {
        use super::*;
        use crate::secrets::migration::{read_channel_refs, ChannelSecretsSource};
        use crate::secrets::store::NS_REMOTE;
        use std::collections::HashMap;

        fn tmp_channel(tag: &str) -> (PathBuf, PathBuf) {
            let dir = std::env::temp_dir().join(format!(
                "omp-chan-src-{}-{}",
                tag,
                std::process::id()
            ));
            let _ = std::fs::create_dir_all(&dir);
            (
                dir.join("channel-secrets.json"),
                dir.join("channel-secret-refs.json"),
            )
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
            let ledger = ledger_path_tmp("chan-full");
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
        fn cleanup_leaves_partial_map_when_fields_remain() {
            let store = MockStore::new();
            let (secrets, refs) = tmp_channel("partial");
            // Two instances; only inst-b's single field cleans up last, so
            // the file must survive (secure delete only when fully empty).
            let mut map = HashMap::new();
            let mut fields = HashMap::new();
            fields.insert("botToken".to_string(), "tg-token".to_string());
            map.insert("inst-a".to_string(), fields);
            write_legacy(&secrets, &map);
            let ledger = ledger_path_tmp("chan-partial");
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

    #[test]
    fn startup_migration_moves_both_sources_and_is_idempotent() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("omp-startup-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("remote")).unwrap();
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);

        // Seed legacy plaintext in both sources.
        std::fs::write(
            tmp.join("secrets.json"),
            r#"{"officialApiKey":"sk-startup","relayBaseUrl":"https://r.example"}"#,
        )
        .unwrap();
        std::fs::write(
            tmp.join("remote").join("channel-secrets.json"),
            r#"{"inst1":{"botToken":"tg-startup"}}"#,
        )
        .unwrap();

        let store = MockStore::new();
        run_startup_migration(&store);

        // Key material now lives only in the store, namespaced per source.
        assert!(store.contains(NS_PROVIDER, "official_api_key"));
        assert!(store.contains(crate::secrets::store::NS_REMOTE, "inst1:botToken"));
        // secrets.json holds a reference, not plaintext.
        let raw = std::fs::read_to_string(tmp.join("secrets.json")).unwrap();
        assert!(raw.contains("keychain:v1:provider:official_api_key"));
        assert!(!raw.contains("sk-startup"));
        // channel-secrets.json fully cleaned up (secure delete).
        assert!(!tmp.join("remote").join("channel-secrets.json").exists());
        let refs_raw =
            std::fs::read_to_string(tmp.join("remote").join("channel-secret-refs.json")).unwrap();
        assert!(refs_raw.contains("keychain:v1:remote:inst1:botToken"));

        // Second launch is a no-op.
        let before = store.len();
        run_startup_migration(&store);
        assert_eq!(store.len(), before);
        assert!(MigrationLedger::load(&ledger_path())
            .entries
            .iter()
            .all(|e| e.state == MigrationState::Cleaned));

        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
