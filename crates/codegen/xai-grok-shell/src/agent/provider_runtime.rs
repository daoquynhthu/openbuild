use std::sync::Arc;

use indexmap::IndexMap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::provider::SharedProvider;
use xai_grok_provider::registry::{ProviderRegistry, RegistrySnapshot};
use xai_grok_provider::types::ProviderId;

use super::provider_catalog::{self, CatalogShutdownError, ProviderCatalogService};

/// Runtime container holding the provider registry and catalog service.
///
/// The launcher creates one instance and injects it into shell and pager.
/// Provides transactional rebuild and explicit refresh.
/// Shares a CancellationToken with the catalog for coordinated shutdown (P9-009).
/// Exposes catalog revision events for model view rebuild (P9-014).
#[derive(Debug)]
pub struct ProviderRuntime {
    pub registry: Arc<ProviderRegistry>,
    pub catalog: Arc<ProviderCatalogService>,
    config_revision: RwLock<u64>,
    cancel_token: CancellationToken,
}

impl ProviderRuntime {
    /// Create a new runtime with a fresh cancellation token shared with the catalog.
    pub fn new() -> Self {
        let token = CancellationToken::new();
        let catalog = Arc::new(ProviderCatalogService::with_client_and_token(
            ProviderCatalogService::build_http_client(),
            token.child_token(),
            ProviderCatalogService::DEFAULT_CONCURRENCY,
        ));
        Self {
            registry: Arc::new(ProviderRegistry::new()),
            catalog,
            config_revision: RwLock::new(0),
            cancel_token: token,
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

    /// Cancel the shared cancellation token (P9-009).
    /// All spawned catalog tasks will see cancellation at their next check point.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Cancel and wait for catalog tasks with a 2-second deadline.
    pub async fn shutdown(&self) -> Result<(), CatalogShutdownError> {
        self.cancel_token.cancel();
        self.catalog.shutdown().await
    }

    /// Current config revision.
    pub async fn config_revision(&self) -> u64 {
        *self.config_revision.read().await
    }

    /// Subscribe to catalog revision changes (P9-014).
    ///
    /// Each catalog refresh that increments the revision sends the new value
    /// through this watch. Consumers should rebuild the model view on change
    /// without making direct network requests.
    pub fn subscribe_catalog_revision(&self) -> tokio::sync::watch::Receiver<u64> {
        self.catalog.subscribe_catalog_revision()
    }
}

impl Default for ProviderRuntime {
    fn default() -> Self {
        Self::new()
    }
}
