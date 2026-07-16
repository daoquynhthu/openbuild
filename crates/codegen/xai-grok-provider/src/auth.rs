use std::collections::HashMap;

pub type HeaderMap = HashMap<String, String>;

#[derive(Debug)]
pub struct AuthInput {
    pub method: String,
    pub url: String,
    pub headers: HeaderMap,
}

pub trait AuthFn: Send + Sync + core::fmt::Debug {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String>;
}

impl dyn AuthFn {
    pub fn or_else(self: Box<Self>, that: Box<dyn AuthFn>) -> Box<dyn AuthFn> {
        Box::new(ChainAuth(self, that))
    }
}

#[derive(Debug)]
struct ChainAuth(Box<dyn AuthFn>, Box<dyn AuthFn>);

impl AuthFn for ChainAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        self.0.apply(input).or_else(|_| self.1.apply(input))
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
}

#[derive(Debug)]
struct NoopAuth;

impl AuthFn for NoopAuth {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap, String> {
        Ok(input.headers.clone())
    }
}

#[derive(Debug)]
pub enum Credential {
    Inline(Option<String>),
    Config(String),
    None,
}

impl Credential {
    pub fn optional(key: Option<String>, _source: &str) -> Self {
        Credential::Inline(key)
    }

    pub fn config(name: &str) -> Self {
        Credential::Config(name.to_owned())
    }

    pub fn bearer(self) -> Box<dyn AuthFn> {
        match self {
            Credential::None => Box::new(NoopAuth),
            _ => match self.resolve() {
                Some(key) => Box::new(BearerAuth(key)),
                None => Box::new(FailAuth("no credential resolved".into())),
            },
        }
    }

    pub fn header(self, name: &str) -> Box<dyn AuthFn> {
        let n = name.to_owned();
        match self {
            Credential::None => Box::new(NoopAuth),
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
            _ => None,
        }
    }
}

#[derive(Debug)]
struct FailAuth(String);

impl AuthFn for FailAuth {
    fn apply(&self, _input: &AuthInput) -> Result<HeaderMap, String> {
        Err(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_auth_sets_header() {
        let auth = Credential::optional(Some("sk-test".into()), "api_key").bearer();
        let input = AuthInput {
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer sk-test");
    }

    #[test]
    fn header_auth_sets_custom_header() {
        let auth = Credential::optional(Some("ant-key".into()), "api_key")
            .header("x-api-key");
        let input = AuthInput {
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert_eq!(headers.get("x-api-key").unwrap(), "ant-key");
    }

    #[test]
    fn chain_auth_falls_through() {
        let auth = Credential::optional(None, "first")
            .bearer()
            .or_else(
                Credential::optional(Some("fallback".into()), "second").bearer(),
            );
        let input = AuthInput {
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer fallback");
    }

    #[test]
    fn noop_auth_preserves_headers() {
        let auth = Credential::None.bearer();
        let input = AuthInput {
            method: "GET".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn optional_none_falls_through_chain() {
        let auth = Credential::optional(None, "first")
            .bearer()
            .or_else(Credential::optional(Some("fallback".into()), "second").bearer());
        let input = AuthInput {
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer fallback");
    }

    #[test]
    fn empty_inline_key_falls_through() {
        // Inline("") should be treated as None
        let auth = Credential::optional(Some(String::new()), "empty")
            .bearer()
            .or_else(Credential::None.bearer());
        let input = AuthInput {
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HeaderMap::new(),
        };
        let headers = auth.apply(&input).unwrap();
        assert!(headers.is_empty());
    }
}
