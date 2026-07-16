use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::config::ProviderConfig;
use crate::model::Model;
use crate::provider::{ConfiguredProvider, SharedProvider};
use crate::route::Route;
use crate::types::ProviderId;

#[derive(Debug)]
pub struct ProviderRegistry {
    providers: RwLock<HashMap<ProviderId, SharedProvider>>,
    routes: RwLock<HashMap<String, Arc<Route>>>,
    configs: RwLock<HashMap<ProviderId, ProviderConfig>>,
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
            routes: RwLock::new(HashMap::new()),
            configs: RwLock::new(HashMap::new()),
        }
    }

    /// Store the resolved configuration for a provider so it can be
    /// queried later (e.g. when building ModelEntry items).
    pub fn store_config(&self, id: &ProviderId, config: ProviderConfig) {
        self.configs
            .write()
            .expect("ProviderRegistry lock poisoned")
            .insert(id.clone(), config);
    }

    /// Retrieve the last-stored configuration for a provider, if any.
    pub fn get_config(&self, id: &ProviderId) -> Option<ProviderConfig> {
        self.configs
            .read()
            .expect("ProviderRegistry lock poisoned")
            .get(id)
            .cloned()
    }

    pub fn register(&self, provider: SharedProvider) {
        let id = provider.id().clone();
        self.providers
            .write()
            .expect("ProviderRegistry lock poisoned")
            .insert(id, provider);
    }

    pub fn register_route(&self, id: impl Into<String>, route: Route) {
        self.routes
            .write()
            .expect("ProviderRegistry lock poisoned")
            .insert(id.into(), Arc::new(route));
    }

    pub fn get_route(&self, id: &str) -> Option<Arc<Route>> {
        self.routes
            .read()
            .expect("ProviderRegistry lock poisoned")
            .get(id)
            .cloned()
    }

    pub fn get(&self, id: &ProviderId) -> Option<SharedProvider> {
        self.providers
            .read()
            .expect("ProviderRegistry lock poisoned")
            .get(id)
            .cloned()
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
        self.providers
            .read()
            .expect("ProviderRegistry lock poisoned")
            .keys()
            .cloned()
            .collect()
    }

    pub fn detect_from_url(&self, base_url: &str) -> ProviderId {
        let host = url::Url::parse(base_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
            .unwrap_or_default();

        match host.as_str() {
            h if h == "api.x.ai" || h == "api.grok.com" || h.ends_with(".grok.com") => {
                ProviderId::new(ProviderId::XAI)
            }
            "api.openai.com" => ProviderId::new(ProviderId::OPENAI),
            "api.anthropic.com" => ProviderId::new(ProviderId::ANTHROPIC),
            h if h == "opencode.ai" || h == "console.opencode.ai" => {
                ProviderId::new(ProviderId::OPENCODE)
            }
            h if h == "localhost" || h.starts_with("localhost:") || h == "127.0.0.1" => {
                ProviderId::new(ProviderId::OLLAMA)
            }
            _ => ProviderId::new(ProviderId::OPENAI_COMPATIBLE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::framing::SseFraming;
    use crate::provider::{ConfiguredProvider, Provider};
    use crate::route::{Route, RouteDefaults, RouteInput};
    use crate::types::{ModelId, ProviderDefaults};

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

    impl Provider for DummyProvider {
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
                defaults: Some(RouteDefaults { headers: None }),
            });
            let id = ProviderId::new("dummy");
            ConfiguredProvider {
                id: id.clone(),
                route,
                model: |id, rt| {
                    Model::make(
                        ModelId::new(id),
                        ProviderId::new("dummy"),
                        Arc::new(rt.clone()),
                        None,
                    )
                },
                configure: move |c| DummyProvider::new().configure(c),
            }
        }

    }

    fn dummy_provider() -> SharedProvider {
        Arc::new(DummyProvider::new())
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
    fn registry_register_route_and_get() {
        let registry = ProviderRegistry::new();
        let route = Route::make(RouteInput {
            id: "test-route".into(),
            provider: None,
            protocol: "chat".into(),
            endpoint: Endpoint {
                base_url: None,
                path: EndpointPart::Static("/test".into()),
                query: None,
            },
            auth: None,
            framing: Box::new(SseFraming),
            defaults: None,
        });
        registry.register_route("my-route", route);
        let r = registry.get_route("my-route");
        assert!(r.is_some());
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
        assert_eq!(model.unwrap().id.0, "dummy-model");
    }

    #[test]
    fn registry_all_ids() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let ids = registry.all_ids();
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn detect_from_url_xai() {
        let registry = ProviderRegistry::new();
        assert_eq!(registry.detect_from_url("https://api.x.ai/v1").0, "xai");
        assert_eq!(registry.detect_from_url("https://api.grok.com/v1").0, "xai");
    }

    #[test]
    fn detect_from_url_openai() {
        let registry = ProviderRegistry::new();
        assert_eq!(
            registry.detect_from_url("https://api.openai.com/v1").0,
            "openai"
        );
    }

    #[test]
    fn detect_from_url_anthropic() {
        let registry = ProviderRegistry::new();
        assert_eq!(
            registry.detect_from_url("https://api.anthropic.com/v1").0,
            "anthropic"
        );
    }

    #[test]
    fn detect_from_url_ollama() {
        let registry = ProviderRegistry::new();
        assert_eq!(
            registry.detect_from_url("http://localhost:11434/v1").0,
            "ollama"
        );
    }

    #[test]
    fn detect_from_url_fallback() {
        let registry = ProviderRegistry::new();
        assert_eq!(
            registry.detect_from_url("https://api.groq.com/v1").0,
            "openai-compatible"
        );
    }

    #[test]
    fn store_config_and_get_config() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let pid = ProviderId::new("dummy");
        let cfg = ProviderConfig {
            id: Some("dummy".into()),
            api_key: Some("sk-test".into()),
            base_url: Some("https://dummy.test/v1".into()),
            ..Default::default()
        };
        registry.store_config(&pid, cfg.clone());
        let retrieved = registry.get_config(&pid);
        assert!(retrieved.is_some());
        let r = retrieved.unwrap();
        assert_eq!(r.api_key.as_deref(), Some("sk-test"));
        assert_eq!(r.base_url.as_deref(), Some("https://dummy.test/v1"));
    }

    #[test]
    fn get_config_unknown_returns_none() {
        let registry = ProviderRegistry::new();
        let pid = ProviderId::new("unknown");
        assert!(registry.get_config(&pid).is_none());
    }

    #[test]
    fn store_config_overwrites() {
        let registry = ProviderRegistry::new();
        registry.register(dummy_provider());
        let pid = ProviderId::new("dummy");
        registry.store_config(
            &pid,
            ProviderConfig {
                id: Some("dummy".into()),
                api_key: Some("first".into()),
                ..Default::default()
            },
        );
        registry.store_config(
            &pid,
            ProviderConfig {
                id: Some("dummy".into()),
                api_key: Some("second".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            registry.get_config(&pid).unwrap().api_key.as_deref(),
            Some("second")
        );
    }
}
