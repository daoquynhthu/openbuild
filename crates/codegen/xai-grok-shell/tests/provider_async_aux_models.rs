//! R3-RED-03: Reproduce auxiliary-model and web-search blocking paths.
//!
//! Both `resolve_aux_model_sampling_config` and
//! `resolve_web_search_sampling_config` use `Handle::block_on`
//! internally.  When invoked from within an active tokio context
//! (single-thread LocalSet), this triggers a nested-runtime panic.
//!
//! The test must prove these paths do not silently construct a
//! separate xAI model when a provider-bound auxiliary model is
//! configured — before the Phase 2 fix they cannot even complete
//! preparation.
//!
//! This test MUST fail (panic) before the Phase 2 async fix.
//! After the fix, it must return a typed result without panicking.

use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_shell::agent::config::{
    EndpointsConfig, ModelEntry, resolve_aux_model_sampling_config,
    resolve_web_search_sampling_config,
};

fn build_agent_and_snapshot() -> (
    tokio::runtime::Runtime,
    tokio::task::LocalSet,
    std::sync::Arc<xai_grok_provider::registry::RegistrySnapshot>,
) {
    let rt = tokio::runtime::Runtime::new().expect("threaded runtime");
    let local = tokio::task::LocalSet::new();

    let snapshot = local.block_on(&rt, async {
        let toml_str = r#"
            [provider.test-aux]
            implementation = "openai-compatible"
            base_url = "http://127.0.0.1:9999/v1"
            api_key = "test-aux-key"

            [provider.test-aux.models.aux-model]
            context_window = 128000
            max_output_tokens = 8192
        "#;
        let toml: toml::Value = toml::from_str(toml_str).unwrap();

        let provider_runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap_from_config must succeed");

        Arc::clone(&provider_runtime.snapshot())
    });

    (rt, local, snapshot)
}

fn make_model_entry(provider_id: &str, model: &str) -> ModelEntry {
    let mut entry = ModelEntry::fallback(model, &EndpointsConfig::default());
    entry.provider_id = Some(provider_id.to_string());
    entry
}

const AUX_MODEL: &str = "aux-model";
const WEB_SEARCH_MODEL: &str = "grok-search";

#[test]
fn aux_model_nested_runtime_panic() {
    let (rt, local, snapshot) = build_agent_and_snapshot();

    let mut catalog = IndexMap::new();
    catalog.insert(
        AUX_MODEL.to_string(),
        make_model_entry("test-aux", AUX_MODEL),
    );

    let endpoints = EndpointsConfig::default();

    let join = local.spawn_local(async move {
        let _result = resolve_aux_model_sampling_config(
            AUX_MODEL,
            &catalog,
            &endpoints,
            None,  // session_key
            false, // disable_api_key_auth
            None,  // alpha_test_key
            None,  // client_version
            Some(&snapshot),
        );
    });

    let result = local.block_on(&rt, join);

    match result {
        Ok(()) => {}
        Err(e) if e.is_panic() => {
            let boxed: Box<dyn std::any::Any + Send> = e.into_panic();
            let msg = if let Some(s) = boxed.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = boxed.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            panic!("R3-RED-03 aux: nested-runtime panic (pre-fix expected): {msg}");
        }
        Err(e) => panic!("R3-RED-03 aux: unexpected JoinError: {e}"),
    }
}

#[test]
fn web_search_nested_runtime_panic() {
    let (rt, local, snapshot) = build_agent_and_snapshot();

    let mut catalog = IndexMap::new();
    catalog.insert(
        WEB_SEARCH_MODEL.to_string(),
        make_model_entry("test-aux", WEB_SEARCH_MODEL),
    );

    let endpoints = EndpointsConfig::default();

    let join = local.spawn_local(async move {
        let _result = resolve_web_search_sampling_config(
            WEB_SEARCH_MODEL,
            &catalog,
            Some("session-token"),
            false, // disable_api_key_auth
            None,  // alpha_test_key
            None,  // client_version
            &endpoints,
            Some(&snapshot),
        );
    });

    let result = local.block_on(&rt, join);

    match result {
        Ok(()) => {}
        Err(e) if e.is_panic() => {
            let boxed: Box<dyn std::any::Any + Send> = e.into_panic();
            let msg = if let Some(s) = boxed.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = boxed.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            panic!("R3-RED-03 web: nested-runtime panic (pre-fix expected): {msg}");
        }
        Err(e) => panic!("R3-RED-03 web: unexpected JoinError: {e}"),
    }
}
