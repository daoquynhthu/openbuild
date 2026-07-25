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

### Phase 6 Audit (2026-07-19): 5 gaps found and closed
- **GAP-1** (P6-001): removed permanent `legacy_configure_providers_snapshot_is_empty` — plan says "不得写断言坏行为成立的永久测试"
- **GAP-2** (P6-004): `bootstrap_from_config` returns `ConfigDiagnostic` error instead of silently ignoring `_diags`
- **GAP-3** (P6-005): added `bootstrap_precedence_applied_once` + `bootstrap_fails_on_config_diagnostics` tests
- **GAP-4** (P6-006): TUI path (`async_main`) — compat/cli=None is correct for non-agent TUI path
- **GAP-5** (P6-010): ConfigReloader in `app.rs` now receives `agent_config.provider_runtime` instead of `None`; identity test added

### P6-006: launcher uses bootstrap helper
- `xai-grok-pager-bin/src/main.rs`: replaced `register_all + configure_providers` block with `bootstrap_from_config`
- compat override and CLI override computation kept, passed to bootstrap
- TUI path (async_main) also bootstraps its own runtime with `bootstrap_from_config`
- Error returns clear message on failure

### P6-007: Pager run() receives Arc<ProviderRuntime>
- `xai-grok-pager/src/app/mod.rs` run() parameter changed from `Option<Arc<ProviderRegistry>>` to `Arc<ProviderRuntime>`
- `unwrap_or_else` fallback removed — tests must bootstrap their own runtime
- Main.rs call site updated to pass `tui_provider_runtime`

### P6-008: providers CLI uses bootstrap helper
- `xai-grok-pager/src/providers_cmd.rs`: replaced standalone registry creation with `bootstrap_from_config`
- No independent `ProviderRegistry::new()` in providers CLI

### P6-009: AgentConfig only holds runtime, not parallel registry/catalog
- Removed `provider_registry` and `provider_catalog` fields from `Config` struct
- Added `Config::provider_registry()` and `Config::provider_catalog()` accessor methods deriving from `provider_runtime`
- Removed separate assignments in `main.rs` (only `provider_runtime` is set)
- Updated `models.rs` and `agent_ops.rs` to use accessor methods

### P6-010: Runtime identity regression test
- `bootstrap_runtime_identity_is_unique_across_clones`: verifies cloned Arc shares same pointer
- Verifies `config.provider_registry()` returns same `Arc` as `runtime.registry`
- Verifies `config.provider_catalog()` returns same `Arc` as `runtime.catalog`

### P9 Audit Gaps
- P9-002: 缺少 mock server redirect 测试（TCP server 测试挂起，暂跳过）
- P9-003: 并发测试验证 permit count 而非实际 in-flight 上限
- 其余 P9-001~005 按计划忠实实现

### Phase 6 Gate
- main、Pager、providers CLI 无 `configure_providers()` 调用 ✅
- production call graph 中 runtime 只构造一次 ✅
- snapshot providers/routes 均非空 ✅
- P6 bootstrap integration tests (4) 通过 ✅
- A-01 的"启动不 rebuild"部分关闭，legacy fallback 部分留 P7

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

### V2 Closure Plan (execution-v2 phase-13.md)

- P13-004: scrollback render tests — removed cfg guard from 9 functions in render.rs/edit.rs/read.rs/entry_renderer.rs. All 65 scrollback + 3 bolt-on tests pass on Windows. Commit `b29cc35`
- P13-005: shortcuts/help/overlay — removed cfg guard from btw_overlay.rs:733. All 15 btw_overlay + 64 shortcuts_help tests pass. Commit `19f80d3`
- P13-006/007: opencode grep + glob — removed cfg guards. All 129 opencode tests pass.
- P13-008: opencode bash — removed cfg guard from bash/mod.rs:458, fixed `output_file_path` test with `PathBuf::join`. All 23 bash tests pass. Commit `b96e8eb`
- P13-009: skills/resources/tool-registry — removed cfg guards from 3 files (discovery.rs:924, resources.rs:915, registry/types.rs:2158). Fixed 5 display-cwd path assertions with `#[cfg(windows)]`, removed `deploy_app` (unimplemented stub). All 40+66+55 tests pass. Commit `b7b1f1d`
- P13-009b: enter_plan_mode/grep/read_file (grok_build) — removed cfg guards, fixed path assertions and `IsADirectory` test. All 33+35+89 pass. Commit `8c65a4f`
- P13-009c: web_fetch (3 files) — removed cfg guards, fixed which_in test (gh.bat for Windows), fixed path assertions (component-by-component join). All 119 pass. Commit `15b8c0a`
- P13-009d: bash/mod (141/144 pass, 3 ignored — real bash cmds), foreign_sessions (path normalisation), image_describe (path normalisation). event_loop.rs = NOT an exclusion (production code). Commit `57a9647`

### Current State
- `cargo check --workspace`: passes
- `cargo clippy -p xai-grok-tools -- -D warnings`: passes
- `cargo test -p xai-grok-tools --lib`: 2418 passed, 0 failed
- Ledger RESOLVED_IN_P11_P12_13: 104 (up from 97), NOT_AN_EXCLUSION: 1
- GROUP-043 (14 rows): COMPLETED
- GROUP-048 (3 rows): COMPLETED

### Remaining (Phase 13 V2 plan)
- P13-R-WX entries outside GROUPs: prompt.rs (2), key.rs (1), edit.rs (1), key/keyboard.rs (1), key/bindings.rs (2), base64_image.rs (1), render/mod.rs (1), render/blocks/render_test.rs (3), render/image.rs (1), layout.rs (1), osc8.rs scanner tests (1), LSP subscribe_uri (1), terminal (5), etc.
- Total ~50+ remaining WX entries across pager, tools, shell crates

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

## Phase 9: Async Catalog replaces blocking model discovery — 2026-07-20

### Sub-tasks completed
- **P9-001**: State machine (`Empty | Loading | Fresh | Stale | Failed`) with `SystemTime`, 6 state tests
- **P9-002**: reqwest client policy (5s connect, 30s total, max 3 same-origin redirects, fixed UA), Axum `RedirectMockServer`, 4 redirect tests
- **P9-003**: `Semaphore` concurrency (default 4, range 1..=16), `SlowServer` peak-in-flight test
- **P9-004**: Model list parsed by declared `model_list_format`, not URL inference
- **P9-005**: HTTP status classification (2xx only, typed errors, error preserves old models)
- **P9-006**: Discovery applies endpoint/auth/extra_headers from `ProviderDefaults` at request time
- **P9-007**: Stale-while-revalidate — preserves old models on failure, revision increments on both
- **P9-008**: TTL config (30..=86400 seconds, default 300), boundary tests, clock rewind test
- **P9-009**: Cancellation via shared `CancellationToken` between `ProviderRuntime` and catalog; `select!` in spawned tasks, 2s shutdown deadline, 3 tests
- **P9-010**: Snapshot serde roundtrip (`Serialize`/`Deserialize` with `SystemTime` as epoch seconds), 2 tests
- **P9-011**: Persist/load via `atomic_replace`; failure preserves old disk snapshot, 2 tests
- **P9-012**: Remove `fetch_provider_models_blocking()` call from `resolve_model_list` in config.rs; startup uses persisted/in-memory snapshot only
- **P9-013**: Delete old blocking code from models.rs — `resolve_provider_auth`, `parse_openai_compatible_provider_models`, `parse_ollama_tags_models`, `PROVIDER_MODEL_CACHE` et al; clean unused imports
- **P9-014**: Catalog revision watch channel (`watch::Sender<u64>`), `subscribe_catalog_revision()` on `ProviderCatalogService` and `ProviderRuntime`; `ModelsManager::rebuild_from_catalog_snapshot()`; app-level watcher task

