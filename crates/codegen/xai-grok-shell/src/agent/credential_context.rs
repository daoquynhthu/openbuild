use std::sync::Arc;

use xai_grok_provider::auth::SecretValue;

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
