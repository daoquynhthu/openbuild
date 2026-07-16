use std::sync::Arc;

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

/// Parse `[provider.*]` TOML sections and log configuration details.
/// The actual configuration is applied lazily when `registry.configure()` is called
/// during model resolution. Built-in provider defaults are merged with user overrides
/// at that point.
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
        }
    }
}