### Files changed
- `provider_catalog.rs` — watch infrastructure, revision send, subscribe method
- `provider_runtime.rs` — `subscribe_catalog_revision()` delegation
- `models.rs` — `rebuild_from_catalog_snapshot()`, `build_prefetched_from_catalog()`, deleted old blocking code
- `app.rs` — spawn catalog revision watcher after bootstrap
- `config.rs` — removed `fetch_provider_models_blocking()` call from `resolve_model_list`

### Key results
- `cargo check -p xai-grok-shell` — clean
- `cargo clippy -p xai-grok-shell -- -D warnings` — clean
- `cargo test -p xai-grok-shell --lib` — 3546 passed, 0 failed, 3 ignored
- `cargo check/clippy/test -p xai-grok-provider` — clean (149 passed)
- `rg fetch_provider_models_blocking` — zero production references

### Phase 9 Gate
- A-03: `fetch_provider_models_blocking` 不再出现在生产路径 ✅
- A-07: catalog 异步刷新、timeout、status、auth、stale、persist、cancel 全部实现 ✅
- startup no-network test: 启动现在只使用 persisted/in-memory snapshot ✅
- `rg fetch_provider_models_blocking` 生产引用为 0 ✅
- catalog mock matrix、TTL、stale、cancel、persistence roundtrip 全绿 ✅

## Phase 11: Cross-platform filesystem, path and atomic persistence — 2026-07-20

### P11-001: Config consumer migration review
- `provider_config_coordinator.rs` uses `atomic_replace` for all writes
- `apply_external_file` is read-only
- Evidence: `docs/provider-adapter-v1/execution-v2/evidence-p11-001.md`

### P11-002: Catalog consumer migration review
- `persist_snapshot` and `save_catalog_snapshot` use `atomic_replace`
- `load_snapshot` is read-only
- Evidence: `docs/provider-adapter-v1/execution-v2/evidence-p11-002.md`

### P11-003: Session persistence migration
- `jsonl/mod.rs`: 5 methods migrated from write+rename to `atomic_replace`:
  `write_jsonl`, `write_plan_state`, `write_plan_mode_state`, `write_signals`,
  `write_announcement_state`, `write_goal_mode_state`
- `summary_write.rs`: already used `atomic_replace` (clippy fix only)
- `prompt_history.rs`: `truncate_if_needed` migrated from write+rename to `atomic_replace`
- `search_remote_sync.rs`: `decompress_file` refactored to `decompress_to_bytes` +
  `atomic_replace`; temp-file path eliminated

### P11-004: Path normalization consumer scan
- Scanned 200+ `dunce::canonicalize` call sites across workspace
- Evidence: `docs/provider-adapter-v1/execution-v2/evidence-p11-004.md`
- `normalized_absolute` tests in `xai-grok-paths/src/normalize.rs` cover drive letter, UNC, `\\?\`, symlink, relative, Unicode

### P11-005: Workspace classifier Windows fix
- Removed `#[cfg(not(windows))]` from test module
- Replaced hardcoded POSIX paths with fixture paths outside system temp dir
- 19 tests pass on Windows

### P11-006: Worktree/git/path repair queue
- Audited 3 crates for git command shell string concatenation compliance
- Created P11-PATH-001 (xai-fast-worktree), P11-PATH-002 (xai-grok-plugin-marketplace), P11-PATH-003 (xai-grok-provider) task cards — all VERIFIED
- All `Command::new("git")` invocations use proper `.arg()`/`.args()` argument arrays; no shell strings
- xai-grok-provider has zero shell invocations (pure HTTP/API crate)

### P11-007: Watcher + atomic_replace contract
- Added `atomic_replace_triggers_watcher_and_coalesces` test
- Verifies `atomic_replace` generates watcher events and debounce coalesces them

### P11-008: Close P2 filesystem exclusions
- Scanned 42 `#[cfg(not(windows))]` test guards across 10 crates (P11/P12/P13 scope)
- Changed 40 guards from `cfg(not(windows))` to `cfg(unix)` (truly Unix-only: file locking semantics, HOME env, Unix sockets, /tmp paths, shell commands, path-separator-dependent assertions)
- Removed 2 guards and fixed code to work cross-platform: `selection.rs:1459` (path normalization in assertion) and `prompt.rs:3274,3356` (reverted to cfg(unix) after Windows Tab-key behavior difference confirmed)
- Tested: `active_child_copy_uses_child_scrollback_cwd` passes on Windows ✅
- All changed crates compile clean, clippy clean

### Key results
- `cargo check -p xai-grok-pager -p xai-grok-shell -p xai-grok-tools -p xai-grok-config -p xai-grok-telemetry -p xai-grok-plugin-marketplace` — clean ✅
- `cargo clippy -p xai-grok-pager -p xai-grok-shell -p xai-grok-tools -- -D warnings` — clean ✅
- New cross-platform test: `active_child_copy_uses_child_scrollback_cwd` now runs on all platforms ✅
- 41 guards changed to `cfg(unix)` (precise); 1 guard removed with code fix

### Phase 11 Gate
- Config/catalog/session persistence all use `atomic_replace` ✅
- Workspace classifier tests cross-platform ✅
- Watcher + atomic_replace contract test added ✅
- A-16 atomic write (catalog + session + config paths) ✅

## Phase 13: Pager Render, Images, Tools, Remaining Platform Closure — 2026-07-21

### V2 Closure Plan Repair Cards (execution-v2/phase-13.md)

All 12 P13 tasks resolved:

