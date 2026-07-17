use std::collections::HashMap;
use url::Url;

use crate::types::LLMRequest;

#[non_exhaustive]
pub struct EndpointInput<Body> {
    pub request: LLMRequest,
    pub body: Body,
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
    pub fn render(&self, input: &EndpointInput<Body>) -> Url {
        let base = self.base_url.as_deref().unwrap_or_else(|| {
            tracing::warn!("no base_url configured, falling back to http://localhost");
            "http://localhost"
        });
        let base = base.trim_end_matches('/');
        let path = match &self.path {
            EndpointPart::Static(s) => s.clone(),
            EndpointPart::Dynamic(f) => f(input),
        };
        let mut url = Url::parse(&format!("{base}{path}")).unwrap_or_else(|_| {
            Url::parse("http://localhost/").unwrap_or_else(|_| unreachable!())
        });
        if let Some(query) = &self.query {
            for (k, v) in query {
                url.query_pairs_mut().append_pair(k, v);
            }
        }
        url
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
        let url = ep.render(&input);
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
        let url = ep.render(&input);
        assert!(url.as_str().contains("limit=10"));
    }

    #[test]
    fn endpoint_no_base_url_defaults_to_localhost() {
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
        let url = ep.render(&input);
        assert_eq!(url.as_str(), "http://localhost/test");
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
