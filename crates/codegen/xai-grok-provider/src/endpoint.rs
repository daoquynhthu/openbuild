use std::collections::HashMap;
use url::Url;

use crate::error::ProviderError;
use crate::types::LLMRequest;

/// Validate endpoint URL for safety.
/// - Remote providers require https.
/// - Local/loopback hosts may use http.
/// - Reject embedded credentials, fragments, unsupported schemes, empty host.
fn validate_endpoint_url(url: &Url) -> Result<(), ProviderError> {
    let host = url
        .host_str()
        .ok_or_else(|| ProviderError::InvalidEndpoint("endpoint URL has no host".into()))?;

    if url.fragment().is_some() {
        return Err(ProviderError::InvalidEndpoint(
            "endpoint URL must not contain a fragment".into(),
        ));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(ProviderError::InvalidEndpoint(
            "endpoint URL must not contain embedded credentials".into(),
        ));
    }

    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            if host == "localhost"
                || host == "127.0.0.1"
                || host == "::1"
                || host.starts_with("127.")
            {
                Ok(())
            } else {
                Err(ProviderError::InvalidEndpoint(format!(
                    "remote endpoint {host} requires https, got http"
                )))
            }
        }
        scheme => Err(ProviderError::InvalidEndpoint(format!(
            "unsupported URL scheme {scheme:?} in endpoint"
        ))),
    }
}

#[non_exhaustive]
pub struct EndpointInput<Body> {
    pub request: LLMRequest,
    pub body: Body,
}

impl<Body> EndpointInput<Body> {
    pub fn new(request: LLMRequest, body: Body) -> Self {
        Self { request, body }
    }
}

/// A path segment in an API endpoint URL.
/// Either a static string or a dynamic function that takes request input.
#[derive(Clone)]
#[non_exhaustive]
pub enum EndpointPart<Body> {
    Static(String),
    Dynamic(fn(&EndpointInput<Body>) -> String),
}

impl<Body> core::fmt::Debug for EndpointPart<Body> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EndpointPart::Static(s) => write!(f, "Static({s})"),
            EndpointPart::Dynamic(_) => write!(f, "Dynamic(<fn>)"),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Endpoint<Body> {
    pub base_url: Option<String>,
    pub path: EndpointPart<Body>,
    pub query: Option<HashMap<String, String>>,
}

/// Partial endpoint overrides for route patching.
/// All fields are optional — absent fields inherit from the base endpoint.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EndpointPatch<Body> {
    pub base_url: Option<String>,
    pub path: Option<EndpointPart<Body>>,
    pub query: Option<HashMap<String, String>>,
}

impl<Body> Endpoint<Body> {
    /// Return the default path string (Static variant) or empty string for Dynamic.
    pub fn path_for_default(&self) -> String {
        match &self.path {
            EndpointPart::Static(s) => s.clone(),
            EndpointPart::Dynamic(_) => String::new(),
        }
    }

    /// Render the endpoint into a full URL.
    ///
    /// Returns `ProviderError::InvalidEndpoint` if the base URL is missing,
    /// malformed, contains credentials/fragments, uses an unsupported scheme,
    /// or is a remote HTTP endpoint.
    pub fn render(&self, input: &EndpointInput<Body>) -> Result<Url, ProviderError> {
        let base_str = self
            .base_url
            .as_deref()
            .ok_or_else(|| ProviderError::InvalidEndpoint("no base_url configured".into()))?;
        let base = base_str.trim_end_matches('/');
        let path = match &self.path {
            EndpointPart::Static(s) => s.clone(),
            EndpointPart::Dynamic(f) => f(input),
        };
        let path = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        let url_str = format!("{base}{path}");
        let url = Url::parse(&url_str).map_err(|e| {
            ProviderError::InvalidEndpoint(format!("malformed endpoint URL {url_str}: {e}"))
        })?;

        validate_endpoint_url(&url)?;

        let mut url = url;
        if let Some(query) = &self.query {
            for (k, v) in query {
                url.query_pairs_mut().append_pair(k, v);
            }
        }
        Ok(url)
    }
}

