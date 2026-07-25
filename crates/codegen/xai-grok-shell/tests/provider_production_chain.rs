//! Phase A + R3-RED-04: Production-entry regression + hard-error fallback tests.
//!
//! These tests reproduce production defects before fixes are applied.
//! Each test MUST fail initially (documenting the known bug) and pass after
//! the corresponding OBPA issue is fixed. Tests use the real async production
//! chain to avoid the OBPA-001 nested runtime panic that blocks direct use
//! of `execution_to_sampler_config` inside `#[tokio::test]`.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::StreamExt;
use xai_grok_provider::auth::{
    CredentialError, EnvironmentReader, RequestCredentialContext, SecretValue,
    SessionCredentialResolver,
};
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::headers::RequestHeaderOverrides;
use xai_grok_provider::prepared::{RequestPreparationError, prepare_sampler_config};
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_shell::agent::config::{EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::provider_resolution::{
    execution_to_sampler_config, resolve_model_execution,
};
use xai_grok_test_support::MockInferenceServer;

/// Environment reader that always returns None (no env vars set).
struct TestEnv;
impl EnvironmentReader for TestEnv {
    fn read(&self, _var: &str) -> Result<Option<SecretValue>, CredentialError> {
        Ok(None)
    }
}

/// Session resolver that always returns None (no session token).
struct TestSession;
impl SessionCredentialResolver for TestSession {
    fn resolve(
        &self,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>,
    > {
        Box::pin(async { Ok(None) })
    }
}

fn model_entry(provider_id: &str, model: &str) -> ModelEntry {
    let mut entry = ModelEntry::fallback(model, &EndpointsConfig::default());
    entry.provider_id = Some(provider_id.to_string());
    entry
}

async fn resolve_prepare_with_overrides(
    model: &ModelEntry,
    snapshot: &RegistrySnapshot,
    api_key: Option<&str>,
    request_headers: &RequestHeaderOverrides,
) -> Result<xai_grok_sampler::SamplerConfig, RequestPreparationError> {
    let execution = resolve_model_execution(model, snapshot, None)
        .expect("resolve_model_execution must succeed");
    let model_inline = api_key.map(|k| SecretValue::new(k.to_string()));
    let provider_inline = model.provider_id.as_ref().and_then(|pid| {
        let provider_pid = xai_grok_provider::types::ProviderId::new(pid);
        snapshot
            .providers
            .get(&provider_pid)
            .and_then(|cp| cp.config.api_key.as_ref())
            .map(|k| SecretValue::new(k.to_string()))
    });
    let env = TestEnv;
    let session = TestSession;
    let creds = RequestCredentialContext::new(
        None,
        model_inline.as_ref(),
        provider_inline.as_ref(),
        &env,
        &session,
    );
    let prepared = prepare_sampler_config(&execution, &creds, request_headers).await?;
    Ok(xai_grok_sampler::SamplerConfig::try_from(prepared)
        .expect("valid protocol in test"))
}

async fn resolve_prepare(
    model: &ModelEntry,
    snapshot: &RegistrySnapshot,
    api_key: Option<&str>,
) -> xai_grok_sampler::SamplerConfig {
    resolve_prepare_with_overrides(model, snapshot, api_key, &RequestHeaderOverrides::new())
        .await
        .expect("resolve_prepare must succeed")
}

async fn bootstrap_async(
    toml_str: &str,
) -> Arc<xai_grok_shell::agent::provider_runtime::ProviderRuntime> {
    let toml: toml::Value = toml::from_str(toml_str).unwrap();
    xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
        .await
        .expect("bootstrap must succeed")
}

// ---------------------------------------------------------------------------
// A2.1: OBPA-001 — async chain no longer creates nested runtime
// ---------------------------------------------------------------------------
//
// `execution_to_sampler_config` is now async, so calling it inside a
// tokio runtime context no longer panics from `Runtime::new().block_on()`.
#[tokio::test]
async fn obpa001_no_nested_runtime_panic_after_fix() {
    let snapshot = bootstrap_async(
        r#"
        [provider.test-provider]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "test-key"


    "#,
    )
    .await
    .snapshot();

    let model = model_entry("test-provider", "test-model");

    // This previously panicked with "Cannot start a runtime from within
    // a runtime". Now that execution_to_sampler_config is async, it
    // runs cleanly inside the existing tokio context.
    let result = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model,
        &snapshot,
        Some("test-key"),
        None,
    )
    .await;

    assert!(
        result.is_ok(),
        "execution_to_sampler_config must succeed inside tokio runtime after OBPA-001 fix: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// A2.2: OBPA-003/005 — Provider inline key does not appear in request
// ---------------------------------------------------------------------------
#[tokio::test]
async fn obpa005_provider_inline_key_flows_to_auth_header() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.custom-key]
        kind = "openai_compatible"
        base_url = "{mock_url}"
        api_key = "provider-inline-secret"

        "#,
    );

    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-key", "test-model");
    let config = resolve_prepare(&model, &snapshot, None).await;

    let has_auth = config
        .extra_headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v.starts_with("Bearer "));

    assert!(
        has_auth,
        "provider inline api_key must produce Authorization header via credential context (OBPA-005): extra_headers={:?}",
        config.extra_headers,
    );

    let client = xai_grok_sampler::SamplingClient::new(config).unwrap();
    let request = xai_grok_sampling_types::ConversationRequest::from_items(vec![
        xai_grok_sampling_types::ConversationItem::user("hello"),
    ]);
    let (mut stream, _) = client.conversation_stream(request).await.unwrap();
    while (stream.next().await).is_some() {}

    let requests = server.requests();
    let request_with_auth: Vec<_> = requests
        .iter()
        .filter(|r| {
            r.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        })
        .collect();

    assert!(
        !request_with_auth.is_empty(),
        "request MUST carry Authorization header with provider inline key (OBPA-005): headers: {:?}",
        requests.iter().flat_map(|r| &r.headers).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// A2.3: OBPA-007 — Custom protocol is fixed as Chat Completions
// ---------------------------------------------------------------------------
#[tokio::test]
async fn obpa007_custom_protocol_not_overridden_by_chat() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.custom-protocol]
        kind = "openai_compatible"
        base_url = "{mock_url}"
        api_key = "test-key"
        protocol = "responses"
        "#,
    );

    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-protocol", "test-model");

    let execution = resolve_model_execution(&model, &snapshot, None)
        .expect("resolve_model_execution must succeed");

    assert_eq!(
        &*execution.protocol_id.0, "responses",
        "protocol=responses must produce responses protocol_id (OBPA-007)"
    );

    let config = resolve_prepare(&model, &snapshot, Some("test-key")).await;
    assert_eq!(
        config.api_backend,
        xai_grok_sampler::ApiBackend::Responses,
        "protocol=responses must produce Responses backend (OBPA-007)"
    );

    let client = xai_grok_sampler::SamplingClient::new(config).unwrap();
    let request = xai_grok_sampling_types::ConversationRequest::from_items(vec![
        xai_grok_sampling_types::ConversationItem::user("hello"),
    ]);
    let (mut stream, _) = client.conversation_stream(request).await.unwrap();
    while (stream.next().await).is_some() {}

    let requests = server.requests();
    let responses_req: Vec<_> = requests
        .iter()
        .filter(|r| r.path.contains("/responses"))
        .collect();
    assert!(
        !responses_req.is_empty(),
        "protocol=responses must produce /responses endpoint (OBPA-007), got: {:?}",
        requests.iter().map(|r| &r.path).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// A2.4: OBPA-008 — CLI base URL override lost after reload
// ---------------------------------------------------------------------------
#[tokio::test]
async fn obpa008_cli_base_url_override_survives_reload() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let mut cli_override = ProviderConfig::default();
    cli_override.id = Some("test-provider".into());
    cli_override.base_url = Some(mock_url.clone());

    let toml_str = r#"
        [provider.test-provider]
        kind = "openai_compatible"
        api_key = "test-key"


    "#;

    // Bootstrap with CLI override
    let toml: toml::Value = toml::from_str(toml_str).unwrap();
    let runtime = xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(
        &toml,
        None,
        Some(cli_override),
    )
    .await
    .expect("bootstrap must succeed");

    let snapshot = runtime.snapshot();

    let model = model_entry("test-provider", "test-model");
    let config = resolve_prepare(&model, &snapshot, Some("test-key")).await;
    assert!(
        config.base_url.contains("127.0.0.1"),
        "CLI override base_url must be used: {}",
        config.base_url
    );

    // Reload through coordinator path (production reload path which
    // preserves the startup CLI override context).
    let dir = std::env::temp_dir().join(format!(
        "obpa008-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.toml");
    std::fs::write(&config_path, toml_str).unwrap();

    let ctx = std::sync::Arc::new(
        xai_grok_shell::agent::provider_config_coordinator::ProviderResolutionContext {
            legacy_migration: runtime
                .startup_legacy_migration
                .try_read()
                .ok()
                .and_then(|v| v.clone()),
            cli_overrides: runtime
                .startup_cli_overrides
                .try_read()
                .ok()
                .and_then(|v| v.clone()),
        },
    );
    let coordinator = std::sync::Arc::new(
        xai_grok_shell::agent::provider_config_coordinator::ProviderConfigCoordinator::new(
            runtime.clone(),
            config_path,
            ctx,
        ),
    );

    let reload_result = coordinator.apply_external_file().await;
    assert!(
        reload_result.is_ok(),
        "coordinator reload must succeed: {:?}",
        reload_result
    );

    // After reload via coordinator, the CLI override must be preserved
    // because the coordinator uses the resolution context.
    let config_after = resolve_prepare(&model, &runtime.snapshot(), Some("test-key")).await;
    assert!(
        config_after.base_url.contains("127.0.0.1"),
        "CLI base_url override must survive coordinator reload (OBPA-008), got: {}",
        config_after.base_url
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Phase E gate: provider extra header appears in HTTP request
// ---------------------------------------------------------------------------
#[tokio::test]
async fn provider_extra_header_flows_to_request() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.custom-header]
        kind = "openai_compatible"
        base_url = "{mock_url}"
        api_key = "test-key"
        extra_headers = {{ "X-Provider-Custom" = "provider-value" }}
        "#,
    );
    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-header", "test-model");
    let config = resolve_prepare(&model, &snapshot, None).await;

    let client = xai_grok_sampler::SamplingClient::new(config).unwrap();
    let request = xai_grok_sampling_types::ConversationRequest::from_items(vec![
        xai_grok_sampling_types::ConversationItem::user("hello"),
    ]);
    let (mut stream, _) = client.conversation_stream(request).await.unwrap();
    while (stream.next().await).is_some() {}

    let requests = server.requests();
    let has_custom_header = requests.iter().any(|r| {
        r.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Provider-Custom") && v == "provider-value")
    });
    assert!(
        has_custom_header,
        "provider extra_headers must appear in HTTP request: {:?}",
        requests.iter().flat_map(|r| &r.headers).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Phase E gate: request override header merge works
// ---------------------------------------------------------------------------
#[tokio::test]
async fn request_override_header_appears_in_request() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.custom-req]
        kind = "openai_compatible"
        base_url = "{mock_url}"
        api_key = "test-key"
        "#,
    );
    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-req", "test-model");

    let req_overrides = RequestHeaderOverrides::from_slice(&[("X-Request-Override", "req-value")])
        .expect("valid request overrides");

    let config = resolve_prepare_with_overrides(&model, &snapshot, None, &req_overrides)
        .await
        .expect("resolve_prepare must succeed");

    let client = xai_grok_sampler::SamplingClient::new(config).unwrap();
    let request = xai_grok_sampling_types::ConversationRequest::from_items(vec![
        xai_grok_sampling_types::ConversationItem::user("hello"),
    ]);
    let (mut stream, _) = client.conversation_stream(request).await.unwrap();
    while (stream.next().await).is_some() {}

    let requests = server.requests();
    let has_override = requests.iter().any(|r| {
        r.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Request-Override") && v == "req-value")
    });
    assert!(
        has_override,
        "request override headers must appear in HTTP request: {:?}",
        requests.iter().flat_map(|r| &r.headers).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// R3-RED-04: Reproduce hard-error fallback
//
// The agent's `prepare_sampling_config_for_model` (agent_ops.rs:1185) uses
// `unwrap_or_else` to catch provider errors and fall back to the legacy
// `sampling_config_for_model`. This means provider-bound models get a
// `SamplerConfig` via legacy fallback instead of a typed error.
//
// At the direct-chain level (tested here), some scenarios already produce
// correct errors (invalid endpoint, unknown route). Others incorrectly
// return `Ok(SamplingConfig)` without validation:
//   - missing credential  → BUG: chain returns Ok with no auth
//   - incompatible proto  → BUG: chain does not validate protocol compat
//
// After Phase 2 the agent-level fallback becomes reachable; after Phase 3
// all four scenarios must produce typed errors at every level.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn red04_invalid_endpoint_produces_chain_error() {
    let snapshot = bootstrap_async(
        r#"
        [provider.bad-endpoint]
        kind = "openai_compatible"
        base_url = ""
        api_key = "test-key"
    "#,
    )
    .await
    .snapshot();

    let model = model_entry("bad-endpoint", "test-model");

    let result = execution_to_sampler_config(&model, &snapshot, Some("test-key"), None).await;

    // Lower chain correctly rejects empty base_url.
    // The BUG is at the agent level (agent_ops.rs:1185) which catches this
    // error and falls back to legacy — visible after Phase 2.
    assert!(
        result.is_err(),
        "R3-RED-04 invalid-endpoint: chain must return Err — agent-level fallback is the bug"
    );
}

