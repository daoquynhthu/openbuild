use std::collections::HashMap;
use std::sync::Arc;

use crate::route::Route;

#[derive(Debug, Clone)]
pub struct Model {
    pub id: String,
    pub provider: String,
    pub route: Arc<Route>,
    pub defaults: Option<ModelDefaults>,
}

#[derive(Debug, Clone)]
pub struct ModelDefaults {
    pub limits: Option<ModelLimits>,
    pub generation: Option<GenerationOptions>,
    pub provider_options: Option<HashMap<String, serde_json::Value>>,
    pub http: Option<HttpOptions>,
}

#[derive(Debug, Clone)]
pub struct ModelLimits {
    pub context: Option<u64>,
    pub output: Option<u32>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct HttpOptions {
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
    pub query: Option<HashMap<String, String>>,
}

impl Model {
    pub fn make(
        id: impl Into<String>,
        provider: impl Into<String>,
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
    use crate::endpoint::{Endpoint, EndpointPart};
    use crate::route::{Route, RouteDefaults, RouteInput};

    fn dummy_route() -> Route {
        Route::make(RouteInput {
            id: "test".into(),
            provider: None,
            protocol: "chat_completions".into(),
            endpoint: Endpoint {
                base_url: None,
                path: EndpointPart::Static("/test".into()),
                query: None,
            },
            auth: None,
            framing: Box::new(crate::framing::SseFraming),
            defaults: Some(RouteDefaults {
                headers: None,
            }),
        })
    }

    #[test]
    fn model_make_sets_fields() {
        let route = Arc::new(dummy_route());
        let model = Model::make("gpt-4o", "openai", route.clone(), None);
        assert_eq!(model.id, "gpt-4o");
        assert_eq!(model.provider, "openai");
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
        let model = Model::make("gpt-4o", "openai", route, Some(defaults));
        assert_eq!(model.id, "gpt-4o");
        assert!(model.defaults.is_some());
    }
}
