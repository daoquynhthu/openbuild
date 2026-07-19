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
        env_key: vec![], // No default env key — user must configure explicitly
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
        let route = Route::make(
            "openai-compatible-chat",
            Some(self.defaults.id.clone()),
            "chat_completions",
            Endpoint {
                base_url: Some(resolved_url),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            AuthPolicy::Bearer(CredentialSource::Environment(self.defaults.env_key.clone())),
        );
        let pid = self.defaults.id.clone();
        let route_id = RouteId::new("openai-compatible-chat");
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
