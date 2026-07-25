use indexmap::IndexMap;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::types::ProviderId;

fn configured_for(
    reg: &ProviderRegistry,
    pid: &str,
    mut overrides: ProviderConfig,
) -> xai_grok_provider::provider::ConfiguredProvider {
    overrides.id = Some(pid.into());
    let id = ProviderId::new(pid);
    let provider = reg.get(&id).expect("provider must be registered");
    provider.configure(overrides)
}

fn config_with_extra(
    pid: &str,
    base_url: &str,
    api_key: Option<&str>,
    extra_key: &str,
    extra_val: &str,
) -> ProviderConfig {
    let mut c = ProviderConfig::new(
        Some(pid.into()),
        api_key.map(|k| k.into()),
        Some(base_url.into()),
    );
    let mut h = IndexMap::new();
    h.insert(extra_key.into(), extra_val.into());
    c.extra_headers = Some(h);
    c
}

// ---------------------------------------------------------------------------
// OpenAI
// ---------------------------------------------------------------------------
#[test]
fn openai_extra_headers_merged() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "openai",
        config_with_extra(
            "openai",
            "https://custom.example.com/v1",
            Some("test-key"),
            "X-Custom-OpenAI",
            "openai-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");
    let has_base = route.endpoint.base_url.as_deref() == Some("https://custom.example.com/v1");
    assert!(has_base, "openai base_url must be consumed");

    let has_extra = route.static_headers.contains_key("X-Custom-OpenAI");
    assert!(has_extra, "R3-CFG-09 openai: extra_headers must be present in route");
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------
#[test]
fn anthropic_extra_headers_merged() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "anthropic",
        config_with_extra(
            "anthropic",
            "https://custom.example.com/v1",
            Some("test-key"),
            "X-Custom-Anthropic",
            "anthropic-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");
    let has_extra = route.static_headers.contains_key("X-Custom-Anthropic");
    assert!(has_extra, "R3-CFG-09 anthropic: extra_headers must be present in route");
}

// ---------------------------------------------------------------------------
// xAI
// ---------------------------------------------------------------------------
#[test]
fn xai_extra_headers_merged() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "xai",
        config_with_extra(
            "xai",
            "https://custom.example.com/v1",
            Some("test-key"),
            "X-Custom-xAI",
            "xai-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");
    let has_extra = route.static_headers.contains_key("X-Custom-xAI");
    assert!(has_extra, "R3-CFG-09 xai: extra_headers must be present in route");
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------
#[test]
fn opencode_extra_headers_merged() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "opencode",
        config_with_extra(
            "opencode",
            "https://custom.example.com/v1",
            None,
            "X-Custom-OpenCode",
            "opencode-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");
    let has_extra = route.static_headers.contains_key("X-Custom-OpenCode");
    assert!(has_extra, "R3-CFG-09 opencode: extra_headers must be present in route");
}

// ---------------------------------------------------------------------------
// Ollama
// ---------------------------------------------------------------------------
#[test]
fn ollama_extra_headers_merged() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "ollama",
        config_with_extra(
            "ollama",
            "https://custom.example.com/v1",
            None,
            "X-Custom-Ollama",
            "ollama-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");
    let has_extra = route.static_headers.contains_key("X-Custom-Ollama");
    assert!(has_extra, "R3-CFG-09 ollama: extra_headers must be present in route");
}
