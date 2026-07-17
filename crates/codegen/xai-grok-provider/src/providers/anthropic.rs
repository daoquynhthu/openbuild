use std::num::NonZeroU64;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::{AuthPolicy, CredentialSource};
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider};
use crate::route::Route;
use crate::types::{
    ApiBackend, AuthScheme, ModelSourceSpec, ProviderDefaults, ProviderId, RouteId,
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
        let mut route = Route::make(
            "anthropic-messages",
            Some(self.defaults.id.clone()),
            "messages",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/messages".into()),
                query: None,
            },
            AuthPolicy::Header {
                name: "x-api-key".into(),
                source: CredentialSource::Environment(vec!["ANTHROPIC_API_KEY".into()]),
            },
        );
        route
            .static_headers
            .insert("anthropic-version".into(), "2023-06-01".into());
        let pid = self.defaults.id.clone();
        let route_id = RouteId::new("anthropic-messages");
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