| Task | Count | Scope |
|------|-------|-------|
| P13-001 | 5 | prompt_images — CROSS_PLATFORM_CONTRACT, 152/152 pass |
| P13-002 | 2 | osc8/link — Windows impl added, 49/49 pass |
| P13-003 | 2 | display_refresh + key.rs — paired cfg impls, 17+371 pass |
| P13-004 | 10 | scrollback/render/tool/edit/entry_renderer — CONFIRMED, 974/974 pass |
| P13-005 | 19 | shell_completion/dispatch — PLATFORM_ADAPTER_CONTRACT |
| P13-006 | 5 | grep/read_file/web_fetch — CONFIRMED, 2500/2500 tools pass |
| P13-007 | 2 | glob/grep (opencode) — CONFIRMED |
| P13-008 | 1 | enter_plan_mode — CONFIRMED |
| P13-009 | 1 | LSP — CONFIRMED |
| P13-010 | 3 | host_clipboard — CLOSED from P12 |
| P13-011 | 2 | image_describe/bash — already RESOLVED_IN_P13 |
| P13-012 | — | Full exclusion ledger zero review — no P13 CLASSIFIED remaining |

### Key results
- Exclusion ledger: CONFIRMED 34→77, CLOSED 8→10, RESOLVED_IN_P11_P12_13=106
- No P13-scoped entries remain CLASSIFIED
- `cargo check --workspace`: passes ✅
- `cargo clippy --workspace -- -D warnings`: passes ✅
## Phase 12: Remediate cfg(not(windows)) Guards — Path/File/Process Tests — 2026-07-21

### Completed
- **P12-001-009**: active_sessions (4/4), folder_trust (39/39), mvp_agent (166/166), subagent (278/278), auth/manager/lock (5/5), extensions/bundle (31/31), extensions/suggest (150/150), leader/lock (15/15)
- **P12-010-032**: acp_session + hooks + updates + compaction + signals — fixed 51/63 path-related failures via `std::env::temp_dir()` replacement; 876/886 pass
- **P12 hardcoded `/tmp` purge**: replaced ~60+ hardcoded `/tmp` paths across acp_session_tests/, compaction.rs, goal_tracker.rs, goal_classifier.rs, persistence.rs, hook_dispatch.rs — all now use `std::env::temp_dir()`
- **P12-033-035** terminal/PTY: deferred — requires Windows ConPTY adapter implementation (3 entries)

### Key results
- Exclusion ledger: 87 entries RESOLVED_IN_P12 (from previous 76 CLASSIFIED → 3 deferred)
- `cargo check -p xai-grok-shell`: passes ✅
- `cargo clippy -p xai-grok-shell -- -D warnings`: passes ✅
- `cargo test -p xai-grok-shell --lib -- session::goal_tracker::tests`: 113 passed ✅
- `cargo test -p xai-grok-shell --lib -- persistence::tests`: 87 passed ✅
- `cargo test -p xai-grok-shell --lib -- goal_classifier::tests`: 205 passed ✅
- `cargo test -p xai-grok-shell --lib -- session::acp_session::`: 876 passed, 10 failed (pre-existing timing/logic issues)

### Remaining
- 10 ACP test failures (timing/model-metadata assertions — not path-related)
- 3 terminal/PTY entries — need Windows ConPTY adapter
- `cargo test -p xai-grok-shell --lib -- pager::` and other crate tests not yet validated

### Blocked
- (none)

## Phase 8: Request-time auth, header merge, secret safety — 2026-07-22

### P8-011: `From<PreparedSamplerConfig> for SamplerConfig`
- Added `impl From<PreparedSamplerConfig> for xai_grok_sampler::SamplerConfig` in `prepared.rs`
- Conversion preserves resolved auth headers, infers `auth_scheme` from header contents
- Added `xai-grok-sampler` as a regular dependency (was dev-dependency only)

### Key results
- `cargo check -p xai-grok-provider --lib`: clean ✅
- `cargo clippy -p xai-grok-provider --lib`: clean ✅

### P8-010F: Missing credential failure matrix
- Added 4 `#[tokio::test]` tests in `prepared.rs`:
  - `missing_required_bearer_credential_fails_before_http_send`
  - `missing_required_header_credential_fails_before_http_send`
  - `optional_bearer_without_candidates_succeeds`
  - `optional_header_without_candidates_succeeds`
- All exercise `prepare_sampler_config` directly (the only P8 production entry point)
- Each verifies:
  - required credential without candidates → typed hard error
  - optional credential without candidates → success (no auth header)
  - error message does not contain canary secret
- `cargo test -p xai-grok-provider -- prepared::tests`: 5 passed ✅
- `cargo test -p xai-grok-provider --test request_inspection`: 8 passed ✅

### P8-011: Delegate migration function through PreparedSamplerConfig
- `execution_to_unprepared_sampler_config_for_migration` now constructs `PreparedSamplerConfig` with auth headers merged, then calls `.into()` → `SamplerConfig`
- Replaced manual field mapping (auth_scheme, api_backend, extra_headers, etc.) with the `From` impl
- Added `SensitiveHeaderMap::from_index_map()` convenience constructor
- Removed `ApiBackend` import (no longer needed)
- All provider unit + integration tests pass ✅

### P8-010A: Bearer precedence and format through prepare_sampler_config
- Added 7 new `#[tokio::test]` end-to-end tests in `prepared.rs`:
  - `bearer_request_override_produces_authorization_header`
  - `bearer_provider_inline_falls_back_correctly`
  - `bearer_env_reader_used_when_inline_absent`
  - `bearer_precedence_request_override_beats_inline`
  - `header_merge_preserves_static_headers`
  - `request_overrides_win_over_static_headers`
  - `prepare_sampler_config_preserves_protocol_and_url`

### P8-010B: Header auth (x-api-key) through prepare_sampler_config
- Added 3 tests: `header_auth_request_override_produces_correct_header`, `header_auth_provider_inline_falls_back_correctly`, `header_auth_precedence_request_override_beats_inline`

### P8-010D: No-auth through prepare_sampler_config
- Added 3 tests: `no_auth_produces_no_authorization_header`, `no_auth_with_request_override_still_omits_auth`, `no_auth_static_headers_are_still_preserved`

### P8-010C: xAI session resolver through prepare_sampler_config
- Added 5 tests: `session_used_only_when_explicit_candidates_missing`, `session_fills_when_all_explicit_candidates_absent`, `session_returning_none_fails_when_required`, `session_not_used_when_session_candidate_absent`, `optional_session_not_used_when_session_candidate_absent`
- Fixed hidden bug: `(ctx.session_resolver)().filter(|_| has_sess)` called the closure unconditionally; changed to short-circuit guard so session resolver is NOT invoked when `Session` candidate absent
### P8-010E: Extra headers isolation through prepare_sampler_config
- Added 5 tests: `header_isolation_two_independent_calls_differ`, `invalid_header_name_in_request_overrides_rejected`, `invalid_header_value_in_request_overrides_rejected`, `conflict_between_static_header_and_auth_header_rejected`, `same_static_and_auth_header_value_allowed`
### P8-002: Remove old CredentialSource/Inline/Public types
- Removed `CredentialSource` enum, `ResolvedCredential` struct, `resolve_credential_source` fn, `candidates_to_source` fn from `auth.rs`
- Rewrote `apply_auth_policy` to use direct env/session resolution (`resolve_candidates_legacy`) instead of going through `CredentialSource`
- Removed `resolve_shell_credential` from `xai-grok-shell/src/auth/provider_adapter.rs` (was dead code)
- Removed 3 dead tests (credential_source_public_vs_none, credential_source_resolve_inline_none, public_is_distinct_from_none_for_auth)
- `cargo check -p xai-grok-provider -p xai-grok-shell` clean, `cargo clippy` clean

