use indexmap::IndexMap;
use xai_grok_provider::auth::AuthPolicy;
use xai_grok_provider::auth::CredentialCandidate;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::protocol::ProtocolId;
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

fn config_with_full_fields(
    pid: &str,
    base_url: &str,
    api_key: Option<&str>,
    env_key: Option<&[String]>,
    extra_key: &str,
    extra_val: &str,
) -> ProviderConfig {
    let mut c = ProviderConfig::new(
        Some(pid.into()),
        api_key.map(|k| k.into()),
        Some(base_url.into()),
    );
    c.env_key = env_key.map(|v| v.to_vec());
    let mut h = IndexMap::new();
    h.insert(extra_key.into(), extra_val.into());
    c.extra_headers = Some(h);
    c
}

fn provider_inline_candidates<'a>(auth: &'a AuthPolicy) -> Vec<&'a CredentialCandidate> {
    let candidates = match auth {
        AuthPolicy::Bearer { candidates, .. } => candidates,
        AuthPolicy::Header { candidates, .. } => candidates,
        AuthPolicy::None => return Vec::new(),
    };
    candidates
        .iter()
        .filter(|c| matches!(c, CredentialCandidate::ProviderInline))
        .collect()
}

fn provider_env_candidates<'a>(auth: &'a AuthPolicy) -> Vec<&'a Vec<String>> {
    let candidates = match auth {
        AuthPolicy::Bearer { candidates, .. } => candidates,
        AuthPolicy::Header { candidates, .. } => candidates,
        AuthPolicy::None => return Vec::new(),
    };
    candidates
        .iter()
        .filter_map(|c| {
            if let CredentialCandidate::ProviderEnvironment(keys) = c {
                Some(keys)
            } else {
                None
            }
        })
        .collect()
}

fn assert_field_fidelity(
    configured: &xai_grok_provider::provider::ConfiguredProvider,
    expected_base_url: &str,
    expected_api_key: Option<&str>,
    expected_env_key: Option<&[String]>,
    expected_extra_key: &str,
    expected_extra_val: &str,
    expected_protocol: &str,
) {
    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");

    // base_url must be reflected in endpoint
    let actual_base = route.endpoint.base_url.as_deref();
    assert!(
        actual_base == Some(expected_base_url),
        "base_url: expected `{expected_base_url}`, got `{actual_base:?}`"
    );

    // api_key must produce ProviderInline credential candidate
    let inline = provider_inline_candidates(&route.auth);
    if expected_api_key.is_some() {
        assert!(
            !inline.is_empty(),
            "api_key override must produce ProviderInline candidate"
        );
    } else {
        assert!(
            inline.is_empty(),
            "no api_key must not produce ProviderInline candidate"
        );
    }

    // env_key must produce ProviderEnvironment credential candidate
    let env_candidates = provider_env_candidates(&route.auth);
    if let Some(keys) = expected_env_key {
        let has_keys = env_candidates.iter().any(|c| c.as_slice() == keys);
        assert!(
            has_keys,
            "env_key override must produce ProviderEnvironment candidate with keys {keys:?}"
        );
    }

    // extra_headers must be reflected in static_headers
    let has_extra = route.static_headers.contains_key(expected_extra_key);
    assert!(
        has_extra,
        "extra_headers must be present in route static_headers"
    );
    let extra_val = route.static_headers.get(expected_extra_key);
    assert_eq!(
        extra_val,
        Some(&expected_extra_val.to_owned()),
        "extra_headers value mismatch"
    );

    // protocol_id must not be default
    let pid = ProtocolId::from(expected_protocol);
    assert_eq!(
        route.protocol_id, pid,
        "protocol_id: expected `{expected_protocol}`, got `{:?}`",
        route.protocol_id
    );
}

// OpenAI tests ---------------------------------------------------------------

#[test]
fn openai_config_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let env_keys = vec!["OPENAI_CUSTOM_ENV".to_string()];
    let configured = configured_for(
        &reg,
        "openai",
        config_with_full_fields(
            "openai",
            "https://custom.example.com/v1",
            Some("test-key"),
            Some(&env_keys),
            "X-Custom-OpenAI",
            "openai-value",
        ),
    );

    assert_field_fidelity(
        &configured,
        "https://custom.example.com/v1",
        Some("test-key"),
        Some(&env_keys),
        "X-Custom-OpenAI",
        "openai-value",
        "chat_completions",
    );
}

