pub use crate::types::HeaderMap;

/// Type alias for auth header maps. Prefer this over `HeaderMap` to avoid
/// confusion with `reqwest::HeaderMap`.
pub type AuthHeaderMap = HeaderMap;

use crate::error::ProviderError;

/// Declarative credential source. Provider constructors declare the source
/// but do not resolve it — resolution happens at request time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    /// Inline key from config or CLI.
    Inline,
    /// Ordered list of environment variable names to try.
    Environment(Vec<String>),
    /// xAI OAuth session token.
    Session,
    /// No auth required; allow discovery/inference without credentials.
    Public,
    /// No credential available.
    None,
}

/// Declarative authentication policy for a route.
/// Provider constructors set this; the shell runtime resolves it.
#[derive(Debug, Clone)]
pub enum AuthPolicy {
    /// No authentication.
    None,
    /// Bearer token from a credential source.
    Bearer(CredentialSource),
    /// Arbitrary header from a credential source.
    Header {
        name: String,
        source: CredentialSource,
    },
}

impl AuthPolicy {
    /// Validate header names against HTTP token rules.
    pub fn validate(&self) -> Result<(), ProviderError> {
        match self {
            AuthPolicy::Header { name, .. } => {
                if name.is_empty() || name.bytes().any(|b| b <= 32 || b > 126 || b == 58) {
                    return Err(ProviderError::InvalidHeader(format!(
                        "invalid header name: {name:?}"
                    )));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Resolved credential with the key material.
/// This struct is deliberately NOT Clone/Debug to avoid leaking secrets.
#[derive(Default)]
pub struct ResolvedCredential {
    pub value: Option<String>,
}

/// Resolve a CredentialSource at runtime.
/// Returns an error if required credentials are missing.
pub fn resolve_credential_source(
    source: &CredentialSource,
) -> Result<ResolvedCredential, ProviderError> {
    match source {
        CredentialSource::Inline => Ok(ResolvedCredential { value: None }),
        CredentialSource::Environment(keys) => {
            for key in keys {
                if let Ok(val) = std::env::var(key)
                    && !val.is_empty()
                {
                    return Ok(ResolvedCredential { value: Some(val) });
                }
            }
            Ok(ResolvedCredential { value: None })
        }
        CredentialSource::Session => match std::env::var("XAI_SESSION_TOKEN") {
            Ok(val) if !val.is_empty() => Ok(ResolvedCredential { value: Some(val) }),
            _ => Ok(ResolvedCredential { value: None }),
        },
        CredentialSource::Public => Ok(ResolvedCredential { value: None }),
        CredentialSource::None => Ok(ResolvedCredential { value: None }),
    }
}

/// Apply an AuthPolicy to produce headers at request time.
pub fn apply_auth_policy(
    policy: &AuthPolicy,
    existing: &HeaderMap,
) -> Result<HeaderMap, ProviderError> {
    match policy {
        AuthPolicy::None => Ok(existing.clone()),
        AuthPolicy::Bearer(source) => {
            let cred = resolve_credential_source(source)?;
            match cred.value {
                Some(token) => {
                    let mut headers = existing.clone();
                    headers.insert("Authorization".into(), format!("Bearer {token}"));
                    Ok(headers)
                }
                None => Err(ProviderError::MissingCredential(
                    "Bearer credential not resolved".into(),
                )),
            }
        }
        AuthPolicy::Header { name, source } => {
            let cred = resolve_credential_source(source)?;
            match cred.value {
                Some(value) => {
                    let mut headers = existing.clone();
                    headers.insert(name.clone(), value);
                    Ok(headers)
                }
                None => Err(ProviderError::MissingCredential(format!(
                    "header credential for {name} not resolved"
                ))),
            }
        }
    }
}

/// Input to an [`AuthFn::apply`] call. Carries request metadata and
/// existing headers that the auth function may augment.
#[non_exhaustive]
pub struct AuthInput {
    pub body: String,
    pub method: String,
    pub url: String,
    pub headers: HeaderMap,
}

impl AuthInput {
    pub fn new(body: String, method: String, url: String, headers: HeaderMap) -> Self {
        Self {
            body,
            method,
            url,
            headers,
        }
    }
}

pub trait AuthFn: Send + Sync + core::fmt::Debug {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError>;
    fn clone_box(&self) -> Box<dyn AuthFn>;
}

impl dyn AuthFn {
    pub fn or_else(self: Box<Self>, that: Box<dyn AuthFn>) -> Box<dyn AuthFn> {
        Box::new(ChainAuth(self, that))
    }

    pub fn and_then(self: Box<Self>, that: Box<dyn AuthFn>) -> Box<dyn AuthFn> {
        Box::new(ThenAuth(self, that))
    }
}

#[derive(Debug)]
struct ChainAuth(Box<dyn AuthFn>, Box<dyn AuthFn>);

impl AuthFn for ChainAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        self.0.apply(input).or_else(|_| self.1.apply(input))
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(ChainAuth(self.0.clone_box(), self.1.clone_box()))
    }
}

#[derive(Debug)]
struct ThenAuth(Box<dyn AuthFn>, Box<dyn AuthFn>);

impl AuthFn for ThenAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        let headers = self.0.apply(input)?;
        let chained_input = AuthInput {
            headers,
            body: input.body.clone(),
            method: input.method.clone(),
            url: input.url.clone(),
        };
        self.1.apply(&chained_input)
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(ThenAuth(self.0.clone_box(), self.1.clone_box()))
    }
}

/// Auth function that sets an `Authorization: Bearer <token>` header.
#[derive(Debug)]
pub struct BearerAuth(pub String);

impl AuthFn for BearerAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        let mut headers = input.headers.clone();
        headers.insert("Authorization".into(), format!("Bearer {}", self.0));
        Ok(headers)
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(Self(self.0.clone()))
    }
}

