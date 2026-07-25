# Provider Adapter V1 — Public API Contracts

> Provider type definitions, error handling, and contract guarantees.

## Validated IDs

`ProviderId`, `RouteId`, `ModelId` validate non-empty trimmed values.
Reject whitespace-only strings and invalid separators.

## ProviderError Variants

- `InvalidProviderId` — provider ID is empty or contains invalid characters
- `InvalidRouteId` — route ID is empty or contains invalid characters
- `DuplicateProvider` — definition registration with duplicate ID
- `DuplicateRoute` — route with duplicate ID
- `UnknownProvider` — referenced provider not found
- `UnknownRoute` — referenced route not found
- `InvalidEndpoint` — malformed URL, unsupported scheme, embedded credentials
- `MissingCredential` — required credential not available
- `InvalidHeader` — header name/value violates HTTP token rules
- `UnknownProtocol` — protocol ID not recognized
- `AmbiguousModel` — bare model ID matches more than one provider
- `Config` — configuration-level error with diagnostic details

Error display text must never contain secret values.

## Configuration Fields

| Field | Type | Description |
|-------|------|-------------|
| `kind` | string (optional) | Provider implementation kind: `"openai-compatible"` for custom |
| `profile` | string (optional) | Named profile: `"openai-compatible"`, `"deepseek"`, `"groq"`, `"openrouter"` |
| `base_url` | string (optional) | Provider base URL; remote endpoints require `https`, loopback allows `http` |
| `api_key` | string (optional) | Inline API key (deprecated — prefer `env_key`) |
| `env_key` | string array (optional) | Environment variable names to read credential from |
| `extra_headers` | table (optional) | Additional HTTP headers to include in requests |
| `model_list_format` | string (optional) | Model discovery format: `"openai_compatible"`, `"ollama_tags"`, `"ollama"` |
| `model_list_path` | string (optional) | Custom model list URL path |
| `allow_insecure_http` | bool (optional) | Allow `http://` for non-loopback endpoints |
| `protocol` | string (optional) | Route protocol: `"chat_completions"`, `"responses"`, `"messages"` |

## Credential Priority

```
1. Inline api_key (from config or CLI)
2. ProviderEnvironment (env_key variables, in declaration order)
3. Session token (XAI_SESSION_TOKEN or auth-managed)
4. Public/None (no auth)
```

## Built-in Providers

| Provider ID | Default Base URL | Auth Required | Insecure HTTP |
|-------------|-----------------|---------------|---------------|
| `xai` | `https://api.x.ai/v1` | Yes | No |
| `openai` | `https://api.openai.com/v1` | Yes | No |
| `anthropic` | `https://api.anthropic.com/v1` | Yes | No |
| `opencode` | `https://opencode.ai/zen/v1` | No (public mode) | No |
| `ollama` | `http://localhost:11434/v1` | No | Yes (loopback only) |

## Named Profiles

`deepseek`, `groq`, `openrouter` — inherit base URL and env key conventions.

## Custom OpenAI-Compatible Provider

Use `kind = "openai-compatible"` or `profile = "openai-compatible"` with explicit `base_url`.

## Route Selection

Determined by the model's `protocol` field or the provider's default protocol.
- `chat_completions` → Chat Completions route
- `responses` → Responses route
- `messages` → Messages route (Anthropic)

## Hot-Reload Semantics

`rebuild_from_resolved` creates a new RegistrySnapshot atomically:
- On success: revision increments by 1, new snapshot replaces old
- On failure: old snapshot preserved, error returned
- In-flight requests continue using their acquired snapshot reference

## Stale Catalog

When model discovery fails:
- Catalog state transitions to `Stale`
- Previously discovered models remain visible
- Next refresh retries discovery

## Supported Platforms

Linux, Windows, macOS — all CI-gated with identical contract tests.

## Hard Errors (no HTTP request sent)

| Condition | Error |
|-----------|-------|
| Missing required credential | `MissingCredential` |
| Invalid endpoint URL | `InvalidEndpoint` |
| Unknown protocol | `UnknownProtocol` |
| Ambiguous model reference | `AmbiguousModel` |
| Insecure HTTP on remote endpoint | `InvalidEndpoint` |
