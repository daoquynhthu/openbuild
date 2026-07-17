# Issue Audit — Model Adapter Layer Refactoring

> 仅当用户请求代码审计时使用。审计完成后按分类写入。

---

## 修复执行顺序（权威计划）

**规则**: 按 Batch 顺序执行，每 Batch 4 项。完成后在项目前加 `[x]`。

### Batch 1 — `#[allow(...)]` 清理（低风险，暴露真问题）`[x]`

- [x] **C22** `main.rs:1` — 移除 `#![allow(dead_code)]`
- [x] **P5-M05** `main.rs:1-7` — 移除 `#![allow(unused_imports, ...)]`
- [x] **M61** `openai_compatible.rs:15` — `profile_base_url` 从未被调用，`configure()` 注释描述意图但未实现。已在 `configure()` 中接上 caller：当 `base_url` 未设置时，通过 `overrides.id` 查 `profile_base_url` 推得知名服务商 base URL
- [x] **M62** `providers/mod.rs:49` — 移除内联测试上 `#[allow(dead_code)]`

### Batch 2 — 死代码移除（低风险）

- [ ] **C20** 跨 crate — 移除 Provider `Model` 死类型 (~80 行，被 `resolve_model_list()` 绕过)
- [ ] **M48** `registry.rs:89-97` — 移除 `ProviderRegistry::model()` + `Route::model()`（仅测试可达）
- [ ] **M49** `registry.rs:108-128` — 移除 `detect_from_url()` + `url` 依赖（仅测试可达 + 死依赖）
- [ ] **P2-M02** `xai-grok-sampler/Cargo.toml:10` — 移除未使用的 `xai-grok-provider` 依赖
- [ ] **P3-M04** `xai-grok-sampler/Cargo.toml:10` — 同 P2-M02，P3-M04 曾被误标 `-Closed`，一并关闭

### Batch 3 — `#[non_exhaustive]` 保护（低风险，机械操作）

- [ ] **M55** `sampler/src/events.rs:18,28,121,152` — 4 类型加 `#[non_exhaustive]`
- [ ] **M56** `sampler/src/config.rs:49,186,207` — 3 类型加 `#[non_exhaustive]`
- [ ] **M57** `sampler/src/metrics.rs:32` — `InferenceLatencyStats` 加 `#[non_exhaustive]`
- [ ] **M58** `sampler/src/types.rs:13` — `RequestId` 加 `#[non_exhaustive]`
- [ ] **M59** `sampler/src/sampling_log.rs:10` — `AuthInfo` 加 `#[non_exhaustive]`
- [ ] **M60** `sampler/src/attribution.rs:39` — `SamplingConsumer` 加 `#[non_exhaustive]`

### Batch 4 — `serde` 属性补全（低风险）

- [ ] **M54** `events.rs:113` — `Usage` Option 字段加 `#[serde(default, skip_serializing_if = "Option::is_none")]`
- [ ] **S12** 多文件 — `ProviderModelDef`、`ProviderConfig`、`ProviderTomlEntry` 等 Option 字段加 serde 属性

### Batch 5 — `.expect()`/`.unwrap()` 消除（中风险，可能暴露错误路径）

- [ ] **M28** `auth_method.rs:280` — `push_interactive_login()` 中 `.expect()` 替换
- [ ] **M29+M53** `registry.rs:37,45,54,61,68,76,102` — 7 处 `RwLock` `.expect("lock poisoned")` 替换
- [ ] **M50** `endpoint.rs:55-56` — `Url::parse("http://localhost/").unwrap()` 替换
- [ ] **M51** 6 provider `configure()` 中 10 处 `NonZeroU64::new(n).unwrap()` 替换
- [ ] **M52** `types.rs:97` — `ProviderDefaults::default()` 中 `.expect()` 替换

### Batch 6 — Provider 模型修正（中风险，功能影响）

- [ ] **M34+M39** `providers/openai.rs:32-53` — 补充缺的 4 模型（`o1`, `o3-mini`, `gpt-4.1`, `gpt-4.1-mini`）+ 双 Route
- [ ] **M65** `providers/opencode.rs:68-71` — auth 链顺序改为 `PublicKey → EnvVar`（Arch §5.4）
- [ ] **C19** 跨 crate — `ApiBackend`/`AuthScheme` 枚举去重（当前两处定义）
- [ ] **P5-M04** `providers/mod.rs:30-38` — `register_from_config()` 对未知 provider ID 添加警告

### Batch 7 — 注释 + 测试规范（低风险）

- [ ] **C23+S18** `providers/mod.rs:48-255` — `detect_env_vars()` 体内 `#[cfg(test)]` 移至模块级
- [ ] **M63+S19** 多文件 — 删除违反 AGENTS.md §3.1 的 6 处内联注释
- [ ] **S02+S14** 多文件 — 补充 `pub` 项 `///` 文档注释（覆盖面评估后增量补齐）

### Batch 8 — Auth 栈对齐（高风险，需深入理解）

- [ ] **C21** 跨 crate — 三套认证抽象栈统一（`AuthFn`/`HttpAuth`/`AuthManager`）
- [ ] **C25** `auth.rs:14`, `framing.rs:8-9`, `protocol.rs` — 底层 trait 从 `String` 错误转为 `thiserror` 类型
- [ ] **M36+M37+M45** — `AuthInput.request` 类型修正 + `Route::with` auth 透传 + `resolve_model_to_sampling_config()` 调用 `route.auth.apply()`
- [ ] **P3-S01～S09** — 9 项 Auth 建议项逐步清理

