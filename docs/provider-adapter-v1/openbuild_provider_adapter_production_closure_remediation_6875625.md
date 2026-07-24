# Provider Adapter V1 Production Closure Remediation Guide

> **Execution baseline:** `feat/provider-adapter` at `6875625349a4a067dbb41d1249adae4143fc35d9`
>
> **Supersedes:** `docs/provider-adapter-v1/openbuild_provider_adapter_production_closure_remediation_458d0ce.md`
>
> **Scope:** This document guides remediation only. It does **not** create, replace, or update an ISSUE list. Existing ISSUE documents remain historical references; this guide's task IDs are execution IDs, not issue IDs.

**Goal:** Bring the current Provider Adapter implementation from its present partially-integrated state to a demonstrable first production closure on Linux, Windows, and macOS, with one authoritative provider execution path, strict configuration semantics, a live model catalog, production-faithful end-to-end tests, and blocking release gates.

**Architecture:** All model requests must pass through one deterministic chain:

```text
CLI / TOML / legacy migration
    -> ResolvedProviderSet
    -> transactional ProviderRegistry snapshot
    -> canonical ModelEntry
    -> ResolvedModelExecution
    -> async prepare_sampler_config
    -> PreparedSamplerConfig
    -> Sampler
```

No sampler, auxiliary model, model switch, session creation path, TUI path, or background service may construct an alternative request configuration or silently fall back to a legacy provider path.

**Tech stack:** Rust 1.92.0, Tokio, Serde/TOML, Reqwest, GitHub Actions, Windows MSVC, Linux GNU, macOS, existing `xai-grok-provider`, `xai-grok-sampler`, `xai-grok-shell`, `xai-grok-pager`, and `xai-grok-paths` crates.

---

## 1. Authority and non-negotiable execution rules

### 1.1 Document authority

This guide is the only implementation-order authority for this remediation cycle.

The agent must not:

- replace this guide with a new plan;
- merge or reorder phases;
- mark a task complete because a nearby task appears equivalent;
- reinterpret a failure as "pre-existing" and skip it;
- reduce a gate so that current code passes;
- create a separate ISSUE document;
- alter historical ISSUE statuses as a substitute for implementation evidence;
- treat commit messages, prior CI logs, or comments such as `P7-003` as proof of closure.

If a hidden prerequisite appears, the agent must:

1. stop the current task;
2. create `docs/provider-adapter-v1/execution-v3/blockers/<TASK-ID>.md`;
3. record the failing command, complete error, root-cause hypothesis, affected files, and the smallest prerequisite task;
4. complete that prerequisite as a child task named `<TASK-ID>-P1`, `<TASK-ID>-P2`, and so on;
5. return to the parent task;
6. not edit the phase order.

A blocker note is execution evidence, not a new issue list.

### 1.2 Task granularity

Each task in this guide is one independently reviewable unit. One task must produce:

- one behavioral change;
- one or more tests proving that change;
- one evidence file;
- one commit.

The agent may not combine two task IDs into one commit. Documentation-only gate updates may be included only in the task whose evidence they describe.

Commit format:

```text
provider-v1(<TASK-ID>): <imperative summary>
```

Example:

```text
provider-v1(R3-ASYNC-02): make model switch await prepared sampling config
```

### 1.3 Definition of "complete"

A task is complete only when all conditions are true:

1. its required failing test was observed before the fix;
2. the test passes after the fix;
3. all task-specific commands pass;
4. no new `allow`, `ignore`, `cfg` exclusion, `continue-on-error`, fallback, or panic was introduced;
5. `git diff --check HEAD^..HEAD` passes;
6. the evidence file contains command output and commit hash;
7. no unrelated source file changed.

### 1.4 Forbidden shortcuts

The following changes are prohibited unless a task explicitly requires them:

```rust
#[ignore]
#[cfg(not(windows))]
#[cfg(unix)]
#[allow(dead_code)]
#[allow(unused_*)]
unimplemented!()
todo!()
panic!()
.unwrap()
.expect("...")
```

The prohibition applies to new production code and to changes that hide a newly exposed test failure. Existing occurrences must not be expanded.

Also prohibited:

- replacing a hard error with a log and fallback;
- treating unknown provider configuration as a default provider;
- using model-name string heuristics in Sampler;
- directly reading provider credential environment variables outside the credential resolver;
- constructing `SamplingConfig` with `Default::default()` in production;
- creating a new `ProviderRuntime` when the shared runtime is absent;
- deleting a target file before rename on Windows;
- changing CI to targeted tests before the full gate is green;
- claiming Windows support from Linux cross-compilation alone.

### 1.5 Evidence directory

Create and maintain:

```text
docs/provider-adapter-v1/execution-v3/
  baseline/
  blockers/
  phases/
  gates/
  release/
```

Each task writes:

```text
docs/provider-adapter-v1/execution-v3/phases/<TASK-ID>.md
```

Required evidence template:

```markdown
# <TASK-ID> Evidence

- Baseline commit:
- Result commit:
- Files changed:
- Failing test before fix:
- Failure output summary:
- Passing test after fix:
- Targeted check:
- Targeted clippy:
- `git diff --check HEAD^..HEAD`:
- Deviations: none
```

"Deviations: none" may only be written when no child prerequisite was needed.

---

## 2. Frozen technical contracts

The implementation must converge on these contracts. The agent may refine names only when required by existing code, but may not change semantics.

### 2.1 Async request preparation contract

The production API must be asynchronous:

```rust
pub async fn prepare_sampling_config_for_model(
    &self,
    model: &ModelEntry,
    origin_client: Option<OriginClientInfo>,
) -> Result<SamplingConfig, AgentConfigError>;
```

Preferred final form, if call-site migration permits it without a second bridge:

```rust
pub async fn prepare_sampling_config_for_model(
    &self,
    model: &ModelEntry,
    origin_client: Option<OriginClientInfo>,
) -> Result<PreparedSamplerConfig, AgentConfigError>;
```

Only one of these forms may remain public inside Shell. If the first form is retained, the conversion from `PreparedSamplerConfig` to `SamplingConfig` must happen exactly once in that function.

The following production functions must also be async and return `Result`:

```rust
resolve_sampling_config_for_model
apply_agent_model_override
resolve_aux_model_sampling_config
resolve_web_search_sampling_config
```

No production function in the Provider request path may call:

```rust
tokio::runtime::Handle::block_on
Runtime::block_on
futures::executor::block_on
```

Test-only runtimes remain allowed when they are outside an existing Tokio runtime.

### 2.2 Provider configuration fidelity contract

Every parsed provider entry must retain these fields through Registry commit:

