//! R3-RED-01: Reproduce nested-runtime failure through ACP session creation.
//!
//! Before the Phase 2 async fix, `prepare_sampling_config_for_model` uses
//! `Handle::block_on` internally. When invoked from within an active Tokio
//! runtime context (e.g. a spawned task), this causes a nested-runtime panic.
//!
//! This test MUST fail (panic) before the fix and pass after it.
//! The panic is caught via `tokio::spawn` + `JoinError::is_panic()`.
//!
//! MvpAgent is !Send, so we use a current-thread runtime (LocalSet) which
//! is necessary to trigger the nested-runtime panic — Handle::block_on
//! from within a current-thread runtime cannot block the only thread.

use std::sync::Arc;

use xai_acp_lib::AcpAgentGatewaySender;
use xai_grok_shell::agent::config::{Config, EndpointsConfig, ModelEntry};
use xai_grok_shell::agent::mvp_agent::MvpAgent;
use xai_grok_shell::auth::AuthManager;
use xai_grok_shell::auth::GrokComConfig;

#[test]
fn nested_runtime_panic_in_acp_session() {
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

        let mut cfg = Config::default();
        cfg.provider_runtime = Some(provider_runtime);

        let temp_dir = tempfile::tempdir().unwrap();
        let auth_manager = Arc::new(AuthManager::new(
            temp_dir.path(),
            GrokComConfig::default(),
        ));

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = AcpAgentGatewaySender::new(tx);

        MvpAgent::new(gateway, &cfg, auth_manager, None).expect("MvpAgent::new")
    });

    let model = {
        let mut entry = ModelEntry::fallback("test-model", &EndpointsConfig::default());
        entry.provider_id = Some("test-provider".to_string());
        entry
    };

    // Spawn inside the LocalSet so we stay on one thread.
    // The spawned task calls test_prepare_for_model which internally
    // does Handle::block_on — on a single-thread runtime this panics.
    let join = local.spawn_local(async move {
        agent.test_prepare_for_model(&model, None);
    });

    let result = local.block_on(&rt, async { join.await });

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
            panic!(
                "R3-RED-01: nested-runtime panic detected (pre-fix expected): {msg}"
            );
        }
        Err(e) => {
            panic!("R3-RED-01: unexpected JoinError (not a panic): {e}");
        }
    }
}
