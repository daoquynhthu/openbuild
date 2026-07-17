use xai_grok_sampling_types::error::SamplingError;

/// Protocol identifier constants used for dispatching.
pub mod id {
    pub const CHAT_COMPLETIONS: &str = "chat_completions";
    pub const RESPONSES: &str = "responses";
    pub const MESSAGES: &str = "messages";
}

/// All known protocol IDs.
pub const ALL_KNOWN: &[&str] = &[id::CHAT_COMPLETIONS, id::RESPONSES, id::MESSAGES];

/// Validate and resolve a protocol ID.
/// Returns `Ok(protocol_id)` if the ID is known.
/// Returns `Err(SamplingError)` if unknown.
pub fn resolve_protocol_id(id: &str) -> Result<&'static str, SamplingError> {
    match id {
        id::CHAT_COMPLETIONS => Ok(id::CHAT_COMPLETIONS),
        id::RESPONSES => Ok(id::RESPONSES),
        id::MESSAGES => Ok(id::MESSAGES),
        _ => Err(SamplingError::InvalidConfiguration("unknown protocol ID")),
    }
}

/// Resolve a protocol ID from an `Option`, falling back to `api_backend`
/// for legacy callers. Returns error when `Some(unknown)`.
pub fn resolve_protocol_id_optional(
    id: Option<&str>,
    backend: &xai_grok_sampling_types::ApiBackend,
) -> Result<&'static str, SamplingError> {
    match id {
        Some(id_str) => resolve_protocol_id(id_str),
        None => Ok(api_backend_to_protocol_id(backend)),
    }
}

/// Map an ApiBackend to the corresponding protocol ID string.
pub fn api_backend_to_protocol_id(backend: &xai_grok_sampling_types::ApiBackend) -> &'static str {
    match backend {
        xai_grok_sampling_types::ApiBackend::ChatCompletions => id::CHAT_COMPLETIONS,
        xai_grok_sampling_types::ApiBackend::Responses => id::RESPONSES,
        xai_grok_sampling_types::ApiBackend::Messages => id::MESSAGES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_protocol_id_known() {
        assert_eq!(
            resolve_protocol_id("chat_completions").unwrap(),
            "chat_completions"
        );
        assert_eq!(resolve_protocol_id("responses").unwrap(), "responses");
        assert_eq!(resolve_protocol_id("messages").unwrap(), "messages");
    }

    #[test]
    fn resolve_protocol_id_unknown_returns_error() {
        let err = resolve_protocol_id("unknown_protocol").unwrap_err();
        assert!(matches!(err, SamplingError::InvalidConfiguration(_)));
    }

    #[test]
    fn resolve_protocol_id_empty_returns_error() {
        let err = resolve_protocol_id("").unwrap_err();
        assert!(matches!(err, SamplingError::InvalidConfiguration(_)));
    }

    #[test]
    fn resolve_protocol_id_optional_some_unknown_returns_error() {
        let err = resolve_protocol_id_optional(
            Some("bogus"),
            &xai_grok_sampling_types::ApiBackend::ChatCompletions,
        )
        .unwrap_err();
        assert!(matches!(err, SamplingError::InvalidConfiguration(_)));
    }

    #[test]
    fn resolve_protocol_id_optional_none_falls_back_to_api_backend() {
        let id =
            resolve_protocol_id_optional(None, &xai_grok_sampling_types::ApiBackend::Responses)
                .unwrap();
        assert_eq!(id, "responses");
    }
}
