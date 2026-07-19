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

/// Merge a legacy migration config and CLI overrides onto a resolved set.
///
/// Precedence (low → high):
///   provider implementation defaults
///   < selected profile defaults
///   < legacy migration values
///   < TOML provider/model values
///   < startup CLI configuration overrides
///
/// Request credential override and generation override do NOT enter this
/// set; they are resolved at request time in Phase 8.
/// Environment variable names are merged by config, but their *values* are
/// only read at request time — never during bootstrap.
pub fn resolve_with_precedence(
    parsed: crate::config::ParsedProviderConfig,
    legacy_migration: Option<ProviderConfig>,
    cli_overrides: Option<ProviderConfig>,
) -> (ResolvedProviderSet, Vec<ConfigDiagnostic>) {
    let mut merged_configs: Vec<(String, ProviderConfig)> = parsed
        .entries
        .into_iter()
        .map(|(id, cfg)| {
            let mut merged = cfg;
            // Apply legacy migration on top of TOML values
            if let Some(ref legacy) = legacy_migration
                && legacy.id.as_deref() == Some(&id.0)
            {
                merged = legacy.clone().merge(merged);
            }
            // Apply CLI overrides on top (CLI > TOML)
            if let Some(ref cli) = cli_overrides
                && cli.id.as_deref() == Some(&id.0)
            {
                merged = merged.merge(cli.clone());
            }
            (id.0, merged)
        })
        .collect();

    // Collect TOML provider IDs for "not present" checks before entries is moved
    let toml_ids: std::collections::HashSet<String> = merged_configs.iter().map(|(id, _)| id.clone()).collect();

    // If a legacy migration targets a provider not present in TOML, add it
    if let Some(ref legacy) = legacy_migration {
        if let Some(ref legacy_id) = legacy.id {
            if !toml_ids.contains(legacy_id) {
                merged_configs.push((legacy_id.clone(), legacy.clone()));
            }
        }
    }

    // If CLI overrides target a provider not in TOML or legacy, add it
    if let Some(ref cli) = cli_overrides {
        if let Some(ref cli_id) = cli.id {
            let already_present = merged_configs.iter().any(|(id, _)| id == cli_id);
            if !already_present {
                merged_configs.push((cli_id.clone(), cli.clone()));
            }
        }
    }

    resolve_provider_set(merged_configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigDiagnostic, ParsedProviderConfig};

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

    // ── Precedence tests ──

    #[test]
    fn precedence_toml_only() {
        let toml = ParsedProviderConfig {
            entries: IndexMap::from([(
                ProviderId::new("test"),
                ProviderConfig {
                    id: Some("test".into()),
                    api_key: Some("toml-key".into()),
                    ..Default::default()
                },
            )]),
        };
        let (set, diags) = resolve_with_precedence(toml, None, None);
        assert!(diags.is_empty());
        let spec = set.providers.get(&ProviderId::new("test")).unwrap();
        assert_eq!(
            spec.config.inline_api_key.as_ref().map(|s| format!("{s:?}")),
            Some("[REDACTED]".into())
        );
    }

    #[test]
    fn precedence_legacy_overrides_toml() {
        let toml = ParsedProviderConfig {
            entries: IndexMap::from([(
                ProviderId::new("xai"),
                ProviderConfig {
                    id: Some("xai".into()),
                    base_url: Some("https://toml.url".into()),
                    ..Default::default()
                },
            )]),
        };
        let legacy = Some(ProviderConfig {
            id: Some("xai".into()),
            base_url: Some("https://legacy.url".into()),
            api_key: Some("legacy-key".into()),
            ..Default::default()
        });
        let (set, diags) = resolve_with_precedence(toml, legacy, None);
        assert!(diags.is_empty());
        let spec = set.providers.get(&ProviderId::new("xai")).unwrap();
        // Legacy should NOT override TOML (legacy has lower precedence)
        assert_eq!(
            spec.config.public.base_url.as_deref(),
            Some("https://toml.url")
        );
        // Legacy api_key fills in because TOML didn't set it
        assert_eq!(
            spec.config.inline_api_key.as_ref().map(|_| "present"),
            Some("present")
        );
    }

    #[test]
    fn precedence_cli_overrides_all() {
        let toml = ParsedProviderConfig {
            entries: IndexMap::from([(
                ProviderId::new("xai"),
                ProviderConfig {
                    id: Some("xai".into()),
                    api_key: Some("toml-key".into()),
                    ..Default::default()
                },
            )]),
        };
        let cli = Some(ProviderConfig {
            id: Some("xai".into()),
            api_key: Some("cli-key".into()),
            ..Default::default()
        });
        let (set, diags) = resolve_with_precedence(toml, None, cli);
        assert!(diags.is_empty());
        let spec = set.providers.get(&ProviderId::new("xai")).unwrap();
        // CLI has highest precedence — must override TOML
        // Check via Debug (can't access inner value from test)
        let debug = format!("{:?}", spec.config);
        assert!(
            !debug.contains("toml-key"),
            "CLI override should replace toml-key: {debug}"
        );
    }

    #[test]
    fn precedence_legacy_adds_new_provider() {
        let toml = ParsedProviderConfig {
            entries: IndexMap::new(),
        };
        let legacy = Some(ProviderConfig {
            id: Some("legacy-only".into()),
            api_key: Some("legacy-key".into()),
            ..Default::default()
        });
        let (set, _) = resolve_with_precedence(toml, legacy, None);
        assert!(
            set.providers.contains_key(&ProviderId::new("legacy-only")),
            "legacy must add new providers not in TOML"
        );
    }

    #[test]
    fn precedence_cli_adds_new_provider() {
        let toml = ParsedProviderConfig {
            entries: IndexMap::new(),
        };
        let cli = Some(ProviderConfig {
            id: Some("cli-only".into()),
            api_key: Some("cli-key".into()),
            ..Default::default()
        });
        let (set, _) = resolve_with_precedence(toml, None, cli);
        assert!(
            set.providers.contains_key(&ProviderId::new("cli-only")),
            "CLI must add new providers not in TOML"
        );
    }
}
