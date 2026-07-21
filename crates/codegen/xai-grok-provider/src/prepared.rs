//! P8-011: `PreparedSamplerConfig` — the only sampler input after Phase 8.
//!
//! Production chain: `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`.

use indexmap::IndexMap;

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

/// Bridge conversion for P8-011: `PreparedSamplerConfig` → `SamplerConfig`.
///
/// This conversion preserves the resolved auth headers from the prepared config
/// and infers the `auth_scheme` from the header contents. The resulting
/// `SamplerConfig` can be passed to `SamplingClient::new`.
impl From<PreparedSamplerConfig> for xai_grok_sampler::SamplerConfig {
    fn from(prepared: PreparedSamplerConfig) -> Self {
        let protocol_id = prepared.protocol_id.clone();
        let api_backend = match protocol_id.as_str() {
            "chat_completions" => xai_grok_sampler::ApiBackend::ChatCompletions,
            "responses" => xai_grok_sampler::ApiBackend::Responses,
            "messages" => xai_grok_sampler::ApiBackend::Messages,
            _ => xai_grok_sampler::ApiBackend::ChatCompletions,
        };

        let mut extra_headers = IndexMap::new();
        let mut auth_scheme = xai_grok_sampler::AuthScheme::None;
        for (name, value) in prepared.headers.inner().iter() {
            if let Ok(v) = value.to_str() {
                let n = name.as_str().to_lowercase();
                if n == "authorization" {
                    if v.starts_with("Bearer ") {
                        auth_scheme = xai_grok_sampler::AuthScheme::Bearer;
                    }
                } else if n == "x-api-key" {
                    auth_scheme = xai_grok_sampler::AuthScheme::XApiKey;
                }
                extra_headers.insert(name.as_str().to_string(), v.to_string());
            }
        }

        Self {
            model: prepared.model_id.0,
            base_url: prepared.request_url.to_string(),
            request_url: Some(prepared.request_url.to_string()),
            api_backend,
            protocol_id: Some(protocol_id.into()),
            auth_scheme,
            extra_headers,
            context_window: prepared.limits.context.unwrap_or(0),
            max_completion_tokens: prepared.generation.max_tokens,
            temperature: prepared.generation.temperature,
            top_p: prepared.generation.top_p,
            ..Default::default()
        }
    }
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

