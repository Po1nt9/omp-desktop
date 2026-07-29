//! Outbound proxy resolution (NEW-02).
//!
//! Single source of truth for how the app and every child process reach the
//! network. Three modes (Settings → `proxy_mode`):
//!
//! - `system` (default): honor the OS proxy. On Windows the system proxy lives
//!   in the WinINET registry and is **not** exported as env vars, so GUI-spawned
//!   children (runtime, login, updaters) would silently bypass it — we read the
//!   registry and inject `HTTP_PROXY`/`HTTPS_PROXY` ourselves. On macOS we ask
//!   `scutil --proxy`. Plain env vars win when already present.
//! - `manual`: use `proxy_url` from Settings verbatim.
//! - `none`: force direct — children get proxy env vars stripped.
//!
//! Never log credentials: URLs are redacted to `scheme://host:port` via
//! [`redact_proxy_url`] before any tracing output.

use std::process::Command as StdCommand;

use tokio::process::Command as TokioCommand;

/// Resolved proxy decision for the current settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyDecision {
    /// Inherit whatever the process env already has (no injection).
    Inherit,
    /// Force-direct: strip proxy env vars from children, disable in reqwest.
    Direct,
    /// Use this proxy URL (inject into children, configure reqwest).
    Use {
        url: String,
        no_proxy: Option<String>,
    },
}

/// Env var names children understand (both cases for maximum tool coverage).
const PROXY_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

/// Redact userinfo from a proxy URL for logs: `http://u:p@h:1` → `http://h:1`.
pub fn redact_proxy_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("?");
            match u.port() {
                Some(p) => format!("{}://{}:{}", u.scheme(), host, p),
                None => format!("{}://{}", u.scheme(), host),
            }
        }
        Err(_) => "<unparseable-proxy-url>".into(),
    }
}

/// Basic sanity check for a user-entered proxy URL.
pub fn is_valid_proxy_url(url: &str) -> bool {
    match url::Url::parse(url.trim()) {
        Ok(u) => {
            matches!(u.scheme(), "http" | "https" | "socks5" | "socks5h")
                && u.host_str().is_some()
        }
        Err(_) => false,
    }
}

/// True when any proxy env var is already set on this process.
fn env_proxy_present() -> bool {
    PROXY_ENV_KEYS
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false))
}

/// Windows: read the WinINET proxy (`HKCU\...\Internet Settings`).
/// Returns a URL like `http://127.0.0.1:7890` when a proxy is enabled.
#[cfg(windows)]
fn system_proxy_url() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    // Formats: "host:port" or "http=host:port;https=host:port;…" — prefer https/http entry.
    if server.contains('=') {
        let mut http_entry = None;
        for part in server.split(';') {
            let mut kv = part.splitn(2, '=');
            let scheme = kv.next().unwrap_or("").trim().to_ascii_lowercase();
            let addr = kv.next().unwrap_or("").trim();
            if addr.is_empty() {
                continue;
            }
            if scheme == "https" {
                return Some(format!("http://{addr}"));
            }
            if scheme == "http" {
                http_entry = Some(format!("http://{addr}"));
            }
        }
        return http_entry;
    }
    Some(format!("http://{server}"))
}

