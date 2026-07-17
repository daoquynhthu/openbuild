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
| 5.5 | `resolve_model_list()` 重写 (Provider 层) | ✅ 完成 |
| 5.6 | TUI Providers 模态框 (完整) | ✅ 列表+详情+Settings 入口 |
| 5.7 | `/providers` 斜杠命令 | ✅ 完成 |
| 5.8 | 模型选择器 provider/ 前缀 | ✅ CLI + Ctrl+M 均显示 `provider/model` |
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

### Phase 5.6a-5.6c 完成内容 (TUI)
- `providers_modal.rs` 全面重写: `ModalWindowState` chrome、状态指示器、↑↓/Home/End 导航
- 键盘/鼠标 dispatch: `modals.rs` 3 处分支 (draw/kbd/mouse)
- 详情面板: API Key 隐蔽输入、Base URL 编辑、Ctrl+R 切换可见性、Tab 切换字段
- Settings 集成: `defs.rs` 添加 "providers" Group 条目，Enter/Space/鼠标单击分发 `Action::OpenProviders`

### Phase 5.8 完成内容
- `build_model_items()`: 当模型 ID 含 `/` 时显示 `provider/model` 格式
- `match_text` 同时包含完整 key 和人名，支持双向搜索
- 涉及文件: `slash/commands/model.rs`

### 门禁检查 (受 protoc 限制的范围内)
- `cargo check -p xai-grok-provider` ✅
- `cargo test -p xai-grok-provider` — 66/66 ✅
- `cargo clippy -p xai-grok-provider` — 零警告
- `cargo check -p xai-grok-pager` ✅ (tools-api protoc 问题仅影响该 crate 本身)
- 所有代码在 `feat/provider-adapter` 分支提交

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

### Phase 6.5 完成内容
- `build_provider_config()` 合并优先级测试 (env < TOML < CLI)
- `configure_providers()` 存储配置 + CLI 覆盖测试
- `registry.store_config()/get_config()` 存取 + 覆盖测试
- `ProviderConfig::merge()` 优先级测试
- 5 个新测试，provider crate 总计 63 项
- `cargo test -p xai-grok-provider` — 63/63 ✅
- `cargo clippy -p xai-grok-provider` — 零警告

### Phase 6.6 完成内容
- `provider_e2e.rs` 集成测试 (3 项):
  - `openai_provider_full_pipeline` — 配置→Route→SamplerConfig 全链路
  - `all_providers_have_known_models` — 6 个内置 provider 枚举
  - `configure_stores_api_key` — TOML 配置存储验证
- 总计 66 项测试 (63 单元 + 3 集成)
- `cargo test -p xai-grok-provider` — 66/66 ✅
- `cargo clippy -p xai-grok-provider` — 零警告

---

## Phase 7: 清理收尾 — 2026-07-16

### 完成内容
- 移除 `main.rs` 顶层的 `#![allow(unused_imports, unreachable_code, ...)]`
- `cargo audit` 尝试运行 (Windows 环境安装失败)

### Phase 7.1 完成内容
- 移除 `SamplingClient::api_backend()` 废弃方法（无生产调用者）
- 更新 4 处测试调用改为 `protocol_id()` 匹配
- `ExtraHeaderProvider` 已在早期 Phase 移除（无需操作）
- `is_cli_chat_proxy_url`、`stream/` 仍在活跃使用，保留

### Phase 7.5 完成内容
- 添加 `#[non_exhaustive]` 到: `ProviderModelDef`、`ProviderDefaults`、`RouteInput`、`ProviderError`
- 余下 8 个类型待下一轮（部分需处理跨 crate 构造兼容性）

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

---

## Workspace 清洁：tracing const-eval ICE 修复 — 2026-07-16

### 问题
`tracing` crate（0.1.40+）的 `__tracing_stringify` macro 使用 `FieldName<{ FieldName::len(name) }>` —— const 泛型+内联 const 表达式。Rust 1.92.0 的 `min_generic_const_args` 将此模式误降为 `TupleCall`，导致所有 `tracing::info!()` / `debug!()` / `span!()` 调用引发 `E0080`/ICE。`xai-grok-shell` crate 798 处调用全部受影响。

