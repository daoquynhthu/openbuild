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
