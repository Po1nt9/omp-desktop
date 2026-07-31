//! App secrets backend: OS secure store only (strict mode, master design §8.1).
//!
//! Callers use [`crate::secrets::load_secrets`] / [`crate::secrets::save_secrets`] only —
//! this module owns where `official_api_key` / `relay_api_key` actually live.
//!
//! - macOS: Keychain via `keyring` (`apple-native`)
//! - Windows: Credential Manager (`windows-native`)
//! - Linux: FreeDesktop Secret Service when available (`sync-secret-service`)
//!
//! There is **no plaintext fallback**: when the OS store is unavailable, saving
//! is blocked with an actionable error and `secrets.json` holds at most opaque
//! `keychain:v1:<ns>:<key>` references written by the §8.2 migration.
//!
//! Non-secret metadata (`relay_base_url`, `default_model`, `keychain_has_*`) always
//! stays in `secrets.json`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::paths::{ensure_app_dirs, secrets_file};
use crate::store::SecretsFile;

pub mod migration;
pub mod store;

use migration::{is_reference, ledger_path, make_reference, MigrationLedger};
use store::{default_store, SecretStore, NS_PROVIDER};

const KEY_OFFICIAL: &str = "official_api_key";
const KEY_RELAY: &str = "relay_api_key";

/// Process-lifetime cache after first full unlock — avoids re-prompting Keychain
/// on every `load_secrets()` within the same app session.
static SESSION_CACHE: Mutex<Option<SecretsFile>> = Mutex::new(None);

fn store_handle() -> Arc<dyn SecretStore> {
    default_store()
}

/// Resolve one sensitive field read from `secrets.json`.
///
/// - `keychain:v1:` reference → fetch from the OS store (strict mode).
/// - Plaintext leftover (pre-migration) → passed through only while the store is
///   healthy; once the ledger recorded `store_unavailable`, fail closed (None)
///   so callers cannot act on credentials the store can no longer protect.
///
/// Never logs secret values.
fn resolve_field(
    value: Option<String>,
    key: &str,
    store_unavailable: bool,
    store: &dyn SecretStore,
) -> Option<String> {
    let v = value.filter(|s| !s.is_empty())?;
    if is_reference(&v) {
        match store.get(NS_PROVIDER, key) {
            Ok(val) => val.filter(|s| !s.is_empty()),
            Err(e) => {
                tracing::warn!(
                    target: "grok_app::secrets",
                    field = key,
                    error = %e,
                    message_key = e.message_key(),
                    "failed to resolve secret reference from OS store"
                );
                None
            }
        }
    } else if store_unavailable {
        tracing::warn!(
            target: "grok_app::secrets",
            field = key,
            message_key = "credentials.storeUnavailable",
            "OS store unavailable; refusing to use plaintext secret on disk"
        );
        None
    } else {
        Some(v)
    }
}

fn non_empty(s: &Option<String>) -> bool {
    s.as_ref().map(|v| !v.is_empty()).unwrap_or(false)
}

/// True when the on-disk payload still holds plaintext API keys that should migrate.
pub fn disk_has_plaintext_keys(disk: &SecretsFile) -> bool {
    non_empty(&disk.official_api_key) || non_empty(&disk.relay_api_key)
}

/// Whether UI / setup should treat an official key as configured.
/// Uses plaintext on disk **or** keychain presence flag — never unlocks Keychain.
pub fn has_official_key_configured(disk: &SecretsFile) -> bool {
    non_empty(&disk.official_api_key) || disk.keychain_has_official
}

/// Whether a relay key is configured (disk plaintext or keychain flag).
pub fn has_relay_key_configured(disk: &SecretsFile) -> bool {
    non_empty(&disk.relay_api_key) || disk.keychain_has_relay
}