### Batch 9 — Protocol 分发架构（最高风险，依赖 Batch 8）

- [ ] **C10** `route.rs:10-14` — `RouteDefaults` 增加 `generation`/`limits` 字段
- [ ] **C11+P2-C03+M40** — `ProtocolTable` 存储 `HashMap<ProtocolId, Protocol>` 值结构体
- [ ] **P2-C02+M43+M44** — `request_task.rs` dispatch 改为 `Protocol::stream.step()` 分发
- [ ] **P2-M03+P2-S02** — `protocol_id` 字段类型从 `String` 统一为 `ProtocolId`

### 不在计划中的项目

| 条目 | 原因 |
|------|------|
| C17 | 已 `-Deferred`，等待 auth 栈统一 |
| C24 | `anyhow::Result` 在 main.rs 广泛使用，单项替换无意义 |
| M64 | 单文件过大，纯风格，无功能价值 |
| P5-C01/C02/M01/M02/M03/M46 | TUI Providers Modal 当前无消费者 |
| S15–S45 | 纯建议项（pub 可见性、硬编码常量、命名提炼等） |

### `-Fixed` 待确认关闭项

以下条目已修复，需用户再审确认后关闭：

- S02, S04, S05, S07, S08, S09, S10
- P2-C01, P2-M01, P2-S01
- P4-C01, P4-C02, P4-C03, P4-M01, P4-M02, P4-M03, P4-M04
- C06, C07, C08
- M22, M23, M24, M25, M26, M27, M30, M31, M32, M33, M35, M38
- C12, C13, C14, C15, C16, C18

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

- **P4-C01** `xai.rs:64-66` — xAI auth 链缺少 `SessionToken(OAuth)` 回退。`grok login` 建立的 OAuth 会话令牌无法通过新 provider 系统解析，破坏向后兼容 -Fixed
- **P4-C02** `xai.rs:32` — xAI `known_models` 为空 (Arch §5.1 要求 `grok-build`) -Fixed
- **P4-C03** `anthropic.rs:33` — Anthropic `known_models` 为空 (Arch §5.3 要求 `claude-sonnet-4-20250514`, `claude-haiku-3-5-20241022`) -Fixed

### 中等

- **P4-M01** `xai.rs:31` — xAI provider 缺少 `x-grok-*` 头部注入 (Arch §5.1 + 实施计划 4.2) -Fixed
- **P4-M02** `openai.rs:86-98` — OpenAI 缺少双协议支持 (Arch §5.2 + 实施计划 4.3 要求 Chat + Responses 双 Route) -Fixed
- **P4-M03** `opencode.rs:63-65` — OpenCode 免费层公共回退缺失 (Arch §5.4 要求 `PublicKey("public")` 回退) -Fixed
- **P4-M04** `anthropic.rs:15,70` — `anthropic-version` 头部在 `ProviderDefaults.extra_headers` 和 `RouteDefaults.headers` 中重复 -Fixed

### 建议

- **P4-S01** 全局 provider — `pub` 结构体缺少 `///` 文档注释 (AGENTS.md §3.3)
- **P4-S02** 全局 provider — `defaults()` 函数可见性不一致 (`pub`/`pub(crate)`/私有混合)
- **P4-S03** `anthropic.rs:13` — `use indexmap::IndexMap` 局部导入而非模块顶部导入
- **P4-S04** `openai_compatible.rs:13` — `#[allow(dead_code)]` 在 `profile_base_url()` 上
- **P4-S05** 全局 provider — `configure()` 中 `model` 闭包可简化为 `route.model(id)`
- **P4-S06** 全局 provider — 常量 `NonZeroU64::new(x).unwrap()` 应使用 `.expect()`
- **P4-S07** `xai.rs:69` — 路由 ID `"xai-responses"` 硬编码，未来扩展不便

---

## Phase 5 审计: 2026-07-16

**范围**: `xai-grok-pager/src/cli.rs`, `slash/commands/providers.rs`, `views/providers_modal.rs`, `xai-grok-provider/src/config.rs`, `providers/mod.rs`, `xai-grok-pager-bin/src/main.rs`, `app/actions.rs`, `dispatch/router.rs`
**参考**: `docs/implementation-plan.md` Phase 5, `docs/model-adapter-architecture.md` §7

### 严重

- **P5-C01** `views/providers_modal.rs` — `ActiveModal::Providers` 变体未注册到 `ActiveModal` 枚举，模态框无法打开
- **P5-C02** `main.rs:904-910` — `ProviderRegistry` 仅在 `run_agent_command()` 中初始化，交互式 TUI 路径 (`async_main`) 未创建

### 中等

- **P5-M01** `dispatch/router.rs:594-597` — `Action::OpenProviders` 是 `TODO` 存根，`/providers` 静默无反应
- **P5-M02** `views/providers_modal.rs` — 模态框未接入输入处理和分派系统（无键盘处理、无 `update_modal_state`）
- **P5-M03** `views/providers_modal.rs:52-58` — Provider 列表硬编码，未使用 `ProviderRegistry` 实时数据
- **P5-M04** `providers/mod.rs:30-38` — `register_from_config()` 对未知 Provider ID 静默忽略，无警告
- **P5-M05** `main.rs:1-7` — 顶层 `#![allow(unused_imports, ...)]` 屏蔽了所有合法诊断
- **P5-M06** `client.rs:547-549` — `#[deprecated]` 的 `api_backend()` 在内部仍被调用

