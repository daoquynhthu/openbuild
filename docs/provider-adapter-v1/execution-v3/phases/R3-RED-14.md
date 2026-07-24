# R3-RED-14 Evidence

- File created: `crates/codegen/xai-grok-shell/tests/provider_real_entry_e2e.rs`
- Constraints verified:
  - No `ModelEntry::fallback` — uses direct struct construction
  - No manual key passing — `execution_to_sampler_config` called with `None`
  - No `resolve_model_execution` as top-level action — called inside `execution_to_sampler_config`
- RED failures (4):
  1. `e2e_provider_inline_key_dropped` — `execution_to_sampler_config` passes `None` for `provider_inline` credential; openai-compatible provider's `api_key` from TOML is silently dropped
  2. `e2e_custom_provider_inline_key_lost` — same root cause for custom OpenAI-compatible provider
  3. `e2e_openai_builtin_inline_key_dropped` — built-in OpenAI has stricter credential check; dropped provider key causes `AuthCredential` error instead of successful request
  4. `e2e_missing_credential_hard_error` — no credential anywhere still returns `Ok` (openai-compatible); should return typed hard error
- Baseline PASS (1):
  - `e2e_responses_protocol_respected` — `protocol=responses` correctly produces `ApiBackend::Responses`
- Deviations: none
