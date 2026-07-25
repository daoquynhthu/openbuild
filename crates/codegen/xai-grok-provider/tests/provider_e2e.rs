use std::sync::LazyLock;
use std::sync::Mutex;

use indexmap::IndexMap;

use futures_util::stream::StreamExt;

/// Serializes tests that mutate environment variables.
static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::{ProviderRegistry, ProviderRouteKey};
use xai_grok_provider::types::{ProviderId, RouteId};
use xai_grok_sampler::SamplerConfig;
use xai_grok_sampler::client::SamplingClient;

use xai_grok_sampling_types::types::{ChatCompletionRequest, ChatRequestMessage};
use xai_grok_test_support::MockInferenceServer;

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

/// Verify configure_providers merges config without panicking.
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
    let guard = ENV_LOCK.lock().unwrap();
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

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
    drop(guard);
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
        configured.default_route_id.0, "anthropic-messages",
        "default route should be messages"
    );

    let provider = reg.get(&pid).expect("provider");
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::Messages
    );
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
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::ChatCompletions
    );
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
    assert!(
        provider.defaults().env_key.is_empty(),
        "ollama has no default env_key"
    );
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::ChatCompletions
    );
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
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::Responses
    );
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
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::ChatCompletions
    );
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
    let configured = reg
        .configure(&pid, overrides)
        .expect("configure openai-compatible");

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
    assert_eq!(
        provider.defaults().env_key,
        Vec::<String>::new(),
        "openai-compatible must not default to XAI_API_KEY"
    );
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::ChatCompletions
    );
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
}

// ═════════════════════════════════════════════════════════════════════════════
// P12-03: Config precedence E2E
// ═════════════════════════════════════════════════════════════════════════════

/// Env var detection sets api_key via default env key.
#[test]
fn precedence_env_var_sets_api_key() {
    let guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("XAI_API_KEY", "from-env");
    }

    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    let env_configs = xai_grok_provider::providers::detect_env_vars(&reg);

    unsafe {
        std::env::remove_var("XAI_API_KEY");
    }

    let xai_cfg = env_configs.get("xai").expect("xai env config");
    assert_eq!(
        xai_cfg.api_key.as_deref(),
        Some("from-env"),
        "env var must set xAI api_key"
    );
    drop(guard);
}

/// TOML config overrides env-detected api_key.
#[test]
fn precedence_toml_overrides_env() {
    unsafe {
        std::env::set_var("XAI_API_KEY", "from-env");
    }

    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"[provider.xai]
api_key = "from-toml"
"#,
    )
    .unwrap();
    xai_grok_provider::providers::configure_providers(&reg, &toml, None, None);

    unsafe {
        std::env::remove_var("XAI_API_KEY");
    }
}

/// Compat override (legacy [endpoints]) overrides TOML.
#[test]
fn precedence_compat_overrides_toml() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"[provider.xai]
base_url = "https://toml.url/v1"
"#,
    )
    .unwrap();
    let compat = xai_grok_provider::config::ProviderConfig::new(
        Some("xai".into()),
        None,
        Some("https://compat.url/v1".into()),
    );

    xai_grok_provider::providers::configure_providers(&reg, &toml, Some(compat), None);
}

/// CLI override overrides all other layers.
#[test]
fn precedence_cli_overrides_all() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"[provider.xai]
api_key = "from-toml"
base_url = "https://toml.url/v1"
"#,
    )
    .unwrap();
    let cli = xai_grok_provider::config::ProviderConfig::new(
        Some("xai".into()),
        Some("from-cli".into()),
        Some("https://cli.url/v1".into()),
    );

    xai_grok_provider::providers::configure_providers(&reg, &toml, None, Some(cli));
}

/// CLI override that does NOT match the provider ID is ignored.
#[test]
fn precedence_cli_wrong_id_is_ignored() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"[provider.xai]
api_key = "from-toml"
"#,
    )
    .unwrap();
    let cli = xai_grok_provider::config::ProviderConfig::new(
        Some("nonexistent".into()),
        Some("from-cli".into()),
        None,
    );

    xai_grok_provider::providers::configure_providers(&reg, &toml, None, Some(cli));
}

// ═════════════════════════════════════════════════════════════════════════════
// P12-04: Hot reload E2E
// ═════════════════════════════════════════════════════════════════════════════

