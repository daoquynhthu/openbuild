/// Type alias for a thread-safe provider factory.
/// Defined here for forward compatibility with P5 registry integration.
#[allow(dead_code)]
pub type SharedProviderFactory = std::sync::Arc<dyn ProviderFactory + Send + Sync>;

/// Trait for creating a `SharedProvider` from a resolved provider spec.
/// Concrete implementations handle one `ProviderImplementation` variant.
#[allow(dead_code)]
pub trait ProviderFactory {
    fn create(
        &self,
        spec: &crate::resolution::ResolvedProviderSpec,
    ) -> Result<crate::provider::SharedProvider, crate::error::ProviderError>;
}

/// Placeholder factory for OpenAI-compatible providers.
/// Full implementation deferred to P5 (requires Route/Endpoint/Config integration).
#[allow(dead_code)]
pub struct OpenAiCompatibleProviderFactory;

#[allow(dead_code)]
impl ProviderFactory for OpenAiCompatibleProviderFactory {
    fn create(&self, _spec: &crate::resolution::ResolvedProviderSpec) -> Result<crate::provider::SharedProvider, crate::error::ProviderError> {
        Err(crate::error::ProviderError::Config(
            "OpenAiCompatibleProviderFactory not yet implemented — deferred to P5".into(),
        ))
    }
}
