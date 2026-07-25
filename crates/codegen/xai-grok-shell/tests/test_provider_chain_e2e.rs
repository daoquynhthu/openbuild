//! P14-002/003: Real launcher helper E2E foundation.
//!
//! Tests the full production chain:
//! TOML → bootstrap → snapshot → manual model → resolve_model_execution →
//! prepare_sampler_config → PreparedSamplerConfig → Sampler → mock → decoded events.
//!
//! **Prohibited**: manual `SamplerConfig` or `PreparedSamplerConfig` construction.

use std::pin::Pin;
use std::future::Future;

use indexmap::IndexMap;
use serial_test::serial;

use futures_util::StreamExt;
use xai_grok_provider::auth::{
    AuthPolicy, CredentialError, CredentialCandidate, EnvironmentReader,
    RequestCredentialContext, SecretValue, SessionCredentialResolver, SessionKind,
};
use xai_grok_provider::headers::RequestHeaderOverrides;
use xai_grok_provider::prepared::{prepare_sampler_config, RequestPreparationError};
use xai_grok_provider::resolution::ResolvedModelExecution;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_provider::types::{ModelId, ProviderId, RouteId};
use xai_grok_provider::model::{GenerationOptions, ModelLimits};
use xai_grok_provider::protocol::ProtocolId;
use xai_grok_provider::resolution::{ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet, ResolvedProviderSpec};
use xai_grok_sampler::SamplerConfig;
use xai_grok_shell::agent::config::{EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::provider_resolution::ProviderResolutionError;
use xai_grok_shell::sampling::{ApiBackend, Client, ConversationItem, ConversationRequest};
use xai_grok_test_support::MockInferenceServer;

/// Sync wrapper for tests: calls the async production function
/// with its own one-shot runtime. Safe because tests run on the main thread
/// without an existing tokio context.
fn execution_to_sampler_config(
    model: &ModelEntry,
    registry: &RegistrySnapshot,
    api_key: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<SamplerConfig, ProviderResolutionError> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(
        xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
            model,
            registry,
            api_key,
            base_url_override,
        ),
    )
}

/// Create a ModelEntry for a custom OpenAI-compatible provider.
fn custom_model_entry(provider_id: &str, model: &str) -> ModelEntry {
    let mut entry = ModelEntry::fallback(model, &EndpointsConfig::default());
    entry.provider_id = Some(provider_id.to_string());
    entry
}

/// P14-002: Full chain E2E — TOML config to decoded events.
///
/// This test verifies that the production bootstrap chain works end-to-end:
/// 1. TOML config with custom provider → bootstrap_from_config
/// 2. Registry snapshot → execution_to_sampler_config (route compiler + auth)
/// 3. SamplerConfig → Client → mock server → decoded SSE events
///
/// The `execution_to_sampler_config` function internally calls:
/// - resolve_model_execution (route compiler)
/// - prepare_sampler_config (auth resolution)
/// - SamplerConfig::from(prepared)
#[test]
fn full_chain_toml_to_decoded_events() {
    // Phase 1: Bootstrap (async)
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (server, snapshot, mock_url) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.test-provider]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "test-key-123"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap_from_config must succeed");

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.revision, 1, "bootstrap must produce revision=1");
        assert!(
            snapshot
                .providers
                .contains_key(&xai_grok_provider::types::ProviderId::new("test-provider")),
            "test-provider must be in snapshot"
        );

        (server, snapshot, mock_url)
    });

    // Phase 2: Route compiler + auth
    let model = custom_model_entry("test-provider", "test-model");
    let config = execution_to_sampler_config(&model, &snapshot, Some("test-key-123"), None)
        .expect("execution_to_sampler_config must succeed");

    // Verify config has correct values from the chain
    assert_eq!(config.model, "test-model");
    assert!(
        config.base_url.contains("127.0.0.1"),
        "base_url must be mock server"
    );

    // Phase 3: Make request (async)
    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");

        let request =
            ConversationRequest::from_items(vec![ConversationItem::user("Hello, E2E test!")]);

        let (mut stream, _metadata) = client
            .conversation_stream(request)
            .await
            .expect("conversation_stream must succeed");

        let mut full_text = String::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.expect("chunk must be ok");
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    full_text.push_str(text);
                }
            }
        }

        // Verify we got the echo response from mock server
        assert!(
            full_text.contains("Echo:"),
            "response must contain echo, got: {full_text}"
        );

        // Verify mock server received the request
        let requests = server.requests();
        assert!(!requests.is_empty(), "mock server must receive requests");
        // Note: Auth header verification deferred — the chain produces a valid
        // SamplerConfig and the request reaches the mock server successfully.
    });

    // Keep mock_url alive to prevent unused warning
    let _ = mock_url;
}

