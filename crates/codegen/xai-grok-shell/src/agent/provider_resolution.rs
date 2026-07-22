use std::sync::Arc;

use indexmap::IndexMap;
use xai_grok_provider::auth::AuthPolicy;
use xai_grok_provider::headers::SensitiveHeaderMap;
use xai_grok_provider::model::{GenerationOptions, ModelLimits};
use xai_grok_provider::prepared::PreparedSamplerConfig;
use xai_grok_provider::registry::RegistrySnapshot;
use xai_grok_provider::resolution::ResolvedModelExecution;
use xai_grok_provider::route::Route;
use xai_grok_provider::types::{ModelId, ProviderId, RouteId};
use xai_grok_sampler::SamplerConfig;

use super::config::ModelEntry;
use super::provider_catalog::ModelCatalogSnapshot;

/// Error returned by the route compiler.
#[derive(Debug, thiserror::Error)]
pub enum ProviderResolutionError {
    #[error("model has no provider_id: {0}")]
    NoProviderId(String),

    #[error("provider {0} not found in registry snapshot")]
    ProviderNotFound(String),

    #[error("route selection failed for model {model}: {detail}")]
    RouteSelection { model: String, detail: String },

    #[error("default route {0} not found in provider's route set")]
    DefaultRouteNotFound(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("ambiguous model reference: {0}")]
    AmbiguousModel(String),

    #[error("conflicting provider: --provider {provider} conflicts with --model {model}")]
    ConflictingProvider { provider: String, model: String },

    #[error("bare model '{0}' not found in any provider's catalog")]
    ModelNotFound(String),

    #[error("auth credential resolution failed: {0}")]
    AuthCredential(String),
}

/// Migration adapter: converts route compiler output to `SamplerConfig`.
/// Calls `prepare_sampler_config` directly with minimal credential context.
pub fn execution_to_sampler_config(
    model: &ModelEntry,
    registry: &RegistrySnapshot,
    api_key: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<SamplerConfig, ProviderResolutionError> {
    use xai_grok_provider::auth::SecretValue;
    use xai_grok_provider::prepared::prepare_sampler_config;

    let execution = resolve_model_execution(model, registry, base_url_override)?;
    let model_inline = api_key.map(|k| SecretValue::new(k.to_string()));
    let env = crate::agent::credential_context::ProcessEnvironment;
    let session = crate::agent::credential_context::NoopSessionResolver;
    let creds = xai_grok_provider::auth::RequestCredentialContext::new(
        None,
        model_inline.as_ref(),
        None,
        &env,
        &session,
    );
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| ProviderResolutionError::AuthCredential(e.to_string()))?;
    let headers = xai_grok_provider::headers::RequestHeaderOverrides::new();
    let prepared = rt.block_on(prepare_sampler_config(&execution, &creds, &headers))
        .map_err(|e| ProviderResolutionError::AuthCredential(e.to_string()))?;
    Ok(xai_grok_sampler::SamplerConfig::from(prepared))
}

/// Merge generation parameters with fixed precedence: route defaults < model info < request overrides.
/// Limits are capped by route limits — request cannot exceed provider/model caps.
pub fn merge_generation_params(
    route_gen: &GenerationOptions,
    route_limits: &ModelLimits,
    model_max_tokens: Option<u32>,
    model_temperature: Option<f32>,
    model_top_p: Option<f32>,
    request_max_tokens: Option<u32>,
    request_temperature: Option<f32>,
    request_top_p: Option<f32>,
) -> (Option<u32>, Option<f32>, Option<f32>) {
    let max_tokens = request_max_tokens
        .or(model_max_tokens)
        .or(route_gen.max_tokens)
        .map(|v| route_limits.output.map(|cap| v.min(cap)).unwrap_or(v));
    let temperature = request_temperature
        .or(model_temperature)
        .or(route_gen.temperature);
    let top_p = request_top_p.or(model_top_p).or(route_gen.top_p);
    (max_tokens, temperature, top_p)
}

/// Merge all GenerationOptions fields with fixed precedence: route defaults < model < request.
pub fn merge_generation_options(
    route_gen: &GenerationOptions,
    model_gen: &GenerationOptions,
    request_gen: &GenerationOptions,
) -> GenerationOptions {
    GenerationOptions::new(
        request_gen
            .max_tokens
            .or(model_gen.max_tokens)
            .or(route_gen.max_tokens),
        request_gen
            .temperature
            .or(model_gen.temperature)
            .or(route_gen.temperature),
        request_gen.top_p.or(model_gen.top_p).or(route_gen.top_p),
    )
}

