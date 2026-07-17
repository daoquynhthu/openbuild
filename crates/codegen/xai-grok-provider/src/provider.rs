use std::sync::Arc;

use crate::config::ProviderConfig;
use crate::model::Model;
use crate::route::Route;
use crate::types::{ProviderDefaults, ProviderId};

#[non_exhaustive]
pub struct ConfiguredProvider {
    pub id: ProviderId,
    pub route: Route,
    pub model: fn(&str, &Route) -> Model,
    pub configure: fn(ProviderConfig) -> ConfiguredProvider,
}

impl core::fmt::Debug for ConfiguredProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfiguredProvider")
            .field("id", &self.id)
            .field("route", &self.route)
            .finish()
    }
}

/// A provider of LLM inference capabilities. Implementations represent
/// specific model providers (OpenAI, Anthropic, xAI, etc.) and are
/// registered with a [`ProviderRegistry`] for model resolution.
pub trait Provider: Send + Sync + core::fmt::Debug + 'static {
    fn id(&self) -> &ProviderId;
    fn name(&self) -> &str;
    fn defaults(&self) -> &ProviderDefaults;
    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider;
}

/// Thread-safe reference to a [`Provider`] trait object.
pub type SharedProvider = Arc<dyn Provider>;
