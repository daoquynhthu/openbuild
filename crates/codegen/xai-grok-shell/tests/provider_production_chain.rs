//! Phase A: Production-entry regression tests.
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
use xai_grok_provider::prepared::prepare_sampler_config;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_shell::agent::config::{EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::provider_resolution::resolve_model_execution;
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
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
    {
        Box::pin(async { Ok(None) })
    }
}

fn model_entry(provider_id: &str, model: &str) -> ModelEntry {
    let mut entry = ModelEntry::fallback(model, &EndpointsConfig::default());
    entry.provider_id = Some(provider_id.to_string());
    entry
}

async fn resolve_prepare(
    model: &ModelEntry,
    snapshot: &RegistrySnapshot,
    api_key: Option<&str>,
) -> xai_grok_sampler::SamplerConfig {
    let execution = resolve_model_execution(model, snapshot, None)
        .expect("resolve_model_execution must succeed");
    let model_inline = api_key.map(|k| SecretValue::new(k.to_string()));
    let env = TestEnv;
    let session = TestSession;
    let creds = RequestCredentialContext::new(None, model_inline.as_ref(), None, &env, &session);
    let headers = RequestHeaderOverrides::new();
    let prepared = prepare_sampler_config(&execution, &creds, &headers)
        .await
        .expect("prepare_sampler_config must succeed");
    xai_grok_sampler::SamplerConfig::from(prepared)
}

async fn bootstrap_async(toml_str: &str) -> Arc<xai_grok_shell::agent::provider_runtime::ProviderRuntime> {
    let toml: toml::Value = toml::from_str(toml_str).unwrap();
    xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
        .await
        .expect("bootstrap must succeed")
}

// ---------------------------------------------------------------------------
// A2.1: OBPA-001 — execution_to_sampler_config panics with nested runtime
// ---------------------------------------------------------------------------
//
// The sync helper calls `Runtime::new().block_on(...)` which panics on
// multi-thread tokio runtimes.
#[tokio::test]
async fn obpa001_nested_runtime_panics() {
    let snapshot = bootstrap_async(r#"
        [provider.test-provider]
        implementation = "openai-compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "test-key"

        [provider.test-provider.models.test-model]
        context_window = 64000
    "#)
    .await
    .snapshot();

    let model = model_entry("test-provider", "test-model");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
            &model,
            &snapshot,
            Some("test-key"),
            None,
        )
    }));

    assert!(
        result.is_err(),
        "execution_to_sampler_config MUST panic inside tokio runtime (OBPA-001)"
    );
}

// ---------------------------------------------------------------------------
// A2.2: OBPA-003/005 — Provider inline key does not appear in request
// ---------------------------------------------------------------------------
#[tokio::test]
async fn obpa003_provider_inline_key_flows_to_auth_header() {
    let server = MockInferenceServer::start().await.unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.custom-key]
        implementation = "openai-compatible"
        base_url = "{mock_url}"
        api_key = "provider-inline-secret"

        [provider.custom-key.models.test-model]
        context_window = 64000
        "#,
    );

    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-key", "test-model");
    let config = resolve_prepare(&model, &snapshot, None).await;

    assert!(
        config.api_key.is_some(),
        "provider inline api_key must flow through to sampler config (OBPA-003)"
    );
    assert_eq!(
        config.api_key.as_deref(),
        Some("provider-inline-secret"),
        "provider inline key must match TOML value"
    );

    let client = xai_grok_sampler::SamplingClient::new(config).unwrap();
    let request = xai_grok_sampling_types::ConversationRequest::from_items(vec![
        xai_grok_sampling_types::ConversationItem::user("hello"),
    ]);
    let (mut stream, _) = client.conversation_stream(request).await.unwrap();
    while let Some(_) = stream.next().await {}

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
        implementation = "openai-compatible"
        base_url = "{mock_url}"
        api_key = "test-key"
        protocol = "responses"

        [provider.custom-protocol.models.test-model]
        context_window = 64000
        "#,
    );

    let snapshot = bootstrap_async(&toml_str).await.snapshot();
    let model = model_entry("custom-protocol", "test-model");

    let execution = resolve_model_execution(&model, &snapshot, None)
        .expect("resolve_model_execution must succeed");

    assert_eq!(
        &*execution.protocol_id.0,
        "responses",
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
    while let Some(_) = stream.next().await {}

    let requests = server.requests();
    let responses_req: Vec<_> = requests.iter().filter(|r| r.path.contains("/responses")).collect();
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
        implementation = "openai-compatible"
        api_key = "test-key"

        [provider.test-provider.models.test-model]
        context_window = 64000
    "#;

    // Bootstrap with CLI override
    let toml: toml::Value = toml::from_str(toml_str).unwrap();
    let runtime = xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(
        &toml, None, Some(cli_override),
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

    // Reload without CLI override (simulates ConfigReloader bug where
    // the ConfigReloader creates a fresh bootstrap without preserving
    // the CLI configuration context).
    let reload_toml: toml::Value = toml::from_str(toml_str).unwrap();
    let reloaded = xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(
        &reload_toml, None, None,
    )
    .await
    .expect("reload must succeed");

    let reloaded_snapshot = reloaded.snapshot();

    // After reload without CLI override, resolve_model_execution may fail
    // (endpoint has no valid URL) or produce a default base_url.
    // Either way, the CLI override is lost — this assertion documents the bug.
    let execution_result = resolve_model_execution(&model, &reloaded_snapshot, None);
    match execution_result {
        Ok(execution) => {
            // Even if execution succeeds, the base_url MUST still contain
            // the CLI override. This is the primary OBPA-008 assertion.
            assert!(
                execution.request_url.as_str().contains("127.0.0.1"),
                "CLI base_url override must survive reload (OBPA-008), got request_url: {}",
                execution.request_url
            );
        }
        Err(_) => {
            // resolve_model_execution fails because the endpoint has no
            // valid base URL after CLI override is lost. This is also
            // a manifestation of OBPA-008.
            panic!("CLI base_url override lost after reload (OBPA-008): resolve_model_execution failed because endpoint URL became invalid");
        }
    }
}
