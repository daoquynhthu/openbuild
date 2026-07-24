use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::types::ProviderId;

fn openai_configured(
    protocol: Option<&str>,
) -> xai_grok_provider::provider::ConfiguredProvider {
    let reg = ProviderRegistry::new();
    xai_grok_provider::providers::register_all(&reg);

    let mut cfg = ProviderConfig::new(Some("openai".into()), Some("test-key".into()), None);
    cfg.protocol = protocol.map(|s| s.into());

    let pid = ProviderId::new("openai");
    let provider = reg.get(&pid).expect("openai provider");
    provider.configure(cfg)
}

#[test]
fn explicit_chat_route_returns_chat() {
    let configured = openai_configured(None);

    let routes = &configured.routes;
    let chat_route = routes.get(&xai_grok_provider::types::RouteId::new("openai-chat"));
    assert!(chat_route.is_some(), "openai-chat route must exist");
    assert_eq!(
        chat_route.unwrap().protocol_id.0,
        "chat_completions",
        "chat route must use chat_completions protocol"
    );
}

#[test]
fn explicit_responses_route_returns_responses() {
    let configured = openai_configured(None);

    let routes = &configured.routes;
    let resp_route =
        routes.get(&xai_grok_provider::types::RouteId::new("openai-responses"));
    assert!(resp_route.is_some(), "openai-responses route must exist");
    assert_eq!(
        resp_route.unwrap().protocol_id.0,
        "responses",
        "responses route must use responses protocol"
    );
}

#[test]
fn generic_model_selects_chat_by_default() {
    let configured = openai_configured(None);

    let selected = configured
        .route_selector
        .select("gpt-4o")
        .expect("selector must succeed");

    assert_eq!(
        selected.0, "openai-chat",
        "generic model must select chat route"
    );
}

#[test]
fn protocol_responses_does_not_affect_default_selection() {
    let configured = openai_configured(Some("responses"));

    let selected = configured
        .route_selector
        .select("gpt-4o")
        .expect("selector must succeed");

    // BUG: protocol=responses is configured but the default route is still chat.
    // After Phase 5, setting protocol=responses should make responses the default.
    assert_eq!(
        selected.0, "openai-chat",
        "R3-RED-06: protocol=responses does NOT affect default route selection (BUG)"
    );
}

#[test]
fn no_incompatibility_error_for_conflicting_protocol_and_model() {
    let configured = openai_configured(None);

    // There is no mechanism to express "this model requires responses protocol"
    // or to check incompatibility. The selector never returns an error for
    // any model ID — it always returns the default.
    let result = configured.route_selector.select("any-nonexistent-model");
    assert!(
        result.is_ok(),
        "R3-RED-06: selector should FAIL for unknown model (BUG — no incompatibility check)"
    );

    // After Phase 5, model metadata should be able to require responses,
    // and an incompatible combination should return Err.
}
