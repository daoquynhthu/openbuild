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
            defaults: input.defaults.unwrap_or(RouteDefaults { headers: None }),
        }
    }

    pub fn with(self, _patch: RoutePatch) -> Self {
        self
    }

    pub fn model(&self, _id: &str) -> Model {
        Model::make(
            String::new(),
            self.provider.clone().map(|p| p.0).unwrap_or_default(),
            Arc::new(self.clone()),
            None,
        )
    }
}

#[derive(Debug)]
pub struct RoutePatch {
    pub provider: Option<ProviderId>,
}
