use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use xai_grok_provider::auth::{CredentialCandidate, SecretValue};

/// Environment reader — only resolves variables at request time.
pub trait EnvironmentReader: Send + Sync {
    fn read(&self, var: &str) -> Result<Option<SecretValue>, String>;
}

/// Deterministic environment reader for testing.
#[derive(Default)]
pub(crate) struct TestEnvironment {
    vars: std::collections::HashMap<String, String>,
}

impl TestEnvironment {
    pub fn set(mut self, var: &str, value: &str) -> Self {
        self.vars.insert(var.to_string(), value.to_string());
        self
    }
}

impl EnvironmentReader for TestEnvironment {
    fn read(&self, var: &str) -> Result<Option<SecretValue>, String> {
        Ok(self.vars.get(var).map(|v| SecretValue::new(v.clone())))
    }
}

/// Session credential resolver — async, no block_on.
/// Uses boxed-future pattern as required by P8-003.
pub trait SessionCredentialResolver: Send + Sync {
    fn resolve(&self) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, String>> + Send>>;
}

/// Concrete environment reader that reads from process env at request time.
pub(crate) struct ProcessEnvironment;

/// xAI OAuth session resolver. Wraps the existing `AuthManager` to provide
/// session tokens as a `CredentialCandidate::Session` resolver (P8-005).
#[derive(Clone)]
pub struct XaiSessionResolver {
    manager: std::sync::Arc<crate::auth::manager::AuthManager>,
}

impl XaiSessionResolver {
    pub fn new(manager: std::sync::Arc<crate::auth::manager::AuthManager>) -> Self {
        Self { manager }
    }
}

impl SessionCredentialResolver for XaiSessionResolver {
    fn resolve(&self) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, String>> + Send>> {
        let auth = self.manager.current_or_expired();
        Box::pin(async move {
            match auth {
                Some(a) => Ok(Some(SecretValue::new(a.key.clone()))),
                None => match std::env::var("XAI_SESSION_TOKEN") {
                    Ok(val) if !val.is_empty() => Ok(Some(SecretValue::new(val))),
                    _ => Ok(None),
                },
            }
        })
    }
}

impl EnvironmentReader for ProcessEnvironment {
    fn read(&self, var: &str) -> Result<Option<SecretValue>, String> {
        match std::env::var(var) {
            Ok(val) if !val.is_empty() => Ok(Some(SecretValue::new(val))),
            _ => Ok(None),
        }
    }
}

/// Request-time credential context for `prepare_sampler_config`.
pub struct RequestCredentialContext<'a> {
    pub request_override: Option<&'a SecretValue>,
    pub model_inline: Option<&'a SecretValue>,
    pub provider_inline: Option<&'a SecretValue>,
    pub environment: &'a dyn EnvironmentReader,
    pub session: &'a dyn SessionCredentialResolver,
}

impl<'a> RequestCredentialContext<'a> {
    pub fn new(
        request_override: Option<&'a SecretValue>,
        model_inline: Option<&'a SecretValue>,
        provider_inline: Option<&'a SecretValue>,
        environment: &'a dyn EnvironmentReader,
        session: &'a dyn SessionCredentialResolver,
    ) -> Self {
        Self {
            request_override,
            model_inline,
            provider_inline,
            environment,
            session,
        }
    }

