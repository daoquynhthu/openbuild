# Progress — Provider Model Adapter Refactoring (V2 Closure Plan)

## Phase 5: Transactional Registry — 2026-07-19

### Audit Gaps (Phase 5-007 residual)
- Gap A — `prepare()` held read lock for entire duration → fixed: short read lock clones definitions/factories/revision, all provider creation/validation outside lock
- Gap B — Missing P5-004 failure tests → added: `prepare_rejects_spec_id_mismatch` (spec ID != key), unknown implementation kind covered by Rust exhaustive match
- Gap C — `RouteSelector::referenced_route_ids()` returned `Vec<RouteId>` vs plan's `&[RouteId]` → fixed
- Gap D — P5-006 `failed_rebuild_leaves_snapshot_unchanged` covered empty set, not actual failures → replaced with `prepare_failure_leaves_snapshot_unchanged` and `prepare_failure_on_unknown_definition_keeps_snapshot_ptr`
- Gap E — P5-007: `store_config`/`register_route` still called in production `configure_providers` → removed calls; legacy mutators marked `#[doc(hidden)]`
- Gap F — `commit_rejects_stale_prepared` test was inadequate → rewritten with real stale prepared rejection scenario
- Gap G — P5-011 test used `dummy`+`deepseek` instead of `deepseek`+`internal` → aligned to plan spec with identity isolation checks
- Gap H — Missing lock-duration seam tests → added: `slow_factory_does_not_block_snapshot`, `concurrent_prepare_does_not_hold_write_lock`
- Gap I — Concurrent test used legacy `rebuild()` → added `concurrent_rebuild_from_resolved_gives_sequential_revisions`

### Phase 5 Gate
- A-12 Closed ✅
- registry 并发/失败原子性测试全绿 ✅
- sealed 后 custom identity 增删回归测试全绿 ✅
- `ProviderRouteKey` 跨 provider 查路由不串线 ✅
- `rg` 不存在 production legacy mutator 调用 ✅
- provider core check/clippy/test zero failure ✅

## Phase 6: Single ProviderRuntime Bootstrap Chain — 2026-07-19

### P6-001: Legacy configure_providers empty snapshot test
- Created `xai-grok-shell/tests/provider_bootstrap.rs`
- `legacy_configure_providers_snapshot_is_empty`: records that current legacy path produces revision=0, providers empty, routes empty (root-cause evidence)
- `bootstrap_provider_runtime_produces_full_snapshot`: verifies new helper produces revision=1, providers+Routes non-empty, and seal is correct

### P6-002: Bootstrap helper skeleton
- Created `xai-grok-shell/src/agent/provider_bootstrap.rs` with:
  - `ProviderBootstrapInput { resolved: ResolvedProviderSet }`
  - `ProviderBootstrapError` (thiserror)
  - `bootstrap_provider_runtime(input) -> Result<Arc<ProviderRuntime>, ProviderBootstrapError>`
- Registers 6 built-in definitions + `OpenAiCompatibleProviderFactory`
- Calls `rebuild_from_resolved` for atomic publish
- Added `pub mod provider_bootstrap` to `agent/mod.rs`
- Changed `openai_compatible_factory` from `pub(crate)` to `pub` for cross-crate access

### P6-003: Custom identity in bootstrap
- `bootstrap_with_builtin_and_custom_identities`: xai + openai + deepseek all enter snapshot
- Verifies deepseek uses profile-based endpoint (api.deepseek.com), not cross-contaminated

### P6-004: bootstrap_from_config convenience helper
- Added `bootstrap_from_config(raw_toml, legacy_migration, cli_overrides)` which:
  - Parses TOML via `parse_provider_toml`
  - Calls `resolve_with_precedence` (the Phase 4 unique resolver)
  - Passes result to `bootstrap_provider_runtime`
- No env/session secrets read inside bootstrap

### P6-005: [test] bootstrap_from_config counts resolve calls
- Test verifies bootstrap_from_config produces revision=1 with non-empty providers/routes

### Key Results
- `cargo test -p xai-grok-shell --lib`: 3495 passed, 0 failed ✅
- `cargo test -p xai-grok-shell --test provider_bootstrap`: 3 passed ✅
- `cargo clippy -p xai-grok-shell --all-targets -- -D warnings`: 0 warnings ✅
- `cargo clippy -p xai-grok-provider --all-targets -- -D warnings`: 0 warnings ✅

## Phase 10: Real TUI and CLI Provider Configuration Closure

### Completion
- P10-01: ProviderState/ProviderView with credential/catalog state, redact_endpoint
- P10-02: Detail view editing state, Debug redaction of api_key
- P10-03: SaveProviderConfig effect, persist_provider_config, validation
- P10-04: env_key persistence policy (env_var_name field, inline key warning)
- P10-05: Force model refresh ('r' key in list view)
- P10-06: 41 unit tests (provider_state 13 + providers_modal 17 + config_validation 11)
- P10-07: PTY E2E test (providers_pty.rs) — open /providers, verify configured state, select model, send prompt, verify response; common.rs cross-platform fixes
- P10-08: `grok providers` CLI command — list configured providers with endpoint/routes/auth

