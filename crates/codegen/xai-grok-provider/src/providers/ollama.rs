use std::num::NonZeroU64;

use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::framing::SseFraming;
use crate::model::Model;
use crate::provider::{ConfiguredProvider, Provider};
use crate::route::{Route, RouteInput};
use crate::types::{
    ApiBackend, AuthScheme, ModelId, ProviderDefaults, ProviderId,
};

fn ollama_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::OLLAMA),
        name: "Ollama".into(),
        base_url: "http://localhost:11434/v1".into(),
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: AuthScheme::None,
        env_key: vec![],
        context_window: NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(8192),
        supports_backend_search: false,
        supports_reasoning_effort: false,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: false,
        extra_headers: Default::default(),
        model_list_endpoint: Some("http://localhost:11434/api/tags".into()),
        model_list_format: crate::types::ModelListFormat::OllamaTags,
    }
}

#[derive(Debug)]
pub struct OllamaProvider {
    defaults: ProviderDefaults,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            defaults: ollama_defaults(),
        }
    }
}

impl Provider for OllamaProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "Ollama"
    }

    fn defaults(&self) -> &ProviderDefaults {
        &self.defaults
    }

    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
        let base_url = overrides
            .base_url
            .clone()
            .unwrap_or_else(|| self.defaults.base_url.clone());
        // Ollama requires no authentication.
        let route = Route::make(RouteInput {
            id: "ollama-chat".into(),
            provider: Some(self.defaults.id.clone()),
            protocol: "chat_completions".into(),
            endpoint: Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            auth: None,
            framing: Box::new(SseFraming),
            defaults: None,
        });
        let pid = self.defaults.id.clone();
        ConfiguredProvider {
            id: pid,
            route,
            model: Box::new(|id, route| {
                Model::make(
                    ModelId::new(id),
                    ProviderId::new(ProviderId::OLLAMA),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            }),
            configure: move |c| OllamaProvider::new().configure(c),
        }
    }

}