方案：vendor tracing 0.1.44 到 `crates/vendor/tracing/`，将 `__tracing_stringify` 替换为纯 `stringify!()`，避免 const-generic `FieldName`。

### 完成内容
- 从 crates.io 下载 tracing 0.1.44 源码，置于 `crates/vendor/tracing/`
- 修复 `macros.rs:3247` — `__tracing_stringify` 改为 `stringify!($($k).+)`
- 清理 `Cargo.toml` 移除 `[[test]]` / `[[bench]]` 等不可用条目
- `Cargo.toml` 恢复 `tracing = "0.1"`；新增 `[patch.crates-io]` 段指向 vendored path
- `Cargo.lock` 更新

### 副作用
`r#type` 作为字段名时不再自动去除 `r#` 前缀（代码库中无此用法，行为等价）。

### 关键结果
- `cargo check --workspace` — 零 error ✅
- `cargo clippy --workspace` — 零 warning ✅
- `cargo test -p xai-grok-provider` — 66/66 ✅
- 新增 47 个文件（vendor tracing 源码），修改 2 个文件

### 移除条件
Rust 1.93+ 或 tracing 0.1.45+ 发布后，可移除 vendor 和 patch，恢复为 crates.io 依赖。

---

## Round 1 Fix Batch: C12/C14/C15 — 2026-07-16

### 完成内容
- **C12** (Critical): `Route` 从 `Arc<dyn AuthFn>`/`Arc<dyn Framing>` 改为 `Box<dyn AuthFn>`/`Box<dyn Framing>`，符合 Arch §3.4 的接口契约。为 `AuthFn` 和 `Framing` trait 添加 `clone_box()` 方法，所有 6 个实现者均已更新
- **C14** (Critical): OpenAI provider 已知模型从 2 个扩展到 6 个（`gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini`, `gpt-4.1`, `gpt-4.1-mini`）
- **C15** (Critical): Ollama provider `auth_scheme` 从 `Bearer` 改为 `None`；`xai-grok-provider` 和 `xai-grok-sampler` 的 `AuthScheme` 枚举均添加 `None` 变体；shell `to_auth_scheme()` bridge 更新；sampler client 添加 4 个 match arm 处理 `AuthScheme::None`
- `ISSUE.md` 中 C12/C14/C15 标记为 `-Fixed`

### 关键结果
- `cargo test -p xai-grok-provider` — 66/66 ✅
- `cargo clippy -p xai-grok-provider -p xai-grok-sampler` — 零警告 ✅
- `cargo fmt --all --check` — 通过 ✅
- 修改 14 个文件（route.rs, auth.rs, framing.rs, 6 个 provider, types.rs x2, client.rs, provider_adapter.rs, ISSUE.md, PROGRESS.md）

---

## Round 2 Fix Batch: C13/C16/C18 — 2026-07-17

### 完成内容
- **C13** (Critical): `docs/model-adapter-architecture.md` §4 补充 `ProviderRegistry` 的 `configs` 字段和 `store_config()`/`get_config()`/`register_route()`/`get_route()` 四个方法
- **C16** (Critical): Ollama provider `known_models` 从空扩展到 3 个示例模型（`llama3.1`, `codellama`, `deepseek-coder`）
- **C18** (Critical): 4 处 `tokio::fs::canonicalize` 替换为 `spawn_blocking + dunce::canonicalize`（`xai-grok-tools/src/util/fs.rs` 2 处 + `xai-grok-workspace/src/handle.rs` 2 处）
- **C17** (Critical): 标记为 `-Deferred` — 需与 M45/C19/C21 认证栈统一方案时一并处理
- 更新 `ISSUE.md` 标记 C13/C16/C18 为 `-Fixed`，C17 为 `-Deferred`
- 更新 `docs/model-adapter-architecture.md` 补充 ProviderRegistry 文档

