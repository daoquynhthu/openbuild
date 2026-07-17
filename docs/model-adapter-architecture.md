# Model Adapter Layer — Architecture Specification

## 1. Overview

Grok Build currently supports three API backends (`ChatCompletions`, `Responses`, `Messages`)
but is tightly coupled to xAI's proprietary authentication, headers, and endpoint defaults.
This document defines the target architecture for a **provider-agnostic model adapter layer**
that treats xAI as one provider among many while preserving full backward compatibility.

### Design goals

- **Provider as a first-class concept**: each provider (xAI, OpenAI, Anthropic, OpenCode Zen,
  Ollama, etc.) is a self-contained unit carrying its own protocol, auth, defaults, and
  known model list.
- **Layered isolation**: protocol logic (wire format), transport (HTTP/SSE), authentication,
  and provider metadata are each owned by separate layers with单向 dependencies.
- **Immutable composition**: providers, routes, and protocols are assembled via composition
  (not inheritance) and combined into immutable configurations.
- **Backward compatible**: existing xAI OAuth flows, config.toml sections, env vars, and
  CLI flags continue to work unchanged.
- **Zero-config discovery**: when the user provides only `--api-key` and `--base-url`, the
  system auto-detects the provider from the URL and loads appropriate defaults.

---

## 2. Layer Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│  Provider Facades                                                   │
│  xai.rs | openai.rs | anthropic.rs | opencode.rs | ollama.rs | ...  │
│                                                                      │
│  Each facade is a thin function that calls                           │
│  `route.with({ endpoint, auth, defaults })` and returns              │
│  `{ id, model: (modelID) => Model, configure }`.                    │
│  Bundles one or more Protocol implementations via Route.            │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Route — 4-axis immutable composition                               │
│                                                                      │
│  Route =  Protocol  +  Endpoint  +  Auth  +  Framing                │
│           ↑              ↑            ↑          ↑                   │
│       wire format    base URL +    credential   byte→frame          │
│                      path + query  resolution   decoding            │
│                                                                      │
│  route.with(patch) → new Route  (immutable patching)                │
│  route.model(input) → Model    (bind model ID to route)             │
│                                                                      │
│  RouteRegistry — map<protocol_id, Route>                             │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Protocol Layer (xai-grok-sampler)                                   │
│                                                                      │
│  Protocol<Body, Frame, Event, State> {                               │
│    id: ProtocolID                                                     │
│    body:  { schema: Codec<Body>, from(LLMRequest) → Body }           │
│    stream: { event: Codec<Event>,                                     │
│              initial(request) → State,                               │
│              step(state, event) → [State, LLMEvent[]],               │
│              terminal?(event) → bool,                                │
│              onHalt?(state) → LLMEvent[] }                           │
│  }                                                                   │
│                                                                      │
│  Pure wire format — no URL, no auth, no headers.                     │
│  Reusable across providers: OpenAIChat.protocol shared by            │
│  20+ OpenAI-compatible providers.                                    │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Auth Layer — functional composition                                 │
│                                                                      │
│  Auth is a function: apply(AuthInput) → Effect<Headers>              │
│                                                                      │
│  Composition:                                                        │
│    Auth.optional(apiKey)              // try inline key               │
│      .orElse(Auth.config("ENV_VAR"))  // fallback to env             │
│      .bearer()                        // render as Bearer header     │
│    // OR:                                                             │
│    Auth.optional(apiKey).orElse(...).header("x-api-key")             │
│                                                                      │
│  Auth chain is: credential resolution → header rendering             │
│  Rendered per-request via `auth.apply(input) → Headers`             │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Transport + Framing Layer                                           │
│                                                                      │
│  Transport: prepare(body) → Prepared, frames(Prepared) → Stream<Frame>│
│  Framing:   Stream<Uint8Array> → Stream<Frame>                       │
│    • sse — Server-Sent Events (decode UTF-8, emit JSON data strings) │
│    • (future) aws-event-stream — binary frames with CRC              │
│                                                                      │
│  reqwest HTTP client (protocol-agnostic).                            │
│  Headers assembled from: Provider defaults > Route defaults > Auth   │
│  > HTTP options (request-level overlay with denylist).              │
└──────────────────────────────────────────────────────────────────────┘
```

### Layer ownership

| Layer | Crate | Responsibility |
|-------|-------|----------------|
| Provider Facades | `xai-grok-provider` | Pre-built provider definitions |
| Provider Trait | `xai-grok-provider` | Interface + registry |
| Protocol | `xai-grok-sampler` (refactored) | Wire format ↔ LLMEvent |
| Auth | `xai-grok-shell/src/auth/` (refactored) | Credential chain resolution |
| Transport | `xai-grok-sampler/src/client.rs` | HTTP + SSE (unchanged) |

---

## 3. Core Types

### 3.1 ProviderId

```rust
/// Type-safe provider identifier.
/// Newtype over String for future extensibility (e.g. custom providers).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub const XAI: &'static str = "xai";
    pub const OPENAI: &'static str = "openai";
    pub const ANTHROPIC: &'static str = "anthropic";
    pub const OPENCODE: &'static str = "opencode";
    pub const OLLAMA: &'static str = "ollama";
    pub const OPENAI_COMPATIBLE: &'static str = "openai-compatible";
}
```

### 3.2 ProviderDefaults

```rust
/// Format of the model list endpoint response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelListFormat {
    /// Standard OpenAI-compatible /v1/models: {"data": [{...}]}
    OpenAiCompatible,
    /// Ollama /api/tags format: {"models": [{"name": "...", ...}]}
    OllamaTags,
}

/// Immutable set of defaults baked into each provider definition.
/// Users override individual fields via [model.*] or [provider.*] config.
#[derive(Debug, Clone)]
pub struct ProviderDefaults {
    pub id: ProviderId,
    pub name: String,

    // Endpoint
    pub base_url: String,

    // Protocol
    pub api_backend: ApiBackend,  // ChatCompletions | Responses | Messages

    // Auth
    pub auth_scheme: AuthScheme,  // Bearer | XApiKey | None
    pub env_key: Vec<String>,     // env vars to try, in order

    // Model list endpoint
    pub model_list_endpoint: Option<String>,   // None → auto-derived from base_url
    pub model_list_format: ModelListFormat,    // response format

