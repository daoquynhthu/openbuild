#[allow(unused_imports)]
use super::common::*;

/// Open `/providers`, verify a mock provider configured via config.toml
/// with an env-key reference shows as ready, select its model, send a
/// prompt to the local mock server, and verify the streamed response.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn providers_pty() {
    let content = ContentController::start_with_models(vec![
        MockModel::new("mock-model-1")
            .with_api_backend("chat_completions"),
    ])
    .await
    .expect("start content");
    content
        .set_response(format!("{MOCK_RESPONSE_SENTINEL} hello from mock provider."));

    let grok_home = content.home().join(".grok");
    std::fs::create_dir_all(&grok_home).expect("create .grok");
    std::fs::write(
        grok_home.join("config.toml"),
        format!(
            r#"[provider."mock-test"]
env_key = ["MOCK_TEST_API_KEY"]
base_url = "{}"
"#,
            content.url()
        ),
    )
    .expect("write config.toml");

    let binary = pager_binary().expect("resolve pager binary");
    let env: Vec<(String, String)> = content
        .env_for_pager()
        .into_iter()
        .chain([("MOCK_TEST_API_KEY".into(), "test-key".into())])
        .collect();
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
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

    // Open /providers modal
    inject_keys_paced(&mut harness, b"/providers");
    harness.inject_keys(b"\r").expect("submit /providers");
    harness
        .wait_for_text("Providers", Duration::from_secs(10))
        .expect("providers modal title");

    // Verify mock-test provider is listed and shows as configured.
    harness
        .wait_for_text("mock-test", Duration::from_secs(5))
        .expect("mock-test provider row");
    harness
        .wait_for_text("Configured", Duration::from_secs(5))
        .expect("mock-test should show Configured");

    // Dismiss providers modal.
    harness.inject_keys(keys::ESC).expect("close providers");
    harness
        .wait_for_text(WELCOME_SCREEN_SENTINEL, Duration::from_secs(5))
        .expect("back to welcome");

    // Select the mock provider's model and send a prompt.
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
