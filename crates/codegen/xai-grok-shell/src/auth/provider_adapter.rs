use std::sync::Arc;

use xai_grok_provider::auth::{AuthFn, AuthInput, HeaderMap};

use crate::auth::AuthManager;

/// Wraps the shell's AuthManager as an xai-grok-provider AuthFn.
/// This allows the uniform credential chain to resolve xAI OAuth tokens.
pub struct AuthManagerAsAuthFn {
    auth_manager: Arc<AuthManager>,
}

impl std::fmt::Debug for AuthManagerAsAuthFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthManagerAsAuthFn").finish()
    }
}

impl AuthManagerAsAuthFn {
    /// Wrap an [`AuthManager`] as an [`AuthFn`] for use in the provider route chain.
    pub fn new(auth_manager: Arc<AuthManager>) -> Self {
        Self { auth_manager }
    }
}

impl AuthFn for AuthManagerAsAuthFn {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, xai_grok_provider::error::ProviderError> {
        match self.auth_manager.current_or_expired() {
            Some(auth) => {
                let mut headers = input.headers.clone();
                headers.insert("Authorization".into(), format!("Bearer {}", auth.key));
                Ok(headers)
            }
            None => Err(xai_grok_provider::error::ProviderError::Auth(
                "xAI auth: no session token available".into(),
            )),
        }
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(Self {
            auth_manager: self.auth_manager.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthManager;
    use crate::auth::model::GrokAuth;
    use parking_lot::RwLock;

    struct MockManager;

    impl MockManager {
        fn new() -> AuthManager {
            // Create AuthManager with test paths
            let home = tempfile::tempdir().unwrap();
            AuthManager::new(&home.path().join("auth.json"), Default::default())
        }
    }

    #[test]
    fn auth_manager_as_auth_fn_no_session_returns_err() {
        let manager = Arc::new(MockManager::new());
        let adapter = AuthManagerAsAuthFn::new(manager);
        let input = AuthInput::new(
            String::new(),
            "GET".into(),
            "http://localhost".into(),
            HeaderMap::new(),
        );
        let result = adapter.apply(&input);
        assert!(result.is_err());
    }
}