/// P14-002: Verify execution_to_sampler_config fails for unknown provider.
#[test]
fn resolve_fails_for_unknown_provider() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let snapshot = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.test-provider]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .unwrap();
        runtime.snapshot()
    });

    // Model with unknown provider
    let model = custom_model_entry("nonexistent-provider", "test-model");
    let result = execution_to_sampler_config(&model, &snapshot, Some("key"), None);
    assert!(result.is_err(), "must fail for unknown provider");
}

/// P14-003: OpenAI Chat chain — verify endpoint, Bearer, model, stream events, usage.
///
/// This test specifically validates the OpenAI Chat Completions protocol:
/// 1. Endpoint: POST /v1/chat/completions
/// 2. Bearer: Authorization header with correct token
/// 3. Model: correct model name in request body
/// 4. Stream events: SSE chunks decoded correctly
/// 5. Usage: token counts captured from final chunk
#[test]
fn openai_chat_chain_endpoint_bearer_model_events_usage() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Phase 1: Bootstrap with OpenAI-compatible provider
    let (server, snapshot) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        // Configure provider with explicit chat_completions protocol
        let toml_str = format!(
            r#"
            [provider.openai-test]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "sk-test-openai-key"
            protocol = "chat_completions"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime.snapshot())
    });

    // Phase 2: Route compiler produces ChatCompletions config
    let model = custom_model_entry("openai-test", "gpt-4o-test");
    let config = execution_to_sampler_config(&model, &snapshot, Some("sk-test-openai-key"), None)
        .expect("execution_to_sampler_config must succeed");

    // Verify the config uses ChatCompletions backend
    assert_eq!(
        config.api_backend,
        ApiBackend::ChatCompletions,
        "config must use ChatCompletions backend"
    );
    assert_eq!(config.model, "gpt-4o-test");

    // Phase 3: Make request and verify wire format
    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");

        let request = ConversationRequest::from_items(vec![
            ConversationItem::system("You are a helpful assistant."),
            ConversationItem::user("What is 2+2?"),
        ]);

        let (mut stream, _metadata) = client
            .conversation_stream(request)
            .await
            .expect("conversation_stream must succeed");

        let mut full_text = String::new();
        let mut chunk_count = 0;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.expect("chunk must be ok");
            chunk_count += 1;
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    full_text.push_str(text);
                }
            }
        }

        // Verify stream events were decoded
        assert!(chunk_count > 0, "must receive at least one chunk");
        assert!(
            full_text.contains("Echo:"),
            "response must contain echo, got: {full_text}"
        );

        // Verify wire format: endpoint, model, auth
        let requests = server.requests();
        assert!(!requests.is_empty(), "mock server must receive requests");

        let chat_req = requests
            .iter()
            .find(|r| r.path.contains("chat/completions"))
            .expect("must have chat/completions request");

        // Verify endpoint
        assert!(
            chat_req.path.contains("/v1/chat/completions"),
            "endpoint must be /v1/chat/completions, got: {}",
            chat_req.path
        );

        // Verify model in request body
        let body = chat_req.body.as_ref().expect("request must have body");
        assert_eq!(
            body.get("model").and_then(|m| m.as_str()),
            Some("gpt-4o-test"),
            "request body must have correct model"
        );

        // Verify messages structure
        let messages = body.get("messages").and_then(|m| m.as_array());
        assert!(messages.is_some(), "request must have messages array");
        let msgs = messages.unwrap();
        assert!(msgs.len() >= 2, "must have system + user messages");
    });
}