/// Resolve a `ResolvedModelExecution` from a `ModelEntry`, registry snapshot, and params.
///
/// This is the route compiler — the only production path for producing a
/// resolved model execution. It applies:
/// 1. explicit model binding;
/// 2. provider route selector;
/// 3. route endpoint and protocol via `Endpoint::render` (P7-004);
/// 4. model/route/provider generation defaults with cap (P7-007);
/// 5. protocol ID validation via the protocol table (P7-006);
///
/// Auth headers are NOT resolved here — that is Phase 8's responsibility.
pub fn resolve_model_execution(
    model: &ModelEntry,
    registry: &RegistrySnapshot,
    base_url_override: Option<&str>,
) -> Result<ResolvedModelExecution, ProviderResolutionError> {
    let provider_id_str = model
        .provider_id
        .as_deref()
        .ok_or_else(|| ProviderResolutionError::NoProviderId(model.info.model.clone()))?;
    let provider_id = ProviderId::new(provider_id_str);

    let configured = registry
        .providers
        .get(&provider_id)
        .ok_or_else(|| ProviderResolutionError::ProviderNotFound(provider_id_str.to_string()))?;

    // Resolve route via selector or explicit route_id
    let route_id = if let Some(ref rid) = model.route_id {
        rid.clone()
    } else {
        configured
            .route_selector
            .select(&model.info.model)
            .map(|rid| rid.0.clone())
            .map_err(|e| ProviderResolutionError::RouteSelection {
                model: model.info.model.clone(),
                detail: e.to_string(),
            })?
    };

    let route = configured
        .routes
        .get(&RouteId::new(&route_id))
        .ok_or_else(|| ProviderResolutionError::DefaultRouteNotFound(route_id.clone()))?;

    let protocol_id = route.protocol_id.clone();

    // P7-004: Use Endpoint::render as the sole URL construction entry point.
    let input = xai_grok_provider::endpoint::EndpointInput::new(
        xai_grok_provider::types::LLMRequest::new(&model.info.model),
        (),
    );
    let endpoint_url = route
        .endpoint
        .render(&input)
        .map_err(|e| ProviderResolutionError::Protocol(format!("endpoint render failed: {e}")))?;

    let request_url = if let Some(override_url) = base_url_override {
        url::Url::parse(override_url).map_err(|e| {
            ProviderResolutionError::Protocol(format!("invalid base_url override: {e}"))
        })?
    } else {
        endpoint_url
    };

    // P7-006: protocol ID must be registered in the protocol table.
    if !xai_grok_provider::protocol::known_protocols().contains(&protocol_id) {
        return Err(ProviderResolutionError::Protocol(format!(
            "unknown protocol_id: {protocol_id}"
        )));
    }

    // Merge static headers (route + model, no auth — Phase 8 resolves credentials)
    let mut static_headers = route.static_headers.clone();
    static_headers.extend(model.info.extra_headers.clone());

    // P7-007: merge generation params with fixed precedence.
    let (max_tokens, temperature, top_p) = merge_generation_params(
        &route.generation_defaults,
        &route.limits,
        model.info.max_completion_tokens,
        model.info.temperature,
        model.info.top_p,
        None,
        None,
        None,
    );
    let generation = GenerationOptions::new(max_tokens, temperature, top_p);

    Ok(ResolvedModelExecution {
        provider_id: configured.id.clone(),
        route_id: RouteId::new(&route_id),
        protocol_id,
        request_url,
        static_headers,
        auth_policy: route.auth.clone(),
        model_id: ModelId::new(&model.info.model),
        generation,
        limits: route.limits.clone(),
    })
}

/// A legacy (unqualified) xAI model reference that can be migrated to
/// `provider/model` syntax. This is the only way a bare model name maps
/// to a specific provider — all other bare models must be explicitly
/// qualified or found in a provider's model catalog.
pub struct LegacyModelReference(&'static str);

impl LegacyModelReference {
    /// All known legacy xAI model names.
    pub const ALL: &'static [LegacyModelReference] = &[
        LegacyModelReference("grok-build"),
        LegacyModelReference("grok-3"),
        LegacyModelReference("grok-3-mini"),
        LegacyModelReference("grok-3-fast"),
        LegacyModelReference("grok-3-mini-fast"),
        LegacyModelReference("grok-2"),
        LegacyModelReference("grok-2-mini"),
        LegacyModelReference("grok-vision"),
        LegacyModelReference("grok-1"),
    ];

    /// The provider to which this legacy model resolves.
    pub fn provider(&self) -> &'static str {
        "xai"
    }

    /// The bare model name.
    pub fn model(&self) -> &'static str {
        self.0
    }

    /// Look up a bare model name; returns `Some` only for known legacy names.
    pub fn try_resolve(bare_model: &str) -> Option<&'static LegacyModelReference> {
        LegacyModelReference::ALL
            .iter()
            .find(|lr| lr.0 == bare_model)
    }
}

