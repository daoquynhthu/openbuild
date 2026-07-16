use std::sync::Arc;

use crate::config::ProviderConfig;
use crate::model::Model;
use crate::route::Route;
use crate::types::{ProviderDefaults, ProviderId, ProviderModelDef};

pub struct ConfiguredProvider {
    pub id: ProviderId,
    pub route: Route,
    pub model: fn(&str, &Route) -> Model,
    pub configure: fn(ProviderConfig) -> ConfiguredProvider,
}

pub trait Provider: Send + Sync + core::fmt::Debug {
    fn id(&self) -> &ProviderId;
    fn name(&self) -> &str;
    fn defaults(&self) -> &ProviderDefaults;
    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider;
    fn known_models(&self) -> &[ProviderModelDef];
}

pub type SharedProvider = Arc<dyn Provider>;
