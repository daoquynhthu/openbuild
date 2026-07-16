use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Type-safe provider identifier.
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

/// Known model definition shipped with a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelDef {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: u64,
    pub hidden: bool,
}

/// Immutable set of defaults baked into each provider definition.
#[derive(Debug, Clone)]
pub struct ProviderDefaults {
    pub id: ProviderId,
    pub name: String,
    pub base_url: String,
    pub api_backend: String,
    pub auth_scheme: String,
    pub env_key: Vec<String>,
    pub context_window: u64,
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
            api_backend: "chat_completions".into(),
            auth_scheme: "bearer".into(),
            env_key: Vec::new(),
            context_window: 128_000,
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

/// A portable, provider-independent LLM request.
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub model: String,
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
    fn provider_defaults_default() {
        let d = ProviderDefaults::default();
        assert_eq!(d.id.0, "unknown");
        assert_eq!(d.context_window, 128_000);
        assert!(d.supports_streaming);
        assert!(d.known_models.is_empty());
    }

    #[test]
    fn provider_model_def_serde() {
        let def = ProviderModelDef {
            id: "gpt-4o".into(),
            model: "gpt-4o-2024-11-20".into(),
            name: "GPT-4o".into(),
            description: Some("Flagship model".into()),
            context_window: 128_000,
            hidden: false,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: ProviderModelDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "gpt-4o");
        assert_eq!(back.model, "gpt-4o-2024-11-20");
    }
}
