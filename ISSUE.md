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
- **M11** `auth.rs:16-19` — 认证链模式不匹配：Arch 要求 `Credential::opt().or_else().bearer()`，当前需 `Credential::opt().bearer().or_else()` -Closed
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

## 第三审计: 2026-07-16 (再审)

**再审结果**: M11 通过 → `-Closed`。其余 7 项 (S02/S04/S05/S07/S08/S09/S10) 保留 `-Fixed`，属非功能性改进或后续 Phase 范围

---

## 第二审计: 2026-07-16 (再审 + 自由审计)

**再审结论**: 22 项核对通过 → `-Closed`；S02 (doc comments) 覆盖面不足，保留 `-Fixed`
**自由审计结论**: 新增 6 项发现，已合并上方 ISSUE 列表

### 自由审计新增条目（已合并至对应分类）

- **C03** `route.rs:47` — `Route` 缺少 `headers` 字段 -Closed
- **C04** `route.rs:105-112` — `RoutePatch` 缺少 `auth` 和 `headers` 字段 -Closed
- **C05** `Cargo.toml` — 缺少 `thiserror` 依赖；所有错误类型均用裸 `String` 而非 `thiserror` 派生 -Closed
- **S11** `types.rs:121-126` — 新增 `HeaderMap` 类型别名避免与 `reqwest` 冲突 -Closed

---

## Phase 2 审计: 2026-07-16

**范围**: `crates/codegen/xai-grok-sampler/src/` — protocol dispatch 重构
**参考**: `docs/model-adapter-architecture.md` §6.3, `docs/implementation-plan.md` Phase 2

### 严重

- **P2-C01** `client.rs:2027-2056`, `actor/state.rs:82-112` — 测试辅助函数 (`minimal_config`, `cfg`) 未添加 `protocol_id` 字段，导致所有测试编译失败 -Fixed
- **P2-C02** `request_task.rs:429-473` — dispatch key 改为 `protocol_id` 但实际仍调用旧 `stream_*()` 函数，未使用 `Protocol::stream.step()`。此条目为设计阶段差异 — 完整的 Protocol trait dispatch 需在 Phase 4 之后才能实现
- **P2-C03** `(缺失文件)` — `protocol.rs` 文件不存在。Arch §13 文件映射要求此文件包含 `ProtocolTable` 注册逻辑

### 中等

- **P2-M01** `client.rs:1980-2018` — `conversation_collect()` 仍使用 `match api_backend()`，与 `request_task.rs` 的 `protocol_id` dispatch 不一致 -Fixed
- **P2-M02** `Cargo.toml:10` — `xai-grok-provider` 依赖已声明但未在任何 `.rs` 文件中被 `use`，当前无实际用途
- **P2-M03** `config.rs:60, client.rs:316` — `protocol_id` 字段类型为 `String` 而非 `ProtocolId`（当前 `ProtocolId` 是类型别名，功能上等价）
- **P2-M04** `protocols/` — 目录不完整，仅有 `mod.rs`，缺少 `chat_completions.rs`/`responses.rs`/`messages.rs` 协议值文件

### 建议

- **P2-S01** `request_task.rs:109` — span label 仍引用 `api_backend()` 而非 `protocol_id()` -Fixed
- **P2-S02** `protocols/mod.rs` — 协议 ID 常量类型为 `&str` 而非 `ProtocolId`
- **P2-S03** `stream/` — 目录未被删除（计划在 Phase 7 清理，当前状态不一致）

---

## Phase 3 审计: 2026-07-16

**范围**: `xai-grok-provider/src/auth.rs`, `xai-grok-sampler/src/`, `xai-grok-shell/src/auth/provider_adapter.rs`
**参考**: `docs/model-adapter-architecture.md` §3.9, `docs/implementation-plan.md` Phase 3

### 中等

