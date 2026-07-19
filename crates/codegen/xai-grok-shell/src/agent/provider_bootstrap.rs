use std::sync::Arc;

use thiserror::Error;
use xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory;
use xai_grok_provider::registry::{ProviderFactoryKind, ProviderRegistry};
use xai_grok_provider::resolution::ResolvedProviderSet;

use super::provider_runtime::ProviderRuntime;

/// Input to [`bootstrap_provider_runtime`].
pub struct ProviderBootstrapInput {
    pub resolved: ResolvedProviderSet,
}

/// Errors during provider runtime bootstrap.
#[derive(Debug, Error)]
pub enum ProviderBootstrapError {
    #[error("registry rebuild failed: {0}")]
    RebuildFailed(String),
}

/// Construct the single process-wide [`ProviderRuntime`].
///
/// 1. Registers all built-in provider definitions and the generic
///    `OpenAiCompatibleProviderFactory`.
/// 2. Calls `Registry::rebuild_from_resolved` to atomically publish the
///    resolved provider set.
///
/// The launcher must call this **once** after the precedence resolver
/// produces a `ResolvedProviderSet`. No TOML, env, or CLI config is read
/// inside this function.
pub async fn bootstrap_provider_runtime(
    input: ProviderBootstrapInput,
) -> Result<Arc<ProviderRuntime>, ProviderBootstrapError> {
    let runtime = Arc::new(ProviderRuntime::new());

    // Register built-in definitions.
    xai_grok_provider::providers::register_all(&runtime.registry);

    // Register the generic factory for OpenAiCompatible identities.
    runtime
        .registry
        .register_factory(
            ProviderFactoryKind::OpenAiCompatible,
            Arc::new(OpenAiCompatibleProviderFactory),
        )
        .map_err(|e| ProviderBootstrapError::RebuildFailed(e.to_string()))?;

    // Atomic publish.
    runtime
        .registry
        .rebuild_from_resolved(&input.resolved)
        .map_err(|e| ProviderBootstrapError::RebuildFailed(e.to_string()))?;

    Ok(runtime)
}
