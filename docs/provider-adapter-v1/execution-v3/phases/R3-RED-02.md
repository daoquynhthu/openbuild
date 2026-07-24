# R3-RED-02 Evidence

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: d551219ab8bdb90836da65a5714a9d38ae978b6c
- Parent commit: f5a7611b33448170e8eb987f42e593a0fe4d57fe
- Files changed:
  - `crates/codegen/xai-grok-shell/src/agent/handlers/model_switch.rs` (test seam)
  - `crates/codegen/xai-grok-shell/tests/provider_async_model_switch.rs` (test)
  - `crates/codegen/xai-grok-shell/tests/provider_async_session_entry.rs` (type fix in panic handler)
- Failing test before fix: `model_switch_nested_runtime_panic`
- Failure output summary:

```
R3-RED-02 model_switch path: nested-runtime panic (pre-fix expected):
  Cannot start a runtime from within a runtime.
```

- Passing test after fix: N/A (Phase 2)
- Targeted check: `cargo check -p xai-grok-shell --test provider_async_model_switch --locked` ✅
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
