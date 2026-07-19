use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::config::ProviderConfig;
use crate::error::ProviderError;
use crate::provider::{ConfiguredProvider, SharedProvider};

use crate::route::Route;
use crate::types::{ProviderId, RouteId};

// ── Frozen types (P5) ──

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderFactoryKind {
    OpenAiCompatible,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProviderRouteKey {
    pub provider_id: ProviderId,
    pub local_route_id: RouteId,
}

#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub revision: u64,
    pub providers: IndexMap<ProviderId, Arc<ConfiguredProvider>>,
    pub routes: IndexMap<ProviderRouteKey, Arc<Route>>,
}

#[derive(Debug)]
struct RegistryState {
    definitions: IndexMap<ProviderId, SharedProvider>,
    factories: IndexMap<
        ProviderFactoryKind,
        crate::providers::openai_compatible_factory::SharedProviderFactory,
    >,
    snapshot: Arc<RegistrySnapshot>,
    sealed: bool,
    /// Legacy config store — transitional, will be removed by P5-007.
    legacy_configs: HashMap<ProviderId, ProviderConfig>,
}

/// Process-wide provider registry with transactional snapshot semantics.
#[derive(Debug)]
pub struct ProviderRegistry {
    state: RwLock<RegistryState>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(RegistryState {
                definitions: IndexMap::new(),
                factories: IndexMap::new(),
                snapshot: Arc::new(RegistrySnapshot {
                    revision: 0,
                    providers: IndexMap::new(),
                    routes: IndexMap::new(),
                }),
                sealed: false,
                legacy_configs: HashMap::new(),
            }),
        }
    }

    pub fn register_definition(&self, provider: SharedProvider) -> Result<(), ProviderError> {
        let mut state = self.state.write();
        if state.sealed {
            return Err(ProviderError::Config(
                "registry is sealed — cannot register new definitions".into(),
            ));
        }
        let id = provider.id().clone();
        if state.definitions.contains_key(&id) {
            return Err(ProviderError::DuplicateProvider(format!(
                "provider {} already registered",
                id.0
            )));
        }
        state.definitions.insert(id, provider);
        Ok(())
    }

    pub fn register_factory(
        &self,
        kind: ProviderFactoryKind,
        factory: crate::providers::openai_compatible_factory::SharedProviderFactory,
    ) -> Result<(), ProviderError> {
        let mut state = self.state.write();
        if state.sealed {
            return Err(ProviderError::Config(
                "registry is sealed — cannot register new factories".into(),
            ));
        }
        if state.factories.contains_key(&kind) {
            return Err(ProviderError::Config(format!(
                "factory for {kind:?} already registered"
            )));
        }
        state.factories.insert(kind, factory);
        Ok(())
    }

    pub fn register(&self, provider: SharedProvider) {
        let _ = self.register_definition(provider);
    }

    pub fn get(&self, id: &ProviderId) -> Option<SharedProvider> {
        self.state.read().definitions.get(id).cloned()
    }

    pub fn all_ids(&self) -> Vec<ProviderId> {
        self.state.read().definitions.keys().cloned().collect()
    }

    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        self.state.read().snapshot.clone()
    }

    pub fn configured(&self, id: &ProviderId) -> Option<Arc<ConfiguredProvider>> {
        self.state.read().snapshot.providers.get(id).cloned()
    }

    // ── Legacy config storage (transitional — will be removed by P5-007) ──

    pub fn store_config(&self, id: &ProviderId, config: ProviderConfig) {
        self.state.write().legacy_configs.insert(id.clone(), config);
    }

    pub fn get_config(&self, id: &ProviderId) -> Option<ProviderConfig> {
        let state = self.state.read();
        // Check legacy store first, then snapshot
        state
            .legacy_configs
            .get(id)
            .cloned()
            .or_else(|| state.snapshot.providers.get(id).map(|cp| cp.config.clone()))
    }

    pub fn register_route(&self, _id: impl Into<String>, _route: Route) {
        tracing::debug!("register_route called (legacy path, no-op)");
    }

    pub fn get_route(&self, id: &str) -> Option<Arc<Route>> {
        let state = self.state.read();
        for (key, route) in &state.snapshot.routes {
            if key.local_route_id.0 == id {
                return Some(route.clone());
            }
        }
        None
    }

    pub fn configure(
        &self,
        id: &ProviderId,
        overrides: ProviderConfig,
    ) -> Option<ConfiguredProvider> {
        self.get(id).map(|p| p.configure(overrides))
    }

    // ── Legacy rebuild (will be replaced by prepare/commit) ──

    pub fn rebuild(
        &self,
        configs: &IndexMap<ProviderId, ProviderConfig>,
    ) -> Result<u64, ProviderError> {
        let state = self.state.read();
        let defs = &state.definitions;

        let mut new_providers: IndexMap<ProviderId, Arc<ConfiguredProvider>> = IndexMap::new();
        let mut new_routes: IndexMap<ProviderRouteKey, Arc<Route>> = IndexMap::new();

        for (pid, provider) in defs.iter() {
            let overrides = configs.get(pid).cloned().unwrap_or_default();
            let configured = provider.configure(overrides);

            for (rid, route) in &configured.routes {
                let key = ProviderRouteKey {
                    provider_id: pid.clone(),
                    local_route_id: rid.clone(),
                };
                if new_routes.contains_key(&key) {
                    return Err(ProviderError::DuplicateRoute(format!(
                        "duplicate route: {} for provider {}",
                        rid.0, pid.0
                    )));
                }
                route
                    .validate()
                    .map_err(|e| ProviderError::InvalidRouteId(format!("route {}: {e}", rid.0)))?;
                new_routes.insert(key, route.clone());
            }
            new_providers.insert(pid.clone(), Arc::new(configured));
        }

        let current_revision = state.snapshot.revision;
        let new_revision = current_revision + 1;
        let new_snapshot = Arc::new(RegistrySnapshot {
            revision: new_revision,
            providers: new_providers,
            routes: new_routes,
        });

        drop(state);
        let mut state = self.state.write();
        state.snapshot = new_snapshot;
        state.sealed = true;

        Ok(new_revision)
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthPolicy;
    use crate::config::ProviderConfig;
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider};
    use crate::types::{ModelSourceSpec, ProviderDefaults};

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
        fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
            let rid = RouteId::new("chat");
            let route = Arc::new(Route::make(
                rid.0.clone(),
                Some(self.id.clone()),
                "chat_completions",
                Endpoint {
                    base_url: overrides.base_url.clone(),
                    path: EndpointPart::Static("/chat/completions".into()),
                    query: None,
                },
                AuthPolicy::None,
            ));
            let routes = IndexMap::from([(rid.clone(), route)]);
            ConfiguredProvider::new(
                self.id.clone(),
                self.name().to_string(),
                overrides,
                routes,
                rid.clone(),
                Arc::new(DefaultRouteSelector {
                    default_route_id: rid,
                }),
                ModelSourceSpec::Dynamic,
            )
        }
    }

    #[test]
    fn registry_new_has_revision_zero() {
        let reg = ProviderRegistry::new();
        assert_eq!(reg.snapshot().revision, 0);
    }

    #[test]
    fn registry_register_definition_and_get() {
        let reg = ProviderRegistry::new();
        let p = Arc::new(DummyProvider::new());
        reg.register_definition(p.clone()).unwrap();
        assert_eq!(reg.get(&ProviderId::new("dummy")).unwrap().id().0, "dummy");
    }

    #[test]
    fn registry_register_definition_rejects_duplicate() {
        let reg = ProviderRegistry::new();
        let p = Arc::new(DummyProvider::new());
        reg.register_definition(p.clone()).unwrap();
        let r = reg.register_definition(p);
        assert!(r.is_err());
    }

    #[test]
    fn rebuild_increments_revision() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        let rev = reg.rebuild(&IndexMap::new()).unwrap();
        assert_eq!(rev, 1);
        assert_eq!(reg.snapshot().revision, 1);
    }

    #[test]
    fn rebuild_second_call_increments_again() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        let r1 = reg.rebuild(&IndexMap::new()).unwrap();
        let r2 = reg.rebuild(&IndexMap::new()).unwrap();
        assert_eq!(r2, r1 + 1);
    }

    #[test]
    fn registry_configured_returns_provider() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        reg.rebuild(&IndexMap::new()).unwrap();
        let cp = reg.configured(&ProviderId::new("dummy"));
        assert!(cp.is_some());
        assert_eq!(cp.unwrap().id.0, "dummy");
    }

    #[test]
    fn registry_route_lookup() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        reg.rebuild(&IndexMap::new()).unwrap();
        let route = reg.get_route("chat");
        assert!(route.is_some());
    }

    #[test]
    fn store_config_and_get_config() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        let pid = ProviderId::new("dummy");
        reg.store_config(
            &pid,
            ProviderConfig {
                id: Some("dummy".into()),
                api_key: Some("stored".into()),
                ..Default::default()
            },
        );
        reg.rebuild(&IndexMap::new()).unwrap();
        let cfg = reg.get_config(&pid);
        assert!(cfg.is_some());
    }

    #[test]
    fn get_config_unknown_returns_none() {
        let reg = ProviderRegistry::new();
        let cfg = reg.get_config(&ProviderId::new("unknown"));
        assert!(cfg.is_none());
    }

    #[test]
    fn registry_seals_after_first_rebuild() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        reg.rebuild(&IndexMap::new()).unwrap();
        let result = reg.register_definition(Arc::new(DummyProvider {
            id: ProviderId::new("second"),
            ..DummyProvider::new()
        }));
        assert!(
            result.is_err(),
            "registry must reject new definitions after seal"
        );
        assert!(result.unwrap_err().to_string().contains("sealed"));
    }

    #[test]
    fn concurrent_rebuild_gives_sequential_revisions() {
        let reg = Arc::new(ProviderRegistry::new());
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();

        let reg1 = Arc::clone(&reg);
        let h1 = std::thread::spawn(move || reg1.rebuild(&IndexMap::new()).unwrap());
        let reg2 = Arc::clone(&reg);
        let h2 = std::thread::spawn(move || reg2.rebuild(&IndexMap::new()).unwrap());

        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();
        assert_ne!(
            r1, r2,
            "concurrent rebuilds must return different revisions"
        );
        assert_eq!(reg.snapshot().revision, 2, "final revision must be 2");
    }
}