### 建议

- **P5-S01** `providers_modal.rs:48-49` — `let _ = registry/footer;` 标记未完成的实现
- **P5-S02** `providers_modal.rs:32-34` — 快捷键列表仅含 `Esc`，缺少 `Add/Configure/Remove/Test`
- **P5-S03** `commands/providers.rs:43-76` — `suggest_args()` 硬编码 Provider ID 而非查询 `AppCtx`
- **P5-S04** `commands/providers.rs:43` — `_query` 参数未用于过滤补全列表
- **P5-S05** `providers/mod.rs:31` — `env_key` 空向量或空字符串可能误报"已配置"
- **P5-S06** `config.rs:33-34` — `parse_provider_toml()` 通过字符串序列化/反序列化往返，效率低
- **P5-S07** `cli.rs:507-514` — `provider`/`api_key`/`base_url` 被解析但永不消费

---

## 审计: 2026-07-16 (Phase 5+6 全面审计)

**范围**: Phase 1-6 所有改动文件，跨 4 个子代理并行审计
**参考**: `docs/model-adapter-architecture.md`, `docs/implementation-plan.md`, `AGENTS.md`

### 严重

- **C06** `views/providers_modal.rs:73-124` — `builtin_providers()` 返回硬编码静态数据，从不查询 `ProviderRegistry` 获取实时配置/凭据状态/模型计数。用户通过 `[provider.*]` TOML 配置的 API key 不显示在 UI 中 -Fixed

- **C07** `xai-grok-pager-bin/src/main.rs:929,946` — 双向配置路径冲突：`[endpoints]` 后向兼容块 (line 946) 在 `configure_providers()` (line 929) 之后重新配置 xAI provider。若用户同时配置 `[provider.xai]` 和 `[endpoints]`，后者静默覆盖前者，违反 Arch §8 优先级（新配置 > 旧配置） -Fixed

- **C08** `xai-grok-shell/src/agent/config.rs:3365` — Provider 模型键使用 `"provider/model"` 格式（如 `"xai/grok-build"`），与内置 xAI 模型条目 `"grok-build"` 重复。`find_model_by_id()` 的 slug 回退可缓解，但拾取器显示两个重复条目 `grok-build` 和 `xai/grok-build` -Fixed

### 中等

- **M22** `views/providers_modal.rs:408,420,425,438,452,465,478,483` — 8 处注释违反 AGENTS.md §3.1 "不要添加注释，代码应是自解释的" -Fixed

- **M23** `views/providers_modal.rs:127-130,191,249` — `ProvidersKeyOutcome` 缺少 `Unchanged` 变体；`_ => Changed` 导致未处理按键时仍触发重渲染 -Fixed

- **M24** `views/providers_modal.rs:253` — `adjust_scroll()` 硬编码 `8` 为可见行数，应为具名常量 -Fixed

- **M25** `views/settings_modal.rs:5661-5752` — 测试 `rows_contain_categories_and_settings_through_pr_14` 的期望行列表缺少 `"providers"` 条目（介于 `"plan_mode"` 和 `"coding_data_sharing"` 之间），测试将失败 -Fixed

- **M26** 多文件 — 14+ 处 `NonZeroU64::new(n).unwrap()` 在生产代码中 — 全部已替换为 `.expect("n is non-zero")` -Fixed

- **M27** `xai-grok-shell/src/auth/provider_adapter.rs:20` — `pub fn new` 缺少文档注释，违反 AGENTS.md §3.3 -Fixed

- **M28** `xai-grok-shell/src/agent/auth_method.rs:280` — `.expect()` 在生产代码 `push_interactive_login()` 中，违反 AGENTS.md §3.6

- **M29** `xai-grok-provider/src/registry.rs:37,45,54,61,68,76,102` — 7 处 `RwLock` 上的 `.expect("lock poisoned")` 在生产代码中，AGENTS.md §3.6 禁止 (`?` 或 `.context()`)

- **M30** `views/providers_modal.rs:23-27` — `ProvidersModalState` 全部 4 个字段为 `pub`，AGENTS.md §3.3 要求默认私有。仅 `window` 需在 `modals.rs` 访问 -Fixed

- **M31** `xai-grok-provider/src/types.rs:111` + `auth.rs:3` — 重复 `pub type HeaderMap` 定义，导出时造成命名冲突 -Fixed

### 建议

- **S12** 多文件 — `ProviderModelDef`、`ProviderConfig`、`ProviderTomlEntry`、`ModelDefaults`、`ModelLimits`、`GenerationOptions`、`HttpOptions`、`Usage`、`RouteInput` 等 struct 的 `Option` 字段缺少 `#[serde(default, skip_serializing_if = "Option::is_none")]`

