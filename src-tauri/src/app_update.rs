//! Check for newer App releases on GitHub (manual fallback path).
//!
//! Prefer the Tauri updater plugin (`updater` module) when the binary was built
//! with `OMP_DESKTOP_UPDATER_*` secrets — that path downloads, verifies, installs, and
//! relaunches. This module remains for:
//! - Local / unsigned builds (plugin not registered)
//! - Linux `.deb` / `.rpm` installs (in-place update unsupported)
//! - Settings → About "open release page" fallback
//!
//! Strategy:
//! 1. GitHub REST `GET /repos/.../releases/latest` (rich payload: body, assets).
//! 2. On API failure (rate limit 403/429, network, etc.) fall back to following
//!    `https://github.com/.../releases/latest` redirect and parsing the tag
//!    from the final URL — no API quota, works on shared IPs.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

const DEFAULT_RELEASES_API_URL: &str =
    "https://api.github.com/repos/Po1nt9/omp-desktop/releases/latest";
const DEFAULT_RELEASES_HTML_URL: &str = "https://github.com/Po1nt9/omp-desktop/releases/latest";
const DEFAULT_RELEASES_PAGE: &str = "https://github.com/Po1nt9/omp-desktop/releases";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    pub body: Option<String>,
    /// Download asset names on the release (for UI hints; not auto-fetched).
    pub asset_names: Vec<String>,
    /// Best-effort direct installer URL for this platform (if assets list one).
    pub download_url: Option<String>,
    pub download_name: Option<String>,
}

/// Strip optional `v` / `V` prefix and parse `major.minor.patch` (extra suffix ignored).
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    if s.is_empty() {
        return None;
    }
    // Drop pre-release / build metadata: 1.2.3-beta.1+meta → 1.2.3
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Full semver parse (prerelease-aware) for update comparison.
fn parse_full_semver(raw: &str) -> Option<semver::Version> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(s).ok()
}

/// True when `remote` is a higher semver than `current` (prerelease-aware:
/// `0.3.1-nightly.20260731` > `0.3.1-nightly.20260730`, `1.0.0` >
/// `1.0.0-nightly.1`). Unparseable input → false (never offer a bogus update).
pub fn is_remote_newer(current: &str, remote: &str) -> bool {
    match (parse_full_semver(current), parse_full_semver(remote)) {
        (Some(a), Some(b)) => b > a,
        _ => false,
    }
}

/// Pick the newest release belonging to `channel` from a `/releases` list
/// payload (drafts excluded). Channel membership is defined by the tag's
/// prerelease segment via [`crate::update_channel::UpdateChannel::owns_tag`];
/// ordering is semver, not list position (AC-10.9, D6).
pub fn select_release_for_channel<'a>(
    releases: &'a [Value],
    channel: crate::update_channel::UpdateChannel,
) -> Option<&'a Value> {
    releases
        .iter()
        .filter(|r| r.get("draft").and_then(|d| d.as_bool()) != Some(true))
        .filter(|r| {
            r.get("tag_name")
                .and_then(|t| t.as_str())
                .map(|t| channel.owns_tag(t))
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            let ta = a.get("tag_name").and_then(|t| t.as_str()).unwrap_or("");
            let tb = b.get("tag_name").and_then(|t| t.as_str()).unwrap_or("");
            parse_full_semver(ta).cmp(&parse_full_semver(tb))
        })
}

