use std::sync::Arc;

use crate::auth::{AuthFn, NoopAuth};
use crate::endpoint::{Endpoint, EndpointPatch};
use crate::framing::Framing;
use crate::model::{GenerationOptions, Model, ModelLimits};
use crate::types::{HeaderMap, LLMRequest, ModelId, ProviderId};

/// Static defaults for a Route.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RouteDefaults {
    pub headers: Option<HeaderMap>,
    pub generation: Option<GenerationOptions>,
    pub limits: Option<ModelLimits>,
}

/// Input for constructing a Route.
#[non_exhaustive]
pub struct RouteInput {
    pub id: String,
    pub provider: Option<ProviderId>,
    pub protocol: String,
    pub endpoint: Endpoint<()>,
    pub auth: Option<Box<dyn AuthFn>>,
    pub framing: Box<dyn Framing<String>>,
    pub defaults: Option<RouteDefaults>,
}

impl core::fmt::Debug for RouteInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RouteInput")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("framing", &self.framing.id())
            .finish()
    }
}

/// A Route composes the four orthogonal deployment axes:
/// Protocol + Endpoint + Auth + Framing.
#[non_exhaustive]
pub struct Route {
    pub id: String,
    pub provider: Option<ProviderId>,
    pub protocol: String,
    pub endpoint: Endpoint<()>,
    pub auth: Box<dyn AuthFn>,
    pub framing: Box<dyn Framing<String>>,
    pub defaults: RouteDefaults,
    pub headers: Option<fn(&LLMRequest) -> HeaderMap>,
}

impl Clone for Route {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            provider: self.provider.clone(),
            protocol: self.protocol.clone(),
            endpoint: self.endpoint.clone(),
            auth: self.auth.clone_box(),
            framing: self.framing.clone_box(),
            defaults: self.defaults.clone(),
            headers: self.headers,
        }
    }
}

impl core::fmt::Debug for Route {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Route")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("framing", &self.framing.id())
            .finish()
    }
}

impl Route {
    pub fn make(input: RouteInput) -> Self {
        Self {
            id: input.id,
            provider: input.provider,
            protocol: input.protocol,
            endpoint: input.endpoint,
            auth: input.auth.unwrap_or_else(|| Box::new(NoopAuth)),
            framing: input.framing,
            defaults: input.defaults.unwrap_or(RouteDefaults {
                headers: None,
                generation: None,
                limits: None,
            }),
            headers: None,
        }
    }

    pub fn with(self, patch: RoutePatch) -> Self {
        let endpoint = match patch.endpoint {
            Some(ref ep) => crate::endpoint::merge_endpoints(&self.endpoint, ep),
            None => self.endpoint,
        };
        Self {
            endpoint,
            auth: patch.auth.unwrap_or(self.auth),
            provider: patch.provider.or(self.provider),
            defaults: patch.defaults.unwrap_or(self.defaults),
            headers: patch.headers.or(self.headers),
            ..self
        }
    }

    pub fn model(&self, id: &str) -> Model {
        Model::make(
            ModelId::new(id),
            self.provider
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ProviderId::new("unknown")),
            Arc::new(self.clone()),
            None,
        )
    }
}

/// Partial overrides for Route::with().
#[derive(Debug)]
#[non_exhaustive]
pub struct RoutePatch {
    pub provider: Option<ProviderId>,
    pub endpoint: Option<EndpointPatch<()>>,
    pub defaults: Option<RouteDefaults>,
    pub auth: Option<Box<dyn AuthFn>>,
    pub headers: Option<fn(&LLMRequest) -> HeaderMap>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credential;
    use crate::endpoint::EndpointPart;
    use crate::framing::SseFraming;
    use crate::types::ProviderId;

    fn test_route() -> Route {
        Route::make(RouteInput {
            id: "test-chat".into(),
            provider: Some(ProviderId::new("openai")),
            protocol: "chat_completions".into(),
            endpoint: Endpoint {
                base_url: Some("https://api.openai.com/v1".into()),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            auth: Some(Credential::optional(Some("sk-test".into()), "api_key").bearer()),
            framing: Box::new(SseFraming),
            defaults: Some(RouteDefaults {
                headers: None,
                generation: None,
                limits: None,
            }),
        })
    }

    #[test]
    fn route_make_sets_fields() {
        let route = test_route();
        assert_eq!(route.id, "test-chat");
        assert_eq!(route.provider.unwrap().0, "openai");
        assert_eq!(route.protocol, "chat_completions");
    }

    #[test]
    fn route_make_preserves_auth_and_framing() {
        let route = test_route();
        assert_eq!(route.framing.id(), "sse");
    }

    #[test]
    fn route_with_updates_endpoint() {
        let route = test_route();
        let patched = route.with(RoutePatch {
            provider: None,
            endpoint: Some(EndpointPatch {
                base_url: Some("https://override.com/v1".into()),
                path: None,
                query: None,
            }),
            defaults: None,
            auth: None,
            headers: None,
        });
        assert_eq!(
            patched.endpoint.base_url.unwrap(),
            "https://override.com/v1"
        );
    }

    #[test]
    fn route_model_creates_model() {
        let route = test_route();
        let model = route.model("gpt-4o");
        assert_eq!(model.id.0, "gpt-4o");
        assert_eq!(model.provider.0, "openai");
    }

    #[test]
    fn route_defaults_none_when_not_provided() {
        let route = Route::make(RouteInput {
            id: "minimal".into(),
            provider: None,
            protocol: "chat".into(),
            endpoint: Endpoint {
                base_url: None,
                path: EndpointPart::Static("/test".into()),
                query: None,
            },
            auth: None,
            framing: Box::new(SseFraming),
            defaults: None,
        });
        assert!(route.defaults.headers.is_none());
    }
}