- **S13** 多文件 — `ProviderModelDef`、`ProviderDefaults`、`RouteInput`、`ProviderError` 已添加 `#[non_exhaustive]`；余下 8 个类型（`Route`、`Endpoint`、`ProtocolBody`、`ProtocolStream`、`Protocol`、`ProviderConfig`、`ProviderTomlEntry`、`ConfiguredProvider`）待评估跨 crate 构造影响 -Fixed

- **S14** 多文件 — `ProviderModelDef`、`ProviderDefaults`、`ConfiguredProvider`、`Provider` trait、`ProviderRegistry`、`Endpoint`、`EndpointPart`、`Framing` trait、`SseFraming`、`AuthFn` trait、`NoopAuth`、`Credential`、`ProtocolStream`、`Protocol`、`ProviderResult` 等 20+ pub 项缺少 `///` 文档注释

- **S15** `views/providers_modal.rs:253` — 同 M24，建议添加 `const VISIBLE_ROWS: usize = 8`

- **S16** `views/settings_modal.rs:4544,4555,4915` — 字符串 `"providers"` 在三处硬编码拦截点重复，提取为 `const PROVIDERS_SETTING_KEY: &str` 可避免偏移

- **S17** `xai-grok-shell/src/agent/config.rs:3423` — `api_base_url: None` 硬编码；若未来 provider 需要区分 API-key 与 session 认证端点，需从 `ProviderDefaults` 传递 `api_base_url`

- **S18** `xai-grok-provider/src/providers/mod.rs:47-248` — `#[cfg(test)] mod tests` 嵌入在 `detect_env_vars()` 函数体内，不易读。应移至模块级别

- **S19** `xai-grok-provider/src/providers/anthropic.rs:83`, `opencode.rs:62`, `ollama.rs:61` — 生产代码中的注释违反 AGENTS.md §3.1

### 遗漏补录 (此前未写入)

- **C09** `xai-grok-shell/src/agent/config.rs:3423` + Arch §8 — 7 个 xAI 特性门控中仅 `XAI_API_KEY` 回退已实现（config.rs:4441）。缺项已验证：OAuth 刷新跳过 (`is_xai_auth()`)、`x-grok-*` 头部 (`inject_url_derived_headers`)、doom-loop (无害头部)、URL 派生头部 (URL 门控)—均已实现。实际剩余缺口 `XAI_API_KEY` 回退已在 Phase 6.4.2 修复 -Closed

- **C10** 全局 — `RouteDefaults` 仅含 `headers: Option<HeaderMap>`（`route.rs:10-14`），Arch §3.4 定义 `RouteDefaultsInput` 含 `generation`/`limits`/`headers` 三字段。OpenAI 示例（Arch §10 line 967-973）传 `defaults: Some(RouteDefaultsInput { generation: Some(...), ... })` 编译失败。Route 层无法传递生成参数。需要架构变更，推迟到 Phase 7

- **C11** 全局 — Arch §8 的 `ProtocolTable` 在 `xai-grok-sampler` 中仅含 18 行 ID 常量 + 映射函数，无 `Protocol<Body,Frame,Event,State>` 值结构体。`xai-grok-provider/src/protocol.rs:66-88` 的 `ProtocolTable` 仅存字符串 ID，从未被实际协议值填充。需要架构变更，推迟到 Phase 7

- **M32** `xai-grok-provider/src/endpoint.rs:54` — `Url::parse("http://localhost/").unwrap()` 在 `Endpoint::default_base_url()` 中（生产代码），违反 AGENTS.md §3.6 -Fixed

- **M33** `xai-grok-provider/src/providers/openai_compatible.rs:13` — `#[allow(dead_code)]` 在 `pub fn profile_base_url()` 上，函数已降为 `pub(crate)` + 保留 `#[allow(dead_code)]`（工具性函数，供未来使用） -Fixed

- **M34** 全局 — Arch §5.2 要求 OpenAI 支持 ChatCompletions + Responses 双 Route，当前实现仅单 Route。Arch §5.4 要求 OpenCode 动态获取模型列表（`https://opencode.ai/zen/v1/models`），当前 `known_models` 为空

- **M35** `xai-grok-provider/src/providers/mod.rs:34` — `detect_env_vars()` 每次迭代克隆 `Vec<String>`，应改为引用 `&[String]` -Fixed

- **M36** `xai-grok-provider/src/auth.rs:7-13` — `AuthInput` 的 `request: String`，Arch §3.9 要求 `request: &LLMRequest`。认证系统无法检查结构化请求（model/messages），阻碍上下文感知的凭据解析

- **M37** `xai-grok-provider/src/route.rs:79-91` — `Route::with()` 的 `RoutePatch` 中 `auth` 字段在 patching 时被静默丢弃（`Self { ...self, ...patch }` 不含 `auth`）

- **M38** PROGRESS.md — Phase 6 的子任务状态已同步 -Fixed

- **S20** `xai-grok-provider/src/providers/openai_compatible.rs:13` — 同 M33，`profile_base_url` 死代码

- **S21** `xai-grok-provider/src/providers/mod.rs:48` — `#[allow(dead_code)]` 在 `mod tests` 上掩盖了真正的死代码问题

- **S22** `xai-grok-provider/src/providers/xai.rs:79-80` — `x-grok-client-identifier` 注入 `RouteDefaults.headers`，但 Arch 指定 `ProviderDefaults.extra_headers`。若任何代码读取 `extra_headers` 期望找到 x-grok 头部，则不会找到

