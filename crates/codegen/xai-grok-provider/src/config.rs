use std::fmt;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::error::ProviderError;
use crate::types::ProviderId;

/// Severity level for a configuration diagnostic (R3-PARSE-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

/// A single configuration diagnostic with severity, provider ID, field path,
/// category, and safe value summary (R3-PARSE-05).
///
/// Never contains API keys or Authorization header values.
#[derive(Debug, Clone)]
pub struct ConfigDiagnostic {
    pub severity: DiagnosticSeverity,
    pub provider_id: String,
    pub field_path: String,
    pub category: String,
    pub message: String,
}

impl ConfigDiagnostic {
    pub fn new(
        provider_id: impl Into<String>,
        field_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            provider_id: provider_id.into(),
            field_path: field_path.into(),
            category: "validation".into(),
            message: message.into(),
        }
    }

    pub fn new_error(
        provider_id: impl Into<String>,
        field_path: impl Into<String>,
        category: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            provider_id: provider_id.into(),
            field_path: field_path.into(),
            category: category.into(),
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "provider `{}`.`{}` [{}]: {}",
            self.provider_id, self.field_path, self.category, self.message
        )
    }
}

/// The result of parsing all `[provider.*]` sections.
/// `Ok` carries the parsed configs; `Err` carries cumulative diagnostics
/// for entries that could not be parsed at all.
pub type ParseProviderTomlResult = Result<ParsedProviderConfig, Vec<ConfigDiagnostic>>;

/// Wrapper over all successfully-parsed provider configurations.
#[derive(Debug, Clone)]
pub struct ParsedProviderConfig {
    pub entries: IndexMap<ProviderId, ProviderConfig>,
}

/// Serde-facing input for a single provider's configuration.
/// Values exist only during resolution; after resolution they are consumed
/// into `ProviderRuntimeConfig` (crate::resolution::ProviderRuntimeConfig).
/// `api_key` is immediately wrapped in `SecretValue` (crate::auth::SecretValue)
/// and never exposed as a plain `String` beyond this point.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct ProviderConfigInput {
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub protocol: Option<String>,
    pub model_list_path: Option<String>,
    pub model_list_format: Option<String>,
    pub allow_insecure_http: Option<bool>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

/// Well-known protocol identifiers.
fn is_known_protocol(protocol: &str) -> bool {
    matches!(protocol, "chat_completions" | "responses" | "messages")
}

impl ProviderConfigInput {
    /// Validate config fields and return diagnostics for any issues.
    /// Error-level diagnostics prevent the provider from being used (R3-PARSE-02).
    pub fn validate(&self, provider_id: &str) -> Vec<ConfigDiagnostic> {
        let mut diags = Vec::new();

        // Insecure HTTP is an error unless explicitly opted in.
        if self.allow_insecure_http == Some(true) {
            diags.push(ConfigDiagnostic::new(
                provider_id,
                "allow_insecure_http",
                "insecure HTTP is enabled — this is a security risk",
            ));
        }

        // Protocol validation: unknown protocols produce an error.
        if let Some(proto) = self
            .protocol
            .as_ref()
            .filter(|p| !is_known_protocol(p.as_str()))
        {
            diags.push(ConfigDiagnostic::new_error(
                provider_id,
                "protocol",
                "unknown_value",
                format!("unknown protocol `{proto}` — must be one of: chat_completions, responses, messages"),
            ));
        }

        diags
    }

    /// Consume this input and produce a `ProviderRuntimeConfig`.
    /// `api_key` is wrapped in `SecretValue` and never returned as `String`.
    pub fn into_runtime_config(self) -> crate::resolution::ProviderRuntimeConfig {
        crate::resolution::ProviderRuntimeConfig::from(ProviderConfig {
            id: None,
            enabled: None,
            kind: None,
            profile: None,
            api_key: self.api_key,
            env_key: self.env_key,
            base_url: self.base_url,
            protocol: self.protocol,
            model_list_path: self.model_list_path,
            model_list_format: self.model_list_format,
            allow_insecure_http: self.allow_insecure_http,
            extra_headers: self.extra_headers,
        })
    }
}