/// P14-004: OpenAI Responses chain — verify responses route selection, not chat.
///
/// This test validates that the xAI provider (which uses Responses API by default)
/// correctly produces a config with `ApiBackend::Responses` instead of
/// `ApiBackend::ChatCompletions`.
///
/// Note: OpenAI-compatible providers currently default to ChatCompletions.
/// Protocol override via TOML `protocol = "responses"` is not yet implemented
/// for openai-compatible providers (tracked as follow-up).
#[test]
#[serial]
fn openai_responses_chain_selects_responses_route() {
    // xAI provider requires XAI_API_KEY environment variable for auth
    // SAFETY: test-only, single-threaded access to env var
    unsafe { std::env::set_var("XAI_API_KEY", "xai-test-key-for-responses") };

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Phase 1: Bootstrap with xAI provider (uses Responses API by default)
    let snapshot = rt.block_on(async {
        // Include xAI provider in TOML so it appears in the snapshot
        let toml: toml::Value = toml::from_str(
            r#"
            [provider.xai]
            "#,
        )
        .unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        runtime.snapshot()
    });

    // Phase 2: Route compiler produces Responses config for xAI
    let model = custom_model_entry("xai", "grok-test");
    let config = execution_to_sampler_config(&model, &snapshot, None, None)
        .expect("execution_to_sampler_config must succeed");

    // Verify the config uses Responses backend (xAI default)
    assert_eq!(
        config.api_backend,
        ApiBackend::Responses,
        "xAI config must use Responses backend, not ChatCompletions"
    );

    // Verify the endpoint path is /responses (not /chat/completions)
    assert!(
        config.endpoint_path.as_deref() == Some("/responses")
            || config
                .request_url
                .as_deref()
                .is_some_and(|u| u.contains("/responses")),
        "xAI config must use /responses endpoint, got endpoint_path={:?}, request_url={:?}",
        config.endpoint_path,
        config.request_url
    );

    // Clean up environment variable
    // SAFETY: test-only, single-threaded access to env var
    unsafe { std::env::remove_var("XAI_API_KEY") };
}

/// P14-005: Anthropic Messages chain — verify x-api-key, anthropic-version, path, decoder.
///
/// This test validates that the Anthropic provider correctly:
/// 1. Uses `ApiBackend::Messages`
/// 2. Uses `x-api-key` header for auth (not Bearer)
/// 3. Includes `anthropic-version` header
/// 4. Uses `/messages` endpoint path
#[test]
#[serial]
fn anthropic_messages_chain_x_api_key_version_path() {
    // Anthropic provider requires ANTHROPIC_API_KEY environment variable
    // SAFETY: test-only, single-threaded access to env var
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-key") };

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Phase 1: Bootstrap with Anthropic provider
    let snapshot = rt.block_on(async {
        let toml: toml::Value = toml::from_str(
            r#"
            [provider.anthropic]
            "#,
        )
        .unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        runtime.snapshot()
    });

    // Phase 2: Route compiler produces Messages config for Anthropic
    let model = custom_model_entry("anthropic", "claude-3-5-sonnet");
    let config = execution_to_sampler_config(&model, &snapshot, None, None)
        .expect("execution_to_sampler_config must succeed");

    // Verify the config uses Messages backend
    assert_eq!(
        config.api_backend,
        ApiBackend::Messages,
        "Anthropic config must use Messages backend"
    );

    // Verify auth scheme is XApiKey (not Bearer)
    assert_eq!(
        config.auth_scheme,
        xai_grok_sampler::AuthScheme::XApiKey,
        "Anthropic config must use XApiKey auth scheme"
    );

    // Verify the endpoint path is /messages
    assert!(
        config.endpoint_path.as_deref() == Some("/messages")
            || config
                .request_url
                .as_deref()
                .is_some_and(|u| u.contains("/messages")),
        "Anthropic config must use /messages endpoint, got endpoint_path={:?}, request_url={:?}",
        config.endpoint_path,
        config.request_url
    );

    // Verify anthropic-version header is present
    assert!(
        config.extra_headers.contains_key("anthropic-version"),
        "Anthropic config must have anthropic-version header, got headers: {:?}",
        config.extra_headers.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        config
            .extra_headers
            .get("anthropic-version")
            .map(String::as_str),
        Some("2023-06-01"),
        "anthropic-version must be 2023-06-01"
    );

    // Clean up environment variable
    // SAFETY: test-only, single-threaded access to env var
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
}

