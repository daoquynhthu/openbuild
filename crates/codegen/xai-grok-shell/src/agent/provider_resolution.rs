use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::auth::{AuthPolicy, apply_auth_policy};
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_provider::route::Route;
use xai_grok_provider::types::{ProviderId, RouteId};
use xai_grok_sampler::SamplerConfig;
use xai_grok_sampling_types::ApiBackend;

use super::config::ModelEntry;
use super::provider_catalog::ModelCatalogSnapshot;

/// Error returned by the route compiler.
#[derive(Debug, thiserror::Error)]
pub enum ProviderResolutionError {
    #[error("model has no provider_id: {0}")]
    NoProviderId(String),

    #[error("provider {0} not found in registry snapshot")]
    ProviderNotFound(String),

    #[error("route selection failed for model {model}: {detail}")]
    RouteSelection { model: String, detail: String },

    #[error("default route {0} not found in provider's route set")]
    DefaultRouteNotFound(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("ambiguous model reference: {0}")]
    AmbiguousModel(String),

    #[error("conflicting provider: --provider {provider} conflicts with --model {model}")]
    ConflictingProvider { provider: String, model: String },

    #[error("bare model '{0}' not found in any provider's catalog")]
    ModelNotFound(String),
}

/// Resolve a `SamplerConfig` from a `ModelEntry`, registry snapshot, and credentials.
///
/// This is the only provider-aware constructor of `SamplerConfig`. It applies:
/// 1. explicit model binding;
/// 2. provider route selector;
/// 3. route endpoint and protocol;
/// 4. model/route/provider generation defaults;
/// 5. header merge;
/// 6. credential resolution.
pub fn resolve_model_execution(
    model: &ModelEntry,
    registry: &RegistrySnapshot,
    api_key: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<SamplerConfig, ProviderResolutionError> {
    let provider_id_str = model
        .provider_id
        .as_deref()
        .ok_or_else(|| ProviderResolutionError::NoProviderId(model.info.model.clone()))?;
    let provider_id = ProviderId::new(provider_id_str);

    let configured = registry
        .providers
        .get(&provider_id)
        .ok_or_else(|| ProviderResolutionError::ProviderNotFound(provider_id_str.to_string()))?;

    // Resolve route via selector or explicit route_id
    let route_id = if let Some(ref rid) = model.route_id {
        rid.clone()
    } else {
        configured
            .route_selector
            .select(&model.info.model)
            .map(|rid| rid.0.clone())
            .map_err(|e| ProviderResolutionError::RouteSelection {
                model: model.info.model.clone(),
                detail: e.to_string(),
            })?
    };

    let route = configured
        .routes
        .get(&RouteId::new(&route_id))
        .ok_or_else(|| ProviderResolutionError::DefaultRouteNotFound(route_id.clone()))?;

    let protocol_id = route.protocol_id.clone();

    // P7-004: Use Endpoint::render as the sole URL construction entry point.
    // No string concatenation for URL building.
    let input = xai_grok_provider::endpoint::EndpointInput::new(
        xai_grok_provider::types::LLMRequest::new(&model.info.model),
        (),
    );
    let endpoint_url = route
        .endpoint
        .render(&input)
        .map_err(|e| ProviderResolutionError::Protocol(format!("endpoint render failed: {e}")))?;

    let base_url = if let Some(override_url) = base_url_override {
        override_url.to_string()
    } else {
        endpoint_url.to_string()
    };

    // Merge static headers and auth
    let mut extra_headers = route.static_headers.clone();
    extra_headers.extend(model.info.extra_headers.clone());

    if let Ok(auth_headers) = apply_auth_policy(&route.auth, &std::collections::HashMap::new()) {
        extra_headers.extend(auth_headers);
    }

    // Derive auth_scheme from the auth policy
    let auth_scheme = match &route.auth {
        AuthPolicy::None => xai_grok_sampler::AuthScheme::None,
        AuthPolicy::Bearer(_) => xai_grok_sampler::AuthScheme::Bearer,
        AuthPolicy::Header { name, .. } if name == "x-api-key" => {
            xai_grok_sampler::AuthScheme::XApiKey
        }
        _ => xai_grok_sampler::AuthScheme::Bearer,
    };

    // P7-006: protocol ID must be registered. Unknown is a typed error.
    // api_backend is derived for legacy migration only, not as primary dispatch.
    let protocol_id_known = matches!(
        protocol_id.as_str(),
        "chat_completions" | "responses" | "messages"
    );
    if !protocol_id_known {
        return Err(ProviderResolutionError::Protocol(format!(
            "unknown protocol_id: {protocol_id}"
        )));
    }
    let api_backend = match protocol_id.as_str() {
        "chat_completions" => ApiBackend::ChatCompletions,
        "responses" => ApiBackend::Responses,
        "messages" => ApiBackend::Messages,
        _ => unreachable!("checked above"),
    };

    Ok(SamplerConfig {
        api_key: api_key.map(|s| s.to_string()),
        model: model.info.model.clone(),
        base_url,
        endpoint_path: None,
        endpoint_query: None,
        api_backend,
        protocol_id: Some(protocol_id.as_str().into()),
        auth_scheme,
        extra_headers,
        context_window: model.info.context_window.get(),
        max_completion_tokens: model.info.max_completion_tokens,
        temperature: model.info.temperature,
        top_p: model.info.top_p,
        reasoning_effort: model.info.reasoning_effort,
        stream_tool_calls: model.info.stream_tool_calls.unwrap_or(false),
        ..Default::default()
    })
}

/// A legacy (unqualified) xAI model reference that can be migrated to
/// `provider/model` syntax. This is the only way a bare model name maps
/// to a specific provider — all other bare models must be explicitly
/// qualified or found in a provider's model catalog.
pub struct LegacyModelReference(&'static str);

impl LegacyModelReference {
    /// All known legacy xAI model names.
    pub const ALL: &'static [LegacyModelReference] = &[
        LegacyModelReference("grok-build"),
        LegacyModelReference("grok-3"),
        LegacyModelReference("grok-3-mini"),
        LegacyModelReference("grok-3-fast"),
        LegacyModelReference("grok-3-mini-fast"),
        LegacyModelReference("grok-2"),
        LegacyModelReference("grok-2-mini"),
        LegacyModelReference("grok-vision"),
        LegacyModelReference("grok-1"),
    ];

    /// The provider to which this legacy model resolves.
    pub fn provider(&self) -> &'static str {
        "xai"
    }

    /// The bare model name.
    pub fn model(&self) -> &'static str {
        self.0
    }

    /// Look up a bare model name; returns `Some` only for known legacy names.
    pub fn try_resolve(bare_model: &str) -> Option<&'static LegacyModelReference> {
        LegacyModelReference::ALL
            .iter()
            .find(|lr| lr.0 == bare_model)
    }
}

