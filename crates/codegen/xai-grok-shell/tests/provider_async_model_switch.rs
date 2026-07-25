//! R3-RED-02: Reproduce model-switch nested-runtime failure.
//!
//! The model switch handler (`model_switch::apply` at line 116) calls
//! `prepare_sampling_config_for_model` (sync, uses Handle::block_on)
//! inside an async context. This test exercises that exact code path
//! through the model_switch handler's test seam
//! (`test_switch_model_prepare`), proving the nested-runtime panic
//! from within an active tokio context on a single-thread LocalSet.
//!
//! This test MUST fail (panic) before the Phase 2 async fix.
//! After the fix, it must return a typed result without panicking.

use std::sync::Arc;

use xai_grok_shell::agent::config::{Config, EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::mvp_agent::MvpAgent;
use xai_grok_shell::auth::AuthManager;
use xai_grok_shell::auth::GrokComConfig;

fn build_agent() -> (tokio::runtime::Runtime, tokio::task::LocalSet, MvpAgent) {
    let rt = tokio::runtime::Runtime::new().expect("threaded runtime");
    let local = tokio::task::LocalSet::new();

    let agent = local.block_on(&rt, async {
        let toml_str = r#"
            [provider.test-provider]
            implementation = "openai-compatible"
            base_url = "http://127.0.0.1:9999/v1"
            api_key = "test-key-123"

            [provider.test-provider.models.test-model]
            context_window = 128000
            max_output_tokens = 8192
        "#;
        let toml: toml::Value = toml::from_str(toml_str).unwrap();

        let provider_runtime =
            xai_grok_shell::agent::provider_bootstrap::bootstrap_from_config(&toml, None, None)
                .await
                .expect("bootstrap_from_config must succeed");

        let cfg = Config {
            provider_runtime: Some(provider_runtime),
            ..Default::default()
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let auth_manager = Arc::new(AuthManager::new(temp_dir.path(), GrokComConfig::default()));

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = xai_acp_lib::AcpAgentGatewaySender::new(tx);

        MvpAgent::new(gateway, &cfg, auth_manager, None).expect("MvpAgent::new")
    });

    (rt, local, agent)
}

#[test]
fn model_switch_nested_runtime_panic() {
    let (rt, local, agent) = build_agent();

    let model = {
        let mut entry = ModelEntry::fallback("test-model", &EndpointsConfig::default());
        entry.provider_id = Some("test-provider".to_string());
        entry
    };

    // model_switch::apply (line 116) calls prepare_sampling_config_for_model.
    // We exercise the same code path through the model_switch test seam.
    let join = local.spawn_local(async move {
        let _ = agent.test_switch_model_prepare(&model, None).await;
    });

    let result = local.block_on(&rt, join);

    match result {
        Ok(()) => {
            // After Phase 2: no panic — the test passes.
        }
        Err(e) if e.is_panic() => {
            let panic_box: Box<dyn std::any::Any + Send> = e.into_panic();
            let msg = if let Some(s) = panic_box.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_box.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            panic!("R3-RED-02 model_switch path: nested-runtime panic (pre-fix expected): {msg}");
        }
        Err(e) => {
            panic!("R3-RED-02: unexpected JoinError (not a panic): {e}");
        }
    }
}
