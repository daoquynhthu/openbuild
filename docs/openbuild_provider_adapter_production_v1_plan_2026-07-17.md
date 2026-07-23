# OpenBuild Provider Adapter V1 Production Closure Implementation Plan

> **⚠️ 作废 — 以 V2 计划为准**
>
> 本文件保留为历史参考。Provider Adapter V1 生产闭环的**唯一执行权威**是：
> `docs/openbuild_provider_adapter_production_v1_closure_plan_v2_2026_07.md`
>
> **禁止使用本文件进行任何执行决策。**

> **For agentic workers:** This document is the execution authority for the provider-adapter production closure. Execute exactly one task at a time, preserve the stated architecture, run every required gate, and stop on any gate failure. Do not infer missing requirements, invent shortcuts, or continue after a failed prerequisite.

**Goal:** Transform the current partially integrated provider-adapter branch into a deterministic, testable, cross-platform V1 production closure in which provider configuration, model discovery, model selection, authentication, request routing, protocol dispatch, runtime reload, and TUI configuration form one authoritative end-to-end chain.

**Architecture:** Replace the present dual-authority system with one immutable provider snapshot. A configured provider owns a deterministic route set and model-binding policy; the shell resolves a selected model into one explicit sampling execution policy; the sampler executes that policy without re-deriving provider behavior from legacy fields. Model discovery becomes an asynchronous catalog service and never blocks the synchronous model resolver or startup critical path.

**Tech Stack:** Rust 1.92.0, edition 2024, Tokio, reqwest 0.12, serde, toml/toml_edit, indexmap, axum-based mock servers, existing Grok Build crates and test harnesses. No new third-party dependency is allowed unless the repository owner explicitly approves it.

## Global Constraints

- Work only in a real Git checkout with `.git` metadata. If `.git` is absent, stop and report `BLOCKED: incomplete checkout`; do not run `git init`.
- Work on `feat/provider-adapter` unless the repository owner explicitly names another branch.
- Preserve existing xAI OAuth/session behavior and legacy `[endpoints]` compatibility until the final cleanup phase proves equivalent behavior.
- V1 supported providers: xAI, OpenAI, Anthropic, OpenCode Zen, Ollama, named OpenAI-compatible profiles, and arbitrary user-defined OpenAI-compatible providers.
- V1 supported wire protocols: OpenAI Chat Completions, OpenAI Responses, and Anthropic Messages.
- Unknown protocol IDs, invalid endpoint URLs, missing mandatory credentials, invalid headers, duplicate provider IDs, duplicate route IDs, and ambiguous model references must produce explicit typed errors. No silent fallback is permitted.
- Provider/model ordering must be deterministic across runs and platforms.
- Startup and config parsing must not perform synchronous provider network requests.
- Unit and integration tests must not require public internet access. Use local mock servers.
- Secrets must never appear in `Debug`, logs, panic messages, snapshots, test output, telemetry, or persisted non-secret configuration fields.
- Do not weaken a test, add `#[allow]`, add `unwrap()` to production code, disable a lint, skip a platform, or change a public behavior merely to make a gate pass.
- Every task ends with a focused commit. Every phase ends with a phase gate and a `PROGRESS.md` update.
- This plan explicitly authorizes new source/test files inside existing crates, new files under `docs/`, and the Phase 13 workflow file under `.github/workflows/`; this is the only exception to the older `AGENTS.md` file-placement restriction.
- A phase is not complete until fresh command output proves every stated gate passes.

---

## 1. Authority and Document Precedence

For the provider-adapter V1 closure, use this precedence:

1. This plan.
2. `docs/model-adapter-architecture.md`, after it is reconciled in Phase 1.
3. `AGENTS.md`.
4. Existing tests that do not contradict items 1–3.
5. `docs/implementation-plan.md`, which is historical and must not be used to override this plan.
6. `PROGRESS.md` and `ISSUE.md`, which are evidence logs rather than architecture authorities.

The agent must add a notice to `docs/implementation-plan.md` during Phase 1 stating that this file supersedes its provider-adapter execution sequence. Do not delete the historical plan.

## 2. Current Baseline and Known Blocking Defects

The execution starts from a branch with the following confirmed static defects:

| Audit ID | Defect | Closure phase |
|---|---|---|
| C-01 | Provider registry tests call missing `ProviderRegistry::model()` and the documented passing baseline is not trustworthy. | Phase 0–2 |
| C-02 | `Route` is not the authority for the production request. | Phase 3–4 |
| C-03 | OpenAI Responses route is constructed but not registered or selected. | Phase 2, 6 |
| C-04 | Route defaults and mandatory provider headers do not reach the sampler. | Phase 3–6 |
| C-05 | Dynamic model discovery ignores resolved provider base URL. | Phase 7 |
| C-06 | Provider model cache does not enforce TTL and synchronous discovery can block for minutes. | Phase 7 |
| C-07 | Provider `env_key` and `extra_headers` are accepted but not authoritative. | Phase 3, 5 |
| C-08 | Authentication failures are swallowed and auth is resolved at the wrong boundary. | Phase 5 |
| C-09 | OpenCode public/free mode is not closed. | Phase 6–7 |
| C-10 | Named OpenAI-compatible profiles and arbitrary provider IDs are unreachable. | Phase 2, 6, 8 |
| C-11 | Protocol dispatch is a fixed switch with silent fallback. | Phase 4 |
| C-12 | `/providers` displays false status and “save” does not persist or reload. | Phase 10 |
| M-01 | Valid `[provider.*]` configuration is reported as unknown by the main config parser. | Phase 3 |
| M-02 | Provider registry lifecycle and runtime synchronization are unreliable. | Phase 2, 9 |
| M-03 | Provider/model ordering is nondeterministic. | Phase 2, 7–8 |

The agent must not mark any issue `-Fixed` until the issue-specific regression test passes. It must not mark an issue `-Closed` until the complete phase gate passes and a separate review confirms the behavior.

## 3. Definition of V1 Production Closure

V1 is closed only when all statements below are true.

### 3.1 Configuration closure

- `[provider.<id>]` is parsed by the main typed config without unknown-key warnings.
- Precedence is deterministic and tested: built-in defaults < environment < TOML < legacy compatibility mapping < CLI.
- `--provider`, `--model provider/model`, `--api-key`, and `--base-url` target one explicit provider.
- Manual `[model.<id>]` entries can explicitly bind `provider` and optionally `route`.
- Legacy xAI configuration remains functional and has regression coverage.
- Configuration changes are persisted atomically and can be reloaded without restarting the process.

### 3.2 Registry and routing closure

- A single process-wide registry instance is constructed by the launcher and passed to shell/TUI runtime state; the pager must not create a second independent registry.
- Registry snapshots are immutable to readers, revisioned, deterministic, and atomically replaced after successful validation.
- Each configured provider exposes all of its routes, a default route, and a tested route-selection policy.
- A selected model resolves to exactly one `provider_id`, `route_id`, protocol, endpoint, auth policy, headers, and generation/default limits.
- The sampler does not infer endpoint paths or provider headers from provider names or URL patterns.

### 3.3 Request execution closure

- Request URL is rendered from the selected route, not from a legacy hardcoded protocol path.
- Mandatory static headers, user extra headers, dynamic headers, and authentication are merged under a documented precedence with invalid/conflicting values rejected.
- Authentication is resolved at request construction or through the existing live credential resolver; missing credentials fail before network transmission.
- Chat Completions, Responses, and Messages request/stream paths pass protocol-specific mock-server tests.
- Unknown protocol IDs return a typed error; they never fall back to Chat Completions.

### 3.4 Model catalog closure

- Startup does not synchronously fetch provider model lists.
- Catalog discovery runs asynchronously, concurrently with a bounded concurrency limit, per-provider timeout, cancellation, TTL cache, stale fallback, and explicit refresh.
- The cache key includes provider ID, effective model-list URL, auth identity class, and relevant config revision without containing a secret.
- OpenCode public discovery works without an API key.
- Ollama discovery works over local HTTP.
- Dynamic models, static provider models, embedded defaults, and manual user models merge deterministically with tested precedence.

### 3.5 Runtime and UI closure

- Initial session, model switching, subagents, web-search model, session-summary model, compaction helpers, and restored sessions all use the same resolved provider route chain.
- `/providers` reports actual configured/credential/discovery/error state, not mere registration.
- Saving provider settings validates, atomically writes config, rebuilds the registry, refreshes the catalog, and updates the UI only after success.
- Failed saves preserve the previous working snapshot and show a non-secret actionable error.

### 3.6 Release closure

- Targeted crates and full workspace gates pass on Linux and Windows CI. macOS must pass if the upstream product currently supports macOS.
- No provider-adapter test requires internet access.
- Release documentation, migration examples, troubleshooting, and rollback instructions exist.
- A clean checkout can build, run mocked product E2E, and produce the existing distributable artifact.
- A release candidate commit is tagged only after the final gate report is committed.

