use xai_grok_provider::auth::{
    CredentialCandidate, CredentialError, EnvironmentReader, RequestCredentialContext, SecretValue,
    SessionCredentialResolver,
};

struct TestEnvReader(Result<Option<SecretValue>, String>);

impl EnvironmentReader for TestEnvReader {
    fn read(&self, _var: &str) -> Result<Option<SecretValue>, CredentialError> {
        match &self.0 {
            Ok(v) => Ok(v.clone()),
            Err(msg) => Err(CredentialError::Read(msg.clone())),
        }
    }
}

struct TestSession(Result<Option<SecretValue>, String>);

impl SessionCredentialResolver for TestSession {
    fn resolve(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<SecretValue>, CredentialError>> + Send>,
    > {
        let result = match &self.0 {
            Ok(v) => Ok(v.clone()),
            Err(msg) => Err(CredentialError::Read(msg.clone())),
        };
        Box::pin(async move { result })
    }
}

fn ctx<'a>(
    env: &'a dyn EnvironmentReader,
    session: &'a dyn SessionCredentialResolver,
) -> RequestCredentialContext<'a> {
    RequestCredentialContext::new(None, None, None, env, session)
}

#[tokio::test]
async fn absent_credential_returns_none() {
    let env = TestEnvReader(Ok(None));
    let session = TestSession(Ok(None));
    let c = ctx(&env, &session);
    let result = c
        .resolve_candidates(&[CredentialCandidate::ProviderEnvironment(vec![
            "MY_KEY".into(),
        ])])
        .await;
    assert!(result.is_none(), "absent credential should return None");
}

#[tokio::test]
async fn backend_not_found_is_silently_dropped() {
    let env = TestEnvReader(Err("not found".into()));
    let session = TestSession(Ok(None));
    let c = ctx(&env, &session);
    let result = c
        .resolve_candidates(&[CredentialCandidate::ProviderEnvironment(vec![
            "MY_KEY".into(),
        ])])
        .await;
    // BUG: `resolve_candidates` uses `if let Ok(Some(v)) = ...` so Err is silently skipped.
    // After fixing, errors should propagate instead of being dropped.
    assert!(
        result.is_none(),
        "R3-RED-09: 'not found' error is silently dropped instead of propagated"
    );
}

#[tokio::test]
async fn not_found_and_session_expired_have_same_variant() {
    let not_found = CredentialError::Read("not found".into());
    let expired = CredentialError::Read("session expired".into());
    assert_eq!(
        std::mem::discriminant(&not_found),
        std::mem::discriminant(&expired),
        "R3-RED-09: both errors are the same variant — no typed distinction"
    );
}

#[tokio::test]
async fn session_backend_error_is_silently_dropped() {
    let env = TestEnvReader(Ok(None));
    let session = TestSession(Err("session backend I/O failure".into()));
    let c = ctx(&env, &session);
    let result = c
        .resolve_candidates(&[CredentialCandidate::Session(
            xai_grok_provider::auth::SessionKind::Xai,
        )])
        .await;
    assert!(
        result.is_none(),
        "R3-RED-09: session backend error is silently dropped"
    );
}

#[tokio::test]
async fn malformed_credential_error_is_silently_dropped() {
    let env = TestEnvReader(Err("malformed credential: invalid bytes".into()));
    let session = TestSession(Ok(None));
    let c = ctx(&env, &session);
    let result = c
        .resolve_candidates(&[CredentialCandidate::ProviderEnvironment(vec![
            "BAD_KEY".into(),
        ])])
        .await;
    assert!(
        result.is_none(),
        "R3-RED-09: malformed credential error is silently dropped"
    );
}
