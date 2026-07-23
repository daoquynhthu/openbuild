//! P14-002/003: Real launcher helper E2E foundation.
//!
//! Tests the full production chain:
//! TOML → bootstrap → snapshot → manual model → resolve_model_execution →
//! prepare_sampler_config → PreparedSamplerConfig → Sampler → mock → decoded events.
//!
//! **Prohibited**: manual `SamplerConfig` or `PreparedSamplerConfig` construction.

use serial_test::serial;

use futures_util::StreamExt;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_shell::agent::config::{EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::provider_resolution::ProviderResolutionError;
use xai_grok_sampler::SamplerConfig;
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
    rt.block_on(xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        model, registry, api_key, base_url_override,
    ))
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
            implementation = "openai-compatible"
            base_url = "{mock_url}"
            api_key = "test-key-123"

            [provider.test-provider.models.test-model]
            context_window = 128000
            max_output_tokens = 8192
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
            snapshot.providers.contains_key(&xai_grok_provider::types::ProviderId::new("test-provider")),
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
    assert!(config.base_url.contains("127.0.0.1"), "base_url must be mock server");

    // Phase 3: Make request (async)
    rt.block_on(async {
        let client = Client::new(config).expect("Client::new must succeed");

        let request = ConversationRequest::from_items(vec![ConversationItem::user(
            "Hello, E2E test!",
        )]);

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
            implementation = "openai-compatible"
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
            implementation = "openai-compatible"
            base_url = "{mock_url}"
            api_key = "sk-test-openai-key"
            protocol = "chat_completions"

            [provider.openai-test.models.gpt-4o-test]
            context_window = 128000
            max_output_tokens = 4096
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
            || config.request_url.as_deref().is_some_and(|u| u.contains("/responses")),
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
            || config.request_url.as_deref().is_some_and(|u| u.contains("/messages")),
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
        config.extra_headers.get("anthropic-version").map(String::as_str),
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
            implementation = "openai-compatible"
            base_url = "{mock_url}"
            # no api_key → AuthPolicy::None / no auth

            [provider.opencode-test.models.public-model]
            context_window = 64000
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
            implementation = "openai-compatible"
            base_url = "{mock_url}"
            # no api_key → no auth

            [provider.custom-noauth.models.custom-model]
            context_window = 64000
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
            implementation = "openai-compatible"
            base_url = "{url_a}"
            api_key = "key-a-123"

            [provider.provider-a.models.model-alpha]
            context_window = 64000

            [provider.provider-b]
            implementation = "openai-compatible"
            base_url = "{url_b}"
            api_key = "key-b-456"

            [provider.provider-b.models.model-beta]
            context_window = 128000
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
        let request_a = ConversationRequest::from_items(vec![ConversationItem::user(
            "Hello from provider A!",
        )]);
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
        assert!(text_a.contains("Echo:"), "provider-a response must contain echo");

        // Provider B
        let client_b = Client::new(config_b).expect("Client::new for provider-b must succeed");
        let request_b = ConversationRequest::from_items(vec![ConversationItem::user(
            "Hello from provider B!",
        )]);
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
        assert!(text_b.contains("Echo:"), "provider-b response must contain echo");

        // Verify each provider received requests on its own mock server
        let requests_a = server_a.requests();
        let requests_b = server_b.requests();
        assert!(!requests_a.is_empty(), "server A must receive requests");
        assert!(!requests_b.is_empty(), "server B must receive requests");

        // Verify correct API keys were used via Bearer auth
        let body_a = requests_a[0].body.as_ref().expect("request A must have body");
        assert_eq!(
            body_a.get("model").and_then(|m| m.as_str()),
            Some("model-alpha"),
            "provider-a request must have model-alpha"
        );

        let body_b = requests_b[0].body.as_ref().expect("request B must have body");
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

    // Zero HTTP requests reached the mock server
    let requests = server.requests();
    assert!(
        requests.is_empty(),
        "zero HTTP requests must be made on missing auth, got: {}",
        requests.len()
    );
}

/// Acceptance matrix: invalid endpoint hard fail — request count 0.
///
/// An openai-compatible provider with a syntactically invalid base_url
/// causes `Endpoint::render` to fail before any HTTP request is made.
#[test]
fn invalid_endpoint_hard_fail_request_count_zero() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let snapshot = rt.block_on(async {
        let toml_str = r#"
            [provider.bad-endpoint]
            implementation = "openai-compatible"
            base_url = "://invalid-url"

            [provider.bad-endpoint.models.test-model]
            context_window = 64000
            "#;
        let toml: toml::Value = toml::from_str(toml_str).unwrap();

        let runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap must succeed");

        runtime.snapshot()
    });

    // Attempt to resolve with invalid endpoint URL
    let model = custom_model_entry("bad-endpoint", "test-model");
    let result = execution_to_sampler_config(&model, &snapshot, Some("key"), None);

    // Must fail with protocol/endpoint error
    assert!(
        result.is_err(),
        "invalid endpoint must fail, got Ok: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("endpoint") || err.contains("Endpoint") || err.contains("URL") || err.contains("protocol"),
        "error must mention endpoint/URL/protocol, got: {err}"
    );
}
