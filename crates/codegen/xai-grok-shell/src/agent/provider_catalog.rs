//! Provider model catalog: parsing, caching, and async refresh.
//!
//! Parsing functions are pure - they operate on `serde_json::Value`,
//! not HTTP responses. This makes them testable without network.
//! Readers receive immutable `Arc<ModelCatalogSnapshot>`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use tokio::sync::RwLock;
use xai_grok_provider::types::{ProviderDefaults, ProviderId};

use super::config::{self, ModelEntryConfig};

/// Default TTL for cached model lists (300 seconds).
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Per-provider catalog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCatalogState {
    Idle,
    Refreshing,
    Ready,
    Stale,
    Error(String),
    Disabled,
}

/// Per-provider catalog entry.
#[derive(Debug, Clone)]
pub struct ProviderCatalogEntry {
    pub provider_id: ProviderId,
    pub state: ProviderCatalogState,
    pub fetched_at: Option<Instant>,
    pub source_url: String,
    pub models: Vec<ModelEntryConfig>,
    pub error_summary: Option<String>,
}

impl ProviderCatalogEntry {
    /// Returns true if the entry is stale (TTL exceeded) or has no fetch timestamp.
    pub fn is_stale(&self, ttl: Duration) -> bool {
        match self.fetched_at {
            Some(then) => then.elapsed() >= ttl,
            None => true,
        }
    }
}

/// Immutable, atomic, revisioned snapshot of the entire model catalog.
#[derive(Debug, Clone)]
pub struct ModelCatalogSnapshot {
    pub catalog_revision: u64,
    pub providers: IndexMap<ProviderId, ProviderCatalogEntry>,
}

/// Refresh strategy for the catalog service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    CacheOnly,
    RefreshIfStale,
    ForceRefresh,
}

/// Asynchronous provider catalog service with bounded concurrent refresh,
/// TTL cache, cancellation, and stale fallback.
pub struct ProviderCatalogService {
    snapshot: RwLock<Arc<ModelCatalogSnapshot>>,
    http_client: reqwest::Client,
    cancelled: Arc<AtomicBool>,
}

impl std::fmt::Debug for ProviderCatalogService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCatalogService").finish()
    }
}

/// Build a cache key for a provider's model list.
/// Includes provider ID, effective URL, revision, and credential identity class.
/// Never includes credential contents.
pub fn cache_key(provider_id: &ProviderId, url: &str, revision: u64) -> String {
    format!("{}|{}|rev{}", provider_id.0, url, revision)
}

/// Derive the model-list URL for a provider from its defaults and optional
/// user-configured base_url. OpenAI-compatible custom providers must never
/// use an empty relative "/models" URL.
pub fn derive_model_list_url(
    defaults: &ProviderDefaults,
    base_url_override: Option<&str>,
) -> String {
    if let Some(ref explicit) = defaults.model_list_endpoint {
        return explicit.clone();
    }
    let base = base_url_override
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| defaults.base_url.as_str());
    let base = base.trim_end_matches('/');
    if defaults.model_list_format == xai_grok_provider::types::ModelListFormat::OllamaTags {
        // Ollama has a separate /api/tags endpoint
        "http://localhost:11434/api/tags".into()
    } else {
        format!("{base}/models")
    }
}