## 4. Explicit V1 Non-Goals

The weak agent must not expand scope into these items:

- Runtime loading of third-party protocol plugins or dynamic libraries.
- A general-purpose HTTP signing DSL beyond the credential/header modes needed by supported V1 providers.
- Refactoring unrelated xAI product services, telemetry, storage, MCP, tools, or agent orchestration.
- Replacing all legacy model metadata structures across the repository when an adapter at the model-resolution boundary suffices.
- Adding a cloud account manager, browser OAuth UI for non-xAI providers, encrypted secret vault, billing UI, or provider marketplace.
- Rewriting the entire sampler protocol implementation.
- Introducing a new async runtime, HTTP client, UI framework, config library, or dependency-injection framework.
- Supporting macOS if upstream has explicitly dropped it; do not create a new platform promise.

## 5. Frozen Architecture Decisions

These decisions are not optional implementation suggestions. A weak agent must follow them unless the repository owner changes this plan.

### AD-01: One authority per concern

- Provider definition owns defaults and route construction.
- Config resolver owns precedence.
- Registry snapshot owns configured provider truth.
- Model catalog owns available model truth.
- Model resolver owns model-reference disambiguation.
- Sampler owns HTTP transport, retries, and protocol codecs.
- TUI reflects runtime state and dispatches explicit configuration effects; it does not mutate global state directly.

### AD-02: Route shape for V1

`Route` must be declarative and cloneable. It must contain:

```rust
pub struct Route {
    pub id: RouteId,
    pub provider_id: ProviderId,
    pub protocol_id: ProtocolId,
    pub endpoint: Endpoint,
    pub auth: AuthPolicy,
    pub static_headers: IndexMap<String, String>,
    pub generation_defaults: GenerationDefaults,
    pub limits: ModelLimits,
}
```

`Framing` must not be a separate route authority in V1. Stream framing/decoding is owned by the protocol implementation selected by `protocol_id`. Remove or deprecate the unused route-level framing abstraction after equivalent tests exist.

### AD-03: Configured provider shape

A configured provider must expose a route set, not a single route:

```rust
pub struct ConfiguredProvider {
    pub id: ProviderId,
    pub display_name: String,
    pub config: ResolvedProviderConfig,
    pub routes: IndexMap<RouteId, Arc<Route>>,
    pub default_route_id: RouteId,
    pub route_selector: Arc<dyn RouteSelector>,
    pub model_source: ModelSourceSpec,
}
```

`RouteSelector::select(model_id)` returns `Result<RouteId, ProviderError>`. OpenAI uses this seam to select Responses versus Chat Completions. All providers must test their selector.

### AD-04: Immutable registry snapshots

Use existing standard-library synchronization and `IndexMap`; do not add a new dependency.

```rust
pub struct RegistrySnapshot {
    pub revision: u64,
    pub providers: IndexMap<ProviderId, Arc<ConfiguredProvider>>,
    pub routes: IndexMap<RouteId, Arc<Route>>,
}

pub struct ProviderRegistry {
    definitions: IndexMap<ProviderId, SharedProvider>,
    snapshot: RwLock<Arc<RegistrySnapshot>>,
}
```

Registry rebuild is transactional:

1. Resolve all provider configs.
2. Configure and validate every provider into a new local snapshot.
3. Reject duplicate IDs and invalid routes.
4. Replace the current `Arc<RegistrySnapshot>` only if all validation succeeds.
5. Increment revision exactly once per successful replacement.

Never mutate routes/config maps independently.

### AD-05: Declarative authentication policy

Replace eager credential resolution in provider constructors with a declarative policy:

```rust
pub enum CredentialSource {
    Inline,
    Environment(Vec<String>),
    Session,
    Public,
    None,
}

pub enum AuthPolicy {
    None,
    Bearer(CredentialSource),
    Header { name: String, source: CredentialSource },
}
```

The exact representation may be adjusted to existing types, but it must preserve these semantics:

- Provider constructors do not read environment variables.
- Provider constructors do not capture plaintext keys in `Debug` values.
- Shell runtime resolves inline/env/session credentials through one tested resolver.
- `Public` means “send no auth header but allow discovery/inference if the provider permits it.”
- Missing required credentials produce `ProviderError::MissingCredential` before the HTTP request.
- Live xAI session refresh uses the existing `bearer_resolver` or an equivalent request-time resolver.

### AD-06: Explicit sampler execution policy

Extend `SamplerConfig` only with the minimum fields necessary to execute a resolved route. Required effective data:

- `protocol_id`
- rendered request endpoint path or full request URL policy
- query parameters
- resolved auth scheme/live resolver
- merged static headers
- selected model ID and generation defaults

`api_backend` may remain temporarily for backward compatibility, but when `protocol_id` is present it must not override or alter the selected route. Unknown `protocol_id` is an error.

### AD-07: Asynchronous catalog service

Create a dedicated provider catalog service in the shell. It must not live inside `resolve_model_list()` and must not expose blocking fetch functions to startup code.

Required API shape:

```rust
pub enum RefreshStrategy {
    CacheOnly,
    RefreshIfStale,
    ForceRefresh,
}

pub struct ProviderCatalogService { /* cancellation, client, cache */ }

impl ProviderCatalogService {
    pub async fn refresh_provider(...);
    pub async fn refresh_all(...);
    pub fn snapshot(&self) -> Arc<ModelCatalogSnapshot>;
}
```

### AD-08: Runtime state is injected, not re-created

The launcher constructs the registry and catalog service once. Shell config and pager state receive `Arc` references. `xai-grok-pager/src/app/mod.rs` must not independently construct a second registry.

### AD-09: Deterministic model identity

Canonical reference syntax is `provider/model`. Bare model IDs are accepted only when they resolve uniquely under deterministic precedence. Persisted model selection must store the canonical provider-qualified reference for non-xAI providers.

Manual model config gains:

```toml
[model.my-model]
provider = "openai-compatible"
route = "openai-compatible-chat" # optional
model = "server-routing-slug"
```

### AD-10: Safe endpoint policy

- Parse all configured URLs with `url::Url`.
- Remote provider endpoints require `https`, except explicit local providers/hosts (`localhost`, loopback, Unix-local equivalent where already supported) may use `http`.
- Reject embedded credentials, fragments, unsupported schemes, malformed base/path joining, and empty host for network endpoints.
- Never fall back to `http://localhost` after URL parse failure.

## 6. Weak-Agent Behavioral Contract

### 6.1 Mandatory pre-task sequence

Before changing any file, the agent must:

1. Read this plan’s current phase and task.
2. Read `AGENTS.md`.
3. Read the relevant architecture section and all files listed by the task.
4. Run `git status --short` and `git diff --stat`.
5. Confirm no unrelated user changes would be overwritten.
6. Write one explicit hypothesis in the work log: `Task Pn-m changes X because Y; success is proven by Z.`
7. Run the task’s pre-change test and record whether it passes or fails.

### 6.2 Change discipline

- Execute one task ID only. Do not opportunistically implement later tasks.
- Modify only the files listed by the task. If another file is required, stop and record why before touching it.
- Keep production changes and regression tests in the same commit.
- Do not perform repository-wide formatting; format only changed Rust files via `cargo fmt --all` and verify diff scope.
- Do not rename public APIs unless the task explicitly requires it.
- Do not introduce compatibility shims that hide an invalid state.
- Do not duplicate logic to avoid touching the correct abstraction.
- Do not add “temporary” fallbacks, feature flags, environment variables, or hidden defaults unless specified here.
- Do not use timing sleeps in tests. Use paused Tokio time, channels, barriers, or explicit mock-server signals.
- Do not call public provider APIs in tests.
- Do not place real-looking secrets in fixtures; use obvious sentinels such as `test-secret-not-real`.

### 6.3 Failure behavior

On any compile, test, lint, or doc failure:

1. Stop the task.
2. Capture the exact command, exit code, and first causal error.
3. Determine whether the failure existed before the task by checking the pre-change result or temporarily testing the parent commit.
4. Revert only the task’s incomplete changes if they cannot be corrected within the task’s scope.
5. Do not continue to another task.
6. Report `BLOCKED` with root cause and the minimal next action.

After three failed fix attempts for the same task, stop and request architectural review. Do not make a fourth speculative patch.

### 6.4 Prohibited completion language

The agent may not say “done,” “fixed,” “complete,” “passes,” or equivalent unless it includes fresh command evidence. A valid completion statement must include:

- task ID;
- commit hash;
- changed files;
- exact commands run;
- pass/fail counts or exit codes;
- remaining known failures.

### 6.5 Commit protocol

Use one commit per task:

```text
phase-N: <task ID> <imperative summary>
```

