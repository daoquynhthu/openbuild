use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum LLMEvent {
    StepStart { index: u32 },
    TextStart { id: String },
    TextDelta { id: String, text: String },
    TextEnd { id: String },
    ReasoningStart { id: String },
    ReasoningDelta { id: String, text: String },
    ReasoningEnd { id: String },
    ToolInputStart { id: String, name: String },
    ToolInputDelta { id: String, text: String },
    ToolInputEnd { id: String, name: String },
    ToolCall { id: String, name: String, input: serde_json::Value },
    ToolResult { id: String, name: String, result: serde_json::Value },
    ToolError { id: String, name: String, message: String },
    StepFinish { index: u32, reason: FinishReason, usage: Option<Usage> },
    Finish { reason: FinishReason, usage: Option<Usage> },
    Error { message: String, kind: ErrorKind },
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub non_cached_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
    pub cache_write_input_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,
    pub provider_metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
    MaxTokens,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum ErrorKind {
    Authentication,
    RateLimit,
    InvalidRequest,
    ProviderError,
    Timeout,
    Other(String),
}