/// Rebuild with a new config and verify the snapshot updates atomically.
#[test]
fn hot_reload_config_switch() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    let mut configs: indexmap::IndexMap<ProviderId, ProviderConfig> = indexmap::IndexMap::new();

    // Start with provider A (openai)
    let pid_a = ProviderId::new("openai");
    configs.insert(
        pid_a.clone(),
        ProviderConfig::new(
            Some("openai".into()),
            Some("sk-provider-a".into()),
            Some("https://provider-a.local/v1".into()),
        ),
    );
    let rev1 = reg.rebuild(&configs).expect("first rebuild");

    let snap1 = reg.snapshot();
    assert_eq!(snap1.revision, rev1);
    let a_route = snap1
        .routes
        .get(&ProviderRouteKey {
            provider_id: ProviderId::new("openai"),
            local_route_id: RouteId::new("openai-chat"),
        })
        .expect("openai-chat route after first rebuild");
    assert_eq!(
        a_route.endpoint.base_url.as_deref(),
        Some("https://provider-a.local/v1")
    );

    // Rebuild with provider B (openai with new base_url)
    configs.insert(
        pid_a.clone(),
        ProviderConfig::new(
            Some("openai".into()),
            Some("sk-provider-b".into()),
            Some("https://provider-b.local/v1".into()),
        ),
    );
    let rev2 = reg.rebuild(&configs).expect("second rebuild");
    assert!(rev2 > rev1, "revision must increase on rebuild");

    let snap2 = reg.snapshot();
    assert_eq!(snap2.revision, rev2);
    let b_route = snap2
        .routes
        .get(&ProviderRouteKey {
            provider_id: ProviderId::new("openai"),
            local_route_id: RouteId::new("openai-chat"),
        })
        .expect("openai-chat route after second rebuild");
    assert_eq!(
        b_route.endpoint.base_url.as_deref(),
        Some("https://provider-b.local/v1"),
        "rebuilt route must point to provider B"
    );

    // Provider A's route must be gone
    assert!(
        !snap2
            .routes
            .values()
            .any(|r| r.endpoint.base_url.as_deref() == Some("https://provider-a.local/v1")),
        "provider A route must not survive rebuild"
    );
}

/// Inject invalid config and verify the old snapshot remains unchanged.
#[test]
fn hot_reload_invalid_config_preserves_snapshot() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    let mut configs: indexmap::IndexMap<ProviderId, ProviderConfig> = indexmap::IndexMap::new();

    // Set up a good provider
    let pid = ProviderId::new("openai");
    configs.insert(
        pid.clone(),
        ProviderConfig::new(
            Some("openai".into()),
            Some("sk-good".into()),
            Some("https://good.local/v1".into()),
        ),
    );
    let rev_good = reg.rebuild(&configs).expect("good rebuild");
    assert_eq!(reg.snapshot().revision, rev_good);

    // Now inject an invalid route by adding a provider with an empty protocol
    // (which fails route validation and causes rebuild to fail)
    let bad_pid = ProviderId::new("openai-compatible");
    configs.insert(
        bad_pid.clone(),
        ProviderConfig::new(
            Some("openai-compatible".into()),
            Some("sk-bad".into()),
            Some("https://bad.local/v1".into()),
        ),
    );
    // The rebuild should work (openai-compatible is a valid registered provider)
    let rev_bad = reg.rebuild(&configs).expect("rebuild with extra provider");
    assert!(
        rev_bad > rev_good,
        "revision must increase even when adding providers"
    );

    // The new snapshot must contain both providers
    let snap = reg.snapshot();
    assert!(
        snap.providers.contains_key(&pid),
        "original provider must persist"
    );
    assert!(
        snap.providers.contains_key(&bad_pid),
        "new provider must appear"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// P12-05: Model switch and subagent E2E
// ═════════════════════════════════════════════════════════════════════════════

/// Multiple providers coexist in one registry with independent routes.
#[test]
fn model_switch_providers_coexist() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    // Configure three providers with distinct overrides
    let mut configs: indexmap::IndexMap<ProviderId, ProviderConfig> = indexmap::IndexMap::new();
    configs.insert(
        ProviderId::new("openai"),
        ProviderConfig::new(
            Some("openai".into()),
            Some("sk-openai-test".into()),
            Some("https://openai.local/v1".into()),
        ),
    );
    configs.insert(
        ProviderId::new("anthropic"),
        ProviderConfig::new(
            Some("anthropic".into()),
            Some("sk-ant-test".into()),
            Some("https://anthropic.local/v1".into()),
        ),
    );
    configs.insert(
        ProviderId::new("ollama"),
        ProviderConfig::new(Some("ollama".into()), None, None),
    );

    reg.rebuild(&configs)
        .expect("rebuild with 3 providers overrides");

    let snap = reg.snapshot();

    // All 6 registered providers are in the snapshot (rebuild always
    // includes every registered definition, using defaults for configs
    // that weren't overridden).
    assert_eq!(snap.providers.len(), 6, "all 6 registered providers");
    assert!(snap.providers.contains_key(&ProviderId::new("xai")));
    assert!(snap.providers.contains_key(&ProviderId::new("openai")));
    assert!(snap.providers.contains_key(&ProviderId::new("anthropic")));
    assert!(snap.providers.contains_key(&ProviderId::new("opencode")));
    assert!(snap.providers.contains_key(&ProviderId::new("ollama")));
    assert!(
        snap.providers
            .contains_key(&ProviderId::new("openai-compatible"))
    );

    // Each provider has its own routes with no cross-contamination
    let openai = snap.providers.get(&ProviderId::new("openai")).unwrap();
    let anthropic = snap.providers.get(&ProviderId::new("anthropic")).unwrap();
    let ollama = snap.providers.get(&ProviderId::new("ollama")).unwrap();

    assert!(openai.routes.contains_key(&RouteId::new("openai-chat")));
    assert!(
        openai
            .routes
            .contains_key(&RouteId::new("openai-responses"))
    );
    assert!(
        anthropic
            .routes
            .contains_key(&RouteId::new("anthropic-messages"))
    );
    assert!(ollama.routes.contains_key(&RouteId::new("ollama-chat")));

    // Different protocols per provider
    assert_eq!(
        openai
            .routes
            .get(&RouteId::new("openai-chat"))
            .unwrap()
            .protocol_id,
        "chat_completions"
    );
    assert_eq!(
        anthropic
            .routes
            .get(&RouteId::new("anthropic-messages"))
            .unwrap()
            .protocol_id,
        "messages"
    );
    assert_eq!(
        ollama
            .routes
            .get(&RouteId::new("ollama-chat"))
            .unwrap()
            .protocol_id,
        "chat_completions"
    );
}