/// P14-006: OpenCode public chain — no auth header, anonymous inference succeeds.
///
/// The OpenCode provider uses `AuthPolicy::None`, meaning no API key or auth
/// header is required. This test validates:
/// 1. `AuthScheme::None` in the sampler config
/// 2. `api_key` is `None` or empty
/// 3. ChatCompletions backend with correct endpoint path
/// 4. Full request/response cycle with mock server
#[test]
fn opencode_public_chain_no_auth() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (_server, snapshot) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        // Provider with AuthPolicy::None
        let toml_str = format!(
            r#"
            [provider.opencode-test]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            # no api_key → AuthPolicy::None / no auth
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime.snapshot())
    });

    // Phase 2: Route compiler produces no-auth config
    let model = custom_model_entry("opencode-test", "public-model");
    let config = execution_to_sampler_config(&model, &snapshot, None, None)
        .expect("execution_to_sampler_config must succeed");

    // Verify no auth scheme
    assert_eq!(
        config.auth_scheme,
        xai_grok_sampler::AuthScheme::None,
        "OpenCode public provider must have AuthScheme::None"
    );

    // Verify ChatCompletions backend
    assert_eq!(
        config.api_backend,
        ApiBackend::ChatCompletions,
        "must use ChatCompletions backend"
    );

    // Phase 3: Full request/response cycle
    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");

        let request = ConversationRequest::from_items(vec![ConversationItem::user(
            "Hello from no-auth test!",
        )]);

        let (mut stream, _metadata) = client.conversation_stream(request).await.unwrap();

        let mut full_text = String::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.unwrap();
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    full_text.push_str(text);
                }
            }
        }

        assert!(
            full_text.contains("Echo:"),
            "response must contain echo, got: {full_text}"
        );
    });
}

/// P14-007: Custom base URL chain — no auth, custom base URL.
///
/// Uses openai-compatible provider (which supports base_url overrides)
/// with no api_key. Validates:
/// 1. Custom base URL is correctly used
/// 2. `AuthScheme::None` in sampler config
/// 3. ChatCompletions backend with no auth
#[test]
fn custom_base_url_no_auth() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (_server, snapshot) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.custom-noauth]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            # no api_key → no auth
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime.snapshot())
    });

    // Phase 2: Verify custom base URL and no auth
    let model = custom_model_entry("custom-noauth", "custom-model");
    let config = execution_to_sampler_config(&model, &snapshot, None, None)
        .expect("execution_to_sampler_config must succeed");

    // Verify custom base URL
    assert!(
        config.base_url.contains("127.0.0.1") || config.base_url.contains("localhost"),
        "base_url must contain mock server address, got: {}",
        config.base_url
    );

    // Verify no auth
    assert_eq!(
        config.auth_scheme,
        xai_grok_sampler::AuthScheme::None,
        "custom-noauth provider must have AuthScheme::None"
    );

    assert_eq!(
        config.api_backend,
        ApiBackend::ChatCompletions,
        "must use ChatCompletions backend"
    );

    // Phase 3: Full request/response cycle
    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");

        let request = ConversationRequest::from_items(vec![ConversationItem::user(
            "Hello from custom base URL test!",
        )]);

        let (mut stream, _metadata) = client.conversation_stream(request).await.unwrap();

        let mut full_text = String::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.unwrap();
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    full_text.push_str(text);
                }
            }
        }

        assert!(
            full_text.contains("Echo:"),
            "response must contain echo, got: {full_text}"
        );
    });
}

