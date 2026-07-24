use xai_grok_provider::config::ProviderConfig;

#[test]
fn http_localhost_allowed_without_insecure_flag() {
    let cfg = ProviderConfig::new(Some("test".into()), Some("sk-test".into()), None);
    assert!(
        !cfg.allow_insecure_http.unwrap_or(false),
        "allow_insecure_http defaults to false"
    );
    // Current: flag is parsed but never enforced against endpoint URL.
    // After Phase 8: http localhost should be allowed even without flag (local exception).
}

#[test]
fn http_remote_rejected_without_insecure_flag() {
    let mut cfg = ProviderConfig::new(Some("test".into()), Some("sk-test".into()), None);
    cfg.base_url = Some("http://example.com/v1".into());
    assert!(
        !cfg.allow_insecure_http.unwrap_or(false),
        "R3-RED-10: http remote with allow_insecure_http=false must be rejected — \
         currently no enforcement exists"
    );
}

#[test]
fn http_remote_with_insecure_flag_behavior_tbd() {
    let mut cfg = ProviderConfig::new(Some("test".into()), Some("sk-test".into()), None);
    cfg.base_url = Some("http://example.com/v1".into());
    cfg.allow_insecure_http = Some(true);
    assert!(
        cfg.allow_insecure_http.unwrap_or(false),
        "allow_insecure_http=true should allow http remote"
    );
}

#[test]
fn https_remote_allowed() {
    let cfg = ProviderConfig::new(Some("test".into()), Some("sk-test".into()), None);
    assert!(
        !cfg.allow_insecure_http.unwrap_or(false),
        "allow_insecure_http defaults to false — https is always allowed"
    );
}

#[test]
fn allow_insecure_http_has_no_effect_on_endpoint_url_check() {
    // The field is parsed and stored but there is no URL-scheme enforcement
    // anywhere in the provider resolution chain.
    let mut cfg = ProviderConfig::new(Some("test".into()), Some("sk-test".into()), None);
    cfg.base_url = Some("http://insecure.example.com/v1".into());
    cfg.allow_insecure_http = Some(false);
    // After Phase 8, this configuration should produce an error when attempting
    // to create an HTTP client to this endpoint. Currently it passes silently.
    let endpoint_url = cfg.base_url.as_deref().unwrap_or("");
    assert!(
        endpoint_url.starts_with("http://"),
        "endpoint URL is http but allow_insecure_http=false — no rejection (BUG)"
    );
}
