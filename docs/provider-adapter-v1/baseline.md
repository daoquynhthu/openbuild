# Provider Adapter V1 — Baseline Manifest

## Metadata

- **Date:** 2026-07-17
- **Base commit:** `e284cc1` (`fix: TUI provider registry - OnceLock init in app::run(), live data to providers modal`)
- **Branch:** `feat/provider-adapter`
- **Platform:** Windows (x86_64-pc-windows-msvc)
- **No source behavior was changed in this phase.**

## Toolchain

| Tool | Version |
|------|---------|
| rustc | 1.92.0 (ded5c06cf 2025-12-08) |
| cargo | 1.92.0 (344c4567c 2025-10-21) |
| rustfmt | 1.8.0-stable |
| clippy | 0.1.92 |
| protoc | libprotoc 35.0 |

## Working-tree Changes (pre-existing, preserved)

- `crates/codegen/xai-grok-pager/src/views/providers_modal.rs` — 43 insertions, 2 deletions; adds `models: Option<usize>` field, Enter-save handler, `save_provider_config()` fn.

## Baseline Gate Results

### xai-grok-provider (lib + tests)

- `cargo test -p xai-grok-provider --all-targets` — **FAIL** (exit code 1)
  - `E0433` — undeclared `Model` type at `registry.rs:156` (`Model::make(...)`)
  - `E0599` — no `model()` method on `ProviderRegistry` at `registry.rs:222`
  - Matches audit items **C-01**.
- `cargo check -p xai-grok-provider --all-targets` — same 2 errors (lib compiles, only test targets fail).

### xai-grok-sampler (lib + tests)

- `cargo check -p xai-grok-sampler --all-targets` — **PASS** (exit 0).

### xai-grok-shell (lib + tests)

- `cargo check -p xai-grok-shell --all-targets` — **FAIL** (exit code 1, 32+ errors in test targets)
  - `E0063` — missing field `provider_id` in `ModelEntry` initializers (25+ sites across `config.rs`, `models.rs`, `subagent/tests/mod.rs`, session tests).
  - `E0061` — `sampling_config_for_model()` now requires 7th argument `Option<&Route>` (6 call sites updated in config.rs but callers not yet fixed).
  - `E0061` — `resolve_mcp_liveness_watchers()` / `resolve_mcp_auto_restart()` / `resolve_mcp_push_server_status()` / `resolve_mcp_recursive_config_watch()` called with 6 args but only accept 5 (4 call sites).
  - `E0433` — `signed_policy::test_seam` not found in `tests/signed_managed_config/common.rs`.
  - Pre-existing; scope matches audit items **C-01**, **M-01**, **M-02**.

### xai-grok-pager (lib + tests)

- `cargo check -p xai-grok-pager --all-targets` — **TIMEOUT** at 5 min (build still in progress, prior dependency compilation failures observed).

### xai-grok-pager-bin (lib + tests)

- `cargo check -p xai-grok-pager-bin --all-targets` — **TIMEOUT** at 5 min.

## Known Audit IDs (from plan, all confirmed present)

| ID | Defect | Present |
|----|--------|---------|
| C-01 | Provider registry tests call missing `model()` | Confirmed (both `Model::make` and `registry.model()` error) |
| C-02 | Route is not authority for production request | Presumed (no pass evidence) |
| C-03 | OpenAI Responses route not registered/selected | Presumed |
| C-04 | Route defaults/headers don't reach sampler | Presumed |
| C-05 | Model discovery ignores base URL | Presumed |
| C-06 | Provider model cache no TTL / synchronous blocking | Presumed |
| C-07 | `env_key`/`extra_headers` accepted not authoritative | Presumed |
| C-08 | Auth failures swallowed | Presumed |
| C-09 | OpenCode public/free mode not closed | Presumed |
| C-10 | Named OpenAI-compatible profiles unreachable | Presumed |
| C-11 | Protocol dispatch fixed switch with silent fallback | Presumed |
| C-12 | `/providers` false status, save no-op | Presumed |
| M-01 | Valid `[provider.*]` reported as unknown | Presumed |
| M-02 | Registry lifecycle unreliable | Presumed |
| M-03 | Provider/model ordering nondeterministic | Presumed |

## Unavailable Platforms/Tools

- macOS: not tested (Windows host only).
- CI: not available locally.

## Explicit Statement

No source behavior was changed. All output above is from `e284cc1` without modification.
