use std::collections::HashMap;
use std::sync::RwLock;

use crate::config::ProviderConfig;
use crate::model::Model;
use crate::provider::{ConfiguredProvider, SharedProvider};
use crate::types::ProviderId;

#[derive(Debug)]
pub struct ProviderRegistry {
    providers: RwLock<HashMap<ProviderId, SharedProvider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, provider: SharedProvider) {
        let id = provider.id().clone();
        self.providers.write().unwrap().insert(id, provider);
    }

    pub fn get(&self, id: &ProviderId) -> Option<SharedProvider> {
        self.providers.read().unwrap().get(id).cloned()
    }

    pub fn configure(
        &self,
        id: &ProviderId,
        overrides: ProviderConfig,
    ) -> Option<ConfiguredProvider> {
        self.get(id).map(|p| p.configure(overrides))
    }

    pub fn model(
        &self,
        provider_id: &ProviderId,
        model_id: &str,
        overrides: ProviderConfig,
    ) -> Option<Model> {
        self.configure(provider_id, overrides)
            .map(|cp| (cp.model)(model_id, &cp.route))
    }

    pub fn all_ids(&self) -> Vec<ProviderId> {
        self.providers.read().unwrap().keys().cloned().collect()
    }

    pub fn detect_from_url(&self, _base_url: &str) -> ProviderId {
        ProviderId::new("openai-compatible")
    }
}
