use std::num::NonZeroU64;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProviderId(pub String);

impl ProviderId {
    pub const XAI: &'static str = "xai";
    pub const OPENAI: &'static str = "openai";
    pub const ANTHROPIC: &'static str = "anthropic";
    pub const OPENCODE: &'static str = "opencode";
    pub const OLLAMA: &'static str = "ollama";
    pub const OPENAI_COMPATIBLE: &'static str = "openai-compatible";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Stable model identifier in the ACP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Which API backend protocol to use for inference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiBackend {
    #[default]
    ChatCompletions,
    Responses,
    Messages,
}

/// HTTP auth scheme for API requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthScheme {
    #[default]
    Bearer,
    XApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelDef {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: NonZeroU64,
    pub hidden: bool,
    pub api_backend: Option<ApiBackend>,
    pub supports_reasoning_effort: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ProviderDefaults {
    pub id: ProviderId,
    pub name: String,
    pub base_url: String,
    pub api_backend: ApiBackend,
    pub auth_scheme: AuthScheme,
    pub env_key: Vec<String>,
    pub context_window: NonZeroU64,
    pub max_completion_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub supports_backend_search: bool,
    pub supports_reasoning_effort: bool,
    pub supports_streaming: bool,
    pub supports_tool_calling: bool,
    pub supports_structured_output: bool,
    pub extra_headers: IndexMap<String, String>,
    pub known_models: Vec<ProviderModelDef>,
}

impl Default for ProviderDefaults {
    fn default() -> Self {
        Self {
            id: ProviderId::new("unknown"),
            name: String::new(),
            base_url: String::new(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::Bearer,
            env_key: Vec::new(),
            context_window: NonZeroU64::new(128_000).unwrap(),
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            supports_backend_search: false,
            supports_reasoning_effort: false,
            supports_streaming: true,
            supports_tool_calling: true,
            supports_structured_output: false,
            extra_headers: IndexMap::new(),
            known_models: Vec::new(),
        }
    }
}

/// Shared header map type used across the provider crate.
pub type HeaderMap = std::collections::HashMap<String, String>;

/// A portable, provider-independent request body for an LLM call.
/// Protocol implementations convert this to their native wire format.
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub model: String,
    pub messages: Vec<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Result value from a provider-executed tool call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultValue {
    Text(String),
    Json(serde_json::Value),
    Error(String),
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;

    #[test]
    fn provider_id_constants() {
        assert_eq!(ProviderId::new(ProviderId::XAI).0, "xai");
        assert_eq!(ProviderId::new(ProviderId::OPENAI).0, "openai");
        assert_eq!(ProviderId::new(ProviderId::ANTHROPIC).0, "anthropic");
        assert_eq!(ProviderId::new(ProviderId::OPENCODE).0, "opencode");
        assert_eq!(ProviderId::new(ProviderId::OLLAMA).0, "ollama");
    }

    #[test]
    fn provider_id_equality() {
        assert_eq!(ProviderId::new("xai"), ProviderId::new("xai"));
        assert_ne!(ProviderId::new("xai"), ProviderId::new("openai"));
    }

    #[test]
    fn provider_id_serde_roundtrip() {
        let id = ProviderId::new("test-provider");
        let json = serde_json::to_string(&id).unwrap();
        let back: ProviderId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn model_id_newtype() {
        let id = ModelId::new("gpt-4o");
        assert_eq!(id.0, "gpt-4o");
    }

    #[test]
    fn api_backend_default() {
        assert_eq!(ApiBackend::default(), ApiBackend::ChatCompletions);
    }

    #[test]
    fn api_backend_serde() {
        let json = serde_json::to_string(&ApiBackend::Responses).unwrap();
        assert_eq!(json, "\"responses\"");
        let back: ApiBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ApiBackend::Responses);
    }

    #[test]
    fn auth_scheme_default() {
        assert_eq!(AuthScheme::default(), AuthScheme::Bearer);
    }

    #[test]
    fn provider_defaults_default() {
        let d = ProviderDefaults::default();
        assert_eq!(d.id.0, "unknown");
        assert_eq!(d.context_window.get(), 128_000);
        assert!(d.supports_streaming);
        assert!(d.known_models.is_empty());
        assert_eq!(d.api_backend, ApiBackend::ChatCompletions);
        assert_eq!(d.auth_scheme, AuthScheme::Bearer);
    }

    #[test]
    fn provider_model_def_serde() {
        let def = ProviderModelDef {
            id: "gpt-4o".into(),
            model: "gpt-4o-2024-11-20".into(),
            name: "GPT-4o".into(),
            description: Some("Flagship model".into()),
            context_window: NonZeroU64::new(128_000).unwrap(),
            hidden: false,
            api_backend: None,
            supports_reasoning_effort: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: ProviderModelDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "gpt-4o");
    }
}
