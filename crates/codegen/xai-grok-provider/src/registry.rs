use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use indexmap::IndexMap;

use crate::config::ProviderConfig;
use crate::error::ProviderError;
use crate::provider::{ConfiguredProvider, SharedProvider};
use crate::route::Route;
use crate::types::{ProviderId, RouteId};

/// Immutable, atomic, revisioned view of all configured providers and routes.
#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub revision: u64,
    pub providers: IndexMap<ProviderId, Arc<ConfiguredProvider>>,
    pub routes: IndexMap<RouteId, Arc<Route>>,
}

/// Process-wide provider registry with transactional snapshot semantics.
///
/// V1 architecture (AD-04): State is published as atomic `Arc<RegistrySnapshot>`.
/// Readers always see a complete, immutable, revisioned view.
/// Writes validate fully before replacing the snapshot.
#[derive(Debug)]
pub struct ProviderRegistry {
    definitions: RwLock<IndexMap<ProviderId, SharedProvider>>,
    configs: RwLock<HashMap<ProviderId, ProviderConfig>>,
    snapshot: RwLock<Arc<RegistrySnapshot>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            definitions: RwLock::new(IndexMap::new()),
            configs: RwLock::new(HashMap::new()),
            snapshot: RwLock::new(Arc::new(RegistrySnapshot {
                revision: 0,
                providers: IndexMap::new(),
                routes: IndexMap::new(),
            })),
        }
    }

    /// Register a provider definition. Rejects duplicates.
    pub fn register_definition(&self, provider: SharedProvider) -> Result<(), ProviderError> {
        let id = provider.id().clone();
        let mut defs = self
            .definitions
            .write()
            .map_err(|_| ProviderError::Config("registry lock poisoned".into()))?;
        if defs.contains_key(&id) {
            return Err(ProviderError::DuplicateProvider(format!(
                "provider {} already registered",
                id.0
            )));
        }
        defs.insert(id, provider);
        Ok(())
    }

    /// Register a provider definition (legacy shorthand).
    pub fn register(&self, provider: SharedProvider) {
        let _ = self.register_definition(provider);
    }

    /// Get a registered provider definition.
    pub fn get(&self, id: &ProviderId) -> Option<SharedProvider> {
        self.definitions
            .read()
            .ok()
            .and_then(|defs| defs.get(id).cloned())
    }

    /// All registered provider IDs.
    pub fn all_ids(&self) -> Vec<ProviderId> {
        self.definitions
            .read()
            .ok()
            .map(|defs| defs.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Store a provider configuration.
    pub fn store_config(&self, id: &ProviderId, config: ProviderConfig) {
        if let Ok(mut configs) = self.configs.write() {
            configs.insert(id.clone(), config);
        }
    }

    /// Retrieve the last-stored configuration for a provider.
    pub fn get_config(&self, id: &ProviderId) -> Option<ProviderConfig> {
        self.configs
            .read()
            .ok()
            .and_then(|configs| configs.get(id).cloned())
    }

    /// Register a route (legacy, adds to a separate routes map).
    /// Prefer rebuilding via `rebuild()` for transactional semantics.
    pub fn register_route(&self, id: impl Into<String>, route: Route) {
        if let Ok(mut snap) = self.snapshot.write() {
            let mut new_snapshot = (**snap).clone();
            let rid = RouteId::new(id);
            new_snapshot.routes.insert(rid, Arc::new(route));
            *snap = Arc::new(new_snapshot);
        }
    }

    /// Get a previously registered route by key string (legacy).
    pub fn get_route(&self, id: &str) -> Option<Arc<Route>> {
        self.snapshot.read().ok().and_then(|snap| {
            snap.routes
                .iter()
                .find(|(k, _)| k.0 == id)
                .map(|(_, v)| v.clone())
        })
    }

    /// Configure a provider (legacy, returns a standalone ConfiguredProvider).
    pub fn configure(
        &self,
        id: &ProviderId,
        overrides: ProviderConfig,
    ) -> Option<ConfiguredProvider> {
        self.get(id).map(|p| p.configure(overrides))
    }

    /// Return the current immutable snapshot.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        self.snapshot.read().map(|s| s.clone()).unwrap_or_else(|_| {
            Arc::new(RegistrySnapshot {
                revision: 0,
                providers: IndexMap::new(),
                routes: IndexMap::new(),
            })
        })
    }

    /// Look up a configured provider from the current snapshot.
    pub fn configured(&self, id: &ProviderId) -> Option<Arc<ConfiguredProvider>> {
        self.snapshot
            .read()
            .ok()
            .and_then(|snap| snap.providers.get(id).cloned())
    }

    /// Look up a route from the current snapshot.
    pub fn route(&self, id: &RouteId) -> Option<Arc<Route>> {
        self.snapshot
            .read()
            .ok()
            .and_then(|snap| snap.routes.get(id).cloned())
    }

    /// Transactional rebuild from resolved configs.
    ///
    /// 1. Resolve all provider configs.
    /// 2. Configure and validate every provider into a new local snapshot.
    /// 3. Reject duplicate IDs and invalid routes.
    /// 4. Replace current `Arc<RegistrySnapshot>` only if all validation succeeds.
    /// 5. Increment revision exactly once per successful replacement.
    ///
    /// Failed rebuild leaves old snapshot and revision unchanged.
    pub fn rebuild(
        &self,
        configs: &IndexMap<ProviderId, ProviderConfig>,
    ) -> Result<u64, ProviderError> {
        let defs = self
            .definitions
            .read()
            .map_err(|_| ProviderError::Config("registry lock poisoned".into()))?;

        let mut new_providers: IndexMap<ProviderId, Arc<ConfiguredProvider>> = IndexMap::new();
        let mut new_routes: IndexMap<RouteId, Arc<Route>> = IndexMap::new();

        for (pid, provider) in defs.iter() {
            let overrides = configs.get(pid).cloned().unwrap_or_default();
            let configured = provider.configure(overrides);

            for (rid, route) in &configured.routes {
                if new_routes.contains_key(rid) {
                    return Err(ProviderError::DuplicateRoute(format!(
                        "duplicate route ID: {}",
                        rid.0
                    )));
                }
                route
                    .validate()
                    .map_err(|e| ProviderError::InvalidRouteId(format!("route {}: {e}", rid.0)))?;
                new_routes.insert(rid.clone(), route.clone());
            }

            new_providers.insert(pid.clone(), Arc::new(configured));
        }

        let current_revision = self
            .snapshot
            .read()
            .map_err(|_| ProviderError::Config("registry lock poisoned".into()))?
            .revision;

        let new_revision = current_revision + 1;
        let new_snapshot = Arc::new(RegistrySnapshot {
            revision: new_revision,
            providers: new_providers,
            routes: new_routes,
        });

        let mut snap = self
            .snapshot
            .write()
            .map_err(|_| ProviderError::Config("registry lock poisoned".into()))?;
        *snap = new_snapshot;

        Ok(new_revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthPolicy;
    use crate::config::ProviderConfig;
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider};
    use crate::types::ProviderDefaults;

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
            let rid = RouteId::new("dummy-route");
            let route = Route::make(
                "dummy-route",
                Some(self.id.clone()),
                "chat_completions",
                Endpoint {
                    base_url: Some("https://dummy.com/v1".into()),
                    path: EndpointPart::Static("/chat/completions".into()),
                    query: None,
                },
                AuthPolicy::None,
            );
            ConfiguredProvider::new(
                self.id.clone(),
                "Dummy".into(),
                ProviderConfig::default(),
                IndexMap::from([(rid.clone(), Arc::new(route))]),
                rid,
                Arc::new(DefaultRouteSelector {
                    default_route_id: RouteId::new("dummy-route"),
                }),
            )
        }
    }

    fn dummy_provider() -> SharedProvider {
        Arc::new(DummyProvider::new())
    }

    #[test]
    fn registry_register_definition_and_get() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        let p = registry.get(&ProviderId::new("dummy"));
        assert!(p.is_some());
    }

    #[test]
    fn registry_register_definition_rejects_duplicate() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        let result = registry.register_definition(dummy_provider());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::DuplicateProvider(_)
        ));
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let registry = ProviderRegistry::new();
        let p = registry.get(&ProviderId::new("unknown"));
        assert!(p.is_none());
    }

    #[test]
    fn registry_rebuild_increments_revision() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();

        let configs = IndexMap::new();
        let rev1 = registry.rebuild(&configs).unwrap();
        assert_eq!(rev1, 1);

        let rev2 = registry.rebuild(&configs).unwrap();
        assert_eq!(rev2, 2);
    }

    #[test]
    fn registry_snapshot_contains_providers_after_rebuild() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        registry.rebuild(&IndexMap::new()).unwrap();

        let snap = registry.snapshot();
        assert_eq!(snap.providers.len(), 1);
        assert!(snap.providers.contains_key(&ProviderId::new("dummy")));
        assert_eq!(snap.revision, 1);
    }

    #[test]
    fn registry_snapshot_order_is_deterministic() {
        let registry = ProviderRegistry::new();

        // Use providers with different route IDs to avoid DuplicateRoute
        #[derive(Debug)]
        struct ProviderA {
            pid: ProviderId,
        }
        impl ProviderA {
            fn new() -> Self {
                Self {
                    pid: ProviderId::new("a"),
                }
            }
        }
        impl Provider for ProviderA {
            fn id(&self) -> &ProviderId {
                &self.pid
            }
            fn name(&self) -> &str {
                "A"
            }
            fn defaults(&self) -> &ProviderDefaults {
                panic!("not called")
            }
            fn configure(&self, _: ProviderConfig) -> ConfiguredProvider {
                let rid = RouteId::new("route-a");
                let route = Route::make(
                    "route-a",
                    Some(ProviderId::new("a")),
                    "chat",
                    Endpoint {
                        base_url: Some("https://a.com".into()),
                        path: EndpointPart::Static("/chat".into()),
                        query: None,
                    },
                    AuthPolicy::None,
                );
                ConfiguredProvider::new(
                    self.pid.clone(),
                    "A".into(),
                    ProviderConfig::default(),
                    IndexMap::from([(rid.clone(), Arc::new(route))]),
                    rid,
                    Arc::new(DefaultRouteSelector {
                        default_route_id: RouteId::new("route-a"),
                    }),
                )
            }
        }

        #[derive(Debug)]
        struct ProviderB {
            pid: ProviderId,
        }
        impl ProviderB {
            fn new() -> Self {
                Self {
                    pid: ProviderId::new("b"),
                }
            }
        }
        impl Provider for ProviderB {
            fn id(&self) -> &ProviderId {
                &self.pid
            }
            fn name(&self) -> &str {
                "B"
            }
            fn defaults(&self) -> &ProviderDefaults {
                panic!("not called")
            }
            fn configure(&self, _: ProviderConfig) -> ConfiguredProvider {
                let rid = RouteId::new("route-b");
                let route = Route::make(
                    "route-b",
                    Some(ProviderId::new("b")),
                    "chat",
                    Endpoint {
                        base_url: Some("https://b.com".into()),
                        path: EndpointPart::Static("/chat".into()),
                        query: None,
                    },
                    AuthPolicy::None,
                );
                ConfiguredProvider::new(
                    self.pid.clone(),
                    "B".into(),
                    ProviderConfig::default(),
                    IndexMap::from([(rid.clone(), Arc::new(route))]),
                    rid,
                    Arc::new(DefaultRouteSelector {
                        default_route_id: RouteId::new("route-b"),
                    }),
                )
            }
        }

        registry
            .register_definition(Arc::new(ProviderA::new()))
            .unwrap();
        registry
            .register_definition(Arc::new(ProviderB::new()))
            .unwrap();

        registry.rebuild(&IndexMap::new()).unwrap();
        let snap1 = registry.snapshot();

        registry.rebuild(&IndexMap::new()).unwrap();
        let snap2 = registry.snapshot();

        let ids1: Vec<&String> = snap1.providers.keys().map(|k| &k.0).collect();
        let ids2: Vec<&String> = snap2.providers.keys().map(|k| &k.0).collect();
        assert_eq!(ids1, ids2, "provider order must be deterministic");
    }

    #[test]
    fn registry_configured_returns_provider() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        registry.rebuild(&IndexMap::new()).unwrap();

        let cp = registry.configured(&ProviderId::new("dummy"));
        assert!(cp.is_some());
        assert_eq!(cp.unwrap().display_name, "Dummy");
    }

    #[test]
    fn registry_route_lookup() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        registry.rebuild(&IndexMap::new()).unwrap();

        let route = registry.route(&RouteId::new("dummy-route"));
        assert!(route.is_some());
        assert_eq!(route.unwrap().protocol_id, "chat_completions");
    }

    #[test]
    fn registry_failed_rebuild_does_not_change_snapshot() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        registry.rebuild(&IndexMap::new()).unwrap();
        let snap_before = registry.snapshot();

        // Inject a provider that will fail validation
        #[derive(Debug)]
        struct BadProvider {
            pid: ProviderId,
        }
        impl BadProvider {
            fn new() -> Self {
                Self {
                    pid: ProviderId::new("bad"),
                }
            }
        }
        impl Provider for BadProvider {
            fn id(&self) -> &ProviderId {
                &self.pid
            }
            fn name(&self) -> &str {
                "Bad"
            }
            fn defaults(&self) -> &ProviderDefaults {
                panic!("not called")
            }
            fn configure(&self, _: ProviderConfig) -> ConfiguredProvider {
                let rid = RouteId::new("bad-route");
                let route = Route::make(
                    "bad-route",
                    Some(self.pid.clone()),
                    "",
                    Endpoint {
                        base_url: Some("https://bad.com".into()),
                        path: EndpointPart::Static("/bad".into()),
                        query: None,
                    },
                    AuthPolicy::None,
                );
                ConfiguredProvider::new(
                    self.pid.clone(),
                    "Bad".into(),
                    ProviderConfig::default(),
                    IndexMap::from([(rid.clone(), Arc::new(route))]),
                    rid,
                    Arc::new(DefaultRouteSelector {
                        default_route_id: RouteId::new("bad-route"),
                    }),
                )
            }
        }

        // Rebuild with the bad provider also in definitions
        registry
            .register_definition(Arc::new(BadProvider::new()))
            .unwrap();
        let result = registry.rebuild(&IndexMap::new());
        assert!(result.is_err(), "rebuild must fail when a route is invalid");

        // The old snapshot must still be intact
        let snap_after = registry.snapshot();
        assert_eq!(snap_after.revision, 1);
        assert_eq!(snap_before.revision, snap_after.revision);
    }

    #[test]
    fn store_config_and_get_config() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
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
        registry.register_definition(dummy_provider()).unwrap();
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

    #[test]
    fn registry_register_route_and_get() {
        let registry = ProviderRegistry::new();
        let route = Route::make(
            "test-route",
            None::<ProviderId>,
            "chat",
            Endpoint {
                base_url: None,
                path: EndpointPart::Static("/test".into()),
                query: None,
            },
            AuthPolicy::None,
        );
        registry.register_route("my-route", route);
        let r = registry.get_route("my-route");
        assert!(r.is_some());
    }

    #[test]
    fn registry_all_ids() {
        let registry = ProviderRegistry::new();
        registry.register_definition(dummy_provider()).unwrap();
        let ids = registry.all_ids();
        assert_eq!(ids.len(), 1);
    }
}
