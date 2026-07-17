use std::sync::Arc;
use std::sync::OnceLock;

use indexmap::IndexMap;
use xai_grok_provider::registry::{ProviderRegistry, RegistrySnapshot};
use xai_grok_provider::types::ProviderId;

/// Global singleton registry (legacy accessor).
static PROVIDER_REGISTRY: OnceLock<Arc<ProviderRegistry>> = OnceLock::new();

/// Initialize the global provider registry. Returns an error if already set.
pub fn init(registry: Arc<ProviderRegistry>) -> Result<(), &'static str> {
    PROVIDER_REGISTRY
        .set(registry)
        .map_err(|_| "provider_state::init called more than once")
}

pub fn registry() -> Option<&'static Arc<ProviderRegistry>> {
    PROVIDER_REGISTRY.get()
}

pub fn configured_providers() -> Vec<String> {
    match PROVIDER_REGISTRY.get() {
        Some(reg) => reg.all_ids().into_iter().map(|p| p.0).collect(),
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
    pub model_count: usize,
    pub last_error: Option<String>,
    pub registry_revision: u64,
    pub catalog_revision: u64,
    pub env_key: Vec<String>,
}

/// Runtime-backed provider state container.
#[derive(Debug)]
pub struct ProviderState {
    registry: Arc<ProviderRegistry>,
    views: IndexMap<ProviderId, ProviderView>,
    revision: u64,
}

impl ProviderState {
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        let mut state = Self {
            registry,
            views: IndexMap::new(),
            revision: 0,
        };
        state.refresh();
        state
    }

    /// Rebuild all views from the current registry snapshot.
    pub fn refresh(&mut self) {
        let snapshot = self.registry.snapshot();
        self.revision = snapshot.revision;
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
                    model_count,
                    last_error: None,
                    registry_revision: snapshot.revision,
                    catalog_revision: 0,
                    env_key: configured.config.env_key.clone().unwrap_or_default(),
                },
            );
        }

        // Add any registered-but-not-configured providers
        for pid in self.registry.all_ids() {
            if !views.contains_key(&pid) {
                let display_name = self
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
        self.registry.snapshot()
    }

    pub fn registry(&self) -> &Arc<ProviderRegistry> {
        &self.registry
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

    #[test]
    fn redact_endpoint_removes_userinfo() {
        let result = redact_endpoint("https://user:pass@api.x.ai/v1");
        assert!(!result.contains("user:pass"), "should remove userinfo: {result}");
        assert!(result.contains("api.x.ai"), "should keep host");
    }

    #[test]
    fn redact_endpoint_removes_query_params() {
        let result = redact_endpoint("https://api.x.ai/v1?api_key=sk-test&model=grok");
        assert!(!result.contains("api_key=sk-test"), "should remove api_key: {result}");
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
}