- **S23** `xai-grok-shell/src/agent/config.rs:3330,3343` — 内部函数 `to_api_backend`/`to_auth_scheme` 缺少文档注释

---

## 全面审计: 2026-07-16 (4 子代理并行)

**范围**: `xai-grok-provider/src/` (全模块) + `xai-grok-sampler/src/` (protocol/config/client/request_task/events) + `xai-grok-shell/src/` (agent/config.rs, auth/) + `xai-grok-pager/src/` (views/providers_modal, modals, cli) + `xai-grok-pager-bin/src/main.rs`

**方法**: 4 子代理并行扫描 — (1) 架构一致性, (2) AGENTS.md 代码风格, (3) 实施计划吻合度, (4) 跨 crate 一致性。合并去重后归集。

**未重开已关闭条目**: 下列条目与现有 ISSUE.md 已 `-Closed` 或 `-Fixed` 的旧条目重复，不再重新编号。仅列出交叉引用供追溯：

| 新发现 | 现有引用 | 状态 |
|--------|----------|------|
| Provider `.unwrap()` (10 处) | P4-S06, M26 | P4-S06 开放中 (S 级), M26 已 -Fixed |
| Provider 中非 doc 注释 (6 文件) | P4-S01 | 覆盖 doc comment 规则，不覆盖 inline 注释 |
| `Route::with` 丢弃 `patch.auth` | M37 | M37 开放中 (M 级) |
| `RouteDefaultsInput` 缺失 | C10 | C10 开放中 (C 级) |
| `ProtocolTable` 存储空字符串 | C11 (P2-C03, P2-M04) | C11 开放中 (C 级) |
| Phase 2 `stream/` 未删除 | P2-S03, 7.1 | P2-S03 开放中 (S 级) |
| Phase 2 dispatch 仍 match 分支 | P2-C02 | P2-C02 开放中 (C 级) |
| `AuthInput.request` 类型不符 | M36 | M36 开放中 (M 级) |
| `Endpoint::default_base_url` 硬编码 | S04 (原始 S04) | 原始 S04 已 -Fixed |
| `shell/src/auth/credential_provider.rs` 未适配 `AuthFn` | P3-S06 | P3-S06 开放中 (S 级) |
| `xai-grok-sampler` 依赖 `xai-grok-provider` 未使用 | P3-M04 | P3-M04 错误标注 -Closed (问题仍存在) |
| OpenCode `known_models` 为空 | P4-M03 | P4-M03 已 -Fixed (修复的是 public key fallback) |
| 跨 crate `pub` 字段可见性 | S01-S14 (原始) | 多数已 -Closed 作为 Phase 1 设计约定 |
| `xai.rs` 缺少 `x-grok-*` 头部 (provider 层) | P4-M01 | P4-M01 已 -Fixed |
| `ProtocolBody`/`ProtocolStream` 错误类型为 String | P3-S01 | P3-S01 开放中 (S 级) |
| 提供者 `detect_env_vars` 中测试嵌入函数体 | M38 | M38 已 -Fixed |
| `Allow(dead_code)` 在 `profile_base_url` 上 | P4-S04, M33, S20 | P4-S04 开放中, M33 已 -Fixed, S20 开放中 |

---

### 严重 (C) — 架构契约破坏 / 功能必损

- **C12** `xai-grok-provider/src/route.rs:48-49` — `Route` 使用 `Arc<dyn AuthFn>` 和 `Arc<dyn Framing>` 而非 Arch §3.4 要求的 `Box<dyn ...>`. `Arc` 表示共享所有权 (可在 `Route::with` 的 `Clone` 中重用), 但 Arch 签约为 `Box` 表示所有权转移。实际行为兼容, 但接口契约偏离 -Fixed

- **C13** `xai-grok-provider/src/registry.rs:13-16` — `ProviderRegistry` 包含 Arch §4 未指定的 `configs: RwLock<HashMap<ProviderId, ProviderConfig>>` 字段及 `store_config()`/`get_config()`/`register_route()`/`get_route()` 四个额外方法。功能上必要, 但公开 API 面与文档差异 -Fixed

- **C14** `xai-grok-provider/src/providers/openai.rs:32-53` — 仅定义 2 个已知模型 (`gpt-4o`, `gpt-4o-mini`), Arch §5.2 要求 6 个 (`gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini`, `gpt-4.1`, `gpt-4.1-mini`)。缺少 4 个模型 -Fixed

- **C15** `xai-grok-provider/src/providers/ollama.rs:19` — `auth_scheme: AuthScheme::Bearer`. Arch §5.5 要求 `None`。Ollama 不需要认证, 声明 Bearer scheme 会导致上游代码添加空的 `Authorization: Bearer` 头 -Fixed

- **C16** `xai-grok-provider/src/providers/ollama.rs:31` — `known_models: vec![]`. Arch §5.5 至少列出 3 个示例模型 (`llama3.1`, `codellama`, `deepseek-coder`) -Fixed

- **C17** `xai-grok-provider/src/auth.rs:5-11` — `AuthInput` 所有字段为 owned `String` (request, method, url, body), Arch §3.9 要求引用类型 (`&LLMRequest`, `&str`)。认证系统无法检查结构化请求, 阻碍上下文感知凭据解析 (同 M36) -Deferred (see audit C17 report: route.auth.apply() never called in production; to be bundled with M45/C19/C21 auth stack unification)