fn pick_platform_asset(assets: Option<&Vec<Value>>) -> (Option<String>, Option<String>) {
    let Some(arr) = assets else {
        return (None, None);
    };
    // Prefer exact installers first (setup.exe / dmg / AppImage), then arch tokens.
    let prefer: &[&str] = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            &["aarch64", "arm64", "apple-silicon", ".dmg", "macos"]
        } else {
            &["x64", "x86_64", ".dmg", "macos"]
        }
    } else if cfg!(target_os = "windows") {
        &["-setup.exe", "setup.exe", ".msi", "x64", "windows", ".exe"]
    } else {
        &[".appimage", "appimage", ".deb", "linux"]
    };
    let mut best: Option<(usize, String, String)> = None; // score, name, url
    for a in arr {
        let name = match a.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let url = match a.get("browser_download_url").and_then(|u| u.as_str()) {
            Some(u) if u.starts_with("https://") => u.to_string(),
            _ => continue,
        };
        let lower = name.to_ascii_lowercase();
        // Skip checksum / signature sidecars.
        if lower.ends_with(".sig")
            || lower == "sha256sums"
            || lower.contains("sha256")
            || lower.ends_with(".json")
        {
            continue;
        }
        let mut score = 0usize;
        for (i, token) in prefer.iter().enumerate() {
            if lower.contains(token) {
                score += 100 - i;
            }
        }
        // Prefer non-portable on Windows when both match.
        if cfg!(target_os = "windows") && lower.contains("portable") {
            score = score.saturating_sub(30);
        }
        if score == 0 {
            continue;
        }
        match &best {
            None => best = Some((score, name, url)),
            Some((s, _, _)) if score > *s => best = Some((score, name, url)),
            _ => {}
        }
    }
    match best {
        Some((_, n, u)) => (Some(u), Some(n)),
        None => (None, None),
    }
}

/// Map GitHub `/releases/latest` JSON into [`AppUpdateCheck`].
pub fn parse_github_release(current_version: &str, v: &Value) -> Result<AppUpdateCheck, String> {
    let tag = v
        .get("tag_name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "release missing tag_name".to_string())?
        .trim();
    if tag.is_empty() {
        return Err("empty tag_name".into());
    }
    let html_url = v
        .get("html_url")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_RELEASES_PAGE)
        .to_string();
    let release_name = v
        .get("name")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let published_at = v
        .get("published_at")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let body = v
        .get("body")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let assets = v.get("assets").and_then(|a| a.as_array());
    let asset_names = assets
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (download_url, download_name) = pick_platform_asset(assets);

    let latest_version = tag.trim_start_matches(['v', 'V']).to_string();
    let update_available = is_remote_newer(current_version, tag);

    Ok(AppUpdateCheck {
        current_version: current_version.to_string(),
        latest_version,
        update_available,
        release_name,
        html_url,
        published_at,
        body,
        asset_names,
        download_url,
        download_name,
    })
}

/// Extract `v0.1.7` / `0.1.7` from a releases tag URL or path.
///
/// Accepts:
/// - `https://github.com/Po1nt9/omp-desktop/releases/tag/v0.1.7`
/// - `.../releases/tag/v0.1.7?foo=1`
/// - `/Po1nt9/omp-desktop/releases/tag/0.1.7`
pub fn extract_tag_from_release_url(url: &str) -> Option<String> {
    let base = url.split(['?', '#']).next().unwrap_or(url);
    // Find `/releases/tag/<tag>`
    let marker = "/releases/tag/";
    let idx = base.find(marker)?;
    let after = &base[idx + marker.len()..];
    let tag = after
        .split('/')
        .next()
        .unwrap_or(after)
        .trim()
        .trim_end_matches('/');
    if tag.is_empty() {
        return None;
    }
    // Basic sanity: must look like a version tag
    if parse_semver(tag).is_none() {
        return None;
    }
    Some(tag.to_string())
}

fn build_check_from_tag(current_version: &str, tag: &str, html_url: &str) -> AppUpdateCheck {
    let latest_version = tag.trim_start_matches(['v', 'V']).to_string();
    let update_available = is_remote_newer(current_version, tag);
    let release_name = format!("v{latest_version}");
    AppUpdateCheck {
        current_version: current_version.to_string(),
        latest_version,
        update_available,
        release_name: Some(release_name),
        html_url: html_url.to_string(),
        published_at: None,
        body: None,
        asset_names: vec![],
        download_url: None,
        download_name: None,
    }
}

pub(crate) fn is_allowed_update_url(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
}

pub(crate) fn format_http_error(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if status == 403 || status == 429 {
        if lower.contains("rate limit") {
            return format!(
                "GitHub API rate limit (HTTP {status}). Unauthenticated limit is 60/hour per IP — try again later, or open the release page."
            );
        }
        if !body.trim().is_empty() {
            // Prefer short message field when JSON
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                    return format!("GitHub releases returned HTTP {status}: {msg}");
                }
            }
            let snippet: String = body.chars().take(160).collect();
            return format!("GitHub releases returned HTTP {status}: {snippet}");
        }
    }
    format!("GitHub releases returned HTTP {status}")
}

