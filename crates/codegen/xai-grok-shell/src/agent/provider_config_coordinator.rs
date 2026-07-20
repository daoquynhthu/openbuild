use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use tokio::sync::Mutex;
use xai_grok_provider::config::ProviderConfig;
use xai_grok_provider::registry::ProviderRegistry;

use super::provider_runtime::ProviderRuntime;

/// A typed patch for a single `[provider.<id>]` section.
pub struct ProviderConfigPatch {
    pub provider_id: String,
    pub fields: IndexMap<String, toml_edit::Value>,
}

/// Apply a patch to a TOML config string using toml_edit.
/// Only the `[provider.<id>]` section is modified; all other sections,
/// comments, and ordering are preserved.
pub fn apply_toml_patch(config: &str, patch: &ProviderConfigPatch) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = config.parse()
        .map_err(|e| format!("TOML parse error: {e}"))?;

    let provider_table = doc
        .entry("provider")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| "provider entry is not a table".to_string())?;

    let target = provider_table
        .entry(&patch.provider_id)
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| format!("provider.{} is not a table", patch.provider_id))?;

    for (key, value) in &patch.fields {
        target.insert(key, toml_edit::value(value.clone()));
    }

    Ok(doc.to_string())
}

/// Frozen resolution context captured at startup: legacy migration policy
/// and CLI overrides.  Excludes env/session secret values.
pub struct ProviderResolutionContext {
    pub legacy_migration: Option<ProviderConfig>,
    pub cli_overrides: Option<ProviderConfig>,
}

/// Outcome of a config apply operation.
#[derive(Debug)]
pub enum ConfigApplyOutcome {
    Applied { new_revision: u64 },
    Unchanged,
}

/// Error during config application.
#[derive(Debug, thiserror::Error)]
pub enum ConfigApplyError {
    #[error("failed to read config file: {0}")]
    FileReadError(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("config diagnostics: {0}")]
    ResolveError(String),
    #[error("prepare failed: {0}")]
    PrepareError(String),
    #[error("commit failed: {0}")]
    CommitError(String),
    #[error("file changed since read — not overwriting")]
    FileChanged,
    #[error("failed to write: {0}")]
    WriteError(String),
}

/// Single coordination point for all provider config writes.
///
/// Every config-driven registry commit must pass through this struct
/// so that file I/O, resolution, prepare/commit, and catalog refresh
/// are serialized under the same `update_lock`.
pub struct ProviderConfigCoordinator {
    pub runtime: Arc<ProviderRuntime>,
    pub config_path: PathBuf,
    pub resolution_context: Arc<ProviderResolutionContext>,
    update_lock: Mutex<()>,
}

impl ProviderConfigCoordinator {
    pub fn new(
        runtime: Arc<ProviderRuntime>,
        config_path: PathBuf,
        resolution_context: Arc<ProviderResolutionContext>,
    ) -> Self {
        Self {
            runtime,
            config_path,
            resolution_context,
            update_lock: Mutex::new(()),
        }
    }

    /// Read the config file, parse, resolve, prepare, and commit.
    ///
    /// Does NOT write back to the file (read-only).
    pub async fn apply_external_file(&self) -> Result<ConfigApplyOutcome, ConfigApplyError> {
        let _lock = self.update_lock.lock().await;

        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|e| ConfigApplyError::FileReadError(format!("{}: {e}", self.config_path.display())))?;
        let raw_toml: toml::Value = toml::from_str(&content)
            .map_err(|e| ConfigApplyError::ParseError(format!("{}: {e}", self.config_path.display())))?;

        let parsed = xai_grok_provider::config::parse_provider_toml(&raw_toml)
            .map_err(|diags| {
                ConfigApplyError::ParseError(
                    diags.into_iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; "),
                )
            })?;

        let (resolved, diags) = xai_grok_provider::resolution::resolve_with_precedence(
            parsed,
            self.resolution_context.legacy_migration.clone(),
            self.resolution_context.cli_overrides.clone(),
        );

        if !diags.is_empty() {
            let msg = diags.into_iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; ");
            return Err(ConfigApplyError::ResolveError(msg));
        }

        let revision = self
            .runtime
            .registry
            .rebuild_from_resolved(&resolved)
            .map_err(|e| ConfigApplyError::CommitError(e.to_string()))?;

