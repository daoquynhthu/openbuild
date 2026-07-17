use indexmap::IndexMap;
use serde::Deserialize;

/// Merged configuration for a single provider.
/// Priority order (low→high): env var → TOML config → CLI override.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct ProviderConfig {
    pub id: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

impl ProviderConfig {
    /// Create a ProviderConfig from parts.
    pub fn new(id: Option<String>, api_key: Option<String>, base_url: Option<String>) -> Self {
        Self {
            id,
            api_key,
            base_url,
            ..Default::default()
        }
    }

    /// Merge `other` on top of `self`. Non-None fields in `other` override.
    pub fn merge(self, other: ProviderConfig) -> ProviderConfig {
        ProviderConfig {
            id: other.id.or(self.id),
            api_key: other.api_key.or(self.api_key),
            env_key: other.env_key.or(self.env_key),
            base_url: other.base_url.or(self.base_url),
            extra_headers: match (self.extra_headers, other.extra_headers) {
                (Some(mut base), Some(other)) => {
                    base.extend(other);
                    Some(base)
                }
                (None, other) => other,
                (base, None) => base,
            },
        }
    }
}

/// A single `[provider.<id>]` entry from config.toml.
/// This is a tagless TOML table that serde maps directly.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct ProviderTomlEntry {
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

/// Parse all `[provider.*]` sections from a TOML Value.
/// Returns a list of (provider_id, ProviderConfig) pairs.
pub fn parse_provider_toml(toml: &toml::Value) -> Vec<(String, ProviderConfig)> {
    let Some(table) = toml.get("provider").and_then(|v| v.as_table()) else {
        return vec![];
    };
    table
        .iter()
        .filter_map(|(id, entry)| {
            let entry_str = toml::to_string(entry).ok()?;
            let parsed: ProviderTomlEntry = toml::from_str(&entry_str).ok()?;
            Some((
                id.clone(),
                ProviderConfig {
                    id: Some(id.clone()),
                    api_key: parsed.api_key,
                    env_key: parsed.env_key,
                    base_url: parsed.base_url,
                    extra_headers: parsed.extra_headers,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_default_is_empty() {
        let c = ProviderConfig::default();
        assert!(c.api_key.is_none());
        assert!(c.base_url.is_none());
        assert!(c.id.is_none());
    }

    #[test]
    fn provider_config_deserialize_json_with_id() {
        let json = r#"{"id":"my-provider","api_key":"sk-test"}"#;
        let c: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.id.unwrap(), "my-provider");
        assert_eq!(c.api_key.unwrap(), "sk-test");
    }

    #[test]
    fn parse_provider_toml_empty() {
        let toml: toml::Value = toml::from_str("").unwrap();
        let result = parse_provider_toml(&toml);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_provider_toml_single_entry() {
        let toml: toml::Value = toml::from_str(
            r#"
[provider.openai]
api_key = "sk-test"
base_url = "https://api.openai.com/v1"
"#,
        )
        .unwrap();
        let result = parse_provider_toml(&toml);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "openai");
        assert_eq!(result[0].1.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn provider_config_merge_self_overrides_none() {
        let base = ProviderConfig {
            id: Some("p".into()),
            api_key: None,
            base_url: Some("https://default.url".into()),
            ..Default::default()
        };
        let merged = base.merge(ProviderConfig::default());
        assert_eq!(merged.id.as_deref(), Some("p"));
        assert_eq!(merged.base_url.as_deref(), Some("https://default.url"));
        assert!(merged.api_key.is_none());
    }

    #[test]
    fn provider_config_merge_other_overrides() {
        let base = ProviderConfig {
            id: Some("p".into()),
            api_key: Some("base-key".into()),
            base_url: Some("https://base.url".into()),
            ..Default::default()
        };
        let other = ProviderConfig {
            id: Some("p".into()),
            api_key: Some("other-key".into()),
            ..Default::default()
        };
        let merged = base.merge(other);
        assert_eq!(merged.api_key.as_deref(), Some("other-key"));
        assert_eq!(merged.base_url.as_deref(), Some("https://base.url"));
    }

    #[test]
    fn parse_provider_toml_multiple() {
        let toml: toml::Value = toml::from_str(
            r#"
[provider.openai]
api_key = "sk-1"

[provider.anthropic]
api_key = "sk-2"
"#,
        )
        .unwrap();
        let result = parse_provider_toml(&toml);
        assert_eq!(result.len(), 2);
    }
}
