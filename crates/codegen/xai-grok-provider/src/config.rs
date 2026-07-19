use std::fmt;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::error::ProviderError;

/// A single configuration diagnostic: a non-fatal warning or error with
/// a path that points to the exact TOML field.
#[derive(Debug, Clone)]
pub struct ConfigDiagnostic {
    pub provider_id: String,
    pub field_path: String,
    pub message: String,
}

impl ConfigDiagnostic {
    pub fn new(provider_id: impl Into<String>, field_path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            field_path: field_path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "provider `{}`.`{}`: {}",
            self.provider_id, self.field_path, self.message
        )
    }
}

/// Single parsed [provider.*] entry — the deserialization target.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct ParsedProviderEntry {
    pub enabled: Option<bool>,
    pub kind: Option<String>,
    pub profile: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub allow_insecure_http: Option<bool>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

/// Parse all `[provider.*]` sections from a TOML Value, collecting diagnostics
/// for parse errors instead of silently dropping them.
pub fn parse_provider_toml(
    toml: &toml::Value,
) -> (Vec<(String, ProviderConfig)>, Vec<ConfigDiagnostic>) {
    let Some(table) = toml.get("provider").and_then(|v| v.as_table()) else {
        return (vec![], vec![]);
    };

    let mut configs = Vec::new();
    let mut diagnostics = Vec::new();

    for (id, entry) in table.iter() {
        let entry_str = match toml::to_string(entry) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(ConfigDiagnostic::new(
                    id,
                    "[provider.{id}]",
                    format!("failed to serialize entry: {e}"),
                ));
                continue;
            }
        };
        match toml::from_str::<ParsedProviderEntry>(&entry_str) {
            Ok(parsed) => {
                configs.push((
                    id.clone(),
                    ProviderConfig {
                        id: Some(id.clone()),
                        enabled: parsed.enabled,
                        kind: parsed.kind,
                        profile: parsed.profile,
                        api_key: parsed.api_key,
                        env_key: parsed.env_key,
                        base_url: parsed.base_url,
                        allow_insecure_http: parsed.allow_insecure_http,
                        extra_headers: parsed.extra_headers,
                    },
                ));
            }
            Err(e) => {
                diagnostics.push(ConfigDiagnostic::new(
                    id,
                    "[provider.{id}]",
                    format!("parse error: {e}"),
                ));
            }
        }
    }

    (configs, diagnostics)
}

/// Merged configuration for a single provider.
/// Priority order (low→high): env var → TOML config → CLI override.
///
/// Custom Debug implementation redacts the api_key field.
#[derive(Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct ProviderConfig {
    pub id: Option<String>,
    pub enabled: Option<bool>,
    pub kind: Option<String>,
    pub profile: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub allow_insecure_http: Option<bool>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("id", &self.id)
            .field("enabled", &self.enabled)
            .field("kind", &self.kind)
            .field("profile", &self.profile)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("env_key", &self.env_key)
            .field("base_url", &self.base_url)
            .field("allow_insecure_http", &self.allow_insecure_http)
            .field("extra_headers", &self.extra_headers)
            .finish()
    }
}

impl ProviderConfig {
    /// Validate header names in extra_headers.
    pub fn validate_headers(&self) -> Result<(), ProviderError> {
        if let Some(ref headers) = self.extra_headers {
            for (name, _) in headers {
                if name.is_empty() || name.bytes().any(|b| b <= 32 || b > 126 || b == 58) {
                    return Err(ProviderError::InvalidHeader(format!(
                        "invalid header name: {name:?}"
                    )));
                }
            }
        }
        Ok(())
    }
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
            enabled: other.enabled.or(self.enabled),
            kind: other.kind.or(self.kind),
            profile: other.profile.or(self.profile),
            api_key: other.api_key.or(self.api_key),
            env_key: other.env_key.or(self.env_key),
            base_url: other.base_url.or(self.base_url),
            allow_insecure_http: other.allow_insecure_http.or(self.allow_insecure_http),
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
    fn provider_config_debug_redacts_secret() {
        let c = ProviderConfig {
            id: Some("test".into()),
            api_key: Some("test-secret-not-real".into()),
            ..Default::default()
        };
        let debug = format!("{c:?}");
        assert!(
            !debug.contains("test-secret-not-real"),
            "Debug must not contain the secret key"
        );
        assert!(
            debug.contains("[REDACTED]"),
            "Debug must show [REDACTED] for api_key"
        );
    }

    #[test]
    fn provider_config_validate_headers() {
        let valid = ProviderConfig {
            extra_headers: Some([("x-custom".into(), "value".into())].into()),
            ..Default::default()
        };
        assert!(valid.validate_headers().is_ok());

        let invalid = ProviderConfig {
            extra_headers: Some([("".into(), "value".into())].into()),
            ..Default::default()
        };
        assert!(invalid.validate_headers().is_err());
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
        let (configs, diags) = parse_provider_toml(&toml);
        assert!(configs.is_empty());
        assert!(diags.is_empty());
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
        let (configs, diags) = parse_provider_toml(&toml);
        assert!(diags.is_empty());
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].0, "openai");
        assert_eq!(configs[0].1.api_key.as_deref(), Some("sk-test"));
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
        let (configs, diags) = parse_provider_toml(&toml);
        assert!(diags.is_empty());
        assert_eq!(configs.len(), 2);
    }

    #[test]
    fn parse_provider_toml_enabled_kind_profile() {
        let toml: toml::Value = toml::from_str(
            r#"
[provider.deepseek]
enabled = true
kind = "openai_compatible"
profile = "deepseek"
api_key = "sk-test"
"#,
        )
        .unwrap();
        let (configs, diags) = parse_provider_toml(&toml);
        assert!(diags.is_empty());
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].1.enabled, Some(true));
        assert_eq!(
            configs[0].1.kind.as_deref(),
            Some("openai_compatible")
        );
        assert_eq!(configs[0].1.profile.as_deref(), Some("deepseek"));
    }

    #[test]
    fn config_diagnostic_display_shows_field_path() {
        let d = ConfigDiagnostic::new("test", "api_key", "invalid value");
        let s = d.to_string();
        assert!(s.contains("test"));
        assert!(s.contains("api_key"));
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

}
