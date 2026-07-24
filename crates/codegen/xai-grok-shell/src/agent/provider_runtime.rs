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
/// Preserves startup resolution context (F1) for hot reload.
#[derive(Debug)]
pub struct ProviderRuntime {
    pub registry: Arc<ProviderRegistry>,
    pub catalog: Arc<ProviderCatalogService>,
    pub startup_legacy_migration: RwLock<Option<ProviderConfig>>,
    pub startup_cli_overrides: RwLock<Option<ProviderConfig>>,
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
            startup_legacy_migration: RwLock::new(None),
            startup_cli_overrides: RwLock::new(None),
            config_revision: RwLock::new(0),
            cancel_token: token,
        }
    }

    /// Bootstrap catalog from persisted snapshot (F3a). Must be called after
    /// construction from an async context.
    pub async fn bootstrap_catalog(&self) {
        let persisted = provider_catalog::load_catalog_snapshot();
        if !persisted.providers.is_empty() {
            self.catalog.bootstrap_from_snapshot(persisted).await;
        }
    }

    /// Set the startup resolution context (F1: hot reload preserves CLI/legacy).
    pub async fn set_startup_context(
        &self,
        legacy_migration: Option<ProviderConfig>,
        cli_overrides: Option<ProviderConfig>,
    ) {
        let mut lm = self.startup_legacy_migration.write().await;
        *lm = legacy_migration;
        let mut co = self.startup_cli_overrides.write().await;
        *co = cli_overrides;
    }

    /// Register a provider definition.
    pub fn register(&self, provider: SharedProvider) {
        let _ = self.registry.register_definition(provider);
    }

    /// Current registry snapshot (F4: session consistency).
    ///
    /// Each call returns an `Arc` to the latest committed state. The snapshot
    /// is atomic and immutable: concurrent rebuilds replace the pointer without
    /// affecting already-held `Arc` references. This guarantees:
    ///   1. Each new inference request reads the current snapshot.
    ///   2. Within a single request, the snapshot is fixed (Arc immutability).
    ///   3. Model switching re-resolves by calling snapshot() again.
    ///   4. A hot reload during an in-flight request does NOT change the
    ///      snapshot used by that request — the old `Arc` remains valid.
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
