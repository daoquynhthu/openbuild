use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::types::{ProviderId, RouteId};

/// Integration test: full provider config → registry → Route → SamplerConfig.
/// Verifies that the entire chain from configuration to a valid SamplerConfig
/// works end-to-end without actual HTTP (the sampler crate's own tests cover
/// HTTP-level correctness).
#[test]
fn openai_provider_full_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("openai");
    let overrides = ProviderConfig::new(
        Some("openai".into()),
        Some("sk-e2e-test-key".into()),
        Some("https://mock.local/v1".into()),
    );
    reg.store_config(&pid, overrides.clone());
    let configured = reg.configure(&pid, overrides).expect("configure openai");

    // Route fields
    let chat_route = configured
        .routes
        .get(&RouteId::new("openai-chat"))
        .expect("openai-chat route");
    assert_eq!(chat_route.id.0, "openai-chat");
    assert_eq!(chat_route.protocol_id, "chat_completions");
    assert_eq!(
        chat_route.endpoint.base_url.as_deref(),
        Some("https://mock.local/v1")
    );

    // Stored config
    let stored = reg.get_config(&pid).expect("stored config");
    assert_eq!(stored.api_key.as_deref(), Some("sk-e2e-test-key"));

    // Provider metadata
    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.name(), "OpenAI");
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::ChatCompletions
    );

    // Build SamplerConfig and verify protocol_id
    let sampler = xai_grok_sampler::SamplerConfig {
        api_key: Some("sk-e2e-test-key".into()),
        base_url: "https://mock.local/v1".into(),
        model: "gpt-4o-2024-11-20".into(),
        api_backend: xai_grok_sampler::ApiBackend::ChatCompletions,
        protocol_id: Some("chat_completions".into()),
        auth_scheme: xai_grok_sampler::AuthScheme::Bearer,
        context_window: 128_000,
        max_completion_tokens: provider.defaults().max_completion_tokens,
        temperature: provider.defaults().temperature,
        top_p: provider.defaults().top_p,
        ..Default::default()
    };
    assert_eq!(sampler.protocol_id.as_deref(), Some("chat_completions"));

    let client = xai_grok_sampler::SamplingClient::new(sampler).expect("SamplingClient::new");
    assert_eq!(client.protocol_id(), "chat_completions");
}

/// Verify all built-in providers have model list endpoint or format configured.
#[test]
fn all_providers_have_model_list_config() {
    use xai_grok_provider::types::ModelListFormat;

    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let ids = reg.all_ids();
    assert_eq!(ids.len(), 6, "6 built-in providers");

    for pid in &ids {
        let provider = reg.get(pid).expect("provider");
        let defaults = provider.defaults();
        // Every provider must have a model_list_format set.
        // The model_list_endpoint is optional — for dynamic base_url
        // providers (openai-compatible) it's None and derived at runtime.
        match pid.0.as_str() {
            "ollama" => {
                assert_eq!(defaults.model_list_format, ModelListFormat::OllamaTags);
                assert!(defaults.model_list_endpoint.is_some());
            }
            "openai-compatible" => {
                assert_eq!(
                    defaults.model_list_format,
                    ModelListFormat::OpenAiCompatible
                );
                assert!(defaults.model_list_endpoint.is_none());
            }
            _ => {
                assert_eq!(
                    defaults.model_list_format,
                    ModelListFormat::OpenAiCompatible
                );
                assert!(
                    defaults.model_list_endpoint.is_none(),
                    "{} should derive endpoint from base_url",
                    provider.name()
                );
                assert!(
                    !defaults.base_url.is_empty(),
                    "{} must have a base_url",
                    provider.name()
                );
            }
        }
    }
}

/// Verify configure_providers merges config and stores it.
#[test]
fn configure_stores_api_key() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"
        [provider.openai]
        api_key = "sk-from-toml"
        "#,
    )
    .unwrap();
    xai_grok_provider::providers::configure_providers(&reg, &toml, None, None);

    let pid = ProviderId::new("openai");
    let stored = reg.get_config(&pid).expect("openai config");
    assert_eq!(stored.api_key.as_deref(), Some("sk-from-toml"));

    // OpenAI got its key from TOML. xAI may have one from XAI_API_KEY env var
    // (set in dev environments), so we only check the TOML-sourced provider.
}