    // Generation defaults
    pub context_window: NonZeroU64,
    pub max_completion_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,

    // Capabilities
    pub supports_backend_search: bool,
    pub supports_reasoning_effort: bool,
    pub supports_streaming: bool,
    pub supports_tool_calling: bool,
    pub supports_structured_output: bool,

    // Default headers
    pub extra_headers: IndexMap<String, String>,
}
```

### 3.3 Provider Facade — Function, Not Trait

Unlike OpenCode (TypeScript), Rust's type system requires a trait for
dynamic dispatch. The `Provider` trait is `Arc`-clonable and stateless —
`configure()` returns a `ConfiguredProvider` that owns a **route set**.

**V1 architecture (AD-03):** A configured provider owns zero or more routes,
a default route ID, a route selector, and a model source spec. The sampler
never infers endpoint paths or provider headers from provider names or URL
patterns.

```rust
/// Stateless provider definition. Singleton registered in ProviderRegistry.
/// configure() merges user overrides into baked-in defaults → ConfiguredProvider.
pub trait Provider: Send + Sync + Debug {
    fn id(&self) -> &ProviderId;
    fn name(&self) -> &str;
    fn defaults(&self) -> &ProviderDefaults;

    /// Override defaults with user config → a configured provider
    /// that can create Models.
    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider;
}

/// User-supplied overrides for a provider.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}

/// Result of provider.configure(): holds the composed Route set and credential
/// chain, ready to create Model values.
///
/// V1: owns an IndexMap of routes, a default route ID, and a route selector.
pub struct ConfiguredProvider {
    pub id: ProviderId,
    pub display_name: String,
    pub config: ResolvedProviderConfig,
    pub routes: IndexMap<RouteId, Arc<Route>>,
    pub default_route_id: RouteId,
    pub route_selector: Arc<dyn RouteSelector>,
    pub model_source: ModelSourceSpec,
}

/// Determines which route a model ID resolves to.
/// OpenAI uses this seam to select Responses versus Chat Completions.
pub trait RouteSelector: Send + Sync {
    fn select(&self, model_id: &str) -> Result<RouteId, ProviderError>;
}
```

**Route selection policy:** `RouteSelector::select(model_id)` returns `Result<RouteId, ProviderError>`.
All providers must test their selector. The default route ID must exist in the
route map. The selector must never return a route outside the map.

### 3.4 Route — Declarative Route Shape (V1, AD-02)

**V1 architecture (AD-02):** `Route` is declarative and cloneable. Stream framing/decoding
is owned by the protocol implementation selected by `protocol_id`. The four-axis route-level
framing abstraction is removed — no production caller uses `Route.framing` to influence
wire-format decoding.

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

**Key design rules:**
- `Route` is cloneable — it may be stored in `Arc<Route>` for shared ownership.
- `protocol_id` selects the protocol implementation (Chat Completions, Responses, Messages).
- `Framing` is not a separate route authority; the protocol owns frame/stream decoding.
- Route validation confirms provider ID, route ID, protocol ID, endpoint, auth policy, and headers.
- `Route::model()` returns a model binding containing the explicit `route_id`.

### 3.5 Model — Executable Process-local Value

```rust
/// A model ready to run. Contains identity, capabilities, route reference,
/// and reusable request-behavior defaults.
pub struct Model {
    pub id: ModelId,
    pub provider: ProviderId,
    pub route: Arc<Route>,
    pub defaults: Option<ModelDefaults>,
}

pub struct ModelDefaults {
    pub limits: Option<ModelLimits>,       // context_window, max_output
    pub generation: Option<GenerationOptions>,  // temperature, top_p, etc.
    pub provider_options: Option<HashMap<String, Value>>,
    pub http: Option<HttpOptions>,          // headers, body, query overlays
}

pub struct HttpOptions {
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<Value>,        // JSON overlay (deny-listed for protocol fields)
    pub query: Option<HashMap<String, String>>,
}
```

### 3.4 ProviderConfig

```rust
/// User-supplied overrides for a provider, from config.toml [provider.*]
/// and/or CLI flags --provider/--api-key/--base-url.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    pub id: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub extra_headers: Option<IndexMap<String, String>>,
}
```

### 3.5 Protocol

```rust
/// Semantic API contract of one model server family.
/// Pure wire-format logic — no URL, no auth, no headers.
/// Reusable across providers: OpenAIChat.protocol shared by 20+ providers.
pub struct Protocol<Body, Frame, Event, State> {
    pub id: ProtocolId,
    pub body: ProtocolBody<Body>,
    pub stream: ProtocolStream<Frame, Event, State>,
}

pub struct ProtocolBody<Body> {
    /// Schema for the validated provider-native body sent as JSON.
    pub schema: Schema<Body>,
    /// Build provider-native body from the common request.
    pub from: fn(LLMRequest) -> Result<Body>,
}

pub struct ProtocolStream<Frame, Event, State> {
    /// Schema for one decoded streaming event (from a transport frame).
    pub event: Schema<Event>,
    /// Initial parser state. Called once per response with the resolved request.
    pub initial: fn(LLMRequest) -> State,
    /// Translate one event into emitted LLMEvents plus the next state.
    pub step: fn(&mut State, Event) -> Result<Vec<LLMEvent>>,
    /// Optional request-completion signal for transports that don't end naturally.
    pub terminal: Option<fn(&Event) -> bool>,
    /// Optional flush emitted when the framed stream ends.
    pub on_halt: Option<fn(&State) -> Vec<LLMEvent>>,
}
```

Factory helper equivalent to OpenCode's `Protocol.make({...})`:

```rust
impl<B, F, E, S> Protocol<B, F, E, S> {
    pub fn new(
        id: impl Into<ProtocolId>,
        body: ProtocolBody<B>,
        stream: ProtocolStream<F, E, S>,
    ) -> Self { ... }
}
```

### 3.6 LLMEvent — Normalized Provider Event Union

All provider-native events map to this union. Contains exactly one `Finish` per
response. Content-type events follow a `Start → Delta* → End` pattern.

```rust
#[derive(Debug, Clone)]
pub enum LLMEvent {
    StepStart { index: u32 },

    TextStart { id: String },
    TextDelta { id: String, text: String },
    TextEnd { id: String },

    ReasoningStart { id: String },
    ReasoningDelta { id: String, text: String },
    ReasoningEnd { id: String },