- **C18** `crates/codegen/xai-grok-workspace/src/handle.rs:1649,1714` + `crates/codegen/xai-grok-tools/src/util/fs.rs:22,56` — 使用 `tokio::fs::canonicalize`, 违反 AGENTS.md §3.6 禁止模式。需替换为 `spawn_blocking + dunce::canonicalize` -Fixed

- **C19** 跨 crate — `ApiBackend` 枚举在 `xai-grok-provider/src/types.rs:37-42` 和 `xai-grok-sampling-types/src/types.rs:1013-1021` 重复定义。`AuthScheme` 枚举在 `xai-grok-provider/src/types.rs:47-51` 和 `xai-grok-sampler/src/config.rs:20-24` 重复定义。添加变体须同步更新两处, 编译器无帮助。shell 靠 `to_api_backend()`/`to_auth_scheme()` 手工桥接 (config.rs:3337-3353)

- **C20** 跨 crate — `xai-grok-provider` 的 `Model` 类型 (model.rs:9-14) 被整个解析链绕过。`resolve_model_list()` → `provider_known_models()` 直接构造 shell 的 `ModelEntry`, 从不调用 `ProviderRegistry::model()`、`Route::model()` 或使用 provider 的 `Model`/`ModelDefaults`/`ModelLimits`/`GenerationOptions`/`HttpOptions`。约 80 行生产代码 + 测试代码处于死状态

- **C21** 跨 crate — 三套重叠的认证抽象栈: (1) `xai-grok-provider/src/auth.rs` 的 `AuthFn`/`Credential`, (2) `xai-grok-auth` 的 `HttpAuth`/`AuthCredentialProvider`, (3) `xai-grok-shell/src/auth/` 的 `AuthManager`/`ShellAuthCredentialProvider`。桥接器 `AuthManagerAsAuthFn` 存在但从未在真实请求路径中被调用 — sampler 直接使用 `SamplerConfig.api_key`。provider 的 `Credential` 枚举 (5 变体含 `resolve()`/`or_else()`) 仅为测试代码可达

- **C22** `xai-grok-pager-bin/src/main.rs:1` — `#![allow(dead_code)]` 在 crate 级别, 全局屏蔽死代码诊断, 掩盖所有不可达路径

- **C23** `xai-grok-provider/src/providers/mod.rs:48-255` — `#[cfg(test)] mod tests` 嵌入在 `detect_env_vars()` 函数体内 (for 循环之后、return 之前)。严重违反常规代码组织, 测试隔离及可维护性风险

- **C24** 跨 crate — `xai-grok-pager-bin/src/main.rs:22` 使用 `use anyhow::Result;`, AGENTS.md §3.2 要求自定义 `thiserror` 错误类型

- **C25** `xai-grok-provider/src/auth.rs:14`, `framing.rs:8-9`, `protocol.rs` — `AuthFn::apply` 返回 `Result<HeaderMap, String>`, `Framing::frame` stream item 错误为 `String`, `ProtocolBody::from` 和 `ProtocolStream::step` 错误类型亦为 `String`。所有底层 trait 使用裸 String 而非 `thiserror` 定义的自定义类型

### 中等 (M) — 功能重要偏差

- **M39** `xai-grok-provider/src/providers/openai.rs:32-53` — `known_models` 返回 2 个模型 (`gpt-4o`, `gpt-4o-mini`), Arch §5.2 实际列出 6 个。缺少 `o1`, `o3-mini`, `gpt-4.1`, `gpt-4.1-mini`

- **M40** `xai-grok-sampler/src/protocol.rs` — 文件不存在。Arch §13 和 Phase 2 要求的新 `protocol.rs` (包含 `ProtocolTable: HashMap<ProtocolId, Protocol>`) 从未创建。仅有 `protocols/mod.rs` 存放 ID 常量

- **M41** `xai-grok-sampler/src/protocols/` — 缺少 Phase 2 要求的 `chat_completions.rs`/`responses.rs`/`messages.rs` 协议值文件。`stream/` 目录 (6 文件) 仍存在且活跃引用。Phase 2 实质未完成

- **M42** `xai-grok-workspace/src/session/acp_session_impl/` — `SessionActor` 无 `provider_registry: Option<Arc<ProviderRegistry>>` 字段。Phase 6.2 要求 `ProviderRegistry` 注入 Session Actor, 未实现

- **M43** `xai-grok-sampler/src/client.rs:546-551` — `SamplingClient::protocol_id()` 使用 `match protocol_id` 字符串分支, 而非 Phase 2 要求的 `ProtocolTable.get()` 查找

- **M44** `xai-grok-sampler/src/actor/request_task.rs:429-473` — dispatch 改为了 `match client.protocol_id()` 但仍调用旧 `stream_*()` 模块函数, 未使用 `Protocol::stream.step()`

- **M45** `xai-grok-shell/src/agent/config.rs:4831-4851` — `resolve_model_to_sampling_config()` 使用独立 `resolve_credentials()` + `ModelEntry` 字段组装 `SamplerConfig`, 未调用 `route.auth.apply()`. Route 的认证函数被完全绕过