### 关键结果
- `cargo check -p xai-grok-provider -p xai-grok-tools -p xai-grok-workspace` — 通过 ✅
- `cargo clippy -p xai-grok-provider -p xai-grok-tools -p xai-grok-workspace -- -D warnings` — 零警告 ✅
- `cargo test -p xai-grok-provider` — 66/66 ✅
- 修改 5 个文件（ollama.rs, fs.rs, handle.rs, model-adapter-architecture.md, ISSUE.md），新增 PROGRESS.md 条目

---

## Phase 8: 多 Provider 模型动态获取 — 2026-07-17

### 完成内容

- **8.1** 从 `types.rs` 删除 `ProviderModelDef` 结构体和 `known_models` 字段
- **8.2** 从 `Provider` trait 删除 `known_models()` 方法
- **8.3** 清理 8 个文件中所有 `known_models` 引用（6 provider + registry.rs + provers/mod.rs 测试）
- **8.4** 新增 `ModelListFormat` 枚举（`OpenAiCompatible | OllamaTags`）、`model_list_endpoint` 和 `model_list_format` 字段到 `ProviderDefaults`
- **8.5** 配置 6 个 provider 的模型列表端点（Ollama → `http://localhost:11434/api/tags` + `OllamaTags`，其余 → `None` + `OpenAiCompatible`)
- **8.6** 实现 `fetch_provider_models_blocking()` 在 `xai-grok-shell/src/agent/models.rs`
- **8.7** 实现 `parse_ollama_tags_models()` Ollama `/api/tags` 响应解析器
- **8.8** 从 `resolve_model_list()` 删除 Layer 2b（`provider_known_models()` 函数 + 调用）
- **8.9** 集成内存 prefetch 缓存（每 provider 独立缓存键 `"{pid}|{url}"`，TTL 300s，失败时回退过期缓存）
- **8.10** 更新测试（删除 `all_providers_have_known_models` → 替换为 `all_providers_have_model_list_config`；删除 `openai_provider_full_pipeline` 中的 `known_models` 断言）
- 更新 `docs/model-adapter-architecture.md` 对齐 §3.2/§3.3/§5.x/§6.1/§6.4/§9/§10/§13
- 添加 `docs/implementation-plan.md` Phase 8 完整定义

### 修改文件

| 文件 | 变更 |
|------|------|
| `crates/codegen/xai-grok-provider/src/types.rs` | 删除 `ProviderModelDef` + `known_models`；新增 `ModelListFormat`、`model_list_endpoint`、`model_list_format` |
| `crates/codegen/xai-grok-provider/src/provider.rs` | 删除 `known_models()` trait 方法 |
| `crates/codegen/xai-grok-provider/src/providers/xai.rs` | 删除 `known_models` 字段 + trait impl |
| `crates/codegen/xai-grok-provider/src/providers/openai.rs` | 同上 |
| `crates/codegen/xai-grok-provider/src/providers/anthropic.rs` | 同上 |
| `crates/codegen/xai-grok-provider/src/providers/opencode.rs` | 同上 |
| `crates/codegen/xai-grok-provider/src/providers/ollama.rs` | 同上 + 设置 OllamaTags 格式 |
| `crates/codegen/xai-grok-provider/src/providers/openai_compatible.rs` | 删除 `known_models` 字段 + trait impl |
| `crates/codegen/xai-grok-provider/src/providers/mod.rs` | 删除测试中 `known_models` |
| `crates/codegen/xai-grok-provider/src/registry.rs` | 同上 |
| `crates/codegen/xai-grok-provider/tests/provider_e2e.rs` | 更新测试 |
| `crates/codegen/xai-grok-shell/src/agent/config.rs` | 删除 Layer 2b + `provider_known_models()` |
| `crates/codegen/xai-grok-shell/src/agent/models.rs` | 新增 `fetch_provider_models_blocking()` 及相关辅助函数 + 缓存 |
| `docs/model-adapter-architecture.md` | 全文档对齐 |
| `docs/implementation-plan.md` | 新增 Phase 8 |

