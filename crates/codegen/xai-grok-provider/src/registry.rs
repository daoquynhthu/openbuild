use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::config::ProviderConfig;
use crate::error::ProviderError;
use crate::provider::{ConfiguredProvider, SharedProvider};
use crate::resolution::{ProviderImplementation, ResolvedProviderSet};
use crate::route::Route;
use crate::types::{ProviderId, RouteId};

/// Result of `prepare()` — a validated snapshot ready for atomic commit.
#[derive(Debug)]
pub struct PreparedRegistrySnapshot {
    pub base_revision: u64,
    pub snapshot: Arc<RegistrySnapshot>,
}

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
    /// Legacy config store — only accessed through test-only methods after P5-007.
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

    // ── Legacy API — no production callers after P5-007 ──

    #[doc(hidden)]
    pub fn store_config(&self, id: &ProviderId, config: ProviderConfig) {
        self.state.write().legacy_configs.insert(id.clone(), config);
    }

    #[doc(hidden)]
    pub fn get_config(&self, id: &ProviderId) -> Option<ProviderConfig> {
        let state = self.state.read();
        state
            .legacy_configs
            .get(id)
            .cloned()
            .or_else(|| state.snapshot.providers.get(id).map(|cp| cp.config.clone()))
    }

    #[doc(hidden)]
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

    /// Prepare a new snapshot from a resolved provider set without publishing.
    /// Returns `Err` if any provider/route/selector validation fails.
    /// The current snapshot is never modified.
    /// Prepare a new snapshot from a resolved provider set without publishing.
    /// Returns `Err` if any provider/route/selector validation fails.
    /// The current snapshot is never modified.
    ///
    /// Lock discipline: only a short read lock to clone definitions/factories/revision;
    /// all provider creation and validation happens outside the lock.
    pub fn prepare(
        &self,
        resolved: &ResolvedProviderSet,
    ) -> Result<PreparedRegistrySnapshot, ProviderError> {
        let (definitions, factories, base_revision) = {
            let state = self.state.read();
            (
                state.definitions.clone(),
                state.factories.clone(),
                state.snapshot.revision,
            )
        };

        let mut new_providers: IndexMap<ProviderId, Arc<ConfiguredProvider>> = IndexMap::new();
        let mut new_routes: IndexMap<ProviderRouteKey, Arc<Route>> = IndexMap::new();

        for (pid, spec) in &resolved.providers {
            if &spec.id != pid {
                return Err(ProviderError::Config(format!(
                    "spec ID `{}` does not match provider key `{}`",
                    spec.id.0, pid.0
                )));
            }
            let configured = match &spec.implementation {
                ProviderImplementation::Builtin { definition_id } => {
                    let provider = definitions.get(definition_id).ok_or_else(|| {
                        ProviderError::Config(format!(
                            "built-in definition `{}` not registered",
                            definition_id.0
                        ))
                    })?;
                    let model_list_format = spec.config.public.model_list_format.map(|f| match f {
                        crate::types::ModelListFormat::OpenAiCompatible => "openai_compatible".to_string(),
                        crate::types::ModelListFormat::OllamaTags => "ollama_tags".to_string(),
                    });
                    let overrides = ProviderConfig {
                        id: Some(spec.id.0.clone()),
                        base_url: spec.config.public.base_url.clone(),
                        protocol: spec.config.public.protocol.clone(),
                        model_list_path: spec.config.public.model_list_path.clone(),
                        model_list_format,
                        env_key: if spec.config.env_keys.is_empty() {
                            None
                        } else {
                            Some(spec.config.env_keys.clone())
                        },
                        extra_headers: if spec.config.public.extra_headers.is_empty() {
                            None
                        } else {
                            Some(spec.config.public.extra_headers.clone())
                        },
                        allow_insecure_http: if spec.config.public.allow_insecure_http {
                            Some(true)
                        } else {
                            None
                        },
                        ..Default::default()
                    };
                    provider.configure(overrides)
                }
                ProviderImplementation::OpenAiCompatible { .. } => {
                    let factory = factories
                        .get(&ProviderFactoryKind::OpenAiCompatible)
                        .ok_or_else(|| {
                            ProviderError::Config("OpenAiCompatible factory not registered".into())
                        })?;
                    let provider = factory.create(spec)?;
                    let model_list_format = spec.config.public.model_list_format.map(|f| match f {
                        crate::types::ModelListFormat::OpenAiCompatible => "openai_compatible".to_string(),
                        crate::types::ModelListFormat::OllamaTags => "ollama_tags".to_string(),
                    });
                    let overrides = ProviderConfig {
                        id: Some(spec.id.0.clone()),
                        base_url: spec.config.public.base_url.clone(),
                        protocol: spec.config.public.protocol.clone(),
                        model_list_path: spec.config.public.model_list_path.clone(),
                        model_list_format,
                        env_key: if spec.config.env_keys.is_empty() {
                            None
                        } else {
                            Some(spec.config.env_keys.clone())
                        },
                        extra_headers: if spec.config.public.extra_headers.is_empty() {
                            None
                        } else {
                            Some(spec.config.public.extra_headers.clone())
                        },
                        allow_insecure_http: if spec.config.public.allow_insecure_http {
                            Some(true)
                        } else {
                            None
                        },
                        ..Default::default()
                    };
                    provider.configure(overrides)
                }
            };

            // Validate that all referenced routes by the selector exist
            let referenced = configured.route_selector.referenced_route_ids();
            for rid in referenced {
                if !configured.routes.contains_key(rid) {
                    return Err(ProviderError::Config(format!(
                        "selector for `{}` references route `{}` which is not defined",
                        pid.0, rid.0
                    )));
                }
            }

            // Validate each route
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

        let new_snapshot = Arc::new(RegistrySnapshot {
            revision: base_revision,
            providers: new_providers,
            routes: new_routes,
        });

        Ok(PreparedRegistrySnapshot {
            base_revision,
            snapshot: new_snapshot,
        })
    }

    /// Atomically commit a prepared snapshot if no concurrent change occurred.
    pub fn commit(&self, prepared: PreparedRegistrySnapshot) -> Result<u64, ProviderError> {
        let mut state = self.state.write();
        if state.snapshot.revision != prepared.base_revision {
            return Err(ProviderError::Config(format!(
                "revision conflict: expected {}, got {}",
                prepared.base_revision, state.snapshot.revision
            )));
        }
        let new_revision = prepared.base_revision + 1;
        let mut new_snapshot = (*prepared.snapshot).clone();
        new_snapshot.revision = new_revision;
        state.snapshot = Arc::new(new_snapshot);
        state.sealed = true;
        Ok(new_revision)
    }

    /// Convenience: prepare + commit.
    pub fn rebuild_from_resolved(
        &self,
        resolved: &ResolvedProviderSet,
    ) -> Result<u64, ProviderError> {
        let prepared = self.prepare(resolved)?;
        self.commit(prepared)
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
    use crate::resolution::ResolvedProviderSpec;
    use crate::route::Route;
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
    fn prepare_builtin_with_unknown_definition_fails() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("missing"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("missing"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("does-not-exist"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        let result = reg.prepare(&resolved);
        assert!(result.is_err(), "unknown definition must fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("does-not-exist"),
            "error should mention the missing definition: {err}"
        );
    }

    #[test]
    fn prepare_openai_compatible_without_factory_fails() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("custom"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("custom"),
                    implementation: ProviderImplementation::OpenAiCompatible { profile: None },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: Some("https://api.example.com/v1".into()),
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        let result = reg.prepare(&resolved);
        assert!(result.is_err(), "missing factory must fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("factory"),
            "error should mention factory: {err}"
        );
    }

    #[test]
    fn prepare_failure_leaves_snapshot_unchanged() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();

        // First, successfully build revision 1
        let resolved_ok = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("dummy"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("dummy"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("dummy"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        reg.rebuild_from_resolved(&resolved_ok).unwrap();
        assert_eq!(reg.snapshot().revision, 1);

        // Now try to prepare a set with unknown definition — must fail
        let resolved_bad = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("ghost"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("ghost"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("does-not-exist"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        assert!(
            reg.prepare(&resolved_bad).is_err(),
            "prepare with unknown def must fail"
        );
        assert_eq!(
            reg.snapshot().revision,
            1,
            "revision must not change after failed prepare"
        );
        assert_eq!(
            reg.snapshot().providers.len(),
            1,
            "providers must not change after failed prepare"
        );
    }

    #[test]
    fn prepare_failure_on_unknown_definition_keeps_snapshot_ptr() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        let resolved_ok = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("dummy"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("dummy"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("dummy"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        let _old_snapshot = reg.snapshot();
        reg.rebuild_from_resolved(&resolved_ok).unwrap();
        let current = reg.snapshot();

        // Prepare failure should not change Arc pointer
        let resolved_bad = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("ghost"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("ghost"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("nope"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        assert!(reg.prepare(&resolved_bad).is_err());
        assert!(
            Arc::ptr_eq(&current, &reg.snapshot()),
            "Arc pointer must be unchanged after failed prepare"
        );
    }

    #[test]
    fn commit_increments_revision() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::new(),
        };
        let prepared = reg.prepare(&resolved).unwrap();
        let rev = reg.commit(prepared).unwrap();
        assert_eq!(rev, 1);
        assert_eq!(reg.snapshot().revision, 1);
    }

    #[test]
    fn commit_rejects_stale_prepared() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::new(),
        };
        let first = reg.prepare(&resolved).unwrap();

        // Commit a *second* prepared snapshot first (to advance revision)
        let resolved2 = ResolvedProviderSet {
            providers: IndexMap::new(),
        };
        let second = reg.prepare(&resolved2).unwrap();
        reg.commit(second).unwrap();
        assert_eq!(reg.snapshot().revision, 1);

        // Now try to commit the first (stale) prepared snapshot
        let result = reg.commit(first);
        assert!(result.is_err(), "stale prepared must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("conflict"),
            "stale error should mention conflict: {err}"
        );
        assert_eq!(
            reg.snapshot().revision,
            1,
            "revision unchanged after stale commit"
        );
    }

    #[test]
    fn commit_rejects_double_commit_of_same_prepared() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::new(),
        };
        let prepared = reg.prepare(&resolved).unwrap();

        // First commit succeeds
        reg.commit(prepared).unwrap();
        assert_eq!(reg.snapshot().revision, 1);

        // Can't hold a second reference to prepared — it was consumed.
        // Double-commit rejection is covered by stale test above.
    }

    #[test]
    fn rebuild_from_resolved_creates_snapshot() {
        let reg = ProviderRegistry::new();
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();

        // Register factory
        let factory =
            Arc::new(crate::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory);
        reg.register_factory(ProviderFactoryKind::OpenAiCompatible, factory)
            .unwrap();

        // Build resolved set with builtin dummy + custom compatible
        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([
                (
                    ProviderId::new("dummy"),
                    crate::resolution::ResolvedProviderSpec {
                        id: ProviderId::new("dummy"),
                        implementation: ProviderImplementation::Builtin {
                            definition_id: ProviderId::new("dummy"),
                        },
                        config: crate::resolution::ProviderRuntimeConfig {
                            public: crate::resolution::ProviderPublicConfig {
                                base_url: None,
                                protocol: None,
                                model_list_path: None,
                                allow_insecure_http: false,
                                model_list_format: None,
                                extra_headers: IndexMap::new(),
                            },
                            inline_api_key: None,
                        env_keys: vec![],
                        },
                    },
                ),
                (
                    ProviderId::new("custom"),
                    crate::resolution::ResolvedProviderSpec {
                        id: ProviderId::new("custom"),
                        implementation: ProviderImplementation::OpenAiCompatible { profile: None },
                        config: crate::resolution::ProviderRuntimeConfig {
                            public: crate::resolution::ProviderPublicConfig {
                                base_url: Some("https://custom.api/v1".into()),
                                protocol: None,
                                model_list_path: None,
                                allow_insecure_http: false,
                                model_list_format: None,
                                extra_headers: IndexMap::new(),
                            },
                            inline_api_key: None,
                        env_keys: vec![],
                        },
                    },
                ),
            ]),
        };

        let rev = reg.rebuild_from_resolved(&resolved).unwrap();
        assert_eq!(rev, 1);
        assert_eq!(reg.snapshot().revision, 1);
        assert_eq!(reg.snapshot().providers.len(), 2);
    }

    #[test]
    fn sealed_registry_hot_add_custom_identity() {
        // Per plan P5-011: first rebuild only deepseek, then add internal, then remove deepseek.
        let reg = ProviderRegistry::new();
        let factory =
            Arc::new(crate::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory);
        reg.register_factory(ProviderFactoryKind::OpenAiCompatible, factory)
            .unwrap();

        fn deepseek_spec() -> crate::resolution::ResolvedProviderSpec {
            crate::resolution::ResolvedProviderSpec {
                id: ProviderId::new("deepseek"),
                implementation: ProviderImplementation::OpenAiCompatible {
                    profile: Some(crate::types::CompatibleProfileId::new("deepseek")),
                },
                config: crate::resolution::ProviderRuntimeConfig {
                    public: crate::resolution::ProviderPublicConfig {
                        base_url: None,
                        protocol: None,
                        model_list_path: None,
                        allow_insecure_http: false,
                        model_list_format: None,
                        extra_headers: IndexMap::new(),
                    },
                    inline_api_key: None,
                    env_keys: vec![],
                },
            }
        }

        fn internal_spec() -> crate::resolution::ResolvedProviderSpec {
            crate::resolution::ResolvedProviderSpec {
                id: ProviderId::new("internal"),
                implementation: ProviderImplementation::OpenAiCompatible {
                    profile: Some(crate::types::CompatibleProfileId::new("internal")),
                },
                config: crate::resolution::ProviderRuntimeConfig {
                    public: crate::resolution::ProviderPublicConfig {
                        base_url: Some("https://internal.api/v1".into()),
                        protocol: None,
                        model_list_path: None,
                        allow_insecure_http: false,
                        model_list_format: None,
                        extra_headers: IndexMap::new(),
                    },
                    inline_api_key: None,
                    env_keys: vec![],
                },
            }
        }

        // Step 1: rebuild with only deepseek → sealed, revision=1
        let set1 = ResolvedProviderSet {
            providers: IndexMap::from([(ProviderId::new("deepseek"), deepseek_spec())]),
        };
        let r1 = reg.rebuild_from_resolved(&set1).unwrap();
        assert_eq!(r1, 1);
        assert_eq!(reg.snapshot().providers.len(), 1);
        assert!(
            reg.snapshot()
                .providers
                .contains_key(&ProviderId::new("deepseek"))
        );

        // Step 2: add internal alongside deepseek
        let set2 = ResolvedProviderSet {
            providers: IndexMap::from([
                (ProviderId::new("deepseek"), deepseek_spec()),
                (ProviderId::new("internal"), internal_spec()),
            ]),
        };
        let r2 = reg.rebuild_from_resolved(&set2).unwrap();
        assert_eq!(r2, 2, "revision must increment on hot add");
        assert_eq!(reg.snapshot().providers.len(), 2);
        assert!(
            reg.snapshot()
                .providers
                .contains_key(&ProviderId::new("deepseek"))
        );
        assert!(
            reg.snapshot()
                .providers
                .contains_key(&ProviderId::new("internal"))
        );

        // Verify identity isolation: each identity's route is independent.
        let snap = reg.snapshot();
        let ds_has_route = snap
            .routes
            .iter()
            .any(|(k, _)| k.provider_id.0 == "deepseek");
        assert!(ds_has_route, "deepseek must have a route");
        let internal_has_route = snap
            .routes
            .iter()
            .any(|(k, _)| k.provider_id.0 == "internal");
        assert!(internal_has_route, "internal must have a route");
        // deepseek uses profile default (api.deepseek.com), internal uses explicit URL.
        // Check that they're different endpoint URLs.
        let ds_url = snap
            .routes
            .get(&ProviderRouteKey {
                provider_id: ProviderId::new("deepseek"),
                local_route_id: RouteId::new("deepseek-chat"),
            })
            .map(|r| r.endpoint.base_url.clone());
        let internal_url = snap
            .routes
            .get(&ProviderRouteKey {
                provider_id: ProviderId::new("internal"),
                local_route_id: RouteId::new("internal-chat"),
            })
            .map(|r| r.endpoint.base_url.clone());
        assert!(
            ds_url.is_some() && ds_url.as_ref().unwrap().is_some(),
            "deepseek must have a profile-based base_url"
        );
        assert!(
            ds_url.as_ref().unwrap().as_deref() != internal_url.as_ref().unwrap().as_deref(),
            "deepseek and internal must have different base_urls"
        );
        assert_eq!(
            internal_url.as_ref().unwrap().as_deref(),
            Some("https://internal.api/v1"),
            "internal must use its explicit base_url"
        );

        // Step 3: remove deepseek, keep internal
        let set3 = ResolvedProviderSet {
            providers: IndexMap::from([(ProviderId::new("internal"), internal_spec())]),
        };
        let r3 = reg.rebuild_from_resolved(&set3).unwrap();
        assert_eq!(r3, 3, "revision must increment on remove");
        assert_eq!(reg.snapshot().providers.len(), 1);
        assert!(
            reg.snapshot()
                .providers
                .contains_key(&ProviderId::new("internal")),
            "internal must survive after deepseek removal"
        );
        assert!(
            !reg.snapshot()
                .providers
                .contains_key(&ProviderId::new("deepseek")),
            "deepseek must have been removed"
        );
    }

    #[test]
    fn prepare_rejects_spec_id_mismatch() {
        let reg = ProviderRegistry::new();
        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("key-foo"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("spec-bar"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("dummy"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };
        let result = reg.prepare(&resolved);
        assert!(result.is_err(), "spec ID mismatch must fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("key-foo") && err.contains("spec-bar"),
            "error should mention both IDs: {err}"
        );
    }

    #[test]
    fn slow_factory_does_not_block_snapshot() {
        let reg = Arc::new(ProviderRegistry::new());
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();

        // Register a slow factory that simulates a 100ms provider creation
        #[derive(Debug)]
        struct SlowFactory;
        impl crate::providers::openai_compatible_factory::ProviderFactory for SlowFactory {
            fn create(&self, spec: &ResolvedProviderSpec) -> Result<SharedProvider, ProviderError> {
                std::thread::sleep(std::time::Duration::from_millis(100));
                Ok(Arc::new(DummyProvider {
                    id: spec.id.clone(),
                    ..DummyProvider::new()
                }))
            }
        }

        let factory: crate::providers::openai_compatible_factory::SharedProviderFactory =
            Arc::new(SlowFactory);
        reg.register_factory(ProviderFactoryKind::OpenAiCompatible, factory)
            .unwrap();

        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("slow"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("slow"),
                    implementation: ProviderImplementation::OpenAiCompatible { profile: None },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: Some("https://slow.api/v1".into()),
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };

        // Spawn prepare on one thread (will block 100ms in factory)
        let reg_clone = Arc::clone(&reg);
        let handle = std::thread::spawn(move || reg_clone.prepare(&resolved));

        // snapshot() must return immediately while prepare is still running
        let start = std::time::Instant::now();
        let snap = reg.snapshot();
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "snapshot() blocked for {elapsed:?} — slow factory held read lock"
        );
        assert_eq!(snap.revision, 0);

        // Wait for prepare to complete
        let prepared = handle.join().unwrap().unwrap();
        assert_eq!(prepared.base_revision, 0);
    }

    #[test]
    fn concurrent_prepare_does_not_hold_write_lock() {
        let reg = Arc::new(ProviderRegistry::new());
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();

        // First prepare to get a snapshot
        let resolved = ResolvedProviderSet {
            providers: IndexMap::new(),
        };
        let prepared = reg.prepare(&resolved).unwrap();
        reg.commit(prepared).unwrap();

        // Spawn a second prepare on one thread
        let reg_clone = Arc::clone(&reg);
        let handle = std::thread::spawn(move || reg_clone.prepare(&resolved));

        // snapshot() must NOT be blocked — prepare only holds read lock
        let start = std::time::Instant::now();
        let snap = reg.snapshot();
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "snapshot() blocked for {elapsed:?} — concurrent prepare held write lock"
        );
        assert_eq!(snap.revision, 1);

        let prepared2 = handle.join().unwrap().unwrap();
        assert_eq!(prepared2.base_revision, 1);
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

    #[test]
    fn concurrent_rebuild_from_resolved_gives_sequential_revisions() {
        let reg = Arc::new(ProviderRegistry::new());
        reg.register_definition(Arc::new(DummyProvider::new()))
            .unwrap();
        let factory =
            Arc::new(crate::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory);
        reg.register_factory(ProviderFactoryKind::OpenAiCompatible, factory)
            .unwrap();

        let resolved = ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("dummy"),
                crate::resolution::ResolvedProviderSpec {
                    id: ProviderId::new("dummy"),
                    implementation: ProviderImplementation::Builtin {
                        definition_id: ProviderId::new("dummy"),
                    },
                    config: crate::resolution::ProviderRuntimeConfig {
                        public: crate::resolution::ProviderPublicConfig {
                            base_url: None,
                            protocol: None,
                            model_list_path: None,
                            allow_insecure_http: false,
                            model_list_format: None,
                            extra_headers: IndexMap::new(),
                        },
                        inline_api_key: None,
                        env_keys: vec![],
                    },
                },
            )]),
        };

        let resolved1 = resolved.clone();
        let reg1 = Arc::clone(&reg);
        let h1 = std::thread::spawn(move || reg1.rebuild_from_resolved(&resolved1).unwrap());
        let resolved2 = resolved.clone();
        let reg2 = Arc::clone(&reg);
        let h2 = std::thread::spawn(move || reg2.rebuild_from_resolved(&resolved2).unwrap());

        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();
        assert_ne!(
            r1, r2,
            "concurrent rebuild_from_resolved must return different revisions"
        );
        assert_eq!(reg.snapshot().revision, 2, "final revision must be 2");
    }
}