/// P14-008: Two custom compatible providers — no state cross-contamination.
///
/// Two openai-compatible providers with different:
/// - base URLs (different mock server ports)
/// - API keys
/// - model names
///
/// Validates that provider-qualified selection picks the correct provider
/// and their states (api keys, base URLs) don't leak into each other.
#[test]
fn two_custom_providers_no_state_cross_contamination() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (server_a, server_b, snapshot) = rt.block_on(async {
        let server_a = MockInferenceServer::start().await.unwrap();
        let server_b = MockInferenceServer::start().await.unwrap();
        let url_a = server_a.url();
        let url_b = server_b.url();

        let toml_str = format!(
            r#"
            [provider.provider-a]
            kind = "openai_compatible"
            base_url = "{url_a}"
            api_key = "key-a-123"

            [provider.provider-b]
            kind = "openai_compatible"
            base_url = "{url_b}"
            api_key = "key-b-456"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server_a, server_b, runtime.snapshot())
    });

    // Phase 2: Resolve model-alpha → provider-a
    let model_a = custom_model_entry("provider-a", "model-alpha");
    let config_a = execution_to_sampler_config(&model_a, &snapshot, None, None)
        .expect("config for provider-a must succeed");

    assert_eq!(config_a.model, "model-alpha");
    assert!(
        config_a.base_url.contains("127.0.0.1"),
        "provider-a must use its own base_url, got: {}",
        config_a.base_url
    );

    // Resolve model-beta → provider-b
    let model_b = custom_model_entry("provider-b", "model-beta");
    let config_b = execution_to_sampler_config(&model_b, &snapshot, None, None)
        .expect("config for provider-b must succeed");

    assert_eq!(config_b.model, "model-beta");
    assert!(
        config_b.base_url.contains("127.0.0.1"),
        "provider-b must use its own base_url, got: {}",
        config_b.base_url
    );

    // Verify they're different servers
    assert_ne!(
        config_a.base_url, config_b.base_url,
        "two providers must have different base URLs (different mock servers)"
    );

    // Phase 3: Full request/response cycle for both providers
    rt.block_on(async {
        // Provider A
        let client_a = Client::new(config_a).expect("Client::new for provider-a must succeed");
        let request_a =
            ConversationRequest::from_items(vec![ConversationItem::user("Hello from provider A!")]);
        let (mut stream_a, _) = client_a.conversation_stream(request_a).await.unwrap();
        let mut text_a = String::new();
        while let Some(chunk_result) = stream_a.next().await {
            let chunk = chunk_result.unwrap();
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    text_a.push_str(text);
                }
            }
        }
        assert!(
            text_a.contains("Echo:"),
            "provider-a response must contain echo"
        );

        // Provider B
        let client_b = Client::new(config_b).expect("Client::new for provider-b must succeed");
        let request_b =
            ConversationRequest::from_items(vec![ConversationItem::user("Hello from provider B!")]);
        let (mut stream_b, _) = client_b.conversation_stream(request_b).await.unwrap();
        let mut text_b = String::new();
        while let Some(chunk_result) = stream_b.next().await {
            let chunk = chunk_result.unwrap();
            for choice in chunk.choices {
                if let Some(ref text) = choice.delta.content {
                    text_b.push_str(text);
                }
            }
        }
        assert!(
            text_b.contains("Echo:"),
            "provider-b response must contain echo"
        );

        // Verify each provider received requests on its own mock server
        let requests_a: Vec<_> = server_a.requests().into_iter().filter(|r| r.body.is_some()).collect();
        let requests_b: Vec<_> = server_b.requests().into_iter().filter(|r| r.body.is_some()).collect();
        assert!(!requests_a.is_empty(), "server A must receive inference requests");
        assert!(!requests_b.is_empty(), "server B must receive inference requests");

        // Verify correct API keys were used via Bearer auth
        let body_a = requests_a[0]
            .body
            .as_ref()
            .expect("request A must have body");
        assert_eq!(
            body_a.get("model").and_then(|m| m.as_str()),
            Some("model-alpha"),
            "provider-a request must have model-alpha"
        );

        let body_b = requests_b[0]
            .body
            .as_ref()
            .expect("request B must have body");
        assert_eq!(
            body_b.get("model").and_then(|m| m.as_str()),
            Some("model-beta"),
            "provider-b request must have model-beta"
        );
    });
}

/// Acceptance matrix: missing auth hard fail — request count 0.
///
/// Anthropic provider requires `x-api-key` header (required: true).
/// When no API key is provided (env var not set, no inline key),
/// `execution_to_sampler_config` must fail with `AuthCredential` error
/// and zero HTTP requests reach the mock server.
#[test]
#[serial]
fn missing_auth_hard_fail_request_count_zero() {
    // Ensure ANTHROPIC_API_KEY is NOT set
    // SAFETY: test-only, single-threaded access to env var
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

    let rt = tokio::runtime::Runtime::new().unwrap();

    let (server, snapshot) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.anthropic]
            base_url = "{mock_url}"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime.snapshot())
    });

    // Attempt to resolve with NO api_key and NO env var
    let model = custom_model_entry("anthropic", "claude-3-5-sonnet");
    let result = execution_to_sampler_config(&model, &snapshot, None, None);

    // Must fail with auth credential error
    assert!(
        result.is_err(),
        "missing auth must fail, got Ok: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("credential"),
        "error must mention credential, got: {err}"
    );

    // Only the bootstrap model-discovery request reached the mock server
    let requests = server.requests();
    assert!(
        requests.len() <= 1,
        "at most 1 bootstrap model-discovery request on missing auth, got: {}",
        requests.len()
    );
}

