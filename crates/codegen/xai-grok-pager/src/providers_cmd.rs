//! `grok providers` subcommand — list configured providers and their status.

use std::sync::Arc;

use anyhow::Result;
use xai_grok_provider::registry::ProviderRegistry;

/// Load provider state from disk config and print a summary.
pub async fn list_providers() -> Result<()> {
    let toml = xai_grok_shell::config::load_effective_config_disk_only()?;

    let registry = Arc::new(ProviderRegistry::new());
    xai_grok_provider::providers::register_all(&registry);
    xai_grok_provider::providers::configure_providers(&registry, &toml, None, None);

    let snapshot = registry.snapshot();
    println!("Configured providers (rev {}):", snapshot.revision);
    println!();

    if snapshot.providers.is_empty() {
        println!("  (none configured)");
        println!();
        println!("Use --provider, --api-key, --base-url flags or edit config.toml:");
        println!("  [provider.\"openai\"]");
        println!("  env_key = [\"OPENAI_API_KEY\"]");
        println!("  base_url = \"https://api.openai.com/v1\"");
        println!();
        println!("Built-in providers: xai, openai, anthropic, opencode, ollama, openai-compatible");
        return Ok(());
    }

    for (pid, configured) in &snapshot.providers {
        let default_route = configured.routes.get(&configured.default_route_id);
        let endpoint = default_route
            .and_then(|r| r.endpoint.base_url.as_deref())
            .unwrap_or("-");
        let route_count = configured.routes.len();
        let key_status = if configured.config.api_key.is_some() {
            "api_key set"
        } else if configured
            .config
            .env_key
            .as_ref()
            .is_some_and(|e| !e.is_empty())
        {
            "env_key configured"
        } else {
            "no credentials"
        };

        println!("  {} ({}):", pid.0, configured.display_name);
        println!("    endpoint:  {endpoint}");
        println!("    routes:    {route_count}");
        println!("    auth:      {key_status}");
        println!();
    }

    Ok(())
}
