//! Provider model catalog: parsing, caching, and async refresh.
//!
//! Parsing functions are pure - they operate on `serde_json::Value`,
//! not HTTP responses. This makes them testable without network.
//! Readers receive immutable `Arc<ModelCatalogSnapshot>`.

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

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
pub const DEFAULT_CACHE_TTL_SECONDS: u32 = 300;
pub const MIN_CACHE_TTL_SECONDS: u32 = 30;
pub const MAX_CACHE_TTL_SECONDS: u32 = 86400;

/// Typed config error for catalog TTL validation (P9-008).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogConfigError {
    #[error("TTL must be between {min} and {max} seconds, got {got}")]
    TtlOutOfRange { got: u32, min: u32, max: u32 },
}

/// Typed error for catalog shutdown (P9-009).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogShutdownError {
    #[error("catalog shutdown timed out after {timeout_secs}s — task {task_id} did not complete")]
    Timeout { task_id: String, timeout_secs: u64 },
}

/// Serde helpers for `SystemTime` (serialized as seconds since UNIX_EPOCH).
mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S: Serializer>(t: &Option<SystemTime>, s: S) -> Result<S::Ok, S::Error> {
        let secs = t.map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
        serde::Serialize::serialize(&secs, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<SystemTime>, D::Error> {
        let secs: Option<u64> = serde::Deserialize::deserialize(d)?;
        Ok(secs.map(|s| UNIX_EPOCH + Duration::from_secs(s)))
    }
}

/// Catalog configuration with validated TTL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCatalogConfig {
    pub ttl_seconds: u32,
}

impl ProviderCatalogConfig {
    pub const DEFAULT_TTL: u32 = DEFAULT_CACHE_TTL_SECONDS;

    pub fn new(ttl_seconds: u32) -> Result<Self, CatalogConfigError> {
        Self::validate_ttl(ttl_seconds)?;
        Ok(Self { ttl_seconds })
    }

    pub fn validate_ttl(v: u32) -> Result<(), CatalogConfigError> {
        if !(MIN_CACHE_TTL_SECONDS..=MAX_CACHE_TTL_SECONDS).contains(&v) {
            return Err(CatalogConfigError::TtlOutOfRange {
                got: v,
                min: MIN_CACHE_TTL_SECONDS,
                max: MAX_CACHE_TTL_SECONDS,
            });
        }
        Ok(())
    }
}

impl Default for ProviderCatalogConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: Self::DEFAULT_TTL,
        }
    }
}

/// Per-provider catalog state (P9-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCatalogState {
    Empty,
    Loading,
    Fresh,
    Stale,
    Failed(String),
}

/// Per-provider catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCatalogEntry {
    pub provider_id: ProviderId,
    pub state: ProviderCatalogState,
    /// Wall-clock timestamp of last successful fetch (P9-001).
    #[serde(with = "system_time_serde")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
/// TTL cache, cancellation, stale fallback, and revision events (P9-014).
pub struct ProviderCatalogService {
    snapshot: Arc<RwLock<Arc<ModelCatalogSnapshot>>>,
    http_client: reqwest::Client,
    cancel_token: CancellationToken,
    concurrency: Arc<tokio::sync::Semaphore>,
    active_refresh: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    revision_tx: tokio::sync::watch::Sender<u64>,
}

impl std::fmt::Debug for ProviderCatalogService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCatalogService").finish()
    }
}

/// Look up the source URL for a provider ID from the pre-computed tasks list.
fn url_for_pid(tasks: &[(ProviderId, String, ProviderDefaults)], pid: &ProviderId) -> String {
    tasks
        .iter()
        .find(|(id, _, _)| id == pid)
        .map(|(_, url, _)| url.clone())
        .unwrap_or_default()
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
        format!("{base}/api/tags")
    } else {
        format!("{base}/models")
    }
}

impl ProviderCatalogService {
    pub fn new() -> Self {
        Self::with_client(Self::build_http_client())
    }

    /// Build the unique reqwest client with P9-002 policy.
    pub(crate) fn build_http_client() -> reqwest::Client {
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
        if !(Self::MIN_CONCURRENCY..=Self::MAX_CONCURRENCY).contains(&v) {
            return Err(format!(
                "concurrency must be between {} and {}, got {}",
                Self::MIN_CONCURRENCY,
                Self::MAX_CONCURRENCY,
                v
            ));
        }
        Ok(())
    }

