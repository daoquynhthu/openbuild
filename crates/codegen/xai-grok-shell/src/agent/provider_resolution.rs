use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::auth::{AuthPolicy, apply_auth_policy};
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_provider::route::Route;
use xai_grok_provider::types::{ProviderId, RouteId};
use xai_grok_sampler::SamplerConfig;
use xai_grok_sampling_types::ApiBackend;

use super::config::ModelEntry;

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
    let endpoint_path = route.endpoint.path_for_default();
    let endpoint_query = route.endpoint.query.as_ref().map(|q| {
        q.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
    });

    let base_url = base_url_override.map(|s| s.to_string()).unwrap_or_else(|| {
        route
            .endpoint
            .base_url
            .clone()
            .unwrap_or_else(|| String::new())
    });

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

    let api_backend = match protocol_id.as_str() {
        "chat_completions" => ApiBackend::ChatCompletions,
        "responses" => ApiBackend::Responses,
        "messages" => ApiBackend::Messages,
        _ => {
            return Err(ProviderResolutionError::Protocol(format!(
                "unknown protocol_id: {protocol_id}"
            )));
        }
    };

    Ok(SamplerConfig {
        api_key: api_key.map(|s| s.to_string()),
        model: model.info.model.clone(),
        base_url,
        endpoint_path: Some(endpoint_path),
        endpoint_query,
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
