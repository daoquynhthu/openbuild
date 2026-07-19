//! P8-011: `PreparedSamplerConfig` — the only sampler input after Phase 8.
//!
//! Production chain: `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`.

use crate::auth::{AuthPolicy, CredentialCandidate, SecretValue};
use crate::headers::SensitiveHeaderMap;
use crate::model::{GenerationOptions, ModelLimits};
use crate::resolution::ResolvedModelExecution;
use crate::types::{ModelId, ProviderId, RouteId};

/// Fully-prepared sampler configuration with resolved credentials and headers.
/// This is the ONLY input the sampler accepts after Phase 8.
#[derive(Clone, Debug)]
pub struct PreparedSamplerConfig {
    pub provider_id: ProviderId,
    pub route_id: RouteId,
    pub protocol_id: String,
    pub request_url: url::Url,
    pub headers: SensitiveHeaderMap,
    pub model_id: ModelId,
    pub generation: GenerationOptions,
    pub limits: ModelLimits,
}

/// Errors during request preparation.
#[derive(Debug, thiserror::Error)]
pub enum RequestPreparationError {
    #[error("credential resolution failed: {0}")]
    Credential(String),
    #[error("header conflict: {0}")]
    HeaderConflict(String),
    #[error("invalid header: {0}")]
    InvalidHeader(String),
}

/// Request-time credential context for `prepare_sampler_config`.
pub struct RequestCredential<'a> {
    pub request_override: Option<&'a SecretValue>,
    pub model_inline: Option<&'a SecretValue>,
    pub provider_inline: Option<&'a SecretValue>,
    pub env_reader: &'a dyn Fn(&str) -> Result<Option<SecretValue>, String>,
    pub session_resolver: &'a dyn Fn() -> Option<SecretValue>,
}

/// Prepare a `PreparedSamplerConfig` from a resolved execution and credentials.
/// This is the ONLY production entry point for constructing the sampler input.
///
/// 1. Resolves auth header from the execution's auth policy using the credential context.
/// 2. Merges all headers (route static + auth + request overrides).
/// 3. Produces a `PreparedSamplerConfig` with no plaintext secrets in Debug output.
pub async fn prepare_sampler_config(
    execution: &ResolvedModelExecution,
    credentials: &RequestCredential<'_>,
    request_headers: &[(&str, &str)],
) -> Result<PreparedSamplerConfig, RequestPreparationError> {
    use crate::headers::merge_headers;

    // Resolve auth header from policy candidates (system-fixed priority order)
    let auth_header = resolve_auth_from_policy(&execution.auth_policy, credentials)?;

    // Build request override header map
    let mut override_map = http::HeaderMap::new();
    for (name, value) in request_headers {
        let n = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| RequestPreparationError::InvalidHeader(name.to_string()))?;
        let v = http::HeaderValue::from_str(value)
            .map_err(|_| RequestPreparationError::InvalidHeader(value.to_string()))?;
        override_map.insert(n, v);
    }

    // Merge all headers
    let merged = merge_headers(
        &[],  // transport-required
        &execution.static_headers,
        &execution.static_headers,  // route static = provider extra in current model
        auth_header.as_ref().map(|(n, v)| (n.as_str(), v.as_str())),
        &override_map,
    ).map_err(|e| RequestPreparationError::HeaderConflict(e.to_string()))?;

    Ok(PreparedSamplerConfig {
        provider_id: execution.provider_id.clone(),
        route_id: execution.route_id.clone(),
        protocol_id: execution.protocol_id.to_string(),
        request_url: execution.request_url.clone(),
        headers: merged,
        model_id: execution.model_id.clone(),
        generation: execution.generation.clone(),
        limits: execution.limits.clone(),
    })
}

/// Resolve auth header from an `AuthPolicy` using the credential context.
pub(crate) fn resolve_auth_from_policy(
    policy: &AuthPolicy,
    ctx: &RequestCredential<'_>,
) -> Result<Option<(String, String)>, RequestPreparationError> {
    match policy {
        AuthPolicy::None => Ok(None),
        AuthPolicy::Bearer { candidates, required } => {
            let value = resolve_candidates_system_order(candidates, ctx);
            match value {
                Some(v) => Ok(Some(("Authorization".to_string(), format!("Bearer {v}")))),
                None if *required => Err(RequestPreparationError::Credential(
                    "required Bearer credential not resolved".into(),
                )),
                None => Ok(None),
            }
        }
        AuthPolicy::Header { name, candidates, required } => {
            let value = resolve_candidates_system_order(candidates, ctx);
            match value {
                Some(v) => Ok(Some((name.clone(), v))),
                None if *required => Err(RequestPreparationError::Credential(
                    format!("required header credential for {name} not resolved"),
                )),
                None => Ok(None),
            }
        }
    }
}

/// Resolve candidates in SYSTEM-fixed order, ignoring provider-declared order.
/// Providers declare ONLY which candidate types exist.
fn resolve_candidates_system_order(
    candidates: &[CredentialCandidate],
    ctx: &RequestCredential<'_>,
) -> Option<String> {
    let has_req = candidates.iter().any(|c| matches!(c, CredentialCandidate::RequestOverride));
    let has_model = candidates.iter().any(|c| matches!(c, CredentialCandidate::ModelInline));
    let has_prov = candidates.iter().any(|c| matches!(c, CredentialCandidate::ProviderInline));
    let env_keys: Vec<String> = candidates.iter().filter_map(|c| match c {
        CredentialCandidate::ModelEnvironment(k) | CredentialCandidate::ProviderEnvironment(k) | CredentialCandidate::BuiltinEnvironment(k) => Some(k.clone()),
        _ => None,
    }).flatten().collect();
    let has_sess = candidates.iter().any(|c| matches!(c, CredentialCandidate::Session(_)));

    if let Some(v) = ctx.request_override.filter(|_| has_req) { return Some(v.inner().to_string()); }
    if let Some(v) = ctx.model_inline.filter(|_| has_model) { return Some(v.inner().to_string()); }
    if let Some(v) = ctx.provider_inline.filter(|_| has_prov) { return Some(v.inner().to_string()); }
    for key in &env_keys {
        if let Ok(Some(v)) = (ctx.env_reader)(key) { return Some(v.inner().to_string()); }
    }
    if let Some(v) = (ctx.session_resolver)().filter(|_| has_sess) { return Some(v.inner().to_string()); }
    None
}

/// Test helper: create a `PreparedSamplerConfig` for direct sampler tests.
pub fn test_prepared_config(
    model_id: &str,
    base_url: &str,
) -> PreparedSamplerConfig {
    PreparedSamplerConfig {
        provider_id: ProviderId::new("test"),
        route_id: crate::types::RouteId::new("test-chat"),
        protocol_id: "chat_completions".to_string(),
        request_url: url::Url::parse(&format!("{base_url}/v1/chat/completions")).unwrap(),
        headers: crate::headers::SensitiveHeaderMap::new(http::HeaderMap::new()),
        model_id: ModelId::new(model_id),
        generation: GenerationOptions::default(),
        limits: ModelLimits::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepared_config_creates_valid_config() {
        let cfg = test_prepared_config("gpt-4o", "https://api.test.com");
        assert_eq!(cfg.model_id.0, "gpt-4o");
        assert_eq!(cfg.protocol_id, "chat_completions");
        assert!(cfg.request_url.as_str().contains("api.test.com"));
    }
}
