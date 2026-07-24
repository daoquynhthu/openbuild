use xai_grok_provider::config::parse_provider_toml;

fn parse_accepts(src: &str) {
    let toml = toml::from_str::<toml::Value>(src).expect("valid TOML");
    let parsed = parse_provider_toml(&toml).expect("parse must succeed");
    assert!(
        !parsed.entries.is_empty(),
        "must have at least one entry"
    );
}

#[test]
fn unknown_kind_is_silently_accepted() {
    parse_accepts(
        r#"
[provider.test]
kind = "unknown_kind"
api_key = "sk-test"
"#,
    );
}

#[test]
fn unknown_protocol_is_silently_accepted() {
    parse_accepts(
        r#"
[provider.test]
api_key = "sk-test"
protocol = "unknown_protocol"
"#,
    );
}

#[test]
fn unknown_model_list_format_is_silently_accepted() {
    parse_accepts(
        r#"
[provider.test]
api_key = "sk-test"
model_list_format = "unknown_format"
"#,
    );
}

#[test]
fn custom_provider_without_kind_or_profile_is_accepted() {
    parse_accepts(
        r#"
[provider.custom]
api_key = "sk-test"
base_url = "https://example.test/v1"
"#,
    );
}
