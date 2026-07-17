//! Provider model catalog: parsing, caching, and async refresh.
//!
//! Parsing functions are pure - they operate on `serde_json::Value`,
//! not HTTP responses. This makes them testable without network.

use indexmap::IndexMap;
use xai_grok_provider::types::ProviderDefaults;

use super::config::{self, ModelEntryConfig};

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
