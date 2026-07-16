use std::sync::Arc;

use crate::auth::AuthFn;
use crate::endpoint::Endpoint;
use crate::framing::Framing;
use crate::model::Model;
use crate::types::ProviderId;

#[derive(Debug, Clone)]
pub struct RouteDefaults {
    pub headers: Option<std::collections::HashMap<String, String>>,
}

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

#[derive(Debug, Clone)]
pub struct Route {
    pub id: String,
    pub provider: Option<ProviderId>,
    pub protocol: String,
    pub endpoint: Endpoint<()>,
    pub defaults: RouteDefaults,
}

impl Route {
    pub fn make(input: RouteInput) -> Self {
        Self {
            id: input.id,
            provider: input.provider,
            protocol: input.protocol,
            endpoint: input.endpoint,
            defaults: input.defaults.unwrap_or(RouteDefaults {
                headers: None,
            }),
        }
    }

    pub fn with(&self, patch: RoutePatch) -> Self {
        let mut route = self.clone();
        if let Some(provider) = patch.provider {
            route.provider = Some(provider);
        }
        if let Some(endpoint) = patch.endpoint {
            route.endpoint = crate::endpoint::merge_endpoints(&self.endpoint, &endpoint);
        }
        route
    }

    pub fn model(&self, id: &str) -> Model {
        Model::make(
            id.to_owned(),
            self.provider
                .as_ref()
                .map(|p| p.0.clone())
                .unwrap_or_default(),
            Arc::new(self.clone()),
            None,
        )
    }
}

#[derive(Debug)]
pub struct RoutePatch {
    pub provider: Option<ProviderId>,
    pub endpoint: Option<Endpoint<()>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credential;
    use crate::endpoint::EndpointPart;
    use crate::framing::SseFraming;

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
    fn route_with_updates_endpoint() {
        let route = test_route();
        let patched = route.with(RoutePatch {
            provider: None,
            endpoint: Some(Endpoint {
                base_url: Some("https://override.com/v1".into()),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            }),
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
        assert_eq!(model.id, "gpt-4o");
        assert_eq!(model.provider, "openai");
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