    ToolInputStart { id: String, name: String },
    ToolInputDelta { id: String, text: String },
    ToolInputEnd { id: String, name: String },

    /// A completed tool call (all deltas accumulated).
    ToolCall { id: String, name: String, input: Value },

    ToolResult { id: String, name: String, result: ToolResultValue },
    ToolError { id: String, name: String, message: String },

    StepFinish { index: u32, reason: FinishReason, usage: Option<Usage> },
    Finish { reason: FinishReason, usage: Option<Usage> },

    Error { message: String, kind: ErrorKind },
}

/// Inclusive totals with non-overlapping breakdown.
/// Every breakdown field is independently meaningful — consumers never subtract.
/// Invariant: nonCachedInputTokens + cacheReadInputTokens + cacheWriteInputTokens = inputTokens
///            reasoningTokens ≤ outputTokens
#[derive(Debug, Clone)]
pub struct Usage {
    // Inclusive totals (match OpenAI/Anthropic convention)
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,

    // Non-overlapping breakdown
    pub non_cached_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
    pub cache_write_input_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,

    /// Raw provider usage data for fields we don't normalize.
    pub provider_metadata: Option<HashMap<String, Value>>,
}
```

### 3.7 Endpoint

```rust
/// Declarative URL construction for one route.
/// baseURL + path (string or function) + query params.
pub struct Endpoint<Body> {
    pub base_url: Option<String>,
    pub path: EndpointPart<Body>,
    pub query: Option<HashMap<String, String>>,
}

/// Path can be a static string or a function of the request body
/// (for routes whose URL embeds model id, region, etc.).
pub enum EndpointPart<Body> {
    Static(String),
    Dynamic(fn(&EndpointInput<Body>) -> String),
}

pub struct EndpointInput<Body> {
    pub request: LLMRequest,
    pub body: Body,
}

impl<Body> Endpoint<Body> {
    pub fn render(&self, input: &EndpointInput<Body>) -> Url { ... }
}

/// Merge base endpoint with an override patch (used by route.with()).
pub fn merge_endpoints<Body>(
    base: &Endpoint<Body>,
    patch: &EndpointPatch<Body>,
) -> Endpoint<Body> { ... }
```

### 3.8 Framing

```rust
/// Byte-stream decoder: raw HTTP body → protocol frames.
/// The frame type is opaque; the Protocol's `event` schema decodes it.
pub trait Framing<Frame>: Send + Sync {
    fn id(&self) -> &str;
    fn frame(&self, bytes: ByteStream) -> Stream<Frame>;
}

/// Server-Sent Events framing. Used by every JSON-streaming HTTP provider.
/// UTF-8 decode → SSE channel decoder → filter empty/[DONE] → emit data strings.
pub struct SseFraming;
impl Framing<String> for SseFraming { ... }
```

### 3.9 Auth — Functional Composition

```rust
/// Auth is a function: apply(AuthInput) → Headers.
/// Supports chaining: .orElse() for fallback, .andThen() for layering.
pub trait AuthFn: Send + Sync {
    fn apply(&self, input: &AuthInput) -> Result<HeaderMap>;
    fn or_else(self, that: Box<dyn AuthFn>) -> Box<dyn AuthFn>;
    fn and_then(self, that: Box<dyn AuthFn>) -> Box<dyn AuthFn>;
}

/// Credential source: an Effect that resolves to a secret string.
pub enum Credential {
    Inline(Option<String>),
    Config(String),  // environment variable name
    Session,         // existing OAuth session
    None,
}

// Builder helpers:
impl Credential {
    pub fn optional(key: Option<String>, source: &str) -> Self { ... }
    pub fn config(name: &str) -> Self { ... }

    /// Render credential as Bearer token → Auth
    pub fn bearer(self) -> Box<dyn AuthFn> { ... }

    /// Render credential as arbitrary header → Auth
    pub fn header(self, name: &str) -> Box<dyn AuthFn> { ... }
}

/// Chain multiple auth sources. First success wins.
/// Usage: `Credential::optional(api_key).or_else(Credential::config("OPENAI_API_KEY")).bearer()`
///        `Credential::optional(api_key).or_else(Credential::config("ANTHROPIC_API_KEY")).header("x-api-key")`
```

---

## 4. ProviderRegistry and Immutable Snapshots (V1, AD-04)

**V1 architecture (AD-04):** Registry state is published as atomic `Arc<RegistrySnapshot>`.
Readers always see a complete, immutable, revisioned view. Writes validate fully before
replacing the snapshot. No independent mutable provider/route/config maps are exposed.

```rust
/// Immutable, atomic, revisioned view of all configured providers and routes.
pub struct RegistrySnapshot {
    pub revision: u64,
    pub providers: IndexMap<ProviderId, Arc<ConfiguredProvider>>,
    pub routes: IndexMap<RouteId, Arc<Route>>,
}

/// Process-wide registry. Holds provider definitions and the current
/// immutable snapshot. The launcher constructs one instance and injects
/// it into shell and pager state.
pub struct ProviderRegistry {
    definitions: IndexMap<ProviderId, SharedProvider>,
    snapshot: RwLock<Arc<RegistrySnapshot>>,
}

impl ProviderRegistry {
    pub fn new() -> Self;

    /// Register a built-in provider definition. Rejects duplicates.
    pub fn register_definition(&mut self, provider: SharedProvider) -> Result<(), ProviderError>;

    /// Transactional rebuild from resolved configs.
    /// 1. Resolve all provider configs.
    /// 2. Configure and validate every provider into a new local snapshot.
    /// 3. Reject duplicate IDs and invalid routes.
    /// 4. Replace current Arc<RegistrySnapshot> only if all validation succeeds.
    /// 5. Increment revision exactly once per successful replacement.
    /// Failed rebuild leaves old snapshot and revision unchanged.
    pub fn rebuild(&self, configs: &ResolvedProviderConfigs) -> Result<u64, ProviderError>;

    /// Returns the current immutable snapshot.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot>;

    /// Look up a configured provider by ID.
    pub fn configured(&self, id: &ProviderId) -> Option<Arc<ConfiguredProvider>>;

    /// Look up a route by ID.
    pub fn route(&self, id: &RouteId) -> Option<Arc<Route>>;

    /// All registered provider IDs (definitions).
    pub fn all_ids(&self) -> Vec<ProviderId>;