```text
enabled
kind
profile
api_key
env_key
base_url
protocol
model_list_path
model_list_format
allow_insecure_http
extra_headers
```

The authoritative runtime type is `ProviderRuntimeConfig`. Built-in providers and the OpenAI-compatible factory must consume the same field semantics.

Profiles provide defaults only. User configuration overrides profile defaults field by field.

### 2.3 Credential priority contract

The fixed candidate priority is:

```text
request inline override
model inline credential
provider inline credential
provider-configured environment variables, in declared order
provider session / OAuth resolver
legacy compatibility source, only when explicitly migrated
```

Missing credential is different from credential backend failure.

The resolver must return:

```rust
Result<Option<SecretValue>, CredentialError>
```

It must not convert environment decoding errors or session backend failures into `None`.

### 2.4 Header merge contract

The only allowed merge order is:

```text
transport-required
route-mandatory
provider extra headers
resolved authentication header
request-level overrides
```

Rules:

- transport-required headers cannot be removed;
- route-mandatory protocol headers cannot be removed;
- request-level overrides cannot replace authorization headers;
- case-insensitive duplicate names must be detected;
- secrets must not appear in `Debug`, tracing, error messages, snapshots, or evidence files.

### 2.5 Route selection contract

A route selector may use only canonical provider/model metadata and explicit configuration. It may not inspect URL domains or recreate protocol decisions in Sampler.

OpenAI selection must be deterministic:

- explicit `route_id` wins;
- explicit provider protocol selects the matching route;
- model metadata requiring Responses selects `openai-responses`;
- all other OpenAI models select `openai-chat`;
- an explicit incompatible combination returns an error.

The exact model metadata representation must be documented in `public-contracts.md` and tested. Do not hard-code an expanding list in Sampler.

### 2.6 Catalog lifecycle contract

A production runtime owns exactly one Catalog worker.

On startup:

1. load persisted snapshot;
2. publish cached models as stale-but-visible;
3. start refresh for enabled dynamic providers;
4. publish a revision when each provider transitions;
5. atomically persist the resulting snapshot.

On provider config commit:

1. calculate added, changed, and removed providers;
2. cancel superseded provider refreshes;
3. refresh added and changed providers;
4. remove deleted providers;
5. emit one monotonic revision per committed visible state;
6. persist atomically.

A failed refresh with prior models keeps prior models visible and marks the provider stale/failed. A failed first refresh has no models and reports the error.

### 2.7 Runtime singleton contract

The Agent, Pager, Providers modal, ConfigReloader, Catalog worker, and model views must share the same `Arc<ProviderRuntime>`.

When runtime injection is absent, UI must return `ProviderRuntimeUnavailable`; it must not create `ProviderRuntime::new()`.

### 2.8 Cross-platform support contract

Linux, Windows, and macOS are release platforms. "Compiles" is insufficient.

Each platform must prove:

- workspace check;
- workspace Clippy with warnings denied;
- workspace test build;
- Provider/Sampler/Shell/Pager behavior tests;
- production Provider chain E2E;
- configuration atomic-replace behavior;
- process, path, terminal, and PTY capabilities applicable to that platform;
- package/binary startup smoke test.

A platform exclusion is allowed only when the feature is genuinely absent on the target OS and an explicit capability result is tested.

---

## 3. Gate hierarchy

### G0 — Baseline captured

All current failures are captured without changing source behavior.

### G1 — Task gate

The current task's red/green test, targeted check, targeted Clippy, format, and diff check pass.

### G2 — Subsystem gate

The full affected crates pass on Linux:

```bash
cargo fmt --all -- --check
cargo check -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell -p xai-grok-pager --all-targets --locked
cargo clippy -p xai-grok-provider -p xai-grok-sampler -p xai-grok-shell -p xai-grok-pager --all-targets --locked -- -D warnings
cargo test -p xai-grok-provider --all-targets --locked
cargo test -p xai-grok-sampler --all-targets --locked
cargo test -p xai-grok-shell --all-targets --locked
cargo test -p xai-grok-pager --all-targets --locked
```

### G3 — Platform gate

The complete required matrix passes independently on Linux, Windows, and macOS.

### G4 — Release gate

A fresh clone from the release commit passes all gates, packaging, smoke tests, repository hygiene checks, and documentation validation. Only then may a new RC tag be created.

---

# Phase 0 — Freeze the latest baseline

**Purpose:** Prevent the agent from hiding current failures or using stale evidence.

## R3-BASE-01 — Verify repository identity

**Files:**

- Create: `docs/provider-adapter-v1/execution-v3/baseline/repository.md`

**Actions:**

```bash
git status --short
git branch --show-current
git rev-parse HEAD
git fsck --full
git submodule status --recursive
git diff --check origin/main...HEAD
```

Record the full output and verify HEAD equals:

```text
6875625349a4a067dbb41d1249adae4143fc35d9
```

Do not clean whitespace in this task. Record all current `git diff --check` failures as baseline debt.

**Gate:** no source change; evidence commit only.

## R3-BASE-02 — Capture toolchain and host environment

**Files:**

- Create: `docs/provider-adapter-v1/execution-v3/baseline/environment.md`

**Commands:**

```bash
rustc --version --verbose
cargo --version
rustup show
protoc --version
cmake --version
python --version
node --version || true
git --version
```

On Windows also record:

```powershell
$PSVersionTable
Get-ComputerInfo | Select-Object WindowsProductName,WindowsVersion,OsArchitecture
Get-Volume | Select-Object DriveLetter,SizeRemaining,Size
```

On Linux/macOS record:

```bash
uname -a
df -h .
ulimit -a
```

**Gate:** Rust must be 1.92.0. A different toolchain is a blocker, not permission to edit `rust-toolchain.toml`.

## R3-BASE-03 — Capture unmodified compile and test failures

**Files:**

- Create: `docs/provider-adapter-v1/execution-v3/baseline/linux.md`
- Create via CI: corresponding `windows.md` and `macos.md`

Run without changing source:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked --keep-going
cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings
cargo test --workspace --all-targets --locked --no-run --keep-going
cargo test --workspace --all-targets --locked -- --list
cargo doc --workspace --no-deps --locked
```

Each failure must be recorded by:

```text
platform | command | crate/target | first root error | source path | pre-existing/new
```

The agent may not proceed to source remediation until G0 evidence exists for all three platforms.

## R3-BASE-04 — Freeze invariant scans

**Files:**

- Create: `scripts/provider-v1/assert-provider-v1-invariants.py`
- Create: `docs/provider-adapter-v1/execution-v3/baseline/invariants.md`

The script must fail on production occurrences of:

```text
Handle::block_on in provider request preparation
Runtime::block_on in provider request preparation
route-compiler error followed by legacy sampling_config_for_model fallback
ProviderRuntime::new() in Pager production paths
unimplemented!/todo! in xai-grok-provider production code
continue-on-error in release-gate jobs
unknown-field-tolerant provider schema
```

The script must distinguish `#[cfg(test)]` modules and test files from production.

