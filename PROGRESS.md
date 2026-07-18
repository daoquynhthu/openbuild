# Progress — Provider Model Adapter Refactoring

## Phase 10: Real TUI and CLI Provider Configuration Closure

### Completion
- P10-01: ProviderState/ProviderView with credential/catalog state, redact_endpoint
- P10-02: Detail view editing state, Debug redaction of api_key
- P10-03: SaveProviderConfig effect, persist_provider_config, validation
- P10-04: env_key persistence policy (env_var_name field, inline key warning)
- P10-05: Force model refresh ('r' key in list view)
- P10-06: 41 unit tests (provider_state 13 + providers_modal 17 + config_validation 11)
- P10-07: PTY E2E test (providers_pty.rs) — open /providers, verify configured state, select model, send prompt, verify response; common.rs cross-platform fixes
- P10-08: `grok providers` CLI command — list configured providers with endpoint/routes/auth

### Current State
- `cargo check --workspace`: passes (no errors or warnings)
- `cargo clippy -p xai-grok-pager --lib`: passes
- `cargo test -p xai-grok-pager --lib`: 41 new tests passing

### Phase 10 gate
- Gate T3: pending (need to verify C-12 closure)
- PTY E2E: ✅ (providers_pty.rs, ignored)
