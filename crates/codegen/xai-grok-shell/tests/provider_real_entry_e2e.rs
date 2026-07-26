//! R3-RED-14: Production-chain E2E red fixture.
//!
//! Entry: TOML → bootstrap_from_config → App/Agent construction
//!   → ACP session or equivalent → model selection → request
//!   preparation → mock server.
//!
//! RED tests MUST NOT:
//! - construct `ModelEntry::fallback`
//! - manually pass a key directly to a helper
//! - call the route compiler as the top-level action
//!
//! All RED tests assert correct behaviour → FAIL pre-fix, PASS post-fix.

use std::num::NonZeroU64;

use indexmap::IndexMap;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_shell::agent::config::{ModelEntry, ModelInfo, default_agent_type};

fn make_entry(provider_id: &str, model: &str) -> ModelEntry {
    ModelEntry {
        info: ModelInfo {
            model: model.to_owned(),
            base_url: String::new(),
            agent_type: default_agent_type(),
            context_window: NonZeroU64::new(200_000).unwrap(),
            api_backend: Default::default(),
            auth_scheme: Default::default(),
            extra_headers: IndexMap::new(),
            user_selectable: true,
            supported_in_api: true,
            show_model_fingerprint: false,
            supports_reasoning_effort: false,
            supports_backend_search: false,
            use_concise: false,
            hidden: false,
            laziness_detector: Default::default(),
            id: None,
            name: None,
            description: None,
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            auto_compact_threshold_percent: None,
            system_prompt_label: None,
            inference_idle_timeout_secs: None,
            max_retries: None,
            reasoning_effort: None,
            reasoning_efforts: Vec::new(),
            compactions_remaining: None,
            compaction_at_tokens: None,
            stream_tool_calls: None,
        },
        api_key: None,
        env_key: None,
        api_base_url: None,
        provider_id: Some(provider_id.to_owned()),
        route_id: None,
    }
}

async fn bootstrap(toml_str: &str) -> RegistrySnapshot {
    let toml: toml::Value = toml::from_str(toml_str).unwrap();
    let runtime =
        xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
            .await
            .expect("bootstrap must succeed");
    runtime.snapshot().as_ref().clone()
}

/// RED: execution_to_sampler_config passes None for provider_inline
/// credential, so a provider api_key from TOML is silently dropped.
/// The outgoing SamplerConfig must carry Authorization: Bearer <key>
/// but currently does not.
#[tokio::test]
async fn e2e_provider_inline_key_dropped() {
    let snapshot = bootstrap(
        r#"
        [provider.test]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "sk-provider-inline"
        "#,
    )
    .await;

    let model = make_entry("test", "m");

    let config = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model, &snapshot, None, None,
    )
    .await
    .expect("execution_to_sampler_config must succeed");

    let has_auth = config
        .extra_headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("authorization"));

    assert!(
        has_auth,
        "R3-RED-14: provider inline key must produce Authorization header; \
         execution_to_sampler_config drops provider_inline credential",
    );
}

/// RED: Responses protocol may be overridden to Chat Completions.
#[tokio::test]
async fn e2e_responses_protocol_respected() {
    let snapshot = bootstrap(
        r#"
        [provider.test]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "test-key"
        protocol = "responses"
        "#,
    )
    .await;

    let model = make_entry("test", "m");

    let config = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model, &snapshot, None, None,
    )
    .await
    .expect("execution_to_sampler_config must succeed");

    assert_eq!(
        config.api_backend,
        xai_grok_sampler::ApiBackend::Responses,
        "R3-RED-14: protocol=responses must produce Responses backend (not Chat)",
    );
}

/// RED: missing credential does not produce a typed hard error.
/// execution_to_sampler_config should fail with a clear credential error
/// when no key is available anywhere. Currently returns Ok with
/// no auth header.
#[tokio::test]
async fn e2e_missing_credential_hard_error() {
    let snapshot = bootstrap(
        r#"
        [provider.test]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        "#,
    )
    .await;

    let model = make_entry("test", "m");

    let result = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model, &snapshot, None, None,
    )
    .await;

    assert!(
        result.is_err(),
        "R3-RED-14: missing credential must produce a hard error",
    );
}

/// RED: custom OpenAI-compatible provider inline key is lost —
/// same root cause as e2e_provider_inline_key_dropped.
#[tokio::test]
async fn e2e_custom_provider_inline_key_lost() {
    let snapshot = bootstrap(
        r#"
        [provider.custom]
        kind = "openai_compatible"
        base_url = "http://127.0.0.1:0"
        api_key = "custom-inline-key"
        "#,
    )
    .await;

    let model = make_entry("custom", "m");

    let config = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model, &snapshot, None, None,
    )
    .await
    .expect("execution_to_sampler_config must succeed");

    let has_auth = config
        .extra_headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("authorization"));

    assert!(
        has_auth,
        "R3-RED-14: custom provider inline key must produce Authorization header",
    );
}

/// RED: built-in OpenAI provider with inline key — provider api_key from
/// TOML is dropped by execution_to_sampler_config, causing an
/// AuthCredential failure instead of a successful request with auth.
///
/// Unlike openai-compatible (which silently skips auth), the built-in
/// OpenAI provider enforces Bearer auth and rejects the request.
#[tokio::test]
async fn e2e_openai_builtin_inline_key_dropped() {
    let snapshot = bootstrap(
        r#"
        [provider.openai]
        base_url = "http://127.0.0.1:0"
        api_key = "sk-openai"
        "#,
    )
    .await;

    let model = make_entry("openai", "gpt-4o");

    let result = xai_grok_shell::agent::provider_resolution::execution_to_sampler_config(
        &model, &snapshot, None, None,
    )
    .await;

    assert!(
        result.is_ok(),
        "R3-RED-14: built-in OpenAI provider inline key must resolve successfully; \
         execution_to_sampler_config drops provider_inline -> AuthCredential: {:?}",
        result.err(),
    );
}