Add unit tests for the scanner under:

```text
scripts/provider-v1/tests/test_assert_provider_v1_invariants.py
```

**Gate:** scanner must fail against the current baseline for the known violations. A scanner that passes before remediation is defective.

---

# Phase 1 — Create production-faithful red tests

**Purpose:** Reproduce every remaining blocker before changing implementation.

## R3-RED-01 — Reproduce nested-runtime failure through ACP session creation

**Files:**

- Create: `crates/codegen/xai-grok-shell/tests/provider_async_session_entry.rs`
- Modify only as test seam: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`

The test must:

1. start a Tokio multi-thread runtime;
2. bootstrap a provider-bound model from TOML;
3. invoke the same async session creation entry used by ACP;
4. reach model request preparation;
5. assert the operation returns a typed result instead of panicking.

Before the fix, catch and prove the nested-runtime panic using `tokio::spawn` and `JoinError::is_panic()`.

Do not call `sampling_config_for_model_with_registry()` directly; that would bypass the defect.

## R3-RED-02 — Reproduce model-switch nested-runtime failure

**Files:**

- Create or extend: `crates/codegen/xai-grok-shell/tests/provider_async_model_switch.rs`

Exercise `agent/handlers/model_switch.rs` through its public handler boundary. The initial test must fail because synchronous preparation is invoked inside async execution.

## R3-RED-03 — Reproduce auxiliary-model and web-search blocking paths

**Files:**

- Create: `crates/codegen/xai-grok-shell/tests/provider_async_aux_models.rs`

Cover:

- session summary or compact auxiliary model;
- image-description auxiliary model if enabled by existing test seams;
- Web Search model resolution.

The test must prove these paths do not construct a separate xAI model when a provider-bound auxiliary model is configured.

## R3-RED-04 — Reproduce hard-error fallback

**Files:**

- Extend: `crates/codegen/xai-grok-shell/tests/provider_production_chain.rs`

Use a provider-bound model with one of:

- missing required credential;
- unknown route ID;
- invalid endpoint;
- explicit incompatible protocol.

Assert the current code incorrectly returns a `SamplingConfig` or continues toward request construction. The final expected behavior will be a typed error.

## R3-RED-05 — Reproduce built-in Provider configuration loss

**Files:**

- Create: `crates/codegen/xai-grok-provider/tests/builtin_config_fidelity.rs`

Create table-driven tests for `xai`, `openai`, `anthropic`, `opencode`, and `ollama`.

For each provider, configure:

```text
custom base_url
custom protocol where supported
provider inline key
custom env_key list
extra_headers
model_list_path
model_list_format
allow_insecure_http
```

Assert the resulting `ConfiguredProvider` and routes preserve or explicitly reject every field. A field may not be silently ignored.

## R3-RED-06 — Reproduce OpenAI route-selection error

**Files:**

- Create: `crates/codegen/xai-grok-provider/tests/openai_route_selection.rs`

Required cases:

```text
explicit chat route -> chat
explicit responses route -> responses
provider protocol=responses -> responses
model metadata requires responses -> responses
generic model -> chat
explicit incompatible metadata/protocol -> error
```

At least one current case must fail before implementation.

## R3-RED-07 — Reproduce tolerant configuration parsing

**Files:**

- Create: `crates/codegen/xai-grok-provider/tests/strict_provider_config.rs`

Required failing inputs:

```toml
implementation = "openai-compatible"
kind = "unknown_kind"
protocol = "unknown_protocol"
model_list_format = "unknown_format"
[provider.custom]
base_url = "https://example.test/v1"
```

The last case must fail because custom provider identity requires explicit `kind` or `profile` under the frozen contract.

Also test duplicate Provider identity after merged configuration layers.

## R3-RED-08 — Reproduce Catalog lifecycle defects

**Files:**

- Create: `crates/codegen/xai-grok-shell/tests/provider_catalog_lifecycle.rs`

Required scenarios:

1. persisted stale models are visible immediately after bootstrap;
2. refresh failure preserves prior models;
3. provider deletion emits a revision notification;
4. a superseded refresh cannot publish after a newer provider revision;
5. shutdown awaits or cancels all refresh tasks;
6. config commit starts refresh automatically.

Use a deterministic mock HTTP server and barriers; do not use wall-clock sleeps as the primary synchronization method.

## R3-RED-09 — Reproduce credential backend error loss

**Files:**

- Create: `crates/codegen/xai-grok-provider/tests/credential_errors.rs`

Use test resolvers that return:

```text
not found
invalid Unicode environment value where platform permits
session expired
session backend I/O failure
malformed credential
```

Assert backend failure is distinguishable from absence.

## R3-RED-10 — Reproduce ineffective `allow_insecure_http`

**Files:**

- Extend: `crates/codegen/xai-grok-provider/tests/endpoint_security.rs`

Required cases:

```text
http localhost + false -> allowed under local exception
http remote + false -> rejected
http remote + true -> behavior chosen by Phase 8 contract
https remote -> allowed
```

This test must force a product decision; the field cannot remain inert.

## R3-RED-11 — Reproduce Pager split Runtime

**Files:**

- Create: `crates/codegen/xai-grok-pager/tests/provider_runtime_injection.rs`

With no injected runtime, opening the Provider modal must return an explicit unavailable state. The current code's empty Runtime fallback must make the red test fail.

## R3-RED-12 — Reproduce model-cache atomic-write defect

**Files:**

- Create: `crates/codegen/xai-grok-shell/tests/model_cache_atomicity.rs`

Test:

- concurrent writers;
- existing destination replacement;
- injected replacement failure;
- no fixed `.tmp` collision;
- no partial JSON after interruption seam;
- Windows replacement behavior through the shared atomic primitive.

## R3-RED-13 — Reproduce exclusion-ledger identity break

**Files:**

- Extend: `scripts/provider-v1/tests/test_scan_platform_exclusions.py`

The test must compare the committed ledger against scanner output and demonstrate:

- stable IDs do not change when unrelated lines are inserted;
- old ledger IDs can be migrated;
- `not(windows)`, `cfg(unix)`, `all(test, unix)`, and `cfg_attr(...windows..., ignore)` are detected;
- duplicate syntax forms map to one canonical classification entry.

## R3-RED-14 — Establish production-chain E2E red fixture

**Files:**

- Create: `crates/codegen/xai-grok-shell/tests/provider_real_entry_e2e.rs`

The fixture must enter through:

```text
TOML/CLI context
-> bootstrap_from_config
-> App/Agent construction
-> ACP session or equivalent production session entry
-> model selection
-> request preparation
-> mock server
```

It must not manually construct `ModelEntry::fallback`, manually pass a key directly to a helper, or call the route compiler as the top-level action.

Initial red cases:

- built-in OpenAI with provider inline key;
- Anthropic custom env key and mandatory header;
- custom OpenAI-compatible Provider;
- Responses route;
- missing credential hard error;
- hot reload from provider A to provider B.

**Phase 1 gate:** all new tests compile; each designated red test fails for the expected root cause, not due to unrelated setup errors.

---

# Phase 2 — Make the production request chain fully asynchronous

## R3-ASYNC-01 — Introduce the async preparation result type

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/config.rs`
- Modify: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`

Define one Shell-level error enum that wraps:

```rust
ProviderError
RequestPreparationError
CredentialError
ModelResolutionError
```

No string-only conversion at internal boundaries.

Convert `prepare_sampling_config_for_model` to `async fn` returning `Result`.

Delete its `Handle::current()` and `block_on` use.

**Test:** R3-RED-01 reaches a typed error or success without panic.

## R3-ASYNC-02 — Migrate ACP session creation

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`

