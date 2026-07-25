# Provider Adapter V1 — Config Reference

> Exact configuration examples for all V1 providers. No real secrets are used.

## Global Configuration

```toml
# ── Provider sections ───────────────────────────────────────────────
# Each [provider.<id>] overrides that provider's baked-in defaults.
# Unknown keys inside a provider section produce path-specific warnings.
# API key fields are never included in serialized diagnostic snapshots.

[provider.xai]
api_key = "test-xai-key-not-real"
# base_url = "https://api.x.ai/v1"     # optional override
# env_key = ["XAI_API_KEY"]            # optional override
# extra_headers = { "Custom-Header" = "value" }

[provider.openai]
api_key = "test-openai-key-not-real"
# env_key = ["OPENAI_API_KEY"]

[provider.anthropic]
api_key = "test-anthropic-key-not-real"
# env_key = ["ANTHROPIC_API_KEY"]

[provider.opencode]
# No api_key → public mode (free models only)
# api_key = "test-opencode-key-not-real"
# env_key = ["OPENCODE_API_KEY"]

[provider.ollama]
# base_url = "http://localhost:11434/v1"  # override default

# ── Named OpenAI-compatible profiles ────────────────────────────────
[provider.deepseek]
api_key = "test-deepseek-key-not-real"
# Inherits base_url from deepseek profile defaults

[provider.groq]
api_key = "test-groq-key-not-real"

[provider.openrouter]
api_key = "test-openrouter-key-not-real"

# ── Arbitrary user-defined OpenAI-compatible provider ───────────────
# Use `profile = "openai-compatible"` (reusable defaults) or `kind = "openai-compatible"` (standalone).
[provider.my-company]
profile = "openai-compatible"
base_url = "https://api.my-company.com/v1"
api_key = "test-my-company-key-not-real"
extra_headers = { "X-Company-Version" = "2.0" }

# Alternative: using `kind` instead of profile
[provider.my-other]
kind = "openai-compatible"
base_url = "https://api.other.com/v1"
env_key = ["MY_OTHER_KEY"]

# ── Manual model provider/route binding ─────────────────────────────
[model.my-custom-model]
model = "server-routing-slug"
provider = "openai-compatible"
route = "openai-compatible-chat"         # optional
# api_key overrides provider key
# base_url overrides provider URL

# ── CLI-equivalent targeting ────────────────────────────────────────
# --provider anthropic --model claude-sonnet-4-20250514
# --provider openai --model openai/gpt-4o
# --provider deepseek --model deepseek-chat
# --api-key <key> --base-url <url>

# ── Legacy xAI configuration (backward compatible) ──────────────────
# [endpoints]
# xai_api_base_url = "https://api.x.ai/v1"
# cli_chat_proxy_base_url = "..."
```

## Configuration Precedence

```
built-in defaults
  < environment variables
  < [provider.<id>] TOML section
  < legacy compatibility mapping ([endpoints], env)
  < CLI override (--provider, --api-key, --base-url)
```

Fields merge independently. Overriding `base_url` does not erase TOML `extra_headers`.
An explicit empty value either rejects or clears the field; it never silently
treats it inconsistently.

## Model Reference Syntax

Canonical: `provider/model` (e.g. `openai/gpt-4o`, `xai/grok-build`)

Bare model IDs are accepted only when they resolve uniquely under deterministic
precedence. Persisted model selection must store the canonical provider-qualified
reference for non-xAI providers.

## Legacy Compatibility

| Legacy feature | Maps to |
|----------------|---------|
| `[endpoints].xai_api_base_url` | xAI provider `base_url` |
| `XAI_API_KEY` env var | xAI provider `api_key` |
| CLI proxy/session auth route | xAI provider route |
| Bare xAI model selection | xAI provider, default route |
| Default models with no provider field | xAI provider |
