use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelDef {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: u64,
    pub hidden: bool,
}

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
