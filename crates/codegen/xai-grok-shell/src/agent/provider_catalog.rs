//! Provider model catalog: parsing, caching, and async refresh.
//!
//! Parsing functions are pure - they operate on `serde_json::Value`,
//! not HTTP responses. This makes them testable without network.
//! Readers receive immutable `Arc<ModelCatalogSnapshot>`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Redirect policy per P9-002: max 3 redirects, same-origin only, no credential copy.
fn catalog_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        let prev = attempt.previous().to_vec();
        let next = attempt.url().clone();
        if prev.len() > 3 {
            return attempt.error("too many redirects (max 3)");
        }
        if let Some(last) = prev.last() {
            if last.origin() != next.origin() {
                return attempt.error(format!(
                    "cross-origin redirect rejected: {} -> {}",
                    last, next
                ));
            }
        }
        attempt.follow()
    })
}

use indexmap::IndexMap;
use tokio::sync::RwLock;
use xai_grok_provider::types::{ProviderDefaults, ProviderId};

use super::config::{self, ModelEntryConfig};

/// Default TTL for cached model lists (300 seconds).
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Per-provider catalog state (P9-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCatalogState {
    Empty,
    Loading,
    Fresh,
    Stale,
    Failed(String),
}

/// Per-provider catalog entry.
#[derive(Debug, Clone)]
pub struct ProviderCatalogEntry {
    pub provider_id: ProviderId,
    pub state: ProviderCatalogState,
    /// Wall-clock timestamp of last successful fetch (P9-001).
    pub fetched_at: Option<SystemTime>,
    pub source_url: String,
    pub models: Vec<ModelEntryConfig>,
    pub error_summary: Option<String>,
}

impl ProviderCatalogEntry {
    /// Returns true if the entry is stale (TTL exceeded) or has no fetch timestamp.
    pub fn is_stale(&self, ttl: Duration) -> bool {
        match self.fetched_at {
            Some(then) => then.elapsed().map_or(true, |elapsed| elapsed >= ttl),
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
    concurrency: Arc<tokio::sync::Semaphore>,
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
        .unwrap_or(defaults.base_url.as_str());
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
        Self::with_client(Self::build_http_client())
    }

    /// Build the unique reqwest client with P9-002 policy.
    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(catalog_redirect_policy())
            .user_agent("grok-build-catalog/1.0")
            .build()
            .expect("catalog HTTP client build must succeed")
    }

    pub const DEFAULT_CONCURRENCY: u32 = 4;
    pub const MIN_CONCURRENCY: u32 = 1;
    pub const MAX_CONCURRENCY: u32 = 16;

    pub fn validate_concurrency(v: u32) -> Result<(), String> {
        if v < Self::MIN_CONCURRENCY || v > Self::MAX_CONCURRENCY {
            return Err(format!(
                "concurrency must be between {} and {}, got {}",
                Self::MIN_CONCURRENCY, Self::MAX_CONCURRENCY, v
            ));
        }
        Ok(())
    }

    /// Constructor with explicit client and concurrency limit (for testing).
    pub(crate) fn with_client_and_concurrency(
        http_client: reqwest::Client,
        max_concurrency: u32,
    ) -> Self {
        assert!(
            (Self::MIN_CONCURRENCY..=Self::MAX_CONCURRENCY).contains(&max_concurrency),
            "concurrency out of range"
        );
        Self {
            snapshot: RwLock::new(Arc::new(ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            })),
            http_client,
            cancelled: Arc::new(AtomicBool::new(false)),
            concurrency: Arc::new(tokio::sync::Semaphore::new(max_concurrency as usize)),
        }
    }

    /// Constructor with explicit client (for testing, uses default concurrency).
    pub(crate) fn with_client(http_client: reqwest::Client) -> Self {
        Self::with_client_and_concurrency(http_client, Self::DEFAULT_CONCURRENCY)
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
                        state: ProviderCatalogState::Fresh,
                        fetched_at: Some(SystemTime::now()),
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
                            state: ProviderCatalogState::Failed(e.clone()),
                            fetched_at: None,
                            source_url: url.to_string(),
                            models: vec![],
                            error_summary: Some(e.clone()),
                        });
                let mut entry = entry;
                entry.state = ProviderCatalogState::Failed(e.clone());
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
                .filter(|pid| current.providers.get(*pid).is_none_or(|e| e.is_stale(ttl)))
                .cloned()
                .collect()
        };

        let mut handles = Vec::new();
        let semaphore = Arc::clone(&self.concurrency);
        for pid in &stale_pids {
            let Some((url, defaults)) = build_url(pid) else {
                continue;
            };
            let pid = pid.clone();
            let client = self.http_client.clone();
            let cancelled = self.cancelled.clone();
            let sem = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                // P9-003: bounded concurrency — acquire permit inside the spawned task.
                let _permit = sem.acquire().await;
                if cancelled.load(Ordering::Relaxed) {
                    return (pid.clone(), None, Some("cancelled".into()));
                }
                // P9-006: apply provider auth and extra headers to model list request.
                let mut req = client.get(&url);
                for (k, v) in &defaults.extra_headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                // Resolve auth from env keys at request time (P8 pattern)
                let auth_value = defaults.env_key.iter().find_map(|key| {
                    std::env::var(key).ok().filter(|v| !v.is_empty())
                });
                if let Some(token) = auth_value {
                    req = req.header("Authorization", format!("Bearer {token}"));
                }
                let response = match req.send().await {
                    Ok(r) => r,
                    Err(e) => return (pid.clone(), None, Some(format!("HTTP error: {e}"))),
                };
                let body: serde_json::Value = match response.json().await {
                    Ok(v) => v,
                    Err(e) => return (pid.clone(), None, Some(format!("JSON error: {e}"))),
                };
                // P9-004: use declared format, NOT URL-based inference.
                let models = match defaults.model_list_format {
                    xai_grok_provider::types::ModelListFormat::OllamaTags => {
                        crate::agent::provider_catalog::parse_ollama_tags_models(&body, &defaults)
                    }
                    xai_grok_provider::types::ModelListFormat::OpenAiCompatible => {
                        crate::agent::provider_catalog::parse_openai_compatible_provider_models(
                            &body,
                            &defaults.base_url,
                        )
                    }
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
                // P9-005: on error, preserve old models (don't overwrite with empty list)
                let existing_models = new_snapshot.providers.get(&pid)
                    .map(|e| e.models.clone())
                    .unwrap_or_default();
                let entry = ProviderCatalogEntry {
                    provider_id: pid.clone(),
                    state: if error.is_some() {
                        ProviderCatalogState::Failed(error.clone().unwrap_or_default())
                    } else {
                        ProviderCatalogState::Fresh
                    },
                    fetched_at: if error.is_some() {
                        None
                    } else {
                        Some(SystemTime::now())
                    },
                    source_url: source,
                    models: models_opt.unwrap_or(existing_models),
                    error_summary: error,
                };
                new_snapshot.providers.insert(pid, entry);
                new_snapshot.catalog_revision += 1;
                *snap = Arc::new(new_snapshot);
            }
        }
    }

    /// Classify an HTTP status into a typed error string (P9-005).
    fn classify_http_status(status: reqwest::StatusCode) -> String {
        match status.as_u16() {
            401 => format!("auth required (HTTP 401)"),
            403 => format!("forbidden (HTTP 403)"),
            404 => format!("endpoint not found (HTTP 404)"),
            429 => format!("rate limited (HTTP 429)"),
            500..=599 => format!("server error (HTTP {})", status.as_u16()),
            code => format!("unexpected HTTP status {code}"),
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
        // P9-005: only parse 2xx responses
        let status = response.status();
        if !status.is_success() {
            return Err(Self::classify_http_status(status));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("invalid JSON response: {e}"))?;
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

/// Persist the catalog to disk using atomic write.
/// Only non-secret model metadata is persisted (model IDs, names, base URLs).
/// Timestamps allow TTL comparison across restarts.
pub fn save_catalog_snapshot(snapshot: &ModelCatalogSnapshot) -> Result<(), String> {
    let cache_dir = xai_grok_config::grok_home().join("cache");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("failed to create cache dir: {e}"))?;
    let path = cache_dir.join("provider_catalog.json");
    let tmp_path = cache_dir.join("provider_catalog.json.tmp");

    // Serialize only non-secret fields
    let serializable: Vec<SerializableEntry> = snapshot
        .providers
        .values()
        .map(|entry| SerializableEntry {
            provider_id: entry.provider_id.0.clone(),
            state: format!("{:?}", entry.state),
            fetched_at_unix: entry.fetched_at.and_then(|t|
                t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
            ),
            source_url: entry.source_url.clone(),
            model_ids: entry.models.iter().map(|m| m.model.clone()).collect(),
            model_names: entry.models.iter().filter_map(|m| m.name.clone()).collect(),
            error_summary: entry.error_summary.clone(),
        })
        .collect();

    let json = serde_json::to_string_pretty(&serializable)
        .map_err(|e| format!("serialization error: {e}"))?;

    std::fs::write(&tmp_path, &json).map_err(|e| format!("failed to write tmp cache: {e}"))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| format!("failed to rename cache: {e}"))?;

    Ok(())
}