Await async request preparation at the true session creation boundary. Map configuration errors to ACP structured error data without exposing secrets.

No `spawn_blocking` and no local runtime.

## R3-ASYNC-03 — Migrate model switching

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/handlers/model_switch.rs`
- Modify call sites in `agent_ops.rs`

Model switching must be transactional:

1. resolve model;
2. prepare request configuration;
3. only on success replace active model and sampler;
4. on failure leave previous model and sampler unchanged.

## R3-ASYNC-04 — Migrate agent profile model overrides

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`

Convert `apply_agent_model_override` and callers to async. A missing or invalid pinned model must follow the explicit profile policy, not silently use an unrelated provider.

## R3-ASYNC-05 — Migrate auxiliary model resolution

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/config.rs`
- Modify: `crates/codegen/xai-grok-shell/src/session/acp_session_impl/sampler_turn.rs`
- Modify all auxiliary-model callers found by `rg "resolve_aux_model_sampling_config"`

Return `Result<Option<SamplingConfig>, AgentConfigError>` or the prepared equivalent. `None` means no auxiliary override configured; errors must not become `None`.

## R3-ASYNC-06 — Migrate Web Search request preparation

**Files:**

- Modify the Web Search resolver around `agent/config.rs:4968`
- Modify all callers

Delete the synchronous wrapper. Web Search must use the same Provider Registry and credential context as normal requests.

## R3-ASYNC-07 — Remove production blocking bridges

**Files:**

- Modify any remaining production paths identified by the invariant scanner

The scanner must report zero Provider request-path blocking calls.

Allowed remaining `block_on` uses must be documented as:

- test harness runtime;
- top-level process runtime boot;
- unrelated synchronous extension boundary with no active runtime.

Each allowed occurrence must have a scanner suppression comment with a stable reason code; blanket path exclusions are forbidden.

**Phase 2 gate:** R3-RED-01 through R3-RED-03 pass; invariant scanner reports no request-path blocking calls; Shell targeted check, Clippy, and tests pass.

---

# Phase 3 — Remove legacy fallback and make errors authoritative

## R3-ERR-01 — Delete route-error fallback in `agent_ops`

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`

Replace `unwrap_or_else` with `?` and typed mapping. A provider-bound model with a live registry may never call legacy `sampling_config_for_model` after route compilation fails.

## R3-ERR-02 — Remove model-ID fallback to global sampler

`resolve_sampling_config_for_model` currently falls back to the global sampling configuration when model resolution fails.

Change semantics:

- exact canonical model found -> prepare it;
- legacy model reference that deterministically migrates -> migrate and prepare;
- unknown or ambiguous model -> structured error;
- no fallback to active/global model.