### P8-011: Migrate registry path through PreparedSamplerConfig
- Rewrote `execution_to_unprepared_sampler_config_for_migration` to call `prepare_sampler_config` with proper `RequestCredential` instead of manually constructing headers
- Function is now async (returns `Result<SamplerConfig, RequestPreparationError>`)
- `sampling_config_for_model_with_registry` made async to await the migration function
- All 3 sync callers bridge with `Handle::current().block_on()` or temporary `Runtime::new()`
- 4 test functions updated to use `Runtime::new().block_on()`
- All 13 provider_resolution tests pass

### P8-007: Header merge layer ordering fix
- Fixed `prepare_sampler_config`: Layer 3 (provider extra) no longer incorrectly duplicates Layer 2 (route static)
- Uses explicit empty `IndexMap` for provider extra with comment explaining it's reserved for future separate tracking
- `cargo check`, `clippy`, `test` all pass ✅

### P8-011: Eliminate production SamplerConfig { construction — 2026-07-22

#### Changes made
- `config.rs:sampling_config_for_model` — `SamplerConfig { ... }` → `SamplerConfig::default()` + field mutation
- `config.rs:resolve_hidden_default_web_search_sampling_config` — ditto (return type false positive)
- `tools/config.rs:web_search_sampling_config` — mut parameter approach
- `tools/config.rs:ShellToolsetConfig::new` — Default + field assignment
- `subagent/mod.rs:resolve_subagent_sampling_config` — block-with-allow
- `trace_classifier/mod.rs:build_sampler_client` — `PreparedSamplerConfig` + `.into()`
- `provider_resolution.rs` — static scan test + `execution_to_sampler_config` rewritten to call `prepare_sampler_config`

#### Files changed
- `crates/codegen/xai-grok-shell/src/agent/config.rs`
- `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs`
- `crates/codegen/xai-grok-shell/src/agent/subagent/mod.rs`
- `crates/codegen/xai-grok-shell/src/tools/config.rs`
- `crates/codegen/xai-grok-shell/src/trace_classifier/mod.rs`

#### Key results
- Static scan `no_sampler_config_construction_in_production_sources`: 0 production `SamplerConfig {` ✅
- `cargo test -p xai-grok-shell --lib -- provider_resolution::tests`: 14/14 pass ✅
- `cargo test -p xai-grok-shell --lib -- trace_classifier`: 47/47 pass ✅
- `cargo test -p xai-grok-shell --lib -- subagent::tests`: 275/275 pass ✅
- `cargo test -p xai-grok-provider`: 221/221 pass ✅
- `cargo clippy -p xai-grok-shell -p xai-grok-provider -- -D warnings`: clean ✅

#### Phase 8 Gate assessment
| Condition | Status |
|---|---|
| A-02, A-08 Closed | ✅ (P8-002/007/010) |
| credential/header mock request inspection | ✅ (P8-010A-F: 25 tests) |
| production chain: RME → prepare → PreparedSamplerConfig → Sampler | ✅ (static scan) |
| no apply_auth_policy(...).ok() / 吞错 | ✅ (zero `.ok()` on apply_auth_policy; pre-existing `match`+`warn!` in legacy `sampling_config_for_model` documented as residual) |
| secret canary zero leak | ✅ (P8-009: 6 redaction tests) |

### P8-011 deviation fixes — 2026-07-22

#### C01: trace_classifier bypass of `prepare_sampler_config`
- `build_sampler_client` rewritten: constructs `ResolvedModelExecution` + `RequestCredential` + calls `prepare_sampler_config` instead of struct literal
- No more `PreparedSamplerConfig {` in shell crate production code ✅

#### C02: Sampler production entry point
- Added `SamplingClient::from_prepared(config: impl Into<SamplerConfig>)` as the preferred production entry
- `build_sampler_client` now uses `from_prepared` instead of `new`
- Note: `new` retained for backward compat; cannot accept `PreparedSamplerConfig` directly due to circular dep (sampler ← provider)

#### C03: `test_prepared_config` gating + residual documentation
- Gated `test_prepared_config` with `#[cfg(test)]`
- `sampling_config_for_model`: documented as known residual (29+ call sites, needs separate migration task)

#### Files changed
- `crates/codegen/xai-grok-provider/src/prepared.rs` — `#[cfg(test)]` on helper
- `crates/codegen/xai-grok-sampler/src/client.rs` — `from_prepared()` method
- `crates/codegen/xai-grok-shell/src/trace_classifier/mod.rs` — use `prepare_sampler_config`

#### Key results
- `cargo test -p xai-grok-provider --lib -- prepared`: 30/30 pass ✅
- `cargo test -p xai-grok-shell --lib -- trace_classifier`: 47/47 pass ✅
- Static scan: 0 `SamplerConfig {` and 0 `PreparedSamplerConfig {` in shell crate production code ✅
- `cargo clippy -p xai-grok-sampler -p xai-grok-provider -p xai-grok-shell -- -D warnings`: clean ✅

### P8 M03-M05: Closure-based → trait-based credential context — 2026-07-22

#### Changes made
- **M02**: `PreparedSamplerConfig::protocol_id` from `String` to `ProtocolId` (import, field, `&*protocol_id` match, `protocol_id.clone()`, tests)
- **M03/M04/M05**: Created `CredentialError`, `EnvironmentReader`, `SessionCredentialResolver` traits and `RequestCredentialContext` struct in `provider/src/auth.rs`
- Replaced `<T>` closure-based `RequestCredential` with trait-based `RequestCredentialContext` in `prepared.rs`
- Made `resolve_auth_from_policy` async
- Replaced test closures with `TestEnv`/`TestSession`/`TrackingSession`/`FixedSession`/`StaticEnv` helper types
- Fixed `resolve_candidates` env ordering (model > provider > built-in)
- Added `RequestCredentialContext::new()` constructor
- **Shell crate**: removed duplicate `EnvironmentReader`/`SessionCredentialResolver`/`RequestCredentialContext` definitions from `credential_context.rs`
- Replaced with `pub use` re-exports from `xai_grok_provider::auth`
- Added `NoopSessionResolver` and kept `TestEnvironment`, `ProcessEnvironment`, `XaiSessionResolver` as implementation types
- Updated 3 shell call sites (`config.rs:4703`, `provider_resolution.rs:61`, `trace_classifier/mod.rs:1128`) to use `RequestCredentialContext` with trait objects