- **M46** `xai-grok-pager/src/views/providers_modal.rs:75-165` — `builtin_providers()` 硬编码静态 provider 元数据 (6 个提供者的状态、key 名、endpoint), 从不查询 `ProviderRegistry`。新增 provider 到 `register_all()` 后 TUI 不感知

- **M47** `xai-grok-shell/src/agent/config.rs:3124-3266` — 模型解析优先级与 Arch §6.1 不同: Arch 为 `[model.*] > prefetched > [provider.*] > built-in > defaults`, 实现为 `defaults → prefetched (全替换) → provider (追加) → [model.*] (覆盖)`。provider 模型不能覆盖 prefetched, 且 prefetched 在全替换模式下丢失了 defaults

- **M48** `xai-grok-provider/src/registry.rs:89-97` — `ProviderRegistry::model()` 方法在 production 代码中未被任何 crate 调用 (仅测试可达)。连带 `Route::model()` (route.rs:97-107) 亦为死代码

- **M49** `xai-grok-provider/src/registry.rs:108-128` — `detect_from_url()` 在 production 代码中未被任何 crate 调用 (仅测试可达)。`url` 依赖 (Cargo.toml:17) 仅为该函数服务 — 这是一个死依赖

- **M50** `xai-grok-provider/src/endpoint.rs:55-56` — `Url::parse("http://localhost/").unwrap_or_else(|_| ...)` 在 production 代码中使用 `.expect()` 进行 URL 解析。硬编码字符串, 永远不应失败, 但仍违反 AGENTS.md §3.6

- **M51** `xai-grok-provider/src/providers/` — 6 个 `configure()` 方法中共计 10 处 `NonZeroU64::new(n).unwrap()`, 违反 AGENTS.md §3.2。包括: xai.rs:24,39; openai.rs:22,38,48; anthropic.rs:22,38,48; ollama.rs:21; opencode.rs:22; openai_compatible.rs:38

- **M52** `xai-grok-provider/src/types.rs:97` — `ProviderDefaults::default()` 中使用 `.expect("128_000 is non-zero")`, 违反 AGENTS.md §3.2

- **M53** `xai-grok-provider/src/registry.rs:37,45,54,61,68,76,102` — 7 处 `.expect("lock poisoned")` 在 `ProviderRegistry` 中, 违反 AGENTS.md §3.6 (优先 `?`/`.context()`)

- **M54** `xai-grok-provider/src/events.rs:113` — `Usage` (lines 112-122) 全 Option 字段缺少 `#[serde(default, skip_serializing_if = "Option::is_none")]`, 序列化含非必要 null 字段

- **M55** `xai-grok-sampler/src/events.rs:18,28,121,152` — `SamplingChannel`, `SamplingEvent`, `SamplingErrorInfo`, `SamplingErrorKind` 四个公共类型缺少 `#[non_exhaustive]`

- **M56** `xai-grok-sampler/src/config.rs:49,186,207` — `SamplerConfig`, `RetryPolicy`, `OriginClientInfo` 三个公共类型缺少 `#[non_exhaustive]`

- **M57** `xai-grok-sampler/src/metrics.rs:32` — `InferenceLatencyStats` 公共结构缺少 `#[non_exhaustive]`

- **M58** `xai-grok-sampler/src/types.rs:13` — `RequestId` 公共 newtype 缺少 `#[non_exhaustive]`

- **M59** `xai-grok-sampler/src/sampling_log.rs:10` — `AuthInfo` 公共结构缺少 `#[non_exhaustive]`

- **M60** `xai-grok-sampler/src/attribution.rs:39` — `SamplingConsumer` 公共枚举缺少 `#[non_exhaustive]`

- **M61** `xai-grok-provider/src/providers/openai_compatible.rs:15` — `pub(crate) fn profile_base_url()` 带有 `#[allow(dead_code)]`, 工具性函数被标记为死代码

- **M62** `xai-grok-provider/src/providers/mod.rs:49` — 内联测试模块上 `#[allow(dead_code)]`, 掩盖真正死代码

- **M63** 多文件 — 6 处 inline 注释违反 AGENTS.md §3.1 "不要添加注释": `config.rs:54`, `anthropic.rs:88`, `ollama.rs:66`, `mod.rs:272,277,282,307,308,311,318`, `openai_compatible.rs:80`, `opencode.rs:67`

- **M64** 跨 crate — `xai-grok-pager-bin/src/main.rs` (约 3000 行), `xai-grok-sampler/src/client.rs` (约 2500 行), `xai-grok-shell/src/agent/config.rs` (约 11400 行) — 单文件规模过大, 应拆分为职责分离的子模块

- **M65** `xai-grok-provider/src/providers/opencode.rs:68-71` — auth 链顺序为 `InlineKey → EnvVar("OPENCODE_API_KEY") → PublicKey("public")`, Arch §5.4 为 `PublicKey("public") → EnvVar("OPENCODE_API_KEY")`。实现更完整 (加了 inline key), 但顺序差异导致回退逻辑不同: 若有 inline key, Arch 要求的 public 回退会被跳过的行为不一致

### 建议 (S) — 编码约定 / 文档 / 可维护性

- **S24** `xai-grok-provider/src/types.rs:8,26` — `ProviderId(pub String)` 和 `ModelId(pub String)` newtype 公开内部字段, 违反 AGENTS.md §3.3 建议 (应私有 + getter)。虽为设计约定, 但列入审计供参考

