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
    StepFinish {
        index: u32,
        reason: FinishReason,
        usage: Option<Usage>,
    },
    Finish {
        reason: FinishReason,
        usage: Option<Usage>,
    },
    Error { message: String, kind: ErrorKind },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_event_variants_create() {
        let events = vec![
            LLMEvent::StepStart { index: 0 },
            LLMEvent::TextStart { id: "t1".into() },
            LLMEvent::TextDelta { id: "t1".into(), text: "hello".into() },
            LLMEvent::TextEnd { id: "t1".into() },
            LLMEvent::ReasoningStart { id: "r1".into() },
            LLMEvent::ReasoningDelta { id: "r1".into(), text: "thinking".into() },
            LLMEvent::ReasoningEnd { id: "r1".into() },
            LLMEvent::ToolInputStart { id: "tc1".into(), name: "get_weather".into() },
            LLMEvent::ToolInputDelta { id: "tc1".into(), text: r#"{"city": "Lo"#.into() },
            LLMEvent::ToolInputEnd { id: "tc1".into(), name: "get_weather".into() },
            LLMEvent::ToolCall { id: "tc1".into(), name: "get_weather".into(), input: serde_json::json!({"city": "London"}) },
            LLMEvent::ToolResult { id: "tc1".into(), name: "get_weather".into(), result: serde_json::json!({"temp": 20}) },
            LLMEvent::ToolError { id: "tc1".into(), name: "get_weather".into(), message: "timeout".into() },
            LLMEvent::StepFinish { index: 0, reason: FinishReason::ToolCalls, usage: None },
            LLMEvent::Finish { reason: FinishReason::Stop, usage: None },
            LLMEvent::Error { message: "test".into(), kind: ErrorKind::Timeout },
        ];
        assert_eq!(events.len(), 16);
    }

    #[test]
    fn usage_default() {
        let usage = Usage {
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            non_cached_input_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            reasoning_tokens: None,
            provider_metadata: None,
        };
        assert!(usage.input_tokens.is_none());
        assert!(usage.total_tokens.is_none());
    }

    #[test]
    fn finish_reason_display() {
        let reasons = [
            FinishReason::Stop,
            FinishReason::Length,
            FinishReason::ToolCalls,
            FinishReason::Error,
        ];
        assert_eq!(reasons.len(), 4);
    }
}