/// macOS: `scutil --proxy` (GUI apps see no proxy env vars; the system proxy
/// lives in SystemConfiguration).
#[cfg(target_os = "macos")]
fn system_proxy_url() -> Option<String> {
    let out = StdCommand::new("scutil").arg("--proxy").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(" : ") {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let pick = |enable_key: &str, host_key: &str, port_key: &str| -> Option<String> {
        if map.get(enable_key).map(String::as_str) != Some("1") {
            return None;
        }
        let host = map.get(host_key)?;
        let port = map.get(port_key)?;
        Some(format!("http://{host}:{port}"))
    };
    pick("HTTPSEnable", "HTTPSProxy", "HTTPSPort")
        .or_else(|| pick("HTTPEnable", "HTTPProxy", "HTTPPort"))
        .or_else(|| {
            if map.get("SOCKSEnable").map(String::as_str) != Some("1") {
                return None;
            }
            let host = map.get("SOCKSProxy")?;
            let port = map.get("SOCKSPort")?;
            Some(format!("socks5://{host}:{port}"))
        })
}

/// Linux and others: env vars are the convention; nothing extra to read.
#[cfg(not(any(windows, target_os = "macos")))]
fn system_proxy_url() -> Option<String> {
    None
}

/// Resolve the proxy decision from persisted settings.
pub fn decision() -> ProxyDecision {
    let settings = crate::store::load_settings();
    decision_from(
        &settings.proxy_mode,
        settings.proxy_url.as_deref(),
        settings.proxy_no_proxy.as_deref(),
    )
}

/// Pure resolution used by [`decision`] and tests.
pub fn decision_from(
    mode: &str,
    manual_url: Option<&str>,
    no_proxy: Option<&str>,
) -> ProxyDecision {
    let no_proxy = no_proxy
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    match mode.trim().to_ascii_lowercase().as_str() {
        "none" => ProxyDecision::Direct,
        "manual" => {
            let url = manual_url.map(str::trim).unwrap_or("");
            if is_valid_proxy_url(url) {
                ProxyDecision::Use {
                    url: url.to_string(),
                    no_proxy,
                }
            } else {
                // Misconfigured manual proxy: fall back to inherit rather than
                // silently going direct — env vars may still be correct.
                tracing::warn!("proxy: manual mode with invalid url; inheriting env");
                ProxyDecision::Inherit
            }
        }
        // "system" and anything unknown.
        _ => {
            if env_proxy_present() {
                // Env already routes traffic (bat-file style launches keep working).
                return ProxyDecision::Inherit;
            }
            match system_proxy_url() {
                Some(url) => ProxyDecision::Use { url, no_proxy },
                None => ProxyDecision::Inherit,
            }
        }
    }
}

/// Loopback names that must never route through a proxy (mirror, local RPC).
const LOCAL_BYPASS: &str = "localhost,127.0.0.1,::1";

/// User no-proxy list merged with the always-on loopback bypass.
fn merged_no_proxy(no_proxy: Option<&str>) -> String {
    match no_proxy {
        Some(np) if !np.trim().is_empty() => format!("{LOCAL_BYPASS},{}", np.trim()),
        _ => LOCAL_BYPASS.to_string(),
    }
}

/// Env pairs to inject into a child process for the given decision.
/// `Direct` yields empty values (explicit unset happens in the apply fns).
pub fn child_env_pairs(dec: &ProxyDecision) -> Vec<(String, String)> {
    match dec {
        ProxyDecision::Inherit => Vec::new(),
        ProxyDecision::Direct => Vec::new(),
        ProxyDecision::Use { url, no_proxy } => {
            let mut pairs: Vec<(String, String)> = PROXY_ENV_KEYS
                .iter()
                .map(|k| (k.to_string(), url.clone()))
                .collect();
            let np = merged_no_proxy(no_proxy.as_deref());
            pairs.push(("NO_PROXY".into(), np.clone()));
            pairs.push(("no_proxy".into(), np));
            pairs
        }
    }
}

/// Apply the current proxy decision to a tokio child command (agent spawn, login).
pub fn apply_to_tokio_command(cmd: &mut TokioCommand) {
    let dec = decision();
    match &dec {
        ProxyDecision::Inherit => {}
        ProxyDecision::Direct => {
            for k in PROXY_ENV_KEYS {
                cmd.env_remove(k);
            }
        }
        ProxyDecision::Use { url, .. } => {
            tracing::info!("proxy: child uses {}", redact_proxy_url(url));
            for (k, v) in child_env_pairs(&dec) {
                cmd.env(k, v);
            }
        }
    }
}

/// Apply the current proxy decision to a std child command (probes, updaters).
pub fn apply_to_std_command(cmd: &mut StdCommand) {
    let dec = decision();
    match &dec {
        ProxyDecision::Inherit => {}
        ProxyDecision::Direct => {
            for k in PROXY_ENV_KEYS {
                cmd.env_remove(k);
            }
        }
        ProxyDecision::Use { url, .. } => {
            tracing::info!("proxy: child uses {}", redact_proxy_url(url));
            for (k, v) in child_env_pairs(&dec) {
                cmd.env(k, v);
            }
        }
    }
}

/// Configure a reqwest builder for the current proxy decision.
/// With the `system-proxy` feature reqwest already honors OS proxies in
/// `Inherit` mode; `Direct` disables proxying; `Use` pins the explicit URL.
pub fn apply_to_reqwest(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    match decision() {
        ProxyDecision::Inherit => builder,
        ProxyDecision::Direct => builder.no_proxy(),
        ProxyDecision::Use { url, no_proxy } => {
            let mut b = builder;
            match reqwest::Proxy::all(&url) {
                Ok(mut p) => {
                    // Loopback always bypasses (mirror / local tunnels).
                    p = p.no_proxy(reqwest::NoProxy::from_string(&merged_no_proxy(
                        no_proxy.as_deref(),
                    )));
                    b = b.proxy(p);
                }
                Err(e) => {
                    tracing::warn!(
                        "proxy: invalid proxy {} ({e}); using default routing",
                        redact_proxy_url(&url)
                    );
                }
            }
            b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_mode_uses_valid_url() {
        let d = decision_from("manual", Some("http://127.0.0.1:7890"), None);
        assert_eq!(
            d,
            ProxyDecision::Use {
                url: "http://127.0.0.1:7890".into(),
                no_proxy: None
            }
        );
    }

    #[test]
    fn manual_mode_with_bad_url_inherits() {
        assert_eq!(decision_from("manual", Some("not a url"), None), ProxyDecision::Inherit);
        assert_eq!(decision_from("manual", None, None), ProxyDecision::Inherit);
        // ftp / file schemes are rejected.
        assert_eq!(
            decision_from("manual", Some("file:///etc/passwd"), None),
            ProxyDecision::Inherit
        );
    }

    #[test]
    fn none_mode_forces_direct() {
        assert_eq!(decision_from("none", Some("http://127.0.0.1:1"), None), ProxyDecision::Direct);
    }

    #[test]
    fn no_proxy_list_is_propagated() {
        let d = decision_from(
            "manual",
            Some("http://127.0.0.1:7890"),
            Some("localhost,127.0.0.1"),
        );
        let pairs = child_env_pairs(&d);
        assert!(pairs.iter().any(|(k, v)| k == "HTTPS_PROXY" && v == "http://127.0.0.1:7890"));
        assert!(pairs.iter().any(|(k, v)| k == "NO_PROXY"
            && v == "localhost,127.0.0.1,::1,localhost,127.0.0.1"));
    }

    #[test]
    fn redacts_credentials_from_logs() {
        assert_eq!(
            redact_proxy_url("http://user:secret@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(redact_proxy_url("socks5://host"), "socks5://host");
        assert_eq!(redact_proxy_url("::::"), "<unparseable-proxy-url>");
    }

    #[test]
    fn validates_proxy_urls() {
        assert!(is_valid_proxy_url("http://127.0.0.1:7890"));
        assert!(is_valid_proxy_url("socks5://127.0.0.1:1080"));
        assert!(!is_valid_proxy_url("127.0.0.1:7890")); // scheme required
        assert!(!is_valid_proxy_url("ftp://x"));
        assert!(!is_valid_proxy_url(""));
    }
}
