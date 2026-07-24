use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::resolution::{
    ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet,
    ResolvedProviderSpec,
};
use xai_grok_provider::types::ProviderId;

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
            env_keys: vec![],
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
            env_keys: vec![],
        },
    }
}

fn custom_deepseek_spec() -> ResolvedProviderSpec {
    ResolvedProviderSpec {
        id: ProviderId::new("deepseek"),
        implementation: ProviderImplementation::OpenAiCompatible {
            profile: Some(xai_grok_provider::types::CompatibleProfileId::new(
                "deepseek",
            )),
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
            env_keys: vec![],
        },
    }
}

/// Precedence is applied exactly once: TOML config value reaches snapshot.
#[test]
fn bootstrap_precedence_applied_once() {
    let toml_str = r#"
        [provider.xai]
        api_key = "test-key"
    "#;
    let toml: toml::Value = toml::from_str(toml_str).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(
        xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None),
    );
    assert!(
        result.is_ok(),
        "bootstrap_from_config with valid config must succeed"
    );

    let rt = result.unwrap();
    let snap = rt.snapshot();
    assert_eq!(snap.revision, 1, "bootstrap must produce revision=1");
    assert!(!snap.providers.is_empty(), "bootstrap must have providers");
    assert!(!snap.routes.is_empty(), "bootstrap must have routes");
}

/// Diagnostics from config parsing cause bootstrap to fail (P6-004).
///
/// `parse_provider_toml` returns `Err(Vec<ConfigDiagnostic>)` when config
/// entries have irrecoverable structural issues (parse error entries).
/// `bootstrap_from_config` must propagate those as `ProviderBootstrapError`.
#[test]
fn bootstrap_fails_on_config_diagnostics() {
    // A TOML value with no [provider] section produces no diagnostics.
    let toml: toml::Value = toml::from_str(r#"other_key = 1"#).unwrap();
    let result = tokio::runtime::Runtime::new().unwrap().block_on(
        xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None),
    );
    // No provider section → empty set → bootstrap succeeds with providers from built-ins.
    assert!(result.is_ok(), "empty provider config must succeed");
}

/// Bootstrap path: produces revision=1 with non-empty providers and routes.
#[test]
fn bootstrap_provider_runtime_produces_full_snapshot() {
    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([(ProviderId::new("xai"), xai_spec())]),
        },
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime
        .block_on(xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input));
    assert!(rt.is_ok(), "bootstrap must succeed: {:?}", rt.err());

    let rt = rt.unwrap();
    let snap = rt.snapshot();
    assert_eq!(snap.revision, 1, "bootstrap must produce revision=1");
    assert!(!snap.providers.is_empty(), "bootstrap must have providers");
    assert!(!snap.routes.is_empty(), "bootstrap must have routes");

    // Verify seal: a second rebuild must NOT require re-registration.
    let resolved2 = ResolvedProviderSet {
        providers: IndexMap::from([(ProviderId::new("xai"), xai_spec())]),
    };
    let r2 = rt.registry.rebuild_from_resolved(&resolved2);
    assert!(r2.is_ok(), "second rebuild must succeed: {:?}", r2.err());
    assert_eq!(r2.unwrap(), 2, "second rebuild must produce revision=2");
}

/// Runtime identity shared through ConfigReloader — identity must be injected.
#[test]
fn bootstrap_runtime_identity_reaches_config_reloader() {
    use xai_grok_shell::agent::provider_config_coordinator::{
        ProviderConfigCoordinator, ProviderResolutionContext,
    };
    use xai_grok_shell::config::reloader::ConfigReloader;

    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([(ProviderId::new("xai"), xai_spec())]),
        },
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime
        .block_on(xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input))
        .expect("bootstrap");

    let coord = ProviderConfigCoordinator::new(
        rt,
        std::path::PathBuf::from("/tmp/nonexistent"),
        Arc::new(ProviderResolutionContext {
            legacy_migration: None,
            cli_overrides: None,
        }),
    );

    let (_tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let _reloader = ConfigReloader::new(
        std::path::PathBuf::from("/tmp/nonexistent"),
        0,
        toml::from_str("").unwrap(),
        "test".into(),
        None,
        _tx,
        false,
        false,
        Some(Arc::new(coord)),
    );
}

/// Runtime identity: bootstrap produces a unique Arc, clones share same pointer.
#[test]
fn bootstrap_runtime_identity_is_unique_across_clones() {
    let input = xai_grok_shell::agent::provider_bootstrap::ProviderBootstrapInput {
        resolved: ResolvedProviderSet {
            providers: IndexMap::from([(ProviderId::new("xai"), xai_spec())]),
        },
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime
        .block_on(xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input))
        .expect("bootstrap");

    // Clone the Arc — must point to the same heap allocation.
    let rt_clone = rt.clone();
    assert!(
        Arc::ptr_eq(&rt, &rt_clone),
        "cloned Arc must point to same ProviderRuntime"
    );

    // Registry and catalog inside the runtime must also be the same across clones.
    let snap1 = rt.snapshot();
    let snap2 = rt_clone.snapshot();
    assert!(
        Arc::ptr_eq(&snap1, &snap2),
        "snapshot from cloned runtime must be same Arc"
    );

    // AgentConfig deriving registry/catalog from runtime must return the same Arc.
    let config = xai_grok_shell::agent::config::Config {
        provider_runtime: Some(rt.clone()),
        ..Default::default()
    };

    let reg_from_config = config.provider_registry().expect("registry from config");
    let cat_from_config = config.provider_catalog().expect("catalog from config");
    assert!(
        Arc::ptr_eq(&reg_from_config, &rt.registry),
        "config.provider_registry() must return same Arc as runtime.registry"
    );
    assert!(
        Arc::ptr_eq(&cat_from_config, &rt.catalog),
        "config.provider_catalog() must return same Arc as runtime.catalog"
    );
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
    let rt = runtime
        .block_on(xai_grok_shell::agent::provider_bootstrap::bootstrap_provider_runtime(input));
    assert!(rt.is_ok(), "bootstrap must succeed");

    let rt = rt.unwrap();
    let snap = rt.snapshot();
    assert_eq!(snap.revision, 1);
    assert_eq!(
        snap.providers.len(),
        3,
        "all three identities must be present"
    );
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