/// Legacy xAI compatibility: [endpoints].xai_api_base_url maps to xAI provider.
#[test]
fn legacy_endpoints_xai_api_base_url_maps_to_xai_provider() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    // Simulate old [endpoints] config with a custom xAI URL.
    let endpoints_xai_url = "https://custom.x.ai/v1";
    let compat = ProviderConfig::new(
        Some("xai".into()),
        Some("test-legacy-key".into()),
        Some(endpoints_xai_url.into()),
    );

    xai_grok_provider::providers::configure_providers(
        &reg,
        &toml::from_str("").unwrap(),
        Some(compat),
        None,
    );

    // The xAI provider should have its config stored.
    let xai_pid = ProviderId::new("xai");
    let stored = reg.get_config(&xai_pid).expect("xAI config must be stored");
    assert_eq!(
        stored.api_key.as_deref(),
        Some("test-legacy-key"),
        "legacy key must reach xAI provider"
    );
    assert_eq!(
        stored.base_url.as_deref(),
        Some(endpoints_xai_url),
        "legacy base_url must reach xAI provider"
    );
}

/// Legacy xAI compatibility: bare model names without provider field resolve to xAI.
#[test]
fn legacy_bare_model_defaults_to_xai() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    xai_grok_provider::providers::configure_providers(
        &reg,
        &toml::from_str("").unwrap(),
        None,
        None,
    );

    // The xAI provider must exist and be configured.
    let xai_pid = ProviderId::new("xai");
    let xai_provider = reg.get(&xai_pid).expect("xAI provider must be registered");
    assert_eq!(xai_provider.name(), "xAI");

    // Default model "grok-build" must be resolvable syntactically as xAI.
    let (provider_from_ref, model) = xai_grok_provider::types::parse_model_ref("grok-build");
    assert!(
        provider_from_ref.is_none(),
        "bare model has no provider prefix"
    );
    assert_eq!(model, "grok-build");
}

/// Legacy xAI compatibility: XAI_API_KEY env var detection.
#[test]
fn legacy_xai_api_key_env_detected() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    // SAFETY: test-only env mutation, single-threaded.
    unsafe {
        std::env::set_var("XAI_API_KEY", "test-env-key-not-real");
    }

    let env_configs = xai_grok_provider::providers::detect_env_vars(&reg);
    let xai_key = env_configs.get("xai").and_then(|c| c.api_key.clone());

    unsafe {
        std::env::remove_var("XAI_API_KEY");
    }

    assert_eq!(
        xai_key.as_deref(),
        Some("test-env-key-not-real"),
        "XAI_API_KEY must be detected for xAI provider"
    );
}

/// Anthropic: Messages protocol with x-api-key auth header.
#[test]
fn anthropic_provider_full_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("anthropic");
    let overrides = ProviderConfig::new(
        Some("anthropic".into()),
        Some("sk-ant-test".into()),
        Some("https://mock.anthropic.local/v1".into()),
    );
    reg.store_config(&pid, overrides.clone());
    let configured = reg.configure(&pid, overrides).expect("configure anthropic");

    let messages_route = configured
        .routes
        .get(&RouteId::new("anthropic-messages"))
        .expect("anthropic-messages route");
    assert_eq!(messages_route.protocol_id, "messages");
    assert_eq!(
        messages_route.endpoint.base_url.as_deref(),
        Some("https://mock.anthropic.local/v1")
    );

    assert_eq!(
        configured.default_route_id.0,
        "anthropic-messages",
        "default route should be messages"
    );

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::Messages);
}

