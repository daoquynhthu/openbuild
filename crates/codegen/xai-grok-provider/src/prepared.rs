//! P8-011: `PreparedSamplerConfig` — the only sampler input after Phase 8.
//!
//! Production chain: `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`.

use indexmap::IndexMap;

use crate::auth::{AuthPolicy, RequestCredentialContext};
use crate::headers::SensitiveHeaderMap;
use crate::model::{GenerationOptions, ModelLimits};
use crate::protocol::ProtocolId;
use crate::resolution::ResolvedModelExecution;
use crate::types::{ModelId, ProviderId, RouteId};

/// Fully-prepared sampler configuration with resolved credentials and headers.
/// This is the ONLY input the sampler accepts after Phase 8.
#[derive(Clone, Debug)]
pub struct PreparedSamplerConfig {
    pub provider_id: ProviderId,
    pub route_id: RouteId,
    pub protocol_id: ProtocolId,
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
        let api_backend = match &*protocol_id {
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
            protocol_id: Some(protocol_id),
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

/// Prepare a `PreparedSamplerConfig` from a resolved execution and credentials.
/// This is the ONLY production entry point for constructing the sampler input.
///
/// 1. Resolves auth header from the execution's auth policy using the credential context.
/// 2. Merges all headers (route static + auth + request overrides).
/// 3. Produces a `PreparedSamplerConfig` with no plaintext secrets in Debug output.
pub async fn prepare_sampler_config(
    execution: &ResolvedModelExecution,
    credentials: &RequestCredentialContext<'_>,
    request_headers: &[(&str, &str)],
) -> Result<PreparedSamplerConfig, RequestPreparationError> {
    use crate::headers::merge_headers;

    // Resolve auth header from policy candidates (system-fixed priority order)
    let auth_header = resolve_auth_from_policy(&execution.auth_policy, credentials).await?;

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
        auth_header.as_ref().map(|(n, v)| (n.as_ref(), v.as_ref())), // Layer 4: auth
        &override_map, // Layer 5: request overrides
    )
    .map_err(|e| RequestPreparationError::HeaderConflict(e.to_string()))?;

    Ok(PreparedSamplerConfig {
        provider_id: execution.provider_id.clone(),
        route_id: execution.route_id.clone(),
        protocol_id: execution.protocol_id.clone(),
        request_url: execution.request_url.clone(),
        headers: merged,
        model_id: execution.model_id.clone(),
        generation: execution.generation.clone(),
        limits: execution.limits.clone(),
    })
}

/// Resolve auth header from an `AuthPolicy` using the credential context.
pub(crate) async fn resolve_auth_from_policy(
    policy: &AuthPolicy,
    ctx: &RequestCredentialContext<'_>,
) -> Result<Option<(String, String)>, RequestPreparationError> {
    match policy {
        AuthPolicy::None => Ok(None),
        AuthPolicy::Bearer {
            candidates,
            required,
        } => {
            let value = ctx.resolve_candidates(candidates).await;
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
            let value = ctx.resolve_candidates(candidates).await;
            match value {
                Some(v) => Ok(Some((name.to_string(), v))),
                None if *required => Err(RequestPreparationError::Credential(format!(
                    "required header credential for {name} not resolved"
                ))),
                None => Ok(None),
            }
        }
    }
}

/// Test helper: create a `PreparedSamplerConfig` for direct sampler tests.
#[cfg(test)]
pub fn test_prepared_config(model_id: &str, base_url: &str) -> PreparedSamplerConfig {
    PreparedSamplerConfig {
        provider_id: ProviderId::new("test"),
        route_id: crate::types::RouteId::new("test-chat"),
        protocol_id: ProtocolId::from("chat_completions"),
        request_url: url::Url::parse(&format!("{base_url}/v1/chat/completions")).unwrap(),
        headers: crate::headers::SensitiveHeaderMap::new(http::HeaderMap::new()),
        model_id: ModelId::new(model_id),
        generation: GenerationOptions::default(),
        limits: ModelLimits::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;

    use super::*;
    use crate::auth::{CredentialCandidate, CredentialError, EnvironmentReader, SecretValue, SessionCredentialResolver, SessionKind};
    use url::Url;

    // ── Test helper types for EnvironmentReader / SessionCredentialResolver ──

    struct TestEnv;
    impl EnvironmentReader for TestEnv {
        fn read(&self, _var: &str) -> Result<Option<SecretValue>, CredentialError> {
            Ok(None)
        }
    }

    struct StaticEnv(&'static str, &'static str);
    impl EnvironmentReader for StaticEnv {
        fn read(&self, var: &str) -> Result<Option<SecretValue>, CredentialError> {
            if var == self.0 {
                Ok(Some(SecretValue::new(self.1.to_string())))
            } else {
                Ok(None)
            }
        }
    }

    struct TestSession;
    impl SessionCredentialResolver for TestSession {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
        {
            Box::pin(async { Ok(None) })
        }
    }

    struct FixedSession(&'static str);
    impl SessionCredentialResolver for FixedSession {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
        {
            let val = self.0.to_string();
            Box::pin(async move { Ok(Some(SecretValue::new(val))) })
        }
    }

    struct TrackingSession {
        called: std::sync::atomic::AtomicBool,
        value: &'static str,
    }

    impl TrackingSession {
        fn new(value: &'static str) -> Self {
            Self {
                called: std::sync::atomic::AtomicBool::new(false),
                value,
            }
        }

        fn was_called(&self) -> bool {
            self.called.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl SessionCredentialResolver for TrackingSession {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
        {
            self.called.store(true, std::sync::atomic::Ordering::SeqCst);
            let val = self.value.to_string();
            Box::pin(async move { Ok(Some(SecretValue::new(val))) })
        }
    }

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

    fn empty_credential() -> RequestCredentialContext<'static> {
        RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
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
        let execution = dummy_execution(AuthPolicy::header(http::HeaderName::from_static("x-api-key"), vec![], true));
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
        let execution = dummy_execution(AuthPolicy::header(http::HeaderName::from_static("x-api-key"), vec![], false));
        let result = prepare_sampler_config(&execution, &empty_credential(), &[]).await;
        assert!(result.is_ok(), "optional header with no candidates must succeed");
    }

    // ── P8-010A: Bearer credential precedence and format ──
    //
    // Full chain through prepare_sampler_config: auth resolution → header merge → final config.

    #[tokio::test]
    async fn bearer_request_override_produces_authorization_header() {
        let val = SecretValue::new("sk-override".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
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
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: Some(&inline),
            environment: &TestEnv,
            session: &TestSession,
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
        let env = StaticEnv("MY_API_KEY", "sk-from-env");
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &env,
            session: &TestSession,
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
        let ctx = RequestCredentialContext {
            request_override: Some(&request_val),
            model_inline: Some(&inline_val),
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
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
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
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
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
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

    // ── P8-010B: Header auth (x-api-key / Anthropic-style) ──

    #[tokio::test]
    async fn header_auth_request_override_produces_correct_header() {
        let val = SecretValue::new("sk-ant-override".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
        };
        let execution = dummy_execution(AuthPolicy::header(
            http::HeaderName::from_static("x-api-key"),
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with request override");
        let header = config.headers.inner().get("x-api-key");
        assert!(
            header.is_some(),
            "must produce x-api-key header for Header auth"
        );
        assert_eq!(header.unwrap().to_str().unwrap(), "sk-ant-override");
    }

    #[tokio::test]
    async fn header_auth_provider_inline_falls_back_correctly() {
        let inline = SecretValue::new("sk-ant-inline".to_string());
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: Some(&inline),
            environment: &TestEnv,
            session: &TestSession,
        };
        let execution = dummy_execution(AuthPolicy::header(
            http::HeaderName::from_static("x-api-key"),
            vec![CredentialCandidate::ProviderInline],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with provider inline");
        let header = config.headers.inner().get("x-api-key").unwrap();
        assert_eq!(header.to_str().unwrap(), "sk-ant-inline");
    }

    #[tokio::test]
    async fn header_auth_precedence_request_override_beats_inline() {
        let request_val = SecretValue::new("sk-ant-request".to_string());
        let inline_val = SecretValue::new("sk-ant-inline".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&request_val),
            model_inline: Some(&inline_val),
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
        };
        let execution = dummy_execution(AuthPolicy::header(
            http::HeaderName::from_static("x-api-key"),
            vec![
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
            ],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("must resolve with request override");
        let header = config.headers.inner().get("x-api-key").unwrap();
        assert_eq!(
            header.to_str().unwrap(),
            "sk-ant-request",
            "request override must beat inline for header auth"
        );
    }

    // ── P8-010D: OpenCode/Ollama no-auth path ──

    #[tokio::test]
    async fn no_auth_produces_no_authorization_header() {
        let execution = dummy_execution(AuthPolicy::None);
        let config = prepare_sampler_config(&execution, &empty_credential(), &[])
            .await
            .expect("AuthPolicy::None must always succeed");
        let headers = config.headers.inner();
        let has_auth = headers.contains_key("authorization");
        assert!(!has_auth, "no-auth must not produce Authorization header");
    }

    #[tokio::test]
    async fn no_auth_with_request_override_still_omits_auth() {
        let val = SecretValue::new("sk-should-not-appear".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
        };
        let execution = dummy_execution(AuthPolicy::None);
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        let config = result.expect("AuthPolicy::None must always succeed");
        let has_auth = config.headers.inner().contains_key("authorization");
        assert!(
            !has_auth,
            "no-auth must ignore credential candidates"
        );
    }

    #[tokio::test]
    async fn no_auth_static_headers_are_still_preserved() {
        let mut execution = dummy_execution(AuthPolicy::None);
        execution.static_headers.insert(
            "x-custom".to_string(),
            "custom-value".to_string(),
        );
        let config = prepare_sampler_config(&execution, &empty_credential(), &[])
            .await
            .expect("AuthPolicy::None must succeed");
        let custom = config.headers.inner().get("x-custom");
        assert_eq!(custom.unwrap().to_str().unwrap(), "custom-value");
    }

    // ── P8-010E: Extra headers isolation, invalid headers, mandatory conflicts ──

    #[tokio::test]
    async fn header_isolation_two_independent_calls_differ() {
        let val_a = SecretValue::new("sk-a".to_string());
        let val_b = SecretValue::new("sk-b".to_string());
        let ctx_a = RequestCredentialContext {
            request_override: Some(&val_a),
            ..empty_credential()
        };
        let ctx_b = RequestCredentialContext {
            request_override: Some(&val_b),
            ..empty_credential()
        };

        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));

        let cfg_a = prepare_sampler_config(&execution, &ctx_a, &[])
            .await
            .expect("config A must resolve");
        let cfg_b = prepare_sampler_config(&execution, &ctx_b, &[])
            .await
            .expect("config B must resolve");

        let auth_a = cfg_a
            .headers
            .inner()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let auth_b = cfg_b
            .headers
            .inner()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(auth_a, "Bearer sk-a");
        assert_eq!(auth_b, "Bearer sk-b");
        assert_ne!(
            auth_a, auth_b,
            "two independent calls must produce different headers"
        );
    }

    #[tokio::test]
    async fn invalid_header_name_in_request_overrides_rejected() {
        let val = SecretValue::new("sk-key".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            ..empty_credential()
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[("bad name", "value")]).await;
        assert!(result.is_err(), "invalid header name must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid header"),
            "error must describe invalid header: {err}"
        );
    }

    #[tokio::test]
    async fn invalid_header_value_in_request_overrides_rejected() {
        let val = SecretValue::new("sk-key".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            ..empty_credential()
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        let result =
            prepare_sampler_config(&execution, &ctx, &[("x-custom", "bad\x00value")]).await;
        assert!(result.is_err(), "invalid header value must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid header"),
            "error must describe invalid header: {err}"
        );
    }

    #[tokio::test]
    async fn conflict_between_static_header_and_auth_header_rejected() {
        let mut execution = dummy_execution(AuthPolicy::header(
            http::HeaderName::from_static("x-api-key"),
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        execution
            .static_headers
            .insert("x-api-key".to_string(), "static-value".to_string());

        let val = SecretValue::new("auth-value".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            ..empty_credential()
        };
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        assert!(
            result.is_err(),
            "conflicting static vs auth header must be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("header conflict"),
            "error must describe header conflict: {err}"
        );
    }

    #[tokio::test]
    async fn same_static_and_auth_header_value_allowed() {
        let mut execution = dummy_execution(AuthPolicy::header(
            http::HeaderName::from_static("x-api-key"),
            vec![CredentialCandidate::RequestOverride],
            true,
        ));
        execution
            .static_headers
            .insert("x-api-key".to_string(), "same-value".to_string());

        let val = SecretValue::new("same-value".to_string());
        let ctx = RequestCredentialContext {
            request_override: Some(&val),
            ..empty_credential()
        };
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        assert!(
            result.is_ok(),
            "matching static and auth header values must be allowed"
        );
    }

    // ── P8-010C: xAI session resolver tests ──
    //
    // Session must only be used when ALL explicit candidates are exhausted.

    #[tokio::test]
    async fn session_used_only_when_explicit_candidates_missing() {
        let request_val = SecretValue::new("sk-request".to_string());

        // Session resolver is called but should NOT be used because request_override wins
        let tracking = TrackingSession::new("sk-session");
        let ctx = RequestCredentialContext {
            request_override: Some(&request_val),
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &tracking,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![
                CredentialCandidate::RequestOverride,
                CredentialCandidate::Session(SessionKind::Xai),
            ],
            true,
        ));
        let config = prepare_sampler_config(&execution, &ctx, &[])
            .await
            .expect("must resolve from request_override");
        let auth = config.headers.inner().get("authorization").unwrap();
        assert_eq!(
            auth.to_str().unwrap(),
            "Bearer sk-request",
            "request override must be used, not session"
        );
        assert!(
            !tracking.was_called(),
            "session resolver must not be called when earlier candidate succeeds"
        );
    }

    #[tokio::test]
    async fn session_fills_when_all_explicit_candidates_absent() {
        let fixed = FixedSession("sk-session");
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &fixed,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::Session(SessionKind::Xai)],
            true,
        ));
        let config = prepare_sampler_config(&execution, &ctx, &[])
            .await
            .expect("must resolve from session");
        let auth = config.headers.inner().get("authorization").unwrap();
        assert_eq!(auth.to_str().unwrap(), "Bearer sk-session");
    }

    #[tokio::test]
    async fn session_returning_none_fails_when_required() {
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &TestSession,
        };
        let execution = dummy_execution(AuthPolicy::bearer(
            vec![CredentialCandidate::Session(SessionKind::Xai)],
            true,
        ));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        assert!(
            result.is_err(),
            "session returning None with mandatory auth must fail"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("required Bearer credential"),
            "error must describe missing credential: {err}"
        );
        assert!(
            !err.contains(CANARY),
            "error must not contain canary secret: {err}"
        );
    }

    #[tokio::test]
    async fn session_not_used_when_session_candidate_absent() {
        let tracking = TrackingSession::new("sk-session");
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &tracking,
        };
        // No Session candidate in the list
        let execution = dummy_execution(AuthPolicy::bearer(vec![], true));
        let result = prepare_sampler_config(&execution, &ctx, &[]).await;
        assert!(
            result.is_err(),
            "mandatory auth with no candidates must fail even if session resolver present"
        );
        assert!(
            !tracking.was_called(),
            "session resolver must not be called when Session candidate is absent"
        );
    }

    #[tokio::test]
    async fn optional_session_not_used_when_session_candidate_absent() {
        let tracking = TrackingSession::new("sk-session");
        let ctx = RequestCredentialContext {
            request_override: None,
            model_inline: None,
            provider_inline: None,
            environment: &TestEnv,
            session: &tracking,
        };
        let execution = dummy_execution(AuthPolicy::bearer(vec![], false));
        let config = prepare_sampler_config(&execution, &ctx, &[])
            .await
            .expect("optional auth with no candidates must succeed");
        assert!(
            config.headers.inner().get("authorization").is_none(),
            "no auth header expected"
        );
        assert!(
            !tracking.was_called(),
            "session resolver must not be called when Session candidate is absent"
        );
    }
}