    // Merge all headers in priority order.
    // Layer 2 (route static) already includes route-mandatory + provider-level extra headers
    // baked in by the route compiler. Layer 3 (provider extra) is empty until
    // ResolvedModelExecution carries them separately.
    let provider_extra: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
    let merged = merge_headers(
        &[], // Layer 1: transport-required
        &execution.static_headers, // Layer 2: route static headers
        &provider_extra,           // Layer 3: provider extra headers (reserved)
        auth_header.as_ref().map(|(n, v)| (n.as_str(), v.as_str())), // Layer 4: auth
        &override_map, // Layer 5: request overrides
    )
    .map_err(|e| RequestPreparationError::HeaderConflict(e.to_string()))?;

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
        AuthPolicy::Bearer {
            candidates,
            required,
        } => {
            let value = resolve_candidates_system_order(candidates, ctx);
            match value {
                Some(v) => Ok(Some(("Authorization".to_string(), format!("Bearer {v}")))),
                None if *required => Err(RequestPreparationError::Credential(
                    "required Bearer credential not resolved".into(),
                )),
                None => Ok(None),
            }
        }
        AuthPolicy::Header {
            name,
            candidates,
            required,
        } => {
            let value = resolve_candidates_system_order(candidates, ctx);
            match value {
                Some(v) => Ok(Some((name.clone(), v))),
                None if *required => Err(RequestPreparationError::Credential(format!(
                    "required header credential for {name} not resolved"
                ))),
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
    let has_req = candidates
        .iter()
        .any(|c| matches!(c, CredentialCandidate::RequestOverride));
    let has_model = candidates
        .iter()
        .any(|c| matches!(c, CredentialCandidate::ModelInline));
    let has_prov = candidates
        .iter()
        .any(|c| matches!(c, CredentialCandidate::ProviderInline));
    let env_keys: Vec<String> = candidates
        .iter()
        .filter_map(|c| match c {
            CredentialCandidate::ModelEnvironment(k)
            | CredentialCandidate::ProviderEnvironment(k)
            | CredentialCandidate::BuiltinEnvironment(k) => Some(k.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let has_sess = candidates
        .iter()
        .any(|c| matches!(c, CredentialCandidate::Session(_)));

    if let Some(v) = ctx.request_override.filter(|_| has_req) {
        return Some(v.inner().to_string());
    }
    if let Some(v) = ctx.model_inline.filter(|_| has_model) {
        return Some(v.inner().to_string());
    }
    if let Some(v) = ctx.provider_inline.filter(|_| has_prov) {
        return Some(v.inner().to_string());
    }
    for key in &env_keys {
        if let Ok(Some(v)) = (ctx.env_reader)(key) {
            return Some(v.inner().to_string());
        }
    }
    if let Some(v) = (ctx.session_resolver)().filter(|_| has_sess) {
        return Some(v.inner().to_string());
    }
    None
}

/// Test helper: create a `PreparedSamplerConfig` for direct sampler tests.
pub fn test_prepared_config(model_id: &str, base_url: &str) -> PreparedSamplerConfig {
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
    use url::Url;

    fn dummy_execution(auth: AuthPolicy) -> ResolvedModelExecution {
        ResolvedModelExecution {
            provider_id: ProviderId::new("test"),
            route_id: RouteId::new("test-chat"),
            protocol_id: "chat_completions".into(),
            request_url: Url::parse("http://127.0.0.1:1/v1/chat/completions").unwrap(),
            static_headers: IndexMap::new(),
            auth_policy: auth,
            model_id: ModelId::new("test-model"),
            generation: GenerationOptions::default(),
            limits: ModelLimits::default(),
        }
    }

    fn empty_credential() -> RequestCredential<'static> {
        RequestCredential {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        }
    }

    #[test]
    fn test_prepared_config_creates_valid_config() {
        let cfg = test_prepared_config("gpt-4o", "https://api.test.com");
        assert_eq!(cfg.model_id.0, "gpt-4o");
        assert_eq!(cfg.protocol_id, "chat_completions");
        assert!(cfg.request_url.as_str().contains("api.test.com"));
    }

    const CANARY: &str = "sk-canary-leak-check";

    // ── P8-010F: Missing credential failure matrix ──
    //
    // All failures must occur before any HTTP send.
    // Error messages must not contain the canary secret.

    #[tokio::test]
    async fn missing_required_bearer_credential_fails_before_http_send() {
        let execution = dummy_execution(AuthPolicy::bearer(vec![], true));
        let result = prepare_sampler_config(&execution, &empty_credential(), &[]).await;
        assert!(result.is_err(), "required bearer with no candidates must error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("required Bearer credential"),
            "error must describe the missing credential: {err}"
        );
        assert!(
            !err.contains(CANARY),
            "error must not contain canary secret: {err}"
        );
    }

    #[tokio::test]
    async fn missing_required_header_credential_fails_before_http_send() {
        let execution = dummy_execution(AuthPolicy::header("x-api-key", vec![], true));
        let result = prepare_sampler_config(&execution, &empty_credential(), &[]).await;
        assert!(result.is_err(), "required header with no candidates must error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("x-api-key"),
            "error must describe the missing header credential: {err}"
        );
        assert!(
            !err.contains(CANARY),
            "error must not contain canary secret: {err}"
        );
    }

    #[tokio::test]
    async fn optional_bearer_without_candidates_succeeds() {
        let execution = dummy_execution(AuthPolicy::bearer(vec![], false));
        let result = prepare_sampler_config(&execution, &empty_credential(), &[]).await;
        assert!(result.is_ok(), "optional bearer with no candidates must succeed");
        let config = result.unwrap();
        // No auth header should be present
        let auth_value = config.headers.inner().get("authorization");
        assert!(auth_value.is_none(), "no authorization header expected");
    }

    #[tokio::test]
    async fn optional_header_without_candidates_succeeds() {
        let execution = dummy_execution(AuthPolicy::header("x-api-key", vec![], false));
        let result = prepare_sampler_config(&execution, &empty_credential(), &[]).await;
        assert!(result.is_ok(), "optional header with no candidates must succeed");
    }

    // ── P8-010A: Bearer credential precedence and format ──
    //
    // Full chain through prepare_sampler_config: auth resolution → header merge → final config.

    #[tokio::test]
    async fn bearer_request_override_produces_authorization_header() {
        let val = SecretValue::new("sk-override".to_string());
        let ctx = RequestCredential {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with request override");
        let auth = config.headers.inner().get("authorization");
        assert!(auth.is_some(), "must produce authorization header");
        assert_eq!(
            auth.unwrap().to_str().unwrap(),
            "Bearer sk-override",
            "must format Bearer token correctly"
        );
    }

    #[tokio::test]
    async fn bearer_provider_inline_falls_back_correctly() {
        let inline = SecretValue::new("sk-inline".to_string());
        let ctx = RequestCredential {
            request_override: None,
            model_inline: None,
            provider_inline: Some(&inline),
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::ProviderInline],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with provider inline");
        let auth = config.headers.inner().get("authorization").unwrap();
        assert_eq!(auth.to_str().unwrap(), "Bearer sk-inline");
    }

    #[tokio::test]
    async fn bearer_env_reader_used_when_inline_absent() {
        let ctx = RequestCredential {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            env_reader: &|k| {
                assert_eq!(k, "MY_API_KEY");
                Ok(Some(SecretValue::new("sk-from-env".to_string())))
            },
            session_resolver: &|| None,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::ProviderEnvironment(vec![
                "MY_API_KEY".into(),
            ])],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve from env reader");
        let auth = config.headers.inner().get("authorization").unwrap();
        assert_eq!(auth.to_str().unwrap(), "Bearer sk-from-env");
    }

    #[tokio::test]
    async fn bearer_precedence_request_override_beats_inline() {
        let request_val = SecretValue::new("sk-request".to_string());
        let inline_val = SecretValue::new("sk-inline".to_string());
        let ctx = RequestCredential {
            request_override: Some(&request_val),
            model_inline: Some(&inline_val),
            provider_inline: None,
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
            ],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with request override");
        let auth = config.headers.inner().get("authorization").unwrap();
        assert_eq!(
            auth.to_str().unwrap(),
            "Bearer sk-request",
            "request override must beat inline"
        );
    }

    #[tokio::test]
    async fn header_merge_preserves_static_headers() {
        let mut execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        execution.static_headers.insert(
            "x-custom".to_string(),
            "custom-value".to_string(),
        );

        let val = SecretValue::new("sk-key".to_string());
        let ctx = RequestCredential {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        };
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must succeed");
        let custom = config.headers.inner().get("x-custom");
        assert!(custom.is_some(), "static headers must be preserved");
        assert_eq!(custom.unwrap().to_str().unwrap(), "custom-value");
    }

    #[tokio::test]
    async fn request_overrides_win_over_static_headers() {
        let mut execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        execution.static_headers.insert(
            "x-custom".to_string(),
            "original".to_string(),
        );

        let val = SecretValue::new("sk-key".to_string());
        let ctx = RequestCredential {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            env_reader: &|_| Ok(None),
            session_resolver: &|| None,
        };
        let result = prepare_sampler_config(&execution, &ctx, &[("x-custom", "override")]).await;
        let config = result.expect("must succeed");
        let custom = config.headers.inner().get("x-custom");
        assert_eq!(
            custom.unwrap().to_str().unwrap(),
            "override",
            "request overrides must win over static headers"
        );
    }

    #[tokio::test]
    async fn prepare_sampler_config_preserves_protocol_and_url() {
        let execution = dummy_execution(AuthPolicy::None);
        let config = prepare_sampler_config(&execution, &empty_credential(), &[])
            .await
            .expect("no-auth must succeed");
        assert_eq!(config.protocol_id, "chat_completions");
        assert_eq!(config.model_id.0, "test-model");
        assert!(config
            .request_url
            .as_str()
            .contains("127.0.0.1:1/v1/chat/completions"));
    }
}
