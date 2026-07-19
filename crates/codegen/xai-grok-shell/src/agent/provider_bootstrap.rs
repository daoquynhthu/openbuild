use std::sync::Arc;

use thiserror::Error;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory;
use xai_grok_provider::registry::{ProviderFactoryKind, ProviderRegistry};
use xai_grok_provider::resolution::{ResolvedProviderSet, resolve_with_precedence};

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
    #[error("config diagnostic: {0}")]
    ConfigDiagnostic(String),
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

/// Convenience: parse TOML config, resolve with precedence, then bootstrap runtime.
///
/// This is the single entry point the launcher should call.
/// It reads no env/session secrets — only TOML structure and CLI-provided overrides.
pub async fn bootstrap_from_config(
    raw_toml: &toml::Value,
    legacy_migration: Option<ProviderConfig>,
    cli_overrides: Option<ProviderConfig>,
) -> Result<Arc<ProviderRuntime>, ProviderBootstrapError> {
    let parsed = xai_grok_provider::config::parse_provider_toml(raw_toml).map_err(|diags| {
        ProviderBootstrapError::RebuildFailed(
            diags
                .into_iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;

    let (resolved, diags) = resolve_with_precedence(parsed, legacy_migration, cli_overrides);

    if !diags.is_empty() {
        let msg = diags
            .into_iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ProviderBootstrapError::ConfigDiagnostic(msg));
    }

    let input = ProviderBootstrapInput { resolved };
    bootstrap_provider_runtime(input).await
}