## R3-ERR-03 — Remove auxiliary xAI reconstruction

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/config.rs`
- Modify auxiliary callers

Delete any code that manually creates an xAI Responses model after Provider Runtime resolution fails. Compatibility must be expressed as a migrated provider/model binding before route compilation.

## R3-ERR-04 — Remove post-preparation mutation

Search:

```bash
rg -n "\.api_key\s*=|\.base_url\s*=|\.auth_scheme\s*=|\.api_backend\s*=" crates/codegen/xai-grok-shell/src crates/codegen/xai-grok-sampler/src
```

After `PreparedSamplerConfig` conversion, production callers may set only non-routing metadata explicitly permitted by the final API, such as origin-client attribution. Credentials, URL, protocol, and headers are immutable.

## R3-ERR-05 — Tighten Sampler construction API

**Files:**

- Modify: `crates/codegen/xai-grok-sampler/src/config.rs`
- Modify: `crates/codegen/xai-grok-sampler/src/client.rs`
- Modify callers

Prefer a constructor that accepts prepared data and make unprepared `SamplerConfig` construction internal or explicitly legacy-only.

Unknown `ProtocolId` in `From<PreparedSamplerConfig>` must return an error; it must not default to Chat Completions.

Replace infallible `From` with `TryFrom` if needed:

```rust
impl TryFrom<PreparedSamplerConfig> for SamplerConfig {
    type Error = UnsupportedProtocolError;
}
```

## R3-ERR-06 — Add static fallback prohibition

Extend `assert-provider-v1-invariants.py` to fail on:

```text
provider-bound route failure followed by sampling_config_for_model
unknown protocol mapped to ChatCompletions
manual auxiliary xAI ModelEntry construction
```

**Phase 3 gate:** R3-RED-04 passes; no production route error can produce a Sampler; all unsupported protocols fail before HTTP construction.

---

# Phase 4 — Unify configuration consumption across all Providers

## R3-CFG-01 — Freeze one resolved Provider config type

**Files:**

- Modify: `crates/codegen/xai-grok-provider/src/config.rs`
- Modify: `crates/codegen/xai-grok-provider/src/resolution.rs`

Ensure exactly one conversion from parsed config to `ProviderRuntimeConfig` exists and is used in production. Delete or deprecate parallel field-by-field conversions after consumers migrate.

The conversion must preserve all fields without lossy defaults.

## R3-CFG-02 — Add shared Provider configuration helpers

**Files:**

- Create: `crates/codegen/xai-grok-provider/src/providers/configure.rs`
- Modify: `providers/mod.rs`

Provide tested helpers for:

```rust
resolve_base_url
resolve_protocol
build_credential_candidates
merge_provider_headers
resolve_model_source
validate_insecure_http_policy
```

Built-in Providers and compatible factory must use these helpers rather than duplicating precedence logic.

## R3-CFG-03 — Fix xAI Provider configuration

**Files:**

- Modify: `providers/xai.rs`

Requirements:

- preserve existing xAI OAuth/session candidates;
- add provider inline and configured env candidates at frozen priority;
- apply configured base URL only through endpoint construction;
- merge extra headers;
- explicitly validate supported protocols;
- preserve legacy migration only when present in resolved config.

No xAI-specific bypass of `prepare_sampler_config` is allowed.

## R3-CFG-04 — Fix OpenAI Provider configuration

**Files:**

- Modify: `providers/openai.rs`

Consume provider inline key, configured env keys, base URL, protocol, headers, and model-list configuration. Keep `OPENAI_API_KEY` only as a profile/default candidate, not a hard override.

## R3-CFG-05 — Fix Anthropic Provider configuration

**Files:**

- Modify: `providers/anthropic.rs`

Consume configured credentials and headers while preserving mandatory `anthropic-version`. User headers cannot remove or conflict with mandatory protocol headers.

## R3-CFG-06 — Fix OpenCode Provider configuration

**Files:**

- Modify: `providers/opencode.rs`

Preserve public/no-auth model behavior where defined, while allowing explicit configured auth. Dynamic model discovery must work without requiring a key when the upstream endpoint is public.

## R3-CFG-07 — Fix Ollama Provider configuration

**Files:**

- Modify: `providers/ollama.rs`

Consume base URL, model-list path/format, headers, and explicit protocol rules. Local HTTP must use the endpoint-security policy rather than a provider-name exception.

## R3-CFG-08 — Bring compatible factory to the shared contract

**Files:**

- Modify: `providers/openai_compatible_factory.rs`

Remove duplicated precedence logic where the shared helper exists. Implement a valid `defaults()` method or redesign construction so the trait cannot call an unavailable value. `unimplemented!()` must be removed.

## R3-CFG-09 — Complete table-driven field fidelity tests

Extend R3-RED-05 so each field is either:

- reflected in routes/auth/model source;
- intentionally unsupported with a typed configuration error.

A warning is not sufficient for an unsupported security- or routing-relevant field.

**Phase 4 gate:** all built-in and custom Provider config fidelity tests pass; no Provider silently ignores an accepted field.

---

# Phase 5 — Implement deterministic route selection

## R3-ROUTE-01 — Add canonical route preference metadata

**Files:**

- Modify: `crates/codegen/xai-grok-provider/src/model.rs`
- Modify: `crates/codegen/xai-grok-provider/src/route.rs`

Add the minimum metadata required to express protocol or route requirement without parsing model names in Sampler.

Document serialization and defaults.

## R3-ROUTE-02 — Implement OpenAI selector

**Files:**

- Modify: `providers/openai.rs`
- Create tests: `tests/openai_route_selection.rs`

Implement the frozen selection order. The selector must check that the selected route exists and return a typed incompatibility error.

## R3-ROUTE-03 — Validate explicit route/protocol consistency

At Registry prepare time, reject:

- route ID not owned by provider;
- provider protocol with no matching route;
- model route requirement incompatible with provider protocol;
- default selector route absent from route set.

## R3-ROUTE-04 — Remove protocol inference outside selectors

Search all Shell and Sampler production code for model-name and URL-based protocol selection. Move valid decisions to Provider model metadata or route selectors; delete duplicate inference.

**Phase 5 gate:** all OpenAI route tests pass; Responses-required models cannot reach Chat Completions; generic models retain deterministic Chat behavior.

---

# Phase 6 — Make Provider configuration strict and transactional

## R3-PARSE-01 — Reject unknown TOML fields

**Files:**

- Modify: `crates/codegen/xai-grok-provider/src/config.rs`

Apply `#[serde(deny_unknown_fields)]` to the external provider entry type or use an equivalent strict deserializer that still supports intentional extension points.

The typo `implementation` must fail with a diagnostic naming the unknown field and provider section.

## R3-PARSE-02 — Enforce semantic validation in production parsing

`ProviderConfigInput::validate()` must be called from the real parse/resolution path. Convert diagnostics that affect correctness into errors:

- unknown protocol;
- unknown model-list format;
- invalid header name/value;
- unsupported Provider kind;
- missing custom Provider kind/profile;
- malformed base URL;
- insecure endpoint violating policy.

Warnings remain allowed only for non-functional deprecations.

## R3-PARSE-03 — Reject duplicate Provider identities after merge

Configuration precedence may replace one layer with a higher-priority definition, but two definitions in the same effective layer or an ambiguous alias must fail.

Add tests covering TOML merge, CLI override, and legacy migration.

## R3-PARSE-04 — Validate complete executable routes before commit

`ProviderRegistry::prepare()` must prove:

- all endpoints render;
- URL security policy passes;
- protocols are registered;
- route selectors refer to existing routes;
- required auth policies have at least one syntactically valid candidate source;
- all headers parse;
- model source paths/formats are valid.

Commit must remain atomic: any error leaves the previous snapshot and revision unchanged.

## R3-PARSE-05 — Add stable diagnostics

Errors must include:

```text
provider ID
field or route ID
error category
safe value summary
```

Never include API keys or Authorization header values.

**Phase 6 gate:** R3-RED-07 passes; invalid configuration cannot be saved or committed; old snapshot remains active after a failed hot reload.

---

# Phase 7 — Close the Catalog production lifecycle