Examples:

```text
phase-2: P2-03 make registry rebuild transactional
phase-7: P7-04 enforce provider catalog TTL
```

Before commit:

```bash
cargo fmt --all -- --check
cargo check -p <affected-crate> --all-targets
cargo clippy -p <affected-crate> --all-targets -- -D warnings
cargo test -p <affected-crate> <task-specific-filter>
```

After commit, run `git status --short`; it must be clean unless the next task has intentionally started.

## 7. Test and Gate Hierarchy

### Gate T0 — Task gate

Run for every task:

```bash
cargo fmt --all -- --check
cargo check -p <affected-crate> --all-targets
cargo clippy -p <affected-crate> --all-targets -- -D warnings
cargo test -p <affected-crate> <focused-test-filter>
```

### Gate T1 — Provider core gate

Run after any provider type/registry/provider implementation change:

```bash
cargo check -p xai-grok-provider --all-targets
cargo clippy -p xai-grok-provider --all-targets -- -D warnings
cargo test -p xai-grok-provider --all-targets
```

### Gate T2 — Request-chain gate

Run after route/sampler/shell integration changes:

```bash
cargo check -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell --all-targets
cargo clippy -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell --all-targets -- -D warnings
cargo test -p xai-grok-provider --all-targets
cargo test -p xai-grok-sampler --all-targets
cargo test -p xai-grok-shell --lib agent::config
cargo test -p xai-grok-shell --lib agent::models
```

### Gate T3 — Product UI gate

```bash
cargo check -p xai-grok-pager -p xai-grok-pager-bin --all-targets
cargo clippy -p xai-grok-pager -p xai-grok-pager-bin --all-targets -- -D warnings
cargo test -p xai-grok-pager --all-targets
cargo test -p xai-grok-pager-bin --all-targets
```

### Gate T4 — Phase gate

At every phase boundary:

```bash
cargo fmt --all -- --check
cargo check -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell -p xai-grok-pager -p xai-grok-pager-bin --all-targets
cargo clippy -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell -p xai-grok-pager -p xai-grok-pager-bin --all-targets -- -D warnings
cargo test -p xai-grok-provider --all-targets
cargo test -p xai-grok-sampler --all-targets
cargo test -p xai-grok-shell --all-targets
cargo test -p xai-grok-pager --all-targets
cargo test -p xai-grok-pager-bin --all-targets
```

If local execution is constrained by a two-minute host timeout, split these commands by crate and use CI for the aggregate gate. A timeout is not a pass.

### Gate T5 — Workspace and release gate

Mandatory in CI and mandatory locally when the environment permits:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo doc --workspace --no-deps
```

The documentation command passes only if it exits zero and emits no new warnings from changed provider-adapter public APIs.

## 8. Phase Dependency Graph

```text
P0 Baseline
 └─ P1 Architecture contract
     └─ P2 Provider core + transactional registry
         ├─ P3 Typed config and precedence
         └─ P4 Route-to-sampler execution policy
             ├─ P5 Authentication and header closure
             ├─ P6 Built-in provider closure
             └─ P7 Async model catalog
                 └─ P8 Deterministic model resolution
                     └─ P9 Runtime lifecycle and hot reload
                         └─ P10 TUI/CLI configuration closure
                             └─ P11 Security, resilience, observability
                                 └─ P12 Cross-surface E2E and compatibility
                                     └─ P13 CI, packaging, release engineering
                                         └─ P14 Cleanup, final audit, RC cut
