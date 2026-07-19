use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::providers::{configure_providers, register_all};
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::resolution::{
    ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet,
    ResolvedProviderSpec,
};
use xai_grok_provider::types::ProviderId;

/// Legacy path: `register_all + configure_providers` produces an empty snapshot.
/// This test records the current broken state as root-cause evidence.
/// It MUST be replaced by the bootstrap test below once P6-002 lands.
#[test]
fn legacy_configure_providers_snapshot_is_empty() {
    let reg = Arc::new(ProviderRegistry::new());
    register_all(&reg);

    // Legacy path reads TOML, env, and CLI — use an empty config.
    let toml: toml::Value = toml::from_str("").unwrap();
    configure_providers(&reg, &toml, None, None);

    let snap = reg.snapshot();
    // Both routes and providers are empty because configure_providers
    // no longer calls store_config/register_route (removed in P5-007).
    // The `configure()` call is a pure query — it does NOT populate the snapshot.
    assert!(
        snap.providers.is_empty(),
        "legacy path must produce empty providers — rev={}, count={}",
        snap.revision,
        snap.providers.len()
    );
    assert!(
        snap.routes.is_empty(),
        "legacy path must produce empty routes — rev={}, count={}",
        snap.revision,
        snap.routes.len()
    );
    assert_eq!(snap.revision, 0, "legacy path must keep revision=0");
}

fn xai_spec() -> ResolvedProviderSpec {
    ResolvedProviderSpec {
        id: ProviderId::new("xai"),
        implementation: ProviderImplementation::Builtin {
            definition_id: ProviderId::new("xai"),
        },
        config: ProviderRuntimeConfig {
            public: ProviderPublicConfig {
                base_url: None,
                protocol: None,
                model_list_path: None,
                allow_insecure_http: false,
                model_list_format: None,
                extra_headers: IndexMap::new(),
            },
            inline_api_key: None,
        },
    }
}

fn openai_spec() -> ResolvedProviderSpec {
    ResolvedProviderSpec {
        id: ProviderId::new("openai"),
        implementation: ProviderImplementation::Builtin {
            definition_id: ProviderId::new("openai"),
        },
        config: ProviderRuntimeConfig {
            public: ProviderPublicConfig {
                base_url: None,
                protocol: None,
                model_list_path: None,
                allow_insecure_http: false,
                model_list_format: None,
                extra_headers: IndexMap::new(),
            },
            inline_api_key: None,
        },
    }
}

fn custom_deepseek_spec() -> ResolvedProviderSpec {
    ResolvedProviderSpec {
        id: ProviderId::new("deepseek"),
        implementation: ProviderImplementation::OpenAiCompatible {
            profile: Some(
                xai_grok_provider::types::CompatibleProfileId::new("deepseek"),
            ),
        },
        config: ProviderRuntimeConfig {
            public: ProviderPublicConfig {
                base_url: None,
                protocol: None,
                model_list_path: None,
                allow_insecure_http: false,
                model_list_format: None,
                extra_headers: IndexMap::new(),
            },
            inline_api_key: None,
        },
    }
}

/// Bootstrap path: produces revision=1 with non-empty providers and routes.
#[test]
fn bootstrap_provider_runtime_produces_full_snapshot() {
    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("xai"),
                xai_spec(),
            )]),
        },
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.block_on(
        xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input),
    );
    assert!(rt.is_ok(), "bootstrap must succeed: {:?}", rt.err());

    let rt = rt.unwrap();
    let snap = rt.snapshot();
    assert_eq!(snap.revision, 1, "bootstrap must produce revision=1");
    assert!(!snap.providers.is_empty(), "bootstrap must have providers");
    assert!(!snap.routes.is_empty(), "bootstrap must have routes");

    // Verify seal: a second rebuild must NOT require re-registration.
    let resolved2 = ResolvedProviderSet {
        providers: IndexMap::from([(
            ProviderId::new("xai"),
            xai_spec(),
        )]),
    };
    let r2 = rt.registry.rebuild_from_resolved(&resolved2);
    assert!(r2.is_ok(), "second rebuild must succeed: {:?}", r2.err());
    assert_eq!(r2.unwrap(), 2, "second rebuild must produce revision=2");
}

/// Bootstrap with both built-in and custom (OpenAiCompatible) identities.
#[test]
fn bootstrap_with_builtin_and_custom_identities() {
    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([
                (ProviderId::new("xai"), xai_spec()),
                (ProviderId::new("openai"), openai_spec()),
                (ProviderId::new("deepseek"), custom_deepseek_spec()),
            ]),
        },
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.block_on(
        xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input),
    );
    assert!(rt.is_ok(), "bootstrap must succeed");

    let rt = rt.unwrap();
    let snap = rt.snapshot();
    assert_eq!(snap.revision, 1);
    assert_eq!(snap.providers.len(), 3, "all three identities must be present");
    assert!(snap.providers.contains_key(&ProviderId::new("xai")));
    assert!(snap.providers.contains_key(&ProviderId::new("openai")));
    assert!(snap.providers.contains_key(&ProviderId::new("deepseek")));

    // Verify deepseek uses profile-based endpoint, not xAI's or OpenAI's.
    let ds_key = xai_grok_provider::registry::ProviderRouteKey {
        provider_id: ProviderId::new("deepseek"),
        local_route_id: xai_grok_provider::types::RouteId::new("deepseek-chat"),
    };
    let ds_route = snap.routes.get(&ds_key);
    assert!(ds_route.is_some(), "deepseek must have a route");
    let ds_url = ds_route.unwrap().endpoint.base_url.as_deref().unwrap_or("");
    assert!(
        ds_url.contains("deepseek"),
        "deepseek endpoint should reference deepseek, got: {ds_url}"
    );
}
