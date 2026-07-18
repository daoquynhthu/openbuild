# Troubleshooting — Provider Adapter V1

## Provider not appearing in `/providers`

**Symptom**: A `[provider."my-id"]` entry in `config.toml` does not
show up in the `/providers` TUI or `grok providers` output.

**Cause**: Only the six built-in provider IDs are registered:
`xai`, `openai`, `anthropic`, `opencode`, `ollama`,
`openai-compatible`. Unknown IDs are silently ignored by the registry.

**Fix**: Use one of the known IDs. For custom OpenAI-compatible
endpoints, use the `openai-compatible` ID:

```toml
[provider."openai-compatible"]
env_key = ["MY_KEY"]
base_url = "https://my-proxy/v1"
```

## Model not selectable

**Symptom**: The provider shows as configured but no models appear
for selection.

**Causes**:

1. **Discovery timeout**: The model list fetch from the provider API
   timed out (default 10s connection timeout). Check network access
   to the provider's base URL.

2. **Model list format mismatch**: The mock server or provider API
   returns models in an unexpected format. Verify the `/v1/models`
   response matches the expected schema.

3. **No matching route**: The model's `apiBackend` field must match
   a route on the provider. For example, `chat_completions` requires
   an OpenAI Chat route; `responses` requires a Responses route.

## Save fails silently

**Symptom**: Pressing Enter in the provider detail view does not
save or shows an error.

**Causes**:

1. **Invalid provider ID**: Only alphanumeric characters, hyphens,
   and underscores are allowed.

2. **Invalid base URL**: Must use `http://` or `https://` scheme
   with a valid host.

3. **Config.toml not writable**: Check file permissions on
   `~/.grok/config.toml`.

## Legacy xAI config stops working

**Symptom**: After adding a new provider, xAI requests fail.

**Most common cause**: The `--provider` CLI flag overrides the
default provider. If set to a non-xAI provider, xAI models won't
resolve. Use `--model xai/grok-3` to explicitly route to xAI.

**Rollback**: Remove all `[provider.*]` entries from `config.toml`
except `[provider.xai]` to restore legacy behavior.

## TUI panic / crash

Run with `RUST_BACKTRACE=1` and capture the backtrace:

```bash
RUST_BACKTRACE=1 grok 2>&1 | tee crash.log
```

Check for `unwrap()` failures in `provider_state.rs` or
`providers_modal.rs`. Known issues are documented in `ISSUE.md`.