```

No phase may begin before its dependency phase gate passes.

---

# Phase 0 — Restore a Trustworthy Baseline

**Objective:** Establish reproducible source, toolchain, compile, test, and issue evidence before architecture changes.

**Entry condition:** Real Git checkout on the expected branch.

### P0-01: Verify checkout integrity

**Files:** None.

**Actions:**

```bash
git rev-parse --show-toplevel
git branch --show-current
git status --short
git log -5 --oneline --decorate
```

**Required results:**

- Repository root resolves.
- Branch is `feat/provider-adapter`, unless explicitly overridden by the owner.
- Current user modifications are listed and preserved.
- Record base commit hash in `PROGRESS.md` under a new “V1 Closure Baseline” section.

**Stop condition:** Missing `.git`, detached HEAD without owner instruction, unresolved merge, or unknown unrelated modifications that overlap provider files.

### P0-02: Verify toolchain and native prerequisites

**Files:** None.

**Commands:**

```bash
rustup show active-toolchain
rustc --version --verbose
cargo --version
cargo fmt --version
cargo clippy --version
protoc --version
```

**Required results:** Rust 1.92.0 and required components are available. If `protoc` is missing, install it through the documented platform mechanism before proceeding. Do not change `rust-toolchain.toml` to fit the machine.

### P0-03: Reproduce the current provider failure

**Files:** None.

**Commands:**

```bash
cargo test -p xai-grok-provider --all-targets
cargo check -p xai-grok-provider --all-targets
```

**Expected baseline:** The current source is expected to expose the stale `ProviderRegistry::model()` test or another concrete compile failure. Save the exact output to the phase log; do not edit tests yet.

### P0-04: Record targeted baseline gates

**Files:** Modify `PROGRESS.md`; do not alter `ISSUE.md` findings except to append current reproduction evidence.

Run and record:

```bash
cargo check -p xai-grok-sampler --all-targets
cargo test -p xai-grok-sampler --all-targets
cargo check -p xai-grok-shell --all-targets
cargo check -p xai-grok-pager --all-targets
cargo check -p xai-grok-pager-bin --all-targets
```

A pre-existing failure is allowed at this phase only if documented with exact scope. New work must not expand the failure set.

### P0-05: Add a machine-readable baseline manifest

**Files:** Create `docs/provider-adapter-v1/baseline.md`.

Include:

- base commit;
- branch;
- toolchain versions;
- each baseline command and exit code;
- known audit IDs;
- any unavailable platform/tool;
- explicit statement that no source behavior was changed.

**Phase 0 gate:** P0-01 through P0-05 evidence is committed. No claim that tests pass is allowed unless they actually pass.

---

# Phase 1 — Freeze the V1 Architecture and Public Contracts

**Objective:** Remove ambiguity before implementation and make this plan and the reconciled architecture the only authoritative design.

### P1-01: Reconcile architecture documentation

**Files:**

- Modify `docs/model-adapter-architecture.md`.
- Modify `docs/implementation-plan.md`.
- Create `docs/provider-adapter-v1/config-reference.md`.
- Create `docs/provider-adapter-v1/provider-matrix.md`.

**Required changes:**

- Replace the unused four-axis “route-level framing” claim with AD-02: protocol owns framing/stream decoding.
- State that configured providers own route sets.
- State registry transactional snapshot semantics.
- State deterministic model identity and merge precedence.
- State async catalog requirements.
- Add a banner to the old implementation plan: “Historical; superseded by `docs/superpowers/plans/2026-07-17-provider-adapter-production-v1.md` for V1 closure.”
- Document exact config examples for all V1 providers without real secrets.

**Tests:** Documentation review only, followed by `cargo doc -p xai-grok-provider --no-deps` after public types exist in Phase 2. At this task, run a placeholder scan:

```bash
rg -n "TBD|TODO|implement later|fix later|暂定|待定" docs/provider-adapter-v1 docs/model-adapter-architecture.md
```

Expected: no unresolved placeholder in newly written contract sections.

### P1-02: Freeze public API and test contracts in documentation

**File:** Create `docs/provider-adapter-v1/public-contracts.md`.

Specify the exact public type fields, ownership, fallible constructors, error variants, and test names that Phase 2 must implement. The document must include these mandatory test cases:

- `configured_provider_contains_multiple_routes`.
- `registry_snapshot_revision_increments_once`.
- `registry_snapshot_order_is_deterministic`.
- `openai_selector_routes_chat_model`.
- `openai_selector_routes_responses_model`.
- `selector_rejects_unknown_route`.
- `provider_config_debug_redacts_secret`.
- `invalid_endpoint_never_falls_back_to_localhost`.

Do not add compile-failing Rust tests in Phase 1. Phase 2 creates each test immediately before its implementation under the normal red/green task cycle.

### P1-03: Define the provider compatibility matrix

**File:** `docs/provider-adapter-v1/provider-matrix.md`.

Required table columns:

- provider ID;
- default base URL;
- inference protocol(s);
- inference path(s);
- auth header and credential sources;
- mandatory static headers;
- model-list URL and format;
- public/no-auth behavior;
- local HTTP allowance;
- tested model examples;
- live smoke-test environment variables.

**Phase 1 gate:** Documentation is internally consistent, has no placeholders, and explicitly supersedes conflicting old sections. Commit as `phase-1: freeze provider adapter V1 contracts`.

---

# Phase 2 — Provider Core and Transactional Registry

**Objective:** Build a deterministic provider core that can represent all V1 providers without using the sampler or shell as a hidden authority.

### P2-01: Introduce validated IDs and typed provider errors

**Files:**

- Modify `crates/codegen/xai-grok-provider/src/types.rs`.
- Modify `crates/codegen/xai-grok-provider/src/error.rs`.
- Modify `crates/codegen/xai-grok-provider/src/lib.rs`.
- Add tests in the same modules.

**Required APIs:**

- `ProviderId`, `RouteId`, and `ModelId` validate non-empty trimmed values.
- IDs reject whitespace-only strings and invalid separators where ambiguity would result.
- `ProviderError` includes at least:
  - `InvalidProviderId`
  - `InvalidRouteId`
  - `DuplicateProvider`
  - `DuplicateRoute`
  - `UnknownProvider`
  - `UnknownRoute`
  - `InvalidEndpoint`
  - `MissingCredential`
  - `InvalidHeader`
  - `UnknownProtocol`
  - `AmbiguousModel`
  - `Config`
- Error display text contains no secret values.

**Tests:** Table-driven ID validation and redaction tests.

### P2-02: Replace eager auth functions with declarative auth policy

**Files:**

- Modify `crates/codegen/xai-grok-provider/src/auth.rs`.
- Modify `crates/codegen/xai-grok-provider/src/config.rs`.
- Modify `crates/codegen/xai-grok-provider/src/lib.rs`.
- Update provider unit tests that construct auth.

**Required behavior:**

- Provider construction performs no environment read.
- `ProviderConfig` can represent ordered env keys and extra headers.
- API keys are excluded from derived `Debug`; implement custom redacted `Debug` if necessary.
- `CredentialSource::Public` is distinguishable from `None` and required-auth missing.
- Header names are validated against HTTP token rules before registry snapshot publication.

**Regression tests:**

- Debug output does not contain a sentinel key.
- Environment changes after provider construction can be observed when runtime resolves credentials.
- OpenCode public policy produces no auth header and no missing-credential error.

### P2-03: Make endpoint rendering fallible and safe

**Files:** Modify `crates/codegen/xai-grok-provider/src/endpoint.rs` and tests.

**Required behavior:**

- `Endpoint::render()` returns `Result<Url, ProviderError>`.
- Remove localhost fallback on missing/invalid URL.
- Correctly join base paths without duplicate or missing `/`.
- Preserve configured query parameters.
- Reject credentials and fragments in configured network URLs.
- Permit `http` only for loopback/local provider policy.

**Tests:** HTTPS remote, localhost HTTP, IPv4 loopback, IPv6 loopback, trailing slash combinations, query merge, malformed URL, empty base, embedded userinfo, fragment, unsupported scheme.

### P2-04: Redefine Route and remove route-level framing authority

**Files:**

- Modify `crates/codegen/xai-grok-provider/src/route.rs`.
- Modify `crates/codegen/xai-grok-provider/src/framing.rs`.
- Modify `crates/codegen/xai-grok-provider/src/model.rs`.
- Modify `crates/codegen/xai-grok-provider/src/lib.rs`.

**Required behavior:**

- Route matches AD-02.
- Route validation confirms provider ID, route ID, protocol ID, endpoint, auth policy, and headers.
- `Route::model()` returns a model binding containing the explicit `route_id`.
- Existing framing code is either removed after all references are removed or retained as deprecated internal code with no production caller until Phase 14. Do not leave two active framing authorities.

### P2-05: Redefine ConfiguredProvider and route selection

**Files:** Modify `provider.rs`, provider fixtures, and contract tests.

**Required behavior:**

- `ConfiguredProvider` stores all routes in deterministic `IndexMap` order.
- Default route ID must exist in the route map.
- Route selector cannot return a route outside the map.
- Provider config and model source spec are retained in resolved form without exposing secrets.

**Tests:** Invalid default route, selector returns unknown route, duplicate route, deterministic route iteration.

### P2-06: Implement transactional registry snapshots

**Files:** Rewrite `crates/codegen/xai-grok-provider/src/registry.rs` and tests.

**Required public operations:**

```rust
pub fn register_definition(&mut self, provider: SharedProvider) -> Result<(), ProviderError>;
pub fn rebuild(&self, configs: &ResolvedProviderConfigs) -> Result<u64, ProviderError>;
pub fn snapshot(&self) -> Arc<RegistrySnapshot>;
pub fn configured(&self, id: &ProviderId) -> Option<Arc<ConfiguredProvider>>;
pub fn route(&self, id: &RouteId) -> Option<Arc<Route>>;
```

Exact names may follow repository conventions, but semantics are fixed.

**Required behavior:**

- No independent mutable `providers`, `routes`, and `configs` maps.
- Failed rebuild leaves old snapshot and revision unchanged.
- Successful rebuild increments revision once.
- Definition registration rejects duplicates.
- Iteration order is built-in registration order followed by deterministic user-defined order.
- Lock poisoning is converted into an internal provider error or avoided; do not panic.

**Regression tests:**

- Remove stale `registry.model()` test or implement the new explicit model-binding API; never simply delete coverage.
- Failure rollback test.
- Concurrent reader sees either old or new complete snapshot, never a partial state.
- Deterministic order test repeated over multiple rebuilds.

### P2-07: Export only required public API

**Files:** `xai-grok-provider/src/lib.rs` and public doc comments.

Run:

```bash
cargo doc -p xai-grok-provider --no-deps
```

Fix all new missing-doc or broken-link warnings from changed public APIs.

**Phase 2 gate:** Gate T1. C-01, C-03 representation, M-02 core, and M-03 registry order must have passing regression tests. Do not yet mark C-03 closed until provider integration is complete.

---

# Phase 3 — Typed Configuration and Deterministic Precedence

**Objective:** Make provider configuration a first-class typed part of the main config and eliminate out-of-band, duplicated resolution.

### P3-01: Add typed provider sections to shell Config

**Files:**

- Modify `crates/codegen/xai-grok-shell/src/agent/config.rs`.
- Modify `crates/codegen/xai-grok-shell/src/config/tests.rs`.
- Modify provider config serde definitions if required.

**Required behavior:**

- Add typed `provider` map to the main config schema.
- Valid `[provider.openai]` and arbitrary `[provider.my-company]` do not appear in unknown-key diagnostics.
- Unknown keys inside a provider config produce a path-specific warning/error according to existing config policy.
- API key fields are never included in serialized diagnostic snapshots.

### P3-02: Implement one precedence resolver

**Files:**

- Modify `crates/codegen/xai-grok-provider/src/config.rs`.
- Modify `crates/codegen/xai-grok-provider/src/providers/mod.rs`.
- Create `crates/codegen/xai-grok-provider/tests/config_precedence.rs`.

**Precedence:**

```text
built-in defaults
  < environment
  < [provider.<id>] TOML
  < legacy compatibility mapping
  < CLI override