pub(crate) fn http_client(user_agent: &str) -> Result<reqwest::Client, String> {
    crate::proxy::apply_to_reqwest(reqwest::Client::builder())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(user_agent)
        .build()
        .map_err(|e| e.to_string())
}

/// Primary path: GitHub REST releases/latest.
async fn fetch_via_api(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    let mut req = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    // Optional auth raises rate limit (5000/h). Never required for public repos.
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        let token = token.trim();
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("update check network: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format_http_error(status.as_u16(), &body));
    }

    res.json()
        .await
        .map_err(|e| format!("update check parse: {e}"))
}

fn resolve_location(base_host_hint: &str, loc: &str) -> String {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        loc.to_string()
    } else if loc.starts_with('/') {
        // Relative Location on github.com
        if base_host_hint.starts_with("https://") || base_host_hint.starts_with("http://") {
            // Prefer scheme+host from the request URL when available.
            if let Ok(u) = url::Url::parse(base_host_hint) {
                if let Some(host) = u.host_str() {
                    return format!("{}://{}{}", u.scheme(), host, loc);
                }
            }
        }
        format!("https://github.com{loc}")
    } else {
        loc.to_string()
    }
}

/// Fallback: follow HTML `/releases/latest` → `/releases/tag/vX.Y.Z` (no API quota).
async fn fetch_via_html_redirect(
    client: &reqwest::Client,
    latest_url: &str,
    current_version: &str,
) -> Result<AppUpdateCheck, String> {
    let ua = format!(
        "OMP-Desktop/{current_version} (desktop; check-update; +https://github.com/Po1nt9/omp-desktop)"
    );

    // 1) Prefer Location header without downloading the HTML body.
    let client_nr = crate::proxy::apply_to_reqwest(reqwest::Client::builder())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(&ua)
        .build()
        .map_err(|e| e.to_string())?;

    let res = client_nr
        .get(latest_url)
        .header("Accept", "text/html")
        .send()
        .await
        .map_err(|e| format!("update fallback network: {e}"))?;

    let status = res.status().as_u16();
    if let Some(loc) = res.headers().get(reqwest::header::LOCATION) {
        if let Ok(loc_s) = loc.to_str() {
            let absolute = resolve_location(latest_url, loc_s);
            if let Some(tag) = extract_tag_from_release_url(&absolute) {
                return Ok(build_check_from_tag(current_version, &tag, &absolute));
            }
        }
    }

    // 2) 200 body scan (proxies that rewrite redirects).
    if status == 200 {
        let body = res.text().await.unwrap_or_default();
        if let Some(idx) = body.find("/releases/tag/") {
            let slice: String = body[idx..].chars().take(80).collect();
            if let Some(tag) = extract_tag_from_release_url(&format!("https://github.com{slice}")) {
                let tag_path = if tag.starts_with('v') || tag.starts_with('V') {
                    tag.clone()
                } else {
                    format!("v{tag}")
                };
                let html = format!("https://github.com/Po1nt9/omp-desktop/releases/tag/{tag_path}");
                return Ok(build_check_from_tag(current_version, &tag, &html));
            }
        }
    }

    // 3) Follow redirects; parse final URL.
    let res2 = client
        .get(latest_url)
        .header("Accept", "text/html")
        .send()
        .await
        .map_err(|e| format!("update fallback follow: {e}"))?;
    let final_url = res2.url().as_str().to_string();
    if let Some(tag) = extract_tag_from_release_url(&final_url) {
        return Ok(build_check_from_tag(current_version, &tag, &final_url));
    }

    Err(format!(
        "update fallback: could not parse latest tag (HTTP {status}, url={final_url})"
    ))
}

