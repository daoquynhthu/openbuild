use indexmap::IndexMap;
use serde::Deserialize;

/// User-supplied overrides for a provider, from config.toml [provider.*]
/// and/or CLI flags.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_default_is_empty() {
        let c = ProviderConfig::default();
        assert!(c.api_key.is_none());
        assert!(c.base_url.is_none());
    }

    #[test]
    fn provider_config_deserialize_json() {
        let json = r#"{"api_key":"sk-test","env_key":["MY_KEY"]}"#;
        let c: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.api_key.unwrap(), "sk-test");
        assert_eq!(c.env_key.unwrap(), vec!["MY_KEY"]);
    }
}
