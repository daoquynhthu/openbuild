use std::collections::HashMap;
use url::Url;

pub struct EndpointInput<Body> {
    pub request: (),
    pub body: Body,
}

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

pub struct Endpoint<Body> {
    pub base_url: Option<String>,
    pub path: EndpointPart<Body>,
    pub query: Option<HashMap<String, String>>,
}

impl<Body> core::fmt::Debug for Endpoint<Body> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("path", &self.path)
            .field("query", &self.query)
            .finish()
    }
}

impl<Body> Clone for Endpoint<Body>
where
    EndpointPart<Body>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            path: self.path.clone(),
            query: self.query.clone(),
        }
    }
}

impl<Body> EndpointPart<Body> where Body: Clone {}

impl<Body> Clone for EndpointPart<Body>
where
    Body: Clone,
{
    fn clone(&self) -> Self {
        match self {
            EndpointPart::Static(s) => EndpointPart::Static(s.clone()),
            EndpointPart::Dynamic(f) => EndpointPart::Dynamic(*f),
        }
    }
}

impl<Body> Endpoint<Body> {
    pub fn render(&self, _input: &EndpointInput<Body>) -> Url {
        Url::parse("http://localhost/").unwrap()
    }
}

pub fn merge_endpoints<Body>(_base: &Endpoint<Body>, _patch: &Endpoint<Body>) -> Endpoint<Body> {
    unimplemented!()
}
