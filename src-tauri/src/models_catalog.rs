//! Live **model** catalog from runtime cache only.
//!
//! Providers / relays are **channels** managed on the Providers settings page —
//! they must never appear as selectable model chips.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEffort {
    pub id: String,
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModel {
    pub id: String,
    pub label: String,
    /// Always "official" for catalog entries (providers are not models).
    pub source: String,
    #[serde(default)]
    pub is_default: bool,
    /// Per-model reasoning efforts from CLI `info.reasoning_efforts` (may be empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<ReasoningEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelsResult {
    pub models: Vec<AvailableModel>,
    pub default_model_id: String,
    pub origin: Option<String>,
    pub fetched_at: Option<String>,
}

struct ParsedCacheModel {
    label: String,
    reasoning_efforts: Vec<ReasoningEffort>,
}

fn agent_home_cache() -> PathBuf {
    crate::paths::agent_home_dir().join("models_cache.json")
}

/// Parse `/info/reasoning_efforts` from a models_cache entry body.
fn parse_reasoning_efforts(body: &serde_json::Value) -> Vec<ReasoningEffort> {
    let Some(arr) = body
        .pointer("/info/reasoning_efforts")
        .and_then(|x| x.as_array())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.trim();
            if id.is_empty() {
                return None;
            }
            let value = item
                .get("value")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .to_string();
            let label = item
                .get("label")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .to_string();
            let description = item
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            // CLI cache uses `"default": true`; host API exposes `isDefault`.
            let is_default = item
                .get("default")
                .and_then(|x| x.as_bool())
                .or_else(|| item.get("isDefault").and_then(|x| x.as_bool()))
                .or_else(|| item.get("is_default").and_then(|x| x.as_bool()))
                .unwrap_or(false);
            Some(ReasoningEffort {
                id: id.to_string(),
                value,
                label,
                description,
                is_default,
            })
        })
        .collect()
}

fn read_models_cache(
    path: &PathBuf,
) -> Option<(
    BTreeMap<String, ParsedCacheModel>,
    Option<String>,
    Option<String>,
)> {
    let raw = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let models_obj = v.get("models")?.as_object()?;
    let mut map = BTreeMap::new();
    for (id, body) in models_obj {
        if id.trim().is_empty() {
            continue;
        }
        let hidden = body
            .pointer("/info/hidden")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        if hidden {
            continue;
        }
        // Skip entries that look like provider routes (have a custom base_url override
        // without being the official chat-proxy catalog shape). Official cache entries
        // expose info.model / info.name from cli-chat-proxy.
        let label = body
            .pointer("/info/name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();
        let reasoning_efforts = parse_reasoning_efforts(body);
        map.insert(
            id.clone(),
            ParsedCacheModel {
                label,
                reasoning_efforts,
            },
        );
    }
    let origin = v
        .get("origin")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let fetched_at = v
        .get("fetched_at")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    Some((map, origin, fetched_at))
}

/// Models the user can select in the composer.
///
/// Returns an empty catalog until a runtime integration supplies live models.
/// No fallback model is invented.
pub fn list_available_models() -> AvailableModelsResult {
    let settings = store::load_settings();

    let mut by_id: BTreeMap<String, AvailableModel> = BTreeMap::new();
    let mut origin = None;
    let mut fetched_at = None;

    // Read from the App-owned agent-home cache only.
    let cache = agent_home_cache();
    if let Some((map, o, f)) = read_models_cache(&cache) {
        origin = o;
        fetched_at = f;
        for (id, parsed) in map {
            by_id.entry(id.clone()).or_insert(AvailableModel {
                id,
                label: parsed.label,
                source: "official".into(),
                is_default: false,
                reasoning_efforts: parsed.reasoning_efforts,
            });
        }
    }

    // No fallback model — empty catalog is valid until runtime integration.
    let preferred = settings
        .model_id
        .clone()
        .filter(|s| by_id.contains_key(s))
        .or_else(|| by_id.keys().next().cloned());

    let mut models: Vec<AvailableModel> = by_id.into_values().collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    for m in &mut models {
        m.is_default = Some(&m.id.clone()) == preferred.as_ref();
    }

    AvailableModelsResult {
        models,
        default_model_id: preferred.unwrap_or_default(),
        origin,
        fetched_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_cache_parses_official_entry() {
        let dir = std::env::temp_dir().join(format!(
            "grok-app-models-test-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "fetched_at": "2026-07-23T00:00:00Z",
              "origin": "https://example.invalid/v1/models",
              "models": {
                "sample-model": {
                  "info": { "id": "sample-model", "name": "Sample Model", "hidden": false }
                }
              }
            }"#,
        )
        .unwrap();
        let (map, origin, _) = read_models_cache(&path).expect("cache");
        assert_eq!(
            map.get("sample-model").map(|m| m.label.as_str()),
            Some("Sample Model")
        );
        assert!(map
            .get("sample-model")
            .map(|m| m.reasoning_efforts.is_empty())
            .unwrap_or(false));
        assert!(origin.unwrap().contains("example.invalid"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_reasoning_efforts_from_cache_pointer() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "info": {
                "reasoning_efforts": [
                  {
                    "id": "high",
                    "value": "high",
                    "label": "High Effort",
                    "description": "Highest quality",
                    "default": true
                  },
                  {
                    "id": "medium",
                    "value": "medium",
                    "label": "Medium Effort",
                    "description": "Balanced",
                    "default": false
                  },
                  {
                    "id": "low",
                    "value": "low",
                    "label": "Low Effort",
                    "description": "Quick",
                    "default": false
                  }
                ]
              }
            }"#,
        )
        .unwrap();
        let efforts = parse_reasoning_efforts(&body);
        assert_eq!(efforts.len(), 3);
        assert_eq!(efforts[0].id, "high");
        assert_eq!(efforts[0].value, "high");
        assert_eq!(efforts[0].label, "High Effort");
        assert_eq!(efforts[0].description, "Highest quality");
        assert!(efforts[0].is_default);
        assert!(!efforts[1].is_default);
        assert_eq!(efforts[2].id, "low");
    }

    #[test]
    fn parse_reasoning_efforts_skips_empty_id() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "info": {
                "reasoning_efforts": [
                  { "id": "", "value": "x", "label": "X" },
                  { "id": "medium", "label": "Med" }
                ]
              }
            }"#,
        )
        .unwrap();
        let efforts = parse_reasoning_efforts(&body);
        assert_eq!(efforts.len(), 1);
        assert_eq!(efforts[0].id, "medium");
        assert_eq!(efforts[0].value, "medium");
        assert_eq!(efforts[0].label, "Med");
    }

    #[test]
    fn read_cache_includes_reasoning_efforts() {
        let dir = std::env::temp_dir().join(format!(
            "grok-app-models-efforts-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "fetched_at": "2026-07-25T00:00:00Z",
              "origin": "https://example.invalid/v1/models",
              "models": {
                "sample-model": {
                  "info": {
                    "id": "sample-model",
                    "name": "Sample Model",
                    "hidden": false,
                    "reasoning_efforts": [
                      {
                        "id": "high",
                        "value": "high",
                        "label": "High Effort",
                        "description": "Deep",
                        "default": true
                      }
                    ]
                  }
                }
              }
            }"#,
        )
        .unwrap();
        let (map, _, _) = read_models_cache(&path).expect("cache");
        let m = map.get("sample-model").expect("model");
        assert_eq!(m.reasoning_efforts.len(), 1);
        assert_eq!(m.reasoning_efforts[0].id, "high");
        assert!(m.reasoning_efforts[0].is_default);
        let _ = fs::remove_dir_all(&dir);
    }
}
