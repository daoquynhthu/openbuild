use std::sync::Arc;
use std::sync::OnceLock;

use indexmap::IndexMap;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_provider::types::ProviderId;
use xai_grok_shell::agent::provider_runtime::ProviderRuntime;

/// Global singleton runtime.
static PROVIDER_RUNTIME: OnceLock<Arc<ProviderRuntime>> = OnceLock::new();

/// Initialize the global provider runtime. Returns an error if already set.
pub fn init(runtime: Arc<ProviderRuntime>) -> Result<(), &'static str> {
    PROVIDER_RUNTIME
        .set(runtime)
        .map_err(|_| "provider_state::init called more than once")
}

/// Access the global provider runtime.
pub fn runtime() -> Option<&'static Arc<ProviderRuntime>> {
    PROVIDER_RUNTIME.get()
}

pub fn configured_providers() -> Vec<String> {
    match PROVIDER_RUNTIME.get() {
        Some(rt) => rt.registry.all_ids().into_iter().map(|p| p.0).collect(),
        None => vec![],
    }
}

/// Credential state for UI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialState {
    NotRequired,
    Configured,
    Missing,
    Session,
    Public,
}

/// Config source for UI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    None,
    Env,
    Toml,
    Cli,
    Builtin,
}

/// Catalog state for UI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogState {
    Idle,
    Fetching,
    Stale,
    Failed(String),
}

/// UI-safe view of a single provider.
#[derive(Debug, Clone)]
pub struct ProviderView {
    pub id: ProviderId,
    pub display_name: String,
    pub endpoint: String,
    pub config_source: ConfigSource,
    pub credential: CredentialState,
    pub catalog: CatalogState,
    pub catalog_last_refresh: Option<String>,
    pub route_count: usize,
    pub model_count: usize,
    pub last_error: Option<String>,
    pub registry_revision: u64,
    pub catalog_revision: u64,
    pub env_key: Vec<String>,
}

/// Runtime-backed provider state container.
#[derive(Debug)]
pub struct ProviderState {
    runtime: Arc<ProviderRuntime>,
    views: IndexMap<ProviderId, ProviderView>,
    revision: u64,
}

impl ProviderState {
    pub fn new(runtime: Arc<ProviderRuntime>) -> Self {
        let mut state = Self {
            runtime,
            views: IndexMap::new(),
            revision: 0,
        };
        state.refresh();
        state
    }

    /// Rebuild all views from the current registry snapshot.
    pub fn refresh(&mut self) {
        let snapshot = self.runtime.registry.snapshot();
        self.revision = snapshot.revision;
        let provider_count = snapshot.providers.len();
        let route_count: usize = snapshot.routes.len();
        tracing::debug!(
            revision = self.revision,
            provider_count,
            route_count,
            "ProviderState::refresh"
        );
        let mut views: IndexMap<ProviderId, ProviderView> = IndexMap::new();

        for (pid, configured) in &snapshot.providers {
            let (credential, config_source) = classify_credential(configured);
            let model_count = configured.routes.len();
            let default_route = configured.routes.get(&configured.default_route_id);
            let endpoint = default_route
                .map(|r| {
                    let raw = r.endpoint.base_url.as_deref().unwrap_or_default();
                    redact_endpoint(raw)
                })
                .unwrap_or_default();
            views.insert(
                pid.clone(),
                ProviderView {
                    id: pid.clone(),
                    display_name: configured.display_name.clone(),
                    endpoint,
                    config_source,
                    credential,
                    catalog: CatalogState::Idle,
                    catalog_last_refresh: None,
                    route_count: configured.routes.len(),
                    model_count,
                    last_error: None,
                    registry_revision: snapshot.revision,
                    catalog_revision: 0,
                    env_key: configured.config.env_key.clone().unwrap_or_default(),
                },
            );
        }

        // Add any registered-but-not-configured providers
        for pid in self.runtime.registry.all_ids() {
            if !views.contains_key(&pid) {
                let display_name = self
                    .runtime
                    .registry
                    .get(&pid)
                    .map(|p| p.name().to_string())
                    .unwrap_or_else(|| pid.0.clone());
                views.insert(
                    pid.clone(),
                    ProviderView {
                        id: pid.clone(),
                        display_name,
                        endpoint: String::new(),
                        config_source: ConfigSource::None,
                        credential: CredentialState::Missing,
                        catalog: CatalogState::Idle,
                        catalog_last_refresh: None,
                        route_count: 0,
                        model_count: 0,
                        last_error: None,
                        registry_revision: snapshot.revision,
                        catalog_revision: 0,
                        env_key: vec![],
                    },
                );
            }
        }

        self.views = views;
    }

