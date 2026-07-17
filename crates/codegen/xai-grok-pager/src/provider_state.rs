use std::sync::Arc;
use std::sync::OnceLock;

use xai_grok_provider::registry::ProviderRegistry;

static PROVIDER_REGISTRY: OnceLock<Arc<ProviderRegistry>> = OnceLock::new();

pub fn init(registry: Arc<ProviderRegistry>) {
    PROVIDER_REGISTRY.set(registry).ok();
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