```

Merge fields independently; an override of `base_url` must not erase TOML `extra_headers`. An explicit empty value must follow one documented rule: reject it or clear the field, but never silently treat it inconsistently.

**Tests:** Full matrix for API key, env keys, base URL, headers, model-list endpoint, disabled flag, and profile.

### P3-03: Unify launcher initialization

**Files:**

- Modify `crates/codegen/xai-grok-pager-bin/src/main.rs`.
- Modify `crates/codegen/xai-grok-pager/src/app/mod.rs`.
- Modify `crates/codegen/xai-grok-pager/src/provider_state.rs`.

**Required behavior:**

- Launcher builds the registry once from effective config and CLI arguments.
- Pager `run()` receives the existing registry/runtime object; it does not create another registry.
- Config is parsed once per startup generation.
- The registry is attached to `AgentConfig` and pager state through explicit arguments, not a second initialization side effect.
- If a process-global `OnceLock` remains temporarily, a second distinct registry initialization must return an error, not silently ignore it.

### P3-04: Preserve legacy xAI compatibility explicitly

**Files:**

- Modify launcher compatibility mapping.
- Add tests in `xai-grok-pager-bin` or shell config tests.

**Required compatibility cases:**

- Existing `[endpoints].xai_api_base_url`.
- Existing CLI proxy/session auth route.
- `XAI_API_KEY`.
- Existing bare xAI model selection.
- Existing default models with no provider field.

Each legacy case must map to an explicit xAI provider/route binding and must not depend on provider-name heuristics downstream.

### P3-05: Add manual model provider binding fields

**Files:** Modify `ModelEntryConfig`, `ModelInfo`, `ModelEntry`, conversion code, and tests in `agent/config.rs`.

Add:

- `provider: Option<String>` in TOML-facing config.
- `route: Option<String>` in TOML-facing config.
- validated `provider_id` and `route_id` in resolved entries.

**Rules:**

- Manual non-xAI endpoints must declare `provider` unless the CLI supplies an unambiguous provider.
- Explicit `route` must belong to the selected provider.
- A manual entry with provider but no route uses the provider selector.
- Existing entries without provider remain xAI-compatible under legacy rules.

**Phase 3 gate:** Gate T2 plus config unknown-key tests. M-01 must be eligible for `-Fixed` after fresh evidence.

---

# Phase 4 — Route-to-Sampler Execution Authority

**Objective:** Ensure every network request is determined by the resolved route and protocol without legacy path/header inference.

### P4-01: Add explicit endpoint policy to SamplerConfig

**Files:**

- Modify `crates/codegen/xai-grok-sampler/src/config.rs`.
- Modify sampler config literals in tests using `..Default::default()` where appropriate.
- Add config round-trip tests.

Required fields must represent route endpoint path and query, or an equivalent validated full-request URL policy. Do not store a single already-rendered URL if retries or protocol request variants require safe re-rendering.

### P4-02: Centralize protocol lookup and reject unknown IDs

**Files:**

- Modify `crates/codegen/xai-grok-sampler/src/protocols/mod.rs`.
- Modify `client.rs` and `actor/request_task.rs` dispatch sites.
- Add tests.

**Required behavior:**

- One function validates/looks up protocol ID.
- `protocol_id: Some(unknown)` returns `SamplingError` during client construction or first request before network I/O.
- `protocol_id: None` may temporarily derive from `api_backend` for legacy callers only.
- No `_ => chat_completions` fallback remains.

### P4-03: Make sampler request URL use route endpoint policy

**Files:** Modify `client.rs`, `actor/request_task.rs`, and protocol-specific request code.

**Required behavior:**

- Chat, Responses, and Messages use route-selected paths.
- Base URL and path joining are deterministic.
- Query parameters are preserved and encoded once.
- No provider name/host inspection changes endpoint path.

**Tests:** Local mock server asserts exact method, path, query, and model for all three protocols.

### P4-04: Create one shell route compiler

**Files:**

- Create `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs`.
- Export from `crates/codegen/xai-grok-shell/src/agent/mod.rs`.
- Modify `agent/config.rs` to delegate.

Required API concept:

```rust
pub fn resolve_model_execution(
    model: &ModelEntry,
    registry: &RegistrySnapshot,
    credentials: &RuntimeCredentialContext,
) -> Result<SamplerConfig, ProviderResolutionError>;
```

This function is the only provider-aware constructor of `SamplerConfig`. It must apply:

1. explicit model binding;
2. provider route selector;
3. route endpoint and protocol;
4. model/route/provider generation defaults;
5. header merge;
6. credential resolution;
7. legacy xAI compatibility only at this boundary.

Delete or reduce `resolve_model_route()` so it cannot reconstruct route IDs with string formatting such as `{provider}-{protocol}`.

### P4-05: Remove legacy URL-derived provider header inference

**Files:** `agent/config.rs`, sampler client, tests.

`inject_url_derived_headers()` may remain only for explicitly documented legacy xAI proxy compatibility and must be invoked by the xAI provider/compat route, not globally based on arbitrary URL. Add a regression test proving a third-party endpoint using a similar hostname substring does not receive xAI headers.

### P4-06: Route-default merge semantics

Document and test precedence:

```text
provider defaults < route defaults < model defaults < manual model overrides < per-request override
```

Header precedence:

```text
protocol mandatory headers
  < provider/route static headers
  < user provider extra_headers
  < model extra_headers
  < dynamic tracing headers
  < authentication header (reserved, conflict-checked)
```

Reserved auth headers cannot be overridden by arbitrary extra headers unless an explicit provider auth mode defines that header.

**Phase 4 gate:** Gate T2. C-02 and C-11 regression tests must pass. Do not close C-04/C-08 until Phase 5.

---

# Phase 5 — Authentication, Headers, and Secret Safety

**Objective:** Resolve credentials at the correct runtime boundary, propagate mandatory headers, and fail safely.

### P5-01: Implement runtime credential resolver

**Files:**

- Modify or replace `crates/codegen/xai-grok-shell/src/auth/provider_adapter.rs`.
- Modify `crates/codegen/xai-grok-shell/src/auth/credential_provider.rs` only where needed.
- Add focused tests.

**Required inputs:** inline CLI/TOML key, ordered env keys, current xAI session, public, none.

**Required outputs:** static API key/auth scheme, live bearer resolver, or explicit error.

**Rules:**

- Blank credentials are absent.
- Inline key wins over env only according to resolved precedence.
- Environment is read at resolution/request time, not provider registration time.
- xAI session uses live refresh-compatible resolver.
- Required credential absence is an error before request.
- Public/no-auth is successful and sends no auth header.

### P5-02: Propagate mandatory provider headers

**Files:** Provider definitions and route compiler.

Must test at least:

- Anthropic `anthropic-version` and `x-api-key`.
- OpenAI bearer authorization.
- xAI legacy/session headers.
- OpenCode no-auth/public behavior.
- Ollama no auth.
- User-configured extra header.

Mock servers must inspect actual received headers. Unit tests that only inspect an intermediate map are insufficient.

### P5-03: Detect header conflicts and invalid values

**Files:** provider header validation and sampler request builder tests.

Reject:

- invalid header names;
- CR/LF in values;
- user override of reserved `Authorization` when route auth is bearer;
- simultaneous incompatible `Authorization` and `x-api-key` policies;
- duplicate header values with inconsistent casing.

### P5-04: Eliminate swallowed authentication errors

**Files:** Remove `if let Ok(auth_headers)` patterns from `agent/config.rs` and any equivalent call sites.

Regression test: missing Anthropic/OpenAI key yields typed resolution error and mock server receives zero requests.

### P5-05: Secret redaction audit

Run:

```bash
rg -n "api_key|Authorization|x-api-key|XAI_SESSION_TOKEN|ANTHROPIC_API_KEY|OPENAI_API_KEY" \
  crates/codegen/xai-grok-provider crates/codegen/xai-grok-shell crates/codegen/xai-grok-pager