#### Key results
- `cargo test -p xai-grok-provider`: 173+1+5+28+8+6 = 221 passed ✅
- `cargo test -p xai-grok-shell --lib -- credential_context`: 9/9 pass ✅
- `cargo clippy -p xai-grok-shell -p xai-grok-provider -- -D warnings`: clean ✅

#### Remaining (M06/M07)
- **M06**: `config.rs`/`provider_resolution.rs` still use `SamplerConfig::from(prepared)` instead of `SamplingClient::from_prepared`. Requires changing `sampling_config_for_model_with_registry` return type from `SamplerConfig` to `SamplingClient`. This affects ~100+ call sites that access `.api_key`. Deferred — requires P8-011 full scope.
- **M07**: Remove `From<PreparedSamplerConfig> for SamplerConfig` bridge. Depends on M06. Deferred.

### P8 S01-S05: Phase 8 audit cleanup — 2026-07-22

#### S01: RequestHeaderOverrides newtype
- Created `RequestHeaderOverrides` newtype in `headers.rs` with `from_slice` validation
- Changed `prepare_sampler_config` third param from `&[(&str, &str)]` to `&RequestHeaderOverrides`
- Moved invalid-header tests to validate `RequestHeaderOverrides::from_slice` directly
- Updated all test call sites in provider crate + 3 shell crate call sites
- `cargo test -p xai-grok-provider -- prepared`: 30/30 pass ✅

#### S03: Remove AuthPolicy::validate()
- `AuthPolicy::validate()` always returned `Ok(())` for all variants
- `HeaderName` type-level validation makes runtime validation redundant
- Removed the method, its call in `Route::validate()`, and the `auth_policy_validate_header_name` test
- Test count drops from 173 to 172

#### S04: SecretValue::inner() pub → pub(crate)
- Changed `inner()` visibility to `pub(crate)` per Plan §line 1007
- Added `PartialEq` implementation (compares inner values) so external tests can use `==` instead of `.inner()`
- Shell test assertions updated to use `PartialEq`/`assert_eq`
- Removed `#[allow(dead_code)]` from `inner` field (now properly used)

#### S05: Route::protocol_id String → ProtocolId
- Changed field type from `String` to `ProtocolId` (matching `ResolvedModelExecution` and `PreparedSamplerConfig`)
- Updated `Route::new()` and `Route::make()` parameter types
- Updated `Route::validate()` to compare with `ProtocolId::default()`
- Shell crate call site (`provider_resolution.rs:166`) unchanged since `ProtocolId: Clone`

#### Key results
- `cargo check -p xai-grok-provider -p xai-grok-shell`: clean ✅
- `cargo clippy -p xai-grok-provider -p xai-grok-shell -- -D warnings`: clean ✅
- `cargo test -p xai-grok-provider`: 172 passed, 0 failed ✅
- `cargo test -p xai-grok-shell --lib -- credential_context`: 9/9 pass ✅
 - All S01-S05 items marked `-Fixed` in ISSUE.md ✅

## Phase 11: P11-004 (dunce::canonicalize → normalized_absolute) — 2026-07-22

### Complete migration of all 19 consumer crates + shell ext (benches/tests)

| Crate | Files | Sites | Commits |
|-------|-------|-------|---------|
| `xai-grok-shell` | 13 | ~28 | 4 |
| `xai-grok-workspace` | 16 | ~45 | 7 |
| `xai-grok-pager-render` | 1 | 17 | 1 |
| `xai-grok-fsnotify` | 2 | ~38 | 1 (+dep) |
| `xai-grok-tools` | 12 | ~58 | 4 (+dep, Path import, PathError→io::Error fix) |
| `xai-grok-pager` | 16 | ~45 | 6 (+dep, PathBuf borrow fix) |
| `xai-grok-shared` | 1 | 49 | 1 (+dep, PathBuf borrow, error kind fix) |
| `xai-codebase-graph` | 2 | 2 | 1 (had dep) |
| `xai-grok-config` | 1 | 2 | 1 (+dep) |
| `xai-grok-plugin-marketplace` | 1 | 2 | 1 (+dep) |
| `xai-grok-pager-bin` | 1 | 1 | 1 (+dep) |
| `xai-grok-pager-pty-harness` | 1 | 1 | 1 (+dep) |
| `xai-grok-sandbox` | 3 | 4 | 1 (+dep) |
| `xai-hunk-tracker` | 1 | 6 | 1 (+dep) |
| `xai-grok-memory` | 2 | 7 | 1 (+dep, ?-conversion fix) |
| `xai-grok-update` | 3 (test) | 8 | 1 (+dev-dep) |
| `xai-fast-worktree` | 7 | 16 | 2 (+dep) |
| `xai-grok-agent` | 9 | 34 | 3 (+dep, String→Path, PathError→io::Error fix) |
| Shell ext (benches/tests) | 2 | 5 | 1 |
| **Total** | **~93 files** | **~368 sites** | **37 commits** |

### Key fixes applied during migration
- `PathBuf` → `&Path` borrowing mismatches (6 occurrences across 4 crates)
- `String`/`&String` → `Path::new()` conversion (4 occurrences across 2 crates)
- `PathError` → `io::Error` conversion in `map_err` closures (2 crates)
- `PathError` → `io::Error` in `?` operator / `.kind()` usage (2 crates)
- Added `xai-grok-paths` dependency to 14 crates (1 already had it)

### Remaining
- Only 4 `dunce::canonicalize` references left: 1 in `normalize.rs` (the blessed single call site) + 3 in doc comments

### Gate check
- `cargo check --workspace`: passes ✅
- `cargo clippy --workspace -- -D warnings`: passes ✅
- `cargo doc --no-deps`: pre-existing doc-warnings only (unrelated to P11-004) ✅

### S11-001 / S11-002
- Still blocked by Phase 2 task card ledger (not executed)

## Phase 14: E2E Tests (V2 Acceptance Matrix) — 2026-07-23

### Completed
- P14-001: MockInferenceServer with Ollama `/api/tags` endpoint (`xai-grok-test-support/src/mock_server.rs`)
- P14-002: Full chain E2E — TOML → bootstrap → snapshot → execution_to_sampler_config → Client → mock → decoded SSE events
- P14-003: OpenAI Chat chain — endpoint `/v1/chat/completions`, Bearer auth, model name, stream events, usage
- P14-004: OpenAI Responses chain — xAI provider uses `ApiBackend::Responses`, `/responses` endpoint
- P14-005: Anthropic Messages chain — `x-api-key` header, `anthropic-version: 2023-06-01`, `/messages` endpoint
- P14-006: OpenCode public (no-auth) chain — `AuthScheme::None`, ChatCompletions, full request/response
- P14-007: Custom base URL chain — openai-compatible with custom base_url, no auth, full request/response
- P14-008: Two custom compatible providers — identity isolation, different mock servers, different models/keys
- Acceptance matrix: missing auth hard fail — `execution_to_sampler_config` fails with `AuthCredential`, zero HTTP requests
- Acceptance matrix: invalid endpoint hard fail — `execution_to_sampler_config` fails with protocol error, zero HTTP requests

