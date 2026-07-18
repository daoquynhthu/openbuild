#[allow(unused_imports)]
use super::common::*;

/// Open `/providers`, verify a built-in provider configured with a local
/// mock base URL and env-key reference shows as ready, select its model,
/// send a prompt to the local mock server, and verify the response.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn providers_pty() {
    let content = ContentController::start_with_models(vec![
        // Use the default api_backend (chat_completions) — this is the
        // same wire protocol the openai-compatible provider uses.
        MockModel::new("mock-model-1"),
    ])
    .await
    .expect("start content");
    content
        .set_response(format!("{MOCK_RESPONSE_SENTINEL} hello from mock provider."));

    // Prewrite config.toml: point the built-in openai-compatible provider
    // at the local mock server and prefer a custom env var for the API key
    // (non-secret reference — the value lives in the environment, not TOML).
    // env_for_pager() already sets XAI_API_KEY, which the openai-compatible
    // provider's auth policy reads by default.
    let grok_home = content.home().join(".grok");
    std::fs::create_dir_all(&grok_home).expect("create .grok");
    std::fs::write(
        grok_home.join("config.toml"),
        format!(
            r#"[provider."openai-compatible"]
env_key = ["MOCK_PROVIDER_KEY"]
base_url = "{}"
"#,
            content.url()
        ),
    )
    .expect("write config.toml");

    let binary = pager_binary().expect("resolve pager binary");
    let env = content.env_for_pager();
    let env_refs: Vec<(&str, &str)> = env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut harness = PtyHarness::new(
        &binary,
        DEFAULT_ROWS,
        DEFAULT_COLS,
        &[],
        &env_refs,
    )
    .expect("spawn pager");

    harness
        .wait_for_text(WELCOME_SCREEN_SENTINEL, WELCOME_TIMEOUT)
        .expect("welcome text");

    // 1. Open /providers modal
    inject_keys_paced(&mut harness, b"/providers");
    harness.inject_keys(b"\r").expect("submit /providers");
    harness
        .wait_for_text("Providers", Duration::from_secs(10))
        .expect("providers modal title");

    // 2. Verify the openai-compatible provider is listed, shows as
    //    Configured (env_key + XAI_API_KEY set), and has a model count > 0.
    harness
        .wait_for_text("OpenAI-Compatible", Duration::from_secs(5))
        .expect("OpenAI-Compatible provider row");
    harness
        .wait_for_text("Configured", Duration::from_secs(5))
        .expect("provider should show Configured");
    harness
        .wait_for_text("1", Duration::from_secs(5))
        .expect("model count should be 1");

    // 3. Dismiss providers modal.
    harness.inject_keys(keys::ESC).expect("close providers");
    harness
        .wait_for_text(WELCOME_SCREEN_SENTINEL, Duration::from_secs(5))
        .expect("back to welcome");

    // 4. Select the mock model and send a prompt.
    harness
        .inject_keys(b"/model mock-model-1\r")
        .expect("select mock model");
    harness
        .inject_keys(format!("{PROMPT}\r").as_bytes())
        .expect("submit prompt");
    harness
        .wait_for_text(MOCK_RESPONSE_SENTINEL, Duration::from_secs(30))
        .expect("response from mock provider");

    assert!(
        !harness.contains_text("panicked"),
        "pager panicked\nscreen:\n{}",
        harness.screen_contents()
    );

    harness.quit().expect("clean quit");
}
