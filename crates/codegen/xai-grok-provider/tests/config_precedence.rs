//! Integration tests for provider configuration precedence.
//!
//! Precedence (low → high):
//!   built-in defaults < environment < [provider.<id>] TOML
//!     < legacy compatibility mapping < CLI override

use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::providers::build_provider_config;

/// No config at any layer → nothing set.
#[test]
fn precedence_empty() {
    let cfg = build_provider_config("test", &[], &indexmap::IndexMap::new(), None);
    assert!(cfg.api_key.is_none());
    assert!(cfg.base_url.is_none());
}

/// Environment layer sets api_key.
#[test]
fn precedence_env_sets_key() {
    let mut env_configs = indexmap::IndexMap::new();
    env_configs.insert(
        "test".into(),
        ProviderConfig::new(
            Some("test".into()),
            Some("env-key".into()),
            None,
        ),
    );
    let cfg = build_provider_config("test", &[], &env_configs, None);
    assert_eq!(cfg.api_key.as_deref(), Some("env-key"));
}

/// TOML overrides environment.
#[test]
fn precedence_toml_overrides_env() {
    let toml_configs = vec![(
        "test".into(),
        ProviderConfig::new(Some("test".into()), Some("toml-key".into()), None),
    )];
    let mut env_configs = indexmap::IndexMap::new();
    env_configs.insert(
        "test".into(),
        ProviderConfig::new(Some("test".into()), Some("env-key".into()), None),
    );
    let cfg = build_provider_config("test", &toml_configs, &env_configs, None);
    assert_eq!(cfg.api_key.as_deref(), Some("toml-key"));
}

/// CLI overrides everything.
#[test]
fn precedence_cli_overrides_all() {
    let toml_configs = vec![(
        "test".into(),
        ProviderConfig::new(Some("test".into()), Some("toml-key".into()), None),
    )];
    let mut env_configs = indexmap::IndexMap::new();
    env_configs.insert(
        "test".into(),
        ProviderConfig::new(Some("test".into()), Some("env-key".into()), None),
    );
    let cli = ProviderConfig::new(Some("test".into()), Some("cli-key".into()), None);
    let cfg = build_provider_config("test", &toml_configs, &env_configs, Some(&cli));
    assert_eq!(cfg.api_key.as_deref(), Some("cli-key"));
}

/// Fields merge independently — overriding base_url does not erase extra_headers.
#[test]
fn precedence_independent_field_merge() {
    let mut env_configs = indexmap::IndexMap::new();
    let mut env_headers = indexmap::IndexMap::new();
    env_headers.insert("x-env".into(), "env-val".into());
    env_configs.insert(
        "test".into(),
        ProviderConfig::new(Some("test".into()), None, Some("https://env.test/v1".into())),
    );
    env_configs.get_mut("test").unwrap().extra_headers = Some(env_headers);

    let toml_configs = vec![(
        "test".into(),
        ProviderConfig::new(Some("test".into()), Some("toml-key".into()), None),
    )];
    let cfg = build_provider_config("test", &toml_configs, &env_configs, None);
    assert_eq!(cfg.api_key.as_deref(), Some("toml-key"));
    assert_eq!(cfg.base_url.as_deref(), Some("https://env.test/v1"));
    let headers = cfg.extra_headers.unwrap_or_default();
    assert_eq!(headers.get("x-env").map(|s| s.as_str()), Some("env-val"));
}
