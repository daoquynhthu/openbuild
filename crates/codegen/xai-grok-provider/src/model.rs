use std::collections::HashMap;
use std::sync::Arc;

use crate::route::Route;
use crate::types::{ModelId, ProviderId};

/// An executable model value bound to a route.
#[derive(Debug, Clone)]
pub struct Model {
    pub id: ModelId,
    pub provider: ProviderId,
    pub route: Arc<Route>,
    pub defaults: Option<ModelDefaults>,
}

/// Reusable request-behavior defaults attached to a Model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ModelDefaults {
    pub limits: Option<ModelLimits>,
    pub generation: Option<GenerationOptions>,
    pub provider_options: Option<HashMap<String, serde_json::Value>>,
    pub http: Option<HttpOptions>,
}

/// Context and output token limits for a model.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ModelLimits {
    pub context: Option<u64>,
    pub output: Option<u32>,
}

/// Generation parameters sent to the provider.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub struct GenerationOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub seed: Option<u64>,
    pub stop: Option<Vec<String>>,
}

impl GenerationOptions {
    pub fn new(
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
    ) -> Self {
        Self {
            max_tokens,
            temperature,
            top_p,
            top_k: None,
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            stop: None,
        }
    }
}

/// Raw HTTP-level overrides for a request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct HttpOptions {
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
    pub query: Option<HashMap<String, String>>,
}

impl Model {
    pub fn make(
        id: impl Into<ModelId>,
        provider: impl Into<ProviderId>,
        route: Arc<Route>,
        defaults: Option<ModelDefaults>,
    ) -> Self {
        Self {
            id: id.into(),
            provider: provider.into(),
            route,
            defaults,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthPolicy, CredentialCandidate, CredentialSource};
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::route::Route;

    fn dummy_route() -> Route {
        Route::make(
            "test",
            None,
            "chat_completions",
            Endpoint {
                base_url: Some("https://example.com".into()),
                path: EndpointPart::Static("/chat".into()),
                query: None,
            },
            AuthPolicy::bearer(vec![], false),
        )
    }

    #[test]
    fn model_make_sets_fields() {
        let route = Arc::new(dummy_route());
        let model = Model::make(
            ModelId::new("gpt-4o"),
            ProviderId::new("openai"),
            route,
            None,
        );
        assert_eq!(model.id.0, "gpt-4o");
        assert_eq!(model.provider.0, "openai");
        assert!(model.defaults.is_none());
    }

    #[test]
    fn model_with_defaults() {
        let route = Arc::new(dummy_route());
        let defaults = ModelDefaults {
            limits: Some(ModelLimits {
                context: Some(128_000),
                output: Some(4096),
            }),
            generation: Some(GenerationOptions {
                max_tokens: Some(4096),
                temperature: Some(0.7),
                top_p: Some(0.95),
                top_k: None,
                frequency_penalty: None,
                presence_penalty: None,
                seed: None,
                stop: None,
            }),
            provider_options: None,
            http: None,
        };
        let model = Model::make(
            ModelId::new("gpt-4o"),
            ProviderId::new("openai"),
            route,
            Some(defaults),
        );
        assert_eq!(model.id.0, "gpt-4o");
        assert!(model.defaults.is_some());
    }
}
