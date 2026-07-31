//! Persist Remote IM channel instances under the app data root `remote/`.
//!
//! Channel credentials are strict-mode (design §8.1): values live in the OS
//! secure store under the isolated `remote` namespace; disk holds only
//! `keychain:v1:` references (`channel-secret-refs.json`). The legacy
//! plaintext `channel-secrets.json` is read only as a pre-migration fallback
//! and is removed by the §8.2 startup migration.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ChannelInstanceDto;
use crate::paths::{app_data_root, ensure_app_dirs};
use crate::secrets::migration::{
    is_reference, ledger_path, make_reference, read_channel_refs, write_channel_refs,
    MigrationLedger, TOMBSTONE,
};
use crate::secrets::store::{default_store, NS_REMOTE};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChannelsFile {
    instances: Vec<ChannelInstanceDto>,
}

fn remote_dir() -> PathBuf {
    let dir = app_data_root().join("remote");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(dir.join("logs"));
    dir
}

fn channels_path() -> PathBuf {
    remote_dir().join("channels.json")
}

pub(crate) fn secrets_path() -> PathBuf {
    remote_dir().join("channel-secrets.json")
}

pub(crate) fn channel_refs_path() -> PathBuf {
    remote_dir().join("channel-secret-refs.json")
}

pub fn list_instances() -> Vec<ChannelInstanceDto> {
    let path = channels_path();
    if !path.exists() {
        return vec![];
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<ChannelsFile>(&raw)
            .map(|f| f.instances)
            .unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn write_instances(list: &[ChannelInstanceDto]) -> Result<(), String> {
    let _ = ensure_app_dirs();
    let path = channels_path();
    let file = ChannelsFile {
        instances: list.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("serialize channels: {e}"))?;
    fs::write(&path, raw).map_err(|e| format!("write channels: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn load_secrets_map() -> HashMap<String, HashMap<String, String>> {
    let path = secrets_path();
    if !path.exists() {
        return HashMap::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_secrets_map(map: &HashMap<String, HashMap<String, String>>) -> Result<(), String> {
    let path = secrets_path();
    let raw = serde_json::to_string_pretty(map)
        .map_err(|e| format!("serialize secrets: {e}"))?;
    fs::write(&path, raw).map_err(|e| format!("write secrets: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Resolve an instance's channel credentials.
///
/// References (`channel-secret-refs.json`) win and are fetched from the OS
/// store; legacy plaintext rows are a pre-migration fallback only and fail
/// closed once the migration ledger recorded a store outage. Tombstoned or
/// reference-shaped legacy values are never returned. Never logs values.
pub fn get_secrets(instance_id: &str) -> HashMap<String, String> {
    let store_unavailable = MigrationLedger::is_store_unavailable(&ledger_path());
    let store = default_store();
    let mut out = HashMap::new();

    if let Some(fields) = read_channel_refs(&channel_refs_path()).get(instance_id).cloned() {
        for (field, reference) in fields {
            if !is_reference(&reference) {
                continue;
            }
            let key = format!("{instance_id}:{field}");
            match store.get(NS_REMOTE, &key) {
                Ok(Some(v)) if !v.is_empty() => {
                    out.insert(field, v);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "grok_app::remote_im",
                        instance_id,
                        field,
                        error = %e,
                        message_key = e.message_key(),
                        "failed to resolve channel secret reference from OS store"
                    );
                }
            }
        }
    }

    if let Some(row) = load_secrets_map().get(instance_id) {
        for (field, value) in row {
            if out.contains_key(field)
                || value.is_empty()
                || value == TOMBSTONE
                || is_reference(value)
            {
                continue;
            }
            if store_unavailable {
                tracing::warn!(
                    target: "grok_app::remote_im",
                    instance_id,
                    field,
                    message_key = "credentials.storeUnavailable",
                    "OS store unavailable; refusing to use plaintext channel secret on disk"
                );
                continue;
            }
            out.insert(field.clone(), value.clone());
        }
    }

    out
}

pub fn save_instance(
    inst: &ChannelInstanceDto,
    secrets: &HashMap<String, String>,
) -> Result<ChannelInstanceDto, String> {
    let mut list = list_instances();
    let mut saved = inst.clone();

    // Strict mode: key material goes through the OS store; disk keeps only
    // keychain:v1: references. A store outage blocks the save — no plaintext
    // is written anywhere (§8.1).
    let store = default_store();
    let mut refs = read_channel_refs(&channel_refs_path());
    let mut ref_row = refs.remove(&saved.id).unwrap_or_default();
    let mut all = load_secrets_map();
    let mut row = all.remove(&saved.id).unwrap_or_default();
    let mut stored_any = false;
    for (k, v) in secrets {
        let t = v.trim();
        if t.is_empty() {
            continue;
        }
        let key = format!("{}:{}", saved.id, k);
        store.set(NS_REMOTE, &key, t).map_err(|e| e.to_string())?;
        ref_row.insert(k.clone(), make_reference(NS_REMOTE, &key));
        // The value now lives only in the OS store — drop any legacy plaintext.
        row.remove(k);
        stored_any = true;
    }
    let has_refs = !ref_row.is_empty();
    if has_refs {
        refs.insert(saved.id.clone(), ref_row);
    }
    if stored_any {
        write_channel_refs(&channel_refs_path(), &refs)?;
    }
    let legacy_has = !row.is_empty();
    if legacy_has {
        all.insert(saved.id.clone(), row);
    } else {
        all.remove(&saved.id);
    }
    if stored_any {
        write_secrets_map(&all)?;
    }

    let has = has_refs || legacy_has;
    saved.has_credentials = has || inst.has_credentials;
    if saved.has_credentials && saved.enabled {
        saved.status = "configured".into();
    } else if saved.has_credentials {
        saved.status = "configured".into();
    } else {
        saved.status = "unconfigured".into();
    }
    saved.last_error = None;

    if let Some(i) = list.iter().position(|x| x.id == saved.id) {
        list[i] = saved.clone();
    } else {
        list.push(saved.clone());
    }
    write_instances(&list)?;

    Ok(saved)
}

pub fn delete_instance(instance_id: &str) -> Result<(), String> {
    let list: Vec<_> = list_instances()
        .into_iter()
        .filter(|x| x.id != instance_id)
        .collect();
    write_instances(&list)?;

    // Remove OS-store entries for every known field. Best-effort: a store
    // outage must not strand instance removal (orphaned entries are
    // re-collected by the next migration dry-run).
    let store = default_store();
    let mut refs = read_channel_refs(&channel_refs_path());
    let mut fields: Vec<String> = refs
        .get(instance_id)
        .map(|r| r.keys().cloned().collect())
        .unwrap_or_default();
    let mut all = load_secrets_map();
    if let Some(row) = all.get(instance_id) {
        for f in row.keys() {
            if !fields.contains(f) {
                fields.push(f.clone());
            }
        }
    }
    for field in fields {
        let key = format!("{instance_id}:{field}");
        if let Err(e) = store.delete(NS_REMOTE, &key) {
            tracing::warn!(
                target: "grok_app::remote_im",
                instance_id,
                field,
                error = %e,
                "failed to delete channel secret from OS store"
            );
        }
    }
    if refs.remove(instance_id).is_some() {
        write_channel_refs(&channel_refs_path(), &refs)?;
    }
    all.remove(instance_id);
    write_secrets_map(&all)?;
    Ok(())
}

/// Persist connector exit / bind errors so UI does not show a false "connected".
pub fn set_instance_last_error(instance_id: &str, err: Option<String>) -> Result<(), String> {
    let mut list = list_instances();
    let Some(row) = list.iter_mut().find(|x| x.id == instance_id) else {
        return Ok(());
    };
    row.last_error = err.clone();
    if err.is_some() {
        row.status = "error".into();
    } else if row.has_credentials {
        row.status = "configured".into();
    }
    write_instances(&list)?;
    Ok(())
}

/// Legacy path kept for doctor/docs; Rust runtime does not require Node config.toml.
pub fn bridge_data_dir() -> PathBuf {
    let dir = app_data_root().join("remote").join("bridge-data");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn bridge_config_path() -> PathBuf {
    bridge_data_dir().join("config.toml")
}

pub fn remote_log_path() -> PathBuf {
    remote_dir().join("logs").join("bridge.log")
}

/// Host-persisted Bridge switch (survives App restart).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgePersistedConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_lifecycle")]
    pub lifecycle: String,
    #[serde(default)]
    pub allow_remote_yolo: bool,
}

fn default_lifecycle() -> String {
    "attached".into()
}

impl Default for BridgePersistedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lifecycle: default_lifecycle(),
            allow_remote_yolo: false,
        }
    }
}

fn bridge_persist_path() -> PathBuf {
    remote_dir().join("bridge-config.json")
}

pub fn load_bridge_config() -> BridgePersistedConfig {
    let path = bridge_persist_path();
    if !path.is_file() {
        // Auto-enable when user already has bound channels (first migration).
        let has_ready = list_instances()
            .iter()
            .any(|i| i.enabled && i.has_credentials);
        return BridgePersistedConfig {
            enabled: has_ready,
            ..Default::default()
        };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_bridge_config(cfg: &BridgePersistedConfig) -> Result<(), String> {
    let _ = ensure_app_dirs();
    let path = bridge_persist_path();
    let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| format!("write bridge-config: {e}"))?;
    Ok(())
}

/// True when at least one channel can be connected.
pub fn has_ready_instances() -> bool {
    list_instances()
        .iter()
        .any(|i| i.enabled && i.has_credentials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::store::{install_test_store, reset_test_store, MockStore, SecretStore};
    use std::sync::Arc;

    /// Serialize env/home + global test store mutations (tests run in one process).
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        GUARD.lock().unwrap()
    }

    fn test_home(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!(
            "omp-remote-cfg-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    fn dto(id: &str) -> ChannelInstanceDto {
        ChannelInstanceDto {
            id: id.into(),
            channel: "telegram".into(),
            name: "Test".into(),
            enabled: true,
            has_credentials: false,
            options: serde_json::json!({}),
            acl: serde_json::json!({}),
            project_scope: serde_json::json!({}),
            presenter: "self".into(),
            status: "unconfigured".into(),
            last_error: None,
        }
    }

    fn secrets(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn save_instance_persists_secrets_via_store_without_plaintext() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = guard();
        let tmp = test_home("save");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        let saved = save_instance(&dto("inst1"), &secrets(&[("bot_token", "tg-secret-token")])).unwrap();
        assert!(saved.has_credentials);
        assert_eq!(saved.status, "configured");

        // Value lives in the OS store under the remote namespace…
        assert!(mock.contains(NS_REMOTE, "inst1:bot_token"));
        // …and disk holds only a reference.
        let refs = read_channel_refs(&channel_refs_path());
        assert_eq!(
            refs.get("inst1").and_then(|r| r.get("bot_token")).map(String::as_str),
            Some("keychain:v1:remote:inst1:bot_token")
        );
        let legacy = tmp.join("remote").join("channel-secrets.json");
        if legacy.is_file() {
            let raw = fs::read_to_string(&legacy).unwrap();
            assert!(!raw.contains("tg-secret-token"));
        }

        // get_secrets resolves the reference back through the store.
        let got = get_secrets("inst1");
        assert_eq!(got.get("bot_token").map(String::as_str), Some("tg-secret-token"));

        // Store outage blocks saving new credentials.
        mock.set_unavailable(true);
        let err = save_instance(&dto("inst2"), &secrets(&[("bot_token", "x")])).unwrap_err();
        assert!(err.contains("credentials.storeUnavailable"));
        assert!(!mock.contains(NS_REMOTE, "inst2:bot_token"));

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_secrets_prefers_refs_and_falls_back_to_legacy_plaintext() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = guard();
        let tmp = test_home("dual");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        // Legacy plaintext row (pre-migration).
        write_secrets_map(&HashMap::from([(
            "inst1".to_string(),
            secrets(&[("bot_token", "legacy-token"), ("app_secret", "legacy-secret")]),
        )]))
        .unwrap();
        // One field already migrated: reference + store value wins over legacy.
        mock.set(NS_REMOTE, "inst1:bot_token", "migrated-token").unwrap();
        let mut refs = HashMap::new();
        refs.insert(
            "inst1".to_string(),
            HashMap::from([(
                "bot_token".to_string(),
                "keychain:v1:remote:inst1:bot_token".to_string(),
            )]),
        );
        write_channel_refs(&channel_refs_path(), &refs).unwrap();

        let got = get_secrets("inst1");
        assert_eq!(got.get("bot_token").map(String::as_str), Some("migrated-token"));
        assert_eq!(got.get("app_secret").map(String::as_str), Some("legacy-secret"));

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_secrets_fails_closed_when_store_unavailable() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = guard();
        let tmp = test_home("closed");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        write_secrets_map(&HashMap::from([(
            "inst1".to_string(),
            secrets(&[("bot_token", "legacy-token"), ("done", TOMBSTONE)]),
        )]))
        .unwrap();
        let mut ledger = MigrationLedger::default();
        ledger.store_unavailable = true;
        ledger.save(&ledger_path()).unwrap();

        let got = get_secrets("inst1");
        assert!(got.get("bot_token").is_none());
        assert!(got.get("done").is_none());

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn delete_instance_removes_store_entries_refs_and_legacy_row() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = guard();
        let tmp = test_home("delete");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        save_instance(&dto("inst1"), &secrets(&[("bot_token", "tok")])).unwrap();
        write_secrets_map(&HashMap::from([(
            "inst1".to_string(),
            secrets(&[("legacy_field", "legacy-value")]),
        )]))
        .unwrap();
        mock.set(NS_REMOTE, "inst1:legacy_field", "legacy-value").unwrap();

        delete_instance("inst1").unwrap();

        assert!(!mock.contains(NS_REMOTE, "inst1:bot_token"));
        assert!(!mock.contains(NS_REMOTE, "inst1:legacy_field"));
        assert!(read_channel_refs(&channel_refs_path()).get("inst1").is_none());
        assert!(load_secrets_map().get("inst1").is_none());
        assert!(get_secrets("inst1").is_empty());

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// AC-8.7 / AC-8.17: key rotation — saving a new value for an existing
    /// field replaces it in the OS store immediately; the on-disk reference
    /// stays stable (one row per field) and no plaintext of either value
    /// touches disk.
    #[test]
    fn save_instance_rotates_existing_secret_in_place() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = guard();
        let tmp = test_home("rotate");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(MockStore::new());
        install_test_store(mock.clone());

        save_instance(&dto("inst1"), &secrets(&[("bot_token", "tok-v1")])).unwrap();
        assert_eq!(
            get_secrets("inst1").get("bot_token").map(String::as_str),
            Some("tok-v1")
        );

        save_instance(&dto("inst1"), &secrets(&[("bot_token", "tok-v2")])).unwrap();

        // The rotated value resolves immediately through the same reference.
        assert_eq!(
            get_secrets("inst1").get("bot_token").map(String::as_str),
            Some("tok-v2")
        );
        let refs = read_channel_refs(&channel_refs_path());
        let row = refs.get("inst1").expect("refs row for inst1");
        assert_eq!(row.len(), 1);
        assert_eq!(
            row.get("bot_token").map(String::as_str),
            Some("keychain:v1:remote:inst1:bot_token")
        );
        let legacy = tmp.join("remote").join("channel-secrets.json");
        if legacy.is_file() {
            let raw = fs::read_to_string(&legacy).unwrap();
            assert!(!raw.contains("tok-v1"));
            assert!(!raw.contains("tok-v2"));
        }

        reset_test_store();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }
}