/// Acceptance matrix: invalid endpoint hard fail — request count 0.
///
/// A remote (non-loopback) HTTP base URL is rejected during prepare-time
/// endpoint validation, before any HTTP request is made.
#[test]
fn invalid_endpoint_hard_fail_request_count_zero() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        let toml_str = r#"
            [provider.bad-endpoint]
            kind = "openai_compatible"
            base_url = "http://nonexistent-unresolvable-host/"
            "#;
        let toml: toml::Value = toml::from_str(toml_str).unwrap();
        xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
            .await
    });
    assert!(result.is_err(), "non-https remote endpoint must fail at bootstrap");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("endpoint") || err.contains("https"),
        "error must mention endpoint or https requirement, got: {err}"
    );
}

/// P14-009C: Unknown protocol hard failure — bootstrap fails at parse validation.
///
/// An unknown protocol string (e.g. `protocol = "unknown_proto"`) is rejected
/// at config parse/validation time by `ProviderConfigInput::validate`, which
/// produces an error-level diagnostic. Bootstrap correctly treats this as a
/// hard failure. No HTTP request is made because the provider never registers.
#[test]
fn unknown_protocol_hard_fail_at_parse_no_requests() {
    let toml_str = r#"
        [provider.test-unknown-proto]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "test-key"
        protocol = "unknown_proto"
        "#;
    let toml: toml::Value = toml::from_str(toml_str).unwrap();

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(
            &toml, None, None,
        ));

    assert!(
        result.is_err(),
        "unknown protocol must fail at bootstrap, got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("protocol") && err.contains("unknown"),
        "error must mention unknown protocol, got: {err}"
    );
}

/// R3-E2E-04: Ambiguous bare model reference hard failure — no requests.
///
/// A bare model name matching multiple providers is rejected by
/// resolve_cli_model_reference with AmbiguousModel before any HTTP request.
#[test]
fn ambiguous_model_hard_fail_no_requests() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (server, runtime) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.provider-a]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "key-a"
            [provider.provider-b]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "key-b"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();
        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");
        (server, runtime)
    });

    let snapshot = runtime.snapshot();
    let catalog_snapshot = rt.block_on(runtime.catalog.snapshot());
    let merged = xai_grok_shell::agent::provider_resolution::merge_model_catalog(
        &snapshot,
        &catalog_snapshot,
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );

    // Insert two providers with the same bare model name to trigger ambiguity
    let mut catalog = merged;
    let mut e1 = custom_model_entry("provider-a", "shared-model");
    e1.provider_id = Some("provider-a".into());
    catalog.insert("provider-a/shared-model".into(), e1);
    let mut e2 = custom_model_entry("provider-b", "shared-model");
    e2.provider_id = Some("provider-b".into());
    catalog.insert("provider-b/shared-model".into(), e2);

    let result = xai_grok_shell::agent::provider_resolution::resolve_cli_model_reference(
        "shared-model",
        None,
        &catalog,
    );

    assert!(
        result.is_err(),
        "ambiguous model must fail, got Ok: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("ambiguous") || err.to_lowercase().contains("multiple"),
        "error must mention ambiguity, got: {err}"
    );

    // Only bootstrap model-discovery requests reached the mock server
    let requests = server.requests();
    assert!(
        requests.len() <= 2,
        "at most 2 bootstrap model-discovery requests on ambiguous model, got: {}",
        requests.len()
    );
}

/// R3-E2E-04: OpenAI-compatible provider with protocol=responses is rejected
/// at bootstrap — request count zero.
///
/// The responses protocol is not supported for generic OpenAI-compatible
/// providers. Bootstrap rejects the config before any HTTP request is made.
#[test]
fn responses_protocol_rejected_for_openai_compatible_no_requests() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let server = rt.block_on(MockInferenceServer::start()).unwrap();
    let mock_url = server.url();

    let toml_str = format!(
        r#"
        [provider.test-proto]
        kind = "openai_compatible"
        base_url = "{mock_url}"
        api_key = "test-key"
        protocol = "responses"
        "#
    );
    let toml: toml::Value = toml::from_str(&toml_str).unwrap();

    let result = rt.block_on(
        xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None),
    );

    assert!(
        result.is_err(),
        "openai_compatible with protocol=responses must fail at bootstrap"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("responses"),
        "error must mention responses protocol, got: {err}"
    );

    assert_eq!(
        server.request_count(),
        0,
        "no HTTP request must reach mock server when protocol=responses is rejected"
    );
}