## R3-CAT-01 — Make stale persisted models visible

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/provider_catalog.rs`
- Modify: `crates/codegen/xai-grok-shell/src/agent/models.rs`
- Modify: `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs`

Model views must include stale cached models with explicit stale status. Only entries with no model data should be invisible.

## R3-CAT-02 — Preserve prior models during refresh

Do not replace an existing catalog entry with `models: vec![]` when entering Loading. Keep prior models and transition status separately.

On failure:

```text
previous models exist -> Failed/Stale + previous models
no previous models -> Failed + empty models
```

## R3-CAT-03 — Introduce per-provider refresh task ownership

Replace a single overwritable `active_refresh` handle with provider/revision-scoped task ownership.

Recommended structure:

```rust
HashMap<ProviderId, RefreshTask {
    provider_revision: u64,
    cancel: CancellationToken,
    join: JoinHandle<()>,
}>
```

A result may publish only if its provider revision still matches the Registry snapshot that launched it.

## R3-CAT-04 — Start refresh on bootstrap

`bootstrap_catalog()` must:

1. load snapshot;
2. publish cached state;
3. start refresh for enabled dynamic providers;
4. return without blocking full startup beyond bounded snapshot I/O.

## R3-CAT-05 — Refresh on transactional config commit

Connect `ProviderConfigCoordinator`/ConfigReloader to Catalog lifecycle. After Registry commit, calculate the provider diff and call `refresh_changed()`.

Do not log "rebuild pending" without scheduling work.

## R3-CAT-06 — Emit revisions for removal and all visible transitions

Every visible catalog state change, including provider deletion, must send through `revision_tx`. Revisions must be monotonic.

## R3-CAT-07 — Persist successful visible snapshots atomically

Use `xai_grok_paths::atomic_write::atomic_replace`. Persist no plaintext credentials. A persistence failure must be surfaced in diagnostics but must not discard the valid in-memory snapshot.

## R3-CAT-08 — Implement bounded shutdown

Shutdown must cancel and await all refresh tasks. Define and test a maximum two-second graceful cancellation window, then abort and await remaining tasks.

## R3-CAT-09 — Add refresh network policy

Freeze:

```text
connect timeout: 5 seconds
total request timeout: 30 seconds
redirects: maximum 3
cross-origin redirect with auth: forbidden
same-origin redirect: allowed within limit
concurrency: bounded per runtime
```

Test with local mock servers.

**Phase 7 gate:** all R3-RED-08 scenarios pass; production bootstrap and hot reload generate catalog refreshes and visible revisions.

---

# Phase 8 — Correct credential errors and endpoint security

## R3-AUTH-01 — Make candidate resolution fallible

**Files:**

- Modify: `crates/codegen/xai-grok-provider/src/auth.rs`
- Modify: `crates/codegen/xai-grok-provider/src/prepared.rs`

Change candidate resolution to:

```rust
pub async fn resolve_candidates(
    &self,
    candidates: &[CredentialCandidate],
) -> Result<Option<SecretValue>, CredentialError>;
```

Update `prepare_sampler_config` to preserve the error category.

## R3-AUTH-02 — Define credential error taxonomy

Required variants:

```text
EnvironmentNotUnicode
SessionExpired
SessionBackend
MalformedCredential
ResolverUnavailable
```

Credential absence is `Ok(None)`, not an error.

Implement redacted `Display` and `Debug`.

## R3-AUTH-03 — Prove priority and failure behavior

Table-driven tests must show:

- a higher-priority valid credential wins;
- an absent candidate continues;
- a backend failure stops resolution and is not skipped for a lower-priority credential unless an explicit policy says otherwise;
- no secret appears in logs or errors.

## R3-SEC-01 — Decide and implement insecure HTTP policy

Preferred V1 policy:

```text
localhost/loopback HTTP: allowed
remote HTTP: rejected even if allow_insecure_http=true in production builds
```

If the product requires remote HTTP, implement it only with explicit config, prominent diagnostics, no credential forwarding across redirects, and tests. Do not retain an ineffective field.

If remote HTTP remains forbidden, remove `allow_insecure_http` from public configuration and migration docs in the same task.

## R3-SEC-02 — Validate redirect credential boundaries

Ensure Reqwest policy does not forward Authorization or API-key headers to a different origin. Add mock-server tests.

**Phase 8 gate:** credential error tests and endpoint-security tests pass; every accepted security field has real behavior.

---

# Phase 9 — Enforce one Runtime and one persistence primitive

## R3-RUNTIME-01 — Remove Pager empty Runtime fallback

**Files:**

- Modify: `crates/codegen/xai-grok-pager/src/app/dispatch/router.rs`
- Modify: `crates/codegen/xai-grok-pager/src/views/providers_modal.rs`
- Modify: `crates/codegen/xai-grok-pager/src/provider_state.rs`

Production UI paths must receive `Arc<ProviderRuntime>`. Missing runtime produces a user-visible unavailable/error state and telemetry event without panic.

Test-only local runtimes remain allowed in `#[cfg(test)]` helpers.

## R3-RUNTIME-02 — Audit all `ProviderRuntime::new()` calls

Classify every occurrence as:

```text
bootstrap owner
test fixture
invalid production duplicate
```

Only `provider_bootstrap.rs` may own the production construction. Remove all other production constructions.

Add scanner enforcement.

## R3-PERSIST-01 — Migrate model cache to shared atomic replace

**Files:**

- Modify: `crates/codegen/xai-grok-shell/src/agent/models.rs`

Replace fixed `.tmp` and raw rename with `xai_grok_paths::atomic_write::atomic_replace`.

Return or record write errors; never `let _ =` them.

## R3-PERSIST-02 — Add concurrent and Windows replacement tests

Use unique temporary directories and a fault-injection seam in the atomic primitive. Verify destination remains either the complete old file or complete new file.

## R3-PERSIST-03 — Audit all Provider-related persistence

Search for:

```bash
rg -n "std::fs::write|tokio::fs::write|std::fs::rename|tokio::fs::rename" crates/codegen/xai-grok-shell/src crates/codegen/xai-grok-pager/src
```

Classify Provider config, catalog, and model cache writes. Migrate only relevant unsafe writes; document unrelated occurrences.

**Phase 9 gate:** runtime injection test and atomicity tests pass on Linux and Windows; scanner finds one production Runtime constructor.

---

# Phase 10 — Rebuild the cross-platform exclusion ledger

## R3-XPLAT-01 — Freeze stable scanner ID algorithm

IDs must derive from:

```text
repository-relative path
canonical syntax category
nearest stable item/function/module name
normalized condition
```

They must not derive from line number alone.

Version the algorithm in scanner output:

```text
schema_version: 2
```

## R3-XPLAT-02 — Create migration from legacy IDs

**Files:**

- Create: `scripts/provider-v1/migrate-exclusion-ledger.py`
- Create: `docs/provider-adapter-v1/execution-v3/gates/exclusion-id-migration.md`

Map old IDs when possible; mark unmatched old entries as retired with evidence. Do not silently discard 163 historical entries.

## R3-XPLAT-03 — Regenerate the complete ledger

The scanner must cover:

```text
cfg(not(windows))
cfg(target_os = ...)
cfg(unix)
cfg(all(test, unix))
cfg_attr(..., ignore)
platform-specific module selection
workflow-level skipped tests
```

Each entry must contain:

```text
stable ID
path/item
condition
behavior protected
classification
Windows equivalent test or reason unavailable
owner task
status
```

## R3-XPLAT-04 — Classify, do not bulk-close

Allowed classifications:

```text
portable behavior missing
platform implementation split
feature genuinely unavailable
invalid exclusion
workflow-only skip
```

Every `portable behavior missing` or `invalid exclusion` entry becomes an individual execution child task. The agent may not close by directory or pattern.

