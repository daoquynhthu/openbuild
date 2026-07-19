use std::num::NonZeroU64;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::ProviderError;

/// Validate that a string is a non-empty, trimmed, non-whitespace-only ID.
/// Rejects empty, whitespace-only, and strings containing '/' (reserved separator).
pub fn validate_id(id: &str, label: &str) -> Result<String, ProviderError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(ProviderError::InvalidProviderId(format!(
            "{label} must not be empty or whitespace-only"
        )));
    }
    if trimmed.contains('/') {
        return Err(ProviderError::InvalidProviderId(format!(
            "{label} must not contain '/'"
        )));
    }
    Ok(trimmed.to_string())
}

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

    pub fn try_new(id: impl Into<String>) -> Result<Self, ProviderError> {
        let s = id.into();
        validate_id(&s, "provider ID")?;
        Ok(Self(s))
    }
}

/// Identifier for a known OpenAI-compatible provider profile (e.g., "deepseek", "groq").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompatibleProfileId(pub String);

impl CompatibleProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for CompatibleProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for CompatibleProfileId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CompatibleProfileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for CompatibleProfileId {
    fn from(s: String) -> Self {
        Self(s)
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

    pub fn try_new(id: impl Into<String>) -> Result<Self, ProviderError> {
        let s = id.into();
        validate_id(&s, "model ID")?;
        Ok(Self(s))
    }
}

/// Route identifier within a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RouteId(pub String);

impl RouteId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn try_new(id: impl Into<String>) -> Result<Self, ProviderError> {
        let s = id.into();
        validate_id(&s, "route ID")?;
        Ok(Self(s))
    }
}

pub use xai_grok_sampling_types::{ApiBackend, AuthScheme};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProviderDefaults {
    pub id: ProviderId,
    pub name: String,
    pub base_url: String,
    pub api_backend: ApiBackend,
    pub auth_scheme: AuthScheme,
    pub env_key: Vec<String>,
    pub model_list_endpoint: Option<String>,
    pub model_list_format: ModelListFormat,
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
            model_list_endpoint: None,
            model_list_format: ModelListFormat::OpenAiCompatible,
            context_window: NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            supports_backend_search: false,
            supports_reasoning_effort: false,
            supports_streaming: true,
            supports_tool_calling: true,
            supports_structured_output: false,
            extra_headers: IndexMap::new(),
        }
    }
}

/// Format of the model list endpoint response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelListFormat {
    #[default]
    OpenAiCompatible,
    /// Ollama /api/tags format: {"models": [{"name": "...", ...}]}
    OllamaTags,
}

/// Shared header map type used across the provider crate.
pub type HeaderMap = std::collections::HashMap<String, String>;

/// Specifies how a provider's model list is sourced.
#[derive(Debug, Clone)]
pub enum ModelSourceSpec {
    /// Models come from a provider-discovered list.
    Dynamic,
    /// Models are statically defined in the provider.
    Static(Vec<String>),
}

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

/// Parse a `--model` value that may be in `provider/model` format.
/// Returns `(Some(provider), model)` when a `/` separator is present,
/// or `(None, model)` for bare model names (backward compatible).
pub fn parse_model_ref(s: &str) -> (Option<ProviderId>, String) {
    if let Some(slash_pos) = s.find('/') {
        let provider = &s[..slash_pos];
        let model = &s[slash_pos + 1..];
        (Some(ProviderId::new(provider)), model.to_string())
    } else {
        (None, s.to_string())
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(d.api_backend, ApiBackend::ChatCompletions);
        assert_eq!(d.auth_scheme, AuthScheme::Bearer);
    }

    #[test]
    fn parse_model_ref_with_provider() {
        let (provider, model) = parse_model_ref("openai/gpt-4o");
        assert_eq!(provider.unwrap().0, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn parse_model_ref_bare() {
        let (provider, model) = parse_model_ref("grok-build");
        assert!(provider.is_none());
        assert_eq!(model, "grok-build");
    }

    #[test]
    fn parse_model_ref_multiple_slashes() {
        let (provider, model) = parse_model_ref("anthropic/claude-sonnet-4-5");
        assert_eq!(provider.unwrap().0, "anthropic");
        assert_eq!(model, "claude-sonnet-4-5");
    }

    #[test]
    fn provider_id_try_new_accepts_valid() {
        let id = ProviderId::try_new("my-provider").unwrap();
        assert_eq!(id.0, "my-provider");
    }

    #[test]
    fn provider_id_try_new_rejects_empty() {
        assert!(ProviderId::try_new("").is_err());
    }

    #[test]
    fn provider_id_try_new_rejects_whitespace() {
        assert!(ProviderId::try_new("   ").is_err());
    }

    #[test]
    fn provider_id_try_new_rejects_slash() {
        assert!(ProviderId::try_new("my/provider").is_err());
    }

    #[test]
    fn model_id_try_new_accepts_valid() {
        let id = ModelId::try_new("gpt-4o").unwrap();
        assert_eq!(id.0, "gpt-4o");
    }

    #[test]
    fn model_id_try_new_rejects_empty() {
        assert!(ModelId::try_new("").is_err());
    }

    #[test]
    fn model_id_try_new_rejects_whitespace() {
        assert!(ModelId::try_new("  ").is_err());
    }

    #[test]
    fn route_id_try_new_accepts_valid() {
        let id = RouteId::try_new("chat-route").unwrap();
        assert_eq!(id.0, "chat-route");
    }

    #[test]
    fn route_id_try_new_rejects_empty() {
        assert!(RouteId::try_new("").is_err());
    }

    #[test]
    fn route_id_try_new_rejects_whitespace() {
        assert!(RouteId::try_new("   ").is_err());
    }

    #[test]
    fn route_id_try_new_rejects_slash() {
        assert!(RouteId::try_new("a/b").is_err());
    }

    #[test]
    fn route_id_serde_roundtrip() {
        let id = RouteId::new("test-route");
        let json = serde_json::to_string(&id).unwrap();
        let back: RouteId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn validate_id_rejects_empty() {
        assert!(validate_id("", "test").is_err());
    }

    #[test]
    fn validate_id_rejects_whitespace() {
        assert!(validate_id(" \t ", "test").is_err());
    }

    #[test]
    fn validate_id_rejects_slash() {
        assert!(validate_id("a/b", "test").is_err());
    }

    #[test]
    fn validate_id_accepts_normal() {
        assert_eq!(validate_id("hello-world", "test").unwrap(), "hello-world");
    }

    #[test]
    fn validate_id_trims() {
        assert_eq!(validate_id("  foo  ", "test").unwrap(), "foo");
    }
}