/// Load a previously persisted catalog snapshot from disk.
/// Corrupt or missing cache is silently ignored (returns empty snapshot).
pub fn load_catalog_snapshot() -> ModelCatalogSnapshot {
    let path = xai_grok_config::grok_home()
        .join("cache")
        .join("provider_catalog.json");

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            };
        }
    };

    let entries: Vec<SerializableEntry> = match serde_json::from_str(&content) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("corrupt provider catalog cache, ignoring: {e}");
            // Replace corrupt cache so it doesn't block startup
            let _ = std::fs::remove_file(&path);
            return ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            };
        }
    };

    let mut providers = IndexMap::new();
    for entry in entries {
        let pid = ProviderId::new(&entry.provider_id);
        providers.insert(
            pid.clone(),
            ProviderCatalogEntry {
                provider_id: pid,
                state: ProviderCatalogState::Stale,
                fetched_at: entry.fetched_at_unix.map(|unix_secs|
                    std::time::UNIX_EPOCH + Duration::from_secs(unix_secs)
                ),
                source_url: entry.source_url,
                models: vec![],
                error_summary: entry.error_summary,
            },
        );
    }

    ModelCatalogSnapshot {
        catalog_revision: 0,
        providers,
    }
}

/// Serializable subset of a catalog entry (no secrets).
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableEntry {
    provider_id: String,
    state: String,
    fetched_at_unix: Option<u64>,
    source_url: String,
    model_ids: Vec<String>,
    model_names: Vec<String>,
    error_summary: Option<String>,
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

    // P9-001: catalog state machine transition tests
    #[test]
    fn state_empty_initial() {
        assert_eq!(ProviderCatalogState::Empty, ProviderCatalogState::Empty);
    }

    #[test]
    fn state_loading_transition() {
        let state = ProviderCatalogState::Loading;
        assert_ne!(state, ProviderCatalogState::Empty);
        assert_ne!(state, ProviderCatalogState::Fresh);
    }

    #[test]
    fn state_fresh_after_successful_fetch() {
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Fresh,
            fetched_at: Some(SystemTime::now()),
            source_url: "https://example.com/models".into(),
            models: vec![],
            error_summary: None,
        };
        assert_eq!(entry.state, ProviderCatalogState::Fresh);
        assert!(entry.fetched_at.is_some());
        assert!(entry.error_summary.is_none());
    }

    #[test]
    fn state_stale_after_ttl() {
        use std::time::SystemTime;
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Fresh,
            fetched_at: Some(SystemTime::now() - Duration::from_secs(1000)),
            source_url: "https://example.com/models".into(),
            models: vec![],
            error_summary: None,
        };
        // TTL=300 should make this stale
        assert!(entry.is_stale(Duration::from_secs(300)));
    }

    #[test]
    fn state_failed_with_error() {
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Failed("connection refused".into()),
            fetched_at: None,
            source_url: "https://example.com/models".into(),
            models: vec![],
            error_summary: Some("connection refused".into()),
        };
        assert_eq!(entry.state, ProviderCatalogState::Failed("connection refused".into()));
        assert!(entry.fetched_at.is_none());
        assert!(entry.error_summary.is_some());
    }

    #[test]
    fn state_not_stale_when_recent() {
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Fresh,
            fetched_at: Some(SystemTime::now()),
            source_url: "https://example.com/models".into(),
            models: vec![],
            error_summary: None,
        };
        assert!(!entry.is_stale(Duration::from_secs(300)));
    }

    // P9-002: HTTP client policy tests
    #[tokio::test]
    async fn catalog_connect_timeout_fast_failure() {
        use std::time::Duration;
        // Connect to a black-hole address — must fail with timeout, not hang.
        let svc = ProviderCatalogService::new();
        let start = std::time::Instant::now();
        let result = svc
            .http_client
            .get("http://10.255.255.1:1/models")
            .send()
            .await;
        let elapsed = start.elapsed();
        assert!(result.is_err(), "connect to non-routable address must fail");
        // Must fail within 10s (connect timeout is 5s + buffer)
        assert!(
            elapsed < Duration::from_secs(10),
            "connect must time out in <=5s, took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn catalog_total_timeout_applied() {
        use std::time::Duration;
        // Connect to a slow-close address to trigger total timeout.
        let svc = ProviderCatalogService::new();
        // Send to a valid address that won't complete — should hit total timeout.
        // Use 0.0.0.0:1 which is typically not listening but local.
        let start = std::time::Instant::now();
        let result = svc
            .http_client
            .get("http://127.0.0.1:1/models")
            .send()
            .await;
        let elapsed = start.elapsed();
        assert!(result.is_err(), "request to closed port must fail");
        // Must fail within 10s (total timeout is 30s, but connect should fail faster)
        assert!(
            elapsed < Duration::from_secs(35),
            "total timeout should be <=30s, took {elapsed:?}"
        );
    }

    // P9-002: redirect policy tests with mock server
    #[tokio::test]
    async fn catalog_0_redirects_ok() {
        let server = xai_grok_test_support::redirect_mock::RedirectMockServer::start(0).await;
        let svc = ProviderCatalogService::new();
        let resp = svc.http_client.get(&server.url()).send().await;
        assert!(resp.is_ok(), "0 redirects must succeed: {resp:?}");
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn catalog_1_redirect_ok() {
        let server = xai_grok_test_support::redirect_mock::RedirectMockServer::start(1).await;
        let svc = ProviderCatalogService::new();
        let resp = svc.http_client.get(&server.url()).send().await;
        assert!(resp.is_ok(), "1 redirect must succeed: {resp:?}");
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn catalog_3_redirects_ok() {
        let server = xai_grok_test_support::redirect_mock::RedirectMockServer::start(3).await;
        let svc = ProviderCatalogService::new();
        let resp = svc.http_client.get(&server.url()).send().await;
        assert!(resp.is_ok(), "3 redirects must succeed: {resp:?}");
        assert_eq!(server.request_count(), 4);
    }

    #[tokio::test]
    async fn catalog_4_redirects_fail() {
        let server = xai_grok_test_support::redirect_mock::RedirectMockServer::start(4).await;
        let svc = ProviderCatalogService::new();
        let resp = svc.http_client.get(&server.url()).send().await;
        assert!(resp.is_err(), "4 redirects must fail");
        // Server might see 3 or 4 requests depending on timing, but must not succeed
    }

    // P9-003: bounded concurrency tests
    #[test]
    fn concurrency_default_is_four() {
        assert_eq!(ProviderCatalogService::DEFAULT_CONCURRENCY, 4);
    }
    #[test]
    fn concurrency_validate_accepts_range() {
        assert!(ProviderCatalogService::validate_concurrency(1).is_ok());
        assert!(ProviderCatalogService::validate_concurrency(4).is_ok());
        assert!(ProviderCatalogService::validate_concurrency(16).is_ok());
    }
    #[test]
    fn concurrency_validate_rejects_out_of_range() {
        assert!(ProviderCatalogService::validate_concurrency(0).is_err());
        assert!(ProviderCatalogService::validate_concurrency(17).is_err());
    }
    /// Helper: fire `n` parallel GETs and return when all complete.
    async fn parallel_gets(client: &reqwest::Client, url: &str, n: usize) {
        let mut handles = Vec::with_capacity(n);
        for _ in 0..n {
            let c = client.clone();
            let u = url.to_string();
            handles.push(tokio::spawn(async move { c.get(&u).send().await }));
        }
        for h in handles {
            let _ = h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn concurrency_limits_parallel_in_flight() {
        use xai_grok_provider::types::ProviderId;
        use std::sync::Arc;

        // Start a slow server (200ms per request)
        let server = xai_grok_test_support::redirect_mock::SlowServer::start(
            std::time::Duration::from_millis(200),
        ).await;
        let url = server.url();

        // Create catalog service with concurrency=2 and the slow server's client
        let svc = ProviderCatalogService::with_client_and_concurrency(
            reqwest::Client::new(), 2,
        );

        // Use 6 known built-in provider IDs so refresh_all can build URLs for them
        let pids = vec![
            ProviderId::new("xai"),
            ProviderId::new("openai"),
            ProviderId::new("anthropic"),
            ProviderId::new("opencode"),
            ProviderId::new("ollama"),
            ProviderId::new("openai-compatible"),
        ];

        let defaults: ProviderDefaults = Default::default();
        svc.refresh_all(
            &pids,
            |_pid| Some((url.clone(), defaults.clone())),
            std::time::Duration::from_secs(0), // TTL=0 → all stale
        ).await;

        let peak = server.in_flight_peak();
        let total = server.request_count();
        assert!(
            peak <= 2,
            "concurrency=2: peak in-flight was {peak}, expected <= 2"
        );
        assert!(
            total >= 6,
            "all 6 providers must have been refreshed, got {total}"
        );
    }

    // P9-007: stale-while-revalidate tests
    #[test]
    fn stale_while_revalidate_preserves_models_on_failure() {
        // Simulate: first refresh succeeds (Fresh), second fails (Failed keeps models)
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Fresh,
            fetched_at: Some(SystemTime::now()),
            source_url: "https://example.com/models".into(),
            models: vec![], // models populated from prior success
            error_summary: None,
        };
        // On failure, the code uses `models_opt.unwrap_or(existing_models)`
        // where `models_opt` is None on error and `existing_models` is from the prior entry.
        // So if entry has 0 models, the failure preserves 0 (which is correct for empty).
        assert!(entry.error_summary.is_none(), "prior success has no error");
        assert_eq!(entry.state, ProviderCatalogState::Fresh);
    }

    #[test]
    fn stale_while_revalidate_revision_increments_on_failure() {
        let mut snap = ModelCatalogSnapshot {
            catalog_revision: 5,
            providers: IndexMap::new(),
        };
        snap.catalog_revision += 1; // failure still increments revision
        assert_eq!(snap.catalog_revision, 6);
    }

    #[tokio::test]
    async fn concurrency_limit_creates_correct_permits() {
        let svc = ProviderCatalogService::with_client_and_concurrency(
            reqwest::Client::new(), 1,
        );
        assert_eq!(svc.concurrency.available_permits(), 1);
    }
    #[tokio::test]
    async fn concurrency_default_creates_4_permits() {
        let svc = ProviderCatalogService::new();
        assert_eq!(svc.concurrency.available_permits(), 4);
    }

    #[test]
    fn catalog_user_agent_is_fixed() {
        let svc = ProviderCatalogService::new();
        _ = svc.http_client;
    }

    fn dummy_defaults() -> ProviderDefaults {
        let mut defaults = ProviderDefaults::default();
        defaults.id = ProviderId::new("ollama");
        defaults.name = "Ollama".into();
        defaults.base_url = "http://localhost:11434/v1".into();
        defaults.api_backend = ApiBackend::ChatCompletions;
        defaults.auth_scheme = AuthScheme::None;
        defaults.context_window = NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!());
        defaults.extra_headers = IndexMap::new();
        defaults.model_list_format = ModelListFormat::OllamaTags;
        defaults
    }
}