```

Inspect every logging and `Debug` path. Add tests that format configuration, registry snapshots, errors, and UI state with sentinel secrets and assert absence.

**Phase 5 gate:** Gate T2. C-04, C-07 auth/header portion, and C-08 must be eligible for `-Fixed`.

---

# Phase 6 — Close Every Built-in Provider

**Objective:** Make each V1 provider independently valid, routable, discoverable, and testable.

For every provider task, tests must cover defaults, overrides, route set, route selector, auth policy, mandatory headers, model source, and endpoint validation.

### P6-01: xAI provider

**Files:** `providers/xai.rs`, legacy compatibility tests.

Required routes:

- existing xAI/API route(s) matching current supported protocol behavior;
- CLI proxy/session route if distinct;
- explicit route IDs and protocol IDs.

Preserve existing OAuth/session and API-key behavior.

### P6-02: OpenAI provider

**Files:** `providers/openai.rs` and tests.

Required routes:

- `openai-chat` → Chat Completions.
- `openai-responses` → Responses.

Route selector must have a documented, testable model-policy table. Avoid broad substring guessing. Prefer explicit known prefixes/model capability metadata, with user route override for unknown models. Tests must prove both routes are reachable.

### P6-03: Anthropic provider

**Files:** `providers/anthropic.rs` and tests.

Required:

- Messages endpoint.
- `x-api-key` auth.
- mandatory `anthropic-version` header.
- no OpenAI path or bearer fallback.

### P6-04: OpenCode provider

**Files:** `providers/opencode.rs` and tests.

Required:

- Public mode is represented explicitly.
- Model discovery may run without API key.
- Optional user key, when supplied, follows documented provider behavior.
- No sentinel string such as `public` is accidentally sent as bearer token.

### P6-05: Ollama provider

**Files:** `providers/ollama.rs` and tests.

Required:

- Local HTTP allowed only for loopback/local host.
- Inference path and `/api/tags` discovery are distinct and correct.
- No auth required.
- User remote non-HTTPS Ollama URL is rejected unless explicitly allowed by owner policy.

### P6-06: OpenAI-compatible built-in profiles

**Files:** `providers/openai_compatible.rs`, `providers/mod.rs`, config docs/tests.

Required named profiles at minimum: those already represented in current code/documentation, such as DeepSeek, Groq, and OpenRouter. Each profile has its own provider ID and defaults, while sharing the Chat Completions protocol implementation.

Rules:

- `[provider.deepseek]` config resolves `deepseek`, not a hidden `openai-compatible` singleton.
- `--provider deepseek` works.
- Profiles may override base URL, env keys, headers, and model-list policy.
- Registration rejects duplicate user-defined IDs.

### P6-07: Arbitrary user-defined OpenAI-compatible provider

Implement a provider definition path driven by `[provider.<custom-id>]` with `profile = "openai-compatible"` or an equivalent explicit type field.

Required fields:

- base URL;
- auth mode/env key;
- optional model-list URL;
- optional headers;
- protocol fixed to supported OpenAI-compatible Chat Completions for V1 unless explicitly configured to Responses and validated.

Do not auto-classify arbitrary provider type solely from URL hostname.

### P6-08: Provider matrix E2E tests

**File:** Expand `crates/codegen/xai-grok-provider/tests/provider_e2e.rs` or replace it with focused files.

The provider crate must not manually construct a `SamplerConfig` that bypasses the production route compiler. Cross-crate route-to-sampler E2E belongs in shell integration tests.

**Phase 6 gate:** Gate T1 and T2. C-03, C-09 provider behavior, and C-10 must be eligible for `-Fixed`.

---

# Phase 7 — Asynchronous Model Catalog and Correct Cache

**Objective:** Remove synchronous network discovery from model resolution and provide bounded, observable, deterministic catalog refresh.

### P7-01: Extract provider discovery parsing

**Files:**

- Create `crates/codegen/xai-grok-shell/src/agent/provider_catalog.rs`.
- Move `parse_openai_compatible_provider_models` and `parse_ollama_tags_models` from `agent/models.rs`.
- Add parser fixture tests.

Parsers must be pure, tolerate unknown fields, reject malformed IDs, deduplicate deterministically, and never assign secrets.

### P7-02: Define model catalog snapshots

Required structures:

- provider revision;
- catalog revision;
- per-provider state: `Idle`, `Refreshing`, `Ready`, `Stale`, `Error`, `Disabled`;
- fetched timestamp;
- source URL redacted of userinfo/query secrets;
- models in deterministic order;
- non-secret error summary.

Readers receive immutable `Arc<ModelCatalogSnapshot>`.

### P7-03: Implement async concurrent refresh

**Required behavior:**

- `refresh_all()` uses bounded concurrency.
- Per-provider timeout is configurable with a safe default; use Tokio timeout.
- Cancellation token aborts outstanding requests during shutdown/rebuild.
- One provider failure does not discard successful providers.
- No blocking reqwest client in the catalog service.
- Startup returns before refresh completes.

**Tests:** paused time for timeout, cancellation, concurrent completion ordering, partial failure.

### P7-04: Implement real TTL and cache keys

Remove the current ineffective `PROVIDER_MODEL_CACHE` behavior from `agent/models.rs`.

Cache key must include:

- provider ID;
- effective model-list URL;
- registry/config revision or stable non-secret config fingerprint;
- credential identity class, not credential contents.

Behavior:

- fresh entry: no network;
- stale entry + refresh-if-stale: return stale snapshot immediately and refresh in background;
- force refresh: perform network request;
- failed refresh with stale cache: preserve stale state and error metadata;
- failed refresh without cache: error state with empty dynamic list;
- TTL comparison is covered using paused/injected clock, not real sleep.

### P7-05: Use resolved model-list URL and auth policy

Model-list URL must derive from the configured provider snapshot, including user `base_url` and explicit model-list override. OpenAI-compatible custom providers must never use an empty relative `"/models"` URL.

Tests:

- custom base URL receives `/models` at that host;
- explicit list URL wins;
- Ollama uses `/api/tags`;
- OpenCode public request is sent without auth;
- missing required key skips network and records missing-credential state.

### P7-06: Integrate catalog startup and explicit refresh

**Files:** launcher/runtime initialization, `agent/models.rs`, config reload flow.

- Create catalog service after registry.
- Start background `RefreshIfStale` after UI/session can initialize.
- Expose explicit force refresh for TUI/CLI.
- Remove `fetch_provider_models_blocking()` and all startup calls.

### P7-07: Catalog persistence policy

Use existing config/cache location facilities. Persist only non-secret model metadata and timestamps using atomic write. Corrupt cache must be ignored with warning and replaced, not crash startup.

**Phase 7 gate:** Gate T2. C-05, C-06, C-09 discovery behavior, and C-07 discovery config behavior must be eligible for `-Fixed`.

---

# Phase 8 — Deterministic Model Merge and Resolution

**Objective:** Produce one canonical model catalog and resolve every model reference without ambiguity.

### P8-01: Define source precedence and identity

Implement and document:

```text
manual user model override
  > dynamic provider-discovered model metadata
  > provider static known-model metadata
  > embedded legacy xAI default metadata
