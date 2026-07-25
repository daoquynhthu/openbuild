use std::sync::Arc;

use indexmap::IndexMap;

use crate::auth::{AuthPolicy, CredentialCandidate};
use crate::endpoint::{Endpoint, EndpointPart};
use crate::provider::{ConfiguredProvider, DefaultRouteSelector, Provider, SharedProvider};
use crate::resolution::{ProviderImplementation, ResolvedProviderSpec};
use crate::route::Route;
use crate::types::{ApiBackend, AuthScheme, ModelSourceSpec, ProviderDefaults, ProviderId, RouteId};

pub type SharedProviderFactory = Arc<dyn ProviderFactory + Send + Sync>;

pub trait ProviderFactory: std::fmt::Debug {
    fn create(
        &self,
        spec: &ResolvedProviderSpec,
    ) -> Result<SharedProvider, crate::error::ProviderError>;
}

/// A lightweight provider struct created by the factory for each identity.
/// Its `configure()` method builds the full `ConfiguredProvider`.
#[derive(Debug)]
struct FactoryProvider {
    id: ProviderId,
    base_url: String,
    profile_env_key: Vec<String>,
    defaults: ProviderDefaults,
}

fn protocol_path(protocol: &str) -> Result<&'static str, crate::error::ProviderError> {
    match protocol {
        "chat_completions" => Ok("/chat/completions"),
        "responses" => Ok("/responses"),
        _ => Err(crate::error::ProviderError::Config(format!(
            "unsupported protocol `{protocol}` for OpenAI-compatible provider"
        ))),
    }
}