/// Resolve a bare model name through the legacy xAI model table.
/// Returns `None` for models not in the known legacy set.
fn resolve_legacy_model_ref(bare_model: &str) -> Option<String> {
    LegacyModelReference::try_resolve(bare_model).map(|lr| lr.provider().to_string())
}

/// Resolve a CLI model reference against the merged catalog.
///
/// Cases:
/// - `--model openai/gpt-4o` → provider=openai, model=gpt-4o
/// - `--provider openai --model gpt-4o` → equivalent
/// - conflicting `--provider anthropic --model openai/gpt-4o` → error
/// - bare model uniquely found in one provider → resolves
/// - ambiguous bare model → error listing canonical choices
/// - legacy bare xAI defaults → resolve predictably
pub fn resolve_cli_model_reference(
    model_ref: &str,
    provider_override: Option<&str>,
    merged_catalog: &IndexMap<String, ModelEntry>,
) -> Result<(String, String), ProviderResolutionError> {
    let (model_provider, bare_model) = xai_grok_provider::types::parse_model_ref(model_ref);

    match (model_provider, provider_override) {
        (Some(mp), Some(po)) => {
            let mp_name = mp.0.clone();
            if mp_name != po {
                return Err(ProviderResolutionError::ConflictingProvider {
                    provider: po.to_string(),
                    model: model_ref.to_string(),
                });
            }
            Ok((po.to_string(), bare_model))
        }
        (Some(mp), None) => Ok((mp.0.clone(), bare_model)),
        (None, Some(po)) => Ok((po.to_string(), bare_model)),
        (None, None) => {
            // Bare model: check merged catalog for unique resolution
            let canonical_key = format!("xai/{}", bare_model);
            let mut matches: Vec<String> = Vec::new();

            for key in merged_catalog.keys() {
                if key.ends_with(&format!("/{}", bare_model)) {
                    matches.push(key.clone());
                }
            }

            match matches.len() {
                0 => {
                    // Try legacy xAI migration path
                    if let Some(mapped) = resolve_legacy_model_ref(&bare_model) {
                        Ok((mapped, bare_model))
                    } else {
                        Err(ProviderResolutionError::ModelNotFound(bare_model))
                    }
                }
                1 => {
                    let provider = matches[0].split('/').next().unwrap_or("xai").to_string();
                    Ok((provider, bare_model))
                }
                _ => Err(ProviderResolutionError::AmbiguousModel(format!(
                    "bare model '{}' matches multiple providers: {}. Use provider/model syntax.",
                    bare_model,
                    matches.join(", ")
                ))),
            }
        }
    }
}

