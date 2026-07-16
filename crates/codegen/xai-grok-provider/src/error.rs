use thiserror::Error;

/// Errors that can occur in the provider layer.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum ProviderError {
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