    /// Auto-detect provider from base URL patterns:
    ///   api.x.ai         → xai
    ///   api.openai.com   → openai
    ///   api.anthropic.com → anthropic
    ///   opencode.ai      → opencode
    ///   localhost        → ollama
    ///   *                → openai-compatible
    pub fn detect_from_url(&self, base_url: &str) -> ProviderId;
}
```

**Registry rebuild is transactional:**
1. Resolve all provider configs.
2. Configure and validate every provider into a new local snapshot.
3. Reject duplicate IDs and invalid routes.
4. Replace the current `Arc<RegistrySnapshot>` only if all validation succeeds.
5. Increment revision exactly once per successful replacement.

Never mutate routes/config maps independently. Lock poisoning is converted into
an internal provider error; the registry must not panic.

---

## 5. Pre-built Provider Definitions

### 5.1 xAI Provider

| Property | Value |
|----------|-------|
| ProviderId | `"xai"` |
| Base URL | `https://api.x.ai/v1` |
| API Backend | `Responses` |
| Auth Scheme | `Bearer` |
| Auth Chain | `InlineKey → EnvVar("XAI_API_KEY") → SessionToken(OAuth)` |
| Context Window | 500,000 |
| Extra Headers | `x-grok-*` headers via `XaiExtraHeaders` |
| Raw Tools | `x_search` |
| Doom Loop | Enabled |
| Known Models | Fetched dynamically from `https://api.x.ai/v1/models` |

The xAI provider is the **default** when no other provider is configured. It preserves the
existing OAuth device-code flow, `grok login`, session token refresh, and all `x-grok-*`
headers for backward compatibility.

### 5.2 OpenAI Provider

| Property | Value |
|----------|-------|
| ProviderId | `"openai"` |
| Base URL | `https://api.openai.com/v1` |
| API Backend | `ChatCompletions` (also supports `Responses`) |
| Auth Scheme | `Bearer` |
| Auth Chain | `InlineKey → EnvVar("OPENAI_API_KEY")` |
| Context Window | 128,000 |
| Extra Headers | None |
| Raw Tools | None |
| Known Models | Fetched dynamically from `https://api.openai.com/v1/models` |

### 5.3 Anthropic Provider

| Property | Value |
|----------|-------|
| ProviderId | `"anthropic"` |
| Base URL | `https://api.anthropic.com/v1` |
| API Backend | `Messages` |
| Auth Scheme | `XApiKey` (via `x-api-key` header) |
| Auth Chain | `InlineKey → EnvVar("ANTHROPIC_API_KEY")` |
| Extra Headers | `anthropic-version: 2023-06-01` |
| Known Models | Fetched dynamically from `https://api.anthropic.com/v1/models`<br>Auth: `x-api-key` header via `ANTHROPIC_API_KEY` |

### 5.4 OpenCode Zen Provider

| Property | Value |
|----------|-------|
| ProviderId | `"opencode"` |
| Base URL | `https://opencode.ai/zen/v1` |
| API Backend | `ChatCompletions` |
| Auth Scheme | `Bearer` |
| Auth Chain | `PublicKey("public") → EnvVar("OPENCODE_API_KEY")` |
| Extra Headers | None |
| Known Models | Dynamically fetched from `https://opencode.ai/zen/v1/models` |

**Free-model fallback**: when no API key is available, the chain resolves to `"public"`
and paid models (those with cost > 0) are filtered out of the model list automatically.
The model list fetch is also made without auth when no key is configured; the server
returns only free models for unauthenticated requests.

### 5.5 Ollama Provider

| Property | Value |
|----------|-------|
| ProviderId | `"ollama"` |
| Base URL | `http://localhost:11434/v1` |
| API Backend | `ChatCompletions` |
| Auth Scheme | `None` |
| Auth Chain | `None` |
| Extra Headers | None |
| Known Models | Fetched dynamically from `http://localhost:11434/api/tags`<br>Format: `ModelListFormat::OllamaTags` |

### 5.6 OpenAI-Compatible Provider

| Property | Value |
|----------|-------|
| ProviderId | `"openai-compatible"` |
| Base URL | User-specified |
| API Backend | `ChatCompletions` |
| Auth Scheme | `Bearer` |
| Auth Chain | `InlineKey → EnvVar("XAI_API_KEY")` (generic fallback) |
| Extra Headers | None |

| Known Models | Fetched dynamically from `{base_url}/models` (depends on user-specified or profile base_url) |

Catch-all provider for any OpenAI Chat Completions-compatible API
(Groq, DeepSeek, Together AI, Fireworks, etc.). Known profiles map
friendly names to base URLs:

| Profile | Base URL | Model List URL |
|---------|----------|----------------|
| `groq` | `https://api.groq.com/openai/v1` | `https://api.groq.com/openai/v1/models` |
| `deepseek` | `https://api.deepseek.com/v1` | `https://api.deepseek.com/v1/models` |
| `togetherai` | `https://api.together.xyz/v1` | `https://api.together.xyz/v1/models` |
| `fireworks` | `https://api.fireworks.ai/inference/v1` | `https://api.fireworks.ai/inference/v1/models` |
| `openrouter` | `https://openrouter.ai/api/v1` | `https://openrouter.ai/api/v1/models` |

---

## 6. Integration Points with Existing Code

### 6.1 Model Resolution Pipeline — V1 Architecture (AD-07, AD-09)

**V1 architecture (AD-07):** Catalog discovery runs asynchronously, concurrently with
a bounded concurrency limit, per-provider timeout, cancellation, TTL cache, stale
fallback, and explicit refresh. Startup does not synchronously fetch provider model lists.

**V1 architecture (AD-09):** Canonical reference syntax is `provider/model`. Bare model IDs
are accepted only when they resolve uniquely under deterministic precedence. Persisted
model selection must store the canonical provider-qualified reference for non-xAI providers.

The existing `resolve_model_list()` function in `xai-grok-shell/src/agent/config.rs`
is extended with a dynamic model-fetch layer:

```
After:
  [model.*] config > unified prefetch > default_models.json (fallback)

Unified prefetch priority:
  xAI /v1/models > provider API models > disk cache > empty
```

The pipeline runs as follows:

1. At startup, after `configure_providers()` merges all config sources
   (env → TOML → CLI), provider model lists are fetched **asynchronously**
   (not blocking startup). A dedicated `ProviderCatalogService` manages
   concurrent refresh, TTL cache, and cancellation.