    pub fn views(&self) -> &IndexMap<ProviderId, ProviderView> {
        &self.views
    }

    pub fn view_for(&self, id: &ProviderId) -> Option<&ProviderView> {
        self.views.get(id)
    }

    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        self.runtime.registry.snapshot()
    }

    pub fn registry(&self) -> Arc<xai_grok_provider::registry::ProviderRegistry> {
        self.runtime.registry.clone()
    }

    pub fn runtime_ref(&self) -> &Arc<ProviderRuntime> {
        &self.runtime
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn ordered_views(&self) -> Vec<&ProviderView> {
        self.views.values().collect()
    }

    /// Update catalog state for a provider. Called by effects after refresh.
    pub fn update_catalog(&mut self, pid: &ProviderId, state: CatalogState, revision: u64) {
        if let Some(view) = self.views.get_mut(pid) {
            view.catalog = state;
            view.catalog_revision = revision;
        }
    }

    /// Update the last error for a provider.
    pub fn update_error(&mut self, pid: &ProviderId, error: Option<String>) {
        if let Some(view) = self.views.get_mut(pid) {
            view.last_error = error;
        }
    }
}

fn classify_credential(
    configured: &xai_grok_provider::provider::ConfiguredProvider,
) -> (CredentialState, ConfigSource) {
    let source = if configured.config.api_key.is_some() || configured.config.base_url.is_some() {
        ConfigSource::Toml
    } else if configured.config.id.as_deref() == Some("opencode") {
        return (CredentialState::Public, ConfigSource::Builtin);
    } else if configured.config.id.as_deref() == Some("ollama") {
        return (CredentialState::NotRequired, ConfigSource::Builtin);
    } else {
        let provider_id = configured.id.0.as_str();
        let env_key = match provider_id {
            "xai" => "XAI_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "opencode" => "OPENCODE_API_KEY",
            _ => return (CredentialState::Missing, ConfigSource::None),
        };
        if std::env::var(env_key).is_ok() {
            return (CredentialState::Configured, ConfigSource::Env);
        }
        return (CredentialState::Missing, ConfigSource::None);
    };

    let credential = if configured.config.api_key.is_some() {
        CredentialState::Configured
    } else {
        CredentialState::Missing
    };

    (credential, source)
}

