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

## Phase 5: TUI 集成 + Provider 启动初始化 — 2026-07-16 (补完)

### 子任务状态

| ID | 任务 | 状态 |
|----|------|------|
| 5.1 | `[provider.*]` TOML 解析 | ✅ 完成 |
| 5.2 | CLI `--provider` / `--api-key` / `--base-url` | ✅ 完成 |
| 5.3 | `--model provider/model` 格式 | ✅ 完成 |
| 5.4 | startup Provider 初始化 + 6 层配置合并 + env 检测 | ✅ 完成 |
| 5.5 | `resolve_model_list()` 重写 (Provider 层) | ❌ 未开始 |
| 5.6 | TUI Providers 模态框 (完整) | ⚠️ Stub 仅变体/action/命令就绪，渲染/交互未实现 |
| 5.7 | `/providers` 斜杠命令 | ✅ 完成 |
| 5.8 | 模型选择器 provider/ 前缀 | ⚠️ `parse_model_ref()` 支持 CLI，`Ctrl+M` 显示未扩展 |
| 5.9 | `[endpoints]` 向后兼容 → xAI Provider | ✅ 完成 |
| 5.10 | 环境变量自动检测 | ✅ 完成 |

### Phase 5.4 完成内容 (本次新增)
- `ProviderConfig::merge()` — 分层配置合并方法
- `detect_env_vars()` — 按 provider 的 `env_key` 列表检测环境变量
- `build_provider_config()` — 三源合并 (env → TOML → CLI)
- `configure_providers()` — 对所有注册 provider 应用合并配置并存储 route
- `register_from_config()` — 改为实际调用 `registry.configure()` (原来仅日志)
- `main.rs` — registry 不再 `_` 丢弃，线程化到 `AgentConfig.provider_registry`
- `Config.provider_registry` — 新增 `Option<Arc<ProviderRegistry>>` 字段

### Phase 5.9 完成内容
- 读取 `agent_config.endpoints.xai_api_base_url` / `alpha_test_key`
- 转换为 xAI provider 的 `ProviderConfig` 并配置到 registry
- 保留旧配置语义：自定义 base_url 或 key 自动映射

### Phase 5.10 完成内容
- `detect_env_vars()` 遍历各 provider 的 `env_key` 列表
- 支持：`XAI_API_KEY` → xAI, `OPENAI_API_KEY` → OpenAI, `ANTHROPIC_API_KEY` → Anthropic 等
- 优先级：env < `[provider.*]` < CLI `--api-key`

### 关键结果
- `cargo check -p xai-grok-pager-bin` ✅
- `cargo clippy -p xai-grok-provider` — 零警告
- `cargo test -p xai-grok-provider` — 58/58 ✅
- 修改 4 个文件，新增 ~170 行代码

### Phase 5.5 完成内容 (本次新增)
- `provider_known_models()` — 将 `ProviderDefaults` + `ProviderModelDef` 转换为 `ModelEntry`
- `resolve_model_list()` 新增 Layer 2b — 在预取模型之后、`[model.*]` 重写之前注入
- 转换函数 `to_api_backend()` / `to_auth_scheme()` 处理跨 crate 类型映射
- 注入的模型使用 `provider/model` 键名，避免与内置 xAI 模型冲突
- 每个模型携带 provider 的 `env_key` 列表，使 `resolve_credentials()` 回退到正确的环境变量
- 优先级链: 内置默认 > 预取 > **provider 模型** > `[model.*]` > 继承 > 全局默认

### 关键结果 (本轮追加)
- `cargo check -p xai-grok-shell` ✅
- `cargo test -p xai-grok-provider` — 58/58 ✅
- `cargo clippy -p xai-grok-provider` — 零警告
- 修改 4 个文件，新增 ~50 行代码

### 凭证集成 (Phase 5 追加)
- `ProviderRegistry.store_config()` / `get_config()` 存储解析后的 ProviderConfig (含 api_key)
- `configure_providers()` / `register_from_config()` 在调用 `configure()` 前存储配置
- `[endpoints]` → xAI 兼容路径也存储配置
- `provider_known_models()` 读取存储的配置以填充 `ModelEntry.api_key` 和合并 `env_key` 列表

### 待完成
- 5.6: Providers 模态框完整交互 (TUI)
- 5.8: 模型选择器 provider/model 显示 (TUI)

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

## Phase 6: 集成与迁移 — 2026-07-16

### 子任务状态

| ID | 任务 | 状态 |
|----|------|------|
| 6.1 | main.rs 初始化 ProviderRegistry | ✅ 已在 5.4 完成 |
| 6.2 | Registry 注入到 SessionActor | ❌ 未开始 — 需修改 spawn_session_actor 参数链 |
| 6.3 | Route-based protocol_id 贯通 | ✅ protocol_id 从 api_backend 派生占位 |
| 6.4 | xAI 特性门控 | ⚠️ 部分完成 — XAI_API_KEY 回退已门控 |
| 6.5-6.6 | 验证测试 | ❌ 待实施 |

### Phase 6.4.2 完成内容
- `resolve_credentials()` PRIORITY 3 添加 `is_first_party_xai_url()` 门控
- 非 xAI 端点不再错误接收 `XAI_API_KEY` 值
- 涉及文件: `config.rs`

### Phase 6.3 完成内容
- `sampling_config_for_model()` — `protocol_id` 从 `api_backend` 派生，不再硬编码 `None`
- `reconstruct_full_config()` — 同上
- `protocol_id` 现在始终反映正确的协议标识符
- 涉及文件: `config.rs`, `sampler_turn.rs`

### 关键结果
- `cargo check -p xai-grok-shell` ✅
- `cargo clippy -p xai-grok-provider` — 零警告
- 修改 2 个文件

### 待完成
- 6.2: Registry 线程化到 SessionActor
- 6.5-6.6: 集成/端到端测试

---

## Phase 7: 清理收尾 — 2026-07-16

### 完成内容
- 移除 `main.rs` 顶层的 `#![allow(unused_imports, unreachable_code, ...)]`
- `cargo audit` 尝试运行 (Windows 环境安装失败)

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