        Ok(ConfigApplyOutcome::Applied { new_revision: revision })
    }

    pub fn registry(&self) -> Arc<ProviderRegistry> {
        Arc::clone(&self.runtime.registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn coordinator_uses_same_runtime_arc() {
        let rt = Arc::new(ProviderRuntime::new());
        let config_path = PathBuf::from("/tmp/test-config.toml");
        let ctx = Arc::new(ProviderResolutionContext {
            legacy_migration: None,
            cli_overrides: None,
        });
        let coord = ProviderConfigCoordinator::new(Arc::clone(&rt), config_path, ctx);
        assert!(Arc::ptr_eq(&rt, &coord.runtime));
    }

    #[tokio::test]
    async fn apply_external_file_valid_config_increments_revision() {
        let dir = std::env::temp_dir().join(format!("coord-test-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("config.toml");
        let config_content = "[provider.xai]\nenabled = true\nkind = \"xai\"\nprofile = \"default\"\napi_key = \"sk-test\"\n\n[provider.openai]\nenabled = true\nkind = \"openai_compatible\"\nprofile = \"openai\"\nbase_url = \"https://api.openai.com/v1\"\napi_key = \"sk-openai-test\"\n";
        std::fs::write(&config_path, config_content).unwrap();

        let rt = Arc::new(ProviderRuntime::new());
        // Register at least the xai definition so rebuild has something to work with
        xai_grok_provider::providers::register_all(&rt.registry);
        rt.registry.register_factory(
            xai_grok_provider::registry::ProviderFactoryKind::OpenAiCompatible,
            Arc::new(xai_grok_provider::providers::openai_compatible_factory::OpenAiCompatibleProviderFactory),
        ).unwrap();

        let ctx = Arc::new(ProviderResolutionContext {
            legacy_migration: None,
            cli_overrides: None,
        });
        let coord = ProviderConfigCoordinator::new(Arc::clone(&rt), config_path.clone(), ctx);

        let rev_before = rt.registry.snapshot().revision;
        let result = coord.apply_external_file().await;
        assert!(result.is_ok(), "apply_external_file should succeed: {:?}", result);
        let rev_after = rt.registry.snapshot().revision;
        assert!(rev_after > rev_before, "revision should increase: {rev_after} > {rev_before}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_toml_patch_modifies_only_target_section() {
        let config = r#"
[provider.xai]
enabled = true
kind = "xai"
api_key = "old-key"

[provider.openai]
enabled = true
kind = "openai_compatible"
base_url = "https://api.openai.com/v1"
"#;
        let mut fields = IndexMap::new();
        fields.insert("api_key".into(), toml_edit::Value::from("new-key"));
        fields.insert("enabled".into(), toml_edit::Value::from(false));

        let patch = ProviderConfigPatch {
            provider_id: "xai".into(),
            fields,
        };
        let result = apply_toml_patch(config, &patch).unwrap();

        // xai section should have new values
        assert!(result.contains(r#"api_key = "new-key""#), "xai api_key should be updated");
        assert!(result.contains("enabled = false"), "xai enabled should be updated");
        // openai section should be unchanged
        assert!(result.contains(r#"kind = "openai_compatible""#), "openai section should be preserved");
        assert!(result.contains(r#"base_url = "https://api.openai.com/v1""#), "openai base_url should be preserved");
        // After patching, the document should still be valid TOML
        let parsed: toml::Value = toml::from_str(&result).unwrap();
        assert_eq!(
            parsed["provider"]["xai"]["api_key"].as_str(),
            Some("new-key")
        );
    }

    #[test]
    fn apply_toml_patch_preserves_unrelated_sections() {
        let config = r#"
[models]
default = "xai/grok-latest"

[provider.xai]
enabled = true
kind = "xai"

[ui]
theme = "dark"
"#;
        let mut fields = IndexMap::new();
        fields.insert("enabled".into(), toml_edit::Value::from(false));

        let patch = ProviderConfigPatch {
            provider_id: "xai".into(),
            fields,
        };
        let result = apply_toml_patch(config, &patch).unwrap();

        assert!(result.contains(r#"[models]"#), "models section should be preserved");
        assert!(result.contains(r#"default = "xai/grok-latest""#), "models content should be preserved");
        assert!(result.contains(r#"[ui]"#), "ui section should be preserved");
        assert!(result.contains(r#"theme = "dark""#), "ui content should be preserved");
    }

    #[tokio::test]
    async fn apply_external_file_invalid_config_keeps_old_revision() {
        let dir = std::env::temp_dir().join(format!("coord-test-invalid-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("config.toml");
        let config_content = r#"
[provider.bad]
enabled = true
kind = "nonexistent"
"#;
        std::fs::write(&config_path, config_content).unwrap();

        let rt = Arc::new(ProviderRuntime::new());
        xai_grok_provider::providers::register_all(&rt.registry);
        let ctx = Arc::new(ProviderResolutionContext {
            legacy_migration: None,
            cli_overrides: None,
        });
        let coord = ProviderConfigCoordinator::new(Arc::clone(&rt), config_path.clone(), ctx);

        let rev_before = rt.registry.snapshot().revision;
        let result = coord.apply_external_file().await;
        assert!(result.is_err(), "apply_external_file should fail for invalid config");
        let rev_after = rt.registry.snapshot().revision;
        assert_eq!(rev_after, rev_before, "revision must not change on invalid config");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
