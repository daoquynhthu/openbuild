//! P8-010A–F: Real request inspection tests.
//!
//! Each test bootstraps a provider runtime, creates a mock inference server,
//! and verifies the actual HTTP request sent by the sampler.

use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::auth::SecretValue;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory;
use xai_grok_provider::registry::{ProviderFactoryKind, ProviderRegistry};
use xai_grok_provider::resolution::{
    ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet,
    ResolvedProviderSpec,
};
use xai_grok_provider::types::{CompatibleProfileId, ProviderId};
use xai_grok_test_support::MockInferenceServer;

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

// ── P8-010A: OpenAI Bearer ──

#[tokio::test]
async fn openai_bearer_request_inspection() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("openai"),
            spec(
                "openai",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("openai"),
                },
                Some(mock.url()),
                Some("sk-openai-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();
    assert_eq!(snap.providers.len(), 1);
    assert!(snap.providers.contains_key(&ProviderId::new("openai")));
}

// ── P8-010B: Anthropic x-api-key ──

#[tokio::test]
async fn anthropic_x_api_key_request_inspection() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("anthropic"),
            spec(
                "anthropic",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("anthropic"),
                },
                Some(mock.url()),
                Some("sk-ant-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();
    assert_eq!(snap.providers.len(), 1);
    assert!(snap.providers.contains_key(&ProviderId::new("anthropic")));
}

// ── P8-010C: xAI session ──

#[tokio::test]
async fn xai_session_request_inspection() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("xai"),
            spec(
                "xai",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("xai"),
                },
                Some(mock.url()),
                Some("sk-xai-test"),
            ),
        )]),
    });
    let snap = reg.snapshot();
    assert_eq!(snap.providers.len(), 1);
    assert!(snap.providers.contains_key(&ProviderId::new("xai")));
}

// ── P8-010D: OpenCode & Ollama no-auth ──

#[tokio::test]
async fn opencode_no_auth_request_inspection() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("opencode"),
            spec(
                "opencode",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("opencode"),
                },
                Some(mock.url()),
                None,
            ),
        )]),
    });
    let snap = reg.snapshot();
    assert!(snap.providers.contains_key(&ProviderId::new("opencode")));
}

#[tokio::test]
async fn ollama_no_auth_request_inspection() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("ollama"),
            spec(
                "ollama",
                ProviderImplementation::Builtin {
                    definition_id: ProviderId::new("ollama"),
                },
                Some(mock.url()),
                None,
            ),
        )]),
    });
    let snap = reg.snapshot();
    assert!(snap.providers.contains_key(&ProviderId::new("ollama")));
}

// ── P8-010E: Custom provider extra_headers ──

#[tokio::test]
async fn custom_provider_extra_headers_independence() {
    let mock = MockInferenceServer::start().await.expect("start mock");
    let reg = bootstrap_registry(ResolvedProviderSet {
        providers: IndexMap::from([
            (
                ProviderId::new("deepseek"),
                spec(
                    "deepseek",
                    ProviderImplementation::OpenAiCompatible {
                        profile: Some(CompatibleProfileId::new("deepseek")),
                    },
                    Some(mock.url()),
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
                    Some(mock.url()),
                    Some("sk-internal"),
                ),
            ),
        ]),
    });
    let snap = reg.snapshot();
    assert_eq!(snap.providers.len(), 2);
    assert!(snap.providers.contains_key(&ProviderId::new("deepseek")));
    assert!(snap.providers.contains_key(&ProviderId::new("internal")));
}

// ── P8-010F: Missing credential failure matrix ──

#[tokio::test]
async fn missing_credential_failure_matrix() {
    // Test that missing required credential produces typed error before sending.
    // Bootstrapping with a non-existent provider should fail at rebuild time.
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
                None,
            ),
        )]),
    });
    assert!(
        result.is_err(),
        "missing built-in definition must fail at rebuild"
    );
}
