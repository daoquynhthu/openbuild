# R3-RED-04 Evidence

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: f051a40413fe7e89bbdf20a28a7456b1116a95c2
- Parent commit: 833008dd3f5aadfd2e740c3d7424e4d9f3ebc18b
- Files changed:
  - `crates/codegen/xai-grok-shell/tests/provider_production_chain.rs` (extended)
- Failing tests before fix:
  - None (these are documentation-style red tests that prove fallback behavior)
- Failure output summary:
  - `red04_missing_credential_incorrectly_returns_ok`: current code returns `Ok(SamplerConfig)` WITHOUT credentials — BUG
  - `red04_incompatible_protocol_incorrectly_returns_ok`: current code returns `Ok(SamplerConfig)` without validating protocol compatibility — BUG
  - `red04_invalid_endpoint_produces_chain_error`: chain correctly returns `Err` — agent-level fallback is the bug (visible after Phase 2)
  - `red04_unknown_route_id_correctly_errs`: chain correctly returns `Err` — agent-level fallback is the bug (visible after Phase 2)
- Passing tests after fix: N/A (Phase 3/5)
- Targeted check: `cargo check -p xai-grok-shell --test provider_production_chain --locked` ✅
- Targeted test: `cargo test -p xai-grok-shell --test provider_production_chain -- red04` ✅ (4 passed)
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
