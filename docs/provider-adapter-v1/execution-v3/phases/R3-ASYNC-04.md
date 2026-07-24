# R3-ASYNC-04 Evidence

- Baseline commit: 2e5f149
- Result commit: 8fd92c2
- Files changed:
  - `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`
  - `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`
- Failing test before fix: R3-RED-03 (auxiliary model through profile override path)
- Failure output summary: before ASYNC-01/02, `Handle::current().block_on()` panicked on single-thread runtime
- Passing test after fix: agent profile model override is async, returns Result, errors propagate
- Targeted check: `cargo check -p xai-grok-shell --lib --tests` — clean
- Targeted clippy: `cargo clippy -p xai-grok-shell --lib` — clean
- `git diff --check HEAD^..HEAD`: clean
- Deviations: none (completed as part of ASYNC-02 scope)