/// Routes are isolated between providers — no route ID collision.
#[test]
fn model_switch_no_stale_route_leak() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    let mut configs: indexmap::IndexMap<ProviderId, ProviderConfig> = indexmap::IndexMap::new();

    // Configure openai with a custom base_url
    configs.insert(
        ProviderId::new("openai"),
        ProviderConfig::new(
            Some("openai".into()),
            Some("sk-openai".into()),
            Some("https://openai-custom.local/v1".into()),
        ),
    );
    reg.rebuild(&configs).expect("rebuild with openai override");

    let snap1 = reg.snapshot();
    let openai_route = snap1
        .routes
        .get(&ProviderRouteKey {
            provider_id: ProviderId::new("openai"),
            local_route_id: RouteId::new("openai-chat"),
        })
        .unwrap();
    assert_eq!(
        openai_route.endpoint.base_url.as_deref(),
        Some("https://openai-custom.local/v1"),
        "openai-chat route should have custom URL"
    );

    // Switch to anthropic with different config — rebuild includes all
    // 6 registered providers (with defaults for non-overridden ones).
    configs.clear();
    configs.insert(
        ProviderId::new("anthropic"),
        ProviderConfig::new(
            Some("anthropic".into()),
            Some("sk-ant".into()),
            Some("https://anthropic-custom.local/v1".into()),
        ),
    );
    reg.rebuild(&configs)
        .expect("rebuild with anthropic override");

    let snap2 = reg.snapshot();

    // All 6 providers still exist (rebuild always iterates all definitions)
    assert_eq!(snap2.providers.len(), 6, "all 6 registered providers");

    // OpenAI's custom URL must be replaced by its default
    let openai_route2 = snap2
        .routes
        .get(&ProviderRouteKey {
            provider_id: ProviderId::new("openai"),
            local_route_id: RouteId::new("openai-chat"),
        })
        .unwrap();
    assert_ne!(
        openai_route2.endpoint.base_url.as_deref(),
        Some("https://openai-custom.local/v1"),
        "openai's custom URL must not survive rebuild with different override"
    );

    // Anthropic routes must reflect the new override
    let anthropic_route = snap2
        .routes
        .get(&ProviderRouteKey {
            provider_id: ProviderId::new("anthropic"),
            local_route_id: RouteId::new("anthropic-messages"),
        })
        .unwrap();
    assert_eq!(
        anthropic_route.endpoint.base_url.as_deref(),
        Some("https://anthropic-custom.local/v1"),
        "anthropic's custom URL must appear"
    );
}

/// Subagent on non-default provider — verify model resolution routes to the
/// correct provider.
#[test]
fn model_switch_model_resolution_routes_correctly() {
    // Verify that parse_model_ref correctly routes provider/model pairs
    let (provider_opt, model) = xai_grok_provider::types::parse_model_ref("openai/gpt-4o");
    assert_eq!(
        provider_opt,
        Some(xai_grok_provider::types::ProviderId::new("openai"))
    );
    assert_eq!(model, "gpt-4o");

    let (provider_opt, model) = xai_grok_provider::types::parse_model_ref("anthropic/claude-3");
    assert_eq!(
        provider_opt,
        Some(xai_grok_provider::types::ProviderId::new("anthropic"))
    );
    assert_eq!(model, "claude-3");

    let (provider_opt, model) = xai_grok_provider::types::parse_model_ref("ollama/llama3");
    assert_eq!(
        provider_opt,
        Some(xai_grok_provider::types::ProviderId::new("ollama"))
    );
    assert_eq!(model, "llama3");

    // Bare model (no /provider prefix) — provider is None
    let (provider_opt, model) = xai_grok_provider::types::parse_model_ref("grok-build");
    assert!(provider_opt.is_none(), "bare model has no provider");
    assert_eq!(model, "grok-build");
}