/// Query GitHub for the latest release *on this build's channel* and compare.
///
/// Stable keeps `/releases/latest` + the HTML redirect fallback. Beta/nightly
/// list recent releases and pick the newest tag their channel owns — the
/// `/latest` endpoints are stable-only by GitHub semantics, and the HTML
/// redirect resolves to the newest stable tag (wrong channel), so non-stable
/// channels skip the HTML fallback and surface API errors directly (AC-10.9).
pub async fn check_app_update() -> Result<AppUpdateCheck, String> {
    let current = env!("CARGO_PKG_VERSION");
    let channel = crate::update_channel::UpdateChannel::from_version(current);
    let api_url = std::env::var("GROK_APP_RELEASES_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASES_API_URL.into());
    let html_url = std::env::var("GROK_APP_RELEASES_HTML_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASES_HTML_URL.into());

    if !is_allowed_update_url(&api_url) {
        return Err("update check URL must be https (or localhost for tests)".into());
    }
    if !is_allowed_update_url(&html_url) {
        return Err("update fallback URL must be https (or localhost for tests)".into());
    }

    let ua = format!(
        "OMP-Desktop/{current} (desktop; check-update; +https://github.com/Po1nt9/omp-desktop)"
    );
    let client = http_client(&ua)?;

    if channel == crate::update_channel::UpdateChannel::Stable {
        return match fetch_via_api(&client, &api_url).await {
            Ok(v) => parse_github_release(current, &v),
            Err(api_err) => {
                tracing::warn!(error = %api_err, "app update API failed; trying HTML redirect fallback");
                match fetch_via_html_redirect(&client, &html_url, current).await {
                    Ok(check) => Ok(check),
                    Err(fallback_err) => Err(format!("{api_err} | {fallback_err}")),
                }
            }
        };
    }

    let list_url = format!(
        "{}?per_page=30",
        api_url.strip_suffix("/latest").unwrap_or(&api_url)
    );
    let v = fetch_via_api(&client, &list_url).await?;
    let empty = Vec::new();
    let releases = v.as_array().unwrap_or(&empty);
    match select_release_for_channel(releases, channel) {
        Some(release) => parse_github_release(current, release),
        // No release published on this channel yet — report up-to-date,
        // pointing at the releases page (spec §5: never error-pop for this).
        None => Ok(AppUpdateCheck {
            current_version: current.to_string(),
            latest_version: current.to_string(),
            update_available: false,
            release_name: None,
            html_url: DEFAULT_RELEASES_PAGE.to_string(),
            published_at: None,
            body: None,
            asset_names: vec![],
            download_url: None,
            download_name: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_remote_newer_prerelease_aware() {
        // Channel-internal ordering (the old tuple parser called these equal).
        assert!(is_remote_newer(
            "0.3.1-nightly.20260730",
            "0.3.1-nightly.20260731"
        ));
        assert!(is_remote_newer("1.1.0-beta.1", "v1.1.0-beta.2"));
        // Cross-channel semver truth: stable outranks same-version prerelease.
        assert!(is_remote_newer("1.0.0-nightly.20260801", "1.0.0"));
        assert!(!is_remote_newer("1.0.0", "1.0.0-nightly.20260801"));
        // Same version on the same channel → no update.
        assert!(!is_remote_newer("0.3.1-nightly", "v0.3.1-nightly"));
    }

    #[test]
    fn select_release_for_channel_picks_newest_owned_tag() {
        let releases = json!([
            {"tag_name": "v1.0.0", "draft": false},
            {"tag_name": "v1.1.0-nightly.20260801", "draft": false},
            {"tag_name": "v1.1.0-nightly.20260730", "draft": false},
            {"tag_name": "v1.1.0-beta.1", "draft": false},
            {"tag_name": "v9.9.9-nightly.draft", "draft": true}
        ]);
        let arr = releases.as_array().unwrap();
        use crate::update_channel::UpdateChannel::*;
        let n = select_release_for_channel(arr, Nightly).unwrap();
        assert_eq!(
            n.get("tag_name").and_then(|t| t.as_str()),
            Some("v1.1.0-nightly.20260801")
        );
        let b = select_release_for_channel(arr, Beta).unwrap();
        assert_eq!(
            b.get("tag_name").and_then(|t| t.as_str()),
            Some("v1.1.0-beta.1")
        );
        let s = select_release_for_channel(arr, Stable).unwrap();
        assert_eq!(s.get("tag_name").and_then(|t| t.as_str()), Some("v1.0.0"));
    }

    #[test]
    fn select_release_for_channel_none_when_no_owned_tag() {
        let releases = json!([{"tag_name": "v1.0.0"}]);
        let arr = releases.as_array().unwrap();
        assert!(
            select_release_for_channel(arr, crate::update_channel::UpdateChannel::Nightly).is_none()
        );
        assert!(
            select_release_for_channel(&[], crate::update_channel::UpdateChannel::Stable).is_none()
        );
    }

    #[test]
    fn parse_semver_strips_v_and_prerelease() {
        assert_eq!(parse_semver("v0.1.5"), Some((0, 1, 5)));
        assert_eq!(parse_semver("0.1.5"), Some((0, 1, 5)));
        assert_eq!(parse_semver("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("2.0"), Some((2, 0, 0)));
        assert!(parse_semver("").is_none());
        assert!(parse_semver("nope").is_none());
    }

    #[test]
    fn is_remote_newer_orders() {
        assert!(!is_remote_newer("0.1.5", "v0.1.5"));
        assert!(!is_remote_newer("0.1.5", "0.1.4"));
        assert!(is_remote_newer("0.1.5", "v0.1.6"));
        assert!(is_remote_newer("0.1.5", "0.2.0"));
        assert!(is_remote_newer("0.9.9", "1.0.0"));
        assert!(!is_remote_newer("bad", "0.1.0"));
    }

    #[test]
    fn parse_github_release_update_and_same() {
        let sample = json!({
            "tag_name": "v0.2.0",
            "name": "OMP Desktop v0.2.0",
            "html_url": "https://github.com/Po1nt9/omp-desktop/releases/tag/v0.2.0",
            "published_at": "2026-07-24T00:00:00Z",
            "body": "### Added\n- hello",
            "assets": [
                {"name": "OMP-Desktop_0.2.0_aarch64.dmg"},
                {"name": "OMP-Desktop_0.2.0_x64-setup.exe"}
            ]
        });
        let up = parse_github_release("0.1.5", &sample).unwrap();
        assert!(up.update_available);
        assert_eq!(up.latest_version, "0.2.0");
        assert_eq!(up.current_version, "0.1.5");
        assert_eq!(up.asset_names.len(), 2);
        assert!(up.body.as_deref().unwrap().contains("hello"));
        // Platform pick is compile-time; at least one of name/url fields is set or both None.
        assert_eq!(up.download_url.is_some(), up.download_name.is_some());

        let same = parse_github_release("0.2.0", &sample).unwrap();
        assert!(!same.update_available);
    }

    #[test]
    fn extract_tag_from_release_url_ok() {
        assert_eq!(
            extract_tag_from_release_url(
                "https://github.com/Po1nt9/omp-desktop/releases/tag/v0.1.7"
            )
            .as_deref(),
            Some("v0.1.7")
        );
        assert_eq!(
            extract_tag_from_release_url(
                "https://github.com/Po1nt9/omp-desktop/releases/tag/0.2.0?foo=1#sec"
            )
            .as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            extract_tag_from_release_url("/Po1nt9/omp-desktop/releases/tag/v1.0.0").as_deref(),
            Some("v1.0.0")
        );
        assert!(
            extract_tag_from_release_url("https://github.com/Po1nt9/omp-desktop/releases")
                .is_none()
        );
        assert!(extract_tag_from_release_url("https://example.com/nope").is_none());
    }

    #[test]
    fn format_http_error_rate_limit() {
        let msg = format_http_error(
            403,
            r#"{"message":"API rate limit exceeded for 1.2.3.4.","documentation_url":"https://docs.github.com"}"#,
        );
        assert!(msg.contains("rate limit"), "{msg}");
        assert!(msg.contains("403"), "{msg}");
    }

    #[test]
    fn build_check_from_tag_compares() {
        let c = build_check_from_tag(
            "0.1.5",
            "v0.1.7",
            "https://github.com/Po1nt9/omp-desktop/releases/tag/v0.1.7",
        );
        assert!(c.update_available);
        assert_eq!(c.latest_version, "0.1.7");
        assert!(c.asset_names.is_empty());
    }
}
