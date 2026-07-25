use indexmap::IndexMap;

use crate::auth::CredentialCandidate;
use crate::resolution::ProviderPublicConfig;
use crate::types::{ModelListFormat, ProviderDefaults};

/// Resolve effective base URL: user-configured override, or provider default.
pub fn resolve_base_url(configured: Option<&str>, default: &str) -> String {
    configured.unwrap_or(default).to_owned()
}

/// Resolve protocol from configured value, defaulting to `chat_completions`.
pub fn resolve_protocol(configured: Option<&str>) -> String {
    configured.unwrap_or("chat_completions").to_owned()
}

/// Build credential candidates with standard precedence:
/// 1. Inline API key (from config file)
/// 2. User-configured environment variable names
/// 3. Provider-default environment variable names
pub fn build_credential_candidates(
    has_inline_key: bool,
    user_env_keys: &[String],
    default_env_keys: &[String],
) -> Vec<CredentialCandidate> {
    let mut candidates = Vec::new();
    if has_inline_key {
        candidates.push(CredentialCandidate::ProviderInline);
    }
    if !user_env_keys.is_empty() {
        candidates.push(CredentialCandidate::ProviderEnvironment(user_env_keys.to_vec()));
    }
    if !default_env_keys.is_empty() {
        candidates.push(CredentialCandidate::ProviderEnvironment(default_env_keys.to_vec()));
    }
    candidates
}

/// Merge configured extra headers on top of provider defaults.
/// User headers override defaults with the same key.
pub fn merge_extra_headers(
    public: &ProviderPublicConfig,
    defaults: &ProviderDefaults,
) -> IndexMap<String, String> {
    let mut headers = defaults.extra_headers.clone();
    for (key, value) in &public.extra_headers {
        headers.insert(key.clone(), value.clone());
    }
    headers
}

/// Resolve model list source: user-configured path/format, or provider default.
pub fn resolve_model_source(
    public: &ProviderPublicConfig,
    defaults: &ProviderDefaults,
) -> Option<(String, ModelListFormat)> {
    if let Some(path) = &public.model_list_path {
        let fmt = public.model_list_format.unwrap_or(defaults.model_list_format);
        return Some((path.clone(), fmt));
    }
    defaults.model_list_endpoint.as_ref().map(|url| (url.clone(), defaults.model_list_format))
}