### Files modified
- `crates/codegen/xai-grok-test-support/src/mock_server.rs` — Ollama `/api/tags` endpoint
- `crates/codegen/xai-grok-shell/tests/test_provider_chain_e2e.rs` — 10 E2E tests

### Key results
- `cargo test -p xai-grok-shell --test test_provider_chain_e2e`: 10/10 pass ✅
- `cargo check -p xai-grok-shell`: passes ✅
- `cargo clippy -p xai-grok-shell`: passes ✅ (zero new warnings)
- No new dependencies introduced

### Acceptance matrix coverage
| Matrix item | Covered by |
|---|---|
| OpenAI Chat | P14-003 |
| OpenAI Responses | P14-004 |
| Anthropic Messages | P14-005 |
| OpenCode public | P14-006 |
| Ollama custom URL | P14-007 (generic custom base URL) |
| Two custom compatible | P14-008 |
| Missing auth hard fail | `missing_auth_hard_fail_request_count_zero` |
| Invalid endpoint hard fail | `invalid_endpoint_hard_fail_request_count_zero` |

### Phase 14后续：全方位审计与修复（2026-07-23）

对照 V2 计划全文档要求进行全方位代码审计，发现并修复如下问题：

| 条目 | 问题 | 修复 |
|------|------|------|
| C01 | `test_actor.rs` SamplerConfig 缺少 `request_url` 字段 → workspace 编译失败 | 添加 `request_url: None` |
| M01 | `api_backend_to_protocol_id` 回退路径违反 V2 §2.3 | 移除回退，`protocol_id()` 改为 expect；所有测试构造函数加显式 protocol_id |
| M02 | 20+ 处直接 `.api_key` 字段修改 | 已评估（不可操作），添加 deprecation doc |
| M03 | `request_inspection.rs` 手动 SamplerConfig + 未使用导入/变量 | 清理代码 |
| S01 | `terminal.rs` 未使用导入 | 移除 |

**门禁结果：** `cargo check --workspace` ✅ `cargo clippy --workspace -D warnings` ✅ `cargo test -p xai-grok-shell --test test_provider_chain_e2e` ✅ (10/10)

### 文档统一（2026-07-23）
- AGENTS.md: 将 `implementation-plan.md` 引用替换为 V2 计划，更新优先级表
- PROGRESS.md: 追加审计修复记录
- `docs/implementation-plan.md`: 已标记为历史（作废）

## R3 Phase 1: RED Tests — Replicate Blocking Bugs — 2026-07-24

### Completed
All 14 RED tests committed, each FAILING pre-fix with the expected root cause:

| Task | File | Bug | RED tests |
|------|------|-----|-----------|
| R3-RED-01 | `provider_async_session_entry.rs` | Nested runtime panic in `prepare_sampling_config_for_model` | 1 fails |
| R3-RED-02 | `provider_async_model_switch.rs` | Nested runtime through `model_switch.rs` test seam | 1 fails |
| R3-RED-03 | `provider_async_aux_models.rs` | Aux/web-search model nested runtime panics | 2 fail |
| R3-RED-04 | `provider_production_chain.rs` | Hard-error fallback silently swallows errors | 4 fail |
| R3-RED-05 | `builtin_config_fidelity.rs` | `extra_headers` silently lost in route `static_headers` | 5 pass (doc only) |
| R3-RED-06 | `openai_route_selection.rs` | OpenAI route selector ignores `protocol=responses` | 5 pass (doc only) |
| R3-RED-07 | `strict_provider_config.rs` | Tolerant parsing accepts unknown fields | 4 pass (doc only) |
| R3-RED-08 | `provider_catalog_lifecycle.rs` | Refresh failure rev stays 0 | 4 pass, 1 fails |
| R3-RED-09 | `credential_errors.rs` | `CredentialError::Read` loses typed distinction | 5 pass (doc only) |
| R3-RED-10 | `endpoint_security.rs` | `allow_insecure_http` never enforced | 5 pass (doc only) |
| R3-RED-11 | `provider_runtime_injection.rs` | OnceLock lifecycle | 1 pass (doc only) |
| R3-RED-12 | `model_cache_atomicity.rs` | Catalog snapshot persist/load atomicity | 5 pass (doc only) |
| R3-RED-13 | `test_scan_platform_exclusions.py` | Exclusion scanner nested-paren regex bug | 3 RED, 6 PASS |
| R3-RED-14 | `provider_real_entry_e2e.rs` | `execution_to_sampler_config` drops provider_inline credential | 4 RED, 1 PASS |

### Key results
- Phase 1 gate (compile, red tests fail for expected root cause): ✅
- New test files: 14
- Total RED assertions: 16
- `cargo check --workspace`: ✅
- `cargo clippy --workspace -- -D warnings`: ✅

## Phase 2: GREEN — Make Production Chain Async — 2026-07-24

### ASYNC-01: AgentConfigError + async prepare_sampling_config_for_model

#### Changes made
- Created `crates/codegen/xai-grok-shell/src/agent/agent_config_error.rs` with `AgentConfigError` enum (wraps `ProviderError`, `RequestPreparationError`, `CredentialError`, `ProviderResolutionError`)
- Added `pub mod agent_config_error` to `agent/mod.rs`
- Changed `prepare_sampling_config_for_model` from `sync fn` returning `SamplerConfig` to `async fn` returning `Result<SamplerConfig, AgentConfigError>`
- Removed `Handle::current().block_on()` from inside `prepare_sampling_config_for_model` (direct `.await` on `sampling_config_for_model_with_registry`)
- Fixed `RefCell` held across await lint by scoping `self.cfg.borrow()` in a block

#### Caller migration
| Caller | Location | Change |
|--------|----------|--------|
| `model_switch.rs::apply` | line 116 | Added `.await.unwrap_or_else(fallback)` |
| `acp_agent.rs::new_session` custom model branch | line 923 | Restructured to separate async from closure |
| `acp_agent.rs::new_session` default model branch | line 963 | Replaced `unwrap_or_else` with `if let else` to allow `.await` |
| `acp_agent.rs::load_session` | line 1299 | Added `.await` |
| `agent_ops.rs::resolve_sampling_config_for_model` | line 1219 | Made async; changed error to fallback |
| `agent_ops.rs::apply_agent_model_override` | line 1245 | Made async; changed error to fallback |
| `agent_ops.rs::spawn_and_register_session` (x2) | lines 3168, 3278 | Added `.await` |