## R3-XPLAT-05 — Repair Windows behavior one entry at a time

For each relevant entry:

1. write a Windows failing behavior/capability test;
2. implement Windows behavior or explicit unsupported capability;
3. run on `windows-latest`;
4. remove or narrow the exclusion;
5. update ledger evidence.

Do not use Linux-only tests as proof.

## R3-XPLAT-06 — Add ledger consistency gate

CI must fail when:

- scanner output contains an untracked ID;
- ledger contains an active missing ID without migration status;
- a closed entry still appears without valid platform-specific classification;
- scanner schema changes without migration tooling.

**Phase 10 gate:** current scan and ledger reconcile exactly; no unclassified entry remains; all V1-relevant Windows behavior has real tests.

---

# Phase 11 — Build production-faithful E2E coverage

## R3-E2E-01 — Replace helper-level tests with true entry tests

Extend `provider_real_entry_e2e.rs` to use the production App/Agent bootstrap. Manual route/compiler tests may remain as unit tests but cannot satisfy this gate.

## R3-E2E-02 — Built-in Provider matrix

Mocked wire-level tests for:

```text
xAI session or test resolver path
OpenAI Chat Completions
OpenAI Responses
Anthropic Messages
OpenCode public/no-auth and optional auth
Ollama local endpoint
```

Assert URL path, method, model, mandatory headers, auth header form, protocol framing, and decoded events.

## R3-E2E-03 — Custom Provider matrix

Test two simultaneously configured custom compatible Providers with distinct:

```text
identity
base URL
env key
inline key
headers
model catalog
```

Prove no state or credential leaks between them.

## R3-E2E-04 — Failure matrix

Required hard failures:

```text
missing credential
invalid endpoint
unknown protocol
ambiguous model reference
unknown route
incompatible OpenAI route requirement
credential backend failure
hot-reload validation failure
```

Assert no request reaches the mock server.

## R3-E2E-05 — Hot reload matrix

While a session exists:

1. change Provider endpoint and credential;
2. commit valid config;
3. verify new sessions use new revision;
4. verify in-flight request uses the snapshot with which it started;
5. submit invalid config and prove old snapshot remains active;
6. remove Provider and verify model catalog revision.

## R3-E2E-06 — Catalog restart matrix

1. successful discovery persists models;
2. restart with network unavailable;
3. stale models remain visible;
4. refresh failure marks stale/failed without removing models;
5. later refresh succeeds and emits a new revision.

## R3-E2E-07 — Cross-platform execution

Run the production-chain suite on Linux, Windows, and macOS. Tests may use platform-specific harness setup, but must exercise the same semantic assertions.

**Phase 11 gate:** the true production entry suite passes on all release platforms with no ignored tests.

---

# Phase 12 — Convert CI from evidence collection to release gates

## R3-CI-01 — Trigger on main and feature branches

Update `provider-adapter.yml` to run on:

```yaml
push:
  branches: [main, feat/provider-adapter]
pull_request:
  branches: [main, feat/provider-adapter]
```

## R3-CI-02 — Add full Windows gate

Windows must run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked --no-run
cargo test -p xai-grok-provider --all-targets --locked
cargo test -p xai-grok-sampler --all-targets --locked
cargo test -p xai-grok-shell --test provider_real_entry_e2e --locked
cargo test -p xai-grok-pager --lib --locked
```

Add targeted Windows behavior suites identified by the ledger.

Resource-heavy targets may be sharded, but not skipped. Record disk before and after each shard.

## R3-CI-03 — Add full macOS gate

macOS must run workspace check/Clippy/test-build and the production Provider E2E. Platform-specific terminal/path tests must execute.

## R3-CI-04 — Add Linux full test gate

The workspace job must include:

```bash
cargo test --workspace --all-targets --locked
```

If runtime is excessive, shard by crate while preserving full target coverage. A no-run build is not a substitute.

## R3-CI-05 — Make PTY Provider E2E blocking

Remove `continue-on-error: true`. Replace blanket `--ignored` execution with explicitly named stable tests or a dedicated feature/harness. Timeout remains allowed, but timeout is failure.

## R3-CI-06 — Make documentation gate honest

Remove broad `RUSTDOCFLAGS --allow` suppressions as warnings are repaired. The final command must be:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Output filename collisions that cannot be fixed in V1 must have a narrowly justified separate job, not global suppression.

## R3-CI-07 — Keep baseline workflow non-authoritative

`provider-v1-baseline.yml` may retain `continue-on-error` only if:

- it is clearly named evidence-only;
- branch protection does not count it as a release gate;
- release documentation never cites it as passing evidence.

## R3-CI-08 — Add invariant and ledger gates

Run:

```bash
python scripts/provider-v1/assert-provider-v1-invariants.py
python scripts/provider-v1/scan-platform-exclusions.py --check-ledger
```

on all relevant pull requests.

## R3-CI-09 — Add package startup smoke tests

For each OS:

1. build the actual user-facing binary/package;
2. run `--version`;
3. run a no-network startup/config validation command;
4. verify config directory creation and path semantics;
5. terminate cleanly.

**Phase 12 gate:** all release jobs are blocking; no `continue-on-error` exists in release jobs; main branch is covered.

---

# Phase 13 — Repository hygiene and documentation truth

## R3-HYGIENE-01 — Repair whitespace and line-ending debt

Do this only after behavioral phases are complete to avoid mixing mechanical churn with logic commits.

Create `.gitattributes` rules appropriate to the repository, then normalize only paths identified by `git diff --check`.

Commands:

```bash
git diff --check origin/main...HEAD
cargo fmt --all -- --check
```

Do not mass-rewrite vendored/generated files without confirming ownership.

## R3-HYGIENE-02 — Remove obsolete comments and phase markers

Delete or update comments claiming closure that contradict current implementation, including stale `P7-003` or `P8-011` statements. Comments must describe current invariant, not historical task intent.

## R3-DOC-01 — Update public Provider contracts

Update:

```text
docs/provider-adapter-v1/config-reference.md
docs/provider-adapter-v1/public-contracts.md
docs/provider-adapter-v1/provider-matrix.md
docs/provider-adapter-v1/migration.md
docs/provider-adapter-v1/troubleshooting.md
docs/provider-adapter-v1/rollback.md
```

Document exact behavior for:

- strict config fields;
- built-in and custom Provider precedence;
- credential priority;
- route selection;
- stale Catalog visibility;
- hot reload snapshot behavior;
- insecure HTTP policy;
- supported platforms;
- hard errors and troubleshooting.

Do not claim a feature unsupported by blocking CI evidence.

## R3-DOC-02 — Update project status

Update `PROGRESS.md` and other non-ISSUE project status documents only to remove factual contradictions. Do not create, replace, append to, or reclassify any ISSUE document during this remediation cycle.

Status language must distinguish:

```text
implemented
verified on Linux
verified on Windows
verified on macOS
release-gated
```

## R3-HYGIENE-03 — Remove dead compatibility APIs

After all consumers migrate, remove deprecated Provider/Sampler functions that are no longer used. Prove with:

```bash
rg -n "sampling_config_for_model\(|configure_providers\(|register_route\(|fetch_provider_models_blocking" crates
cargo check --workspace --all-targets --locked
```

No `#[allow(dead_code)]` may be added to retain obsolete production code.

