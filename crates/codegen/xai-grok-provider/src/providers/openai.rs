use std::num::NonZeroU64;

use crate::auth::Credential;
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::framing::SseFraming;
use crate::model::Model;
use crate::provider::{ConfiguredProvider, Provider};
use crate::route::{Route, RouteInput};
use crate::types::{
    ApiBackend, AuthScheme, ModelId, ProviderDefaults, ProviderId, ProviderModelDef,
};

pub fn openai_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::OPENAI),
        name: "OpenAI".into(),
        base_url: "https://api.openai.com/v1".into(),
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: AuthScheme::Bearer,
        env_key: vec!["OPENAI_API_KEY".into()],
        context_window: NonZeroU64::new(128_000).unwrap(),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(8192),
        supports_backend_search: false,
        supports_reasoning_effort: true,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: true,
        extra_headers: Default::default(),
        known_models: vec![
            ProviderModelDef {
                id: "gpt-4o".into(),
                model: "gpt-4o-2024-11-20".into(),
                name: "GPT-4o".into(),
                description: Some("OpenAI's high-intelligence flagship model".into()),
                context_window: NonZeroU64::new(128_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: None,
            },
            ProviderModelDef {
                id: "gpt-4o-mini".into(),
                model: "gpt-4o-mini".into(),
                name: "GPT-4o Mini".into(),
                description: Some("Fast, affordable small model".into()),
                context_window: NonZeroU64::new(128_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: None,
            },
            ProviderModelDef {
                id: "o1".into(),
                model: "o1-2024-12-17".into(),
                name: "o1".into(),
                description: Some("Reasoning model for complex tasks".into()),
                context_window: NonZeroU64::new(200_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: Some(true),
            },
            ProviderModelDef {
                id: "o3-mini".into(),
                model: "o3-mini-2025-01-31".into(),
                name: "o3-mini".into(),
                description: Some("Fast reasoning model".into()),
                context_window: NonZeroU64::new(200_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: Some(true),
            },
            ProviderModelDef {
                id: "gpt-4.1".into(),
                model: "gpt-4.1-2025-04-14".into(),
                name: "GPT-4.1".into(),
                description: Some("Latest full-size GPT-4 model".into()),
                context_window: NonZeroU64::new(1_000_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: None,
            },
            ProviderModelDef {
                id: "gpt-4.1-mini".into(),
                model: "gpt-4.1-mini-2025-04-14".into(),
                name: "GPT-4.1 Mini".into(),
                description: Some("Compact GPT-4.1 model".into()),
                context_window: NonZeroU64::new(1_000_000).unwrap(),
                hidden: false,
                api_backend: None,
                supports_reasoning_effort: None,
            },
        ],
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
        let auth = Credential::optional(overrides.api_key, "api_key")
            .or_else(Credential::config("OPENAI_API_KEY"))
            .bearer();
        let route = Route::make(RouteInput {
            id: "openai-chat".into(),
            provider: Some(self.defaults.id.clone()),
            protocol: "chat_completions".into(),
            endpoint: Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            auth: Some(auth),
            framing: Box::new(SseFraming),
            defaults: None,
        });
        let pid = self.defaults.id.clone();
        ConfiguredProvider {
            id: pid,
            route,
            model: |id, route| {
                Model::make(
                    ModelId::new(id),
                    ProviderId::new(ProviderId::OPENAI),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            },
            configure: move |c| OpenAIProvider::new().configure(c),
        }
    }

    fn known_models(&self) -> &[ProviderModelDef] {
        &self.defaults.known_models
    }
}
