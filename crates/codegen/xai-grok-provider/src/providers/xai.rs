use std::num::NonZeroU64;

use indexmap::IndexMap;

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

pub(crate) fn xai_defaults() -> ProviderDefaults {
    ProviderDefaults {
        id: ProviderId::new(ProviderId::XAI),
        name: "xAI".into(),
        base_url: "https://api.x.ai/v1".into(),
        api_backend: ApiBackend::Responses,
        auth_scheme: AuthScheme::Bearer,
        env_key: vec!["XAI_API_KEY".into()],
        context_window: NonZeroU64::new(500_000).unwrap_or_else(|| unreachable!()),
        temperature: Some(0.7),
        top_p: Some(0.95),
        max_completion_tokens: Some(16384),
        supports_backend_search: true,
        supports_reasoning_effort: true,
        supports_streaming: true,
        supports_tool_calling: true,
        supports_structured_output: true,
        extra_headers: IndexMap::new(),
        model_list_endpoint: None,
        model_list_format: crate::types::ModelListFormat::OpenAiCompatible,
    }
}

#[derive(Debug)]
pub struct XaiProvider {
    defaults: ProviderDefaults,
}

impl XaiProvider {
    pub fn new() -> Self {
        Self {
            defaults: xai_defaults(),
        }
    }
}

impl Provider for XaiProvider {
    fn id(&self) -> &ProviderId {
        &self.defaults.id
    }

    fn name(&self) -> &str {
        "xAI"
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
            .or_else(Credential::config("XAI_API_KEY"))
            .or_else(Credential::session())
            .bearer();
        let mut xai_headers = std::collections::HashMap::new();
        xai_headers.insert(
            "x-grok-client-identifier".into(),
            "xai-grok-provider".into(),
        );
        let route = Route::make(RouteInput {
            id: "xai-responses".into(),
            provider: Some(self.defaults.id.clone()),
            protocol: "responses".into(),
            endpoint: Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/responses".into()),
                query: None,
            },
            auth: Some(auth),
            framing: Box::new(SseFraming),
            defaults: Some(crate::route::RouteDefaults {
                headers: Some(xai_headers),
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
                    ProviderId::new(ProviderId::XAI),
                    std::sync::Arc::new(route.clone()),
                    None,
                )
            }),
            configure: Box::new(move |c| XaiProvider::new().configure(c)),
        }
    }

}