    /// Resolve a credential from a list of candidates following the UNIFIED priority:
    /// request override > model inline > provider inline > model env > provider env > built-in env > session.
    ///
    /// The provider declares ONLY which candidates exist; the ORDER is fixed by the system.
    /// Provider-declared order is ignored — only the SET of candidate types matters.
    pub async fn resolve_candidates(&self, candidates: &[CredentialCandidate]) -> Option<String> {
        // Build a set of candidate types declared by the provider.
        let provider_has_request_override = candidates
            .iter()
            .any(|c| matches!(c, CredentialCandidate::RequestOverride));
        let provider_has_model_inline = candidates
            .iter()
            .any(|c| matches!(c, CredentialCandidate::ModelInline));
        let provider_has_provider_inline = candidates
            .iter()
            .any(|c| matches!(c, CredentialCandidate::ProviderInline));
        let provider_env_keys: Vec<&Vec<String>> = candidates
            .iter()
            .filter_map(|c| match c {
                CredentialCandidate::ModelEnvironment(k) => Some(k),
                _ => None,
            })
            .collect();
        let provider_env_keys: Vec<&String> =
            provider_env_keys.iter().flat_map(|v| v.iter()).collect();
        let provider_env_keys: Vec<String> = provider_env_keys.into_iter().cloned().collect();
        let builtin_env_keys: Vec<String> = candidates
            .iter()
            .filter_map(|c| match c {
                CredentialCandidate::BuiltinEnvironment(k) => Some(k.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let has_session = candidates
            .iter()
            .any(|c| matches!(c, CredentialCandidate::Session(_)));

        // System-fixed priority order — provider order is ignored.
        // 1. RequestOverride
        if provider_has_request_override {
            if let Some(val) = self.request_override {
                return Some(val.inner().to_string());
            }
        }
        // 2. ModelInline
        if provider_has_model_inline {
            if let Some(val) = self.model_inline {
                return Some(val.inner().to_string());
            }
        }
        // 3. ProviderInline
        if provider_has_provider_inline {
            if let Some(val) = self.provider_inline {
                return Some(val.inner().to_string());
            }
        }
        // 4-6: Environment (model env, provider env, built-in env — in that order)
        for key in &provider_env_keys {
            if let Ok(Some(val)) = self.environment.read(key) {
                return Some(val.inner().to_string());
            }
        }
        for key in &builtin_env_keys {
            if let Ok(Some(val)) = self.environment.read(key) {
                return Some(val.inner().to_string());
            }
        }
        // 7. Session (always last)
        if has_session {
            if let Ok(Some(val)) = self.session.resolve().await {
                return Some(val.inner().to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SessionCredentialResolver for () {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, String>> + Send>> {
            Box::pin(async { Ok(None) })
        }
    }

    /// A session resolver that returns a fixed value.
    struct FixedSession(&'static str);

    impl SessionCredentialResolver for FixedSession {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, String>> + Send>> {
            let val = self.0.to_string();
            Box::pin(async move { Ok(Some(SecretValue::new(val))) })
        }
    }

    #[test]
    fn environment_reader_reads_existing_var() {
        let env = TestEnvironment::default().set("MY_KEY", "secret-value");
        let result = env.read("MY_KEY").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().inner(), "secret-value");
    }

    #[test]
    fn environment_reader_returns_none_for_missing() {
        let env = TestEnvironment::default();
        let result = env.read("NONEXISTENT").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn session_resolver_returns_none_by_default() {
        let resolver = ();
        let result = resolver.resolve().await.unwrap();
        assert!(result.is_none());
    }

    // P8-004: unified priority tests
    // Order: request override > model inline > provider inline > model env > provider env > built-in env > session

    #[tokio::test]
    async fn priority_request_override_wins() {
        let request = SecretValue::new("req".to_string());
        let model = SecretValue::new("model".to_string());
        let env = TestEnvironment::default().set("ENV_KEY", "env-val");
        let session = FixedSession("sess");
        let ctx = RequestCredentialContext::new(Some(&request), Some(&model), None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
                CredentialCandidate::ModelEnvironment(vec!["ENV_KEY".into()]),
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert_eq!(result.as_deref(), Some("req"));
    }

    #[tokio::test]
    async fn priority_model_inline_over_provider_env() {
        let model = SecretValue::new("model-inline".to_string());
        let env = TestEnvironment::default().set("PROV_KEY", "prov-val");
        let session = FixedSession("sess");
        let ctx = RequestCredentialContext::new(None, Some(&model), None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
                CredentialCandidate::ProviderEnvironment(vec!["PROV_KEY".into()]),
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert_eq!(result.as_deref(), Some("model-inline"));
    }

    #[tokio::test]
    async fn priority_env_over_session() {
        let env = TestEnvironment::default().set("XAI_API_KEY", "from-env");
        let session = FixedSession("from-session");
        let ctx = RequestCredentialContext::new(None, None, None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::BuiltinEnvironment(vec!["XAI_API_KEY".into()]),
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert_eq!(result.as_deref(), Some("from-env"));
    }

    #[tokio::test]
    async fn priority_session_fallback_when_none_above() {
        let env = TestEnvironment::default();
        let session = FixedSession("sess-token");
        let ctx = RequestCredentialContext::new(None, None, None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
                CredentialCandidate::ProviderInline,
                CredentialCandidate::BuiltinEnvironment(vec!["MISSING_KEY".into()]),
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert_eq!(result.as_deref(), Some("sess-token"));
    }

    #[tokio::test]
    async fn priority_none_when_all_missing() {
        let env = TestEnvironment::default();
        // Session resolver returns None rather than empty string
        let session = ();
        let ctx = RequestCredentialContext::new(None, None, None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert!(result.is_none());
    }

    #[test]
    fn credential_context_holds_references() {
        let val = SecretValue::new("tok".to_string());
        let env = TestEnvironment::default().set("ENV_KEY", "env-val");
        let resolver = ();
        let ctx = RequestCredentialContext::new(Some(&val), None, None, &env, &resolver);
        assert_eq!(ctx.request_override.unwrap().inner(), "tok");
    }
}
