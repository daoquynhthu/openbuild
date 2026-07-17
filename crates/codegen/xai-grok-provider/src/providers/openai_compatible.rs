use std::num::NonZeroU64;

use crate::auth::Credential;
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::framing::SseFraming;
use crate::model::Model;
use crate::provider::{ConfiguredProvider, Provider};
use crate::route::{Route, RouteInput};
use crate::types::{
    ApiBackend, AuthScheme, ModelId, ProviderDefaults, ProviderId,
};

/// Known OpenAI-compatible profile configurations.
pub(crate) fn profile_base_url(profile: &str) -> Option<&'static str> {
    match profile {
        "groq" => Some("https://api.groq.com/openai/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "togetherai" => Some("https://api.together.xyz/v1"),
        "fireworks" => Some("https://api.fireworks.ai/inference/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "deepinfra" => Some("https://api.deepinfra.com/v1/openai"),
        "cerebras" => Some("https://api.cerebras.ai/v1"),
        "baseten" => Some("https://inference.baseten.co/v1"),
        _ => None,
    }
}

fn compatible_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::OPENAI_COMPATIBLE),
        name: "OpenAI Compatible".into(),
        base_url: String::new(),
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: AuthScheme::Bearer,
        env_key: vec!["XAI_API_KEY".into()],
        context_window: NonZeroU64::new(128_000).unwrap(),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(8192),
        supports_backend_search: false,
        supports_reasoning_effort: false,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: false,
        extra_headers: Default::default(),
        model_list_endpoint: None,
        model_list_format: crate::types::ModelListFormat::OpenAiCompatible,
    }
}

#[derive(Debug)]
pub struct OpenAiCompatibleProvider {
    defaults: ProviderDefaults,
}

impl OpenAiCompatibleProvider {
    pub fn new() -> Self {
        Self {
            defaults: compatible_defaults(),
        }
    }
}

impl Provider for OpenAiCompatibleProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "OpenAI Compatible"
    }

    fn defaults(&self) -> &ProviderDefaults {
        &self.defaults
    }

    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
        let base_url = overrides.base_url.clone().unwrap_or_default();
        let resolved_url = if base_url.is_empty() {
            overrides
                .id
                .as_deref()
                .and_then(profile_base_url)
                .unwrap_or("http://localhost:8080/v1")
                .into()
        } else {
            base_url
        };
        let auth = Credential::optional(overrides.api_key, "api_key")
            .or_else(Credential::config("XAI_API_KEY"))
            .bearer();
        let route = Route::make(RouteInput {
            id: "openai-compatible-chat".into(),
            provider: Some(self.defaults.id.clone()),
            protocol: "chat_completions".into(),
            endpoint: Endpoint {
                base_url: Some(resolved_url),
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
                    ProviderId::new(ProviderId::OPENAI_COMPATIBLE),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            },
            configure: move |c| OpenAiCompatibleProvider::new().configure(c),
        }
    }

}
