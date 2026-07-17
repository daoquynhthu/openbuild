use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use indexmap::IndexMap;
use tokio::sync::RwLock;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::provider::SharedProvider;
use xai_grok_provider::registry::{ProviderRegistry, RegistrySnapshot};
use xai_grok_provider::types::ProviderId;

use super::provider_catalog::{self, ProviderCatalogService};

/// Runtime container holding the provider registry and catalog service.
///
/// The launcher creates one instance and injects it into shell and pager.
/// Provides transactional rebuild and explicit refresh.
#[derive(Debug)]
pub struct ProviderRuntime {
    pub registry: Arc<ProviderRegistry>,
    pub catalog: Arc<ProviderCatalogService>,
    config_revision: RwLock<u64>,
    cancelled: AtomicBool,
}

impl ProviderRuntime {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ProviderRegistry::new()),
            catalog: Arc::new(ProviderCatalogService::new()),
            config_revision: RwLock::new(0),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Register a provider definition.
    pub fn register(&self, provider: SharedProvider) {
        let _ = self.registry.register_definition(provider);
    }

    /// Current registry snapshot.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        self.registry.snapshot()
    }

    /// Transactional rebuild: config → registry → catalog refresh.
    /// On failure, previous state is preserved.
    pub async fn rebuild(
        &self,
        configs: &IndexMap<ProviderId, ProviderConfig>,
    ) -> Result<u64, String> {
        let revision = self
            .registry
            .rebuild(configs)
            .map_err(|e| format!("rebuild failed: {e}"))?;
        let mut rev = self.config_revision.write().await;
        *rev = revision;
        Ok(revision)
    }

    /// Cancel outstanding catalog operations.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Current config revision.
    pub async fn config_revision(&self) -> u64 {
        *self.config_revision.read().await
    }
}

impl Default for ProviderRuntime {
    fn default() -> Self {
        Self::new()
    }
}
