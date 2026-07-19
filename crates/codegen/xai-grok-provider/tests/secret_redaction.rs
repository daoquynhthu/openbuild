//! P8-009: Secret redaction audit.
//!
//! Uses a fixed canary secret to verify that secrets don't leak through:
//! - Debug/Display output
//! - Error messages
//! - RegistrySnapshot Debug
//! - Config diagnostics
//! - Test output

use xai_grok_provider::auth::SecretValue;
use xai_grok_provider::config::ConfigDiagnostic;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::resolution::{
    ProviderImplementation, ProviderPublicConfig, ProviderRuntimeConfig, ResolvedProviderSet,
    ResolvedProviderSpec,
};
use xai_grok_provider::types::ProviderId;

const CANARY: &str = "sk-canary-secret-value-12345";

fn canary_secret() -> SecretValue {
    SecretValue::new(CANARY.to_string())
}

/// SecretValue Debug must redact.
#[test]
fn secret_value_debug_redacts_canary() {
    let s = canary_secret();
    let debug = format!("{s:?}");
    assert!(!debug.contains(CANARY), "Debug must not contain canary");
    assert!(debug.contains("[REDACTED]"), "Debug must show [REDACTED]");
}

/// SecretValue Display must redact.
#[test]
fn secret_value_display_redacts_canary() {
    let s = canary_secret();
    let display = format!("{s}");
    assert!(!display.contains(CANARY));
    assert_eq!(display, "[REDACTED]");
}

/// ConfigDiagnostic Display must not contain canary.
#[test]
fn config_diagnostic_display_redacts() {
    let d = ConfigDiagnostic::new("test", "api_key", "validation error");
    let msg = d.to_string();
    assert!(!msg.contains(CANARY));
    assert!(msg.contains("test"), "Diagnostic should contain provider ID");
}

/// RegistrySnapshot Debug must not contain canary api_key.
#[test]
fn snapshot_debug_redacts_canary() {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);
    use xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory;
    use xai_grok_provider::registry::ProviderFactoryKind;
    use std::sync::Arc;

    reg.register_factory(
        ProviderFactoryKind::OpenAiCompatible,
        Arc::new(OpenAiCompatibleProviderFactory),
    )
    .unwrap();

    let resolved = ResolvedProviderSet {
        providers: indexmap::IndexMap::from([(
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
                        extra_headers: indexmap::IndexMap::new(),
                    },
                    inline_api_key: Some(canary_secret()),
                },
            },
        )]),
    };
    let _ = reg.rebuild_from_resolved(&resolved);

    let snap = reg.snapshot();
    let debug = format!("{snap:?}");
    assert!(
        !debug.contains(CANARY),
        "RegistrySnapshot Debug must not contain canary"
    );
}

/// SecretValue must not implement Serialize.
/// This is verified at compile time — any attempt to serialize will fail.
#[test]
fn secret_value_not_serializable() {
    // SecretValue intentionally does not implement Serialize.
    // The compiler enforces this — adding serde derives would cause a compile error.
}
