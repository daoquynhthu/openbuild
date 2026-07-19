use indexmap::IndexMap;

use crate::auth::SecretValue;
use crate::config::{ConfigDiagnostic, ProviderConfig};
use crate::types::{ProviderId, ModelListFormat};

/// Result of resolving configuration precedence into a provider set.
#[derive(Clone, Debug)]
pub struct ResolvedProviderSet {
    pub providers: IndexMap<ProviderId, ResolvedProviderSpec>,
}

/// A single fully-resolved provider specification, ready for registry preparation.
#[derive(Clone, Debug)]
pub struct ResolvedProviderSpec {
    pub id: ProviderId,
    pub implementation: ProviderImplementation,
    pub config: ProviderRuntimeConfig,
}

/// How a provider is implemented: built-in definition or generic compatible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderImplementation {
    Builtin { definition_id: ProviderId },
    OpenAiCompatible { profile: Option<String> },
}

/// Runtime configuration with secret values wrapped.
#[derive(Clone, Debug)]
pub struct ProviderRuntimeConfig {
    pub public: ProviderPublicConfig,
    pub inline_api_key: Option<SecretValue>,
}

/// Public (non-secret) configuration for diagnostics and display.
#[derive(Clone, Debug)]
pub struct ProviderPublicConfig {
    pub base_url: Option<String>,
    pub protocol: Option<String>,
    pub model_list_path: Option<String>,
    pub model_list_format: Option<ModelListFormat>,
    pub extra_headers: IndexMap<String, String>,
}

/// Known built-in provider IDs.
fn is_builtin_id(id: &str) -> bool {
    matches!(id, "xai" | "openai" | "anthropic" | "opencode" | "ollama")
}

/// Resolve a single provider config into a spec, using built-in defaults
/// for known provider IDs and the `kind` field for custom providers.
fn resolve_one(
    id: String,
    config: ProviderConfig,
) -> (ProviderId, ResolvedProviderSpec) {
    let pid = ProviderId::new(&id);
    let implementation = resolve_implementation(&id, &config);

    let public = ProviderPublicConfig {
        base_url: config.base_url,
        protocol: config.protocol,
        model_list_path: config.model_list_path,
        model_list_format: None,
        extra_headers: config.extra_headers.unwrap_or_default(),
    };
    let inline_api_key = config.api_key.map(SecretValue::new);

    let spec = ResolvedProviderSpec {
        id: pid.clone(),
        implementation,
        config: ProviderRuntimeConfig { public, inline_api_key },
    };
    (pid, spec)
}

fn resolve_implementation(id: &str, config: &ProviderConfig) -> ProviderImplementation {
    match config.kind.as_deref() {
        Some("openai_compatible") => ProviderImplementation::OpenAiCompatible {
            profile: config.profile.clone(),
        },
        Some(other) => {
            tracing::warn!("unknown provider kind `{other}` for `{id}`, falling back to builtin");
            ProviderImplementation::Builtin {
                definition_id: ProviderId::new(id),
            }
        }
        None if is_builtin_id(id) => ProviderImplementation::Builtin {
            definition_id: ProviderId::new(id),
        },
        None => {
            tracing::warn!("no `kind` specified for provider `{id}`, treating as openai-compatible");
            ProviderImplementation::OpenAiCompatible {
                profile: config.profile.clone(),
            }
        }
    }
}

/// Resolve a collection of parsed provider configs (from TOML/env/CLI) into
/// a `ResolvedProviderSet`, collecting diagnostics along the way.
pub fn resolve_provider_set(
    configs: Vec<(String, ProviderConfig)>,
) -> (ResolvedProviderSet, Vec<ConfigDiagnostic>) {
    let mut providers = IndexMap::new();
    let mut diagnostics = Vec::new();

    for (id, cfg) in configs {
        if cfg.enabled == Some(false) {
            continue;
        }
        let (pid, spec) = resolve_one(id, cfg);
        if providers.contains_key(&pid) {
            diagnostics.push(ConfigDiagnostic::new(
                pid.0.clone(),
                "provider",
                format!("duplicate provider `{}` — keeping first entry", pid.0),
            ));
            continue;
        }
        providers.insert(pid, spec);
    }

    (
        ResolvedProviderSet { providers },
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_xai_resolves_correctly() {
        let configs = vec![(
            "xai".into(),
            ProviderConfig {
                id: Some("xai".into()),
                ..Default::default()
            },
        )];
        let (set, diags) = resolve_provider_set(configs);
        assert!(diags.is_empty());
        assert_eq!(set.providers.len(), 1);
        let spec = set.providers.get(&ProviderId::new("xai")).unwrap();
        assert_eq!(
            spec.implementation,
            ProviderImplementation::Builtin {
                definition_id: ProviderId::new("xai")
            }
        );
    }

    #[test]
    fn openai_compatible_kind_resolves() {
        let configs = vec![(
            "deepseek".into(),
            ProviderConfig {
                id: Some("deepseek".into()),
                kind: Some("openai_compatible".into()),
                profile: Some("deepseek".into()),
                base_url: Some("https://api.deepseek.com".into()),
                ..Default::default()
            },
        )];
        let (set, diags) = resolve_provider_set(configs);
        assert!(diags.is_empty());
        let spec = set.providers.get(&ProviderId::new("deepseek")).unwrap();
        assert_eq!(
            spec.implementation,
            ProviderImplementation::OpenAiCompatible {
                profile: Some("deepseek".into())
            }
        );
    }

    #[test]
    fn disabled_provider_is_skipped() {
        let configs = vec![(
            "test".into(),
            ProviderConfig {
                id: Some("test".into()),
                enabled: Some(false),
                ..Default::default()
            },
        )];
        let (set, diags) = resolve_provider_set(configs);
        assert!(diags.is_empty());
        assert!(set.providers.is_empty());
    }

    #[test]
    fn duplicate_producer_diagnostic() {
        let configs = vec![
            (
                "dup".into(),
                ProviderConfig {
                    id: Some("dup".into()),
                    ..Default::default()
                },
            ),
            (
                "dup".into(),
                ProviderConfig {
                    id: Some("dup".into()),
                    base_url: Some("https://other".into()),
                    ..Default::default()
                },
            ),
        ];
        let (set, diags) = resolve_provider_set(configs);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].to_string().contains("dup"));
        assert_eq!(set.providers.len(), 1);
    }

    #[test]
    fn two_custom_compatible_providers() {
        let configs = vec![
            (
                "deepseek".into(),
                ProviderConfig {
                    id: Some("deepseek".into()),
                    kind: Some("openai_compatible".into()),
                    profile: Some("deepseek".into()),
                    base_url: Some("https://api.deepseek.com".into()),
                    ..Default::default()
                },
            ),
            (
                "internal".into(),
                ProviderConfig {
                    id: Some("internal".into()),
                    kind: Some("openai_compatible".into()),
                    base_url: Some("https://llm.internal/v1".into()),
                    protocol: Some("chat_completions".into()),
                    ..Default::default()
                },
            ),
        ];
        let (set, diags) = resolve_provider_set(configs);
        assert!(diags.is_empty());
        assert_eq!(set.providers.len(), 2);
        // Verify identity isolation
        let deepseek = set.providers.get(&ProviderId::new("deepseek")).unwrap();
        let internal = set.providers.get(&ProviderId::new("internal")).unwrap();
        assert_ne!(deepseek.config.public.base_url, internal.config.public.base_url);
    }
}
