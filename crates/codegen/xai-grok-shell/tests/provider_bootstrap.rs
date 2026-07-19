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

/// Bootstrap path: produces revision=1 with non-empty providers and routes.
#[test]
fn bootstrap_provider_runtime_produces_full_snapshot() {
    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([(
                ProviderId::new("xai"),
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
                },
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
            },
        )]),
    };
    let r2 = rt.registry.rebuild_from_resolved(&resolved2);
    assert!(r2.is_ok(), "second rebuild must succeed: {:?}", r2.err());
    assert_eq!(r2.unwrap(), 2, "second rebuild must produce revision=2");
}
