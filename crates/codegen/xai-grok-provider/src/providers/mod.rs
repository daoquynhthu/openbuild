use std::sync::Arc;

use indexmap::IndexMap;

use crate::config::ProviderConfig;
use crate::registry::ProviderRegistry;

mod anthropic;
mod ollama;
mod openai;
mod openai_compatible;
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
pub fn detect_env_vars(registry: &ProviderRegistry) -> IndexMap<String, ProviderConfig> {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::config::ProviderConfig;
        use crate::registry::ProviderRegistry;
        use crate::types::ProviderId;

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
            let provider = std::sync::Arc::new(TestProvider { pid, defaults })
                as crate::provider::SharedProvider;
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
                let route = crate::route::Route::make(crate::route::RouteInput {
                    id: "test-route".into(),
                    provider: Some(self.pid.clone()),
                    protocol: "chat_completions".into(),
                    endpoint: crate::endpoint::Endpoint {
                        base_url: overrides
                            .base_url
                            .clone()
                            .or(Some(self.defaults.base_url.clone())),
                        path: crate::endpoint::EndpointPart::Static("/chat/completions".into()),
                        query: None,
                    },
                    auth: None,
                    framing: Box::new(crate::framing::SseFraming),
                    defaults: None,
                });
                crate::provider::ConfiguredProvider {
                    id: self.pid.clone(),
                    route,
                    model: Box::new(|id, rt| {
                        crate::model::Model::make(
                            crate::types::ModelId::new(id),
                            crate::types::ProviderId::new("test-provider"),
                            std::sync::Arc::new(rt.clone()),
                            None,
                        )
                    }),
                    configure: Box::new(|c| {
                        TestProvider {
                            pid: crate::types::ProviderId::new("test-provider"),
                            defaults: crate::types::ProviderDefaults::default(),
                        }
                        .configure(c)
                    }),
                }
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
            let reg = dummy_registry();
            let toml: toml::Value = toml::from_str(
                r#"
            [provider.test-provider]
            api_key = "cfg-key"
            "#,
            )
            .unwrap();
            configure_providers(&reg, &toml, None, None);
            let pid = ProviderId::new("test-provider");
            let stored = reg.get_config(&pid);
            assert!(stored.is_some());
            assert_eq!(stored.unwrap().api_key.as_deref(), Some("cfg-key"));
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
            configure_providers(&reg, &toml, None, Some(cli));
            let pid = ProviderId::new("test-provider");
            let stored = reg.get_config(&pid);
            assert_eq!(stored.unwrap().api_key.as_deref(), Some("cli-key"));
        }

        #[test]
        fn detect_env_vars_empty_when_not_set() {
            let reg = dummy_registry();
            let env = detect_env_vars(&reg);
            assert!(env.is_empty() || env.get("test-provider").is_none());
        }
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
/// Priority (low→high): env → TOML → compat (old [endpoints]) → CLI.
/// Stores resolved routes back into the registry for later model resolution.
/// The `compat` parameter provides backward-compatible overrides (e.g., from old
/// `[endpoints]` config) that apply between TOML and CLI layers.
pub fn configure_providers(
    registry: &ProviderRegistry,
    toml: &toml::Value,
    compat: Option<ProviderConfig>,
    cli_override: Option<ProviderConfig>,
) {
    let toml_configs = crate::config::parse_provider_toml(toml);
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

        registry.store_config(&pid, merged.clone());
        if let Some(cp) = registry.configure(&pid, merged) {
            let route_id = cp.route.id.clone();
            registry.register_route(&route_id, cp.route);
        }
    }
}

/// Parse `[provider.*]` TOML sections, log configuration details,
/// and apply config to the registry (calling `configure` on each provider).
pub fn register_from_config(registry: &ProviderRegistry, toml: &toml::Value) {
    let configs = crate::config::parse_provider_toml(toml);
    for (id, provider_config) in &configs {
        let provider_id = crate::types::ProviderId::new(id);
        if registry.get(&provider_id).is_some() {
            tracing::info!(
                provider = %id,
                has_api_key = provider_config.api_key.is_some()
                    || provider_config.env_key.as_ref().is_some_and(|k| !k.is_empty()),
                "configured provider from [provider.*]",
            );
            registry.store_config(&provider_id, provider_config.clone());
            if let Some(cp) = registry.configure(&provider_id, provider_config.clone()) {
                let route_id = cp.route.id.clone();
                registry.register_route(&route_id, cp.route);
            }
        } else {
            tracing::warn!(
                provider = %id,
                "unknown provider in [provider.*] config — skipping",
            );
        }
    }
}