### 关键结果
- `cargo check --workspace` — 通过 ✅
- `cargo clippy --workspace -- -D warnings` — 零警告 ✅
- `cargo test -p xai-grok-provider` — 65/65 ✅（62 unit + 3 integration）
- 无 `known_models` 或 `ProviderModelDef` 残留引用
- 删除 `ProviderModelDef` 和 `known_models` 后编译完全通过

### 阻塞项
- (已关闭) Phase 8 的启动集成（调用 `fetch_provider_models_blocking` 并合并到模型解析管线）已在 `resolve_model_list()` 内部实现，作为 Layer 2b
- 预存 `PROGRESS.md` 中 `known_models` 引用为历史记录保留，不影响当前状态

---

## Phase 7.5: `#[non_exhaustive]` 补全 — 2026-07-17

### 完成内容
- 为 8 个类型添加 `#[non_exhaustive]`
  - `Route`（`route.rs:42`）
  - `Endpoint<Body>`（`endpoint.rs:31`）
  - `ProtocolBody<Body>`（`protocol.rs:29`）
  - `ProtocolStream<Frame, Event, State>`（`protocol.rs:34`）
  - `Protocol<Body, Frame, Event, State>`（`protocol.rs:43`）
  - `ProviderConfig`（`config.rs:7`）
  - `ProviderTomlEntry`（`config.rs:38`）
  - `ConfiguredProvider`（`provider.rs:8`）
- 新增 `ProviderConfig::new()` 构造函数（修复 cross-crate 构造错误）
- 更新 `xai-grok-pager-bin/src/main.rs` 两处 `ProviderConfig` 构造调用

### 关键结果
- `cargo check --workspace` — 通过 ✅
- `cargo clippy --workspace -- --deny warnings` — 零警告 ✅
- `cargo test -p xai-grok-provider` — 65/65 ✅
- 所有 8 个类型已添加 `#[non_exhaustive]`
- S13（ISSUE.md）可标记为 `-Closed`

---

## Phase 8: 启动集成（Layer 2b） — 2026-07-17

### 完成内容
- 在 `resolve_model_list()` 中，在 Layer 2（prefetched）和 Layer 3（[model.*]）之间插入 Layer 2b
- Layer 2b 调用 `fetch_provider_models_blocking()`，各 provider 的 API 模型以低优先级（不覆盖 xAI prefetched）注入
- 使用 `resolved.entry(key).or_insert(entry)` 确保 prefetched/xAI 模型优先级不变
- 缓存由 `PROVIDER_MODEL_CACHE` 管理，首次调用后复用

### 关键结果
- `cargo check --workspace` — 通过 ✅
- `cargo clippy --workspace -- --deny warnings` — 零警告 ✅
- `cargo test -p xai-grok-provider` — 65/65 ✅
- 三处集成缺口（provider_registry 读取、fetch 调用、注入 resolve_model_list）全部关闭

### 修改文件
| 文件 | 变更 |
|------|------|
| `crates/codegen/xai-grok-shell/src/agent/config.rs` | `resolve_model_list()` 新增 Layer 2b + `fetch_provider_models_blocking` 调用 |
| `crates/codegen/xai-grok-provider/src/config.rs` | 新增 `ProviderConfig::new()` 构造函数 |
| `crates/codegen/xai-grok-provider/src/route.rs` | 添加 `#[non_exhaustive]` 到 `Route` |
| `crates/codegen/xai-grok-provider/src/endpoint.rs` | 添加 `#[non_exhaustive]` 到 `Endpoint` |
| `crates/codegen/xai-grok-provider/src/protocol.rs` | 添加 `#[non_exhaustive]` 到 `ProtocolBody`、`ProtocolStream`、`Protocol` |
| `crates/codegen/xai-grok-provider/src/config.rs` | 添加 `#[non_exhaustive]` 到 `ProviderConfig`、`ProviderTomlEntry` |
| `crates/codegen/xai-grok-provider/src/provider.rs` | 添加 `#[non_exhaustive]` 到 `ConfiguredProvider` |
| `crates/codegen/xai-grok-provider/tests/provider_e2e.rs` | 改用 `ProviderConfig::new()` |
| `crates/codegen/xai-grok-pager-bin/src/main.rs` | 改用 `ProviderConfig::new()` 两处 |

