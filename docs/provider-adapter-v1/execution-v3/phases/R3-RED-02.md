# R3-RED-02 Evidence

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: 9b99aa722d29a790ef55c7c8a989892d7cd26973
- Parent commit: f5a7611b33448170e8eb987f42e593a0fe4d57fe
- Files changed:
  - `crates/codegen/xai-grok-shell/tests/provider_async_model_switch.rs` (test)
- Failing test before fix: `model_switch_nested_runtime_panic`
- Failure output summary:

```
thread 'model_switch_nested_runtime_panic' panicked at
  agent_ops.rs:1173:38:
Cannot start a runtime from within a runtime. This happens because a
function (like `block_on`) attempted to block the current thread while
the thread is being used to drive asynchronous tasks.

R3-RED-02: nested-runtime panic detected (pre-fix expected):
  Cannot start a runtime from within a runtime.
```

- Passing test after fix: N/A (Phase 2)
- Targeted check: `cargo check -p xai-grok-shell --tests --locked` ✅
- Targeted clippy: N/A (test binary)
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
