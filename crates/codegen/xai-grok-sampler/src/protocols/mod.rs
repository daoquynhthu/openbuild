/// Protocol identifier constants used for dispatching.
pub mod id {
    /// OpenAI Chat Completions (`/v1/chat/completions`).
    pub const CHAT_COMPLETIONS: &str = "chat_completions";
    /// OpenAI Responses API (`/v1/responses`).
    pub const RESPONSES: &str = "responses";
    /// Anthropic Messages API (`/v1/messages`).
    pub const MESSAGES: &str = "messages";
}

/// Map an ApiBackend to the corresponding protocol ID string.
pub fn api_backend_to_protocol_id(backend: &xai_grok_sampling_types::ApiBackend) -> &'static str {
    match backend {
        xai_grok_sampling_types::ApiBackend::ChatCompletions => id::CHAT_COMPLETIONS,
        xai_grok_sampling_types::ApiBackend::Responses => id::RESPONSES,
        xai_grok_sampling_types::ApiBackend::Messages => id::MESSAGES,
    }
}
