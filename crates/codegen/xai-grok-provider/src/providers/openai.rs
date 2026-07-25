use std::num::NonZeroU64;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::AuthPolicy;
use crate::config::ProviderConfig;
use crate::endpoint::{Endpoint, EndpointPart};
use crate::error::ProviderError;
use crate::provider::{ConfiguredProvider, Provider, RouteSelector};
use crate::route::Route;
use crate::types::{
    ApiBackend, AuthScheme, ModelSourceSpec, ProviderDefaults, ProviderId, RouteId,
};

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

/// Route selector for OpenAI that maps model IDs to the correct route.
///
/// Selection rules (frozen for V1):
/// - Models with names starting with "o1" or "o3" → responses route
/// - All other models → chat route (default)
#[derive(Debug)]
pub struct OpenAiRouteSelector {
    responses_route_id: RouteId,
    default_route_id: RouteId,
    referenced: Vec<RouteId>,
}

impl OpenAiRouteSelector {
    pub fn new(chat_route_id: RouteId, responses_route_id: RouteId, default_route_id: RouteId) -> Self {
        let referenced = vec![chat_route_id, responses_route_id.clone()];
        Self {
            responses_route_id,
            default_route_id,
            referenced,
        }
    }
}

impl RouteSelector for OpenAiRouteSelector {
    fn select(&self, model_id: &str) -> Result<RouteId, ProviderError> {
        let lower = model_id.to_lowercase();
        if lower.starts_with("o1") || lower.starts_with("o3") {
            Ok(self.responses_route_id.clone())
        } else {
            Ok(self.default_route_id.clone())
        }
    }

    fn referenced_route_ids(&self) -> &[RouteId] {
        &self.referenced
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
        let base_url = crate::providers::configure::resolve_base_url(
            overrides.base_url.as_deref(),
            &self.defaults.base_url,
        );
        let candidates = crate::providers::configure::build_credential_candidates(
            overrides.api_key.is_some(),
            overrides.env_key.as_deref().unwrap_or_default(),
            &self.defaults.env_key,
        );
        let auth = AuthPolicy::bearer(candidates, true);
        let mut route_chat = Route::make(
            "openai-chat",
            Some(self.defaults.id.clone()),
            "chat_completions",
            Endpoint {
                base_url: Some(base_url.clone()),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            auth.clone(),
        );
        if let Some(ref extra) = overrides.extra_headers {
            for (key, value) in extra {
                route_chat.static_headers.insert(key.clone(), value.clone());
            }
        }
        let route_chat = Arc::new(route_chat);
        let mut route_responses = Route::make(
            "openai-responses",
            Some(self.defaults.id.clone()),
            "responses",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/responses".into()),
                query: None,
            },
            auth,
        );
        if let Some(ref extra) = overrides.extra_headers {
            for (key, value) in extra {
                route_responses.static_headers.insert(key.clone(), value.clone());
            }
        }
        let route_responses = Arc::new(route_responses);

        let pid = self.defaults.id.clone();
        let route_id_chat = RouteId::new("openai-chat");
        let route_id_responses = RouteId::new("openai-responses");
        let routes = IndexMap::from([
            (route_id_chat.clone(), route_chat),
            (route_id_responses.clone(), route_responses),
        ]);
        let default_route_id = match overrides.protocol.as_deref() {
            Some("responses") => route_id_responses.clone(),
            _ => route_id_chat.clone(),
        };
        ConfiguredProvider::new(
            pid,
            self.defaults.name.clone(),
            overrides,
            routes,
            default_route_id.clone(),
            Arc::new(OpenAiRouteSelector::new(
                route_id_chat,
                route_id_responses,
                default_route_id,
            )),
            ModelSourceSpec::Dynamic,
        )
    }
}