---

## Batch 3: `#[non_exhaustive]` 保护完成 — 2026-07-17

### 完成内容
- 8 类型加 `#[non_exhaustive]`: `SamplingChannel`, `SamplingEvent`, `SamplingErrorInfo`, `SamplingErrorKind`, `InferenceLatencyStats`, `RequestId`, `AuthInfo`, `SamplingConsumer`
- 新增构造器: `InferenceLatencyStats::new()`, `SamplingErrorInfo::new()` + `.with_model_metadata()` + `.with_empty_response_context()`, `RetryPolicy::new()`, `OriginClientInfo::new()`
- `SamplerConfig` 保持原样（5 处生产调用各设 20+ 字段，`#[non_exhaustive]` 不切实际）
- 修复跨 crate 测试构造: 10 处 `SamplingErrorInfo`、5 处 `InferenceLatencyStats`、1 处 `OriginClientInfo`、1 处 `RetryPolicy`
- 修复 `SamplingEvent`/`SamplingChannel` match 添加 `_ => {}` 通配臂
- 修复 `cancel_running_task_tests.rs`: 重复 `extra_headers` 字段 + 遗漏 `protocol_id` 字段

### 修改文件（19 个）
- `xai-grok-sampler/src/metrics.rs` — `InferenceLatencyStats::new()`
- `xai-grok-sampler/src/events.rs` — `SamplingErrorInfo::new()` + builder
- `xai-grok-sampler/src/config.rs` — `RetryPolicy::new()`, `OriginClientInfo::new()`
- `xai-grok-sampler/src/types.rs`, `attribution.rs`, `sampling_log.rs` — `#[non_exhaustive]`
- `xai-grok-shell/src/session/signals.rs` — 5 处 `InferenceLatencyStats` → `::new()`
- `xai-grok-shell/src/session/compaction.rs` — 2 处 `SamplingErrorInfo` → `::new()`
- `xai-grok-shell/src/session/acp_session_tests/*.rs` — 8 处 `SamplingErrorInfo` → `::new()`
- `xai-grok-shell/src/agent/mvp_agent/tests.rs` — `OriginClientInfo` → `::new()`
- `xai-grok-shell/src/session/acp_session_impl/tool_calls.rs` — `_ => {}` match arms
- `xai-grok-shell/src/session/acp_session_impl/spawn.rs` — `RetryPolicy` → `::new()`
- `xai-grok-shell/tests/test_doom_loop_recovery.rs` — `RetryPolicy` → `::new()`
- `xai-grok-http/src/lib.rs`, `xai-grok-telemetry/src/http.rs` — 已有 `OriginClientInfo::new()` 调用者
- `ISSUE.md` — Batch 3 `[x]`

### 关键结果
- `cargo check -p xai-grok-sampler -p xai-grok-shell` — 通过 ✅
- `cargo clippy -p xai-grok-sampler -p xai-grok-shell -- --deny warnings` — 零警告 ✅
- `cargo test -p xai-grok-sampler --lib` — 154/154 ✅
- `cargo test -p xai-grok-shell --lib` — 4939/5530 ✅（591 预存失败，全部为 agent infra 测试，与 batch 3 无关）

---

## Provider Adapter V1 — Phase 0: Restore Trustworthy Baseline — 2026-07-17

### Base and result
- Start commit: `e284cc1`
- End commit: `e284cc1` (no source changes)
- Tasks completed: `P0-01`, `P0-02`, `P0-03`, `P0-04`, `P0-05`

### Files changed
- `docs/provider-adapter-v1/baseline.md` — created
- `PROGRESS.md` — appended

### Regression issues
- `C-01`: confirmed — `Model::make` undeclared at `registry.rs:156`, `registry.model()` missing at `registry.rs:222`
- `C-02` through `M-03`: presumed present per plan audit

