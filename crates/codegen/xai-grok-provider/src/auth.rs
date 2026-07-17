pub use crate::types::HeaderMap;

/// Input to an AuthFn::apply call.
#[non_exhaustive]
pub struct AuthInput {
    pub request: String,
    pub body: String,
    pub method: String,
    pub url: String,
    pub headers: HeaderMap,
}

impl AuthInput {
    pub fn new(request: String, body: String, method: String, url: String, headers: HeaderMap) -> Self {
        Self { request, body, method, url, headers }
    }
}

pub trait AuthFn: Send + Sync + core::fmt::Debug + 'static {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String>;
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
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        self.0.apply(input).or_else(|_| self.1.apply(input))
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(ChainAuth(self.0.clone_box(), self.1.clone_box()))
    }
}

#[derive(Debug)]
struct ThenAuth(Box<dyn AuthFn>, Box<dyn AuthFn>);

impl AuthFn for ThenAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        let headers = self.0.apply(input)?;
        let chained_input = AuthInput {
            headers,
            request: input.request.clone(),
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

#[derive(Debug)]
struct BearerAuth(String);

impl AuthFn for BearerAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        let mut headers = input.headers.clone();
        headers.insert("Authorization".into(), format!("Bearer {}", self.0));
        Ok(headers)
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(Self(self.0.clone()))
    }
}

#[derive(Debug)]
struct HeaderAuth {
    name: String,
    value: String,
}

impl AuthFn for HeaderAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
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

#[derive(Debug)]
pub struct NoopAuth;

impl AuthFn for NoopAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        Ok(input.headers.clone())
    }

    fn clone_box(&self) -> Box<dyn AuthFn> {
        Box::new(NoopAuth)
    }
}

#[derive(Debug)]
struct FailAuth(String);

impl AuthFn for FailAuth {
    fn apply(&self, _input: &AuthInput) -> Result<HeaderMap, String> {
        Err(self.0.clone())
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

    pub fn config(name: &str) -> Self {
        Credential::Config(name.to_owned())
    }

    pub fn session() -> Self {
        Credential::Session
    }

    pub fn public_key(value: &str) -> Self {
        Credential::PublicKey(value.to_owned())
    }

    pub fn or_else(self, other: Credential) -> Credential {
        if self.resolve().is_some() {
            self
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
            request: String::new(),
            body: String::new(),
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        }
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
        // Verify the Arch §3.9 pattern: Credential::opt().or_else().bearer()
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
