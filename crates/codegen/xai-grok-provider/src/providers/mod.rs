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