2. Models are keyed as `"{provider_id}/{model_id}"` (e.g. `"openai/gpt-4o"`,
   `"ollama/llama3.1:8b"`) to avoid collisions across providers.

3. Failures are logged and skipped:
   - Ollama not running → model list stays empty; warning shown
   - Missing API key for OpenAI/Anthropic → models not fetched; user must
     configure `[provider.*]` or rely on `[model.*]` overrides
   - Network errors → cached data is used if available

4. The fetched model catalog is merged into the model resolution pipeline
   as a layer between provider defaults and user `[model.*]` overrides.

**Model merge precedence (deterministic):**
```
manual user model override ([model.*])
  > dynamic provider-discovered model metadata (from catalog service)
  > provider static known-model metadata (embedded defaults)
  > embedded legacy xAI default metadata (legacy/fallback)
```

**Model identity rules:**
- Canonical key for non-legacy models: `provider/model` (e.g. `openai/gpt-4o`)
- Maintain display name separately from canonical key
- An override may enrich metadata but may not silently move a model to another provider
- Duplicates within a provider are deduplicated by canonical model ID
- Same bare model under two providers remains two entries (`openai/gpt-4o` vs `xai/gpt-4o`)
- Bare lookup that matches multiple providers returns an `AmbiguousModel` error
- Legacy bare xAI defaults continue to resolve predictably without provider prefix

**Catalog service API shape (AD-07):**
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

Readers receive immutable `Arc<ModelCatalogSnapshot>`. Cache keys include
provider ID, effective model-list URL, config revision, and credential identity
class — never credential contents.

### 6.2 SamplerConfig construction

`resolve_model_to_sampling_config()` builds `SamplerConfig` from the `Model`'s Route:

```rust
pub fn resolve_model_to_sampling_config(
    model: &Model,
    session_key: Option<&str>,
) -> SamplerConfig {
    let route = &model.route;
    let endpoint = &route.endpoint;

    // Resolve credentials via the route's auth function
    let headers = route.auth.apply(&AuthInput {
        request: &request,
        method: "POST",
        url: endpoint.render(...).to_string(),
        body: body_text,
        headers: default_headers(),
    });

    SamplerConfig {
        base_url: endpoint.base_url.clone(),
        protocol: Some(Arc::new(route.protocol.into_protocol())),
        extra_headers: headers,
        // ...existing fields...
    }
}
```

### 6.3 Stream dispatch

The existing triple-match in `request_task.rs` is replaced by trait dispatch:

```rust
// Before:
match client.api_backend() {
    ApiBackend::ChatCompletions => stream_chat_completions(...),
    ApiBackend::Responses => stream_responses(...),
    ApiBackend::Messages => stream_messages(...),
}

// After:
let protocol = client.protocol();  // &dyn Protocol
for frame in stream {
    let event = protocol.stream.event.decode(frame)?;
    let llm_events = protocol.stream.step(&mut state, event)?;
    for ev in llm_events { ... }
}
```

The `SamplingClient` gains a `protocol: ProtocolId` field and the
`Protocol::stream.step` method is used for uniform dispatch. The three
`stream/*.rs` modules are refactored into `Protocol` values (stateless data,
not trait objects — they carry schema + function pointers).

### 6.4 Multi-Provider Model Fetch

Each provider defines its model listing endpoint via `ProviderDefaults`:

```
model_list_endpoint: Option<String>   // None → {base_url}/models
model_list_format:  ModelListFormat   // OpenAiCompatible | OllamaTags
```

#### Endpoint derivation

| Provider | Base URL | Model List URL | Format | Auth |
|----------|----------|---------------|--------|------|
| xAI | `https://api.x.ai/v1` | `{base_url}/models` | OpenAiCompatible | Bearer |
| OpenAI | `https://api.openai.com/v1` | `{base_url}/models` | OpenAiCompatible | Bearer |
| Anthropic | `https://api.anthropic.com/v1` | `{base_url}/models` | OpenAiCompatible | x-api-key |
| OpenCode | `https://opencode.ai/zen/v1` | `{base_url}/models` | OpenAiCompatible | Bearer or none |
| Ollama | `http://localhost:11434/v1` | `http://localhost:11434/api/tags` | OllamaTags | None |
| OpenAiCompatible | user-specified | `{base_url}/models` | OpenAiCompatible | Bearer |

#### Response parsing

**OpenAiCompatible format** (`ModelListFormat::OpenAiCompatible`):
```json
{"data": [{"id": "gpt-4o", "model": "gpt-4o-2024-11-20", ...}]}
```
Parsed by the existing `parse_remote_model_value()` in `xai-grok-shell/src/remote/client.rs`.
Each entry is keyed as `"{provider_id}/{model_id}"` (e.g. `"openai/gpt-4o"`) to
prevent key collisions across providers.

**Ollama format** (`ModelListFormat::OllamaTags`):
```json
{"models": [{"name": "llama3.1:8b", "modified_at": "...", "size": 123}]}
```
A specialized parser maps `name` → `model`, derives `context_window` from the
provider's `ProviderDefaults.context_window`, and populates the remaining
`ModelEntryConfig` fields with provider defaults.

#### Startup integration

```
configure_providers()   // merges env → TOML → CLI → [endpoints]
       │
       ▼
fetch_provider_models_blocking(registry)
       │  for each provider:
       │    derive URL + auth
       │    HTTP GET → parse → build_prefetched_map()
       │    on failure: log warning, continue
       │
       ▼
merge into model catalog   // Layer 2 in resolve_model_list()
```

The function `fetch_provider_models_blocking()` lives in
`xai-grok-shell/src/agent/models.rs` alongside the existing
`prefetch_models_blocking()`. It reuses the existing `reqwest::blocking::Client`,
`parse_remote_model_value()`, and `build_prefetched_map()` from the main prefetch
pipeline.

#### Caching

Results are cached per provider using `ModelsCacheManager` with a TTL of 300 s.
The cache key is `"{provider_id}|{model_list_url}"` so changing a provider's base
URL or endpoint invalidates the cache. If the fetch fails and a cached entry
exists (even stale), it is used as a fallback to keep the catalog populated.

#### Failure handling