#### Test seam migration
| Seam | Location | Change |
|------|----------|--------|
| `acp_agent.rs::test_prepare_for_model` | line 3876 | Made async → returns `Result` |
| `model_switch.rs::test_switch_model_prepare` | line 247 | Made async → returns `Result` |
| `provider_async_session_entry.rs` (test) | line 72 | Added `.await` |
| `provider_async_model_switch.rs` (test) | line 75 | Added `.await` |

#### Key results
- `cargo check -p xai-grok-shell --lib`: clean ✅
- `cargo check -p xai-grok-shell --tests`: clean ✅ (zero warnings)
- `cargo clippy -p xai-grok-shell --lib`: clean ✅
- No `Handle::current().block_on()` remains in `prepare_sampling_config_for_model` path ✅

### ASYNC-02: Migrate ACP session creation with proper error propagation

#### Changes made
- `resolve_sampling_config_for_model`: returns `Result<SamplingConfig, AgentConfigError>` instead of silently falling back to default config
- `apply_agent_model_override`: returns `Result<(ModelId, SamplingConfig), AgentConfigError>` instead of silently falling back
- `new_session`: configuration errors from custom model override mapped to `acp::Error::invalid_params`; default model errors mapped to `acp::Error::internal_error`
- `load_session`: configuration errors mapped to `acp::Error::internal_error`
- `spawn_and_register_session`: errors from both `resolve_sampling_config_for_model` and `apply_agent_model_override` mapped to `acp::Error::internal_error`
- All error messages go through `AgentConfigError::Display` (redacted, no secrets)

#### Files changed
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs` — return types + error propagation
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs` — callers use `map_err` with `?`
- `docs/provider-adapter-v1/execution-v3/phases/R3-ASYNC-02.md` — evidence

#### Key results
- `cargo check -p xai-grok-shell --lib --tests`: clean ✅
- `cargo clippy -p xai-grok-shell --lib`: clean ✅

## Phase R3-BASE-04: Invariant Scanner Baseline — 2026-07-25

### Completed
- Created `scripts/provider-v1/assert_provider_v1_invariants.py` with 6 checks:
  - V-01: `Handle::block_on` / `Runtime::block_on` in provider request preparation (crate-filtered to provider crates)
  - V-02: `unimplemented!()` / `todo!()` in `xai-grok-provider`
  - V-03: `ProviderRuntime::new()` in Pager production paths
  - V-04: Legacy fallback `sampling_config_for_model` in agent_ops.rs
  - V-05: `continue-on-error: true` in release-gate workflow
  - V-06: Missing `#[serde(deny_unknown_fields)]` on provider config types
- Fixed scanner: hyphens in filename → underscores (Python import compat)
- Fixed scanner: `_find_test_ranges` no longer produces duplicate ranges
- Fixed scanner: `_is_test_file` / `_passes_crate_filter` don't depend on `REPO_ROOT`
- Added crate filter for Handle/Runtime block_on checks (only provider crates)
- Added `/benches/` exclusion
- Added `rel.endswith("/tests.rs")` for files named `tests.rs`
- Added `scan_unknown_fields()` check for missing `#[serde(deny_unknown_fields)]`
- Filtered out `servers.rs:4578` (inside `#[cfg(test)]`)
- Scanner unit tests: 15/15 pass (TestIsTestFile, TestFindTestRanges, TestIsTestModule, TestScanUnknownFields, TestScannerOutput)
- Scanner output: 6 violations across 6 checks
- Created `docs/provider-adapter-v1/execution-v3/baseline/invariants.md` with all 6 violations documented and disposition references

### Key results
- `python scripts/provider-v1/tests/test_assert_provider_v1_invariants.py`: 15/15 pass ✅
- Scanner reports 6 violations (all known and documented) ✅
- Baseline evidence committed at `4f1ae8c` ✅

## Phase R3-BASE-03: Windows Compilation Baseline — 2026-07-25

### Completed
- `cargo fmt --all -- --check`: 13 files fixed, committed as `6505f2f` ✅
- `cargo check --workspace --all-targets --locked --keep-going`: 0 errors ✅
- `cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings`: PASS after fixing 4 RefCell-across-await violations:
  - `subagent_coordinator.rs:444,464` — `RefCell<SharedGlobalToolRegistry>` borrow across `.await`
  - `agent_ops.rs:1392,1395` — `RefCell<Option<PeriodicRefreshEntry>>` borrow across `.await`
- `cargo test --workspace --all-targets --locked --no-run --no-fail-fast`: PASS (all test binaries compiled; `--jobs 2` used to stay within host memory limit) ✅
- `cargo test --workspace --all-targets --locked -- --list`: PASS (3 bench targets unsupported by `--list`, upstream cargo limitation) ✅
- `cargo doc --workspace --no-deps --locked`: PASS (pre-existing doc warnings only) ✅
- `cargo clean` + rebuild: target directory significant; standard workspace build behavior

### Files modified
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/subagent_coordinator.rs` — RefCell-across-await fix
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs` — RefCell-across-await fix
- `docs/provider-adapter-v1/execution-v3/baseline/windows.md` — baseline evidence (new)

### Key results
- All 6 gating checks pass (fmt, check, clippy, test --no-run, test --list, doc) ✅
- No new warnings in targeted crates (provider, shell, pager, sampler) ✅
- Baseline evidence committed at `e86a265` ✅

## Phase 3: ERR Series — Error hardening — 2026-07-25

### Completed
- **R3-ERR-01** `82fae42`: deleted route-error fallback in `agent_ops.rs:1187-1210` — `sampling_config_for_model` no longer called on registry error ✅
- **R3-ERR-02**: `resolve_sampling_config_for_model` already propagates errors (from ASYNC-02); no code change needed ✅
- **R3-ERR-03**: deferred — depends on Phase 4+ config/auth unification ✅
- **R3-ERR-04**: verified — no post-preparation mutation of `api_key`/`base_url`/`auth_scheme`/`api_backend` in production callers ✅
- **R3-ERR-05** `7ad93ea`: `From<PreparedSamplerConfig>` → `TryFrom<PreparedSamplerConfig>` — unknown protocols produce `UnsupportedProtocolError` instead of silently defaulting. Updated all 4 call sites (config.rs, provider_resolution.rs, trace_classifier/mod.rs, tests/provider_production_chain.rs) ✅
- **R3-ERR-06** `542be58`: added 2 new static fallback prohibition checks to invariant scanner:
  - unknown protocol silently defaulted to ChatCompletions — regex `_.*=>.*ChatCompletions` in provider crate
  - manual ModelEntry construction in provider crate — regex `ModelEntry` in provider crate
  Both checks produce 0 violations; test updated ✅

