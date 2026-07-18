use std::fmt;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    EmptyProviderId,
    InvalidProviderId(String),
    UnsupportedScheme,
    MissingHost,
    UrlParseError(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProviderId => write!(f, "provider ID cannot be empty"),
            Self::InvalidProviderId(id) => {
                write!(
                    f,
                    "provider ID must contain only letters, numbers, underscores, and hyphens, got `{id}`"
                )
            }
            Self::UnsupportedScheme => write!(f, "base URL must use http or https scheme"),
            Self::MissingHost => write!(f, "base URL must have a host"),
            Self::UrlParseError(msg) => write!(f, "invalid base URL: {msg}"),
        }
    }
}

pub fn validate_provider_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::EmptyProviderId);
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ValidationError::InvalidProviderId(id.to_owned()));
    }
    Ok(())
}

pub fn validate_base_url(url: &str) -> Result<(), ValidationError> {
    if url.is_empty() {
        return Ok(());
    }
    let parsed = Url::parse(url).map_err(|e| ValidationError::UrlParseError(e.to_string()))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(ValidationError::UnsupportedScheme);
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        return Err(ValidationError::MissingHost);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_provider_id_rejects_empty() {
        assert_eq!(
            validate_provider_id("").unwrap_err(),
            ValidationError::EmptyProviderId
        );
    }

    #[test]
    fn validate_provider_id_rejects_special_chars() {
        assert!(matches!(
            validate_provider_id("foo/bar").unwrap_err(),
            ValidationError::InvalidProviderId(_)
        ));
    }

    #[test]
    fn validate_provider_id_accepts_alphanumeric() {
        assert!(validate_provider_id("my-provider_42").is_ok());
    }

    #[test]
    fn validate_base_url_accepts_empty() {
        assert!(validate_base_url("").is_ok());
    }

    #[test]
    fn validate_base_url_accepts_https() {
        assert!(validate_base_url("https://api.x.ai/v1").is_ok());
    }

    #[test]
    fn validate_base_url_rejects_ftp() {
        assert_eq!(
            validate_base_url("ftp://bad.com").unwrap_err(),
            ValidationError::UnsupportedScheme
        );
    }

    #[test]
    fn validate_base_url_rejects_missing_host() {
        assert!(matches!(
            validate_base_url("https://").unwrap_err(),
            ValidationError::UrlParseError(_)
        ));
    }

    #[test]
    fn validate_base_url_rejects_garbage() {
        assert!(matches!(
            validate_base_url("not a url").unwrap_err(),
            ValidationError::UrlParseError(_)
        ));
    }

    #[test]
    fn display_empty_provider_id() {
        let msg = ValidationError::EmptyProviderId.to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn display_invalid_provider_id() {
        let msg = ValidationError::InvalidProviderId("bad/id".into()).to_string();
        assert!(msg.contains("bad/id"));
    }

    #[test]
    fn display_unsupported_scheme() {
        let msg = ValidationError::UnsupportedScheme.to_string();
        assert!(!msg.is_empty());
    }
}
