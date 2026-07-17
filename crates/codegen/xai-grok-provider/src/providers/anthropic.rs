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

fn anthropic_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::ANTHROPIC),
        name: "Anthropic".into(),
        base_url: "https://api.anthropic.com/v1".into(),
        api_backend: ApiBackend::Messages,
        auth_scheme: AuthScheme::XApiKey,
        env_key: vec!["ANTHROPIC_API_KEY".into()],
        context_window: NonZeroU64::new(200_000).unwrap_or_else(|| unreachable!()),
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
pub struct AnthropicProvider {
    defaults: ProviderDefaults,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            defaults: anthropic_defaults(),
        }
    }
}

impl Provider for AnthropicProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "Anthropic"
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
            .or_else(Credential::config("ANTHROPIC_API_KEY"))
            .header("x-api-key");
        let mut route_headers = std::collections::HashMap::new();
        route_headers.insert("anthropic-version".into(), "2023-06-01".into());
        let route = Route::make(RouteInput {
            id: "anthropic-messages".into(),
            provider: Some(self.defaults.id.clone()),
            protocol: "messages".into(),
            endpoint: Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/messages".into()),
                query: None,
            },
            auth: Some(auth),
            framing: Box::new(SseFraming),
            defaults: Some(crate::route::RouteDefaults {
                headers: Some(route_headers),
                generation: None,
                limits: None,
            }),
        });
        let pid = self.defaults.id.clone();
        ConfiguredProvider {
            id: pid,
            route,
            model: Box::new(|id, route| {
                Model::make(
                    ModelId::new(id),
                    ProviderId::new(ProviderId::ANTHROPIC),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            }),
            configure: Box::new(move |c| AnthropicProvider::new().configure(c)),
        }
    }

}