- **P3-M01** `xai-grok-sampler/src/sampling_log.rs:18-27` — span 字段名 `api_backend` 与实际传入值 `protocol_id()` 语义不符 -Closed
- **P3-M02** `xai-grok-sampler/src/client.rs:547-549` — `api_backend()` 公开方法仍保留，已加 `#[deprecated]` -Closed
- **P3-M03** `xai-grok-sampler/src/client.rs:2202` — 测试使用 `api_backend()` 而非 `protocol_id()` -Closed
- **P3-M04** `xai-grok-sampler/Cargo.toml:10` — `xai-grok-provider` 依赖已声明但未在任何 `.rs` 中使用 -Closed
- **P3-M05** `xai-grok-sampler/src/actor/request_task.rs:109` — `request_span()` 参数名 `api_backend` 语义错误，与 M01 同根因 -Closed

### 建议

- **P3-S01** `auth.rs` — `AuthFn` 签名与 Arch §3.9 存在细微偏差（错误类型为 `String` 而非 `HeaderMap`）
- **P3-S02** `auth.rs:121` — `Credential::or_else()` 在胜出分支上重复调用 `resolve()`
- **P3-S03** `auth.rs:8` — `AuthInput.request` 字段无实际用途，仅架构占位
- **P3-S04** `auth.rs:3` — `type HeaderMap` 与 `reqwest::HeaderMap` 命名冲突风险
- **P3-S05** `auth.rs:15` — `AuthFn` trait 的 `'static` 约束限制短生命周期闭包
- **P3-S06** `xai-grok-shell/src/auth/credential_provider.rs` — `ShellAuthCredentialProvider` 未实现 `AuthFn`，计划描述与实现有偏差
- **P3-S07** `auth.rs:19-27` — `or_else`/`and_then` 置于 `impl dyn AuthFn` 而非 trait 中
- **P3-S08** `auth.rs:56,67,90` — `BearerAuth`/`HeaderAuth`/`FailAuth` 为私有类型
- **P3-S09** `auth.rs:112` — `Credential::config()` 接受 `&str` 后 `.to_owned()`，可改 `impl Into<String>`

---

## Phase 4 审计: 2026-07-16

**范围**: `xai-grok-provider/src/providers/` — 6 个内置 Provider 实现
**参考**: `docs/model-adapter-architecture.md` §5, `docs/implementation-plan.md` Phase 4

### 严重

- **P4-C01** `xai.rs:64-66` — xAI auth 链缺少 `SessionToken(OAuth)` 回退。`grok login` 建立的 OAuth 会话令牌无法通过新 provider 系统解析，破坏向后兼容
- **P4-C02** `xai.rs:32` — xAI `known_models` 为空 (Arch §5.1 要求 `grok-build`)
- **P4-C03** `anthropic.rs:33` — Anthropic `known_models` 为空 (Arch §5.3 要求 `claude-sonnet-4-20250514`, `claude-haiku-3-5-20241022`)

### 中等

- **P4-M01** `xai.rs:31` — xAI provider 缺少 `x-grok-*` 头部注入 (Arch §5.1 + 实施计划 4.2)
- **P4-M02** `openai.rs:86-98` — OpenAI 缺少双协议支持 (Arch §5.2 + 实施计划 4.3 要求 Chat + Responses 双 Route)
- **P4-M03** `opencode.rs:63-65` — OpenCode 免费层公共回退缺失 (Arch §5.4 要求 `PublicKey("public")` 回退)
- **P4-M04** `anthropic.rs:15,70` — `anthropic-version` 头部在 `ProviderDefaults.extra_headers` 和 `RouteDefaults.headers` 中重复

### 建议

- **P4-S01** 全局 provider — `pub` 结构体缺少 `///` 文档注释 (AGENTS.md §3.3)
- **P4-S02** 全局 provider — `defaults()` 函数可见性不一致 (`pub`/`pub(crate)`/私有混合)
- **P4-S03** `anthropic.rs:13` — `use indexmap::IndexMap` 局部导入而非模块顶部导入
- **P4-S04** `openai_compatible.rs:13` — `#[allow(dead_code)]` 在 `profile_base_url()` 上
- **P4-S05** 全局 provider — `configure()` 中 `model` 闭包可简化为 `route.model(id)`
- **P4-S06** 全局 provider — 常量 `NonZeroU64::new(x).unwrap()` 应使用 `.expect()`
- **P4-S07** `xai.rs:69` — 路由 ID `"xai-responses"` 硬编码，未来扩展不便
