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

/// Resolve a protocol ID from an `Option`.
/// Returns error when `None` or when `Some(unknown)`.
pub fn resolve_protocol_id_optional(
    id: Option<&str>,
) -> Result<&'static str, SamplingError> {
    match id {
        Some(id_str) => resolve_protocol_id(id_str),
        None => Err(SamplingError::InvalidConfiguration(
            "protocol_id is required — use PreparedSamplerConfig::from to construct SamplerConfig",
        )),
    }
}

/// Map an ApiBackend to the corresponding protocol ID string.
///
/// This is a legacy utility kept for test convenience.
/// Production code must use explicit `protocol_id` instead.
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
        let err = resolve_protocol_id_optional(Some("bogus")).unwrap_err();
        assert!(matches!(err, SamplingError::InvalidConfiguration(_)));
    }

    #[test]
    fn resolve_protocol_id_optional_none_returns_error() {
        let err = resolve_protocol_id_optional(None).unwrap_err();
        assert!(matches!(err, SamplingError::InvalidConfiguration(_)));
    }

    #[test]
    fn api_backend_to_protocol_id_maps_chat() {
        assert_eq!(
            api_backend_to_protocol_id(&xai_grok_sampling_types::ApiBackend::ChatCompletions),
            "chat_completions"
        );
    }
}
