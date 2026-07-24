use std::sync::Arc;

use serial_test::serial;

/// Full lifecycle test for the provider runtime injection.
/// Since runtime() uses a global OnceLock, all lifecycle steps
/// must be in one test to avoid ordering issues.
#[serial]
#[test]
fn runtime_injection_lifecycle() {
    // Before init: runtime is None
    assert!(
        xai_grok_pager::provider_state::runtime().is_none(),
        "R3-RED-11: before init, no runtime"
    );

    // Before init: configured_providers is empty
    assert!(
        xai_grok_pager::provider_state::configured_providers().is_empty(),
        "R3-RED-11: before init, empty providers"
    );

    // Init succeeds
    let rt = Arc::new(xai_grok_shell::agent::provider_runtime::ProviderRuntime::new());
    assert!(
        xai_grok_pager::provider_state::init(rt).is_ok(),
        "R3-RED-11: first init succeeds"
    );

    // After init: runtime is Some
    assert!(
        xai_grok_pager::provider_state::runtime().is_some(),
        "R3-RED-11: after init, runtime available"
    );

    // Duplicate init returns error
    let rt2 = Arc::new(xai_grok_shell::agent::provider_runtime::ProviderRuntime::new());
    assert!(
        xai_grok_pager::provider_state::init(rt2).is_err(),
        "R3-RED-11: duplicate init returns error"
    );
}