/// Remove sensitive query parameters from endpoint URLs.
fn redact_endpoint(raw: &str) -> String {
    if let Ok(mut url) = url::Url::parse(raw) {
        if url.has_authority() {
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }
        // Strip all query params to avoid leaking secrets
        url.set_query(None);
        url.to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use xai_grok_provider::config::ProviderConfig;

    fn real_runtime() -> Arc<ProviderRuntime> {
        let rt = Arc::new(ProviderRuntime::new());
        xai_grok_provider::providers::register_all(&rt.registry);
        let mut configs: IndexMap<ProviderId, ProviderConfig> = IndexMap::new();
        for pid in rt.registry.all_ids() {
            let mut cfg = ProviderConfig::default();
            cfg.id = Some(pid.0.clone());
            configs.insert(pid, cfg);
        }
        rt.registry.rebuild(&configs).unwrap();
        rt
    }

    #[test]
    fn redact_endpoint_removes_userinfo() {
        let result = redact_endpoint("https://user:pass@api.x.ai/v1");
        assert!(
            !result.contains("user:pass"),
            "should remove userinfo: {result}"
        );
        assert!(result.contains("api.x.ai"), "should keep host");
    }

    #[test]
    fn redact_endpoint_removes_query_params() {
        let result = redact_endpoint("https://api.x.ai/v1?api_key=sk-test&model=grok");
        assert!(
            !result.contains("api_key=sk-test"),
            "should remove api_key: {result}"
        );
        assert!(!result.contains("?api_key"), "no leftover query");
    }

    #[test]
    fn redact_endpoint_preserves_clean_url() {
        let result = redact_endpoint("https://api.x.ai/v1");
        assert_eq!(result, "https://api.x.ai/v1");
    }

    #[test]
    fn redact_endpoint_handles_ollama_localhost() {
        let result = redact_endpoint("http://localhost:11434");
        assert_eq!(result, "http://localhost:11434/");
    }

    #[test]
    fn provider_state_empty_registry_has_no_views() {
        let rt = Arc::new(ProviderRuntime::new());
        let state = ProviderState::new(rt);
        assert_eq!(state.ordered_views().len(), 0);
    }

    #[test]
    fn provider_state_real_providers_have_views() {
        let rt = real_runtime();
        let state = ProviderState::new(rt);
        assert!(
            state.ordered_views().len() >= 5,
            "expected at least 5 providers (xai, openai, anthropic, opencode, ollama)"
        );
    }

    #[test]
    fn provider_state_ordering_deterministic() {
        let rt = real_runtime();
        let s1 = ProviderState::new(rt.clone());
        let s2 = ProviderState::new(rt);
        let ids1: Vec<&str> = s1.ordered_views().iter().map(|v| v.id.0.as_str()).collect();
        let ids2: Vec<&str> = s2.ordered_views().iter().map(|v| v.id.0.as_str()).collect();
        assert_eq!(ids1, ids2, "provider order must be deterministic");
    }

    #[test]
    fn provider_state_update_error_persists() {
        let rt = real_runtime();
        let mut state = ProviderState::new(rt);
        let pid = ProviderId::new("xai");
        state.update_error(&pid, Some("something went wrong".into()));
        let view = state.view_for(&pid).expect("xai view should exist");
        assert_eq!(view.last_error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn provider_state_update_error_resets_to_none() {
        let rt = real_runtime();
        let mut state = ProviderState::new(rt);
        let pid = ProviderId::new("xai");
        state.update_error(&pid, Some("old error".into()));
        state.update_error(&pid, None);
        let view = state.view_for(&pid).expect("xai view should exist");
        assert_eq!(view.last_error, None);
    }

    #[test]
    fn provider_state_update_error_unknown_id_noop() {
        let rt = real_runtime();
        let mut state = ProviderState::new(rt);
        state.update_error(&ProviderId::new("unknown"), Some("error".into()));
        assert!(state.view_for(&ProviderId::new("unknown")).is_none());
    }

    #[test]
    fn provider_state_update_catalog_transitions() {
        let rt = real_runtime();
        let mut state = ProviderState::new(rt);
        let pid = ProviderId::new("xai");

        // Initially Idle
        let view = state.view_for(&pid).expect("xai view");
        assert_eq!(view.catalog, CatalogState::Idle);

        // Fetching
        state.update_catalog(&pid, CatalogState::Fetching, 1);
        let view = state.view_for(&pid).expect("xai view");
        assert_eq!(view.catalog, CatalogState::Fetching);
        assert_eq!(view.catalog_revision, 1);

        // Stale
        state.update_catalog(&pid, CatalogState::Stale, 2);
        let view = state.view_for(&pid).expect("xai view");
        assert_eq!(view.catalog, CatalogState::Stale);
        assert_eq!(view.catalog_revision, 2);

        // Failed with error
        state.update_catalog(&pid, CatalogState::Failed("timeout".into()), 3);
        let view = state.view_for(&pid).expect("xai view");
        assert_eq!(view.catalog, CatalogState::Failed("timeout".into()));
        assert_eq!(view.catalog_revision, 3);

        // Back to Idle
        state.update_catalog(&pid, CatalogState::Idle, 4);
        let view = state.view_for(&pid).expect("xai view");
        assert_eq!(view.catalog, CatalogState::Idle);
        assert_eq!(view.catalog_revision, 4);
    }

    #[test]
    fn provider_state_refresh_does_not_panic() {
        let rt = real_runtime();
        let mut state = ProviderState::new(rt);
        state.refresh();
        assert!(!state.ordered_views().is_empty());
    }

    #[test]
    fn provider_state_uses_same_runtime_arc() {
        let rt = real_runtime();
        let state = ProviderState::new(rt.clone());
        assert!(
            Arc::ptr_eq(&rt, state.runtime_ref()),
            "ProviderState must hold the same Arc<ProviderRuntime>"
        );
    }

    #[test]
    fn provider_view_credential_state_missing_by_default() {
        let rt = real_runtime();
        let state = ProviderState::new(rt);
        for view in state.ordered_views() {
            match view.id.0.as_str() {
                "opencode" => assert_eq!(
                    view.credential,
                    CredentialState::Public,
                    "opencode is public"
                ),
                "ollama" => assert_eq!(
                    view.credential,
                    CredentialState::NotRequired,
                    "ollama is not-required"
                ),
                _ => {
                    assert_eq!(
                        view.credential,
                        CredentialState::Missing,
                        "{} should be Missing",
                        view.id.0
                    );
                }
            }
        }
    }
}
