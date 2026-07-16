use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}