pub fn merge_endpoints<Body>(base: &Endpoint<Body>, patch: &EndpointPatch<Body>) -> Endpoint<Body>
where
    EndpointPart<Body>: Clone,
{
    Endpoint {
        base_url: patch.base_url.clone().or_else(|| base.base_url.clone()),
        path: patch.path.clone().unwrap_or_else(|| base.path.clone()),
        query: match (&base.query, &patch.query) {
            (Some(bq), Some(pq)) => {
                let mut merged = bq.clone();
                merged.extend(pq.iter().map(|(k, v)| (k.clone(), v.clone())));
                Some(merged)
            }
            (Some(bq), None) => Some(bq.clone()),
            (None, Some(pq)) => Some(pq.clone()),
            (None, None) => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_static_path() {
        let ep = Endpoint {
            base_url: Some("https://api.openai.com/v1".into()),
            path: EndpointPart::Static("/chat/completions".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "gpt-4o".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let url = ep.render(&input).unwrap();
        assert_eq!(url.as_str(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn endpoint_with_query() {
        let ep = Endpoint {
            base_url: Some("https://api.example.com".into()),
            path: EndpointPart::Static("/v1/models".into()),
            query: Some(HashMap::from([("limit".into(), "10".into())])),
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "gpt-4o".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let url = ep.render(&input).unwrap();
        assert!(url.as_str().contains("limit=10"));
    }

    #[test]
    fn endpoint_no_base_url_returns_error() {
        let ep = Endpoint {
            base_url: None,
            path: EndpointPart::Static("/test".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let result = ep.render(&input);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::InvalidEndpoint(_)
        ));
    }

    #[test]
    fn endpoint_rejects_http_remote() {
        let ep = Endpoint {
            base_url: Some("http://api.example.com/v1".into()),
            path: EndpointPart::Static("/chat".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let result = ep.render(&input);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::InvalidEndpoint(_)
        ));
    }

    #[test]
    fn endpoint_accepts_http_localhost() {
        let ep = Endpoint {
            base_url: Some("http://localhost:11434".into()),
            path: EndpointPart::Static("/v1/chat".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let url = ep.render(&input).unwrap();
        assert!(url.as_str().starts_with("http://localhost:11434"));
    }

    #[test]
    fn endpoint_rejects_embedded_credentials() {
        let ep = Endpoint {
            base_url: Some("https://user:pass@api.example.com/v1".into()),
            path: EndpointPart::Static("/chat".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let result = ep.render(&input);
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_rejects_invalid_scheme() {
        let ep = Endpoint {
            base_url: Some("ftp://files.example.com/resource".into()),
            path: EndpointPart::Static("/chat".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let result = ep.render(&input);
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_rejects_fragment() {
        let ep = Endpoint {
            base_url: Some("https://api.example.com/v1#frag".into()),
            path: EndpointPart::Static("/chat".into()),
            query: None,
        };
        let input = EndpointInput {
            request: LLMRequest {
                model: "test".into(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
            },
            body: (),
        };
        let result = ep.render(&input);
        assert!(result.is_err());
    }

    #[test]
    fn merge_endpoints_uses_patch_base_url() {
        let base = Endpoint::<()> {
            base_url: Some("https://default.com".into()),
            path: EndpointPart::Static("/path".into()),
            query: None,
        };
        let patch = EndpointPatch {
            base_url: Some("https://override.com".into()),
            path: None,
            query: None,
        };
        let merged = merge_endpoints(&base, &patch);
        assert_eq!(merged.base_url.unwrap(), "https://override.com");
    }

    #[test]
    fn merge_endpoints_keeps_base_url_when_patch_has_none() {
        let base = Endpoint::<()> {
            base_url: Some("https://default.com".into()),
            path: EndpointPart::Static("/path".into()),
            query: None,
        };
        let patch = EndpointPatch {
            base_url: None,
            path: None,
            query: None,
        };
        let merged = merge_endpoints(&base, &patch);
        assert_eq!(merged.base_url.unwrap(), "https://default.com");
    }
}
