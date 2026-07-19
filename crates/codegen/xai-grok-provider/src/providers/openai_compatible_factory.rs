#![allow(dead_code)]

use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::{AuthPolicy, CredentialSource};
use crate::endpoint::{Endpoint, EndpointPart};
use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider, SharedProvider};
use crate::resolution::{ProviderImplementation, ResolvedProviderSpec};
use crate::route::Route;
use crate::types::{ModelSourceSpec, ProviderDefaults, ProviderId, RouteId};

pub type SharedProviderFactory = Arc<dyn ProviderFactory + Send + Sync>;

pub trait ProviderFactory {
    fn create(&self, spec: &ResolvedProviderSpec) -> Result<SharedProvider, crate::error::ProviderError>;
}

/// A lightweight provider struct created by the factory for each identity.
/// Its `configure()` method builds the full `ConfiguredProvider`.
#[derive(Debug)]
struct FactoryProvider {
    id: ProviderId,
    profile: Option<String>,
    base_url: String,
    env_key: Vec<String>,
}

impl Provider for FactoryProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn name(&self) -> &str {
        "OpenAI Compatible"
    }

    fn defaults(&self) -> &ProviderDefaults {
        // FactoryProvider doesn't expose defaults — configuration comes from
        // the resolved spec. Delegates to configure() which uses spec fields.
        unimplemented!("use configure() to obtain ConfiguredProvider with full defaults")
    }

    fn configure(&self, overrides: crate::config::ProviderConfig) -> ConfiguredProvider {
        let base_url = overrides.base_url.clone().unwrap_or_else(|| self.base_url.clone());
        let display_name = format!("{} (OpenAI Compatible)", self.id.0);
        let route_id = RouteId::new(format!("{}-chat", self.id.0));
        let route = Route::make(
            route_id.0.clone(),
            Some(self.id.clone()),
            "chat_completions",
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static("/chat/completions".into()),
                query: None,
            },
            AuthPolicy::Bearer(CredentialSource::Environment(self.env_key.clone())),
        );
        let routes = IndexMap::from([(route_id.clone(), Arc::new(route))]);
        let selector = Arc::new(DefaultRouteSelector {
            default_route_id: route_id.clone(),
        });
        ConfiguredProvider::new(
            self.id.clone(),
            display_name,
            overrides,
            routes,
            route_id,
            selector,
            ModelSourceSpec::Dynamic,
        )
    }
}

pub struct OpenAiCompatibleProviderFactory;

impl ProviderFactory for OpenAiCompatibleProviderFactory {
    fn create(&self, spec: &ResolvedProviderSpec) -> Result<SharedProvider, crate::error::ProviderError> {
        let profile = match &spec.implementation {
            ProviderImplementation::OpenAiCompatible { profile } => profile.clone(),
            _ => {
                return Err(crate::error::ProviderError::Config(
                    "factory called with non-openai-compatible implementation".into(),
                ))
            }
        };

        let base_url = spec.config.public.base_url.clone()
            .or_else(|| {
                profile
                    .as_deref()
                    .and_then(super::openai_compatible::profile_base_url)
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let env_key = Self::env_key_for_profile(profile.as_deref());

        Ok(Arc::new(FactoryProvider {
            id: spec.id.clone(),
            profile,
            base_url,
            env_key,
        }))
    }
}

impl OpenAiCompatibleProviderFactory {
    fn env_key_for_profile(profile: Option<&str>) -> Vec<String> {
        match profile {
            Some("deepseek") => vec!["DEEPSEEK_API_KEY".into()],
            Some("groq") => vec!["GROQ_API_KEY".into()],
            Some("openrouter") => vec!["OPENROUTER_API_KEY".into()],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolution::ResolvedProviderSpec;

    fn make_spec(id: &str, profile: Option<&str>, base_url: Option<String>) -> ResolvedProviderSpec {
        ResolvedProviderSpec {
            id: ProviderId::new(id),
            implementation: ProviderImplementation::OpenAiCompatible {
                profile: profile.map(|s| s.to_string()),
            },
            config: crate::resolution::ProviderRuntimeConfig {
                public: crate::resolution::ProviderPublicConfig {
                    base_url,
                    protocol: None,
                    model_list_path: None,
                    model_list_format: None,
                    extra_headers: IndexMap::new(),
                },
                inline_api_key: None,
            },
        }
    }

    #[test]
    fn factory_creates_independent_instances() {
        let factory = OpenAiCompatibleProviderFactory;
        let deepseek = make_spec("deepseek", Some("deepseek"), None);
        let internal = make_spec("internal", None, Some("https://llm.internal/v1".into()));

        let ds_provider = factory.create(&deepseek).unwrap();
        let int_provider = factory.create(&internal).unwrap();

        assert_eq!(ds_provider.id().0, "deepseek");
        assert_eq!(int_provider.id().0, "internal");
        assert_ne!(ds_provider.id().0, int_provider.id().0);
    }

    #[test]
    fn factory_rejects_non_openai_compatible() {
        let factory = OpenAiCompatibleProviderFactory;
        let spec = ResolvedProviderSpec {
            id: ProviderId::new("xai"),
            implementation: ProviderImplementation::Builtin {
                definition_id: ProviderId::new("xai"),
            },
            config: crate::resolution::ProviderRuntimeConfig {
                public: crate::resolution::ProviderPublicConfig {
                    base_url: None,
                    protocol: None,
                    model_list_path: None,
                    model_list_format: None,
                    extra_headers: IndexMap::new(),
                },
                inline_api_key: None,
            },
        };
        let result = factory.create(&spec);
        assert!(result.is_err(), "builtin impl must be rejected");
    }

    #[test]
    fn factory_env_key_matches_profile() {
        assert_eq!(
            OpenAiCompatibleProviderFactory::env_key_for_profile(Some("deepseek")),
            vec!["DEEPSEEK_API_KEY"]
        );
        assert_eq!(
            OpenAiCompatibleProviderFactory::env_key_for_profile(Some("groq")),
            vec!["GROQ_API_KEY"]
        );
        assert_eq!(
            OpenAiCompatibleProviderFactory::env_key_for_profile(Some("openrouter")),
            vec!["OPENROUTER_API_KEY"]
        );
        assert!(OpenAiCompatibleProviderFactory::env_key_for_profile(None).is_empty());
    }
}
