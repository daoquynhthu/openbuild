//! P8-011: `PreparedSamplerConfig` — the only sampler input after Phase 8.
//!
//! Production chain: `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`.

use crate::headers::SensitiveHeaderMap;
use crate::model::{GenerationOptions, ModelLimits};
use crate::types::{ModelId, ProviderId, RouteId};

/// Fully-prepared sampler configuration with resolved credentials and headers.
/// This is the ONLY input the sampler accepts after Phase 8.
#[derive(Clone, Debug)]
pub struct PreparedSamplerConfig {
    pub provider_id: ProviderId,
    pub route_id: RouteId,
    pub protocol_id: String,
    pub request_url: url::Url,
    pub headers: SensitiveHeaderMap,
    pub model_id: ModelId,
    pub generation: GenerationOptions,
    pub limits: ModelLimits,
}

/// Errors during request preparation.
#[derive(Debug, thiserror::Error)]
pub enum RequestPreparationError {
    #[error("credential resolution failed: {0}")]
    Credential(String),
    #[error("header conflict: {0}")]
    HeaderConflict(String),
    #[error("invalid header: {0}")]
    InvalidHeader(String),
}

/// Test helper: create a `PreparedSamplerConfig` for direct sampler tests.
pub fn test_prepared_config(
    model_id: &str,
    base_url: &str,
) -> PreparedSamplerConfig {
    PreparedSamplerConfig {
        provider_id: ProviderId::new("test"),
        route_id: crate::types::RouteId::new("test-chat"),
        protocol_id: "chat_completions".to_string(),
        request_url: url::Url::parse(&format!("{base_url}/v1/chat/completions")).unwrap(),
        headers: crate::headers::SensitiveHeaderMap::new(http::HeaderMap::new()),
        model_id: ModelId::new(model_id),
        generation: GenerationOptions::default(),
        limits: ModelLimits::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepared_config_creates_valid_config() {
        let cfg = test_prepared_config("gpt-4o", "https://api.test.com");
        assert_eq!(cfg.model_id.0, "gpt-4o");
        assert_eq!(cfg.protocol_id, "chat_completions");
        assert!(cfg.request_url.as_str().contains("api.test.com"));
    }
}
