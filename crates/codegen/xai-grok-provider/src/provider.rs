use std::sync::Arc;

use indexmap::IndexMap;

use crate::config::ProviderConfig;
use crate::error::ProviderError;
use crate::route::Route;
use crate::types::{ProviderDefaults, ProviderId, RouteId};

/// A provider fully configured with user overrides, owning a route set.
///
/// V1 architecture (AD-03): A configured provider exposes all of its routes,
/// a default route, and a tested route-selection policy.
#[non_exhaustive]
pub struct ConfiguredProvider {
    pub id: ProviderId,
    pub display_name: String,
    pub config: ProviderConfig,
    pub routes: IndexMap<RouteId, Arc<Route>>,
    pub default_route_id: RouteId,
    pub route_selector: Arc<dyn RouteSelector>,
}

impl ConfiguredProvider {
    /// Create a new ConfiguredProvider from parts.
    pub fn new(
        id: ProviderId,
        display_name: String,
        config: ProviderConfig,
        routes: IndexMap<RouteId, Arc<Route>>,
        default_route_id: RouteId,
        route_selector: Arc<dyn RouteSelector>,
    ) -> Self {
        Self {
            id,
            display_name,
            config,
            routes,
            default_route_id,
            route_selector,
        }
    }
}

impl core::fmt::Debug for ConfiguredProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfiguredProvider")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("default_route_id", &self.default_route_id)
            .field("routes", &self.routes.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Determines which route a model ID should use.
///
/// V1: OpenAI uses this seam to select Responses versus Chat Completions.
/// All providers must test their selector.
pub trait RouteSelector: Send + Sync + core::fmt::Debug {
    fn select(&self, model_id: &str) -> Result<RouteId, ProviderError>;
}

/// Route selector that always returns the default route.
#[derive(Debug)]
pub struct DefaultRouteSelector {
    pub default_route_id: RouteId,
}

impl RouteSelector for DefaultRouteSelector {
    fn select(&self, _model_id: &str) -> Result<RouteId, ProviderError> {
        Ok(self.default_route_id.clone())
    }
}

/// A stateless provider definition. Registered in ProviderRegistry.
pub trait Provider: Send + Sync + core::fmt::Debug + 'static {
    fn id(&self) -> &ProviderId;
    fn name(&self) -> &str;
    fn defaults(&self) -> &ProviderDefaults;

    /// Override defaults with user config → a configured provider
    /// that owns a route set and route selector.
    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider;
}

/// Thread-safe reference to a [`Provider`] trait object.
pub type SharedProvider = Arc<dyn Provider>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_route_selector_returns_default() {
        let rid = RouteId::new("my-route");
        let selector = DefaultRouteSelector {
            default_route_id: rid.clone(),
        };
        let result = selector.select("any-model");
        assert_eq!(result.unwrap(), rid);
    }
}
