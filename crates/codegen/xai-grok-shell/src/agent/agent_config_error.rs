use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentConfigError {
    #[error("provider error: {0}")]
    Provider(#[from] xai_grok_provider::error::ProviderError),

    #[error("request preparation error: {0}")]
    RequestPreparation(#[from] xai_grok_provider::prepared::RequestPreparationError),

    #[error("credential error: {0}")]
    Credential(#[from] xai_grok_provider::auth::CredentialError),

    #[error("model resolution error: {0}")]
    ModelResolution(#[from] super::provider_resolution::ProviderResolutionError),
}