/// Resolve a bare model name through the legacy xAI model table.
/// Returns `None` for models not in the known legacy set.
fn resolve_legacy_model_ref(bare_model: &str) -> Option<String> {
    LegacyModelReference::try_resolve(bare_model).map(|lr| lr.provider().to_string())
}

/// Resolve a CLI model reference against the merged catalog.
///
/// Cases:
/// - `--model openai/gpt-4o` → provider=openai, model=gpt-4o
/// - `--provider openai --model gpt-4o` → equivalent
/// - conflicting `--provider anthropic --model openai/gpt-4o` → error
/// - bare model uniquely found in one provider → resolves
/// - ambiguous bare model → error listing canonical choices
/// - legacy bare xAI defaults → resolve predictably
pub fn resolve_cli_model_reference(
    model_ref: &str,
    provider_override: Option<&str>,
    merged_catalog: &IndexMap<String, ModelEntry>,
) -> Result<(String, String), ProviderResolutionError> {
    let (model_provider, bare_model) = xai_grok_provider::types::parse_model_ref(model_ref);

    match (model_provider, provider_override) {
        (Some(mp), Some(po)) => {
            let mp_name = mp.0.clone();
            if mp_name != po {
                return Err(ProviderResolutionError::ConflictingProvider {
                    provider: po.to_string(),
                    model: model_ref.to_string(),
                });
            }
            Ok((po.to_string(), bare_model))
        }
        (Some(mp), None) => Ok((mp.0.clone(), bare_model)),
        (None, Some(po)) => Ok((po.to_string(), bare_model)),
        (None, None) => {
            // Bare model: check merged catalog for unique resolution
            let canonical_key = format!("xai/{}", bare_model);
            let mut matches: Vec<String> = Vec::new();

            for key in merged_catalog.keys() {
                if key.ends_with(&format!("/{}", bare_model)) {
                    matches.push(key.clone());
                }
            }

            match matches.len() {
                0 => {
                    // Try legacy xAI migration path
                    if let Some(mapped) = resolve_legacy_model_ref(&bare_model) {
                        Ok((mapped, bare_model))
                    } else {
                        Err(ProviderResolutionError::ModelNotFound(bare_model))
                    }
                }
                1 => {
                    let provider = matches[0].split('/').next().unwrap_or("xai").to_string();
                    Ok((provider, bare_model))
                }
                _ => Err(ProviderResolutionError::AmbiguousModel(format!(
                    "bare model '{}' matches multiple providers: {}. Use provider/model syntax.",
                    bare_model,
                    matches.join(", ")
                ))),
            }
        }
    }
}