#[tokio::test]
async fn red04_missing_credential_incorrectly_returns_ok() {
    let snapshot = bootstrap_async(
        r#"
        [provider.no-cred]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0/v1"
        # no api_key, no env_key
    "#,
    )
    .await
    .snapshot();

    let model = model_entry("no-cred", "test-model");

    let result = execution_to_sampler_config(&model, &snapshot, None, None).await;

    // BUG: lower chain returns Ok (SamplerConfig) without any credential.
    // After Phase 3, must return a typed credential error.
    assert!(
        result.is_ok(),
        "R3-RED-04 missing-cred: current code INCORRECTLY returns Ok without credentials"
    );
}

#[tokio::test]
async fn red04_unknown_route_id_correctly_errs() {
    let snapshot = bootstrap_async(
        r#"
        [provider.known]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0/v1"
        api_key = "test-key"
    "#,
    )
    .await
    .snapshot();

    let mut model = model_entry("known", "test-model");
    model.route_id = Some("nonexistent-route".to_string());

    let result = execution_to_sampler_config(&model, &snapshot, Some("test-key"), None).await;

    assert!(
        result.is_err(),
        "R3-RED-04 unknown-route: chain must return Err — BUG is agent-level fallback (agent_ops.rs:1185)"
    );
}

#[tokio::test]
async fn red04_incompatible_protocol_incorrectly_returns_ok() {
    let snapshot = bootstrap_async(
        r#"
        [provider.test-proto]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0/v1"
        api_key = "test-key"
        protocol = "responses"
    "#,
    )
    .await
    .snapshot();

    let model = model_entry("test-proto", "test-model");

    let result = execution_to_sampler_config(&model, &snapshot, Some("test-key"), None).await;

    // BUG: protocol=responses on a generic model is not validated at the
    // chain level. After Phase 5 route selection, this must return Err.
    assert!(
        result.is_ok(),
        "R3-RED-04 incompatible-proto: current code INCORRECTLY returns Ok for protocol=responses on a generic model"
    );
}
