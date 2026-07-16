# Issue Audit — Model Adapter Layer Refactoring

> 仅当用户请求代码审计时使用。审计完成后按分类写入。

---

## 审计: 2026-07-16

**范围**: `crates/codegen/xai-grok-provider/src/` Phase 1 实现 (13 个模块)
**参考**: `docs/model-adapter-architecture.md`

### 严重

- **C01** `route.rs:36-43` — `Route` 丢弃 `auth` 和 `framing` 字段。`RouteInput` 接收了二者，但 `Route` 结构体不包含它们，`Route::make()` 直接丢弃。违背 Arch §3.4 四轴组合 (Protocol + Endpoint + Auth + Framing) -Closed
- **C02** `registry.rs:29,33,55` — `RwLock` 操作使用裸 `.unwrap()`，AGENTS.md §3.2 禁止 -Closed
- **C03** `route.rs:47` — `Route` 缺少 `headers: Option<fn(&LLMRequest) -> HeaderMap>` 字段，Arch §3.4 要求 -Closed
- **C04** `route.rs:105-112` — `RoutePatch` 缺少 `auth: Option<Box<dyn AuthFn>>` 和 `headers` 字段，Arch §3.4 要求 -Closed
- **C05** `Cargo.toml` — 缺少 `thiserror`，AGENTS.md §3.2 要求使用 `thiserror` 定义错误类型 -Closed

### 中等

- **M01** `types.rs:39` — `ProviderDefaults.api_backend` 类型为 `String`，Arch §3.2 要求 `ApiBackend` 枚举 -Closed
- **M02** `types.rs:40` — `ProviderDefaults.auth_scheme` 类型为 `String`，Arch §3.2 要求 `AuthScheme` 枚举 -Closed
- **M03** `types.rs:42,29` — `context_window` 类型为 `u64`，Arch §3.2 要求 `NonZeroU64` -Closed
- **M04** `types.rs:24-31` — `ProviderModelDef` 缺少 `api_backend` 和 `supports_reasoning_effort` 字段 -Closed
- **M05** `config.rs:7-12` — `ProviderConfig` 缺少 `id: Option<String>` 字段 -Closed
- **M06** `endpoint.rs:6` — `EndpointInput.request` 类型为 `()`，Arch §3.7 要求 `LLMRequest` -Closed
- **M07** `endpoint.rs:52-58` — 缺少 `EndpointPatch` 类型，当前使用完整 `Endpoint` 替代 -Closed
- **M08** `auth.rs:12-14` — `AuthFn` trait 缺少 `or_else` 和 `and_then` 方法 -Closed
- **M09** `auth.rs:6-10` — `AuthInput` 缺少 `request` 和 `body` 字段 -Closed
- **M10** `auth.rs:66-70` — `Credential` 枚举缺少 `Session` 变体 -Closed
- **M11** `auth.rs:16-19` — 认证链模式不匹配：Arch 要求 `Credential::opt().or_else().bearer()`，当前需 `Credential::opt().bearer().or_else()` -Fixed
- **M12** `protocol.rs:9-11` — `ProtocolBody` 缺少 `schema: Schema<Body>` 字段 -Closed
- **M13** `protocol.rs:13-19` — `ProtocolStream` 缺少 `event: Schema<Event>` 字段 -Closed
- **M14** `provider.rs:8-12` — `ConfiguredProvider` 缺少 `configure` 字段 -Closed
- **M15** `route.rs:36-43,82-86` — `Route` 缺少 `auth`/`framing` 字段；`RoutePatch` 缺少 `defaults` 字段 -Closed
- **M16** `model.rs:8-9` — `Model.id`/`Model.provider` 使用 `String`，Arch §3.5 要求 `ModelId`/`ProviderId` -Closed
- **M17** `registry.rs:10-12` — `ProviderRegistry` 缺少 `routes` 映射 -Closed
- **M18** `registry.rs:58-60` — `detect_from_url()` 是空存根 -Closed
- **M19** `types.rs:81-83` — `LLMRequest` 定义不完整 -Closed
- **M20** `events.rs:17` — `ToolResult` 使用 `serde_json::Value` 而非 `ToolResultValue` -Closed
- **M21** `Cargo.toml` — 未使用的依赖 `thiserror`、`http`、`tokio-stream` -Closed

### 建议

- **S01** 全局 — 15 个类型缺少 `#[non_exhaustive]` -Closed
- **S02** 全局 — 所有 `pub` 项缺少 `///` 文档注释 -Fixed
- **S03** 全局 — 多类型缺少 serde 派生 -Closed
- **S04** `endpoint.rs:34-42` — `Endpoint::render()` 静默回退到 `http://localhost/` -Fixed
- **S05** `framing.rs:28` — `String::from_utf8_lossy` 静默替换无效 UTF-8 字节 -Fixed
- **S06** `events.rs:52-61` — `Usage` 的非重叠分解 invariants 未记录或验证 -Closed
- **S07** `protocol.rs:44-46` — `ProtocolTable` 存储 `String` 而非实际协议值 -Fixed
- **S08** `protocol.rs:9,13,21` — `Protocol` 因 `fn` 指针缺少 `Debug` -Fixed
- **S09** `provider.rs:27` — `configure()` 使用 `fn` 指针，建议 `Arc<dyn Fn>` -Fixed
- **S10** `providers/mod.rs:1` — 为空，Arch §5 和 §13 要求 6 个 Provider 实现 -Fixed
- **S11** `auth.rs:3` — `type HeaderMap` 与 `reqwest::HeaderMap` 冲突风险 -Closed

---

## 第二审计: 2026-07-16 (再审 + 自由审计)

**再审结论**: 22 项核对通过 → `-Closed`；S02 (doc comments) 覆盖面不足，保留 `-Fixed`
**自由审计结论**: 新增 6 项发现，已合并上方 ISSUE 列表

### 自由审计新增条目（已合并至对应分类）

- **C03** `route.rs:47` — `Route` 缺少 `headers` 字段 -Closed
- **C04** `route.rs:105-112` — `RoutePatch` 缺少 `auth` 和 `headers` 字段 -Closed
- **C05** `Cargo.toml` — 缺少 `thiserror` 依赖；所有错误类型均用裸 `String` 而非 `thiserror` 派生 -Closed
- **S11** `types.rs:121-126` — 新增 `HeaderMap` 类型别名避免与 `reqwest` 冲突 -Closed