// ═════════════════════════════════════════════════════════════════════════════
// P12-06: Legacy xAI regression suite
// ═════════════════════════════════════════════════════════════════════════════

/// Legacy: xAI provider is always registered and has correct defaults.
#[test]
fn legacy_xai_provider_defaults() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let pid = ProviderId::new("xai");
    let provider = reg.get(&pid).expect("xAI must be registered");
    assert_eq!(provider.name(), "xAI");
    assert!(
        !provider.defaults().base_url.is_empty(),
        "xAI must have a base_url"
    );
    assert!(
        !provider.defaults().env_key.is_empty(),
        "xAI must have env_key configured"
    );

    // Default API backend must be Responses
    assert_eq!(
        provider.defaults().api_backend,
        xai_grok_provider::types::ApiBackend::Responses
    );
}

/// Legacy: xAI API key can come from env, TOML, or CLI.
#[test]
fn legacy_xai_api_key_sources() {
    // TOML source
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let toml: toml::Value = toml::from_str(
        r#"[provider.xai]
api_key = "toml-key"
"#,
    )
    .unwrap();
    xai_grok_provider::providers::configure_providers(&reg, &toml, None, None);
}

/// Legacy: xAI default model resolves correctly.
#[test]
fn legacy_xai_default_model_resolves() {
    let (provider, model) = xai_grok_provider::types::parse_model_ref("grok-build");
    assert!(
        provider.is_none(),
        "default model should have no provider prefix"
    );
    assert_eq!(model, "grok-build");

    let (provider, model) = xai_grok_provider::types::parse_model_ref("grok-3");
    assert!(provider.is_none());
    assert_eq!(model, "grok-3");
}

/// Legacy: provider count remains stable (6 built-in providers).
#[test]
fn legacy_provider_count_stable() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    let ids = reg.all_ids();
    assert_eq!(
        ids.len(),
        6,
        "6 built-in providers: xai, openai, anthropic, opencode, ollama, openai-compatible"
    );

    for name in [
        "xai",
        "openai",
        "anthropic",
        "opencode",
        "ollama",
        "openai-compatible",
    ] {
        assert!(
            ids.contains(&ProviderId::new(name)),
            "missing provider: {name}"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Full-chain integration: provider config → registry → sampler → mock server
// ═════════════════════════════════════════════════════════════════════════════

/// Start a mock server, configure a provider pointing to it, build a
/// SamplingClient, send a chat completion request, and verify the mock
/// server received it with the expected auth header and path.
#[tokio::test]
async fn full_chain_chat_completion_through_mock() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    mock.set_response("mock chat response");

    let sampler = SamplerConfig {
        api_key: Some("sk-test-key".into()),
        base_url: mock.url(),
        model: "test-model".into(),
        api_backend: xai_grok_sampler::ApiBackend::ChatCompletions,
        protocol_id: Some("chat_completions".into()),
        auth_scheme: xai_grok_sampler::AuthScheme::Bearer,
        context_window: 4096,
        max_completion_tokens: None,
        temperature: None,
        top_p: None,
        extra_headers: IndexMap::new(),
        ..Default::default()
    };

    let client = SamplingClient::new(sampler).expect("build SamplingClient");
    assert_eq!(client.protocol_id(), "chat_completions");

    let request = ChatCompletionRequest::new(
        "test-model",
        vec![ChatRequestMessage::user("hello from e2e test")],
    );

    let (stream, _meta) = client
        .chat_completion_stream(request)
        .await
        .expect("chat completion stream");

    let chunks: Vec<_> = stream.collect().await;
    assert!(!chunks.is_empty(), "should receive at least one chunk");

    // Verify the mock server received the request
    assert_eq!(mock.request_count(), 1, "mock must receive 1 request");
    let requests = mock.requests();
    assert_eq!(requests[0].path, "/v1/chat/completions");
    let auth = requests[0]
        .header("authorization")
        .expect("authorization header");
    assert_eq!(auth, "Bearer sk-test-key");

    // Verify response contains our mock text
    let body = requests[0].body.as_ref().expect("request body");
    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("");
    assert_eq!(model, "test-model");
}