**Phase 13 gate:** `git diff --check origin/main...HEAD` passes; docs build with warnings denied; no documentation overclaims support.

---

# Phase 14 — Clean-checkout production acceptance

## R3-REL-01 — Create a fresh verification clone

From outside the development tree:

```bash
git clone <repository-url> openbuild-provider-v1-verify
cd openbuild-provider-v1-verify
git checkout <candidate-commit>
git status --porcelain
```

The status must be empty. Do not copy build artifacts or local config into the clone.

## R3-REL-02 — Run full Linux acceptance

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
python scripts/provider-v1/assert-provider-v1-invariants.py
python scripts/provider-v1/scan-platform-exclusions.py --check-ledger
git diff --check origin/main...HEAD
```

## R3-REL-03 — Run full Windows acceptance

Use a clean `windows-latest` runner or clean local VM. Run the Windows G3 matrix, production E2E, atomic replacement tests, path/process/PTY capability tests, and package smoke test.

Store exact workflow run URL and commit SHA in:

```text
docs/provider-adapter-v1/execution-v3/release/windows.md
```

## R3-REL-04 — Run full macOS acceptance

Run the macOS G3 matrix and store evidence in `release/macos.md`.

## R3-REL-05 — Execute fault-injection acceptance

Required fault cases:

```text
invalid hot-reload config
credential backend failure
catalog timeout
catalog malformed response
catalog provider removal during refresh
atomic replace failure
mock server cross-origin redirect
model switch preparation failure
```

Verify old valid runtime state remains usable where transactional behavior requires it.

## R3-REL-06 — Verify secret hygiene

Scan logs, snapshots, fixtures, artifacts, and Git diff for test secrets and credential header values. Use known canary values and prove they do not appear outside expected in-memory mock-server assertions.

## R3-REL-07 — Review release diff

Review:

```bash
git diff --stat <baseline>...<candidate>
git diff --name-status <baseline>...<candidate>
git log --oneline <baseline>..<candidate>
```

Every commit must map to one task ID. No unexplained files may remain.

## R3-REL-08 — Create a new RC only after all evidence is complete

Do not reuse or move an old tag. Create a new immutable tag only after Linux, Windows, macOS, E2E, packaging, hygiene, documentation, and secret gates pass on the exact candidate commit.

Suggested tag:

```text
provider-adapter-v1-rc.2
```

The final tag name may advance if that tag already exists, but must never overwrite an existing tag.

---

## 4. Final acceptance checklist

All boxes must be checked by evidence, not assertion.

### Single execution authority

- [ ] No Provider request-path `block_on` remains.
- [ ] No provider-bound route failure falls back to legacy sampling.
- [ ] Auxiliary models use the same Registry and request-preparation chain.
- [ ] Unknown protocols cannot default to Chat Completions.
- [ ] Sampler receives one prepared configuration path.

### Configuration correctness

- [ ] All accepted Provider fields have defined behavior.
- [ ] Built-in and compatible Providers use the same precedence contract.
- [ ] Unknown fields and invalid semantic values fail strictly.
- [ ] Registry commit validates complete executable routes.
- [ ] Failed hot reload preserves the previous snapshot.

### Authentication and security

- [ ] Provider inline and configured env credentials work for built-ins.
- [ ] Credential absence and backend failure are distinguishable.
- [ ] Secret values are redacted from all diagnostics.
- [ ] Header merge order is tested.
- [ ] Redirects cannot leak credentials.
- [ ] `allow_insecure_http` is either implemented or removed.

### Routing

- [ ] OpenAI Chat and Responses selection is deterministic.
- [ ] Explicit route/protocol incompatibility is a hard error.
- [ ] No URL-domain or model-string routing exists in Sampler.

### Catalog

- [ ] Persisted stale models are visible at startup.
- [ ] Refresh begins automatically at startup and config commit.
- [ ] Failed refresh retains prior models.
- [ ] Provider removal emits revision.
- [ ] Superseded tasks cannot publish stale results.
- [ ] Shutdown cancels and awaits all refresh tasks.
- [ ] Snapshot persistence is atomic.

### Runtime and persistence

- [ ] Exactly one production `ProviderRuntime` is constructed.
- [ ] Pager does not create a fallback Runtime.
- [ ] Model cache uses shared atomic replacement.
- [ ] Concurrent/failed writes preserve a complete file.

### Cross-platform

- [ ] Exclusion scanner IDs are stable.
- [ ] Legacy ledger entries are migrated or retired with evidence.
- [ ] No unclassified platform exclusion remains.
- [ ] Windows behavior is tested rather than skipped.
- [ ] Linux, Windows, and macOS release matrices pass.

### Testing and release

- [ ] Production entry E2E covers built-in and custom Providers.
- [ ] Failure requests do not reach mock servers.
- [ ] Hot reload and Catalog restart are E2E tested.
- [ ] PTY Provider tests are blocking.
- [ ] Full workspace tests run in CI.
- [ ] Documentation warnings are denied.
- [ ] `git diff --check origin/main...HEAD` passes.
- [ ] Clean-checkout acceptance passes on the candidate commit.
- [ ] A new immutable RC tag points to the verified commit.

---

## 5. Required agent completion report

The implementing agent must finish with exactly this structure:

```markdown
# Provider Adapter V1 Remediation Completion Report

## Candidate
- Baseline commit:
- Candidate commit:
- RC tag:

## Phase results
| Phase | Tasks complete | Gate | Evidence path |
|---|---:|---|---|

## Platform evidence
| Platform | Check | Clippy | Tests | E2E | Package smoke | Workflow/run |
|---|---|---|---|---|---|---|

## Invariant scans
- Request-path blocking calls:
- Legacy fallback matches:
- Production Runtime constructors:
- Untracked platform exclusions:
- `git diff --check`:

## Known limitations
- None

## Deviations
- None
```

The report may not say "None" when an unresolved failure, skipped test, non-blocking gate, missing platform run, or unverified assumption remains. In that case, the release remains NO-GO and no RC tag may be created.
