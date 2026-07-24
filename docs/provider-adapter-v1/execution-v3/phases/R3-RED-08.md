# R3-RED-08 Evidence

- Files changed:
  - `crates/codegen/xai-grok-shell/tests/provider_catalog_lifecycle.rs` (new)
- Targeted check: ✅
- Targeted test: 5 tests, 4 pass, 1 fails (`provider_deletion_emits_revision_notification`)
- Failure detail: revision stays 0 after provider deletion via `refresh_changed` — deletion does not increment revision
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