impl Provider for FactoryProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn name(&self) -> &str {
        "OpenAI Compatible"
    }

    fn defaults(&self) -> &ProviderDefaults {
        // FactoryProvider construction relies on the resolved spec for all
        // configuration. The defaults singleton is still required for trait
        // conformance but callers must use configure() for the full picture.
        &self.defaults
    }

    fn configure(&self, overrides: crate::config::ProviderConfig) -> ConfiguredProvider {
        let base_url = crate::providers::configure::resolve_base_url(
            overrides.base_url.as_deref(),
            &self.base_url,
        );
        let display_name = format!("{} (OpenAI Compatible)", self.id.0);

        // D1: Resolve protocol from overrides, use protocol name as path fallback
        let protocol = crate::providers::configure::resolve_protocol(overrides.protocol.as_deref());
        let path: String = protocol_path(&protocol)
            .map(|s| s.to_string())
            .unwrap_or_else(|_| protocol.clone());

        // D3: Merge profile env keys with user-provided env keys
        let mut all_env_keys = self.profile_env_key.clone();
        if let Some(ref user_keys) = overrides.env_key {
            all_env_keys.extend(user_keys.iter().cloned());
        }

        // D3: Build auth candidates — provider inline key first, then env keys
        let mut candidates: Vec<CredentialCandidate> = Vec::new();
        if overrides.api_key.is_some() {
            candidates.push(CredentialCandidate::ProviderInline);
        }
        if !all_env_keys.is_empty() {
            candidates.push(CredentialCandidate::ProviderEnvironment(all_env_keys));
        }
        if candidates.is_empty() {
            candidates.push(CredentialCandidate::ProviderEnvironment(vec![]));
        }

        let route_id = RouteId::new(format!("{}-{protocol}", self.id.0));
        let route = Route::make(
            route_id.0.clone(),
            Some(self.id.clone()),
            protocol.as_str(),
            Endpoint {
                base_url: Some(base_url),
                path: EndpointPart::Static(path),
                query: None,
            },
            AuthPolicy::bearer(candidates, false),
        );

        // D4: Apply extra headers from overrides
        let mut route = route;
        if let Some(ref extra) = overrides.extra_headers {
            for (key, value) in extra {
                route.static_headers.insert(key.clone(), value.clone());
            }
        }

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

#[derive(Debug)]
pub struct OpenAiCompatibleProviderFactory;

impl ProviderFactory for OpenAiCompatibleProviderFactory {
    fn create(
        &self,
        spec: &ResolvedProviderSpec,
    ) -> Result<SharedProvider, crate::error::ProviderError> {
        let profile = match &spec.implementation {
            ProviderImplementation::OpenAiCompatible { profile } => {
                profile.as_ref().map(|p| p.0.clone())
            }
            _ => {
                return Err(crate::error::ProviderError::Config(
                    "factory called with non-openai-compatible implementation".into(),
                ));
            }
        };

        let base_url = spec
            .config
            .public
            .base_url
            .clone()
            .or_else(|| {
                profile
                    .as_deref()
                    .and_then(super::openai_compatible::profile_base_url)
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let profile_env_key = Self::env_key_for_profile(profile.as_deref());

        let defaults = ProviderDefaults {
            id: spec.id.clone(),
            name: format!("{} (OpenAI Compatible)", spec.id.0),
            base_url: base_url.clone(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::Bearer,
            env_key: profile_env_key.clone(),
            context_window: std::num::NonZeroU64::new(128_000).unwrap_or_else(|| unreachable!()),
            model_list_endpoint: None,
            ..ProviderDefaults::default()
        };

        Ok(Arc::new(FactoryProvider {
            id: spec.id.clone(),
            base_url,
            profile_env_key,
            defaults,
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
    use crate::config::ProviderConfig;
    use crate::resolution::ResolvedProviderSpec;
    use crate::types::CompatibleProfileId;

    fn make_spec(
        id: &str,
        profile: Option<&str>,
        base_url: Option<String>,
    ) -> ResolvedProviderSpec {
        ResolvedProviderSpec {
            id: ProviderId::new(id),
            implementation: ProviderImplementation::OpenAiCompatible {
                profile: profile.map(CompatibleProfileId::new),
            },
            config: crate::resolution::ProviderRuntimeConfig {
                public: crate::resolution::ProviderPublicConfig {
                    base_url,
                    protocol: None,
                    model_list_path: None,
                    allow_insecure_http: false,
                    model_list_format: None,
                    extra_headers: IndexMap::new(),
                },
                inline_api_key: None,
                env_keys: vec![],
            },
        }
    }

    #[test]
    fn factory_two_custom_compatible_providers_independent() {
        // Simulate parsing the frozen TOML example from the plan:
        //   [provider.deepseek]
        //   kind = "openai_compatible"
        //   profile = "deepseek"
        //
        //   [provider.internal]
        //   kind = "openai_compatible"
        //   base_url = "https://llm.example/v1"
        //   protocol = "chat_completions"
        //   model_list_path = "/models"
        //   model_list_format = "openai_compatible"
        let factory = OpenAiCompatibleProviderFactory;

        let deepseek_spec = make_spec("deepseek", Some("deepseek"), None);
        let internal_spec = make_spec("internal", None, Some("https://llm.example/v1".into()));

        let ds_provider = factory.create(&deepseek_spec).unwrap();
        let int_provider = factory.create(&internal_spec).unwrap();

        // Identity isolation
        assert_eq!(ds_provider.id().0, "deepseek");
        assert_eq!(int_provider.id().0, "internal");

        // Configure each provider with its own user config
        let ds_cfg = ProviderConfig {
            id: Some("deepseek".into()),
            kind: Some("openai_compatible".into()),
            profile: Some("deepseek".into()),
            ..Default::default()
        };
        let int_cfg = ProviderConfig {
            id: Some("internal".into()),
            kind: Some("openai_compatible".into()),
            base_url: Some("https://llm.example/v1".into()),
            protocol: Some("chat_completions".into()),
            ..Default::default()
        };

        let ds_configured = ds_provider.configure(ds_cfg);
        let int_configured = int_provider.configure(int_cfg);

        // Endpoint isolation: each route must reference its own provider
        for (route_id, route) in &ds_configured.routes {
            assert_eq!(
                route.provider_id.0, "deepseek",
                "route {} must belong to deepseek",
                route_id.0
            );
        }
        for (route_id, route) in &int_configured.routes {
            assert_eq!(
                route.provider_id.0, "internal",
                "route {} must belong to internal",
                route_id.0
            );
        }

        // env_key isolation: deepseek route uses DEEPSEEK_API_KEY, internal uses empty
        let ds_route = ds_configured.routes.values().next().unwrap();
        let int_route = int_configured.routes.values().next().unwrap();
        let ds_auth = format!("{:?}", ds_route.auth);
        let int_auth = format!("{:?}", int_route.auth);
        assert!(
            ds_auth.contains("DEEPSEEK_API_KEY"),
            "deepseek route auth should contain DEEPSEEK_API_KEY, got {ds_auth}"
        );
        assert!(
            !int_auth.contains("DEEPSEEK_API_KEY"),
            "internal route auth should NOT contain DEEPSEEK_API_KEY, got {int_auth}"
        );

        // Route count: each must have exactly one route
        assert_eq!(ds_configured.routes.len(), 1);
        assert_eq!(int_configured.routes.len(), 1);

        // Route IDs must be distinct (use provider-specific prefix)
        let ds_route_id = &ds_configured.routes.keys().next().unwrap().0;
        let int_route_id = &int_configured.routes.keys().next().unwrap().0;
        assert_ne!(
            ds_route_id, int_route_id,
            "route IDs must not collide across providers"
        );
        assert!(
            ds_route_id.starts_with("deepseek-"),
            "deepseek route must use deepseek- prefix, got {ds_route_id}"
        );
        assert!(
            int_route_id.starts_with("internal-"),
            "internal route must use internal- prefix, got {int_route_id}"
        );
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
                    allow_insecure_http: false,
                    model_list_format: None,
                    extra_headers: IndexMap::new(),
                },
                inline_api_key: None,
                env_keys: vec![],
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

    /// D6 gate: two providers with different protocols, env keys, headers — fully isolated.
    #[test]
    fn factory_isolation_different_protocol_envkey_headers() {
        let factory = OpenAiCompatibleProviderFactory;

        // Provider A: chat_completions with custom env key and extra headers
        let spec_a = make_spec("provider-a", None, Some("http://mock-a:8080/v1".into()));
        let p_a = factory.create(&spec_a).unwrap();
        let cfg_a = ProviderConfig {
            id: Some("provider-a".into()),
            base_url: Some("http://mock-a:8080/v1".into()),
            protocol: Some("chat_completions".into()),
            env_key: Some(vec!["CUSTOM_A_KEY".into()]),
            extra_headers: Some(IndexMap::from([("X-Custom-A".into(), "value-a".into())])),
            ..Default::default()
        };
        let configured_a = p_a.configure(cfg_a);

        // Provider B: responses protocol with different env key and headers
        let spec_b = make_spec("provider-b", None, Some("http://mock-b:8080/v1".into()));
        let p_b = factory.create(&spec_b).unwrap();
        let cfg_b = ProviderConfig {
            id: Some("provider-b".into()),
            base_url: Some("http://mock-b:8080/v1".into()),
            protocol: Some("responses".into()),
            env_key: Some(vec!["CUSTOM_B_KEY".into()]),
            extra_headers: Some(IndexMap::from([("X-Custom-B".into(), "value-b".into())])),
            ..Default::default()
        };
        let configured_b = p_b.configure(cfg_b);

        // Route protocol isolation
        let route_a = configured_a.routes.values().next().unwrap();
        assert_eq!(&*route_a.protocol_id.0, "chat_completions");
        let endpoint_debug_a = format!("{:?}", route_a.endpoint);
        assert!(
            endpoint_debug_a.contains("/chat/completions"),
            "provider-a endpoint: {endpoint_debug_a}"
        );

        let route_b = configured_b.routes.values().next().unwrap();
        assert_eq!(&*route_b.protocol_id.0, "responses");
        let endpoint_debug_b = format!("{:?}", route_b.endpoint);
        assert!(
            endpoint_debug_b.contains("/responses"),
            "provider-b endpoint: {endpoint_debug_b}"
        );

        // Endpoint URL isolation (different base URLs)
        assert_ne!(
            endpoint_debug_a, endpoint_debug_b,
            "provider endpoints must differ"
        );

        // Extra header isolation
        assert!(
            format!("{:?}", route_a.static_headers).contains("X-Custom-A"),
            "provider-a must have X-Custom-A header"
        );
        assert!(
            !format!("{:?}", route_a.static_headers).contains("X-Custom-B"),
            "provider-a must NOT have provider-b's headers"
        );
        assert!(
            format!("{:?}", route_b.static_headers).contains("X-Custom-B"),
            "provider-b must have X-Custom-B header"
        );
        assert!(
            !format!("{:?}", route_b.static_headers).contains("X-Custom-A"),
            "provider-b must NOT have provider-a's headers"
        );

        // Auth env key isolation (auth policy must reference correct env keys)
        let auth_a = format!("{:?}", route_a.auth);
        let auth_b = format!("{:?}", route_b.auth);
        assert!(
            auth_a.contains("CUSTOM_A_KEY"),
            "provider-a auth must reference CUSTOM_A_KEY"
        );
        assert!(
            !auth_a.contains("CUSTOM_B_KEY"),
            "provider-a must not reference CUSTOM_B_KEY"
        );
        assert!(
            auth_b.contains("CUSTOM_B_KEY"),
            "provider-b auth must reference CUSTOM_B_KEY"
        );
        assert!(
            !auth_b.contains("CUSTOM_A_KEY"),
            "provider-b must not reference CUSTOM_A_KEY"
        );
    }
}
