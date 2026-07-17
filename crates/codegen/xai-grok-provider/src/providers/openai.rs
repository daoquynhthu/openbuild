use std::num::NonZeroU64;
use std::sync::Arc;

use crate::auth::AuthPolicy;
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::model::Model;
use crate::provider::{ConfiguredProvider, Provider};
use crate::route::Route;
use crate::types::{ApiBackend, AuthScheme, ModelId, ProviderDefaults, ProviderId};

pub fn openai_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::OPENAI),
        name: "OpenAI".into(),
        base_url: "https://api.openai.com/v1".into(),
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: AuthScheme::Bearer,
        env_key: vec!["OPENAI_API_KEY".into()],
        context_window: NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(8192),
        supports_backend_search: false,
        supports_reasoning_effort: true,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: true,
        extra_headers: Default::default(),
        model_list_endpoint: None,
        model_list_format: crate::types::ModelListFormat::OpenAiCompatible,
    }
}

#[derive(Debug)]
pub struct OpenAIProvider {
    defaults: ProviderDefaults,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            defaults: openai_defaults(),
        }
    }
}

impl Provider for OpenAIProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "OpenAI"
    }

    fn defaults(&self) -> &ProviderDefaults {
        &self.defaults
    }

    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
        let base_url = overrides
            .base_url
            .clone()
            .unwrap_or_else(|| self.defaults.base_url.clone());
        let route_chat = Arc::new(Route::make(
            "openai-chat",
            Some(self.defaults.id.clone()),
            "chat_completions",
            Endpoint {
                base_url: Some(base_url.clone()),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            AuthPolicy::None,
        ));
        let route_responses = Arc::new(Route::make(
            "openai-responses",
            Some(self.defaults.id.clone()),
            "responses",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/responses".into()),
                query: None,
            },
            AuthPolicy::None,
        ));

        let pid = self.defaults.id.clone();
        ConfiguredProvider {
            id: pid,
            route: (*route_chat).clone(),
            model: Box::new(move |id, _| {
                let r = if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("gpt-4.1")
                {
                    route_responses.clone()
                } else {
                    route_chat.clone()
                };
                Model::make(
                    ModelId::new(id),
                    ProviderId::new(ProviderId::OPENAI),
                    r,
                    None,
                )
            }),
            configure: Box::new(move |c| OpenAIProvider::new().configure(c)),
        }
    }
}
