use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;

use super::provider_runtime::ProviderRuntime;

/// Frozen resolution context captured at startup: legacy migration policy
/// and CLI overrides.  Excludes env/session secret values.
pub struct ProviderResolutionContext {
    pub legacy_migration: Option<ProviderConfig>,
    pub cli_overrides: Option<ProviderConfig>,
}

/// Outcome of a config apply operation.
pub enum ConfigApplyOutcome {
    Applied { new_revision: u64 },
    Unchanged,
}

/// Error during config application.
#[derive(Debug, thiserror::Error)]
pub enum ConfigApplyError {
    #[error("failed to read config file: {0}")]
    FileReadError(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("config diagnostics: {0}")]
    ResolveError(String),
    #[error("prepare failed: {0}")]
    PrepareError(String),
    #[error("commit failed: {0}")]
    CommitError(String),
    #[error("file changed since read — not overwriting")]
    FileChanged,
    #[error("failed to write: {0}")]
    WriteError(String),
}

/// Single coordination point for all provider config writes.
///
/// Every config-driven registry commit must pass through this struct
/// so that file I/O, resolution, prepare/commit, and catalog refresh
/// are serialized under the same `update_lock`.
pub struct ProviderConfigCoordinator {
    pub runtime: Arc<ProviderRuntime>,
    pub config_path: PathBuf,
    pub resolution_context: Arc<ProviderResolutionContext>,
    update_lock: Mutex<()>,
}

impl ProviderConfigCoordinator {
    pub fn new(
        runtime: Arc<ProviderRuntime>,
        config_path: PathBuf,
        resolution_context: Arc<ProviderResolutionContext>,
    ) -> Self {
        Self {
            runtime,
            config_path,
            resolution_context,
            update_lock: Mutex::new(()),
        }
    }

    /// Read the config file, parse, resolve, prepare, and commit.
    ///
    /// Does NOT write back to the file (read-only).
    pub async fn apply_external_file(&self) -> Result<ConfigApplyOutcome, ConfigApplyError> {
        let _lock = self.update_lock.lock().await;

        let raw_toml = crate::config::load_effective_config()
            .map_err(|e| ConfigApplyError::FileReadError(e.to_string()))?;

        let parsed = xai_grok_provider::config::parse_provider_toml(&raw_toml)
            .map_err(|diags| {
                ConfigApplyError::ParseError(
                    diags.into_iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; "),
                )
            })?;

        let (resolved, diags) = xai_grok_provider::resolution::resolve_with_precedence(
            parsed,
            self.resolution_context.legacy_migration.clone(),
            self.resolution_context.cli_overrides.clone(),
        );

        if !diags.is_empty() {
            let msg = diags.into_iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; ");
            return Err(ConfigApplyError::ResolveError(msg));
        }

        let revision = self
            .runtime
            .registry
            .rebuild_from_resolved(&resolved)
            .map_err(|e| ConfigApplyError::CommitError(e.to_string()))?;

        Ok(ConfigApplyOutcome::Applied { new_revision: revision })
    }

    pub fn registry(&self) -> Arc<ProviderRegistry> {
        Arc::clone(&self.runtime.registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn coordinator_uses_same_runtime_arc() {
        let rt = Arc::new(ProviderRuntime::new());
        let config_path = PathBuf::from("/tmp/test-config.toml");
        let ctx = Arc::new(ProviderResolutionContext {
            legacy_migration: None,
            cli_overrides: None,
        });
        let coord = ProviderConfigCoordinator::new(Arc::clone(&rt), config_path, ctx);
        assert!(Arc::ptr_eq(&rt, &coord.runtime));
    }
}
