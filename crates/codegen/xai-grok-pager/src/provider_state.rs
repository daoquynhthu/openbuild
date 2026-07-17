use std::sync::Arc;
use std::sync::OnceLock;

use xai_grok_provider::registry::ProviderRegistry;

static PROVIDER_REGISTRY: OnceLock<Arc<ProviderRegistry>> = OnceLock::new();

pub fn init(registry: Arc<ProviderRegistry>) {
    if PROVIDER_REGISTRY.set(registry).is_err() {
        tracing::warn!("provider_state::init called more than once — second call ignored");
    }
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
