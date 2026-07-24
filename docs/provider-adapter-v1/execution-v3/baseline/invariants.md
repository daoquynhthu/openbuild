# Invariant Scanner Baseline

- **Date:** 2026-07-25
- **Scanner:** `scripts/provider-v1/assert_provider_v1_invariants.py`
- **Baseline commit:** `916c66e`
- **Schema:** `invariant_check_v1`

## Summary

| Check | Status | Violations |
|---|---|---|
| `Handle::block_on` in provider request preparation | PASS | 0 |
| `Runtime::block_on` in provider request preparation | FAIL | 1 |
| `unimplemented!`/`todo!` in `xai-grok-provider` | FAIL | 1 |
| `ProviderRuntime::new()` in Pager production paths | FAIL | 1 |
| route-compiler error → legacy `sampling_config_for_model` | FAIL | 1 |
| `continue-on-error` in release-gate workflows | FAIL | 1 |
| unknown-field-tolerant provider schema | FAIL | 1 |
| **Total** | **6 failing** | **6** |

## Individual Violations

### V-01: `Runtime::block_on` in provider request preparation

- **Path:** `crates/codegen/xai-grok-shell/src/agent/models.rs:1657`
- **Pattern:** `rt.block_on(`
- **Context:** Startup prefetch — `init_prefetch_tasks::worker_loop` calls into model prefetch from a synchronous extension boundary with no active async runtime. This is a top-level startup path, not a request-time blocking call.
- **Disposition:** Allowed — startup prefetch only, documented at `scanner.rs` with reason code `STARTUP_BLOCKING`.

### V-02: `unimplemented!()` in `xai-grok-provider`

- **Path:** `crates/codegen/xai-grok-provider/src/providers/openai_compatible_factory.rs:52`
- **Pattern:** `unimplemented!("defaults are not yet supported for openai-compatible providers")`
- **Context:** `ProviderFactory::defaults()` is never called in the current production path but must be implemented before Phase 4 gate.
- **Disposition:** Blocked on R3-CFG-08 (Phase 4) — will replace with actual defaults or remove the method.

### V-03: `ProviderRuntime::new()` in Pager production paths

- **Path:** `crates/codegen/xai-grok-pager/src/app/dispatch/router.rs:608`
- **Pattern:** `ProviderRuntime::new()`
- **Context:** Pager constructs its own `ProviderRuntime` when none is injected. This violates the runtime singleton contract (§2.7).
- **Disposition:** Blocked on R3-RUNTIME-01 (Phase 9) — must inject shared runtime from agent bootstrap.

### V-04: Route-compiler error → legacy fallback

- **Path:** `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs:1196`
- **Pattern:** `Err(_) => crate::agent::config::sampling_config_for_model`
- **Context:** When route compilation fails, the code falls back to legacy `sampling_config_for_model` instead of propagating the typed error.
- **Disposition:** Blocked on R3-ERR-01 (Phase 3) — must replace with `?` and typed error mapping.

### V-05: `continue-on-error: true` in release-gate workflow

- **Path:** `.github/workflows/provider-adapter.yml:166`
- **Pattern:** `continue-on-error: true`
- **Context:** PTY Provider E2E step uses `continue-on-error`, making it non-blocking.
- **Disposition:** Blocked on R3-CI-05 (Phase 12) — must remove after E2E tests are stable.

### V-06: Unknown-field-tolerant provider schema

- **Path:** `crates/codegen/xai-grok-provider/src/config.rs:1`
- **Pattern:** Missing `#[serde(deny_unknown_fields)]` on provider config type(s)
- **Context:** The config parser accepts unknown TOML fields without error. A typo like `implementation = "openai-compatible"` (vs `kind`) is silently ignored, leading to behavior surprises.
- **Disposition:** Blocked on R3-PARSE-01 (Phase 6) — must add `#[serde(deny_unknown_fields)]` to the external provider entry type.

## Suppressed Paths

The following files are excluded from production-violation detection:

| Excluded pattern | Reason |
|---|---|
| `/tests/` in path | Integration test code |
| `/benches/` in path | Benchmark harness code |
| `_tests.rs` suffix | Test module files |
| `/tests.rs` suffix | Inline test module files |
| `_test.rs` in path | Test-related files (incl. `_test.rs` substring) |
| `#[cfg(test)] mod tests { ... }` | Inline test module (brace-depth tracking) |

## Running

```powershell
python scripts/provider-v1/assert_provider_v1_invariants.py
```

Exit code: 1 when any production violation exists; 0 when all pass.

## Known False Positive Candidates

- `crates/codegen/xai-grok-workspace/src/handle.rs:2816,2831` — Uses `block_in_place { Handle::current().block_on() }` for tool snapshot re-resolution. This is a Tokio-approved pattern (block then re-enter via existing runtime) and is not in the provider request preparation path. Excluded by crate filter (only `xai-grok-shell`, `xai-grok-provider`, `xai-grok-sampler`, `xai-grok-pager`).
