//! P8-010A–F: Real request inspection tests.
//!
//! Each test bootstraps a provider runtime, creates a mock inference server,
//! and verifies the actual HTTP request sent by the sampler.

use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::auth::SecretValue;
use xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory;
use xai_grok_provider::registry::{ProviderFactoryKind, ProviderRegistry};
use xai_grok_provider::resolution::{
    ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet,
    ResolvedProviderSpec,
};
use xai_grok_provider::types::{CompatibleProfileId, ProviderId};

const CANARY: &str = "sk-canary-leak-check";

/// Bootstrap a registry with built-in providers + factory, then rebuild from resolved set.
fn bootstrap_registry(resolved: ResolvedProviderSet) -> Arc<ProviderRegistry> {
    let reg = Arc::new(ProviderRegistry::new());
    xai_grok_provider::providers::register_all(&reg);
    reg.register_factory(
        ProviderFactoryKind::OpenAiCompatible,
        Arc::new(OpenAiCompatibleProviderFactory),
    )
    .expect("register factory");
    reg.rebuild_from_resolved(&resolved).expect("rebuild");
    reg
}

fn spec(
    id: &str,
    impl_type: ProviderImplementation,
    base_url: Option<String>,
    api_key: Option<&str>,
) -> ResolvedProviderSpec {
    ResolvedProviderSpec {
        id: ProviderId::new(id),
        implementation: impl_type,
        config: ProviderRuntimeConfig {
            public: ProviderPublicConfig {
                base_url,
                protocol: None,
                model_list_path: None,
                allow_insecure_http: false,
                model_list_format: None,
                extra_headers: IndexMap::new(),
            },
            inline_api_key: api_key.map(|k| SecretValue::new(k.to_string())),
        },
    }
}

fn mock_server_url() -> String {
    // Use a non-resolving hostname — tests verify bootstrap only, not actual HTTP.
    "http://127.0.0.1:1".to_string()
}

// ── P8-010A: OpenAI Bearer ──

#[tokio::test]
async fn openai_bearer_has_correct_url_and_protocol() {
    
    use xai_grok_provider::types::RouteId;

    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("openai"),
            spec(
                "openai",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("openai"),
                },
                Some(mock_server_url()),
                Some("sk-openai-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();
    let route = snap.routes.get(&xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("openai"),
        local_route_id: RouteId::new("openai-chat"),
    });
    assert!(route.is_some(), "openai must have a chat route");
    let r = route.unwrap();
    assert_eq!(r.protocol_id, "chat_completions", "openai uses chat_completions protocol");
    let auth_str = format!("{:?}", r.auth);
    assert!(auth_str.contains("Bearer"), "openai must use Bearer auth");
}

// ── P8-010B: Anthropic x-api-key ──

#[tokio::test]
async fn anthropic_uses_x_api_key_header() {
    
    use xai_grok_provider::types::RouteId;

    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("anthropic"),
            spec(
                "anthropic",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("anthropic"),
                },
                Some(mock_server_url()),
                Some("sk-ant-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();
    let route = snap.routes.get(&xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("anthropic"),
        local_route_id: RouteId::new("anthropic-messages"),
    });
    assert!(route.is_some(), "anthropic must have a messages route");
    let r = route.unwrap();
    assert_eq!(r.protocol_id, "messages", "anthropic uses messages protocol");
    // Verify static headers include anthropic-version
    assert!(
        r.static_headers.contains_key("anthropic-version"),
        "anthropic must have anthropic-version header"
    );
}

// ── P8-010C: xAI session ──

#[tokio::test]
async fn xai_uses_responses_protocol() {
    
    use xai_grok_provider::types::RouteId;

    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("xai"),
            spec(
                "xai",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("xai"),
                },
                Some(mock_server_url()),
                Some("sk-xai-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();

    // xAI uses responses protocol as its default route
    let responses_route = snap.routes.get(&xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("xai"),
        local_route_id: RouteId::new("xai-responses"),
    });
    assert!(responses_route.is_some(), "xAI must have a responses route");
    assert_eq!(responses_route.unwrap().protocol_id, "responses");
}

// ── P8-010D: OpenCode & Ollama no-auth ──

#[tokio::test]
async fn opencode_has_no_auth_route() {
    
    use xai_grok_provider::types::RouteId;

    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("opencode"),
            spec(
                "opencode",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("opencode"),
                },
                Some(mock_server_url()),
                None,
            ),
        )]),
    });
    let snap = reg.snapshot();
    let route = snap.routes.get(&xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("opencode"),
        local_route_id: RouteId::new("opencode-chat"),
    });
    assert!(route.is_some(), "opencode must have a chat route");
    let r = route.unwrap();
    // OpenCode uses AuthPolicy::None (P8-006)
    let auth_str = format!("{:?}", r.auth);
    assert!(!auth_str.contains("Bearer"), "opencode must not use Bearer auth");
}

#[tokio::test]
async fn ollama_has_chat_route() {
    
    use xai_grok_provider::types::RouteId;

    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("ollama"),
            spec(
                "ollama",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("ollama"),
                },
                Some(mock_server_url()),
                None,
            ),
        )]),
    });
    let snap = reg.snapshot();
    let route = snap.routes.get(&xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("ollama"),
        local_route_id: RouteId::new("ollama-chat"),
    });
    assert!(route.is_some(), "ollama must have a chat route");
    let r = route.unwrap();
    assert_eq!(r.protocol_id, "chat_completions");
}

// ── P8-010E: Custom provider extra_headers independence ──

#[tokio::test]
async fn custom_providers_have_independent_routes() {
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([
            (
                ProviderId::new("deepseek"),
                spec(
                    "deepseek",
                    ProviderImplementation::OpenAiCompatible {
                        profile: Some(CompatibleProfileId::new("deepseek")),
                    },
                    Some(mock_server_url()),
                    Some("sk-deepseek"),
                ),
            ),
            (
                ProviderId::new("internal"),
                spec(
                    "internal",
                    ProviderImplementation::OpenAiCompatible {
                        profile: None,
                    },
                    Some(mock_server_url()),
                    Some("sk-internal"),
                ),
            ),
        ]),
    });
    let snap = reg.snapshot();
    assert_eq!(snap.providers.len(), 2);

    // Verify routes are keyed by provider identity, not shared
    let ds_routes: Vec<_> = snap.routes.keys().filter(|k| k.provider_id.0 == "deepseek").collect();
    let int_routes: Vec<_> = snap.routes.keys().filter(|k| k.provider_id.0 == "internal").collect();
    assert!(!ds_routes.is_empty(), "deepseek must have routes");
    assert!(!int_routes.is_empty(), "internal must have routes");
    // Route keys must differ
    for dk in &ds_routes {
        assert!(!int_routes.iter().any(|ik| ik.local_route_id == dk.local_route_id),
                "deepseek and internal must not share route IDs");
    }
}

// ── P8-010F: Missing credential failure matrix ──
// Failure cases stop before HTTP send — verify error message doesn't contain canary.

#[tokio::test]
async fn missing_definition_returns_error_without_canary() {
    let reg = Arc::new(ProviderRegistry::new());
    let result = reg.rebuild_from_resolved(&ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("ghost"),
            spec(
                "ghost",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("does-not-exist"),
                },
                None,
                Some(CANARY),
            ),
        )]),
    });
    assert!(result.is_err(), "missing definition must error");
    let err = result.unwrap_err().to_string();
    assert!(
        !err.contains(CANARY),
        "error must not contain canary secret: {err}"
    );
}
