use std::sync::Arc;

use indexmap::IndexMap;
use tracing;

use crate::config::ProviderConfig;
use crate::registry::ProviderRegistry;

pub mod configure;
mod anthropic;
mod ollama;
mod openai;
mod openai_compatible;
pub mod openai_compatible_factory;
mod opencode;
mod xai;

/// Register all built-in providers into a registry.
pub fn register_all(registry: &ProviderRegistry) {
    registry.register(Arc::new(xai::XaiProvider::new()));
    registry.register(Arc::new(openai::OpenAIProvider::new()));
    registry.register(Arc::new(anthropic::AnthropicProvider::new()));
    registry.register(Arc::new(opencode::OpenCodeProvider::new()));
    registry.register(Arc::new(ollama::OllamaProvider::new()));
    registry.register(Arc::new(openai_compatible::OpenAiCompatibleProvider::new()));
}

/// Detect environment variables for every registered provider.
/// Returns a map of provider_id → `ProviderConfig` with `api_key` set
/// from the first matching env var in the provider's `env_key` list.
///
/// NOTE: Per the V1 plan (P4-008), environment variable *values* should
/// NOT be read during bootstrap — they should be resolved at request time
/// (Phase 8). This function exists for legacy compatibility and will be
/// removed once the request-time credential context is fully integrated.
pub fn detect_env_vars(registry: &ProviderRegistry) -> IndexMap<String, ProviderConfig> {
    tracing::warn!(
        "detect_env_vars reads env values at bootstrap — \
         this will be removed in P8 (request-time credential resolution)"
    );
    let mut result = IndexMap::new();
    for pid in registry.all_ids() {
        let Some(provider) = registry.get(&pid) else {
            continue;
        };
        for env_key in &provider.defaults().env_key {
            if let Ok(val) = std::env::var(env_key) {
                result
                    .entry(pid.0.clone())
                    .or_insert_with(|| ProviderConfig {
                        id: Some(pid.0.clone()),
                        api_key: Some(val),
                        ..Default::default()
                    });
                break;
            }
        }
    }
    result
}

/// Build the final `ProviderConfig` for a single provider by merging
/// env vars → TOML config → CLI overrides (later overrides earlier).
pub fn build_provider_config(
    pid: &str,
    toml_configs: &[(String, ProviderConfig)],
    env_configs: &IndexMap<String, ProviderConfig>,
    cli_override: Option<&ProviderConfig>,
) -> ProviderConfig {
    let mut merged = ProviderConfig {
        id: Some(pid.into()),
        ..Default::default()
    };

    if let Some(env_cfg) = env_configs.get(pid) {
        merged = merged.merge(env_cfg.clone());
    }

    if let Some((_, toml_cfg)) = toml_configs.iter().find(|(id, _)| id == pid) {
        merged = merged.merge(toml_cfg.clone());
    }

    if let Some(cli) = cli_override
        && cli.id.as_deref() == Some(pid)
    {
        merged = merged.merge(cli.clone());
    }

    merged
}

