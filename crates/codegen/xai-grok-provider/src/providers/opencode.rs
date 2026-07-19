use std::num::NonZeroU64;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::AuthPolicy;
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider};
use crate::route::Route;
use crate::types::{
    ApiBackend, AuthScheme, ModelSourceSpec, ProviderDefaults, ProviderId, RouteId,
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
        let route = Route::make(
            "opencode-chat",
            Some(self.defaults.id.clone()),
            "chat_completions",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            AuthPolicy::None,
        );
        let pid = self.defaults.id.clone();
        let route_id = RouteId::new("opencode-chat");
        let routes = IndexMap::from([(route_id.clone(), Arc::new(route))]);
        ConfiguredProvider::new(
            pid,
            self.defaults.name.clone(),
            overrides,
            routes,
            route_id.clone(),
            Arc::new(DefaultRouteSelector {
                default_route_id: route_id,
            }),
            ModelSourceSpec::Dynamic,
        )
    }
}