// Anthropic tests ------------------------------------------------------------

#[test]
fn anthropic_config_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let env_keys = vec!["ANTHROPIC_CUSTOM_ENV".to_string()];
    let configured = configured_for(
        &reg,
        "anthropic",
        config_with_full_fields(
            "anthropic",
            "https://custom.example.com/v1",
            Some("test-key"),
            Some(&env_keys),
            "X-Custom-Anthropic",
            "anthropic-value",
        ),
    );

    assert_field_fidelity(
        &configured,
        "https://custom.example.com/v1",
        Some("test-key"),
        Some(&env_keys),
        "X-Custom-Anthropic",
        "anthropic-value",
        "messages",
    );
}

// xAI tests ------------------------------------------------------------------

#[test]
fn xai_config_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let env_keys = vec!["XAI_CUSTOM_ENV".to_string()];
    let configured = configured_for(
        &reg,
        "xai",
        config_with_full_fields(
            "xai",
            "https://custom.example.com/v1",
            Some("test-key"),
            Some(&env_keys),
            "X-Custom-xAI",
            "xai-value",
        ),
    );

    assert_field_fidelity(
        &configured,
        "https://custom.example.com/v1",
        Some("test-key"),
        Some(&env_keys),
        "X-Custom-xAI",
        "xai-value",
        "responses",
    );
}

// OpenCode tests -------------------------------------------------------------

#[test]
fn opencode_config_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let env_keys = vec!["OPENCODE_CUSTOM_ENV".to_string()];
    let configured = configured_for(
        &reg,
        "opencode",
        config_with_full_fields(
            "opencode",
            "https://custom.example.com/v1",
            Some("test-key"),
            Some(&env_keys),
            "X-Custom-OpenCode",
            "opencode-value",
        ),
    );

    assert_field_fidelity(
        &configured,
        "https://custom.example.com/v1",
        Some("test-key"),
        Some(&env_keys),
        "X-Custom-OpenCode",
        "opencode-value",
        "chat_completions",
    );
}

// Ollama tests ---------------------------------------------------------------

#[test]
fn ollama_config_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let env_keys = vec!["OLLAMA_CUSTOM_ENV".to_string()];
    let configured = configured_for(
        &reg,
        "ollama",
        config_with_full_fields(
            "ollama",
            "https://custom.example.com/v1",
            Some("test-key"),
            Some(&env_keys),
            "X-Custom-Ollama",
            "ollama-value",
        ),
    );

    assert_field_fidelity(
        &configured,
        "https://custom.example.com/v1",
        Some("test-key"),
        Some(&env_keys),
        "X-Custom-Ollama",
        "ollama-value",
        "chat_completions",
    );
}

// No-auth provider variant: OpenCode without api_key ------------------------

#[test]
fn opencode_public_no_auth_field_fidelity() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let configured = configured_for(
        &reg,
        "opencode",
        config_with_full_fields(
            "opencode",
            "http://localhost:8080/v1",
            None,
            None,
            "X-Custom-OpenCode",
            "public-value",
        ),
    );

    let route = configured
        .routes
        .values()
        .next()
        .expect("at least one route");

    // base_url should be reflected
    assert_eq!(
        route.endpoint.base_url.as_deref(),
        Some("http://localhost:8080/v1")
    );

    // No api_key → no credential candidates
    assert!(
        provider_inline_candidates(&route.auth).is_empty(),
        "no api_key must not produce ProviderInline candidate"
    );

    // extra_headers must still be present
    assert!(
        route.static_headers.contains_key("X-Custom-OpenCode"),
        "extra_headers must be present in route even without auth"
    );

    // protocol must be correct
    assert_eq!(route.protocol_id, ProtocolId::from("chat_completions"));
}

// Upstream-consumed fields -- these are consumed in resolution/catalog,
// not in configure(). Verified here for documentation only.
// - model_list_path → consumed in resolve_model_source() at resolution layer
// - model_list_format → consumed in ProviderRuntimeConfig::from()
// - allow_insecure_http → consumed in registry endpoint construction