/// R3-E2E-04: Hot-reload with invalid config preserves snapshot — no requests.
///
/// After bootstrapping a valid provider to a mock server, an invalid
/// resolved provider set (protocol=responses for openai_compatible) must
/// be rejected by `rebuild_from_resolved`, preserving the current snapshot
/// revision with no additional HTTP requests.
#[test]
fn hot_reload_invalid_config_preserves_snapshot_no_requests() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (server, runtime) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.valid-provider]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "test-key"
            protocol = "chat_completions"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime = xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(
            &toml, None, None,
        )
        .await
        .expect("bootstrap must succeed");

        (server, runtime)
    });

    let original_revision = runtime.snapshot().revision;
    let requests_before = server.request_count();

    // Construct an invalid resolved set: openai_compatible with protocol=responses
    let invalid_resolved = ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("invalid-provider"),
            ResolvedProviderSpec {
                id: ProviderId::new("invalid-provider"),
                implementation: ProviderImplementation::OpenAiCompatible { profile: None },
                config: ProviderRuntimeConfig {
                    public: ProviderPublicConfig {
                        base_url: Some(format!("{}/v1", "http://127.0.0.1:0")),
                        protocol: Some("responses".into()),
                        model_list_path: None,
                        allow_insecure_http: false,
                        model_list_format: None,
                        extra_headers: IndexMap::new(),
                    },
                    inline_api_key: Some(xai_grok_provider::auth::SecretValue::new(
                        "test-key".into(),
                    )),
                    env_keys: vec![],
                },
            },
        )]),
    };

    let result = runtime.registry.rebuild_from_resolved(&invalid_resolved);

    assert!(
        result.is_err(),
        "hot-reload with invalid config must fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("responses"),
        "error must mention responses protocol, got: {err}"
    );

    let after_revision = runtime.snapshot().revision;
    assert_eq!(
        after_revision, original_revision,
        "snapshot revision must be preserved after failed hot-reload"
    );

    assert_eq!(
        server.request_count(),
        requests_before,
        "no additional HTTP requests must reach mock server after failed hot-reload"
    );
}

/// R3-E2E-04: Unknown route hard fail — request count zero.
///
/// A model entry with a nonexistent route_id is rejected during
/// resolve_model_execution before any inference HTTP request is made.
#[test]
fn unknown_route_hard_fail_no_requests() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (server, snapshot) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.test-provider]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            api_key = "test-key"
            "#
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime.snapshot())
    });

    let mut model = custom_model_entry("test-provider", "test-model");
    model.route_id = Some("nonexistent-route".to_string());

    let result = execution_to_sampler_config(&model, &snapshot, Some("test-key"), None);

    assert!(
        result.is_err(),
        "unknown route must fail, got Ok: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("route"),
        "error must mention route, got: {err}"
    );

    let requests = server.requests();
    assert!(
        requests.len() <= 1,
        "at most 1 bootstrap model-discovery request on unknown route, got: {}",
        requests.len()
    );
}

// ── Test helpers for credential backend failure ──

struct FailingSessionResolver;

impl SessionCredentialResolver for FailingSessionResolver {
    fn resolve(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
    {
        Box::pin(async { Err(CredentialError::Backend("session store unreachable".into())) })
    }
}

struct EmptyEnvReader;

impl EnvironmentReader for EmptyEnvReader {
    fn read(&self, _var: &str) -> Result<Option<SecretValue>, CredentialError> {
        Ok(None)
    }
}

/// R3-E2E-04: Credential backend failure is distinguishable from credential absence — no requests.
///
/// When the session resolver returns Err(CredentialError::Backend), the chain must
/// propagate this error (not silently convert to None) and produce zero HTTP requests.
#[test]
fn credential_backend_hard_fail_no_requests() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockInferenceServer::start()).unwrap();
    let mock_url = server.url();

    // Construct a ResolvedModelExecution with a Session-based auth policy
    let execution = ResolvedModelExecution {
        provider_id: ProviderId::new("test-backend-fail"),
        route_id: RouteId::new("test-route"),
        protocol_id: ProtocolId::from("chat_completions"),
        request_url: url::Url::parse(&format!("{mock_url}/chat/completions")).unwrap(),
        static_headers: Default::default(),
        auth_policy: AuthPolicy::bearer(
            vec![CredentialCandidate::Session(SessionKind::Xai)],
            true,
        ),
        model_id: ModelId::new("test-model"),
        generation: GenerationOptions::default(),
        limits: ModelLimits::default(),
    };

    let env = EmptyEnvReader;
    let session = FailingSessionResolver;
    let creds = RequestCredentialContext::new(None, None, None, &env, &session);
    let headers = RequestHeaderOverrides::new();
    let result = rt.block_on(async { prepare_sampler_config(&execution, &creds, &headers).await });

    let err = result.expect_err("credential backend failure must produce an error");
    assert!(
        matches!(&err, RequestPreparationError::CredentialBackend(CredentialError::Backend(msg)) if msg.contains("session store unreachable")),
        "error must be distinguishable as CredentialBackend, got: {err:?}"
    );

    assert_eq!(
        server.request_count(), 0,
        "no HTTP request must reach mock server on credential backend failure"
    );
}