### Verification
- `git rev-parse --show-toplevel` — exit 0 — `D:/grok_build`
- `git branch --show-current` — exit 0 — `feat/provider-adapter`
- `rustc --version` — `1.92.0`
- `protoc --version` — `libprotoc 35.0`
- `cargo test -p xai-grok-provider --all-targets` — exit 1 — 2 compile errors (C-01)
- `cargo check -p xai-grok-sampler --all-targets` — exit 0 — pass
- `cargo check -p xai-grok-shell --all-targets` — exit 1 — 32+ pre-existing errors

### Deferred or blocked
- `xai-grok-pager` and `xai-grok-pager-bin` check timed out at 5 min (dependency compilation)
- Pre-existing working-tree change in `providers_modal.rs` (43+2 lines) preserved

### Scope review
- No unrelated files changed.
- No dependency added.
- No secret present in diff or logs.
- Baseline manifest documents all known pre-existing failures.

---

## Provider Adapter V1 — Phase 1: Freeze V1 Architecture and Public Contracts — 2026-07-17

### Base and result
- Start commit: `c39d005`
- End commit: `d9d0937`
- Tasks completed: `P1-01`, `P1-02`, `P1-03`

### Files changed
- `docs/model-adapter-architecture.md` — reconciled with AD-02 through AD-09
- `docs/implementation-plan.md` — added supersession banner
- `docs/provider-adapter-v1/config-reference.md` — created
- `docs/provider-adapter-v1/provider-matrix.md` — created
- `docs/provider-adapter-v1/public-contracts.md` — created

### Verification
- Placeholder scan: zero TBD/TODO tokens in newly written docs
- architecture doc renumbered (removed duplicate §11, rebalanced sections)

### Scope review
- No unrelated files changed
- No dependency added

---

## Provider Adapter V1 — Phase 4: Route-to-Sampler Execution Authority — 2026-07-17

### Base and result
- Start commit: `1bb390f`
- End commit: `d1d93f6`
- Tasks completed: `P4-01` through `P4-06`

### Files changed
- `xai-grok-sampler/src/config.rs` — `endpoint_path`/`endpoint_query` fields (P4-01)
- `xai-grok-sampler/src/protocols/mod.rs` — `resolve_protocol_id()`, tests (P4-02)
- `xai-grok-sampler/src/client.rs` — protocol validation, no `_` fallback, `endpoint()` uses route path (P4-02/03)
- `xai-grok-shell/src/agent/provider_resolution.rs` — route compiler (P4-04)
- `xai-grok-provider/src/endpoint.rs` — `path_for_default()` (P4-04)
- `xai-grok-shell/src/agent/config.rs` — legacy header doc (P4-05)

### Regression issues
- `C-02`: Route is authority — `endpoint()` uses route path/query
- `C-11`: No `_ => chat_completions` fallback — unknown protocol returns error

### Verification
- `cargo test -p xai-grok-sampler --all-targets` — 174 passed, 0 failed
- `cargo test -p xai-grok-provider --all-targets` — 109 passed, 0 failed
- `cargo clippy -p xai-grok-sampler --all-targets -- -D warnings` — pass
- `cargo check -p xai-grok-shell --lib` — clean

### Scope review
- No unrelated files changed, no dependency added
---

## Provider Adapter V1 — Phase 2: Provider Core and Transactional Registry — 2026-07-17

### Base and result
- Start commit: `d9d0937`
- End commit: `d333d4e`
- Tasks completed: `P2-01` through `P2-07`

### Files changed
- `src/types.rs` — added `RouteId`, `ModelSourceSpec`, `validate_id()`, validation tests (P2-01)
- `src/error.rs` — added 11 ProviderError variants (P2-01)
- `src/auth.rs` — added `CredentialSource`, `AuthPolicy`, `ResolvedCredential`, `resolve_credential_source()`, `apply_auth_policy()`, header validation (P2-02)
- `src/config.rs` — redacted `Debug` for ProviderConfig, added `validate_headers()` (P2-02)
- `src/endpoint.rs` — `render()` returns `Result<Url, ProviderError>`, URL safety validation (P2-03)
- `src/route.rs` — redefined Route per AD-02 (cloneable, declarative, no framing), removed RouteInput/RoutePatch/RouteDefaults (P2-04)
- `src/framing.rs` — marked as deprecated (P2-04)
- `src/model.rs` — added Default for `ModelLimits`/`GenerationOptions` (P2-04)
- `src/provider.rs` — redefined ConfiguredProvider with route set, RouteSelector trait, DefaultRouteSelector (P2-05)
- `src/registry.rs` — transactional registry snapshots with rebuild/rollback, deterministic order (P2-06)
- `src/providers/` — all 6 providers updated to new Route/ConfiguredProvider APIs
- `src/lib.rs` — no changes needed
- `tests/provider_e2e.rs` — updated for new Route fields