```

An override may enrich metadata but may not silently move a model to another provider.

Canonical key for non-legacy models: `provider/model`. Maintain display name separately.

### P8-02: Rewrite provider dynamic-model merge

**Files:** `agent/models.rs`, new provider catalog module, tests.

Remove direct registry network access from `resolve_model_list()`. It consumes catalog snapshots only.

Required deterministic rules:

- provider order from registry snapshot;
- model order from provider source or sorted stable ID when source order is not meaningful;
- duplicates within a provider deduplicated by canonical model ID;
- same bare model under two providers remains two entries;
- bare lookup becomes ambiguous and returns a helpful error.

### P8-03: Resolve CLI model references

**Files:** launcher and model resolver tests.

Cases:

- `--model openai/gpt-4o` selects provider OpenAI and strips only the provider qualifier from wire model slug.
- `--provider openai --model gpt-4o` is equivalent.
- conflicting `--provider anthropic --model openai/gpt-4o` is an error.
- bare model uniquely found in one provider resolves.
- ambiguous bare model is an error listing canonical choices.
- legacy bare xAI defaults continue to resolve predictably.

### P8-04: Route binding for manual and discovered models

Every resolved `ModelEntry` must carry validated `provider_id` and `route_id` or a provider selector input that is resolved before sampler construction. No production path may use `provider_id: None` for a non-legacy external model.

### P8-05: Preserve model capabilities and defaults

Ensure context window, max output, reasoning effort, backend search, structured output, tool calling, stream tool calls, and compaction metadata merge without being overwritten by generic provider defaults when specific model metadata exists.

### P8-06: Model-list CLI and display behavior

Update `cli_models.rs` and pager model presentation to show canonical provider identity without leaking keys or full sensitive URLs. Add snapshot/selection tests.

**Phase 8 gate:** Gate T2 and model tests. M-03 model ordering and manual-model portion of C-02 must be closed by evidence.

---

# Phase 9 — Runtime Lifecycle, Reload, and All Sampling Surfaces

**Objective:** Ensure every runtime sampling path uses the same registry snapshot and route compiler, including reload and subagents.

### P9-01: Introduce provider runtime container

Create a small runtime object, in an existing suitable shell module or `agent/provider_runtime.rs`, holding:

- `Arc<ProviderRegistry>`;
- `Arc<ProviderCatalogService>`;
- current config/registry revision access;
- rebuild/refresh methods;
- cancellation token.

The launcher creates one instance and injects it into shell and pager.

### P9-02: Transactional config reload

Modify config watcher/reloader:

1. Parse new config.
2. Resolve provider configs.
3. Build candidate registry snapshot.
4. Validate existing selected models against candidate snapshot.
5. Atomically publish registry snapshot.
6. Trigger catalog refresh.
7. Rebuild model catalog/session configs.
8. On any failure, retain previous runtime state and report error.

Tests must prove failed reload leaves active request configuration unchanged.

### P9-03: Main session model construction

Search all `SamplerConfig {` constructors and classify them. Main inference paths must delegate to the route compiler. Test initial session and runtime model switch.

### P9-04: Subagent sampling configs

**Files:** `agent/subagent/mod.rs` and related tests.

Subagents must resolve canonical model references through the same provider runtime. They must not clear `protocol_id`, route endpoint, headers, or auth when inheriting from parent. Add tests for OpenAI Responses and Anthropic subagents.

### P9-05: Auxiliary model surfaces

Audit and migrate:

- web-search model;
- session-summary model;
- compaction helper requests;
- goal classifier/auto mode where configurable;
- memory/embedding only if it uses the same LLM model catalog;
- restored sessions;
- ACP model switching.

For each surface, add at least one route-preservation test.

### P9-06: In-flight request semantics during reload

Freeze policy:

- In-flight request keeps the `SamplerConfig`/route snapshot it started with.
- New requests use the new revision.
- Removing the selected model causes a controlled reselection/error after the in-flight turn, not mid-stream mutation.

Add concurrency tests using barriers/channels.

### P9-07: Remove duplicate registry initialization/global ambiguity

After injection is complete, remove or constrain `provider_state::OnceLock`. Tests must ensure one runtime identity is observed by TUI and shell.

**Phase 9 gate:** Gate T2 and T3. M-02 lifecycle must be eligible for closure.

---

# Phase 10 — Real TUI and CLI Provider Configuration Closure

**Objective:** Replace the false `/providers` UI with an effect-driven, persistent, validated workflow.

### P10-01: Define UI-safe provider view model

**Files:** Rewrite `xai-grok-pager/src/provider_state.rs` and update `views/providers_modal.rs`.

View fields:

- provider ID and display name;
- effective endpoint display with sensitive query/userinfo removed;
- config source summary;
- credential state: `NotRequired`, `Configured`, `Missing`, `Session`, `Public`;
- catalog state and last refresh time;
- model count;
- last non-secret error;
- registry/catalog revision.

“Registered” is not a connected/configured status.

### P10-02: Separate modal editing state from runtime state

The modal edits a draft. Keystrokes must not mutate registry/config directly. API key draft must be cleared when modal closes and redacted in `Debug`.

### P10-03: Add provider save effect

**Files:**

- Add provider action/effect variants in existing app dispatch/effects architecture.
- Modify `app/modals.rs` handling.
- Use existing atomic config editing facilities such as `config_toml_edit.rs`/shell persist helpers.

Save flow:

1. Collect draft.
2. Validate provider ID/base URL/header fields.
3. Persist atomically.
4. Request provider runtime rebuild.
5. Force catalog refresh for that provider.
6. Publish success view state.
7. On failure, preserve prior config/runtime and keep actionable error in modal.

`Enter save` must perform this sequence. If implementation is not ready, remove the label rather than leaving a no-op.

### P10-04: API key persistence policy

Use the repository’s existing supported secret/config policy. Do not invent encryption. If current config stores keys in TOML, display an explicit warning and support env-key entry; never echo the stored value back into the UI. Prefer writing `env_key` and leave inline key optional.

### P10-05: Refresh, disable, and rollback actions

Implement only the actions necessary for V1:

- save/update configuration;
- force model refresh;
- disable/enable user-defined provider if schema supports it;
- cancel/back without changes.

Do not add delete/import/export marketplace features.

### P10-06: TUI unit and reducer tests

Cover:

- status derived from runtime truth;
- save dispatches effect;
- success updates state;
- failure preserves previous snapshot;
- secret absent from rendered buffer and debug output;
- Esc cancels draft;
- deterministic provider ordering;
- refresh state transitions.

### P10-07: PTY E2E

Add a focused scenario under existing pager PTY/scenario harness:

- open `/providers`;
- configure a local mock OpenAI-compatible provider using a non-secret env key reference;
- save;
- observe configured state and model count after refresh;
- select its model;
- send a prompt to local mock server;
- verify streamed response appears.

If full config entry is impractical in PTY, split persistence into reducer integration and use a prewritten temp config for the PTY inference flow. Both must still exist.

### P10-08: Headless CLI parity

Add or update CLI commands/help so users can configure via TOML/flags and list canonical provider/model state without TUI. Do not require TUI for production operation.

**Phase 10 gate:** Gate T3 plus focused PTY E2E. C-12 must be eligible for `-Fixed`.

---

# Phase 11 — Security, Resilience, and Observability

**Objective:** Harden the production boundary against misconfiguration, leaks, hangs, and opaque failures.

### P11-01: Endpoint/SSRF policy audit

Audit every provider inference and model-list URL path. Tests must prove:

- invalid schemes rejected;
- embedded credentials rejected;
- remote plain HTTP rejected;
- loopback local HTTP accepted where allowed;
- redirects follow existing safe reqwest policy and do not silently forward auth to a different origin. Configure redirect behavior explicitly if current client would forward sensitive headers.

### P11-02: Timeouts and cancellation

Define and test:

- connection/request timeout for discovery;
- streaming idle timeout for inference;
- cancellation on shutdown/model switch where existing behavior requires it;
- no global startup wait on provider discovery.

Do not add a total inference timeout that breaks long valid streams unless separately configured.

### P11-03: Retry policy classification

Ensure:

- auth/config/invalid protocol errors are non-retryable;
- rate limit and transient transport behavior preserve existing sampler policy;
- provider discovery retries are bounded and do not multiply across refresh calls;
- no retry sends a request to a fallback provider/endpoint.

### P11-04: Structured non-secret diagnostics

Add tracing fields:

- provider ID;
- route ID;
- protocol ID;
- registry revision;
- catalog revision;
- model canonical ID;
- request endpoint origin/path without secret query values;
- refresh outcome and latency.

Never log headers, keys, full auth errors containing tokens, or request bodies by default.

### P11-05: Offline and degraded behavior

Tests:

- no network: app starts with static/manual/embedded catalog;
- stale cache: stale models remain selectable with status indication;
- selected dynamic model unavailable and no cache: actionable error, no panic;
- one provider failing does not remove healthy provider models;
- corrupt cache is quarantined/ignored.

### P11-06: Panic/unwrap audit in changed path

Run:

```bash
rg -n "unwrap\(|expect\(|panic!|unreachable!" \
  crates/codegen/xai-grok-provider/src \
  crates/codegen/xai-grok-shell/src/agent/provider_* \
  crates/codegen/xai-grok-pager/src/provider_state.rs \
  crates/codegen/xai-grok-pager/src/views/providers_modal.rs
```

Production occurrences require explicit justification or replacement. Test-only unwraps are permitted under repository policy.

**Phase 11 gate:** Gate T2/T3 plus all security regression tests.

---

# Phase 12 — Cross-Surface E2E and Backward Compatibility

**Objective:** Prove the actual product chain, not only isolated units.

### P12-01: Build shared local provider mock harness

Use existing `xai-grok-test-support` or create focused additions there. Harness must support:

- Chat Completions SSE;
- Responses SSE;
- Messages SSE;
- model-list endpoints;
- request capture for path/query/headers/body;
- scripted failures, delay, cancellation, and malformed frames.

Do not create a second unrelated mock framework if existing support can be extended.

### P12-02: Provider request matrix E2E

At minimum:

| Provider | Protocol | Auth | Discovery | Inference |
|---|---|---|---|---|
| xAI API-key compatibility | configured protocol | bearer/current behavior | mock list | streamed turn |
| xAI session compatibility | existing route | live bearer | existing/static behavior | streamed turn |
| OpenAI | Chat | bearer | `/models` | streamed turn |
| OpenAI | Responses | bearer | `/models` | streamed turn |
| Anthropic | Messages | `x-api-key` + version | configured/static/list behavior | streamed turn |
| OpenCode | Chat | public/no auth | public list | streamed turn |
| Ollama | Chat | none | `/api/tags` | streamed turn |
| custom OpenAI-compatible | Chat | configured | overridden base/list URL | streamed turn |

Each test must traverse typed config → registry rebuild → catalog → model resolution → route compiler → sampler → mock server → decoded events.

### P12-03: Config precedence E2E

Use isolated environment guards and temp config files. Prove each precedence layer changes the actual request destination/header, not only intermediate structs.

### P12-04: Hot reload E2E

- Start with provider A local server.
- Complete one request.
- Rewrite config atomically to provider B endpoint.
- Trigger reload.
- Confirm new request reaches B and A receives no second request.
- Inject invalid reload and confirm last good B route remains active.

### P12-05: Model switch and subagent E2E

Switch between protocols/providers in one process. Confirm no stale auth/header/path crosses between clients. Launch one subagent on a non-default provider and verify route.

### P12-06: Legacy xAI regression suite

Run and extend existing tests for default startup, OAuth/session, API key, CLI proxy, default model, web search, compaction, and restored sessions. Any regression blocks release even if third-party providers work.

### P12-07: No-network test enforcement

Provider tests must bind only loopback. Add an environment/network guard if the existing harness supports it. At minimum, grep tests for production provider hostnames and review all intentional constants.

**Phase 12 gate:** Gate T4 and the full provider matrix. All C/M audit items must be `-Fixed`; none are `-Closed` until final audit.

---

# Phase 13 — CI, Cross-Platform, Packaging, and Release Engineering

**Objective:** Make closure reproducible outside one developer machine.

### P13-01: Add/repair CI workflows

**Files:** Create `.github/workflows/provider-adapter.yml` and integrate with existing CI if present in the real checkout. If upstream CI files exist but were absent from the archive, modify rather than duplicate.

Required jobs:

1. Linux targeted gate.
2. Windows targeted gate with `protoc` setup.
3. macOS targeted gate only if upstream supports it.
4. Full workspace check/clippy/test.
5. Documentation build.
6. Mocked provider E2E.

Pin Rust through `rust-toolchain.toml`; do not silently use latest.

### P13-02: Separate fast PR and full release gates

Fast PR gate may run targeted crates and focused E2E. Full release gate must run Gate T5. Both must be required before merge/release according to repository policy.

### P13-03: Cross-platform path and process audit

Provider code must not introduce platform-specific path assumptions. Config/cache atomic writes use existing cross-platform utilities. Run Windows-specific tests for file replacement and config reload.

### P13-04: Packaging smoke test

Build the existing binary/distribution artifact from a clean checkout. Run:

- `--help` showing provider flags;
- headless/list-models command against local mock configuration;
- one TUI or CLI mock inference smoke as supported by CI.

Do not invent a new package format.

### P13-05: Optional live smoke workflow

Create a manually triggered, non-required workflow only if repository secrets policy permits. It may test provider APIs with environment secrets, but must:

- never print secrets;
- use inexpensive minimal requests;
- be disabled for forks;
- not be required for deterministic release gate;
- clearly distinguish provider outage from code regression.

### P13-06: Migration and operations documentation

**Files:**

- Update relevant README/user guide.
- Create `docs/provider-adapter-v1/migration.md`.
- Create `docs/provider-adapter-v1/troubleshooting.md`.
- Create `docs/provider-adapter-v1/rollback.md`.

Include config examples for all providers, precedence, canonical model syntax, offline behavior, cache refresh, error messages, and reverting to legacy xAI-only configuration.

**Phase 13 gate:** Required CI jobs pass on a clean commit. Record workflow run IDs/URLs in `PROGRESS.md` if available.

---

# Phase 14 — Cleanup, Final Static Audit, and Release Candidate Cut

**Objective:** Remove obsolete dual paths, verify issue closure, and create a defensible release candidate.

### P14-01: Remove dead provider-adapter paths

Search and eliminate or explicitly deprecate:

- independent route/config maps;
- eager `Credential::resolve()` in provider construction;
- route-level framing production use;
- string-formatted route lookup;
- blocking provider discovery functions/cache;
- duplicate registry initialization;
- no-op provider save behavior;
- protocol unknown fallback;
- out-of-band provider TOML parsing that bypasses typed config.

Run `rg` evidence and include results in final audit.

### P14-02: Audit every SamplerConfig constructor

Run:

```bash
rg -n "SamplerConfig\s*\{" crates/codegen
```

Classify each constructor in `docs/provider-adapter-v1/final-audit.md`:

- provider-aware production path using route compiler;
- internal protocol test fixture;
- legacy non-provider service with documented reason;
- unacceptable bypass to fix before release.

No unclassified production constructor is allowed.

### P14-03: Re-run original audit issue-by-issue

For each C-01 through M-03:

- cite the regression test;
- cite the implementation file;
- record fresh command output;
- change `-Fixed` to `-Closed` only after independent review of the evidence.

Add any newly discovered issue rather than hiding it. A new critical issue blocks RC.

### P14-04: Full clean-checkout verification

From a fresh clone/worktree at the candidate commit:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo doc --workspace --no-deps
```

Then run targeted product/provider E2E and packaging smoke. Record command outputs, platform, toolchain, commit, and durations in `docs/provider-adapter-v1/release-gate-report.md`.

### P14-05: Final diff and scope review

Commands:

```bash
git diff <baseline-commit>...HEAD --stat
git diff <baseline-commit>...HEAD --name-only
git log --oneline <baseline-commit>..HEAD
```

Review for:

- unrelated changes;
- accidental generated files;
- secret material;
- disabled tests/lints;
- broad formatting churn;
- unapproved dependencies;
- incomplete documentation.

### P14-06: Release candidate commit and tag

Only after all gates pass:

1. Update `PROGRESS.md` with final closure summary.
2. Commit final docs/audit as `docs: record provider adapter V1 release gate`.
3. Create an annotated RC tag using repository naming policy, for example `provider-adapter-v1-rc.1`, only with owner approval.
4. Do not force-push or rewrite phase commits.

**Phase 14 gate:** Gate T5 passes on required platforms; final audit contains zero open critical/high provider-adapter defects; rollback instructions are tested; repository is clean.

---

## 9. Required Test Inventory

The final tree must contain automated coverage for all rows below.

### Provider core

- ID validation.
- endpoint validation/joining.
- route validation.
- route-set/default/selector validity.
- transactional rebuild and rollback.
- deterministic order.
- secret-redacted Debug/error.
- public/no-auth versus missing credential.

### Configuration

- typed `[provider.*]` parsing.
- unknown nested key diagnostics.
- field-wise precedence.
- CLI targeting and conflict errors.
- legacy `[endpoints]` mapping.
- manual model provider/route binding.
- atomic persistence and failed-save rollback.

### Protocol/transport

- exact Chat path/query/headers/body and SSE decode.
- exact Responses path/query/headers/body and event decode.
- exact Messages path/query/headers/body and SSE decode.
- unknown protocol rejected.
- malformed stream errors.
- auth failure causes zero network requests.
- retry does not change endpoint/provider.

### Catalog

- parser fixtures for OpenAI-compatible and Ollama.
- public OpenCode discovery.
- configured base/list URL.
- TTL fresh/stale/force.
- stale fallback.
- cancellation and timeout.
- partial provider failure.
- corrupt persisted cache.
- deterministic merge.

### Runtime

- initial model resolution.
- cross-provider model switch.
- hot reload success/failure rollback.
- in-flight snapshot consistency.
- subagent route inheritance/override.
- web-search/session-summary/compaction route preservation.
- restored session canonical model reference.

### UI/CLI

- true provider status.
- secret-free render.
- save effect.
- refresh effect.
- validation error.
- cancellation.
- PTY provider-to-inference flow.
- headless parity.

## 10. Phase Completion Record Template

Append this exact structure to `PROGRESS.md` after each phase:

```markdown
## Provider Adapter V1 — Phase N: <name> — YYYY-MM-DD

### Base and result
- Start commit: `<hash>`
- End commit: `<hash>`
- Tasks completed: `Pn-01`, `Pn-02`, ...

### Files changed
- `path`: purpose

### Regression issues
- `C-xx`: test path and test name

### Verification
- `command` — exit 0 — `<N> passed; 0 failed`
- `command` — exit 0 — no warnings

### Deferred or blocked
- None

### Scope review
- No unrelated files changed.
- No dependency added.
- No secret present in diff or logs.
```

If any item is not true, replace it with the exact failure. Never write “None” by default without checking.

## 11. Agent Response Template After Each Task

```text
Task: Pn-mm <name>
Status: PASS | BLOCKED | FAIL
Commit: <hash or none>
Files changed:
- <path>

Evidence:
- <exact command> -> exit <code>, <test counts>
- <exact command> -> exit <code>, <warning count>

Behavior proven:
- <specific observable behavior>

Remaining scope:
- <next task only>

Known failures:
- <none or exact failure>
```

## 12. Stop-the-Line Conditions

The agent must stop immediately and not proceed when any condition occurs:

- The current checkout does not match the expected branch/base and ownership is unclear.
- A required test cannot be made deterministic without changing architecture.
- Fixing a task requires an unlisted third-party dependency.
- A change would break xAI OAuth/session behavior and no compatibility test explains the intended change.
- Registry rebuild cannot be made transactional with the selected structure.
- A provider requires an unsupported protocol or signing scheme outside V1.
- A secret appears in logs, snapshots, or committed files.
- Full gate reveals unrelated pre-existing failures that cannot be isolated from this branch.
- Three attempts at one root cause fail.
- The agent is tempted to skip, ignore, delete, or weaken a failing test.

The correct response is a focused blocker report, not speculative continuation.

## 13. Final Acceptance Checklist

The release reviewer must check every box manually from evidence:

- [ ] One registry/runtime instance is injected into shell and pager.
- [ ] Registry snapshots are atomic, revisioned, deterministic, and rollback-safe.
- [ ] All providers expose complete route sets and selectors.
- [ ] OpenAI Chat and Responses routes are both exercised end-to-end.
- [ ] Anthropic mandatory headers and Messages protocol pass wire tests.
- [ ] OpenCode works in public mode without a key.
- [ ] Ollama works against loopback HTTP and `/api/tags`.
- [ ] Named and custom OpenAI-compatible providers are reachable.
- [ ] Typed provider config has no unknown-section warning.
- [ ] Precedence changes actual outgoing requests as documented.
- [ ] Manual models bind provider and route.
- [ ] Unknown protocol/route/provider and missing credentials fail explicitly.
- [ ] No localhost fallback exists for malformed endpoint configuration.
- [ ] Startup performs no synchronous provider discovery.
- [ ] Catalog TTL, stale fallback, timeout, cancellation, and partial failure are tested.
- [ ] Model order and ambiguity behavior are deterministic.
- [ ] Main session, subagent, web search, summary, compaction, and restore use the route compiler.
- [ ] `/providers` persists and reloads real configuration.
- [ ] UI and logs redact secrets.
- [ ] Linux and Windows required CI pass.
- [ ] Full workspace check, clippy, tests, and docs pass.
- [ ] Packaging smoke passes from a clean checkout.
- [ ] Original audit issues are independently reviewed and closed.
- [ ] Rollback procedure has been exercised.

Only after all boxes are checked is the first provider-adapter production closure complete.