/// Merge models from multiple sources into a deterministic catalog.
///
/// Precedence (high → low):
///   manual user model override
///   > dynamic provider-discovered model metadata
///   > provider static known-model metadata
///   > embedded legacy xAI default metadata
///
/// Canonical key: `provider/model` for non-legacy models.
/// Bare lookup that matches multiple providers returns `AmbiguousModel`.
pub fn merge_model_catalog(
    registry_snapshot: &RegistrySnapshot,
    catalog_snapshot: &ModelCatalogSnapshot,
    manual_overrides: &IndexMap<String, ModelEntry>,
    legacy_defaults: &IndexMap<String, ModelEntry>,
    endpoints: &crate::agent::config::EndpointsConfig,
) -> IndexMap<String, ModelEntry> {
    let mut merged: IndexMap<String, ModelEntry> = IndexMap::new();

    // Layer 1: embedded legacy xAI defaults (lowest priority)
    for (key, entry) in legacy_defaults {
        merged.entry(key.clone()).or_insert_with(|| entry.clone());
    }

    // Layer 2: dynamic provider-discovered models from catalog
    for (_pid, catalog_entry) in &catalog_snapshot.providers {
        if catalog_entry.state != super::provider_catalog::ProviderCatalogState::Ready {
            continue;
        }
        for model_cfg in &catalog_entry.models {
            let key = format!("{}/{}", catalog_entry.provider_id.0, model_cfg.model);
            if !merged.contains_key(&key) {
                let display_name = model_cfg.name.clone().unwrap_or_default();
                let mut entry = ModelEntry::from_config_entry(model_cfg);
                entry.provider_id = Some(catalog_entry.provider_id.0.clone());
                merged.insert(key, entry);
            }
        }
    }

    // Layer 3: provider static known-model metadata (from registry snapshot
    // provider defaults). These fill in gaps when no dynamic model exists.
    for (_pid, configured) in &registry_snapshot.providers {
        if let xai_grok_provider::types::ModelSourceSpec::Static(models) = &configured.model_source
        {
            for model_id in models {
                let key = format!("{}/{}", configured.id.0, model_id);
                merged.entry(key).or_insert_with(|| {
                    let mut entry = ModelEntry::fallback(model_id, endpoints);
                    entry.provider_id = Some(configured.id.0.clone());
                    entry.info.base_url = configured
                        .routes
                        .get(&configured.default_route_id)
                        .and_then(|r| r.endpoint.base_url.clone())
                        .unwrap_or_default();
                    entry
                });
            }
        }
    }

    // Layer 4: manual user model overrides (highest priority)
    for (key, entry) in manual_overrides {
        merged.insert(key.clone(), entry.clone());
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn cli_model_ref_provider_prefix() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("openai/gpt-4o", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_provider_override() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("gpt-4o", Some("anthropic"), &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_conflicting_provider_error() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("openai/gpt-4o", Some("anthropic"), &catalog);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderResolutionError::ConflictingProvider { .. }
        ));
    }

    #[test]
    fn cli_model_ref_bare_unique_resolves() {
        let mut catalog = IndexMap::new();
        let mut entry =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        entry.provider_id = Some("openai".into());
        catalog.insert("openai/gpt-4o".into(), entry);
        let result = resolve_cli_model_reference("gpt-4o", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_bare_ambiguous_error() {
        let mut catalog = IndexMap::new();
        let mut e1 =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        e1.provider_id = Some("openai".into());
        catalog.insert("openai/gpt-4o".into(), e1);
        let mut e2 =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        e2.provider_id = Some("xai".into());
        catalog.insert("xai/gpt-4o".into(), e2);
        let result = resolve_cli_model_reference("gpt-4o", None, &catalog);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderResolutionError::AmbiguousModel(_)
        ));
    }

    #[test]
    fn cli_model_ref_bare_legacy_xai_fallback() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("grok-build", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "xai");
        assert_eq!(model, "grok-build");
    }
}
