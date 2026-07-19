use std::num::NonZeroU64;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::{AuthPolicy, CredentialCandidate};
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider};
use crate::route::Route;
use crate::types::{
    ApiBackend, AuthScheme, ModelSourceSpec, ProviderDefaults, ProviderId, RouteId,
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
        let mut route = Route::make(
            "xai-responses",
            Some(self.defaults.id.clone()),
            "responses",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/responses".into()),
                query: None,
            },
            AuthPolicy::bearer(vec![CredentialCandidate::ProviderEnvironment(vec!["XAI_API_KEY".into()])], true),
        );
        // Preserve legacy x-grok-* headers for backward compatibility.
        route
            .static_headers
            .insert("x-grok-auth-mode".into(), "api-key".into());
        let pid = self.defaults.id.clone();
        let route_id = RouteId::new("xai-responses");
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
