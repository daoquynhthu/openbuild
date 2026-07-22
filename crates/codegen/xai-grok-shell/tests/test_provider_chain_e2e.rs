//! P14-002: Real launcher helper E2E foundation.
//!
//! Tests the full production chain:
//! TOML → bootstrap → snapshot → manual model → resolve_model_execution →
//! prepare_sampler_config → PreparedSamplerConfig → Sampler → mock → decoded events.
//!
//! **Prohibited**: manual `SamplerConfig` or `PreparedSamplerConfig` construction.

use futures_util::StreamExt;
use xai_grok_shell::agent::config::{EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::provider_resolution::execution_to_sampler_config;
use xai_grok_shell::sampling::{Client, ConversationItem, ConversationRequest};
use xai_grok_test_support::MockInferenceServer;

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

    // Phase 2: Route compiler + auth (sync — creates its own runtime internally)
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
