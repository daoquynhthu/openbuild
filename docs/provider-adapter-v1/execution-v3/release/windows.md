# Windows Release Acceptance — Provider Adapter V1

**Date**: 2026-07-26

**Candidate commit**: `0d5e89a` (phase-13: R3-DOC-02 — update PROGRESS.md)

**Environment**: Windows 10+ (local dev machine)

## Commands executed

| Command | Result |
|---------|--------|
| `cargo fmt --all -- --check` | PASS ✅ |
| `cargo check --workspace --all-targets --locked` | PASS ✅ |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS ✅ |
| `cargo test -p xai-grok-provider --all-targets --locked` | PASS (all tests) ✅ |
| `cargo test -p xai-grok-shell --test test_provider_chain_e2e --locked` | PASS (21/21) ✅ |
| `cargo test -p xai-grok-shell --test provider_production_chain --locked` | PASS (10/10) ✅ |
| `git diff --check HEAD~5..HEAD` | PASS (clean) ✅ |

## Invariant scanner

```
Total production violations: 3 (3 known baseline — documented)
```

Known baselines:
1. `router.rs:608` — ProviderRuntime::new() residual (P11)
2. `models.rs:1657` — rt.block_on() residual (P11)
3. `config.rs:1` — ParsedProviderEntry denied fields (correct, PerProvider override)

## Secret hygiene

- No real secrets found in any fixture or artifact
- All test canary values are test-only and appear only in expected locations

## Fault injection coverage

Covered by existing E2E tests (all 31 pass):
- invalid hot-reload config (`hot_reload_invalid_config_preserves_snapshot`)
- credential backend failure (`credential_backend_hard_fail_no_requests`)
- catalog timeout and malformed response (R3-E2E-06)
- hot-reload with in-flight requests preserved

## Known limitations

- Linux and macOS CI acceptance not yet executed
- PTY provider tests not run (requires CI runner)
