//! Header merge and `SensitiveHeaderMap` for P8-007.
//!
//! Single point for merging headers from multiple sources,
//! producing a `SensitiveHeaderMap` with redacted Debug/Display.

use crate::error::ProviderError;

/// Typed wrapper for request header overrides passed to `prepare_sampler_config`.
///
/// Provides HTTP-level validation at construction time. Plan §line 1489.
#[derive(Debug, Default)]
pub struct RequestHeaderOverrides(http::HeaderMap);

impl RequestHeaderOverrides {
    pub fn new() -> Self {
        Self(http::HeaderMap::new())
    }

    pub fn from_slice(pairs: &[(&str, &str)]) -> Result<Self, ProviderError> {
        let mut map = http::HeaderMap::new();
        for (name, value) in pairs {
            let n = http::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ProviderError::InvalidHeader(name.to_string()))?;
            let v = http::HeaderValue::from_str(value)
                .map_err(|_| ProviderError::InvalidHeader(value.to_string()))?;
            map.insert(n, v);
        }
        Ok(Self(map))
    }

    pub fn inner(&self) -> &http::HeaderMap {
        &self.0
    }
}

/// Header map that redacts values in Debug/Display.
/// Does not implement Serialize/Deserialize.
#[derive(Clone)]
pub struct SensitiveHeaderMap(http::HeaderMap);

impl SensitiveHeaderMap {
    pub fn new(headers: http::HeaderMap) -> Self {
        Self(headers)
    }

    /// Convenience conversion from `IndexMap<String, String>`.
    /// Converts each entry into an HTTP header, silently skipping invalid entries.
    pub fn from_index_map(map: &indexmap::IndexMap<String, String>) -> Self {
        let mut headers = http::HeaderMap::new();
        for (name, value) in map {
            if let (Ok(n), Ok(v)) = (
                http::HeaderName::from_bytes(name.as_bytes()),
                http::HeaderValue::from_str(value),
            ) {
                headers.insert(n, v);
            }
        }
        Self(headers)
    }

    pub fn into_inner(self) -> http::HeaderMap {
        self.0
    }

    pub fn inner(&self) -> &http::HeaderMap {
        &self.0
    }
}

impl std::fmt::Debug for SensitiveHeaderMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.0.iter().map(|(name, _)| (name.as_str(), "[REDACTED]")))
            .finish()
    }
}

impl std::fmt::Display for SensitiveHeaderMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, _) in self.0.iter() {
            writeln!(f, "{}: [REDACTED]", name.as_str())?;
        }
        Ok(())
    }
}

/// Validate a header value against HTTP rules:
/// - No control characters (bytes 0-31, except tab at 9)
/// - No newlines (LF=10, CR=13)
fn validate_header_value(value: &str) -> Result<(), ProviderError> {
    if value.bytes().any(|b| b <= 31 || b == 127) {
        return Err(ProviderError::InvalidHeader(
            "header value contains control characters".into(),
        ));
    }
    Ok(())
}

/// Validate a header name is a valid HTTP token.
fn validate_header_name(name: &str) -> Result<(), ProviderError> {
    if name.is_empty() || name.bytes().any(|b| b <= 32 || b > 126 || b == 58) {
        return Err(ProviderError::InvalidHeader(format!(
            "invalid header name: {name:?}"
        )));
    }
    Ok(())
}