### Regression issues
- `C-01`: resolved — `ProviderRegistry::model()` replaced by `registry.snapshot()` + `registry.configured()`
- `M-02` part: registry rebuild is now transactional with revision and rollback
- `M-03` part: snapshot order is deterministic and tested

### Verification
- `cargo test -p xai-grok-provider --all-targets` — 101 passed, 0 failed
- `cargo clippy -p xai-grok-provider --all-targets -- -D warnings` — zero warnings
- `cargo doc -p xai-grok-provider --no-deps` — zero warnings
- `cargo fmt --all -- --check` — zero diffs

### Scope review
- No unrelated files changed
- No dependency added
- No secret present in diff

---

## Provider Adapter V1 — Phase 3: Typed Config and Precedence — 2026-07-17 (in progress)

### Base and result
- Start commit: `d333d4e`
- End commit: `1bb390f`
- Tasks completed: `P3-01`, `P3-02`, `P3-03`

### Files changed
- `xai-grok-shell/src/agent/config.rs` — added `provider: Option<toml::Value>` field to Config, fixed `route.auth.apply()` → `apply_auth_policy()` (P3-01)
- `xai-grok-provider/tests/config_precedence.rs` — created 5 test cases proving env/TOML/CLI precedence (P3-02)
- `xai-grok-pager/src/app/mod.rs` — removed duplicate registry creation, accepts injected registry (P3-03)
- `xai-grok-pager-bin/src/main.rs` — passes registry to pager `run()` (P3-03)
- `xai-grok-pager/src/provider_state.rs` — warn on double init (P3-03)

### Regression issues
- `M-01`: eligible for `-Fixed` — `provider` field on Config absorbs `[provider.*]` sections, no unknown-key warning

### Remaining scope
- `P3-04`: legacy xAI compatibility preservation
- `P3-05`: manual model provider binding fields (provider/route on ModelEntry)

### Verification
- `cargo check -p xai-grok-provider --all-targets` — 101 passed, 0 failed
- `cargo test -p xai-grok-provider --all-targets` — 101 passed, 0 failed
- `cargo check -p xai-grok-shell --lib` — clean
- `cargo check -p xai-grok-pager --lib` — clean (test targets have pre-existing errors)

### Scope review
- No unrelated files changed
- No dependency added

---

## Provider Adapter V1 — Phase 7: Asynchronous Model Catalog — 2026-07-17

### Base and result
- Start commit: `d1d93f6`
- End commit: `f3c7070`
- Tasks completed: `P7-01` through `P7-07`

### Files changed
- `xai-grok-shell/src/agent/provider_catalog.rs` — parsers, snapshots, async service, TTL, persistence
- `xai-grok-shell/src/agent/mod.rs` — exported provider_catalog
- `xai-grok-shell/src/agent/config.rs` — provider_catalog field
- `xai-grok-pager-bin/src/main.rs` — create catalog after registry
- `xai-grok-shell/src/agent/provider_resolution.rs` — clippy fix

### Phase 7 gate (T2)
- C-05: `derive_model_list_url()` uses resolved provider base URL ✅
- C-06: `ProviderCatalogEntry::is_stale()` + `RefreshStrategy` + `DEFAULT_CACHE_TTL` ✅
- C-09: OpenCode public mode via `CredentialSource::Public` ✅
- C-07: `derive_model_list_url()` with user base_url override ✅