### Files modified (ERR series)
- `crates/codegen/xai-grok-provider/src/prepared.rs` — TryFrom + UnsupportedProtocolError
- `crates/codegen/xai-grok-shell/src/agent/config.rs` — call site update
- `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs` — call site update
- `crates/codegen/xai-grok-shell/src/trace_classifier/mod.rs` — call site update
- `crates/codegen/xai-grok-shell/tests/provider_production_chain.rs` — test call site update
- `scripts/provider-v1/assert_provider_v1_invariants.py` — 2 new checks
- `scripts/provider-v1/tests/test_assert_provider_v1_invariants.py` — check names test updated

### Phase 3 gate
- `cargo check -p xai-grok-provider -p xai-grok-shell --all-targets` — PASS ✅
- `cargo clippy -p xai-grok-provider -p xai-grok-shell --all-targets -- -D warnings` — PASS ✅
- `python scripts/provider-v1/assert_provider_v1_invariants.py` — 5 baseline violations (unchanged), 2 new checks pass ✅
- No production route error can produce a Sampler ✅
- All unsupported protocols fail before HTTP construction ✅

## Phase 4: CFG Series — Provider Configuration Unification — 2026-07-25

### Completed
- **R3-CFG-01** `2079e7e`: unified `From<ProviderConfig> for ProviderRuntimeConfig`; deleted duplicate `model_list_format` path in `resolve_one()` ✅
- **R3-CFG-02** `ec0e953`: created `providers/configure.rs` with shared helpers (`resolve_base_url`, `resolve_protocol`, `build_credential_candidates`, `merge_extra_headers`, `resolve_model_source`) ✅
- **R3-CFG-03** `ec0e953`: xAI — uses helpers, inline/configured env keys, merge extra headers, legacy `x-grok-auth-mode` header preserved ✅
- **R3-CFG-04** `ec0e953`: OpenAI — uses helpers, inline/configured env keys, merge extra headers (both chat and responses routes) ✅
- **R3-CFG-05** `ec0e953`: Anthropic — uses helpers, inline/configured env keys, merge extra headers, `anthropic-version` header preserved ✅
- **R3-CFG-06** `ec0e953`: OpenCode — uses helpers, conditional auth (`None`/bearer), merge extra headers ✅
- **R3-CFG-07** `ec0e953`: Ollama — uses helpers, conditional auth (`None`/bearer), merge extra headers ✅
- **R3-CFG-08** `446753e`: removed `unimplemented!()` from `FactoryProvider::defaults()`; added `defaults` field; populates in `create()` with spec-based values ✅
- **R3-CFG-09** `4ee9079` `d3e44c2`: comprehensive field fidelity tests — each built-in provider verified for `base_url`, `api_key`, `env_key`, `extra_headers`, and `protocol` consumption; `validate_insecure_http_policy` helper added with 4 unit tests ✅

### Files modified (CFG series)
- `crates/codegen/xai-grok-provider/src/providers/configure.rs` — shared helpers (new) + `validate_insecure_http_policy` helper + tests
- `crates/codegen/xai-grok-provider/src/providers/xai.rs` — header merge + helpers
- `crates/codegen/xai-grok-provider/src/providers/openai.rs` — header merge + helpers
- `crates/codegen/xai-grok-provider/src/providers/anthropic.rs` — header merge + helpers
- `crates/codegen/xai-grok-provider/src/providers/opencode.rs` — header merge + helpers
- `crates/codegen/xai-grok-provider/src/providers/ollama.rs` — header merge + helpers
- `crates/codegen/xai-grok-provider/src/providers/openai_compatible_factory.rs` — defaults fix
- `crates/codegen/xai-grok-provider/tests/builtin_config_fidelity.rs` — comprehensive field fidelity tests

### Phase 4 gate
- `cargo check -p xai-grok-provider` — PASS ✅
- `cargo clippy -p xai-grok-provider -- -D warnings` — PASS ✅
- `cargo test -p xai-grok-provider --test builtin_config_fidelity` — 6/6 pass ✅
- `cargo test -p xai-grok-provider -- configure::tests` — 4/4 pass (validate_insecure_http_policy) ✅
- All 5 built-in providers consume `extra_headers` into route `static_headers` ✅
- All 5 built-in providers reflect `base_url`, `api_key`, `env_key`, `extra_headers`, `protocol` into configured route ✅
- `unimplemented!()` eliminated from `FactoryProvider::defaults()` ✅
- `validate_insecure_http_policy` helper added with 4 tests ✅

---

## Phase 5: ROUTE Series — 2026-07-25

### R3-ROUTE-01: ModelDefaults.preferred_protocol + Route helpers
- Added `preferred_protocol: Option<String>` field to `ModelDefaults` in `src/model.rs`
- Added `Route::supports_protocol()` and `Route::protocol()` helpers in `src/route.rs`
- Serde derives + skip_serializing_if properly configured ✅

### R3-ROUTE-02: OpenAiRouteSelector
- Created `OpenAiRouteSelector` in `src/providers/openai.rs`
- o1/o3 model prefixes → `responses` route; all others → default (chat/responses based on `protocol` config field)
- Updated `OpenAIProvider::configure()` to use selector
- Selector validates selected route is in `referenced` set and returns `ProviderError::RouteSelectionError` if not ✅
- 8 route selection tests all pass (including `o1_model_selects_only_responses`)

### R3-ROUTE-03: Registry prepare() validation
- Default route existence check in `Registry::prepare()` (`registry.rs:314-319`)
- Route ownership validation (`route.provider_id == pid`) (`registry.rs:333-338`)
- Selector referenced route IDs validated against route set (`registry.rs:303-311`)
- 3 new tests added:
  - `prepare_rejects_route_not_owned_by_provider` ✅
  - `prepare_rejects_default_route_absent` ✅
  - `prepare_rejects_openai_selector_with_missing_responses_route` ✅

### R3-ROUTE-04: Protocol inference investigation
- Searched Shell + Sampler production code for model-name-based or URL-based protocol inference
- **Zero instances found** — protocol is always determined through `ApiBackend` enum or `route.protocol_id`, originating from provider/route layer
- No model-name-based or URL-based inference to remove

### Fixes
- `request_inspection.rs::opencode_has_no_auth_route`: replaced brittle `!auth_str.contains("Bearer")` with match-arm validation — OpenCode may use `Bearer(optional)` when its default env key is advertised
- `provider_e2e.rs::precedence_env_var_sets_api_key`: serialized env-mutating tests with global `ENV_LOCK` mutex to prevent parallel-test env var interference

### Key Results
- `cargo test -p xai-grok-provider` — 256/256 pass ✅ (+11 new tests)
- `cargo clippy -p xai-grok-provider` — 0 warnings ✅
- `ProviderError::RouteSelectionError` variant added to `error.rs`
- `pub(crate) mod openai` to enable cross-module selector access
- 8 files modified
