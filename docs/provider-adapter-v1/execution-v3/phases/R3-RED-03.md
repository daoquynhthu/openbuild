# R3-RED-03 Evidence

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: 2f36c8db76d89d10922b52fcf9c2b8b65adc1812
- Parent commit: 9b99aa722d29a790ef55c7c8a989892d7cd26973
- Files changed:
  - `crates/codegen/xai-grok-shell/tests/provider_async_aux_models.rs` (test)
- Failing tests before fix:
  - `aux_model_nested_runtime_panic` — panics at `config.rs:4528`
  - `web_search_nested_runtime_panic` — panics at `config.rs:4973`
- Failure output summary:

```
R3-RED-03 aux: nested-runtime panic (pre-fix expected):
  Cannot start a runtime from within a runtime.
R3-RED-03 web: nested-runtime panic (pre-fix expected):
  Cannot start a runtime from within a runtime.
```

- Passing tests after fix: N/A (Phase 2)
- Targeted check: `cargo check -p xai-grok-shell --tests --locked` ✅
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