impl ProviderCatalogService {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            })),
            http_client: reqwest::Client::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Return the current immutable snapshot.
    pub async fn snapshot(&self) -> Arc<ModelCatalogSnapshot> {
        self.snapshot.read().await.clone()
    }

    /// Refresh a single provider with TTL-aware strategy.
    ///
    /// - `CacheOnly`: never performs network request.
    /// - `RefreshIfStale`: uses stale snapshot if available, refreshes in background.
    /// - `ForceRefresh`: always fetches fresh data.
    pub async fn refresh_provider(
        &self,
        provider_id: ProviderId,
        url: &str,
        parse: fn(&serde_json::Value, &ProviderDefaults) -> Vec<ModelEntryConfig>,
        defaults: &ProviderDefaults,
        strategy: RefreshStrategy,
        ttl: Duration,
    ) {
        let current = self.snapshot.read().await;
        match strategy {
            RefreshStrategy::CacheOnly => return,
            RefreshStrategy::RefreshIfStale => {
                if let Some(entry) = current.providers.get(&provider_id) {
                    if !entry.is_stale(ttl) {
                        // Fresh enough — no network needed.
                        return;
                    }
                    // Stale but present — error fallback is preserved.
                }
            }
            RefreshStrategy::ForceRefresh => {}
        }
        drop(current);

        let result = self.fetch_and_parse(url, parse, defaults).await;
        let mut snap = self.snapshot.write().await;
        let mut new_snapshot = (**snap).clone();

        let pid = provider_id.clone();
        match result {
            Ok(models) => {
                new_snapshot.providers.insert(
                    pid.clone(),
                    ProviderCatalogEntry {
                        provider_id: pid,
                        state: ProviderCatalogState::Ready,
                        fetched_at: Some(Instant::now()),
                        source_url: url.to_string(),
                        models,
                        error_summary: None,
                    },
                );
            }
            Err(e) => {
                let entry =
                    new_snapshot
                        .providers
                        .get(&pid)
                        .cloned()
                        .unwrap_or(ProviderCatalogEntry {
                            provider_id: pid.clone(),
                            state: ProviderCatalogState::Error(e.clone()),
                            fetched_at: None,
                            source_url: url.to_string(),
                            models: vec![],
                            error_summary: Some(e.clone()),
                        });
                let mut entry = entry;
                entry.state = ProviderCatalogState::Error(e.clone());
                entry.error_summary = Some(e);
                new_snapshot.providers.insert(pid, entry);
            }
        }
        new_snapshot.catalog_revision += 1;
        *snap = Arc::new(new_snapshot);
    }

    /// Refresh all providers with bounded concurrency and TTL-awareness.
    /// Skips providers whose cache is still fresh.
    pub async fn refresh_all(
        &self,
        provider_ids: &[ProviderId],
        build_url: impl Fn(&ProviderId) -> Option<(String, ProviderDefaults)>,
        ttl: Duration,
    ) {
        // Collect stale providers first (outside the async loop to avoid
        // holding the snapshot lock across awaits).
        let stale_pids: Vec<ProviderId> = {
            let current = self.snapshot.read().await;
            provider_ids
                .iter()
                .filter(|pid| {
                    current
                        .providers
                        .get(*pid)
                        .map_or(true, |e| e.is_stale(ttl))
                })
                .cloned()
                .collect()
        };

        let mut handles = Vec::new();
        for pid in &stale_pids {
            let Some((url, defaults)) = build_url(pid) else {
                continue;
            };
            let pid = pid.clone();
            let client = self.http_client.clone();
            let cancelled = self.cancelled.clone();

            handles.push(tokio::spawn(async move {
                if cancelled.load(Ordering::Relaxed) {
                    return (pid.clone(), None, Some("cancelled".into()));
                }
                let response = match client.get(&url).send().await {
                    Ok(r) => r,
                    Err(e) => return (pid.clone(), None, Some(format!("HTTP error: {e}"))),
                };
                let body: serde_json::Value = match response.json().await {
                    Ok(v) => v,
                    Err(e) => return (pid.clone(), None, Some(format!("JSON error: {e}"))),
                };
                let models = if url.contains("/api/tags") {
                    crate::agent::provider_catalog::parse_ollama_tags_models(&body, &defaults)
                } else {
                    crate::agent::provider_catalog::parse_openai_compatible_provider_models(
                        &body,
                        &defaults.base_url,
                    )
                };
                (pid, Some(models), None)
            }));
        }

        // Collect results — one failure does not discard others
        for handle in handles {
            if let Ok((pid, models_opt, error)) = handle.await {
                let mut snap = self.snapshot.write().await;
                let mut new_snapshot = (**snap).clone();
                let source = build_url(&pid).map(|(u, _)| u).unwrap_or_default();
                let entry = ProviderCatalogEntry {
                    provider_id: pid.clone(),
                    state: if error.is_some() {
                        ProviderCatalogState::Error(error.clone().unwrap_or_default())
                    } else {
                        ProviderCatalogState::Ready
                    },
                    fetched_at: if error.is_some() {
                        None
                    } else {
                        Some(Instant::now())
                    },
                    source_url: source,
                    models: models_opt.unwrap_or_default(),
                    error_summary: error,
                };
                new_snapshot.providers.insert(pid, entry);
                new_snapshot.catalog_revision += 1;
                *snap = Arc::new(new_snapshot);
            }
        }
    }

    async fn fetch_and_parse(
        &self,
        url: &str,
        parse: fn(&serde_json::Value, &ProviderDefaults) -> Vec<ModelEntryConfig>,
        defaults: &ProviderDefaults,
    ) -> Result<Vec<ModelEntryConfig>, String> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parse failed: {e}"))?;
        Ok(parse(&body, defaults))
    }
}