/// Single parsed [provider.*] entry — the deserialization target.
/// Unknown fields are rejected at parse time (R3-PARSE-01).
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ParsedProviderEntry {
    pub enabled: Option<bool>,
    pub kind: Option<String>,
    pub profile: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub protocol: Option<String>,
    pub model_list_path: Option<String>,
    pub model_list_format: Option<String>,
    pub allow_insecure_http: Option<bool>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

/// Parse all `[provider.*]` sections from a TOML Value, collecting diagnostics
/// for parse errors instead of silently dropping them.
pub fn parse_provider_toml(toml: &toml::Value) -> ParseProviderTomlResult {
    let Some(table) = toml.get("provider").and_then(|v| v.as_table()) else {
        return Ok(ParsedProviderConfig {
            entries: IndexMap::new(),
        });
    };

    let mut entries = IndexMap::new();
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
                let pid = ProviderId::new(id);
                if entries.contains_key(&pid) {
                    diagnostics.push(ConfigDiagnostic::new(
                        id,
                        "[provider.{id}]",
                        format!("duplicate provider `{id}` — keeping first entry"),
                    ));
                    continue;
                }
                entries.insert(
                    pid,
                    ProviderConfig {
                        id: Some(id.clone()),
                        enabled: parsed.enabled,
                        kind: parsed.kind,
                        profile: parsed.profile,
                        api_key: parsed.api_key,
                        env_key: parsed.env_key,
                        base_url: parsed.base_url,
                        protocol: parsed.protocol,
                        model_list_path: parsed.model_list_path,
                        model_list_format: parsed.model_list_format,
                        allow_insecure_http: parsed.allow_insecure_http,
                        extra_headers: parsed.extra_headers,
                    },
                );
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

    if !diagnostics.is_empty() {
        Err(diagnostics)
    } else {
        Ok(ParsedProviderConfig { entries })
    }
}

/// Low-level helper: parse TOML entries into a plain ordered Vec, returning
/// diagnostics separately.  Used by legacy callers that need the flat list.
/// New code should prefer [`parse_provider_toml`].
pub fn parse_provider_toml_legacy(
    toml: &toml::Value,
) -> (Vec<(String, ProviderConfig)>, Vec<ConfigDiagnostic>) {
    match parse_provider_toml(toml) {
        Ok(parsed) => {
            let vec: Vec<(String, ProviderConfig)> = parsed
                .entries
                .into_iter()
                .map(|(id, cfg)| (id.0, cfg))
                .collect();
            (vec, vec![])
        }
        Err(diags) => (vec![], diags),
    }
}

/// Merged configuration for a single provider.
/// Priority order (low→high): env var → TOML config → CLI override.
///
/// Custom Debug implementation redacts the api_key field.
#[derive(Clone, Default, Deserialize)]
#[non_exhaustive]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub id: Option<String>,
    pub enabled: Option<bool>,
    pub kind: Option<String>,
    pub profile: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub protocol: Option<String>,
    pub model_list_path: Option<String>,
    pub model_list_format: Option<String>,
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
            .field("protocol", &self.protocol)
            .field("model_list_path", &self.model_list_path)
            .field("model_list_format", &self.model_list_format)
            .field("allow_insecure_http", &self.allow_insecure_http)
            .field("extra_headers", &self.extra_headers)
            .finish()
    }
}

impl ProviderConfig {
    /// Validate all config fields and return diagnostics for any issues.
    /// Error-level diagnostics prevent the provider from being committed (R3-PARSE-02).
    pub fn validate(&self) -> Vec<ConfigDiagnostic> {
        let mut diags = Vec::new();
        let provider_id = self.id.as_deref().unwrap_or("?");

        // Malformed base URL (PARSE-02: must be error)
        if let Some(ref url_str) = self.base_url
            && let Err(e) = url::Url::parse(url_str)
        {
            diags.push(ConfigDiagnostic::new_error(
                provider_id,
                "base_url",
                "invalid_value",
                format!("malformed base URL `{url_str}` — {e}"),
            ));
        }

        // Insecure HTTP is an error (PARSE-02: non-functional only)
        if self.allow_insecure_http == Some(true) {
            diags.push(ConfigDiagnostic::new_error(
                provider_id,
                "allow_insecure_http",
                "security_policy",
                "insecure HTTP is enabled — this is a security risk",
            ));
        }

        // Unknown protocol
        if let Some(proto) = self
            .protocol
            .as_ref()
            .filter(|p| !is_known_protocol(p.as_str()))
        {
            diags.push(ConfigDiagnostic::new_error(
                provider_id,
                "protocol",
                "unknown_value",
                format!(
                    "unknown protocol `{proto}` — must be one of: chat_completions, responses, messages"
                ),
            ));
        }

        // Unknown model_list_format
        if let Some(fmt) = self.model_list_format.as_deref()
            && !matches!(fmt, "openai_compatible" | "ollama_tags" | "ollama")
        {
            diags.push(ConfigDiagnostic::new_error(
                    provider_id,
                    "model_list_format",
                    "unknown_value",
                    format!(
                        "unknown model_list_format `{fmt}` — must be one of: openai_compatible, ollama_tags"
                    ),
                ));
        }

        // Unknown kind
        if let Some(kind) = self.kind.as_deref() {
            let is_builtin_id = matches!(
                provider_id,
                "xai" | "openai" | "anthropic" | "opencode" | "ollama"
            );
            if kind != "openai_compatible" && !is_builtin_id {
                diags.push(ConfigDiagnostic::new_error(
                    provider_id,
                    "kind",
                    "unsupported_value",
                    format!(
                        "unsupported provider kind `{kind}` — must be \"openai_compatible\" or omitted for built-in providers"
                    ),
                ));
            }
        }

        // Missing kind for non-builtin provider (plan: requires explicit `kind` or `profile`)
        if self.kind.is_none() {
            let is_builtin_id = matches!(
                provider_id,
                "xai" | "openai" | "anthropic" | "opencode" | "ollama"
            );
            if !is_builtin_id && self.profile.is_none() {
                diags.push(ConfigDiagnostic::new_error(
                    provider_id,
                    "kind",
                    "missing_field",
                    "custom provider must specify `kind`, `profile`, or use a built-in provider ID",
                ));
            }
        }

        // Invalid header names (PARSE-02: must be error diagnostic)
        if let Some(ref headers) = self.extra_headers {
            for (name, _) in headers {
                if name.is_empty() || name.bytes().any(|b| b <= 32 || b > 126 || b == 58) {
                    diags.push(ConfigDiagnostic::new_error(
                        provider_id,
                        "extra_headers",
                        "invalid_value",
                        format!("invalid header name: {name:?}"),
                    ));
                }
            }
        }

        diags
    }

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
            protocol: other.protocol.or(self.protocol),
            model_list_path: other.model_list_path.or(self.model_list_path),
            model_list_format: other.model_list_format.or(self.model_list_format),
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
    fn validate_rejects_empty_header_name() {
        let cfg = ProviderConfig {
            id: Some("test".into()),
            extra_headers: Some([("".into(), "value".into())].into()),
            kind: Some("openai_compatible".into()),
            ..Default::default()
        };
        let diags = cfg.validate();
        assert!(
            diags
                .iter()
                .any(|d| d.is_error() && d.to_string().contains("header")),
            "validate() must produce error for empty header name: {diags:?}"
        );
    }

