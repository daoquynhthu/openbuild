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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::framing::SseFraming;
    use crate::provider::ConfiguredProvider;
    use crate::route::{Route, RouteInput};
    use crate::types::ProviderDefaults;

    fn dummy_provider() -> SharedProvider {
        std::sync::Arc::new(DummyProvider::new())
    }

    #[derive(Debug)]
    struct DummyProvider {
        id: ProviderId,
        defaults: ProviderDefaults,
    }

    impl DummyProvider {
        fn new() -> Self {
            Self {
                id: ProviderId::new("dummy"),
                defaults: ProviderDefaults::default(),
            }
        }
    }

    impl crate::provider::Provider for DummyProvider {
        fn id(&self) -> &ProviderId {
            &self.id
        }

        fn name(&self) -> &str {
            "Dummy"
        }

        fn defaults(&self) -> &ProviderDefaults {
            &self.defaults
        }

        fn configure(&self, _overrides: ProviderConfig) -> ConfiguredProvider {
            let route = Route::make(RouteInput {
                id: "dummy".into(),
                provider: Some(ProviderId::new("dummy")),
                protocol: "chat_completions".into(),
                endpoint: Endpoint {
                    base_url: Some("https://dummy.com/v1".into()),
                    path: EndpointPart::Static("/chat/completions".into()),
                    query: None,
                },
                auth: None,
                framing: Box::new(SseFraming),
                defaults: None,
            });
            ConfiguredProvider {
                id: ProviderId::new("dummy"),
                route,
                model: |id, route| {
                    Model::make(id, "dummy", std::sync::Arc::new(route.clone()), None)
                },
            }
        }

        fn known_models(&self) -> &[crate::types::ProviderModelDef] {
            &[]
        }
    }

    #[test]
    fn registry_register_and_get() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let p = registry.get(&ProviderId::new("dummy"));
        assert!(p.is_some());
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let registry = ProviderRegistry::new();
        let p = registry.get(&ProviderId::new("unknown"));
        assert!(p.is_none());
    }

    #[test]
    fn registry_configure_creates_configured_provider() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let cp = registry.configure(&ProviderId::new("dummy"), ProviderConfig::default());
        assert!(cp.is_some());
        assert_eq!(cp.unwrap().route.protocol, "chat_completions");
    }

    #[test]
    fn registry_model_creates_model() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let model = registry.model(
            &ProviderId::new("dummy"),
            "dummy-model",
            ProviderConfig::default(),
        );
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "dummy-model");
    }

    #[test]
    fn registry_all_ids() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let ids = registry.all_ids();
        assert_eq!(ids.len(), 1);
    }
}
