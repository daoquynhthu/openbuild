use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::types::ProviderId;

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
    assert_eq!(configured.route.id.0, "openai-chat");
    assert_eq!(configured.route.protocol_id, "chat_completions");
    assert_eq!(
        configured.route.endpoint.base_url.as_deref(),
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
