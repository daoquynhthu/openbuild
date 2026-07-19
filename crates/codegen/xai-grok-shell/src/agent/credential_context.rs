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
#[async_trait::async_trait]
pub trait SessionCredentialResolver: Send + Sync {
    async fn resolve(&self) -> Result<Option<SecretValue>, String>;
}

/// Concrete environment reader that reads from process env at request time.
pub(crate) struct ProcessEnvironment;

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

    /// Resolve a credential from a list of candidates following the unified priority:
    /// request override > model inline > provider inline > model env > provider env > built-in env > session.
    pub async fn resolve_candidates(
        &self,
        candidates: &[CredentialCandidate],
    ) -> Option<String> {
        for candidate in candidates {
            let value = match candidate {
                CredentialCandidate::RequestOverride => {
                    self.request_override.map(|s| s.inner().to_string())
                }
                CredentialCandidate::ModelInline => {
                    self.model_inline.map(|s| s.inner().to_string())
                }
                CredentialCandidate::ProviderInline => {
                    self.provider_inline.map(|s| s.inner().to_string())
                }
                CredentialCandidate::ModelEnvironment(keys)
                | CredentialCandidate::ProviderEnvironment(keys)
                | CredentialCandidate::BuiltinEnvironment(keys) => {
                    for key in keys {
                        if let Ok(Some(val)) = self.environment.read(key) {
                            return Some(val.inner().to_string());
                        }
                    }
                    None
                }
                CredentialCandidate::Session => {
                    if let Ok(Some(val)) = self.session.resolve().await {
                        return Some(val.inner().to_string());
                    }
                    None
                }
            };
            if value.is_some() {
                return value;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_trait::async_trait]
    impl SessionCredentialResolver for () {
        async fn resolve(&self) -> Result<Option<SecretValue>, String> {
            Ok(None)
        }
    }

    /// A session resolver that returns a fixed value.
    struct FixedSession(&'static str);

    #[async_trait::async_trait]
    impl SessionCredentialResolver for FixedSession {
        async fn resolve(&self) -> Result<Option<SecretValue>, String> {
            Ok(Some(SecretValue::new(self.0.to_string())))
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
        let ctx = RequestCredentialContext::new(
            Some(&request), Some(&model), None, &env, &session,
        );
        let result = ctx.resolve_candidates(&[
            CredentialCandidate::RequestOverride,
            CredentialCandidate::ModelInline,
            CredentialCandidate::ModelEnvironment(vec!["ENV_KEY".into()]),
            CredentialCandidate::Session,
        ]).await;
        assert_eq!(result.as_deref(), Some("req"));
    }

    #[tokio::test]
    async fn priority_model_inline_over_provider_env() {
        let model = SecretValue::new("model-inline".to_string());
        let env = TestEnvironment::default().set("PROV_KEY", "prov-val");
        let session = FixedSession("sess");
        let ctx = RequestCredentialContext::new(
            None, Some(&model), None, &env, &session,
        );
        let result = ctx.resolve_candidates(&[
            CredentialCandidate::RequestOverride,
            CredentialCandidate::ModelInline,
            CredentialCandidate::ProviderEnvironment(vec!["PROV_KEY".into()]),
            CredentialCandidate::Session,
        ]).await;
        assert_eq!(result.as_deref(), Some("model-inline"));
    }

    #[tokio::test]
    async fn priority_env_over_session() {
        let env = TestEnvironment::default().set("XAI_API_KEY", "from-env");
        let session = FixedSession("from-session");
        let ctx = RequestCredentialContext::new(
            None, None, None, &env, &session,
        );
        let result = ctx.resolve_candidates(&[
            CredentialCandidate::BuiltinEnvironment(vec!["XAI_API_KEY".into()]),
            CredentialCandidate::Session,
        ]).await;
        assert_eq!(result.as_deref(), Some("from-env"));
    }

    #[tokio::test]
    async fn priority_session_fallback_when_none_above() {
        let env = TestEnvironment::default();
        let session = FixedSession("sess-token");
        let ctx = RequestCredentialContext::new(
            None, None, None, &env, &session,
        );
        let result = ctx.resolve_candidates(&[
            CredentialCandidate::RequestOverride,
            CredentialCandidate::ModelInline,
            CredentialCandidate::ProviderInline,
            CredentialCandidate::BuiltinEnvironment(vec!["MISSING_KEY".into()]),
            CredentialCandidate::Session,
        ]).await;
        assert_eq!(result.as_deref(), Some("sess-token"));
    }

    #[tokio::test]
    async fn priority_none_when_all_missing() {
        let env = TestEnvironment::default();
        // Session resolver returns None rather than empty string
        let session = ();
        let ctx = RequestCredentialContext::new(
            None, None, None, &env, &session,
        );
        let result = ctx.resolve_candidates(&[
            CredentialCandidate::RequestOverride,
            CredentialCandidate::ModelInline,
            CredentialCandidate::Session,
        ]).await;
        assert!(result.is_none());
    }

    #[test]
    fn credential_context_holds_references() {
        let val = SecretValue::new("tok".to_string());
        let env = TestEnvironment::default().set("ENV_KEY", "env-val");
        let resolver = ();
        let ctx = RequestCredentialContext::new(
            Some(&val),
            None,
            None,
            &env,
            &resolver,
        );
        assert_eq!(
            ctx.request_override.unwrap().inner(),
            "tok"
        );
    }
}