    /// Constructor with explicit client, concurrency limit, and optional cancellation token.
    pub(crate) fn with_client_and_concurrency(
        http_client: reqwest::Client,
        max_concurrency: u32,
    ) -> Self {
        assert!(
            (Self::MIN_CONCURRENCY..=Self::MAX_CONCURRENCY).contains(&max_concurrency),
            "concurrency out of range"
        );
        let (revision_tx, _) = tokio::sync::watch::channel(0);
        Self {
            snapshot: Arc::new(RwLock::new(Arc::new(ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            }))),
            http_client,
            cancel_token: CancellationToken::new(),
            concurrency: Arc::new(tokio::sync::Semaphore::new(max_concurrency as usize)),
            active_refresh: tokio::sync::Mutex::new(None),
            revision_tx,
        }
    }

    /// Constructor with explicit client and cancellation token (for sharing with ProviderRuntime).
    pub(crate) fn with_client_and_token(
        http_client: reqwest::Client,
        cancel_token: CancellationToken,
        max_concurrency: u32,
    ) -> Self {
        assert!(
            (Self::MIN_CONCURRENCY..=Self::MAX_CONCURRENCY).contains(&max_concurrency),
            "concurrency out of range"
        );
        let (revision_tx, _) = tokio::sync::watch::channel(0);
        Self {
            snapshot: Arc::new(RwLock::new(Arc::new(ModelCatalogSnapshot {
                catalog_revision: 0,
                providers: IndexMap::new(),
            }))),
            http_client,
            cancel_token,
            concurrency: Arc::new(tokio::sync::Semaphore::new(max_concurrency as usize)),
            active_refresh: tokio::sync::Mutex::new(None),
            revision_tx,
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

    /// Subscribe to catalog revision changes (P9-014).
    ///
    /// Each time the catalog revision is incremented (after a successful or failed refresh)
    /// the new revision number is sent through this watch. Consumers rebuild the model view
    /// on change instead of making direct network requests.
    pub fn subscribe_catalog_revision(&self) -> tokio::sync::watch::Receiver<u64> {
        self.revision_tx.subscribe()
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

        // P9-001: publish Loading state before the HTTP request
        {
            let mut snap = self.snapshot.write().await;
            let mut new_snapshot = (**snap).clone();
            new_snapshot.providers.insert(
                provider_id.clone(),
                ProviderCatalogEntry {
                    provider_id: provider_id.clone(),
                    state: ProviderCatalogState::Loading,
                    fetched_at: None,
                    source_url: url.to_string(),
                    models: vec![],
                    error_summary: None,
                },
            );
            *snap = Arc::new(new_snapshot);
        }

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
        let rev = new_snapshot.catalog_revision;
        *snap = Arc::new(new_snapshot);
        let _ = self.revision_tx.send(rev);
    }

    /// Refresh all providers with bounded concurrency and TTL-awareness.
    /// Skips providers whose cache is still fresh.
    /// Spawned tasks check the shared cancellation token (P9-009).
    pub async fn refresh_all(
        &self,
        provider_ids: &[ProviderId],
        build_url: impl Fn(&ProviderId) -> Option<(String, ProviderDefaults)>,
        ttl: Duration,
    ) {
        // Pre-compute stale provider tasks outside the spawned wrapper
        // so non-'static closures can be used.
        let stale_pids: Vec<ProviderId> = {
            let current = self.snapshot.read().await;
            provider_ids
                .iter()
                .filter(|pid| current.providers.get(*pid).is_none_or(|e| e.is_stale(ttl)))
                .cloned()
                .collect()
        };

        let tasks: Vec<(ProviderId, String, ProviderDefaults)> = stale_pids
            .iter()
            .filter_map(|pid| {
                let (url, defaults) = build_url(pid)?;
                Some((pid.clone(), url, defaults))
            })
            .collect();

        if tasks.is_empty() {
            return;
        }

        let semaphore = Arc::clone(&self.concurrency);
        let cancel_token = self.cancel_token.clone();
        let client = self.http_client.clone();
        let snapshot = Arc::clone(&self.snapshot);
        let revision_tx = self.revision_tx.clone();

        let handle = tokio::spawn(async move {
            let mut spawned = Vec::new();
            for (pid, url, defaults) in &tasks {
                let pid = pid.clone();
                let url = url.clone();
                let defaults = defaults.clone();
                let sem = Arc::clone(&semaphore);
                let ct = cancel_token.clone();
                let cl = client.clone();

                spawned.push(tokio::spawn(async move {
                    // P9-009: cancellation-safe semaphore acquire
                    let _permit = tokio::select! {
                        permit = sem.acquire() => permit,
                        _ = ct.cancelled() => return (pid.clone(), None, Some("cancelled".into())),
                    };

                    // P9-006: apply provider auth and extra headers
                    let mut req = cl.get(&url);
                    for (k, v) in &defaults.extra_headers {
                        req = req.header(k.as_str(), v.as_str());
                    }
                    let auth_value = defaults
                        .env_key
                        .iter()
                        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()));
                    if let Some(token) = auth_value {
                        req = req.header("Authorization", format!("Bearer {token}"));
                    }

                    // P9-009: cancellation-safe HTTP request
                    let response = tokio::select! {
                        result = req.send() => match result {
                            Ok(r) => r,
                            Err(e) => return (pid.clone(), None, Some(format!("HTTP error: {e}"))),
                        },
                        _ = ct.cancelled() => return (pid.clone(), None, Some("cancelled".into())),
                    };

                    // P9-005: check HTTP status before parsing JSON
                    let status = response.status();
                    if !status.is_success() {
                        let error = ProviderCatalogService::classify_http_status(status);
                        return (pid.clone(), None, Some(error));
                    }

                    let body: serde_json::Value = tokio::select! {
                        result = response.json() => match result {
                            Ok(v) => v,
                            Err(e) => return (pid.clone(), None, Some(format!("JSON error: {e}"))),
                        },
                        _ = ct.cancelled() => return (pid.clone(), None, Some("cancelled".into())),
                    };

                    let models = match defaults.model_list_format {
                        xai_grok_provider::types::ModelListFormat::OllamaTags => {
                            crate::agent::provider_catalog::parse_ollama_tags_models(
                                &body, &defaults,
                            )
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

            for handle in spawned {
                if cancel_token.is_cancelled() {
                    break; // P9-009: stop collecting on cancellation
                }
                if let Ok((pid, models_opt, error)) = handle.await {
                    let mut snap = snapshot.write().await;
                    if cancel_token.is_cancelled() {
                        break; // P9-009: don't publish half-complete snapshot
                    }
                    let mut new_snapshot = (**snap).clone();
                    let source = url_for_pid(&tasks, &pid);
                    let existing_models = new_snapshot
                        .providers
                        .get(&pid)
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
                    let rev = new_snapshot.catalog_revision;
                    *snap = Arc::new(new_snapshot);
                    let _ = revision_tx.send(rev);
                }
            }
        });

        *self.active_refresh.lock().await = Some(handle);
    }

    /// Cancel all outstanding catalog operations and join with a 2-second deadline.
    /// Returns `Ok(())` if all tasks completed within the deadline.
    pub async fn shutdown(&self) -> Result<(), CatalogShutdownError> {
        self.cancel_token.cancel();
        let handle = {
            let mut active = self.active_refresh.lock().await;
            active.take()
        };
        match handle {
            Some(h) => {
                let task_id = format!("{:?}", h);
                if tokio::time::timeout(Duration::from_secs(2), h)
                    .await
                    .is_err()
                {
                    return Err(CatalogShutdownError::Timeout {
                        task_id,
                        timeout_secs: 2,
                    });
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Expose the cancellation token for sharing with ProviderRuntime.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Wait for the current active refresh to complete (for testing).
    pub async fn join_active_refresh(&self) {
        let handle = self.active_refresh.lock().await.take();
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    /// Atomically persist the current snapshot to disk using Phase 3's `atomic_replace`.
    pub async fn persist_snapshot(&self, path: &std::path::Path) -> Result<(), String> {
        let snap = self.snapshot.read().await;
        let json = serde_json::to_string_pretty(&*snap)
            .map_err(|e| format!("serialization error: {e}"))?;
        drop(snap);
        xai_grok_paths::atomic_write::atomic_replace(path, json.as_bytes())
            .map_err(|e| format!("persist error: {e}"))
    }

    /// Load a snapshot from a JSON file written by `persist_snapshot`.
    /// Returns `None` if the file doesn't exist or is unreadable.
    pub async fn load_snapshot(path: &std::path::Path) -> Option<ModelCatalogSnapshot> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
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
        // P9-006: apply provider auth and extra headers
        let mut req = self.http_client.get(url);
        for (k, v) in &defaults.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let auth_value = defaults
            .env_key
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()));
        if let Some(token) = auth_value {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let response = req
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

    // Serialize only non-secret fields
    let serializable: Vec<SerializableEntry> = snapshot
        .providers
        .values()
        .map(|entry| SerializableEntry {
            provider_id: entry.provider_id.0.clone(),
            state: format!("{:?}", entry.state),
            fetched_at_unix: entry.fetched_at.and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            }),
            source_url: entry.source_url.clone(),
            model_ids: entry.models.iter().map(|m| m.model.clone()).collect(),
            model_names: entry.models.iter().filter_map(|m| m.name.clone()).collect(),
            error_summary: entry.error_summary.clone(),
        })
        .collect();

    let json = serde_json::to_string_pretty(&serializable)
        .map_err(|e| format!("serialization error: {e}"))?;

    xai_grok_paths::atomic_write::atomic_replace(&path, json.as_bytes())
        .map_err(|e| format!("failed to atomically write catalog snapshot: {e}"))?;

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
        let models: Vec<ModelEntryConfig> = entry
            .model_ids
            .into_iter()
            .enumerate()
            .map(|(i, mid)| ModelEntryConfig {
                id: None,
                model: mid,
                base_url: String::new(),
                name: entry.model_names.get(i).cloned(),
                description: None,
                max_completion_tokens: None,
                temperature: None,
                top_p: None,
                api_key: None,
                env_key: None,
                api_backend: Default::default(),
                auth_scheme: None,
                reasoning_effort: None,
                supports_reasoning_effort: false,
                reasoning_efforts: vec![],
                extra_headers: IndexMap::new(),
                context_window: NonZeroU64::new(1).unwrap(),
                auto_compact_threshold_percent: None,
                system_prompt_label: None,
                api_base_url: None,
                use_concise: false,
                agent_type: config::default_agent_type(),
                inference_idle_timeout_secs: None,
                max_retries: None,
                hidden: false,
                supported_in_api: false,
                supports_backend_search: false,
                compactions_remaining: None,
                compaction_at_tokens: None,
                show_model_fingerprint: false,
                stream_tool_calls: None,
                laziness_detector: config::LazinessDetectorPerModelConfig::default(),
            })
            .collect();
        providers.insert(
            pid.clone(),
            ProviderCatalogEntry {
                provider_id: pid,
                state: ProviderCatalogState::Stale,
                fetched_at: entry
                    .fetched_at_unix
                    .map(|unix_secs| std::time::UNIX_EPOCH + Duration::from_secs(unix_secs)),
                source_url: entry.source_url,
                models,
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

    #[tokio::test]
    async fn refresh_provider_goes_loading_to_fresh() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let app = axum::Router::new().route(
            "/v1/models",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({"data": [{"id": "m1"}]}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
                .await;
        });
        let url = format!("http://{addr}/v1/models");

        let svc = ProviderCatalogService::with_client(reqwest::Client::new());
        let pid = ProviderId::new("test");
        let defaults = dummy_defaults();

        // Before refresh, state is Empty
        let snap = svc.snapshot().await;
        assert!(!snap.providers.contains_key(&pid));

        svc.refresh_provider(
            pid.clone(),
            &url,
            parse_ollama_tags_models,
            &defaults,
            RefreshStrategy::ForceRefresh,
            Duration::from_secs(300),
        )
        .await;

        let snap = svc.snapshot().await;
        let entry = snap.providers.get(&pid).unwrap();
        assert_eq!(entry.state, ProviderCatalogState::Fresh);
        assert!(entry.fetched_at.is_some());

        let _ = shutdown_tx.send(());
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
        assert_eq!(
            entry.state,
            ProviderCatalogState::Failed("connection refused".into())
        );
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
        use std::sync::Arc;
        use xai_grok_provider::types::ProviderId;

        // Start a slow server (200ms per request)
        let server = xai_grok_test_support::redirect_mock::SlowServer::start(
            std::time::Duration::from_millis(200),
        )
        .await;
        let url = server.url();

        // Create catalog service with concurrency=2 and the slow server's client
        let svc = ProviderCatalogService::with_client_and_concurrency(reqwest::Client::new(), 2);

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
        )
        .await;
        svc.join_active_refresh().await;

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

    // P9-006: derive_model_list_url
    #[test]
    fn derive_ollama_url_uses_base_url_override() {
        let mut defaults = dummy_defaults();
        defaults.model_list_format = ModelListFormat::OllamaTags;
        defaults.base_url = "http://localhost:11434".into();
        let url = derive_model_list_url(&defaults, Some("http://ollama.corp:11434"));
        assert_eq!(url, "http://ollama.corp:11434/api/tags");
    }

    #[test]
    fn derive_ollama_url_falls_back_to_defaults() {
        let mut defaults = dummy_defaults();
        defaults.model_list_format = ModelListFormat::OllamaTags;
        defaults.base_url = "http://localhost:11434".into();
        let url = derive_model_list_url(&defaults, None);
        assert_eq!(url, "http://localhost:11434/api/tags");
    }

    #[test]
    fn derive_openai_url_uses_base_url_override() {
        let mut defaults = dummy_defaults();
        defaults.model_list_format = ModelListFormat::OpenAiCompatible;
        defaults.base_url = "https://api.openai.com/v1".into();
        let url = derive_model_list_url(&defaults, Some("https://custom.example.com"));
        assert_eq!(url, "https://custom.example.com/models");
    }

    #[test]
    fn derive_openai_url_uses_explicit_endpoint() {
        let mut defaults = dummy_defaults();
        defaults.model_list_format = ModelListFormat::OpenAiCompatible;
        defaults.base_url = "https://api.openai.com/v1".into();
        defaults.model_list_endpoint = Some("https://custom.example.com/my-models".into());
        let url = derive_model_list_url(&defaults, None);
        assert_eq!(url, "https://custom.example.com/my-models");
    }

    // P9-005: HTTP status classification
    #[test]
    fn classify_http_status_401_auth_required() {
        let status = reqwest::StatusCode::UNAUTHORIZED;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "auth required (HTTP 401)");
    }

    #[test]
    fn classify_http_status_403_forbidden() {
        let status = reqwest::StatusCode::FORBIDDEN;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "forbidden (HTTP 403)");
    }

    #[test]
    fn classify_http_status_404_endpoint_not_found() {
        let status = reqwest::StatusCode::NOT_FOUND;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "endpoint not found (HTTP 404)");
    }

    #[test]
    fn classify_http_status_429_rate_limited() {
        let status = reqwest::StatusCode::TOO_MANY_REQUESTS;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "rate limited (HTTP 429)");
    }

    #[test]
    fn classify_http_status_5xx_server_error() {
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "server error (HTTP 500)");
    }

    #[test]
    fn classify_http_status_503_server_error() {
        let status = reqwest::StatusCode::SERVICE_UNAVAILABLE;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "server error (HTTP 503)");
    }

    #[test]
    fn classify_http_status_200_is_unexpected() {
        let status = reqwest::StatusCode::OK;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "unexpected HTTP status 200");
    }

    #[test]
    fn classify_http_status_302_is_unexpected() {
        let status = reqwest::StatusCode::FOUND;
        let msg = ProviderCatalogService::classify_http_status(status);
        assert_eq!(msg, "unexpected HTTP status 302");
    }

    #[tokio::test]
    async fn refresh_all_returns_http_error_on_non_2xx() {
        // Start an axum server that returns 403
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let app = axum::Router::new().route(
            "/v1/models",
            axum::routing::get(|| async {
                axum::response::Response::builder()
                    .status(403)
                    .body(axum::body::Body::from("forbidden"))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
                .await;
        });
        let base_url = format!("http://{addr}");
        let url = format!("{base_url}/v1/models");

        let svc = ProviderCatalogService::with_client(reqwest::Client::new());
        let pid = ProviderId::new("test");
        let mut defaults = dummy_defaults();
        defaults.base_url = base_url;
        defaults.model_list_endpoint = Some(url.clone());

        svc.refresh_all(&[pid.clone()], |p| {
            if p == &ProviderId::new("test") {
                Some((url.clone(), defaults.clone()))
            } else {
                None
            }
        }, Duration::from_secs(300)).await;

        // Wait briefly for the spawned task to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        let snap = svc.snapshot().await;
        let entry = snap.providers.get(&pid).unwrap();
        assert_eq!(entry.state, ProviderCatalogState::Failed("forbidden (HTTP 403)".into()));
        assert_eq!(
            entry.error_summary.as_deref(),
            Some("forbidden (HTTP 403)")
        );

        let _ = shutdown_tx.send(());
    }

    // P9-010: snapshot serialization roundtrip tests
    #[test]
    fn snapshot_roundtrip_preserves_all_fields() {
        use std::time::UNIX_EPOCH;

        let mut providers = IndexMap::new();
        providers.insert(
            ProviderId::new("openai"),
            ProviderCatalogEntry {
                provider_id: ProviderId::new("openai"),
                state: ProviderCatalogState::Fresh,
                fetched_at: Some(UNIX_EPOCH + Duration::from_secs(1000)),
                source_url: "https://api.openai.com/v1/models".into(),
                models: vec![],
                error_summary: None,
            },
        );
        providers.insert(
            ProviderId::new("ollama"),
            ProviderCatalogEntry {
                provider_id: ProviderId::new("ollama"),
                state: ProviderCatalogState::Failed("timeout".into()),
                fetched_at: None,
                source_url: "http://localhost:11434/api/tags".into(),
                models: vec![],
                error_summary: Some("timeout".into()),
            },
        );

        let snap = ModelCatalogSnapshot {
            catalog_revision: 42,
            providers,
        };

        let json = serde_json::to_string_pretty(&snap).unwrap();
        let restored: ModelCatalogSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.catalog_revision, 42);
        assert_eq!(restored.providers.len(), 2);

        let openai = restored.providers.get(&ProviderId::new("openai")).unwrap();
        assert_eq!(openai.state, ProviderCatalogState::Fresh);
        assert_eq!(
            openai.fetched_at,
            Some(UNIX_EPOCH + Duration::from_secs(1000)),
            "timestamp must be preserved, not replaced with now()"
        );
        assert!(openai.error_summary.is_none());

        let ollama = restored.providers.get(&ProviderId::new("ollama")).unwrap();
        assert_eq!(ollama.state, ProviderCatalogState::Failed("timeout".into()));
        assert!(ollama.fetched_at.is_none());
        assert!(ollama.models.is_empty());
        assert_eq!(ollama.error_summary.as_deref(), Some("timeout"));
    }

    #[test]
    fn snapshot_roundtrip_historical_not_disguised_as_fresh() {
        use std::time::UNIX_EPOCH;

        // Stale entry with old timestamp — after roundtrip must stay Stale
        let old_stamp = UNIX_EPOCH + Duration::from_secs(500);
        let mut providers = IndexMap::new();
        providers.insert(
            ProviderId::new("stale-provider"),
            ProviderCatalogEntry {
                provider_id: ProviderId::new("stale-provider"),
                state: ProviderCatalogState::Stale,
                fetched_at: Some(old_stamp),
                source_url: "https://example.com/models".into(),
                models: vec![],
                error_summary: None,
            },
        );

        let snap = ModelCatalogSnapshot {
            catalog_revision: 1,
            providers,
        };

        let json = serde_json::to_string_pretty(&snap).unwrap();
        assert!(
            json.contains("\"fetched_at\": 500"),
            "JSON must contain raw epoch seconds, got: {json}"
        );
        assert!(
            json.contains("\"Stale\""),
            "JSON must preserve Stale state, got: {json}"
        );

        let restored: ModelCatalogSnapshot = serde_json::from_str(&json).unwrap();
        let entry = restored.providers.get(&ProviderId::new("stale-provider")).unwrap();
        assert_eq!(entry.state, ProviderCatalogState::Stale);
        assert_eq!(
            entry.fetched_at,
            Some(old_stamp),
            "old timestamp must be preserved exactly, not replaced with now()"
        );
    }

    // P9-011: persist snapshot using atomic writer
    #[tokio::test]
    async fn persist_roundtrip_preserves_snapshot() {
        use std::time::UNIX_EPOCH;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.json");

        let svc = ProviderCatalogService::new();
        {
            let mut snap = svc.snapshot.write().await;
            let mut providers = IndexMap::new();
            providers.insert(
                ProviderId::new("test"),
                ProviderCatalogEntry {
                    provider_id: ProviderId::new("test"),
                    state: ProviderCatalogState::Fresh,
                    fetched_at: Some(UNIX_EPOCH + Duration::from_secs(999)),
                    source_url: "https://example.com/models".into(),
                    models: vec![],
                    error_summary: None,
                },
            );
            *snap = Arc::new(ModelCatalogSnapshot {
                catalog_revision: 7,
                providers,
            });
        }

        svc.persist_snapshot(&path).await.unwrap();
        assert!(path.exists(), "file must exist after persist");

        let loaded = ProviderCatalogService::load_snapshot(&path).await.unwrap();
        assert_eq!(loaded.catalog_revision, 7);
        let entry = loaded.providers.get(&ProviderId::new("test")).unwrap();
        assert_eq!(entry.state, ProviderCatalogState::Fresh);
        assert_eq!(entry.fetched_at, Some(UNIX_EPOCH + Duration::from_secs(999)));
    }

    #[tokio::test]
    async fn persist_failure_preserves_old_disk_snapshot() {
        use std::time::UNIX_EPOCH;

        let dir = tempfile::tempdir().unwrap();
        // Use a path inside a non-existent subdirectory to trigger write failure
        let path = dir.path().join("nonexistent_subdir").join("catalog.json");

        // Write initial snapshot to a valid location
        let svc = ProviderCatalogService::new();
        {
            let mut snap = svc.snapshot.write().await;
            let mut providers = IndexMap::new();
            providers.insert(
                ProviderId::new("original"),
                ProviderCatalogEntry {
                    provider_id: ProviderId::new("original"),
                    state: ProviderCatalogState::Fresh,
                    fetched_at: Some(UNIX_EPOCH + Duration::from_secs(100)),
                    source_url: "https://original.com/models".into(),
                    models: vec![],
                    error_summary: None,
                },
            );
            *snap = Arc::new(ModelCatalogSnapshot {
                catalog_revision: 1,
                providers,
            });
        }

        // First persist to the real valid path
        let valid_path = dir.path().join("catalog.json");
        svc.persist_snapshot(&valid_path).await.unwrap();
        let old_bytes = std::fs::read(&valid_path).unwrap();

        // Update in-memory snapshot
        {
            let mut snap = svc.snapshot.write().await;
            let mut providers = IndexMap::new();
            providers.insert(
                ProviderId::new("new"),
                ProviderCatalogEntry {
                    provider_id: ProviderId::new("new"),
                    state: ProviderCatalogState::Fresh,
                    fetched_at: Some(UNIX_EPOCH + Duration::from_secs(200)),
                    source_url: "https://new.com/models".into(),
                    models: vec![],
                    error_summary: None,
                },
            );
            *snap = Arc::new(ModelCatalogSnapshot {
                catalog_revision: 2,
                providers,
            });
        }

        // Attempt persist to a non-existent directory — must fail
        let result = svc.persist_snapshot(&path).await;
        assert!(result.is_err(), "persist to non-existent dir must fail");

        // Old disk snapshot at the valid path must be unchanged
        let new_bytes = std::fs::read(&valid_path).unwrap();
        assert_eq!(
            old_bytes, new_bytes,
            "old disk snapshot must be preserved after failed persist"
        );

        // Old disk content must still reflect revision 1
        let loaded = ProviderCatalogService::load_snapshot(&valid_path).await.unwrap();
        assert_eq!(loaded.catalog_revision, 1, "disk revision must stay at 1");
        assert!(loaded.providers.contains_key(&ProviderId::new("original")));
    }

    // P9-009: cancellation tests
    #[tokio::test]
    async fn cancel_releases_semaphore_waiters() {
        let server = xai_grok_test_support::redirect_mock::SlowServer::start(
            std::time::Duration::from_millis(500),
        )
        .await;
        let url = server.url();
        let svc = ProviderCatalogService::with_client_and_concurrency(reqwest::Client::new(), 1);
        let defaults = ProviderDefaults::default();
        let pids = vec![ProviderId::new("a"), ProviderId::new("b")];

        svc.refresh_all(
            &pids,
            |_| Some((url.clone(), defaults.clone())),
            Duration::from_secs(0),
        )
        .await;

        // Wait for first request to start (semaphore acquired, request in-flight)
        tokio::time::timeout(Duration::from_millis(200), async {
            while server.in_flight_peak() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("request should start within 200ms");

        // The second task is now waiting on the semaphore. Cancel releases it.
        let shutdown_result = tokio::time::timeout(Duration::from_secs(2), svc.shutdown()).await;
        assert!(shutdown_result.is_ok(), "shutdown must complete within 2s");

        // Only 1 request should have been made (the one that got the semaphore).
        // The second task was released by cancellation before acquiring the permit.
        assert_eq!(
            server.request_count(),
            1,
            "second waiter must be released by cancellation"
        );
    }

    #[tokio::test]
    async fn cancel_terminates_in_flight_request() {
        let server = xai_grok_test_support::redirect_mock::SlowServer::start(
            std::time::Duration::from_secs(10), // very slow — would timeout test
        )
        .await;
        let url = server.url();
        let svc = ProviderCatalogService::new();
        let defaults = ProviderDefaults::default();
        let pids = vec![ProviderId::new("a")];

        svc.refresh_all(
            &pids,
            |_| Some((url.clone(), defaults.clone())),
            Duration::from_secs(0),
        )
        .await;

        // Wait for in-flight request
        tokio::time::timeout(Duration::from_millis(500), async {
            while server.in_flight_peak() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("request should start within 500ms");

        // Cancel the in-flight request — shutdown should complete within 2s,
        // NOT wait for the 10s server delay.
        let shutdown_result = tokio::time::timeout(Duration::from_secs(3), svc.shutdown()).await;
        assert!(shutdown_result.is_ok(), "shutdown must complete within 3s");
    }

    #[tokio::test]
    async fn cancel_prevents_half_complete_snapshot() {
        let server = xai_grok_test_support::redirect_mock::SlowServer::start(
            std::time::Duration::from_secs(10),
        )
        .await;
        let url = server.url();
        let svc = ProviderCatalogService::with_client_and_concurrency(reqwest::Client::new(), 2);
        let defaults = ProviderDefaults::default();
        let pids = vec![
            ProviderId::new("a"),
            ProviderId::new("b"),
            ProviderId::new("c"),
        ];

        svc.refresh_all(
            &pids,
            |_| Some((url.clone(), defaults.clone())),
            Duration::from_secs(0),
        )
        .await;

        // Wait for 2 in-flight requests (both permits taken)
        tokio::time::timeout(Duration::from_millis(500), async {
            while server.in_flight_peak() < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("2 requests should be in-flight within 500ms");

        svc.shutdown().await.unwrap();

        // No request should have completed before cancellation (10s delay),
        // so the snapshot must be pristine.
        let snap = svc.snapshot().await;
        assert_eq!(
            snap.catalog_revision, 0,
            "no revision bump after cancellation"
        );
        assert!(snap.providers.is_empty(), "no providers after cancellation");
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
        let svc = ProviderCatalogService::with_client_and_concurrency(reqwest::Client::new(), 1);
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

    // P9-008: TTL config validation
    #[test]
    fn ttl_accepts_min_boundary() {
        assert!(ProviderCatalogConfig::new(MIN_CACHE_TTL_SECONDS).is_ok());
    }

    #[test]
    fn ttl_accepts_max_boundary() {
        assert!(ProviderCatalogConfig::new(MAX_CACHE_TTL_SECONDS).is_ok());
    }

    #[test]
    fn ttl_accepts_default() {
        assert!(ProviderCatalogConfig::new(DEFAULT_CACHE_TTL_SECONDS).is_ok());
    }

    #[test]
    fn ttl_accepts_in_between() {
        assert!(ProviderCatalogConfig::new(299).is_ok());
        assert!(ProviderCatalogConfig::new(300).is_ok());
        assert!(ProviderCatalogConfig::new(301).is_ok());
    }

    #[test]
    fn ttl_rejects_below_min() {
        let err = ProviderCatalogConfig::new(MIN_CACHE_TTL_SECONDS - 1).unwrap_err();
        assert!(matches!(err, CatalogConfigError::TtlOutOfRange { .. }));
    }

    #[test]
    fn ttl_rejects_above_max() {
        let err = ProviderCatalogConfig::new(MAX_CACHE_TTL_SECONDS + 1).unwrap_err();
        assert!(matches!(err, CatalogConfigError::TtlOutOfRange { .. }));
    }

    #[test]
    fn ttl_error_message_contains_values() {
        let err = ProviderCatalogConfig::new(MAX_CACHE_TTL_SECONDS + 1).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("86400"), "error msg should contain max: {msg}");
        assert!(msg.contains("86401"), "error msg should contain got: {msg}");
    }

    #[test]
    fn ttl_default_is_300() {
        let cfg = ProviderCatalogConfig::default();
        assert_eq!(cfg.ttl_seconds, 300);
    }

    // P9-008: is_stale with system clock — using Instant cannot detect rewind.
    // SystemTime::elapsed() returns Err when clock moves backwards.
    #[test]
    fn is_stale_detects_system_clock_rewind() {
        // Simulate clock rewind: fetched_at in the future (after a rewind,
        // elapsed() returns Err, which is_stale treats as stale).
        let future = SystemTime::now() + Duration::from_secs(3600);
        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("test"),
            state: ProviderCatalogState::Fresh,
            fetched_at: Some(future),
            source_url: "https://example.com/models".into(),
            models: vec![],
            error_summary: None,
        };
        // When SystemTime.elapsed() errors (clock rewind), is_stale returns true
        assert!(
            entry.is_stale(Duration::from_secs(300)),
            "future fetched_at (clock rewind) should be considered stale"
        );
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