/// Merge headers from multiple sources into a `SensitiveHeaderMap`.
///
/// Order: transport-required > route static > provider extra > auth > request override.
/// Conflict rule: same key, different value → `HeaderConflict`.
pub fn merge_headers(
    transport_required: &[(String, String)],
    route_static: &indexmap::IndexMap<String, String>,
    provider_extra: &indexmap::IndexMap<String, String>,
    auth_header: Option<(&str, &str)>,
    request_overrides: &http::HeaderMap,
) -> Result<SensitiveHeaderMap, ProviderError> {
    let mut merged = http::HeaderMap::new();

    // Layer 1: transport-required headers
    for (name, value) in transport_required {
        validate_header_name(name)?;
        validate_header_value(value)?;
        let n = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ProviderError::InvalidHeader(name.clone()))?;
        let v = http::HeaderValue::from_str(value)
            .map_err(|_| ProviderError::InvalidHeader(value.clone()))?;
        merged.insert(n, v);
    }

    // Helper: insert or check conflict
    let mut insert_or_conflict = |name: &str, value: &str| -> Result<(), ProviderError> {
        validate_header_name(name)?;
        validate_header_value(value)?;
        let n = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ProviderError::InvalidHeader(name.to_string()))?;
        let v = http::HeaderValue::from_str(value)
            .map_err(|_| ProviderError::InvalidHeader(value.to_string()))?;
        if let Some(existing) = merged.get(&n) {
            if existing != v {
                return Err(ProviderError::HeaderConflict(format!(
                    "`{name}`: existing value differs from new value"
                )));
            }
        } else {
            merged.insert(n, v);
        }
        Ok(())
    };

    // Layer 2: route static headers
    for (name, value) in route_static {
        insert_or_conflict(name, value)?;
    }

    // Layer 3: provider extra headers
    for (name, value) in provider_extra {
        insert_or_conflict(name, value)?;
    }

    // Layer 4: auth header
    if let Some((name, value)) = auth_header {
        insert_or_conflict(name, value)?;
    }

    // Layer 5: request overrides (highest priority — override without conflict check)
    for (name, value) in request_overrides.iter() {
        let value_str = value
            .to_str()
            .map_err(|_| ProviderError::InvalidHeader("non-utf8 header value".into()))?;
        validate_header_name(name.as_str())?;
        validate_header_value(value_str)?;
        merged.insert(name.clone(), value.clone());
    }

    Ok(SensitiveHeaderMap(merged))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn sensitive_header_map_redacts_values() {
        let mut hm = http::HeaderMap::new();
        hm.insert("authorization", "Bearer sk-secret".parse().unwrap());
        let shm = SensitiveHeaderMap::new(hm);
        let debug = format!("{shm:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn merge_simple_headers() {
        let result = merge_headers(
            &[],
            &IndexMap::new(),
            &IndexMap::new(),
            None,
            &http::HeaderMap::new(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn merge_route_static_headers() {
        let mut static_headers = IndexMap::new();
        static_headers.insert("x-custom".into(), "value1".into());
        let result = merge_headers(
            &[],
            &static_headers,
            &IndexMap::new(),
            None,
            &http::HeaderMap::new(),
        );
        let merged = result.unwrap();
        assert_eq!(merged.inner().get("x-custom").unwrap(), "value1");
    }

    #[test]
    fn merge_duplicate_same_value_ok() {
        let mut static_headers = IndexMap::new();
        static_headers.insert("x-id".into(), "abc".into());
        let mut extra = IndexMap::new();
        extra.insert("x-id".into(), "abc".into());
        let result = merge_headers(&[], &static_headers, &extra, None, &http::HeaderMap::new());
        assert!(result.is_ok(), "same value dedup should be ok");
    }

    #[test]
    fn merge_conflict_different_value_errors() {
        let mut static_headers = IndexMap::new();
        static_headers.insert("x-id".into(), "abc".into());
        let mut extra = IndexMap::new();
        extra.insert("x-id".into(), "def".into());
        let result = merge_headers(&[], &static_headers, &extra, None, &http::HeaderMap::new());
        assert!(result.is_err(), "conflicting values must error");
        assert!(result.unwrap_err().to_string().contains("conflict"));
    }

    #[test]
    fn reject_control_characters_in_value() {
        assert!(validate_header_value("hello\nworld").is_err());
        assert!(validate_header_value("hello\rworld").is_err());
        assert!(validate_header_value("normal-value").is_ok());
    }

    #[test]
    fn auth_header_merges_correctly() {
        let result = merge_headers(
            &[],
            &IndexMap::new(),
            &IndexMap::new(),
            Some(("authorization", "Bearer tok")),
            &http::HeaderMap::new(),
        );
        let merged = result.unwrap();
        assert_eq!(merged.inner().get("authorization").unwrap(), "Bearer tok");
    }

    #[test]
    fn request_override_wins_without_conflict() {
        let mut static_headers = IndexMap::new();
        static_headers.insert("x-id".into(), "original".into());
        let mut overrides = http::HeaderMap::new();
        overrides.insert("x-id", "override".parse().unwrap());
        let result = merge_headers(&[], &static_headers, &IndexMap::new(), None, &overrides);
        assert!(result.is_ok());
        let merged = result.unwrap();
        // Request override is highest priority — replaces without conflict
        assert_eq!(merged.inner().get("x-id").unwrap(), "override");
    }
}