/// Auth function that sets an arbitrary HTTP header.
#[derive(Debug)]
pub struct HeaderAuth {
    pub name: String,
    pub value: String,
}

impl AuthFn for HeaderAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        let mut headers = input.headers.clone();
        headers.insert(self.name.clone(), self.value.clone());
        Ok(headers)
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(Self {
            name: self.name.clone(),
            value: self.value.clone(),
        })
    }
}

/// Auth function that passes through existing headers unchanged.
#[derive(Debug)]
pub struct NoopAuth;

impl AuthFn for NoopAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        Ok(input.headers.clone())
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(NoopAuth)
    }
}

/// Auth function that always returns an error.
#[derive(Debug)]
pub struct FailAuth(pub String);

impl AuthFn for FailAuth {
    fn apply(&self, _input: &AuthInput) -> Result<HeaderMap, ProviderError> {
        Err(ProviderError::NoCredential(self.0.clone()))
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(Self(self.0.clone()))
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Credential {
    Inline(Option<String>),
    Config(String),
    Session,
    /// Always succeeds with a sentinel value (e.g. "public" for free tiers).
    PublicKey(String),
    None,
}

impl Credential {
    pub fn optional(key: Option<String>, _source: &str) -> Self {
        Credential::Inline(key)
    }

    pub fn config(name: impl Into<String>) -> Self {
        Credential::Config(name.into())
    }

    pub fn session() -> Self {
        Credential::Session
    }

    pub fn public_key(value: &str) -> Self {
        Credential::PublicKey(value.to_owned())
    }

    pub fn or_else(self, other: Credential) -> Credential {
        if let Some(val) = self.resolve() {
            Credential::Inline(Some(val))
        } else {
            other
        }
    }

    pub fn bearer(self) -> Box<dyn AuthFn> {
        match self {
            Credential::None | Credential::PublicKey(_) => Box::new(NoopAuth),
            _ => match self.resolve() {
                Some(key) => Box::new(BearerAuth(key)),
                None => Box::new(FailAuth("no credential resolved".into())),
            },
        }
    }

    pub fn header(self, name: &str) -> Box<dyn AuthFn> {
        let n = name.to_owned();
        match self {
            Credential::None | Credential::PublicKey(_) => Box::new(NoopAuth),
            _ => match self.resolve() {
                Some(value) => Box::new(HeaderAuth { name: n, value }),
                None => Box::new(FailAuth("no credential resolved".into())),
            },
        }
    }

    fn resolve(&self) -> Option<String> {
        match self {
            Credential::Inline(Some(key)) if !key.is_empty() => Some(key.clone()),
            Credential::Config(name) => std::env::var(name).ok().filter(|v| !v.is_empty()),
            Credential::Session => std::env::var("XAI_SESSION_TOKEN").ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_input() -> AuthInput {
        AuthInput {
            body: String::new(),
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        }
    }

    #[test]
    fn auth_policy_none_produces_no_headers() {
        let headers = apply_auth_policy(&AuthPolicy::None, &HeaderMap::new()).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn auth_policy_public_no_header() {
        let policy = AuthPolicy::Bearer(CredentialSource::Public);
        let result = apply_auth_policy(&policy, &HeaderMap::new());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::MissingCredential(_)
        ));
    }

    #[test]
    fn auth_policy_validate_header_name() {
        let valid = AuthPolicy::Header {
            name: "x-api-key".into(),
            source: CredentialSource::None,
        };
        assert!(valid.validate().is_ok());

        let invalid = AuthPolicy::Header {
            name: "".into(),
            source: CredentialSource::None,
        };
        assert!(invalid.validate().is_err());

        let with_colon = AuthPolicy::Header {
            name: "bad:name".into(),
            source: CredentialSource::None,
        };
        assert!(with_colon.validate().is_err());
    }

    #[test]
    fn credential_source_public_vs_none() {
        assert_eq!(CredentialSource::Public, CredentialSource::Public);
        assert_ne!(CredentialSource::Public, CredentialSource::None);
    }

    #[test]
    fn credential_source_resolve_inline_none() {
        let result = resolve_credential_source(&CredentialSource::Inline).unwrap();
        assert!(result.value.is_none());
    }

    #[test]
    fn bearer_auth_sets_header() {
        let auth = Credential::optional(Some("sk-test".into()), "api_key").bearer();
        let headers = auth.apply(&test_input()).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer sk-test");
    }

    #[test]
    fn header_auth_sets_custom_header() {
        let auth = Credential::optional(Some("ant-key".into()), "api_key").header("x-api-key");
        let headers = auth.apply(&test_input()).unwrap();
        assert_eq!(headers.get("x-api-key").unwrap(), "ant-key");
    }

    #[test]
    fn optional_none_falls_through_chain() {
        let auth = Credential::optional(None, "first")
            .or_else(Credential::optional(Some("fallback".into()), "second"))
            .bearer();
        let input = test_input();
        let headers = auth.apply(&input).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer fallback");
    }

    #[test]
    fn credential_or_else_uses_credential_style() {
        let auth = Credential::optional(Some("primary".into()), "p")
            .or_else(Credential::optional(Some("fallback".into()), "f"))
            .bearer();
        let headers = auth.apply(&test_input()).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer primary");
    }

    #[test]
    fn noop_auth_preserves_headers() {
        let auth = Credential::None.bearer();
        let headers = auth.apply(&test_input()).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn session_credential_resolves_env() {
        // SAFETY: test-only env mutation, single-threaded test.
        unsafe {
            std::env::set_var("XAI_SESSION_TOKEN", "sess-abc");
        }
        let auth = Credential::session().bearer();
        let headers = auth.apply(&test_input()).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer sess-abc");
        // SAFETY: test-only env cleanup.
        unsafe {
            std::env::remove_var("XAI_SESSION_TOKEN");
        }
    }

    #[test]
    fn empty_inline_key_falls_through() {
        let auth = Credential::optional(Some(String::new()), "empty")
            .bearer()
            .or_else(Credential::None.bearer());
        let headers = auth.apply(&test_input()).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn and_then_chains_auth_steps() {
        let step1 = Credential::optional(Some("key1".into()), "s1").bearer();
        let step2 = Credential::optional(Some("key2".into()), "s2").header("x-custom");
        let auth = step1.and_then(step2);
        let headers = auth.apply(&test_input()).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer key1");
        assert_eq!(headers.get("x-custom").unwrap(), "key2");
    }
}
