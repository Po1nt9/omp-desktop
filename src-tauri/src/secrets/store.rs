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
        match entry.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(_) => false,
        }
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
        // Prefixed account always; the legacy unprefixed account only exists
        // for the provider namespace.
        let mut accounts = vec![Self::account(ns, key)];
        if ns == NS_PROVIDER {
            accounts.push(key.to_string());
        }
        for account in accounts {
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
        assert_eq!(
            s.set(NS_REMOTE, "k", "v").unwrap_err().message_key(),
            "credentials.storeUnavailable"
        );
        s.set_unavailable(false);

        s.set_fail_set(true);
        assert!(matches!(
            s.set(NS_PROVIDER, "k", "v"),
            Err(StoreError::Backend(_))
        ));
        s.set_fail_set(false);

        s.set(NS_PROVIDER, "k", "real").unwrap();
        s.set_corrupt_get(true);
        assert_eq!(
            s.get(NS_PROVIDER, "k").unwrap().as_deref(),
            Some("real-corrupted")
        );
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