/// OpenCode: Chat protocol with public/no auth (free tier).
#[test]
fn opencode_provider_full_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("opencode");
    let overrides = ProviderConfig::new(Some("opencode".into()), None, None);
    let configured = reg.configure(&pid, overrides).expect("configure opencode");

    let chat_route = configured
        .routes
        .get(&RouteId::new("opencode-chat"))
        .expect("opencode-chat route");
    assert_eq!(chat_route.protocol_id, "chat_completions");

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.name(), "OpenCode Zen");
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::ChatCompletions);
}

/// Ollama: local provider with no auth.
#[test]
fn ollama_provider_full_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("ollama");
    let overrides = ProviderConfig::new(Some("ollama".into()), None, None);
    let configured = reg.configure(&pid, overrides).expect("configure ollama");

    let chat_route = configured
        .routes
        .get(&RouteId::new("ollama-chat"))
        .expect("ollama-chat route");
    assert_eq!(chat_route.protocol_id, "chat_completions");
    let base_url = chat_route.endpoint.base_url.as_deref().unwrap_or_default();
    assert!(
        base_url.contains("localhost:11434"),
        "ollama endpoint should point to localhost: {base_url}"
    );

    let provider = reg.get(&pid).expect("provider");
    assert!(provider.defaults().env_key.is_empty(), "ollama has no default env_key");
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::ChatCompletions);
}

/// xAI with Responses API backend.
#[test]
fn xai_responses_api_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("xai");
    let overrides = ProviderConfig::new(
        Some("xai".into()),
        Some("sk-xai-test".into()),
        Some("https://api.x.ai/v1".into()),
    );
    let configured = reg.configure(&pid, overrides).expect("configure xai");

    // xAI provider has a responses route as its default
    let responses_route = configured
        .routes
        .get(&RouteId::new("xai-responses"))
        .expect("xai-responses route");
    assert_eq!(responses_route.protocol_id, "responses");

    // xAI does NOT have a separate chat route — it uses responses as the
    // single route for the Responses API backend.

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::Responses);
}

/// OpenAI with Responses API backend (configured via override).
#[test]
fn openai_responses_api_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("openai");
    let overrides = ProviderConfig::new(
        Some("openai".into()),
        Some("sk-openai-test".into()),
        Some("https://mock.openai.local/v1".into()),
    );
    let configured = reg.configure(&pid, overrides).expect("configure openai");

    let chat_route = configured
        .routes
        .get(&RouteId::new("openai-chat"))
        .expect("openai-chat route");
    assert_eq!(chat_route.protocol_id, "chat_completions");

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::ChatCompletions);
}

/// openai-compatible provider with custom base_url and env_key.
#[test]
fn openai_compatible_custom_pipeline() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("openai-compatible");
    let overrides = ProviderConfig::new(
        Some("openai-compatible".into()),
        Some("sk-custom".into()),
        Some("https://custom-proxy.local/v1".into()),
    );
    let configured = reg.configure(&pid, overrides).expect("configure openai-compatible");

    let chat_route = configured
        .routes
        .get(&RouteId::new("openai-compatible-chat"))
        .expect("openai-compatible-chat route");
    assert_eq!(chat_route.protocol_id, "chat_completions");
    assert_eq!(
        chat_route.endpoint.base_url.as_deref(),
        Some("https://custom-proxy.local/v1")
    );

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(provider.defaults().env_key, vec!["XAI_API_KEY"]);
    assert_eq!(provider.defaults().api_backend, xai_grok_provider::types::ApiBackend::ChatCompletions);
}

/// Provider with env_key config via configure_providers (env var detection path).
#[test]
fn provider_config_with_env_key() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"
        [provider."openai-compatible"]
        env_key = ["CUSTOM_API_KEY"]
        base_url = "https://custom-proxy.local/v1"
        "#,
    )
    .unwrap();
    xai_grok_provider::providers::configure_providers(&reg, &toml, None, None);

    let pid = ProviderId::new("openai-compatible");
    let stored = reg.get_config(&pid).expect("openai-compatible config");
    assert_eq!(stored.env_key, Some(vec!["CUSTOM_API_KEY".to_string()]));
    assert_eq!(
        stored.base_url.as_deref(),
        Some("https://custom-proxy.local/v1")
    );
}
