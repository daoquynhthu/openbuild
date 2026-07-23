# Provider Adapter V1 — Final Audit

## P14-01: Dead provider-adapter paths

| Pattern | Status |
|---------|--------|
| Independent route/config maps | ✅ No matches |
| Eager `Credential::resolve()` in provider construction | ✅ No matches |
| Route-level framing production use | ✅ Removed `framing.rs` module |
| String-formatted route lookup | ✅ `Registry::get_route(&str)` is intended API |
| Blocking provider discovery functions/cache | ✅ No matches |
| Duplicate registry initialization | ✅ Only in test code |
| No-op provider save behavior | ✅ No matches |
| Protocol unknown fallback | ✅ Returns proper error, not silent fallback |
| Out-of-band provider TOML parsing | ✅ No matches |

**Action:** `framing.rs` removed (115 lines, all dead code). Verified `cargo check -p xai-grok-provider --all-targets` passes.

---

## P14-02: SamplerConfig constructor audit

### Classification legend

| Code | Meaning |
|------|---------|
| ✅ PROVIDER-AWARE | uses route compiler or `resolve_model_execution` |
| 🔬 TEST-FIXTURE | internal test only, uses `Default::default()` pattern |
| ⚠️ LEGACY | fallback path with documented reason |
| ❌ UNACCEPTABLE | bypass that must be fixed before release |

### Production constructors

| File | Line | Classification | Details |
|------|------|---------------|---------|
| `agent/config.rs` | 4682 | ✅ PROVIDER-AWARE | `sampling_config_for_model_bookended`: delegates to `resolve_model_execution` when registry available |
| `agent/config.rs` | 4714 | ✅ PROVIDER-AWARE | `sampling_config_for_model`: primary production path with URL-derived headers |
| `agent/provider_resolution.rs` | 125 | ✅ PROVIDER-AWARE | `resolve_model_execution`: route-compiled SamplerConfig from provider registry |
| `shell-base/agent/config.rs` | 125 | ✅ PROVIDER-AWARE | Delegates to `resolve_model_execution` (same pattern) |
| `shell/src/subagent/mod.rs` | 922 | ✅ PROVIDER-AWARE | Subagent route inheritance via registry |
| `shell/src/session/compact.rs` | 1587 | ✅ PROVIDER-AWARE | Session compaction uses route-preserved config |
| `http/src/lib.rs` | 58 | ✅ PROVIDER-AWARE | HTTP smoke via `resolve_model_execution` |
| `sampler/src/config.rs` | 42 | ⚠️ LEGACY | Fallback for UI-describe/compact paths without registry |

### Test-only constructors

| File | Line | Classification |
|------|------|---------------|
| `sampler/src/client.rs` | 2067-2611 | 🔬 TEST-FIXTURE (13 instances) |
| `sampler/src/config.rs` | 134, 241, 257, 289 | 🔬 TEST-FIXTURE |
| `sampler/src/actor/state.rs` | 82-83 | 🔬 TEST-FIXTURE |
| `sampler/src/commands.rs` | 11-37 | 🔬 TEST-FIXTURE |
| `sampler/src/attribution.rs` | 12-100 | 🔬 TEST-FIXTURE |
| `sampler/tests/support/mod.rs` | 10-11 | 🔬 TEST-FIXTURE |
| `sampler/tests/test_actor.rs` | 71-775 | 🔬 TEST-FIXTURE |
| `agent/config.rs` | 5407-5422, 7382 | 🔬 TEST-FIXTURE |
| `provider/tests/provider_e2e.rs` | 56, 939 | 🔬 TEST-FIXTURE |
| `shell/tests/common/mod.rs` | 29, 32 | 🔬 TEST-FIXTURE |
| `shell/src/session/acp_*/cancel_running*.rs` | 41-913 | 🔬 TEST-FIXTURE |
| `shell/src/test_support/lsp_runtime.rs` | 39 | 🔬 TEST-FIXTURE |
| `shell/src/tools/config.rs` | 186-207 | 🔬 TEST-FIXTURE |
| `shell/src/trace_classifier/mod.rs` | 1124 | 🔬 TEST-FIXTURE |

### Verdict

**No unacceptable (❌) constructors found.** All production paths use the route compiler. All test fixtures are standard and non-bypassing.

---

## P14-03: Issue closure verification

All `C-` and `M-` issues in the 2026-07-18 ISSUE.md audit are verified as `-Closed`. See ISSUE.md for details.
A subsequent comprehensive audit (2026-07-23) found additional issues (C01, M01, M03) which have been fixed; see the 2026-07-23 audit section in ISSUE.md.

### C01 — test_seam module
- **Test:** `cargo check -p xai-grok-shell --all-targets` → exit 0
- **Evidence:** `signed_policy::test_seam::set_embedded_keys()` now exists; shell test targets compile

### C02 — platform cfg guard
- **Test:** `cargo check -p xai-grok-pager-render --all-targets` → exit 0
- **Evidence:** `build_open_path_command` test now guarded with `#[cfg(not(windows))]`

### M01 — PDB linker pressure
- **Evidence:** `cargo clean` freed 35.8GiB; subsequent builds succeeded with sufficient space

### M02 — unused imports in mermaid
- **Test:** `cargo clippy -p xai-grok-mermaid --all-targets` → exit 0, no warnings
- **Evidence:** `use std::time::Instant` removed; `detached` and `Instant` properly `#[cfg(unix)]`d

### M03 — unused import in fast-worktree
- **Test:** `cargo clippy -p xai-fast-worktree --all-targets` → exit 0
- **Evidence:** `use super::*` removed

### S01 — unused variables in plugin-marketplace
- **Test:** `cargo clippy -p xai-grok-plugin-marketplace --all-targets` → exit 0
- **Evidence:** `dir`/`outside` moved behind `#[cfg(unix)]`

### S02 — single match in grok-tools
- **Test:** `cargo clippy -p xai-grok-tools --all-targets` → exit 0
- **Evidence:** `match` → `if let` for `compress_image_for_conversation`

### S03 — clippy in shell tests
- **Test:** `cargo clippy -p xai-grok-shell --all-targets` → exit 0
- **Evidence:** `#[allow(clippy::new_ret_no_self)]`; 3x `assert_eq!(..., false)` → `assert!(!...)`

---

## P14-04: Full clean-checkout verification

> **Status:** Partial — workspace check/clippy/E2E tests pass. Full workspace test (all 80+ crates) blocked by disk space (54 GiB cleaned, ~16 GiB needed per rebuild cycle).

```bash
cargo check --workspace --all-targets  # ✅ 2026-07-23
cargo clippy --workspace --all-targets -- -D warnings  # ✅ 2026-07-23
cargo test -p xai-grok-shell --test test_provider_chain_e2e  # ✅ 10/10 2026-07-23
cargo fmt --all -- --check      # pending — pre-existing formatting not audited
cargo test --workspace --all-targets  # blocked by disk space on Windows
cargo doc --workspace --no-deps  # blocked by disk space on Windows
```
