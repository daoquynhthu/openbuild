# Migration Guide — Provider Adapter V1

## Overview

Provider Adapter V1 replaces the legacy xAI-only configuration with a
multi-provider system. Existing xAI configurations continue to work
without changes, but new configuration syntax is available for
OpenAI, Anthropic, OpenCode Zen, Ollama, and arbitrary
OpenAI-compatible providers.

## Config precedence (low to high)

1. **Built-in defaults** — each provider has hardcoded defaults
2. **Environment variables** — e.g. `XAI_API_KEY`, `OPENAI_API_KEY`
3. **`config.toml` `[provider.*]` entries** — see examples below
4. **Legacy `[endpoints]` mapping** — backwards-compatible
5. **CLI flags** — `--provider`, `--api-key`, `--base-url`

## Config examples

### xAI (existing, unchanged)

```toml
[provider.xai]
api_key = "xai-..."
```

Or via env var:
```toml
[provider.xai]
env_key = ["XAI_API_KEY"]
```

### OpenAI

```toml
[provider.openai]
env_key = ["OPENAI_API_KEY"]
base_url = "https://api.openai.com/v1"
```

### Anthropic (Claude)

```toml
[provider.anthropic]
env_key = ["ANTHROPIC_API_KEY"]
base_url = "https://api.anthropic.com/v1"
```

### OpenCode Zen

```toml
[provider.opencode]
base_url = "https://api.opencode.ai/v1"
```

### Ollama (local)

```toml
[provider.ollama]
base_url = "http://localhost:11434"
```

### Custom OpenAI-compatible

```toml
[provider."my-provider"]
env_key = ["MY_PROVIDER_KEY"]
base_url = "https://my-custom-proxy.example.com/v1"
```

## Model reference syntax

- **Bare model**: `grok-3` — resolved to default provider (xAI)
- **Provider/model**: `openai/gpt-4o` — explicit provider routing
- **Provider with route**: `openai/gpt-4o` — route selected by API backend

## CLI usage

```bash
# List configured providers
grok providers

# List available models
grok models

# Single-turn with explicit provider
grok -p "hello" --provider openai --api-key sk-... --base-url https://...

# Set model with provider prefix
grok -p "hello" --model openai/gpt-4o
```

## Secrets policy

- **Prefer `env_key`**: store only the env var name in config.toml,
  set the actual key as an environment variable
- **Inline `api_key`**: supported but triggers a warning in the TUI
- Secrets never appear in `Debug`, logs, or persisted config output