/// Disk payload with sensitive key fields stripped (metadata + presence flags kept).
pub fn strip_keys_for_disk(s: &SecretsFile) -> SecretsFile {
    SecretsFile {
        official_api_key: None,
        relay_api_key: None,
        relay_base_url: s.relay_base_url.clone(),
        default_model: s.default_model.clone(),
        keychain_has_official: s.keychain_has_official,
        keychain_has_relay: s.keychain_has_relay,
    }
}

/// Merge keychain (preferred) over disk for sensitive fields; metadata from disk.
pub fn merge_secrets(disk: SecretsFile, from_keychain: SecretsFile) -> SecretsFile {
    let official = from_keychain
        .official_api_key
        .filter(|k| !k.is_empty())
        .or(disk.official_api_key.filter(|k| !k.is_empty()));
    let relay = from_keychain
        .relay_api_key
        .filter(|k| !k.is_empty())
        .or(disk.relay_api_key.filter(|k| !k.is_empty()));
    SecretsFile {
        // Presence flags: true if we have a value or disk said keychain holds it.
        keychain_has_official: non_empty(&official)
            || disk.keychain_has_official
            || from_keychain.keychain_has_official,
        keychain_has_relay: non_empty(&relay)
            || disk.keychain_has_relay
            || from_keychain.keychain_has_relay,
        official_api_key: official,
        relay_api_key: relay,
        relay_base_url: disk.relay_base_url,
        default_model: disk.default_model,
    }
}

pub(crate) fn read_disk_secrets(path: &PathBuf) -> SecretsFile {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => SecretsFile::default(),
    }
}

