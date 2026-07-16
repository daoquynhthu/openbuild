# Issue Audit — Model Adapter Layer Refactoring

> 仅当用户请求代码审计时使用。审计完成后按分类写入。

---

## 审计: 2026-07-16

**范围**: `crates/codegen/xai-grok-provider/src/` Phase 1 实现，共 13 个模块
**参考**: `docs/model-adapter-architecture.md`
**模式**: 只读审计

---

### 严重

- **Route 丢弃 auth 和 framing 字段** | `route.rs:36-43` | `RouteInput` 接收了 `auth` 和 `framing`，但 `Route` 结构体没有这两个字段，`Route::make()` 直接丢弃它们。违背 Arch §3.4 四轴组合（Protocol + Endpoint + Auth + Framing）。建议修复：将 `auth: Option<Box<dyn AuthFn>>` 和 `framing: Box<dyn Framing<String>>` 加入 `Route`。

- **ProviderRegistry 中使用裸 `unwrap()`** | `registry.rs:29,33,55` | `RwLock` 操作使用 `.unwrap()`，在锁毒化时会直接 panic。违反 AGENTS.md §3.2。建议修复：使用 `.expect("lock poisoned")` 或 `.lock().unwrap_or_else(|e| e.into_inner())`。

---

### 中等

- **`ProviderDefaults.api_backend` 为 `String` 而非 `ApiBackend` 枚举** | `types.rs:39` | Arch §3.2 要求 `ApiBackend`（`ChatCompletions | Responses | Messages`），该类型完全不存在。

- **`ProviderDefaults.auth_scheme` 为 `String` 而非 `AuthScheme` 枚举** | `types.rs:40` | Arch §3.2 要求 `AuthScheme`（`Bearer | XApiKey`），该类型完全不存在。

- **`context_window` 应为 `NonZeroU64`** | `types.rs:42,29` | 当前为 `u64`。Arch §3.2 要求 `NonZeroU64`。

- **`ProviderModelDef` 缺少 `api_backend` 和 `supports_reasoning_effort` 字段** | `types.rs:24-31` | Arch §3.2 定义了这两个可选字段。

- **`ProviderConfig` 缺少 `id` 字段** | `config.rs:7-12` | Arch §3.4 包含 `pub id: Option<String>`。

- **`EndpointInput.request` 为 `()` 而非 `LLMRequest`** | `endpoint.rs:6` | 导致 `EndpointPart::Dynamic` 无法访问请求信息。

- **缺少 `EndpointPatch` 类型** | `endpoint.rs:52-58` | Arch §3.7 要求 `EndpointPatch`（所有字段为 `Option`），当前使用完整 `Endpoint`。

- **`AuthFn` trait 缺少 `or_else` 和 `and_then` 方法** | `auth.rs:12-14` | `or_else` 实现在 `impl dyn AuthFn` 上而非 trait 中，`and_then` 完全缺失。

- **`AuthInput` 缺少 `request` 和 `body` 字段** | `auth.rs:6-10` | Arch §3.9 要求 `request: &LLMRequest` 和 `body: String`。

- **`Credential` 缺少 `Session` 变体** | `auth.rs:66-70` | Arch §3.9 要求支持 OAuth 会话回退，对 xAI 兼容性至关重要。

- **认证链模式与架构不符** | `auth.rs:16-19` | Arch 展示 `Credential::opt(..).or_else(..).bearer()`，当前需 `Credential::opt(..).bearer().or_else(..)`。

- **`ProtocolBody` 缺少 `schema` 字段** | `protocol.rs:9-11` | Arch §3.5 要求 `pub schema: Schema<Body>`。

- **`ProtocolStream` 缺少 `event` 字段** | `protocol.rs:13-19` | Arch §3.5 要求 `pub event: Schema<Event>`。

- **`ConfiguredProvider` 缺少 `configure` 字段** | `provider.rs:8-12` | Arch §3.3 要求 `pub configure: fn(ProviderConfig) -> ConfiguredProvider`。

- **`Route` 缺少 `headers` 字段；`RoutePatch` 缺少 `auth`、`defaults`、`headers`** | `route.rs:36-43,82-86` | Arch §3.4 定义完整。

- **`Model.id` / `Model.provider` 使用 `String` 而非 `ModelId` / `ProviderId`** | `model.rs:8-9` | 丢失类型安全性。`ModelId` 类型不存在。

- **`ProviderRegistry` 缺少 `routes` 映射** | `registry.rs:10-12` | Arch §4 要求管理命名路由。

- **`detect_from_url()` 是存根** | `registry.rs:58-60` | Arch §4 要求 URL 模式匹配，当前仅返回 `"openai-compatible"`。

- **`LLMRequest` 定义不完整** | `types.rs:81-83` | 仅含 `model: String`，缺少消息列表等字段。

- **`ToolResult` 使用 `Value` 而非专用 `ToolResultValue`** | `events.rs:17` | Arch §3.6 要求专用类型。

- **未使用的依赖 `thiserror`、`http`、`tokio-stream`** | `Cargo.toml` | 增加编译时间且无收益。`thiserror` 尤其应被用于自定义错误类型。

---

### 建议

- **缺少 `#[non_exhaustive]`** | 涉及 `LLMEvent`、`FinishReason`、`ErrorKind`、`Usage`、`Credential`、`ProviderDefaults`、`ProviderModelDef`、`ModelDefaults`、`ModelLimits`、`GenerationOptions`、`HttpOptions`、`RouteDefaults`、`EndpointPart`、`EndpointInput`、`AuthInput` 共 15 个类型 | AGENTS.md §3.4 要求。

- **公共项缺少文档注释** | 涉及所有模块的 `pub` 项 | AGENTS.md §3.3 要求。

- **缺少 serde 派生** | `LLMEvent`、`Usage`、`FinishReason`、`ErrorKind`、`GenerationOptions`、`Endpoint`、`RouteDefaults`、`Route`、`Model` | AGENTS.md §3.4 要求。

- **`Endpoint::render()` 静默回退到 localhost** | `endpoint.rs:34-42` | 可能掩盖配置错误，应返回 `Result<Url>`。

- **`SseFraming` 丢失 UTF-8 错误** | `framing.rs:28` | `from_utf8_lossy` 静默替换损坏的字节。

- **`Usage` 未对不变式添加文档/断言** | `events.rs:52-61` | 非重叠分解的 invariants 未记录或验证。

- **`ProtocolTable` 存储 `String` 而非实际协议** | `protocol.rs:44-46` | 功能降级。

- **`ProtocolBody`/`ProtocolStream`/`Protocol` 缺少 `Debug`** | `protocol.rs:9,13,21` | `fn` 指针导致手动 `Debug` 缺失。

- **`Provider::configure()` 使用 `fn` 指针** | `provider.rs:27` | 可能不适用于非装箱闭包；考虑 `Arc<dyn Fn>`。

- **`providers/mod.rs` 为空** | `providers/mod.rs` | Arch §5 和 §13 描述了 6 个 Provider 实现和 `register_all()`。

- **命名差异** | `auth.rs:3` | `HeaderMap` 类型与 `reqwest::HeaderMap` 冲突风险。