- **S25** `xai-grok-provider/src/types.rs:118-123` — `LLMRequest struct` 全字段 `pub`, 违反 §3.3。大结构体 (5 字段) 需封装

- **S26** `xai-grok-provider/src/route.rs`, `endpoint.rs`, `config.rs`, `auth.rs`, `provider.rs`, `model.rs` — 全部 6 个结构体 (`Route`, `RouteInput`, `RoutePatch`, `RouteDefaults`, `Endpoint`, `EndpointInput`, `EndpointPatch`, `ProviderConfig`, `ProviderTomlEntry`, `AuthInput`, `ConfiguredProvider`, `Model`, `ModelDefaults`, `ModelLimits`, `GenerationOptions`, `HttpOptions`) 所有字段为 `pub`, 违反 §3.3

- **S27** `xai-grok-sampler/src/config.rs`, `events.rs`, `metrics.rs`, `sampling_log.rs` — `SamplerConfig`(131+ 字段), `SamplingErrorInfo`, `InferenceLatencyStats`, `AuthInfo` 全字段 `pub`, 违反 §3.3

- **S28** `xai-grok-provider/src/endpoint.rs:49` — 裸 URL 字符串 `"http://localhost"` 硬编码在 `unwrap_or` 中。应定义为具名常量

- **S29** `xai-grok-provider/src/protocol.rs:65-88` — `ProtocolTable` 存储 `HashMap<ProtocolId, String>` 其值为空字符串, 非 `HashMap<ProtocolId, Protocol>`. 结构存在但不含实际协议值 (同 C11/P2-C03)

- **S30** `xai-grok-provider/src/lib.rs` — 13 个 `pub mod` 中多数外部无消费者: `endpoint`, `error`, `events`, `framing`, `model`, `protocol`, `route`, `provider` 均未被其它 crate 引用。巨大的公开 API 面无人使用

- **S31** `xai-grok-provider/src/providers/openai_compatible.rs:13` — 同 M61, `profile_base_url` 处 `#[allow(dead_code)]`

- **S32** `xai-grok-provider/src/providers/anthropic.rs:83`, `opencode.rs:62`, `ollama.rs:61` — 路由 ID 字符串 `"anthropic-messages"`, `"opencode-chat"`, `"ollama-chat"` 硬编码。应引用常量

- **S33** `xai-grok-provider/src/route.rs:133` (test), `providers/mod.rs:95` (test), `xai-grok-sampler/src/client.rs:2197` (test) — 协议字符串 `"chat_completions"` 以原始字面量散落在至少 6 位置, 缺少规范来源

- **S34** `xai-grok-provider/src/providers/xai.rs:79-80` — `x-grok-client-identifier` 注入 `RouteDefaults.headers`, 但 Arch §5.1 指定 `ProviderDefaults.extra_headers`。若任何代码读 `extra_headers` 去找 x-grok 头部则找不到

- **S35** 跨 crate — `SamplerConfig.protocol_id` (sampler config.rs:60) 与 `ProviderConfig.api_backend` (types.rs:37) 的隐式 fallthrough `SamplingClient::protocol_id()` (client.rs:546) 逻辑对序列化消费者不可见。`protocol_id: null` 在序列化输出中不透明

- **S36** `xai-grok-provider/src/auth.rs:221,228` — 测试中使用 `unsafe` 修改 `std::env`, 虽然注释了 `SAFETY:`, 但仍是不安全模式

- **S37** `xai-grok-provider/src/auth.rs:17-25` — `or_else`/`and_then` 为 `impl dyn AuthFn` 方法而非 trait 方法, 分派机制与 Arch §3.9 路径不同

- **S38** `xai-grok-provider/src/providers/xai.rs` — Arch §5.1 属性表列出了 `Raw Tools: x_search` 和 `Doom Loop: Enabled`, 但 `ProviderDefaults` 中无对应字段追踪 (可能上层隐式处理, 但架构文档有记录)

- **S39** `xai-grok-provider/src/route.rs:83-95` — `Route::with()` 中 `patch.auth` 被静默丢弃 (同 M37)。补丁中声明了但不生效

- **S40** 跨 crate — `ConfiguredProvider.configure` (provider.rs:12) 字段类型为 `fn(ProviderConfig) -> ConfiguredProvider`, 与 `Provider::configure` 方法同签名。使用者不清楚调用哪一个, 形成循环 API

- **S41** `xai-grok-pager/src/slash/commands/providers.rs:43-76` — `suggest_args()` 硬编码 Provider ID 列表而非查询 `ProviderRegistry`

- **S42** `xai-grok-pager/src/views/providers_modal.rs:253` — `adjust_scroll()` 硬编码 `8` 为可见行数, 应提取为具名常量

- **S43** `xai-grok-provider/src/providers/xai.rs:69` — 路由 ID `"xai-responses"` 硬编码

- **S44** `xai-grok-provider/src/auth.rs:112` — `Credential::config()` 接受 `&str` 后 `.to_owned()`, 可改 `impl Into<String>` (AGENTS.md §3.6 允许但优先 `&str`)

- **S45** `xai-grok-pager/src/settings/defs.rs` — Arch §7.6 要求 F2 → Settings → Providers 入口, 未实现。`settings/defs.rs` 中无 `"providers"` 设置项类别