impl Default for ProviderCatalogService {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse an OpenAI-compatible `/v1/models` response body.
/// Tolerates unknown fields, rejects malformed IDs, and never assigns secrets.
pub fn parse_openai_compatible_provider_models(
    body: &serde_json::Value,
    base_url: &str,
) -> Vec<ModelEntryConfig> {
    let data = match body.get("data").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            tracing::warn!("provider model list missing 'data' array");
            return vec![];
        }
    };

    let mut models = Vec::with_capacity(data.len());
    for (idx, value) in data.iter().enumerate() {
        match crate::remote::client::parse_remote_model_value(value, base_url) {
            Some(model) => models.push(model),
            None => {
                tracing::warn!(index = idx, "skipping unparseable model entry");
            }
        }
    }
    models
}

/// Parse an Ollama `/api/tags` response body.
/// Maps `name` to `model`, derives metadata from provider defaults.
pub fn parse_ollama_tags_models(
    body: &serde_json::Value,
    defaults: &ProviderDefaults,
) -> Vec<ModelEntryConfig> {
    let models = match body.get("models").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            tracing::warn!("Ollama response missing 'models' array");
            return vec![];
        }
    };

    let provider_api_backend = defaults.api_backend.clone();
    let mut entries = Vec::with_capacity(models.len());
    for model_value in models {
        let obj = match model_value.as_object() {
            Some(o) => o,
            None => continue,
        };
        let name = match obj.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        entries.push(ModelEntryConfig {
            id: None,
            model: name.to_owned(),
            base_url: defaults.base_url.clone(),
            name: Some(name.to_owned()),
            description: None,
            max_completion_tokens: defaults.max_completion_tokens,
            temperature: defaults.temperature,
            top_p: defaults.top_p,
            api_key: None,
            env_key: None,
            api_backend: provider_api_backend.clone(),
            auth_scheme: Some(defaults.auth_scheme),
            reasoning_effort: None,
            supports_reasoning_effort: defaults.supports_reasoning_effort,
            reasoning_efforts: vec![],
            extra_headers: defaults.extra_headers.clone(),
            context_window: defaults.context_window,
            auto_compact_threshold_percent: None,
            system_prompt_label: None,
            api_base_url: None,
            use_concise: false,
            agent_type: config::default_agent_type(),
            inference_idle_timeout_secs: None,
            max_retries: None,
            hidden: false,
            supported_in_api: true,
            supports_backend_search: defaults.supports_backend_search,
            compactions_remaining: None,
            compaction_at_tokens: None,
            show_model_fingerprint: false,
            stream_tool_calls: None,
            laziness_detector: Default::default(),
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use xai_grok_provider::types::{ApiBackend, AuthScheme, ModelListFormat, ProviderId};

    #[test]
    fn parse_openai_compatible_empty_data() {
        let body = serde_json::json!({"data": []});
        let result = parse_openai_compatible_provider_models(&body, "https://example.com/v1");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_openai_compatible_missing_data() {
        let body = serde_json::json!({});
        let result = parse_openai_compatible_provider_models(&body, "https://example.com/v1");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_openai_compatible_single_model() {
        let body = serde_json::json!({
            "data": [{"id": "gpt-4o", "model": "gpt-4o-2024-11-20"}]
        });
        let result = parse_openai_compatible_provider_models(&body, "https://api.openai.com/v1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].model, "gpt-4o-2024-11-20");
    }

    #[test]
    fn parse_ollama_tags_empty() {
        let defaults = dummy_defaults();
        let body = serde_json::json!({"models": []});
        let result = parse_ollama_tags_models(&body, &defaults);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_ollama_tags_single() {
        let defaults = dummy_defaults();
        let body = serde_json::json!({
            "models": [{"name": "llama3.1:8b", "modified_at": "2024-01-01"}]
        });
        let result = parse_ollama_tags_models(&body, &defaults);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].model, "llama3.1:8b");
        assert_eq!(result[0].base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn parse_ollama_tags_missing_models() {
        let defaults = dummy_defaults();
        let body = serde_json::json!({});
        let result = parse_ollama_tags_models(&body, &defaults);
        assert!(result.is_empty());
    }

    fn dummy_defaults() -> ProviderDefaults {
        ProviderDefaults {
            id: ProviderId::new("ollama"),
            name: "Ollama".into(),
            base_url: "http://localhost:11434/v1".into(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::None,
            context_window: NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
            extra_headers: IndexMap::new(),
            model_list_format: ModelListFormat::OllamaTags,
            ..Default::default()
        }
    }
}