    #[test]
    fn validate_rejects_colon_in_header_name() {
        let cfg = ProviderConfig {
            id: Some("test".into()),
            extra_headers: Some([("Invalid:Name".into(), "value".into())].into()),
            kind: Some("openai_compatible".into()),
            ..Default::default()
        };
        let diags = cfg.validate();
        assert!(
            diags
                .iter()
                .any(|d| d.is_error() && d.to_string().contains("header")),
            "validate() must produce error for colon in header name: {diags:?}"
        );
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
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.entries.is_empty());
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
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let (pid, cfg) = parsed.entries.into_iter().next().unwrap();
        assert_eq!(pid.0, "openai");
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
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
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.entries.len(), 2);
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
        let result = parse_provider_toml(&toml);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let cfg = parsed.entries.into_values().next().unwrap();
        assert_eq!(cfg.enabled, Some(true));
        assert_eq!(cfg.kind.as_deref(), Some("openai_compatible"));
        assert_eq!(cfg.profile.as_deref(), Some("deepseek"));
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

    #[test]
    fn provider_config_input_to_runtime_redacts_secret() {
        let input = ProviderConfigInput {
            api_key: Some("canary-secret-not-in-output".into()),
            base_url: Some("https://api.example.com".into()),
            ..Default::default()
        };
        let rt = input.into_runtime_config();
        let debug = format!("{rt:?}");
        assert!(
            !debug.contains("canary-secret-not-in-output"),
            "runtime config Debug must not leak secret"
        );
        assert!(
            debug.contains("[REDACTED]"),
            "runtime config Debug must show [REDACTED] for inline_api_key"
        );
    }

    #[test]
    fn provider_config_input_empty_api_key_is_none() {
        let input = ProviderConfigInput {
            api_key: None,
            ..Default::default()
        };
        let rt = input.into_runtime_config();
        assert!(rt.inline_api_key.is_none());
    }

    #[test]
    fn allow_insecure_http_defaults_to_false() {
        let input = ProviderConfigInput {
            ..Default::default()
        };
        let rt = input.into_runtime_config();
        assert!(!rt.public.allow_insecure_http);
    }

    #[test]
    fn allow_insecure_http_true_produces_diagnostic() {
        let input = ProviderConfigInput {
            allow_insecure_http: Some(true),
            ..Default::default()
        };
        let diags = input.validate("test");
        assert!(!diags.is_empty());
        assert!(diags[0].to_string().contains("insecure HTTP"));
    }

    #[test]
    fn unknown_protocol_produces_diagnostic() {
        let input = ProviderConfigInput {
            protocol: Some("unknown_protocol".into()),
            ..Default::default()
        };
        let diags = input.validate("test");
        assert!(!diags.is_empty());
        assert!(diags[0].to_string().contains("unknown protocol"));
    }

    #[test]
    fn known_protocol_no_diagnostic() {
        for proto in &["chat_completions", "responses", "messages"] {
            let input = ProviderConfigInput {
                protocol: Some((*proto).into()),
                ..Default::default()
            };
            let diags = input.validate("test");
            assert!(diags.is_empty(), "expected no diagnostic for {proto}");
        }
    }

    #[test]
    fn provider_config_input_model_list_format_maps_correctly() {
        let input = ProviderConfigInput {
            model_list_format: Some("ollama_tags".into()),
            ..Default::default()
        };
        let rt = input.into_runtime_config();
        assert_eq!(
            rt.public.model_list_format,
            Some(crate::types::ModelListFormat::OllamaTags)
        );
    }
}
