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

fn opencode_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::OPENCODE),
        name: "OpenCode Zen".into(),
        base_url: "https://opencode.ai/zen/v1".into(),
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: AuthScheme::Bearer,
        env_key: vec!["OPENCODE_API_KEY".into()],
        context_window: NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(8192),
        supports_backend_search: false,
        supports_reasoning_effort: true,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: false,
        extra_headers: Default::default(),
        model_list_endpoint: None,
        model_list_format: crate::types::ModelListFormat::OpenAiCompatible,
    }
}

#[derive(Debug)]
pub struct OpenCodeProvider {
    defaults: ProviderDefaults,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self {
            defaults: opencode_defaults(),
        }
    }
}

impl Provider for OpenCodeProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "OpenCode Zen"
    }

    fn defaults(&self) -> &ProviderDefaults {
        &self.defaults
    }

    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
        let base_url = overrides
            .base_url
            .clone()
            .unwrap_or_else(|| self.defaults.base_url.clone());
        let auth = Credential::config("OPENCODE_API_KEY")
            .or_else(Credential::public_key("public"))
            .bearer();
        let route = Route::make(RouteInput {
            id: "opencode-chat".into(),
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
            model: Box::new(|id, route| {
                Model::make(
                    ModelId::new(id),
                    ProviderId::new(ProviderId::OPENCODE),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            }),
            configure: move |c| OpenCodeProvider::new().configure(c),
        }
    }

}
