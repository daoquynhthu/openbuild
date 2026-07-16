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
        let env_keys = provider.defaults().env_key.clone();
        for env_key in &env_keys {
            if let Ok(val) = std::env::var(env_key) {
                result.entry(pid.0.clone()).or_insert_with(|| ProviderConfig {
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

    // Lowest priority: env vars
    if let Some(env_cfg) = env_configs.get(pid) {
        merged = merged.merge(env_cfg.clone());
    }

    // Middle priority: TOML [provider.*]
    if let Some((_, toml_cfg)) = toml_configs.iter().find(|(id, _)| id == pid) {
        merged = merged.merge(toml_cfg.clone());
    }

    // Highest priority: CLI overrides (--api-key, --base-url)
    if let Some(cli) = cli_override
        && cli.id.as_deref() == Some(pid)
    {
        merged = merged.merge(cli.clone());
    }

    merged
}

/// Configure all providers in the registry with merged config from all sources.
/// Stores resolved routes back into the registry for later model resolution.
pub fn configure_providers(
    registry: &ProviderRegistry,
    toml: &toml::Value,
    cli_override: Option<ProviderConfig>,
) {
    let toml_configs = crate::config::parse_provider_toml(toml);
    let env_configs = detect_env_vars(registry);

    for pid in registry.all_ids() {
        let merged = build_provider_config(
            &pid.0,
            &toml_configs,
            &env_configs,
            cli_override.as_ref(),
        );

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
