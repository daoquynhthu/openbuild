use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::AuthPolicy;
use crate::endpoint::Endpoint;
use crate::error::ProviderError;
use crate::model::{GenerationOptions, Model, ModelLimits};
use crate::protocol::ProtocolId;
use crate::types::{ModelId, ProviderId, RouteId};

/// A Route represents one declarative inference endpoint.
///
/// V1 architecture (AD-02): Route is cloneable and contains all information
/// needed to construct a sampler request. Stream framing/decoding is owned
/// by the protocol implementation selected by `protocol_id`.
#[derive(Debug, Clone)]
pub struct Route {
    pub id: RouteId,
    pub provider_id: ProviderId,
    pub protocol_id: ProtocolId,
    pub endpoint: Endpoint<()>,
    pub auth: AuthPolicy,
    pub static_headers: IndexMap<String, String>,
    pub generation_defaults: GenerationOptions,
    pub limits: ModelLimits,
}

impl Route {
    /// Create a Route from its required fields.
    pub fn new(
        id: RouteId,
        provider_id: ProviderId,
        protocol_id: impl Into<ProtocolId>,
        endpoint: Endpoint<()>,
        auth: AuthPolicy,
    ) -> Self {
        Self {
            id,
            provider_id,
            protocol_id: protocol_id.into(),
            endpoint,
            auth,
            static_headers: IndexMap::new(),
            generation_defaults: GenerationOptions::default(),
            limits: ModelLimits::default(),
        }
    }

    /// Convenience constructor for backward compatibility.
    /// Wraps the new Route::new with string IDs.
    pub fn make(
        id: impl Into<String>,
        provider_id: Option<ProviderId>,
        protocol: impl Into<ProtocolId>,
        endpoint: Endpoint<()>,
        auth: AuthPolicy,
    ) -> Self {
        Self {
            id: RouteId::new(id),
            provider_id: provider_id.unwrap_or_else(|| ProviderId::new("unknown")),
            protocol_id: protocol.into(),
            endpoint,
            auth,
            static_headers: IndexMap::new(),
            generation_defaults: GenerationOptions::default(),
            limits: ModelLimits::default(),
        }
    }

    /// Bind a model ID to this route.
    pub fn model(&self, id: &str) -> Model {
        Model::make(
            ModelId::new(id),
            self.provider_id.clone(),
            Arc::new(self.clone()),
            None,
        )
    }

    /// Validate the route's fields.
    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.id.0.trim().is_empty() {
            return Err(ProviderError::InvalidRouteId(
                "route ID must not be empty".into(),
            ));
        }
        if self.protocol_id == ProtocolId::default() {
            return Err(ProviderError::UnknownProtocol(
                "protocol_id must not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CredentialCandidate;
    use crate::endpoint::EndpointPart;

    fn test_route() -> Route {
        Route::make(
            "test-chat",
            Some(ProviderId::new("openai")),
            "chat_completions",
            Endpoint {
                base_url: Some("https://api.openai.com/v1".into()),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            AuthPolicy::bearer(
                vec![CredentialCandidate::ModelEnvironment(vec![
                    "OPENAI_API_KEY".into(),
                ])],
                true,
            ),
        )
    }

    #[test]
    fn route_make_sets_fields() {
        let route = test_route();
        assert_eq!(route.id.0, "test-chat");
        assert_eq!(route.provider_id.0, "openai");
        assert_eq!(route.protocol_id, ProtocolId::from("chat_completions"));
    }

    #[test]
    fn route_model_creates_model() {
        let route = test_route();
        let model = route.model("gpt-4o");
        assert_eq!(model.id.0, "gpt-4o");
        assert_eq!(model.provider.0, "openai");
    }

    #[test]
    fn route_validate_valid() {
        let route = test_route();
        assert!(route.validate().is_ok());
    }

    #[test]
    fn route_validate_rejects_empty_protocol() {
        let route = Route::make(
            "test",
            Some(ProviderId::new("p")),
            "",
            Endpoint {
                base_url: Some("https://example.com".into()),
                path: EndpointPart::Static("/chat".into()),
                query: None,
            },
            AuthPolicy::None,
        );
        assert!(route.validate().is_err());
    }

    #[test]
    fn route_clone_is_independent() {
        let r1 = test_route();
        let mut r2 = r1.clone();
        r2.id = RouteId::new("other");
        assert_eq!(r1.id.0, "test-chat");
        assert_eq!(r2.id.0, "other");
    }
}
