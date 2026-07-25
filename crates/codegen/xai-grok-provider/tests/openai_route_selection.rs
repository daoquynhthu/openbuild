use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;
use xai_grok_provider::types::ProviderId;

fn openai_configured(protocol: Option<&str>) -> xai_grok_provider::provider::ConfiguredProvider {
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
    let resp_route = routes.get(&xai_grok_provider::types::RouteId::new("openai-responses"));
    assert!(resp_route.is_some(), "openai-responses route must exist");
    assert_eq!(
        resp_route.unwrap().protocol_id.0,
        "responses",
        "responses route must use responses protocol"
    );
}

#[test]
fn protocol_responses_makes_responses_default() {
    let configured = openai_configured(Some("responses"));

    let selected = configured
        .route_selector
        .select("gpt-4o")
        .expect("selector must succeed");

    assert_eq!(
        selected.0, "openai-responses",
        "R3-ROUTE-02: protocol=responses should make responses the default"
    );
}

#[test]
fn o1_model_selects_responses_route() {
    let configured = openai_configured(None);

    let selected = configured
        .route_selector
        .select("o1-preview")
        .expect("selector must succeed");
    assert_eq!(
        selected.0, "openai-responses",
        "o1-preview must select responses route"
    );

    let selected = configured
        .route_selector
        .select("o1-mini")
        .expect("selector must succeed");
    assert_eq!(
        selected.0, "openai-responses",
        "o1-mini must select responses route"
    );

    let selected = configured
        .route_selector
        .select("o3-mini")
        .expect("selector must succeed");
    assert_eq!(
        selected.0, "openai-responses",
        "o3-mini must select responses route"
    );
}

#[test]
fn generic_model_selects_chat_by_default() {
    let configured = openai_configured(None);

    let selected = configured
        .route_selector
        .select("gpt-4o")
        .expect("selector must succeed");
    assert_eq!(selected.0, "openai-chat", "gpt-4o must select chat route");

    let selected = configured
        .route_selector
        .select("any-nonexistent-model")
        .expect("selector must succeed for unknown models");
    assert_eq!(
        selected.0, "openai-chat",
        "unknown models must default to chat route"
    );
}

#[test]
fn default_route_id_matches_selector_default() {
    let configured = openai_configured(None);
    let selected = configured
        .route_selector
        .select("gpt-4o")
        .expect("selector must succeed");
    assert_eq!(
        selected.0, configured.default_route_id.0,
        "default route ID must match selector default for generic models"
    );
}

#[test]
fn default_route_id_is_responses_when_protocol_responses() {
    let configured = openai_configured(Some("responses"));
    assert_eq!(
        configured.default_route_id.0, "openai-responses",
        "default_route_id must be responses when protocol=responses"
    );
}

/// R3-ROUTE-02: Responses-required models cannot reach Chat Completions.
///
/// An o1 model must always select the responses route, never the chat route.
/// This is verified at both the selector level (unit) and the registry level
/// (prepare() rejects selectors whose referenced routes are incomplete).
#[test]
fn o1_model_selects_only_responses() {
    let configured = openai_configured(None);

    // o1-preview → responses
    let selected = configured
        .route_selector
        .select("o1-preview")
        .expect("o1 must select a route");
    assert_eq!(
        selected.0, "openai-responses",
        "o1-preview must NEVER route to chat"
    );
    // o3-mini → responses
    let selected = configured
        .route_selector
        .select("o3-mini")
        .expect("o3 must select a route");
    assert_eq!(
        selected.0, "openai-responses",
        "o3-mini must NEVER route to chat"
    );
}