- **Ollama not running**: connection refused → log warning, skip, empty list
- **Missing API key**: 401/403 → log warning, skip
- **Timeout**: 30 s per-provider timeout → log warning, skip
- **All providers fail**: model catalog falls back to `[model.*]` config entries
  and built-in defaults only

---

## 7. Configuration

### 7.1 New config.toml sections

```toml
# ── Provider configuration ────────────────────────────────────────
# Each [provider.<id>] overrides that provider's baked-in defaults.

[provider.openai]
api_key = "sk-..."
# base_url = "https://api.openai.com/v1"     # optional override
# env_key = ["OPENAI_API_KEY"]                # optional override
# extra_headers = { "Custom-Header" = "value" }

[provider.opencode]
# no api_key → free models only, apiKey="public"
# with api_key → all models available
# env_key = ["OPENCODE_API_KEY"]

[provider.ollama]
base_url = "http://localhost:11434/v1"        # override default

# ── Provider-based model shortcuts ────────────────────────────────
# The [provider.*] sections automatically inject [model.*] entries.
# You can still override individual models explicitly:

[model.gpt-4o]
model = "gpt-4o"
api_key = "sk-custom-key"                    # override provider key

[model.deepseek-v4-flash-free]
model = "deepseek-v4-flash-free"
base_url = "https://opencode.ai/zen/v1"      # from opencode provider
api_key = "public"                           # free-tier marker

# ── Legacy xAI config (unchanged) ──────────────────────────────────
# [endpoints]
# cli_chat_proxy_base_url = "..."
# xai_api_base_url = "..."
```

### 7.2 Auto-detection from CLI

```bash
# Explicit provider selection
grok --provider openai --api-key sk-...

# Auto-detect from base URL + API key
grok --api-key sk-... --base-url https://api.openai.com/v1
# → detects "openai", loads defaults

# Minimal: just the API key
grok --api-key sk-...
# → checks base_url from env or defaults to xAI

# Zero config (no credentials)
grok
# → checks session token → xAI OAuth → fallback to default (xAI)
```

---

## 8. Backward Compatibility

| Existing feature | Compatibility strategy |
|---|---|
| `grok login` (xAI OAuth) | Preserved. `XaiProvider` includes full OAuth flow. |
| `XAI_API_KEY` env var | Retroactively detected by `OpenAICompatibleProvider` as generic fallback. |
| `[endpoints]` config | Preserved for xAI endpoint overrides. |
| `[model.*]` config | Highest priority, unchanged semantics. |
| `default_models.json` | Still loaded as fallback when no provider matches. |
| `grok-4*` model references | Resolved via xAI provider's model list. |
| `[models].default = "grok-build"` | Still works; resolves via xAI provider. |
| ACP protocol (pager ↔ shell) | Unchanged. Pager continues to receive model list via ACP. |

### Feature gating

When no xAI-specific features are needed, the following are automatically disabled:

| Feature | Condition | Behavior |
|---------|-----------|----------|
| OAuth token refresh | Provider is not xAI | Skipped |
| `x-grok-*` headers | Provider is not xAI | Not injected |
| `x_search` raw tools | Provider is not xAI | Removed from body |
| Doom-loop recovery | Provider is not xAI | Disabled |
| `inject_url_derived_headers()` | URL is not cli-chat-proxy | No-op |
| Remote model prefetch | Provider is not xAI | Skipped (unless explicitly configured) |
| Sentry/telemetry | Provider is not xAI | Can be disabled via config |

---

## 9. Provider Registration Flow

```
Startup
  │
  ├── 1. Load config.toml
  │      ├── parse [model.*] sections
  │      ├── parse [provider.*] sections
  │      └── parse [endpoints] (legacy)
  │
  ├── 2. Initialize ProviderRegistry
  │      ├── register built-in Provider: xai
  │      ├── register built-in Provider: openai
  │      ├── register built-in Provider: anthropic
  │      ├── register built-in Provider: opencode
  │      ├── register built-in Provider: ollama
  │      └── register built-in Provider: openai-compatible
  │
  ├── 3. Merge user [provider.*] configs
  │      for each configured provider:
  │        provider.configure(config_overrides) → ConfiguredProvider
  │        ConfiguredProvider.route is composed from:
  │          protocol (reused, shared)
  │          + endpoint (base_url from config or baked-in)
  │          + auth   (inline → env → none, composed via .or_else())
  │          + framing (SseFraming for all HTTP providers)
  │
  ├── 4. Fetch provider model lists
  │      for each registered provider:
  │        derive model_list_url + auth from ProviderDefaults + ProviderConfig
  │        HTTP GET → parse → key as "{provider_id}/{model_id}"
  │        on failure: log warning, skip provider
  │
  ├── 5. Detect from CLI flags
  │      if --provider: use that provider
  │      if --api-key + --base-url: auto-detect provider from URL
  │      if only --api-key: use default provider (xAI)
  │
  ├── 6. Build final model catalog
  │      merge: user [model.*] > unified prefetch (xAI + provider API)
  │      each entry is a ModelEntry { id, provider, base_url, ... }
  │
  └── 7. Start session
         selected Model → route.auth.apply() → resolve credentials
         → build SamplerConfig { base_url, protocol_id, extra_headers }
         → SamplingClient executes Protocol.step for each frame
```

---

## 10. Example Provider Implementation