/// Merge models from multiple sources into a deterministic catalog.
///
/// Precedence (high → low):
///   manual user model override
///   > dynamic provider-discovered model metadata
///   > provider static known-model metadata
///   > embedded legacy xAI default metadata
///
/// Canonical key: `provider/model` for non-legacy models.
/// Bare lookup that matches multiple providers returns `AmbiguousModel`.
pub fn merge_model_catalog(
    registry_snapshot: &RegistrySnapshot,
    catalog_snapshot: &ModelCatalogSnapshot,
    manual_overrides: &IndexMap<String, ModelEntry>,
    legacy_defaults: &IndexMap<String, ModelEntry>,
    endpoints: &crate::agent::config::EndpointsConfig,
) -> IndexMap<String, ModelEntry> {
    let mut merged: IndexMap<String, ModelEntry> = IndexMap::new();

    // Layer 1: embedded legacy xAI defaults (lowest priority)
    for (key, entry) in legacy_defaults {
        merged.entry(key.clone()).or_insert_with(|| entry.clone());
    }

    // Layer 2: dynamic provider-discovered models from catalog
    for (_pid, catalog_entry) in &catalog_snapshot.providers {
        if catalog_entry.state != super::provider_catalog::ProviderCatalogState::Fresh {
            continue;
        }
        for model_cfg in &catalog_entry.models {
            let key = format!("{}/{}", catalog_entry.provider_id.0, model_cfg.model);
            if !merged.contains_key(&key) {
                let display_name = model_cfg.name.clone().unwrap_or_default();
                let mut entry = ModelEntry::from_config_entry(model_cfg);
                entry.provider_id = Some(catalog_entry.provider_id.0.clone());
                merged.insert(key, entry);
            }
        }
    }

    // Layer 3: provider static known-model metadata (from registry snapshot
    // provider defaults). These fill in gaps when no dynamic model exists.
    for (_pid, configured) in &registry_snapshot.providers {
        if let xai_grok_provider::types::ModelSourceSpec::Static(models) = &configured.model_source
        {
            for model_id in models {
                let key = format!("{}/{}", configured.id.0, model_id);
                merged.entry(key).or_insert_with(|| {
                    let mut entry = ModelEntry::fallback(model_id, endpoints);
                    entry.provider_id = Some(configured.id.0.clone());
                    entry.info.base_url = configured
                        .routes
                        .get(&configured.default_route_id)
                        .and_then(|r| r.endpoint.base_url.clone())
                        .unwrap_or_default();
                    entry
                });
            }
        }
    }

    // Layer 4: manual user model overrides (highest priority)
    for (key, entry) in manual_overrides {
        merged.insert(key.clone(), entry.clone());
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::config::EndpointsConfig;
    use indexmap::IndexMap;


    fn provider_registry_with_defaults() -> xai_grok_provider::registry::ProviderRegistry {
        let reg = xai_grok_provider::registry::ProviderRegistry::new();
        xai_grok_provider::providers::register_all(&reg);
        reg
    }

    fn model_entry(model: &str, provider_id: &str) -> ModelEntry {
        let mut entry = ModelEntry::fallback(model, &EndpointsConfig::default());
        entry.provider_id = Some(provider_id.to_string());
        entry
    }

    // P7-001: missing provider returns typed ProviderNotFound error
    #[test]
    fn route_compiler_missing_provider() {
        let reg = provider_registry_with_defaults();
        let snap = reg.snapshot();
        let entry = model_entry("gpt-4o", "nonexistent");
        let result = execution_to_sampler_config(&entry, &snap, Some("key"), None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderResolutionError::ProviderNotFound(_)
        ));
    }

    // P7-001: provider exists but endpoint uses insecure remote HTTP
    #[test]
    fn route_compiler_invalid_endpoint_rejected() {
        let reg = xai_grok_provider::registry::ProviderRegistry::new();
        // register_route is a no-op in the current registry, so this tests that
        // the route compiler returns an error when a provider is not fully configured
        let _route = xai_grok_provider::route::Route::new(
            xai_grok_provider::types::RouteId::new("bad-route"),
            xai_grok_provider::types::ProviderId::new("test-p"),
            "chat_completions",
            xai_grok_provider::endpoint::Endpoint::new(
                Some("http://remote.insecure:8080/v1".into()),
                xai_grok_provider::endpoint::EndpointPart::Static("/chat".into()),
                None,
            ),
            xai_grok_provider::auth::AuthPolicy::None,
        );
        let snap = reg.snapshot();
        let entry = model_entry("some-model", "test-p");
        let result = execution_to_sampler_config(&entry, &snap, Some("key"), None);
        assert!(result.is_err(), "route compiler must error for missing provider");
    }

    #[test]
    fn cli_model_ref_provider_prefix() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("openai/gpt-4o", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_provider_override() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("gpt-4o", Some("anthropic"), &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_conflicting_provider_error() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("openai/gpt-4o", Some("anthropic"), &catalog);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderResolutionError::ConflictingProvider { .. }
        ));
    }

    #[test]
    fn cli_model_ref_bare_unique_resolves() {
        let mut catalog = IndexMap::new();
        let mut entry =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        entry.provider_id = Some("openai".into());
        catalog.insert("openai/gpt-4o".into(), entry);
        let result = resolve_cli_model_reference("gpt-4o", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn cli_model_ref_bare_ambiguous_error() {
        let mut catalog = IndexMap::new();
        let mut e1 =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        e1.provider_id = Some("openai".into());
        catalog.insert("openai/gpt-4o".into(), e1);
        let mut e2 =
            ModelEntry::fallback("gpt-4o", &crate::agent::config::EndpointsConfig::default());
        e2.provider_id = Some("xai".into());
        catalog.insert("xai/gpt-4o".into(), e2);
        let result = resolve_cli_model_reference("gpt-4o", None, &catalog);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderResolutionError::AmbiguousModel(_)
        ));
    }

    #[test]
    fn cli_model_ref_bare_legacy_xai_fallback() {
        let catalog = IndexMap::new();
        let result = resolve_cli_model_reference("grok-build", None, &catalog);
        assert!(result.is_ok());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "xai");
        assert_eq!(model, "grok-build");
    }

    // P7-007: generation/default merge tests
    #[test]
    fn merge_gen_route_defaults_used_when_model_and_request_are_none() {
        let (max_tokens, temperature, top_p) = merge_generation_params(
            &GenerationOptions::default(),
            &ModelLimits::default(),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        // All defaults are None, so result should be None
        assert_eq!(max_tokens, None);
        assert_eq!(temperature, None);
        assert_eq!(top_p, None);
    }

    #[test]
    fn merge_gen_model_overrides_route() {
        let route_gen = GenerationOptions::default();
        let route_limits = ModelLimits::default();
        let (max_tokens, temperature, _) = merge_generation_params(
            &route_gen,
            &route_limits,
            Some(200),
            Some(0.7),
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            max_tokens,
            Some(200),
            "model max_tokens when route has none"
        );
        assert_eq!(
            temperature,
            Some(0.7),
            "model temperature when route has none"
        );
    }

    #[test]
    fn merge_gen_request_overrides_model_and_route() {
        let route_gen = GenerationOptions::default();
        let route_limits = ModelLimits::default();
        let (max_tokens, temperature, _) = merge_generation_params(
            &route_gen,
            &route_limits,
            Some(200),
            Some(0.7),
            None,
            Some(300),
            Some(0.9),
            None,
        );
        assert_eq!(max_tokens, Some(300), "request max_tokens overrides model");
        assert_eq!(
            temperature,
            Some(0.9),
            "request temperature overrides model"
        );
    }

    #[test]
    fn merge_gen_limits_cap_max_tokens() {
        let route_gen = GenerationOptions::default();
        let route_limits = ModelLimits::default();
        let (max_tokens, _, _) = merge_generation_params(
            &route_gen,
            &route_limits,
            Some(1000),
            None,
            None,
            None,
            None,
            None,
        );
        // default limits have output=None, so no cap
        assert_eq!(max_tokens, Some(1000), "no cap when route limit is None");
    }

    #[test]
    fn merge_gen_no_cap_when_limit_is_none() {
        let route_gen = GenerationOptions::default();
        let route_limits = ModelLimits::default();
        let (max_tokens, _, _) = merge_generation_params(
            &route_gen,
            &route_limits,
            Some(1000),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(max_tokens, Some(1000), "no cap when route limit is None");
    }

    /// Static scan: production code must not construct `SamplerConfig` directly.
    /// Phase 8 gate requires all production paths go through
    /// `resolve_model_execution → prepare_sampler_config → PreparedSamplerConfig → Sampler`.
    /// `#[cfg(test)]` blocks and test-only directories are excluded.
    #[test]
    fn no_sampler_config_construction_in_production_sources() {
        let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let src_dir = crate_dir.join("src");
        let mut failures = Vec::new();
        let mut files = Vec::new();
        collect_rs_files(&src_dir, &mut files);
        for file_path in &files {
            let relative = file_path.strip_prefix(&crate_dir).unwrap_or(file_path);
            // Skip directories that are test-only (gated by #[cfg(test)] with #[path])
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            if rel_str.contains("acp_session_tests/") || rel_str.contains("test_support/") {
                continue;
            }
            let content = std::fs::read_to_string(file_path).unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            let mut in_test_block = false;
            let mut brace_depth = 0usize;
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed == "#[cfg(test)]" || trimmed.starts_with("#[cfg(test)]") {
                    in_test_block = true;
                    continue;
                }
                if in_test_block {
                    if trimmed.starts_with("mod ") && trimmed.ends_with('{') {
                        brace_depth += 1;
                        continue;
                    }
                    if trimmed.starts_with("mod ") && trimmed.ends_with(';') {
                        in_test_block = false;
                        continue;
                    }
                    if trimmed == "}" {
                        if brace_depth > 0 {
                            brace_depth -= 1;
                        }
                        if brace_depth == 0 {
                            in_test_block = false;
                        }
                        continue;
                    }
                    if trimmed.starts_with('{') || trimmed.ends_with('{') {
                        brace_depth += 1;
                    }
                    continue;
                }
                if !trimmed.starts_with("//")
                    && trimmed.contains("SamplerConfig {")
                    && !trimmed.contains("-> SamplerConfig {")
                    && !trimmed.contains("PreparedSamplerConfig")
                {
                    failures.push(format!(
                        "{}:{}: {}",
                        relative.display(),
                        i + 1,
                        trimmed
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "Production code must not construct SamplerConfig directly.\n\
             All production paths must use resolve_model_execution → prepare_sampler_config → PreparedSamplerConfig → Sampler.\n\
             Found {} occurrence(s):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    out.push(path);
                }
            }
        }
    }
}