pub(crate) fn write_disk_secrets(path: &PathBuf, value: &SecretsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    // Same exclusive-lock + temp-rename path as the session store.
    crate::store_lock::write_bytes_atomic(path, s.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // write_bytes_atomic creates the final file; ensure mode is private.
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn invalidate_session_cache() {
    *SESSION_CACHE.lock() = None;
}

/// Read `secrets.json` only — no Keychain unlock. Safe for cold-start UI.
pub fn load_secrets_disk_only() -> SecretsFile {
    let _ = ensure_app_dirs();
    read_disk_secrets(&secrets_file())
}

/// Load secrets with values.
///
/// Strict mode: `keychain:v1:` references are resolved through the OS store;
/// plaintext leftovers pass through only until the migration ledger records a
/// store outage, after which loads fail closed (None) for those fields.
///
/// Use [`load_secrets_disk_only`] when you only need presence / metadata.
pub fn load_secrets() -> SecretsFile {
    {
        let cache = SESSION_CACHE.lock();
        if let Some(ref s) = *cache {
            return s.clone();
        }
    }

    let _ = ensure_app_dirs();
    let path = secrets_file();
    let disk = read_disk_secrets(&path);
    let store_unavailable = MigrationLedger::is_store_unavailable(&ledger_path());
    let store = store_handle();

    let official = resolve_field(disk.official_api_key.clone(), KEY_OFFICIAL, store_unavailable, store.as_ref());
    let relay = resolve_field(disk.relay_api_key.clone(), KEY_RELAY, store_unavailable, store.as_ref());

    let merged = SecretsFile {
        official_api_key: official,
        relay_api_key: relay,
        relay_base_url: disk.relay_base_url,
        default_model: disk.default_model,
        keychain_has_official: disk.keychain_has_official,
        keychain_has_relay: disk.keychain_has_relay,
    };
    *SESSION_CACHE.lock() = Some(merged.clone());
    merged
}

/// Save secrets. Strict mode: key material always goes through the OS store;
/// `secrets.json` only ever receives `keychain:v1:` references. A store outage
/// blocks the save with an actionable error — never a plaintext fallback.
pub fn save_secrets(s: &SecretsFile) -> Result<(), String> {
    let _ = ensure_app_dirs();
    let path = secrets_file();
    invalidate_session_cache();
    let store = store_handle();

    let mut disk = SecretsFile {
        official_api_key: None,
        relay_api_key: None,
        relay_base_url: s.relay_base_url.clone(),
        default_model: s.default_model.clone(),
        keychain_has_official: false,
        keychain_has_relay: false,
    };

    match &s.official_api_key {
        Some(k) if !k.is_empty() => {
            store
                .set(NS_PROVIDER, KEY_OFFICIAL, k)
                .map_err(|e| e.to_string())?;
            disk.official_api_key = Some(make_reference(NS_PROVIDER, KEY_OFFICIAL));
            disk.keychain_has_official = true;
        }
        _ => {
            if s.keychain_has_official || non_empty(&s.official_api_key) {
                store
                    .delete(NS_PROVIDER, KEY_OFFICIAL)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    match &s.relay_api_key {
        Some(k) if !k.is_empty() => {
            store
                .set(NS_PROVIDER, KEY_RELAY, k)
                .map_err(|e| e.to_string())?;
            disk.relay_api_key = Some(make_reference(NS_PROVIDER, KEY_RELAY));
            disk.keychain_has_relay = true;
        }
        _ => {
            if s.keychain_has_relay || non_empty(&s.relay_api_key) {
                store
                    .delete(NS_PROVIDER, KEY_RELAY)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    write_disk_secrets(&path, &disk)?;
    let cached = SecretsFile {
        official_api_key: s.official_api_key.clone(),
        relay_api_key: s.relay_api_key.clone(),
        relay_base_url: disk.relay_base_url.clone(),
        default_model: disk.default_model.clone(),
        keychain_has_official: disk.keychain_has_official,
        keychain_has_relay: disk.keychain_has_relay,
    };
    *SESSION_CACHE.lock() = Some(cached);
    Ok(())
}

/// Full wipe of app secrets (OS store + disk file). Used by reset_app_data.
pub fn wipe_all_secrets() -> Result<(), String> {
    invalidate_session_cache();
    let store = store_handle();
    // Best-effort: a store outage must not block a full app-data reset.
    for key in [KEY_OFFICIAL, KEY_RELAY] {
        if let Err(e) = store.delete(NS_PROVIDER, key) {
            tracing::warn!(
                target: "grok_app::secrets",
                field = key,
                error = %e,
                "failed to delete secret from OS store during wipe"
            );
        }
    }
    let path = secrets_file();
    if path.is_file() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_has_plaintext_keys_detects_present() {
        let empty = SecretsFile::default();
        assert!(!disk_has_plaintext_keys(&empty));

        let blank = SecretsFile {
            official_api_key: Some(String::new()),
            relay_api_key: Some("".into()),
            ..Default::default()
        };
        assert!(!disk_has_plaintext_keys(&blank));

        let only_official = SecretsFile {
            official_api_key: Some("sk-test-key-value".into()),
            ..Default::default()
        };
        assert!(disk_has_plaintext_keys(&only_official));

        let only_relay = SecretsFile {
            relay_api_key: Some("rk-test".into()),
            ..Default::default()
        };
        assert!(disk_has_plaintext_keys(&only_relay));
    }

    #[test]
    fn presence_uses_keychain_flags_without_values() {
        let disk = SecretsFile {
            keychain_has_official: true,
            keychain_has_relay: false,
            ..Default::default()
        };
        assert!(has_official_key_configured(&disk));
        assert!(!has_relay_key_configured(&disk));
        assert!(!disk_has_plaintext_keys(&disk));
    }

    #[test]
    fn strip_keys_for_disk_keeps_metadata_and_flags() {
        let s = SecretsFile {
            official_api_key: Some("sk-secret".into()),
            relay_api_key: Some("rk-secret".into()),
            relay_base_url: Some("https://relay.example".into()),
            default_model: Some("grok-4".into()),
            keychain_has_official: true,
            keychain_has_relay: true,
        };
        let disk = strip_keys_for_disk(&s);
        assert!(disk.official_api_key.is_none());
        assert!(disk.relay_api_key.is_none());
        assert_eq!(disk.relay_base_url.as_deref(), Some("https://relay.example"));
        assert_eq!(disk.default_model.as_deref(), Some("grok-4"));
        assert!(disk.keychain_has_official);
        assert!(disk.keychain_has_relay);
    }

    #[test]
    fn merge_prefers_keychain_over_disk() {
        let disk = SecretsFile {
            official_api_key: Some("disk-old".into()),
            relay_api_key: None,
            relay_base_url: Some("https://example.com".into()),
            default_model: Some("m1".into()),
            keychain_has_official: false,
            keychain_has_relay: false,
        };
        let kc = SecretsFile {
            official_api_key: Some("kc-new".into()),
            relay_api_key: Some("kc-relay".into()),
            ..Default::default()
        };
        let m = merge_secrets(disk, kc);
        assert_eq!(m.official_api_key.as_deref(), Some("kc-new"));
        assert_eq!(m.relay_api_key.as_deref(), Some("kc-relay"));
        assert_eq!(m.relay_base_url.as_deref(), Some("https://example.com"));
        assert_eq!(m.default_model.as_deref(), Some("m1"));
        assert!(m.keychain_has_official);
        assert!(m.keychain_has_relay);
    }

    #[test]
    fn merge_falls_back_to_disk_when_keychain_empty() {
        let disk = SecretsFile {
            official_api_key: Some("disk-key".into()),
            relay_api_key: Some("disk-relay".into()),
            relay_base_url: Some("https://x".into()),
            default_model: None,
            ..Default::default()
        };
        let kc = SecretsFile::default();
        let m = merge_secrets(disk, kc);
        assert_eq!(m.official_api_key.as_deref(), Some("disk-key"));
        assert_eq!(m.relay_api_key.as_deref(), Some("disk-relay"));
    }

    #[test]
    fn strip_roundtrip_json_has_no_keys() {
        let s = SecretsFile {
            official_api_key: Some("sk-should-not-serialize".into()),
            relay_api_key: Some("rk-nope".into()),
            relay_base_url: Some("https://relay.example".into()),
            default_model: Some("g".into()),
            keychain_has_official: true,
            keychain_has_relay: false,
        };
        let disk = strip_keys_for_disk(&s);
        let json = serde_json::to_string(&disk).unwrap();
        assert!(!json.contains("sk-should-not-serialize"));
        assert!(!json.contains("rk-nope"));
        assert!(json.contains("relay.example") || json.contains("relayBaseUrl"));
        assert!(json.contains("keychainHasOfficial") || json.contains("true"));
    }

    #[test]
    fn presence_helpers_do_not_need_key_material() {
        let s = SecretsFile {
            keychain_has_official: true,
            keychain_has_relay: true,
            official_api_key: None,
            relay_api_key: None,
            ..Default::default()
        };
        assert!(has_official_key_configured(&s));
        assert!(has_relay_key_configured(&s));
        assert!(!disk_has_plaintext_keys(&s));
    }

    #[test]
    fn file_write_preserves_keys_when_using_full_payload() {
        let tmp = std::env::temp_dir().join(format!(
            "grok-app-secrets-file-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("secrets.json");
        let s = SecretsFile {
            official_api_key: Some("sk-file-only".into()),
            relay_api_key: Some("rk-file".into()),
            relay_base_url: Some("https://f".into()),
            default_model: Some("m".into()),
            ..Default::default()
        };
        write_disk_secrets(&path, &s).unwrap();
        let back = read_disk_secrets(&path);
        assert_eq!(back.official_api_key.as_deref(), Some("sk-file-only"));
        assert_eq!(back.relay_api_key.as_deref(), Some("rk-file"));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Serialize env/home + global test store mutations (tests run in one process).
    fn strict_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        GUARD.lock().unwrap()
    }

    fn strict_test_home(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("omp-secrets-strict-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn save_then_load_roundtrips_through_store_without_plaintext_on_disk() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = strict_test_guard();
        let tmp = strict_test_home("roundtrip");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(store::MockStore::new());
        store::install_test_store(mock.clone());
        invalidate_session_cache();

        let s = SecretsFile {
            official_api_key: Some("sk-strict-official".into()),
            relay_api_key: Some("rk-strict-relay".into()),
            relay_base_url: Some("https://relay.example".into()),
            default_model: Some("grok-4".into()),
            ..Default::default()
        };
        save_secrets(&s).unwrap();

        // Disk holds references only — never key material.
        let raw = fs::read_to_string(tmp.join("secrets.json")).unwrap();
        assert!(!raw.contains("sk-strict-official"));
        assert!(!raw.contains("rk-strict-relay"));
        assert!(raw.contains("keychain:v1:provider:official_api_key"));
        assert!(raw.contains("keychain:v1:provider:relay_api_key"));

        // Load resolves references back through the store.
        invalidate_session_cache();
        let loaded = load_secrets();
        assert_eq!(loaded.official_api_key.as_deref(), Some("sk-strict-official"));
        assert_eq!(loaded.relay_api_key.as_deref(), Some("rk-strict-relay"));
        assert_eq!(loaded.relay_base_url.as_deref(), Some("https://relay.example"));
        assert!(loaded.keychain_has_official && loaded.keychain_has_relay);

        // Clearing a field deletes the store entry (None = cleared semantics).
        save_secrets(&SecretsFile {
            keychain_has_official: true,
            keychain_has_relay: true,
            relay_base_url: Some("https://relay.example".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(!mock.contains(NS_PROVIDER, KEY_OFFICIAL));
        assert!(!mock.contains(NS_PROVIDER, KEY_RELAY));
        invalidate_session_cache();
        let cleared = load_secrets();
        assert!(cleared.official_api_key.is_none());
        assert!(cleared.relay_api_key.is_none());

        store::reset_test_store();
        invalidate_session_cache();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn store_unavailable_blocks_save_and_fails_load_closed() {
        let _env = crate::paths::APP_HOME_ENV_LOCK.lock();
        let _g = strict_test_guard();
        let tmp = strict_test_home("unavailable");
        std::env::set_var("OMP_DESKTOP_HOME", &tmp);
        let mock = Arc::new(store::MockStore::new());
        store::install_test_store(mock.clone());
        invalidate_session_cache();

        // Store outage → save is blocked, nothing touches disk or store.
        mock.set_unavailable(true);
        let s = SecretsFile {
            official_api_key: Some("sk-blocked".into()),
            ..Default::default()
        };
        let err = save_secrets(&s).expect_err("save must fail when store is unavailable");
        assert!(err.contains("credentials.storeUnavailable"));
        assert!(!tmp.join("secrets.json").is_file());
        assert!(!mock.contains(NS_PROVIDER, KEY_OFFICIAL));

        // Pre-migration plaintext on disk + recorded store outage → fail closed.
        mock.set_unavailable(false);
        write_disk_secrets(
            &tmp.join("secrets.json"),
            &SecretsFile {
                official_api_key: Some("sk-legacy-plain".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut ledger = MigrationLedger::default();
        ledger.store_unavailable = true;
        ledger.save(&ledger_path()).unwrap();

        invalidate_session_cache();
        let loaded = load_secrets();
        assert!(loaded.official_api_key.is_none());

        // Healthy store + no outage flag → plaintext leftover still readable.
        ledger.store_unavailable = false;
        ledger.save(&ledger_path()).unwrap();
        invalidate_session_cache();
        let loaded = load_secrets();
        assert_eq!(loaded.official_api_key.as_deref(), Some("sk-legacy-plain"));

        store::reset_test_store();
        invalidate_session_cache();
        std::env::remove_var("OMP_DESKTOP_HOME");
        let _ = fs::remove_dir_all(&tmp);
    }
}