```rust
// crates/codegen/xai-grok-provider/src/providers/openai.rs

use super::*;

pub const PROVIDER_ID: ProviderId = ProviderId(ProviderId::OPENAI);
pub const BASE_URL: &str = "https://api.openai.com/v1";
pub const PATH: &str = "/chat/completions";

/// Protocol reference — the same ChatCompletionsProtocol is shared by all
/// OpenAI-compatible providers (DeepSeek, Groq, Together, etc.).
pub const PROTOCOL_ID: ProtocolId = ProtocolId::CHAT_COMPLETIONS;

pub struct OpenAIProvider;

impl Provider for OpenAIProvider {
    fn id(&self) -> &ProviderId { &PROVIDER_ID }
    fn name(&self) -> &str { "OpenAI" }

    fn defaults(&self) -> &ProviderDefaults {
        &ProviderDefaults {
            id: PROVIDER_ID.clone(),
            name: "OpenAI".into(),
            base_url: BASE_URL.into(),
            api_backend: ApiBackend::ChatCompletions,
            auth_scheme: AuthScheme::Bearer,
            context_window: NonZeroU64::new(128_000).unwrap(),
            temperature: Some(0.7),
            top_p: Some(0.95),
            max_completion_tokens: Some(8192),
            supports_reasoning_effort: true,
            supports_streaming: true,
            supports_tool_calling: true,
            model_list_endpoint: None,
            model_list_format: ModelListFormat::OpenAiCompatible,
            ..Default::default()
        }
    }

    fn configure(&self, overrides: ProviderConfig) -> ConfiguredProvider {
        let auth = Credential::optional(overrides.api_key, "api_key")
            .or_else(Credential::config("OPENAI_API_KEY"))
            .bearer();

        // Compose the Route: protocol + endpoint + auth + framing
        let route = Route::make(RouteInput {
            id: "openai-chat",
            provider: Some(PROVIDER_ID.clone()),
            protocol: PROTOCOL_ID,
            endpoint: Endpoint {
                base_url: overrides.base_url.or(Some(BASE_URL.into())),
                path: EndpointPart::Static(PATH.into()),
                query: None,
            },
            auth: Some(auth),
            framing: Box::new(SseFraming),
            defaults: Some(RouteDefaultsInput {
                generation: Some(GenerationOptions {
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        });

        ConfiguredProvider {
            id: PROVIDER_ID.clone(),
            route,
            model: |model_id, route| Model::make(ModelInput {
                id: model_id.into(),
                provider: PROVIDER_ID.clone(),
                route: route.clone(),
                defaults: None,
            }),
            configure: |c| self.configure(c),
        }
    }

}
```

### Anthropic Provider — Auth Chaining Pattern

Anthropic uses `x-api-key` header instead of Bearer, demonstrating the
composable auth pattern:

```rust
fn anthropic_auth(api_key: Option<String>) -> Box<dyn AuthFn> {
    Credential::optional(api_key, "api_key")
        .or_else(Credential::config("ANTHROPIC_API_KEY"))
        .header("x-api-key")   // ← render as header, not Bearer
}
```

The Anthropic route has one extra default header:

```rust
route.with({
    auth: anthropic_auth(overrides.api_key),
    endpoint: { base_url: "https://api.anthropic.com/v1" },
    defaults: {
        headers: { "anthropic-version": "2023-06-01" },
    },
})

---

## 7. TUI Provider Configuration

### 7.1 Overview

The TUI provides an interactive Provider management interface. The following
entry points and components are **designed** but may be partially implemented:

| Entry | Status | Description |
|-------|--------|-------------|
| `/providers` command | ✅ Done | Opens Providers list modal via `Action::OpenProviders` |
| `Action::OpenProviders` | ✅ Done | Dispatch action, creates `ActiveModal::Providers` |
| `ActiveModal::Providers` | ✅ Done | Variant registered in modal enum |
| Providers modal UI | ⚠️ Stub | Renders hardcoded 5-row list, no keyboard interaction |
| Provider Detail panel | ❌ Not started | API Key / Base URL / Models editing form |
| `F2` → Settings → Providers | ❌ Not started | Settings `defs.rs` not modified |
| `Ctrl+M` provider prefix | ⚠️ Partial | Parser supports `provider/model`, picker display not extended |

### 7.2 Providers Modal

Target design (not yet fully implemented):

```
┌─ Providers ────────────────────────────────────────────[✗]─┐
│                                                              │
│   Provider           Status       Models     Endpoint        │
│  ─────────────────────────────────────────────────────────── │
│   ● xAI              ✅ 已连接      1         api.x.ai       │
│   ○ OpenAI           ❌ 未配置      0         —               │
│   ○ Anthropic        ⚡ 需认证      2         api.anthropic   │
│   ○ OpenCode Zen     🔓 免费模式    —         opencode.ai     │
│   ○ Ollama           🟢 本地        —         localhost:11434 │
│                                                              │
│   [a Add] [Enter Configure] [x Remove] [t Test]              │
└──────────────────────────────────────────────────────────────┘
```

**Implementation status**: `ActiveModal::Providers` variant and `ProvidersModalState`
struct exist. The modal can be opened via `/providers` command but rendering is
a static hardcoded list. Keyboard interaction (↑↓/Enter/Esc), `ModalWindowState`
chrome, status indicators, and live data from `ProviderRegistry` are not yet
implemented. Full implementation requires modifying 5 dispatch points in
`app/modals.rs` (2800+ lines):
- `draw_active_modal()` — render the provider list and detail panel
- `handle_modal_key()` — keyboard navigation and API key input
- `handle_modal_mouse()` — mouse click handling
- `active_modal_height()` — modal sizing
- `modal_can_drain()` — state management

### 7.3 Provider Detail Panel

Target design (not implemented):

| Field | Type | Behavior |
|-------|------|----------|
| API Key | Hidden string | Reveal/hide toggle, masked input `●●●●` |
| Base URL | String | URL input, defaults to provider's baked-in URL |
| Status | Display | Connected / Not configured / Needs auth |
| Models | Display | Fetched dynamically from provider's model list API |

API Key input should follow the existing `ModalInput` pattern used by the
Extensions modal for MCP server configuration (inline form with Tab/BackTab
field switching, readline-style editing).

### 7.4 Model Picker Enhancement

Target design (partially implemented):

```
Before:  grok-build              gpt-4o              claude-sonnet
After:   xai/grok-build          openai/gpt-4o       anthropic/claude-sonnet
```

**`--model` CLI flag** supports `provider/model` format via `parse_model_ref()`
in `xai-grok-provider/src/types.rs`. The `Ctrl+M` picker display enhancement
awaits ACP protocol extension to include `provider` field in `acp::ModelInfo`.

### 7.5 Slash Command: `/providers`

```
/providers           → Opens Provider list modal (stub)
/providers <name>    → Opens configuration panel for that provider (not implemented)
```

Implements `SlashCommand` trait. `run()` returns `CommandResult::Action(Action::OpenProviders)`.
`suggest_args()` returns all built-in Provider IDs (`xai`, `openai`, `anthropic`,
`opencode`, `ollama`) for tab completion.

### 7.6 Settings Integration

Target design (not implemented):

```
Models
  ├── Default model         (DynamicEnum, existing)
  ├── Fork secondary model  (DynamicEnum, existing)
  └── Providers ›           (Group → opens provider list)
