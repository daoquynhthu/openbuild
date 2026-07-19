use thiserror::Error;

/// Errors that can occur in the provider layer.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("invalid provider ID: {0}")]
    InvalidProviderId(String),

    #[error("invalid route ID: {0}")]
    InvalidRouteId(String),

    #[error("duplicate provider: {0}")]
    DuplicateProvider(String),

    #[error("duplicate route: {0}")]
    DuplicateRoute(String),

    #[error("unknown provider: {0}")]
    UnknownProvider(String),

    #[error("unknown route: {0}")]
    UnknownRoute(String),

    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("missing credential: {0}")]
    MissingCredential(String),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("header conflict: {0}")]
    HeaderConflict(String),

    #[error("unknown protocol: {0}")]
    UnknownProtocol(String),

    #[error("ambiguous model reference: {0}")]
    AmbiguousModel(String),

    #[error("configuration error: {0}")]
    Config(String),

    // Legacy variants kept for backward compatibility during migration.
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("credential not resolved: {0}")]
    NoCredential(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;
