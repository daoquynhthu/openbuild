pub trait AuthFn: Send + Sync + core::fmt::Debug {
    fn apply(&self) {}
}

impl dyn AuthFn {
    pub fn or_else(self: Box<Self>, _that: Box<dyn AuthFn>) -> Box<dyn AuthFn> {
        self
    }
}

pub enum Credential {
    Inline(Option<String>),
    Config(String),
}

impl Credential {
    pub fn optional(_key: Option<String>, _source: &str) -> Self {
        Credential::Inline(None)
    }

    pub fn config(_name: &str) -> Self {
        Credential::Config(String::new())
    }

    pub fn bearer(self) -> Box<dyn AuthFn> {
        Box::new(NoopAuth)
    }

    pub fn header(self, _name: &str) -> Box<dyn AuthFn> {
        Box::new(NoopAuth)
    }
}

#[derive(Debug)]
struct NoopAuth;

impl AuthFn for NoopAuth {
    fn apply(&self) {}
}
