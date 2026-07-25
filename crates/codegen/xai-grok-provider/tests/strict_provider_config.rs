use xai_grok_provider::config::parse_provider_toml;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::resolution::resolve_provider_set;

fn resolve_provider_with(src: &str) -> (Vec<xai_grok_provider::config::ConfigDiagnostic>, bool) {
    let toml = toml::from_str::<toml::Value>(src).expect("valid TOML");
    let parsed = parse_provider_toml(&toml).expect("parse must succeed");
    let configs: Vec<(String, ProviderConfig)> = parsed
        .entries
        .into_iter()
        .map(|(id, cfg)| (id.0, cfg))
        .collect();
    let (set, diags) = resolve_provider_set(configs);
    (diags, set.providers.is_empty())
}

// ── PARSE-01: Unknown TOML fields are rejected at parse time ──

#[test]
fn unknown_toml_field_implementation_is_rejected() {
    let toml = toml::from_str::<toml::Value>(
        r#"
[provider.test]
implementation = "builtin"
api_key = "sk-test"
"#,
    )
    .unwrap();
    let result = parse_provider_toml(&toml);
    assert!(
        result.is_err(),
        "unknown TOML field `implementation` must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        err.iter().any(|d| d.to_string().contains("implementation")),
        "error must mention unknown field `implementation`"
    );
}

#[test]
fn unknown_toml_field_is_rejected_with_field_name() {
    let toml = toml::from_str::<toml::Value>(
        r#"
[provider.test]
typo_field = "value"
api_key = "sk-test"
"#,
    )
    .unwrap();
    let result = parse_provider_toml(&toml);
    assert!(
        result.is_err(),
        "unknown TOML field `typo_field` must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        err.iter().any(|d| d.to_string().contains("typo_field")),
        "error must mention unknown field name: {err:?}"
    );
}

// ── PARSE-02: Semantic validation errors at resolution time ──

#[test]
fn unknown_protocol_is_error_at_resolution() {
    let (diags, empty) = resolve_provider_with(
        r#"
[provider.test]
api_key = "sk-test"
base_url = "https://test.api/v1"
protocol = "unknown_protocol"
"#,
    );
    assert!(empty || diags.iter().any(|d| d.is_error()));
    assert!(
        diags.iter().any(|d| d.to_string().contains("unknown protocol")),
        "unknown protocol must produce error diagnostic"
    );
}

#[test]
fn unknown_kind_is_error_at_resolution() {
    let (diags, _) = resolve_provider_with(
        r#"
[provider.test]
api_key = "sk-test"
base_url = "https://test.api/v1"
kind = "unknown_kind"
"#,
    );
    assert!(
        diags.iter().any(|d| d.is_error()),
        "unknown kind must produce error"
    );
}

#[test]
fn known_kind_openai_compatible_is_accepted() {
    let (diags, _) = resolve_provider_with(
        r#"
[provider.custom]
api_key = "sk-test"
base_url = "https://custom.api/v1"
kind = "openai_compatible"
"#,
    );
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "openai_compatible kind must not produce errors: {errors:?}");
}

#[test]
fn custom_provider_without_kind_but_with_base_url_is_accepted() {
    let (diags, _) = resolve_provider_with(
        r#"
[provider.custom]
api_key = "sk-test"
base_url = "https://example.test/v1"
"#,
    );
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "custom provider with base_url must be accepted: {errors:?}");
}

// ── PARSE-03: Duplicate identity rejection ──

#[test]
fn duplicate_in_same_layer_is_error() {
    let configs = vec![
        (
            "dup".into(),
            ProviderConfig::new(
                Some("dup".into()),
                None,
                Some("https://first.url/v1".into()),
            ),
        ),
        (
            "dup".into(),
            ProviderConfig::new(
                Some("dup".into()),
                None,
                Some("https://second.url/v1".into()),
            ),
        ),
    ];
    let (_set, diags) = resolve_provider_set(configs);
    assert!(
        diags.iter().any(|d| d.is_error()),
        "duplicate identity must produce error"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.to_string().contains("duplicate") && d.to_string().contains("dup")),
        "error must mention duplicate and provider ID"
    );
}

// ── PARSE-05: Diagnostic format ──

#[test]
fn error_diagnostic_contains_provider_id_field_and_category() {
    let (diags, _) = resolve_provider_with(
        r#"
[provider.test]
api_key = "sk-test"
base_url = "https://test.api/v1"
protocol = "nonsense"
"#,
    );
    let err = diags.iter().find(|d| d.is_error()).expect("must have error");
    let text = err.to_string();
    assert!(text.contains("test"), "must contain provider ID: {text}");
    assert!(text.contains("protocol"), "must contain field path: {text}");
    assert!(text.contains("unknown_value"), "must contain category: {text}");
    assert!(!text.contains("sk-test"), "must NOT contain API key: {text}");
}