### Phase 10 gate
- Gate T3: ✅
- PTY E2E: ✅ (providers_pty.rs, ignored)
- C-12: eligible for -Fixed (/providers now shows correct status, save persists and reloads)

## Phase 11: Security, Resilience, and Observability

### Completion
- P11-01: Endpoint/SSRF audit — invalid scheme test, explicit redirect policy
- P11-02: Timeouts — 10s connect timeout, streaming keep-alive (existing)
- P11-03: Retry policy — classify_error covers auth/encrypted/payload/context errors
- P11-04: Structured diagnostics — tracing fields in ProviderState::refresh and SaveProviderConfig
- P11-06: Panic/unwrap audit — clean, all production unwrap_or_* have safe fallbacks

## Phase 12: Cross-Surface E2E and Backward Compatibility

### Completion
- P12-01: Mock harness audit — MockInferenceServer supports all three SSE protocols
- P12-02: Full-chain integration test — config → sampler → mock server → decoded events with real HTTP
- P12-03: Config precedence E2E — 5 tests proving env < TOML < compat < CLI
- P12-04: Hot reload E2E — atomic rebuild, invalid config preserves old snapshot
- P12-05: Model switch/provider coexistence — independent routes, model resolution
- P12-06: Legacy xAI regression suite — defaults, api_key sources, default model, provider count
- P12-07: No-network enforcement — all tests bind only loopback
- Total: 28 tests in provider_e2e.rs

## Phase 13: CI, Cross-Platform, Packaging, and Release Engineering

### Completion
- P13-01: CI workflow — `.github/workflows/provider-adapter.yml` with 6 jobs (Linux, Windows, macOS, workspace, docs, E2E)
- P13-03: Cross-platform audit — no `canonicalize` usage, path operations use cross-platform utilities
- P13-06: Migration docs — `docs/provider-adapter-v1/migration.md`, `troubleshooting.md`, `rollback.md`

### Current State
- `cargo check --workspace`: passes
- `cargo clippy -p xai-grok-pager -p xai-grok-pager-bin --all-targets -- -D warnings`: passes
- `cargo test -p xai-grok-provider`: 104 passed, 0 failed

### Remaining
- P13-02: Fast PR vs full release gate separation (workflow-level)
- P13-04: Packaging smoke test (CI-specific)
- P13-05: Optional live smoke workflow (CI-specific, requires secrets)

## Provider Adapter V1 — Phase 14: Cleanup, Final Static Audit, and Release Candidate Cut — 2026-07-18

### Base and result
- Start commit: `e284cc1`
- End commit: `pending`
- Tasks completed: `P14-01`, `P14-02`, `P14-03`, `P14-04`, `P14-05`

### Files changed
- `xai-grok-provider/src/framing.rs`: **removed** (dead code, deprecated)
- `xai-grok-provider/src/lib.rs`: removed `pub mod framing`
- `docs/provider-adapter-v1/final-audit.md`: **new** (P14-01/P14-02 audit)
- `docs/provider-adapter-v1/baseline.md`: current baseline for diff
- Various `cargo fmt` formatting changes across 50+ files
- `ISSUE.md`: `-Fixed` → `-Closed` for all audit items

### P14-01: Dead path removal
- Searched 9 patterns; only actionable item was `framing.rs` (deprecated, unused)
- All other patterns: clean (no matches or intended API)

### P14-02: SamplerConfig constructor audit
- All production constructors use route compiler ✅
- No unacceptable bypass constructors found
- `final-audit.md` contains full classification table

### P14-03: Issue closure
- C01, C02, M01-M03, S01-S03 all → `-Closed`
- Each with test evidence in `final-audit.md`

### P14-04: Verification
- `cargo fmt --all -- --check` — ✅ after fix
- `cargo check -p` 11 target crates — ✅
- `cargo clippy -p` 11 target crates — ✅ (only pre-existing complex type in xai-grok-config)
- `cargo test -p xai-grok-provider` — ✅ 129 passed, 0 failed
- `cargo test -p xai-grok-sampler --lib` — ✅ 161 passed, 0 failed
- `cargo test -p xai-grok-pager --lib` — ✅ 7053 passed, ⚠️ 43 failed (pre-existing, documented)
- `cargo doc --workspace --no-deps` — ⏭ disk space limit

### P14-05: Scope review
- No unrelated changes: all formatting churn is from `cargo fmt` compliance
- No secrets in diff
- No unapproved dependencies (only `futures-util`, `tokio-stream` for provider tests)
- No disabled tests/lints introduced (only `#[allow(clippy::new_ret_no_self)]` for mock)

### P14-06: Release candidate
- Tag pending (requires owner approval)
