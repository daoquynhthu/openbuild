use std::sync::Arc;
use std::sync::OnceLock;

use xai_grok_provider::registry::ProviderRegistry;

static PROVIDER_REGISTRY: OnceLock<Arc<ProviderRegistry>> = OnceLock::new();

/// Initialize the global provider registry. Returns an error if already set.
pub fn init(registry: Arc<ProviderRegistry>) -> Result<(), &'static str> {
    PROVIDER_REGISTRY
        .set(registry)
        .map_err(|_| "provider_state::init called more than once")
}

pub fn registry() -> Option<&'static Arc<ProviderRegistry>> {
    PROVIDER_REGISTRY.get()
}

pub fn configured_providers() -> Vec<String> {
    match PROVIDER_REGISTRY.get() {
        Some(reg) => reg.all_ids().into_iter().map(|p| p.0).collect(),
        None => vec![],
    }
}
