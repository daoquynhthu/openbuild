# P1-009: Baseline Failure Ledger
# Generated: 2026-07-19T10:50:33Z
# Base SHA: 1ec582912c9edce9813b0b02deed21b13f91b84a

## Platform Key
- LNX = Linux (CI pending)
- WIN = Windows (local)
- MAC = macOS (CI pending)

## Ledger Entries

| ID | Platform | Command | Package/Target | Failure Class | First Causal Error | Root Cause | Owner Phase | Status |
|---|---|---|---|---|---|---|---|---|
| BL-WIN-CHECK-001 | WIN | check --all-targets | xai-grok-pager-minimal (lib test) | compile-contract | E0425: cannot find function set_show_thinking_blocks in module minimal_api | xai-grok-pager/src/minimal/api.rs marks 10 functions as #[cfg(test)] but they are called from separate crate xai-grok-pager-minimal's test code | P3 | OPEN |
| BL-WIN-CLIPPY-001 | WIN | clippy --all-targets -D warnings | xai-grok-config (lib) | clippy | type_complexity: Mutex<Option<Vec<(String, Vec<u8>)>>> in signed_policy.rs:75 | Introduced by signed-policy integration seam (ISSUE.md C01 fix). Type alias needed | P3 | OPEN |
| BL-WIN-TEST-001 | WIN | test --all-targets --no-run | multi-crate cascade | compile-contract | E0463/E0432 cascading from check failures | ~182 errors across shell, pager, pager-minimal, update — mostly cascade | P3 | OPEN |

## Unpopulated Baselines (CI Required)
- BL-LNX-CHECK-*
- BL-LNX-CLIPPY-*
- BL-LNX-TEST-*
- BL-MAC-CHECK-*
- BL-MAC-CLIPPY-*
- BL-MAC-TEST-*
- BL-*-DOCS-* (all platforms)
- BL-*-PACKAGE-* (all platforms)

## Notes
- Windows test build completed with 182 errors, but many are cascading from the check-level E0425 errors
- Disk dropped from 33.6 GB to 6.4 GB after three workspace commands
- Full workspace test run, docs, and package verification deferred to CI
- CI workflow dispatched at <TBD> — requires GitHub Actions trigger