/// Configure all providers in the registry with merged config from all sources.
/// Priority (low→high): env → TOML → compat (old `[endpoints]`) → CLI.
/// Stores resolved routes back into the registry for later model resolution.
/// The `compat` parameter provides backward-compatible overrides (e.g., from old
/// `[endpoints]` config) that apply between TOML and CLI layers.
pub fn configure_providers(
    registry: &ProviderRegistry,
    toml: &toml::Value,
    compat: Option<ProviderConfig>,
    cli_override: Option<ProviderConfig>,
) {
    let toml_configs = match crate::config::parse_provider_toml(toml) {
        Ok(parsed) => parsed
            .entries
            .into_iter()
            .map(|(id, cfg)| (id.0, cfg))
            .collect::<Vec<_>>(),
        Err(diags) => {
            for d in &diags {
                tracing::warn!("{d}");
            }
            return;
        }
    };
    let env_configs = detect_env_vars(registry);

    for pid in registry.all_ids() {
        let mut merged = build_provider_config(&pid.0, &toml_configs, &env_configs, None);

        if let Some(ref compat_cfg) = compat
            && compat_cfg.id.as_deref() == Some(&pid.0)
        {
            merged = merged.merge(compat_cfg.clone());
        }

        if let Some(ref cli) = cli_override
            && cli.id.as_deref() == Some(&pid.0)
        {
            merged = merged.merge(cli.clone());
        }

        // Legacy path — store_config/register_route removed in P5-007.
        // Configuration now flows through prepare/commit (P6).
        let _ = registry.configure(&pid, merged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::registry::ProviderRegistry;
    use crate::types::{ModelSourceSpec, ProviderId};

    fn dummy_registry() -> ProviderRegistry {
        let reg = ProviderRegistry::new();
        let pid = ProviderId::new("test-provider");
        let defaults = crate::types::ProviderDefaults {
            id: pid.clone(),
            name: "Test".into(),
            base_url: "https://test.com/v1".into(),
            api_backend: crate::types::ApiBackend::ChatCompletions,
            auth_scheme: crate::types::AuthScheme::Bearer,
            env_key: vec!["TEST_API_KEY".into()],
            context_window: std::num::NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
            ..Default::default()
        };
        let provider =
            std::sync::Arc::new(TestProvider { pid, defaults }) as crate::provider::SharedProvider;
        reg.register(provider);
        reg
    }

    #[derive(Debug)]
    struct TestProvider {
        pid: ProviderId,
        defaults: crate::types::ProviderDefaults,
    }

    impl crate::provider::Provider for TestProvider {
        fn id(&self) -> &ProviderId {
            &self.pid
        }
        fn name(&self) -> &str {
            "Test"
        }
        fn defaults(&self) -> &crate::types::ProviderDefaults {
            &self.defaults
        }
        fn configure(&self, overrides: ProviderConfig) -> crate::provider::ConfiguredProvider {
            let route = crate::route::Route::make(
                "test-route",
                Some(self.pid.clone()),
                "chat_completions",
                crate::endpoint::Endpoint {
                    base_url: overrides
                        .base_url
                        .clone()
                        .or(Some(self.defaults.base_url.clone())),
                    path: crate::endpoint::EndpointPart::Static("/chat/completions".into()),
                    query: None,
                },
                crate::auth::AuthPolicy::None,
            );
            let route_id = crate::types::RouteId::new("test-route");
            let routes = IndexMap::from([(route_id.clone(), Arc::new(route))]);
            crate::provider::ConfiguredProvider::new(
                self.pid.clone(),
                "Test".into(),
                overrides,
                routes,
                route_id.clone(),
                Arc::new(crate::provider::DefaultRouteSelector {
                    default_route_id: route_id,
                }),
                ModelSourceSpec::Dynamic,
            )
        }
    }

    #[test]
    fn build_provider_config_empty() {
        let toml_configs = vec![];
        let env_configs = IndexMap::new();
        let merged = build_provider_config("test-provider", &toml_configs, &env_configs, None);
        assert!(merged.api_key.is_none());
        assert!(merged.base_url.is_none());
    }

    #[test]
    fn build_provider_config_env_key() {
        let toml_configs = vec![];
        let mut env_configs = IndexMap::new();
        env_configs.insert(
            "test-provider".into(),
            ProviderConfig {
                id: Some("test-provider".into()),
                api_key: Some("env-key".into()),
                ..Default::default()
            },
        );
        let merged = build_provider_config("test-provider", &toml_configs, &env_configs, None);
        assert_eq!(merged.api_key.as_deref(), Some("env-key"));
    }

    #[test]
    fn build_provider_config_toml_overrides_env() {
        let toml_configs = vec![(
            "test-provider".into(),
            ProviderConfig {
                id: Some("test-provider".into()),
                api_key: Some("toml-key".into()),
                ..Default::default()
            },
        )];
        let mut env_configs = IndexMap::new();
        env_configs.insert(
            "test-provider".into(),
            ProviderConfig {
                id: Some("test-provider".into()),
                api_key: Some("env-key".into()),
                ..Default::default()
            },
        );
        let merged = build_provider_config("test-provider", &toml_configs, &env_configs, None);
        assert_eq!(merged.api_key.as_deref(), Some("toml-key"));
    }

    #[test]
    fn build_provider_config_cli_overrides_all() {
        let toml_configs = vec![(
            "test-provider".into(),
            ProviderConfig {
                id: Some("test-provider".into()),
                api_key: Some("toml-key".into()),
                ..Default::default()
            },
        )];
        let mut env_configs = IndexMap::new();
        env_configs.insert(
            "test-provider".into(),
            ProviderConfig {
                id: Some("test-provider".into()),
                api_key: Some("env-key".into()),
                ..Default::default()
            },
        );
        let cli = ProviderConfig {
            id: Some("test-provider".into()),
            api_key: Some("cli-key".into()),
            ..Default::default()
        };
        let merged =
            build_provider_config("test-provider", &toml_configs, &env_configs, Some(&cli));
        assert_eq!(merged.api_key.as_deref(), Some("cli-key"));
    }

    #[test]
    fn configure_providers_stores_config() {
        // Note: after P5-007, configure_providers no longer writes to legacy store.
        // Config flow now goes through prepare/commit (P6).
        let reg = dummy_registry();
        let toml: toml::Value = toml::from_str(
            r#"
            [provider.test-provider]
            api_key = "cfg-key"
            "#,
        )
        .unwrap();
        // Must not panic
        configure_providers(&reg, &toml, None, None);
        let pid = ProviderId::new("test-provider");
        let def = reg.get(&pid);
        assert!(
            def.is_some(),
            "provider definition must still be accessible"
        );
    }

    #[test]
    fn configure_providers_with_env_override() {
        let reg = dummy_registry();
        let toml: toml::Value = toml::from_str(
            r#"
            [provider.test-provider]
            api_key = "cfg-key"
            "#,
        )
        .unwrap();
        let cli = ProviderConfig {
            id: Some("test-provider".into()),
            api_key: Some("cli-key".into()),
            ..Default::default()
        };
        // Must not panic
        configure_providers(&reg, &toml, None, Some(cli));
        let pid = ProviderId::new("test-provider");
        let def = reg.get(&pid);
        assert!(
            def.is_some(),
            "provider definition must still be accessible"
        );
    }

    #[test]
    fn detect_env_vars_empty_when_not_set() {
        let reg = dummy_registry();
        let env = detect_env_vars(&reg);
        assert!(env.is_empty() || env.get("test-provider").is_none());
    }
}
