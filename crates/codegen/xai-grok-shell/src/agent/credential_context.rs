use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use xai_grok_provider::auth::{CredentialCandidate, CredentialError, SecretValue};

pub use xai_grok_provider::auth::{
    EnvironmentReader, RequestCredentialContext, SessionCredentialResolver,
};

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
    fn read(&self, var: &str) -> Result<Option<SecretValue>, CredentialError> {
        Ok(self.vars.get(var).map(|v| SecretValue::new(v.clone())))
    }
}

/// Concrete environment reader that reads from process env at request time.
pub(crate) struct ProcessEnvironment;

impl EnvironmentReader for ProcessEnvironment {
    fn read(&self, var: &str) -> Result<Option<SecretValue>, CredentialError> {
        match std::env::var(var) {
            Ok(val) if !val.is_empty() => Ok(Some(SecretValue::new(val))),
            _ => Ok(None),
        }
    }
}

/// Session resolver that always returns `None`.
pub(crate) struct NoopSessionResolver;

impl SessionCredentialResolver for NoopSessionResolver {
    fn resolve(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>> {
        Box::pin(async { Ok(None) })
    }
}

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
    fn resolve(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A session resolver that returns a fixed value.
    struct FixedSession(&'static str);

    impl SessionCredentialResolver for FixedSession {
        fn resolve(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>>
        {
            let val = self.0.to_string();
            Box::pin(async move { Ok(Some(SecretValue::new(val))) })
        }
    }

    #[test]
    fn environment_reader_reads_existing_var() {
        let env = TestEnvironment::default().set("MY_KEY", "secret-value");
        let result = env.read("MY_KEY").unwrap();
        assert_eq!(result, Some(SecretValue::new("secret-value".to_string())));
    }

    #[test]
    fn environment_reader_returns_none_for_missing() {
        let env = TestEnvironment::default();
        let result = env.read("NONEXISTENT").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn session_resolver_returns_none_by_default() {
        let resolver = NoopSessionResolver;
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
        assert_eq!(result.unwrap().as_deref(), Some("req"));
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
        assert_eq!(result.unwrap().as_deref(), Some("model-inline"));
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
        assert_eq!(result.unwrap().as_deref(), Some("from-env"));
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
        assert_eq!(result.unwrap().as_deref(), Some("sess-token"));
    }

    #[tokio::test]
    async fn priority_none_when_all_missing() {
        let env = TestEnvironment::default();
        // Session resolver returns None rather than empty string
        let session = NoopSessionResolver;
        let ctx = RequestCredentialContext::new(None, None, None, &env, &session);
        let result = ctx
            .resolve_candidates(&[
                CredentialCandidate::RequestOverride,
                CredentialCandidate::ModelInline,
                CredentialCandidate::Session(xai_grok_provider::auth::SessionKind::Xai),
            ])
            .await;
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn credential_context_holds_references() {
        let val = SecretValue::new("tok".to_string());
        let env = TestEnvironment::default().set("ENV_KEY", "env-val");
        let session = NoopSessionResolver;
        let ctx = RequestCredentialContext::new(Some(&val), None, None, &env, &session);
        let expected = SecretValue::new("tok".to_string());
        assert_eq!(ctx.request_override, Some(&expected));
    }
}