```

Requires adding a `Provider` setting category entry in `settings/defs.rs`.

### 7.7 Status Indicators

Target design (not yet used in UI):

| Status | Badge | Color | Condition |
|--------|-------|-------|-----------|
| Connected | `✅` | `accent_success` | API key resolved, last test passed |
| Not configured | `❌` | `accent_error` | No API key available |
| Needs auth | `⚡` | `warning` | Session token expired |
| Free tier | `🔓` | `gray` | Public key fallback (OpenCode) |
| Local | `🟢` | `running` | Auto-detected (Ollama) |

### 7.8 Interaction Flows

```
First-time user:  Welcome → /providers → Select Provider → Enter API Key → Verify → Chat
                  (Welcome screen integration not implemented)

Switch provider:  Ctrl+M → select model → (if unconfigured: enter key) → Switch
                  (auto-config prompt not implemented)

CLI headless:     grok -p "hello" --provider openai --api-key sk-...
                  (fully supported)
```

---

## 8. Boundary Conditions

### 8.1 What if no provider matches?

### 8.2 What if both [provider.*] and [model.*] configure the same model?

`[model.*]` always wins — it is the highest-priority layer. Provider defaults only
fill in fields not explicitly set by the user.

### 8.3 What about streaming vs non-streaming?

All providers support streaming. The `Protocol.stream.step()` function handles both
modes. Non-streaming requests use `collect_response()` which drains the stream and
assembles the final `ConversationResponse`.

### 8.4 Provider-specific capabilities

The `ProviderDefaults` struct carries capability flags (`supports_tool_calling`,
`supports_structured_output`, etc.) that the session actor checks before
enabling features. When a capability is unsupported, the system degrades gracefully
(e.g. structured output falls back to tool-based generation).

### 8.5 New providers from config

Users can define entirely new providers without code changes by using the
`openai-compatible` provider type, which accepts arbitrary `base_url`, `api_key`,
and model definitions in config.toml.

---

## 9. Key Design Decisions from OpenCode

The architecture above is informed by studying OpenCode's model adapter layer at
`packages/llm/src/`. The following patterns proved most impactful:

| Pattern | OpenCode Idiom | Grok Build Equivalent |
|---------|---------------|---------------------- |
| **Protocol reuse** | One `OpenAIChat.protocol` shared by 20+ providers | `ProtocolId::ChatCompletions` reused by OpenAI, DeepSeek, Groq, Together, etc. |
| **Values over registries** | `Route.make({...})` returns a value, registers nothing globally | `Route::make(...)` returns a `Route` struct; `ProviderRegistry` is explicit |
| **Auth chaining** | `Auth.optional(key).orElse(Auth.config("ENV")).bearer()` | `Credential::optional(k).or_else(Credential::config("E")).bearer()` |
| **Immutable patching** | `route.with({ auth, endpoint, ... })` returns new route | `Route::with(self, patch) -> Route` |
| **3-axis composition (V1)** | `Route = Protocol + Endpoint + Auth` (framing owned by protocol) | Stream framing/decoding owned by protocol, not route |
| **Request-level HTTP overlay** | `http.body`/`http.headers`/`http.query` with protocol-field denylist | `HttpOptions` with denylist for protocol-owned fields |
| **M:N protocol-to-provider** | One protocol → many providers; one provider → many protocols | `OpenAIProvider` exposes both `ChatCompletions` and `Responses` routes |
| **Provider facade** | `configure({apiKey})` returns `{ id, model, configure }` | `Provider::configure()` returns `ConfiguredProvider` |

## 10. File Map

```
crates/codegen/xai-grok-provider/          [NEW]
├── Cargo.toml
├── src/
│   ├── lib.rs                             # Re-exports
│   ├── types.rs                           # ProviderId, ProviderDefaults, ModelListFormat, ApiBackend, AuthScheme
│   ├── provider.rs                        # Provider trait, ConfiguredProvider
│   ├── registry.rs                        # ProviderRegistry
│   ├── route.rs                           # Route, RouteInput, RoutePatch
│   ├── endpoint.rs                        # Endpoint, EndpointPart, EndpointInput
│   ├── framing.rs                         # Framing trait, SseFraming
│   ├── auth.rs                            # Credential, AuthFn, chaining helpers
│   ├── model.rs                           # Model, ModelDefaults, ModelLimits
│   ├── events.rs                          # LLMEvent, Usage, FinishReason
│   ├── protocol.rs                        # Protocol, ProtocolBody, ProtocolStream
│   ├── config.rs                          # ProviderConfig deserialization
│   └── providers/
│       ├── mod.rs                         # register_all()
│       ├── xai.rs                         # XaiProvider (OAuth, x-grok-* headers)
│       ├── openai.rs                      # OpenAIProvider (Chat + Responses)
│       ├── anthropic.rs                   # AnthropicProvider (Messages, x-api-key)
│       ├── opencode.rs                    # OpenCodeProvider (Zen gateway)
│       ├── ollama.rs                      # OllamaProvider (local, no auth)
│       └── openai_compatible.rs           # Generic provider + profiles

crates/codegen/xai-grok-sampler/           [MODIFIED]
├── src/
│   ├── protocol.rs                        # NEW: Protocol value definitions
│   ├── client.rs                          # MODIFIED: uses Protocol from Route
│   ├── config.rs                          # MODIFIED: SamplerConfig gains protocol_id
│   ├── protocols/                         # [NEW] Protocol values (stateless data)
│   │   ├── mod.rs
│   │   ├── chat_completions.rs            # REFACTORED from stream/*
│   │   ├── responses.rs                   # REFACTORED from stream/*
│   │   └── messages.rs                    # REFACTORED from stream/*
│   └── actor/
│       └── request_task.rs                # MODIFIED: Protocol.step dispatch

crates/codegen/xai-grok-shell/src/         [MODIFIED]
├── agent/
│   └── config.rs                          # MODIFIED: Route-based model resolution
├── auth/
│   ├── manager.rs                         # MODIFIED: wrapped by XaiProvider
│   └── credential_provider.rs             # MODIFIED: adapts to AuthFn
└── session/
    └── acp_session_impl/
        └── sampler_turn.rs                # MODIFIED: uses Route from Model

crates/codegen/xai-grok-models/
└── default_models.json                    # MODIFIED: embeds provider sections
