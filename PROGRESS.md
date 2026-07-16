# Progress — Model Adapter Layer Refactoring

> 每次 Phase 完成后追加摘要。请勿修改已有条目。

---

## Phase 0: Dependency Analysis & Workspace Setup — 2026-07-16

### 完成内容
- **0.1** 创建 `crates/codegen/xai-grok-provider` crate 骨架（13 个模块文件）
- **0.2** 更新 workspace `Cargo.toml`：成员列表 + 依赖路径
- **0.3** 实现全部模块桩：`types.rs`、`auth.rs`、`config.rs`、`endpoint.rs`、`events.rs`、`framing.rs`、`model.rs`、`protocol.rs`、`provider.rs`、`registry.rs`、`route.rs`
- **0.4** 创建 `docs/` 三件套：架构书（1,123 行）、计划书（437 行）、AGENTS.md（170 行）
- **0.5** 创建 `PROGRESS.md` + `ISSUE.md`

### 关键结果
- `cargo check -p xai-grok-provider` — 通过
- `cargo clippy -p xai-grok-provider -- -D warnings` — 通过
- `cargo test -p xai-grok-provider` — 通过
- `cargo check --workspace` — protoc 缺失导致 `xai-grok-tools-api` 失败（Windows 已知限制，非回归）
- 新增文件 13 个，修改 `Cargo.toml` 2 处

### 阻塞项
- `xai-grok-tools-api` 需要 protoc，Windows 上需 `winget install Google.Protobuf`（已解决）
- `xai-grok-shell` 在 Windows 上有 tracing crate const eval bug（Rust 1.92.0，不影响开发）

---

## Phase 3: 认证层分离 — 2026-07-16

### 完成内容
- **3.1** `AuthFn` trait 完善：`BearerAuth`、`HeaderAuth`、`NoopAuth`、`FailAuth`、`ChainAuth`、`ThenAuth` 全部实现
- **3.2** `Credential` 解析引擎：`Inline`/`Config`/`Session`/`None` 四变体 + `or_else()` 链式回退
- **3.3** 创建 `AuthManagerAsAuthFn` 适配器 (`xai-grok-shell/src/auth/provider_adapter.rs`) — 将 `AuthManager` 包装为 `AuthFn`
- **3.4** 添加 `xai-grok-provider` 依赖到 `xai-grok-shell`
- **3.5** 修改 `SamplerConfig.protocol_id` 从 `String` 改为 `Option<String>` — 消除所有向后兼容问题
- 修复 `SamplerConfig` 构造器遗漏 `protocol_id` 字段的 5+ 处

---

## Phase 4: Provider 实现层 — 2026-07-16

### 完成内容
- **4.1** `providers/mod.rs:register_all()` — 注册全部 6 个内置 Provider
- **4.2** `XaiProvider` — Responses 协议, Bearer auth, XAI_API_KEY env, 500K context
- **4.3** `OpenAIProvider` — ChatCompletions, OPENAI_API_KEY, 2 个内置模型 (gpt-4o, gpt-4o-mini)
- **4.4** `AnthropicProvider` — Messages 协议, x-api-key auth, anthropic-version header
- **4.5** `OpenCodeProvider` — Zen 网关, ChatCompletions, OPENCODE_API_KEY
- **4.6** `OllamaProvider` — 无认证, localhost:11434
- **4.7** `OpenAiCompatibleProvider` — catch-all, OpenAI-compatible profiles

### 关键结果
- `cargo test -p xai-grok-provider` — 52/52
- `cargo clippy -p xai-grok-provider -- -D warnings` — 零警告
- 6 个新文件，~600 行代码

### 关键结果
- `cargo test -p xai-grok-provider` — 52/52 ✅
- `cargo test -p xai-grok-sampler` — 154/154 ✅
- 向后兼容：`protocol_id: None` 自动回退到 `api_backend` 派生值

---

## Phase 2: 协议层提取 — 2026-07-16

### 完成内容
- **2.0** 添加 `xai-grok-provider` 依赖到 `xai-grok-sampler`
- **2.1** 创建 `protocols/mod.rs` — 协议 ID 常量 + `api_backend_to_protocol_id()` 映射
- **2.5** 添加 `protocol_id: String` 到 `SamplerConfig`，默认从 `api_backend` 派生
- **2.6** 添加 `protocol_id()` 方法到 `SamplingClient`（含向后兼容回退）
- **2.7** 重写 `request_task.rs` dispatch — 从 `match api_backend` 改为 `match protocol_id`，添加未知协议回退

### 关键结果
- `cargo check -p xai-grok-sampler` — 通过
- `cargo clippy -p xai-grok-sampler -- -D warnings` — 零警告
- 修改 5 个文件，新增 `protocols/` 模块
- 向后兼容：`SamplingClient::api_backend()` 保留，`protocol_id` 在未设置时自动从 `api_backend` 派生

---

## Phase 1: 核心类型层 — 2026-07-16

### 完成内容
- **1.1** `ProviderId` — newtype + 6 个常量 + serde roundtrip
- **1.2** `ProviderDefaults` + `ProviderModelDef` — 完整字段，含默认值
- **1.3** `Endpoint<Body>` + `EndpointPart` + `EndpointInput` + `merge_endpoints` — 含 URL 渲染
- **1.4** `Framing` trait + `SseFraming` — SSE 解码，过滤 `[DONE]`
- **1.5** `Credential` + `AuthFn` trait + 链式回退（`or_else`）+ Bearer/Header/Noop 渲染
- **1.6** `Route` + `RouteInput` + `RoutePatch` — 不可变 `with()` 更新
- **1.7** `Protocol<B,F,E,S>` + `ProtocolBody` + `ProtocolStream` + `ProtocolTable`
- **1.8** `LLMEvent` + `Usage` + `FinishReason` + `ErrorKind` — 16 种事件变体
- **1.9** `Model` + `ModelDefaults` + `ModelLimits` + `GenerationOptions` + `HttpOptions`
- **1.10** `ProviderConfig` — serde 反序列化（JSON + TOML 兼容）
- **1.11** `Provider` trait + `ConfiguredProvider` + `ProviderRegistry`
- 测试覆盖：37 个单元测试覆盖所有核心类型

### 关键结果
- `cargo check -p xai-grok-provider` — 通过
- `cargo clippy -p xai-grok-provider -- -D warnings` — 零警告
- `cargo test -p xai-grok-provider` — 37/37 通过
- 新增 ~1,200 行 Rust 代码