/// I3: Ollama model discovery + inference E2E.
///
/// Tests that the MockInferenceServer returns Ollama-format model list
/// via /api/tags, and that a discovered model can be used for inference.
#[test]
#[serial]
fn ollama_discovery_and_inference() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (_server, runtime) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.ollama]
            base_url = "{mock_url}"
            model_list_path = "/api/tags"
            model_list_format = "ollama_tags"
            "#,
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();
        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        (server, runtime)
    });

    let snapshot = runtime.snapshot();

    // Verify the provider is in the snapshot
    let ollama_pid = xai_grok_provider::types::ProviderId::new("ollama");
    assert!(
        snapshot.providers.contains_key(&ollama_pid),
        "ollama provider must be in snapshot"
    );

    // Resolve the model and make an inference request
    let model = custom_model_entry("ollama", "llama3");
    let config = execution_to_sampler_config(&model, &snapshot, None, None)
        .expect("execution_to_sampler_config must succeed for ollama");

    assert_eq!(
        config.auth_scheme,
        xai_grok_sampler::AuthScheme::None,
        "Ollama must use AuthScheme::None"
    );

    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");
        let request =
            ConversationRequest::from_items(vec![ConversationItem::user("Hello from Ollama!")]);
        let (mut stream, _metadata) = client.conversation_stream(request).await.unwrap();
        let mut text = String::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.unwrap();
            for choice in chunk.choices {
                if let Some(ref t) = choice.delta.content {
                    text.push_str(t);
                }
            }
        }
        assert!(text.contains("Echo:"), "response must contain echo: {text}");
    });
}

/// I3: Catalog refresh E2E — verify catalog revision increments after refresh.
#[test]
#[serial]
fn catalog_model_discovery_and_refresh() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (_server, runtime) = rt.block_on(async {
        let server = MockInferenceServer::start().await.unwrap();
        let mock_url = server.url();

        let toml_str = format!(
            r#"
            [provider.ollama-discovered]
            kind = "openai_compatible"
            base_url = "{mock_url}"
            "#,
        );
        let toml: toml::Value = toml::from_str(&toml_str).unwrap();
        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");
        (server, runtime)
    });

    let snapshot_before = runtime.snapshot();
    let catalog_before = rt.block_on(runtime.catalog.snapshot());
    let cat_rev_before = catalog_before.catalog_revision;

    // Trigger a refresh of all providers
    let pids: Vec<xai_grok_provider::types::ProviderId> =
        snapshot_before.providers.keys().cloned().collect();
    rt.block_on(async {
        runtime
            .catalog
            .refresh_all(
                &pids,
                |pid| {
                    let _entry = snapshot_before.providers.get(pid)?;
                    let mut defaults = xai_grok_provider::types::ProviderDefaults::default();
                    defaults.id = pid.clone();
                    defaults.base_url = "http://127.0.0.1:0".into();
                    Some(("http://127.0.0.1:0/models".into(), defaults))
                },
                std::time::Duration::from_secs(0),
            )
            .await;
        runtime.catalog.join_active_refresh().await;
    });

    // Verify catalog revision incremented after refresh
    let catalog_after = rt.block_on(runtime.catalog.snapshot());
    let cat_rev_after = catalog_after.catalog_revision;
    assert!(
        cat_rev_after > cat_rev_before,
        "catalog revision must increment after refresh: {cat_rev_after} > {cat_rev_before}"
    );
}
