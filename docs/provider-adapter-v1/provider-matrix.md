# Provider Adapter V1 — Provider Compatibility Matrix

| Provider ID | Default Base URL | Inference Protocol(s) | Inference Path(s) | Auth Header & Credential Sources | Mandatory Static Headers | Model-List URL & Format | Public/No-Auth Behavior | Local HTTP Allowed | Tested Model Examples | Live Smoke Env Vars |
|---|---|---|---|---|---|---|---|---|---|---|
| `xai` | `https://api.x.ai/v1` | Responses | `/v1/responses` | `Authorization: Bearer` from inline key → `XAI_API_KEY` → session token | `x-grok-*` headers per existing xAI behavior | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `grok-build`, `grok-4-*` | `XAI_API_KEY`, `XAI_SESSION_TOKEN` |
| `openai` | `https://api.openai.com/v1` | Chat Completions, Responses | `/chat/completions`, `/v1/responses` | `Authorization: Bearer` from inline key → `OPENAI_API_KEY` | None | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini` | `OPENAI_API_KEY` |
| `anthropic` | `https://api.anthropic.com/v1` | Messages | `/v1/messages` | `x-api-key` from inline key → `ANTHROPIC_API_KEY` | `anthropic-version: 2023-06-01` | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `claude-sonnet-4-20250514`, `claude-3-5-haiku` | `ANTHROPIC_API_KEY` |
| `opencode` | `https://opencode.ai/zen/v1` | Chat Completions | `/v1/chat/completions` | `Authorization: Bearer` from inline key → `OPENCODE_API_KEY`, or **public** (no auth) | None | `{base_url}/models` → OpenAI Compatible | **Public mode supported**: no auth header sent, only free models returned | No | opencode free models | `OPENCODE_API_KEY` |
| `ollama` | `http://localhost:11434/v1` | Chat Completions | `/v1/chat/completions` | None | None | `http://localhost:11434/api/tags` → OllamaTags | Always public (no auth) | **Yes** (loopback only) | `llama3.1`, `codellama`, `deepseek-coder` | None |
| `deepseek` (profile) | `https://api.deepseek.com/v1` | Chat Completions | `/v1/chat/completions` | `Authorization: Bearer` from inline key → `DEEPSEEK_API_KEY` | None | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `deepseek-chat`, `deepseek-reasoner` | `DEEPSEEK_API_KEY` |
| `groq` (profile) | `https://api.groq.com/openai/v1` | Chat Completions | `/v1/chat/completions` | `Authorization: Bearer` from inline key → `GROQ_API_KEY` | None | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `llama3-70b-8192`, `mixtral-8x7b-32768` | `GROQ_API_KEY` |
| `openrouter` (profile) | `https://openrouter.ai/api/v1` | Chat Completions | `/v1/chat/completions` | `Authorization: Bearer` from inline key → `OPENROUTER_API_KEY` | None | `{base_url}/models` → OpenAI Compatible | No (always requires auth) | No | `anthropic/claude-sonnet`, `openai/gpt-4o` | `OPENROUTER_API_KEY` |
| `openai-compatible` (custom) | User-specified | Chat Completions | `{base_url}/chat/completions` | `Authorization: Bearer` from inline key → env key per config | User-specified via `extra_headers` | `{base_url}/models` or user-specified → OpenAI Compatible | Only if auth scheme is `None` or `Public` | If URL is loopback | User-defined | Per user `env_key` config |

## Notes

- Remote provider endpoints require `https`, except explicit local providers (localhost, loopback) which may use `http`.
- Embedded credentials and fragments in configured network URLs are rejected.
- Unknown protocol IDs produce a typed error; they never fall back to Chat Completions.
- Provider/model ordering must be deterministic across runs and platforms.
- Secrets must never appear in Debug, logs, panic messages, snapshots, test output, or telemetry.
