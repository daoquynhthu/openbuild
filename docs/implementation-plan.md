# Implementation Plan — Model Adapter Layer (Historical)

> **⚠️ HISTORICAL — SUPERSEDED**
>
> This file is preserved as a historical record. The provider-adapter V1 production
> closure is now governed by `docs/openbuild_provider_adapter_production_v1_plan_2026-07-17.md`,
> which supersedes all prior execution sequences in this document.
>
> **Do not use this file for execution decisions.**
>
> ---
>
> **参考架构文档**: `docs/model-adapter-architecture.md`
> **目标分支**: `feat/provider-adapter`
>
> 本文档是施工的唯一路线图。所有实现工作必须严格遵循此计划推进。

---

## Phase 0: Dependency Analysis & Workspace Setup

### 任务

| ID | 任务 | 产出 | 参考 |
|----|------|------|------|
| 0.1 | 在 workspace `Cargo.toml` members 中添加 `"crates/codegen/xai-grok-provider"` | 修改后成员列表 | — |
| 0.2 | 创建 `crates/codegen/xai-grok-provider/Cargo.toml`，声明依赖 | Cargo.toml | 现有 crate 依赖清单 |
| 0.3 | 创建初始 `lib.rs` 和模块骨架（全 `pub mod` + 桩） | 空模块声明 | Arch §13 File Map |
| 0.4 | 验证 `cargo check -p xai-grok-provider` 通过 | CI 绿 | — |

### 门禁

- `cargo check -p xai-grok-provider` 零错误
- `cargo clippy -p xai-grok-provider` 零新增警告
- 现有 workspace 所有 crate 不受影响：`cargo check --workspace` 零错误

---

## Phase 1: 核心类型层 — xai-grok-provider 基础类型

### 目标

实现架构文档 §3 中定义的全部核心类型（不含 provider 实现）。创建可独立编译的类型库。

### 文件产出

```
xai-grok-provider/src/
├── lib.rs              # pub mod 导出
├── types.rs            # ProviderId, ProviderDefaults, ProviderModelDef
├── route.rs            # Route, RouteInput, RoutePatch, RouteDefaults
├── endpoint.rs         # Endpoint, EndpointPart, EndpointInput, merge_endpoints
├── framing.rs          # Framing trait, SseFraming
├── auth.rs             # Credential, AuthFn trait, chaining helpers
├── model.rs            # Model, ModelDefaults, ModelLimits, HttpOptions
├── events.rs           # LLMEvent, Usage, FinishReason
├── protocol.rs         # Protocol, ProtocolBody, ProtocolStream
├── config.rs           # ProviderConfig
└── provider.rs         # Provider trait, ConfiguredProvider
```

### 子任务

| ID | 任务 | 关键细节 |
|----|------|----------|
| 1.1 | 实现 `ProviderId`（newtype + 常量） | `pub struct ProviderId(pub String)` + `impl ProviderId { pub const XAI: &'static str = ... }` |
| 1.2 | 实现 `ProviderDefaults` + `ProviderModelDef` | 字段完全对齐架构文档 §3.2 |
| 1.3 | 实现 `Endpoint` + `EndpointPart` + `EndpointInput` + `merge_endpoints` | `render()` 返回 `url::Url` |
| 1.4 | 实现 `Framing` trait + `SseFraming` | `frame()` 接收 `ByteStream` → `Stream<String>` |
| 1.5 | 实现 `Credential` 枚举 + `AuthFn` trait | `or_else()` 链式回退，`.bearer()` / `.header(name)` 渲染 |
| 1.6 | 实现 `Route` + `RouteInput` + `RoutePatch` + `RouteDefaults` | `with(self, patch) → Route` 不可变更新 |
| 1.7 | 实现 `Protocol<B,F,E,S>` + `ProtocolBody` + `ProtocolStream` | `Protocol::new(id, body, stream)` |
| 1.8 | 实现 `LLMEvent` + `Usage` + `FinishReason` | 完整含 StepStart/Finish, ToolInputStart/Delta/End, ToolResult |
| 1.9 | 实现 `Model` + `ModelDefaults` + `ModelLimits` + `HttpOptions` | `Model::make(input)` 工厂函数 |
| 1.10 | 实现 `ProviderConfig` | Deserialize + 默认值 |
| 1.11 | 实现 `Provider` trait + `ConfiguredProvider` | trait 关联 `ProviderRegistry` |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| `ProviderId` 创建/比较/序列化 | 100% | 单元测试 |
| `Endpoint::render()` | 100% | 多组 URL/路径/查询参数组合 |
| `SseFraming::frame()` | 90% | 模拟字节流 + 验证输出事件序列 |
| `Credential` 链解析 | 100% | 全分支：Inline/Env/Session/None；`or_else` 级联 |
| `AuthFn` Bearer 渲染 | 100% | 验证生成的 HeaderMap |
| `AuthFn` header 渲染 | 100% | 验证自定义头部名 |
| `Route::with()` | 100% | 补丁前后字段正确性 |
| `Protocol` 构造 | 100% | 简单 smoke test |
| `LLMEvent` 序列化/反序列化 | 90% | 所有变体的 round-trip |
| `Usage` 字段完整性 | 100% | 验证 invariant |
| `Model::make()` | 100% | 正确初始化 |
| `ProviderConfig` 反序列化 | 100% | TOML/JSON 两种格式 |

### 门禁

- 单元测试覆盖率 ≥ 95%（`cargo tarpaulin -p xai-grok-provider`）
- `cargo clippy -p xai-grok-provider` 零警告
- `cargo check --workspace` 零错误（Windows: 已知 `xai-grok-shell` 中 tracing crate const eval bug，不影响开发）
- 所有 trait 和 struct 均带有 `#[non_exhaustive]`（应对外来扩展）

---

## Phase 2: 协议层提取 — 重构 xai-grok-sampler

### 目标

将 `xai-grok-sampler/src/stream/{chat_completions,responses,messages}.rs` 中的 SSE 解析逻辑提取为 `Protocol` 值。Protocol 变为纯数据（struct + 函数指针），不再通过枚举 match 分派。

### 文件变更

```
xai-grok-sampler/src/
├── protocol.rs                  [NEW] — Protocol 值定义 + 注册
├── protocols/                   [NEW]
│   ├── mod.rs                   — 导出所有 Protocol
│   ├── chat_completions.rs      [REFACTOR] — ChatCompletionsProtocol
│   ├── responses.rs             [REFACTOR] — ResponsesProtocol
│   └── messages.rs              [REFACTOR] — MessagesProtocol
├── client.rs                    [MODIFY] — 使用 ProtocolId + 外部协议
├── config.rs                    [MODIFY] — SamplerConfig 携带 protocol_id
├── actor/request_task.rs        [MODIFY] — Protocol.step 分派
└── stream/                      [REMOVE] — 迁移后删除
```

### 子任务

| ID | 任务 | 关键细节 |
|----|------|----------|
| 2.1 | 创建 `protocol.rs`，定义 `ProtocolTable`：`HashMap<ProtocolId, Protocol>` | 注册表，协议在此注册 |
| 2.2 | 重构 `stream/chat_completions.rs` → `protocols/chat_completions.rs` | 保持现有 `ChatCompletionChunk` 类型，包装为 `Protocol` |
| 2.3 | 重构 `stream/responses.rs` → `protocols/responses.rs` | `rs::ResponseStreamEvent` → Protocol |
| 2.4 | 重构 `stream/messages.rs` → `protocols/messages.rs` | `MessageStreamEvent` → Protocol |
| 2.5 | 修改 `SamplerConfig`，新增 `protocol_id: ProtocolId` | 现有 `api_backend` 保持为配置兼容 |
| 2.6 | 修改 `SamplingClient`，通过 `ProtocolTable` 查找并执行协议 | `client.protocol_table().get(&config.protocol_id)` |
| 2.7 | 修改 `request_task.rs`，将 `match api_backend` 替换为 `protocol.step()` | 统一分派路径 |
| 2.8 | 删除废弃的 `stream/` 目录 | 清理 |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| 三个 Protocol 的 `body.from()` | 100% | 与现有采样测试相同的输入集 |
| 三个 Protocol 的 `stream.step()` | 100% | 复制现有流测试到新路径 |
| `ProtocolTable` 查找 | 100% | 所有协议 ID 均可找到 |
| `SamplerConfig` 向后兼容 | 100% | 旧配置反序列化后 `protocol_id` 自动从 `api_backend` 推导 |
| 回归：现有采样集成测试 | 100% 通过 | 现有测试集在修改后全部通过 |

### 门禁

- 现有 `xai-grok-sampler` 测试集零失败
- 新代码覆盖率 ≥ 90%
- 删除 `stream/` 后，`cargo check -p xai-grok-sampler` 通过
- 所有 Protocol 值和 `SamplingClient` 之间无循环依赖

---

## Phase 3: 认证层分离 — 重构 shell/auth

### 目标

将 xAI 的 OAuth 流程封装为 `XaiProvider` 的组成部分，实现可组合的 `AuthFn`。为每个内置 Provider 定义其认证链。

### 文件变更

```
xai-grok-provider/src/
└── auth.rs                     [MODIFY] — 完善 AuthFn + Credential 实现

xai-grok-shell/src/auth/
├── manager.rs                  [MODIFY] — 适配 AuthFn 接口
├── credential_provider.rs      [MODIFY] — 实现 AuthFn trait
└── mod.rs                      [MODIFY] — 统一导出
```

### 子任务

| ID | 任务 | 关键细节 |
|----|------|----------|
| 3.1 | 完善 `AuthFn` trait 实现：`AuthNone`、`AuthBearer`、`AuthHeader`、`AuthChain` | `or_else` 链式级联实现 |
| 3.2 | 实现 `Credential` 解析引擎：从 Inline/Config/Session 提取字符串 | `std::env::var` + 配置查询 + 会话令牌 |
| 3.3 | 将 `AuthManager` 包装为 `AuthFn` | 实现 `AuthFn::apply()` → 调用现有 OAuth 流程 |
| 3.4 | 将现有 `ShellAuthCredentialProvider` 适配为 `AuthFn` | 保持向后兼容 |
| 3.5 | 验证 xAI OAuth 流程不受影响 | `grok login` 全流程测试 |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| `AuthNone` / `AuthBearer` / `AuthHeader` 渲染 | 100% | HeaderMap 验证 |
| `AuthChain` 链式解析 | 100% | 3+ 层链的 fallthrough |
| `Credential::optional()` 空键处理 | 100% | None → 回退 |
| `Credential::config()` 环境变量查找 | 100% | set/unset 环境变量 |
| `AuthManager` 包装为 AuthFn | 100% | mock manager |
| 回归：现有认证测试 | 100% 通过 | 现有测试集 |

### 门禁

- 现有 auth 相关测试 100% 通过
- `grok login` 交互流程不受影响（手动验证）
- 无新环境变量引入

---

## Phase 4: Provider 实现层

### 目标

实现架构文档 §5 中定义的 6 个内置 Provider。每个 Provider 是一个独立的 `.rs` 文件，通过 `ProviderRegistry` 注册。

### 文件产出

```
xai-grok-provider/src/providers/
├── mod.rs                     — register_all() 注册所有内置 Provider
├── xai.rs                     — XaiProvider（OAuth, x-grok-*, doom-loop）
├── openai.rs                  — OpenAIProvider（Chat + Responses 双协议）
├── anthropic.rs               — AnthropicProvider（Messages, x-api-key）
├── opencode.rs                — OpenCodeProvider（Zen 网关）
├── ollama.rs                  — OllamaProvider（无认证, localhost）
└── openai_compatible.rs       — Generic + Profiles（DeepSeek, Groq 等）
```

### 子任务

| ID | 任务 | 关键细节 | 参考 |
|----|------|----------|------|
| 4.1 | 实现 `register_all()` 函数 | 创建所有 Provider 实例并注册 | Arch §4, §13 |
| 4.2 | 实现 `XaiProvider` | `base_url: https://api.x.ai/v1`, `api_backend: Responses`, 认证链含 OAuth, 注入 `x-grok-*` 头部 | Arch §5.1 |
| 4.3 | 实现 `OpenAIProvider` | 双 Route：ChatCompletions + Responses, 认证链 `ENV("OPENAI_API_KEY")` | Arch §5.2 |
| 4.4 | 实现 `AnthropicProvider` | `auth_chain` 使用 `.header("x-api-key")` 而非 Bearer, 默认 `anthropic-version` 头部 | Arch §10 Anthropic 示例 |
| 4.5 | 实现 `OpenCodeProvider` | `base_url: https://opencode.ai/zen/v1`, 匿名回退 `apiKey="public"`，动态模型列表 | Arch §5.4 |
| 4.6 | 实现 `OllamaProvider` | 无认证，`localhost:11434`，动态拉取 `/api/tags` | Arch §5.5 |
| 4.7 | 实现 `OpenAiCompatibleProvider` | catch-all，支持 profiles 映射（Groq, DeepSeek, Together, Fireworks, etc.） | Arch §5.6 |
| 4.8 | 实现 `ProviderRegistry` | `register()`, `get()`, `configure()`, `detect_from_url()`, `model()` | Arch §4 |
| 4.9 | 实现 `detect_from_url()` | 通过 base_url host 模式匹配自动识别 Provider | Arch §4 |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| 每个 Provider 的 `configure()` 输出 | 100% | 验证 Route 的各轴正确组装 |
| 每个 Provider 的 `known_models()` | 100% | 返回非空 |
| 每个 Provider 的认证链 | 100% | 模拟各种凭据状态 |
| `XaiProvider` 的 `x-grok-*` 头部注入 | 100% | 仅对 xAI URL 生效 |
| `ProviderRegistry` 注册/查找 | 100% | 全 Provider ID |
| `detect_from_url()` 自动识别 | 100% | 每个 profile 一个测试用例 |
| `register_all()` 无重复 | 100% | UUID 唯一性 |

### 门禁

- 所有 Provider 的默认 `base_url` 可访问（不要求认证，仅 DNS 解析）
- `ProviderRegistry::detect_from_url()` 对已知 URL 模式 100% 准确
- 每个 Provider 文件 ≤ 200 行（不含 known_models 列表数据）

---

## Phase 5: 配置与 CLI 集成层

### 目标

实现 `[provider.*]` TOML 配置解析、`--provider`/`--api-key`/`--base-url` CLI 标志、自动检测逻辑、Route-based 模型解析，以及 **TUI Provider 配置界面**。

### CLI 标志

| 标志 | 类型 | 作用域 | 说明 |
|------|------|--------|------|
| `--provider` | `Option<String>` | global | 指定 Provider ID（如 `"openai"`, `"anthropic"`） |
| `--api-key` | `Option<String>` | global | API 密钥 |
| `--base-url` | `Option<String>` | global | 端点 URL 覆盖 |
| `--model` | `Option<String>` | global | 扩展为 `provider/model` 格式（向后兼容裸名） |

### TUI Provider 管理界面

**入口**: `/providers` 斜杠命令 + Settings → Models 分类中的 "Providers" 子项

**模态框设计**（参照 Extensions Modal / MCP Tabs 风格）：

```
┌─ Providers ────────────────────────────────────────────[✗]─┐
│                                                              │
│  Provider           Status       Models     Configured       │
│ ─────────────────────────────────────────────────────────── │
│  ● xAI              ✅ 已连接      1         api.x.ai        │
│  ○ OpenAI           ❌ 未配置      0         —               │
│  ○ Anthropic        ⚡ 需认证      2         claude-...      │
│  ○ OpenCode Zen     🔓 免费模式    —         opencode.ai     │
│  ○ Ollama           🟢 本地        —         localhost:11434 │
│                                                              │
│  [a 添加] [r 刷新] [e 编辑] [x 移除] [T 测试连接]            │
└──────────────────────────────────────────────────────────────┘
```

**Provider 详情/配置面板**（选中 Provider 后 Enter 进入）：

```
┌─ Configure: OpenAI ────────────────────────────────────[✗]─┐
│                                                              │
│  API Key:     ●●●●●●●●●●●●●●●●●●●●                          │
│  Base URL:    https://api.openai.com/v1                      │
│  Models:      [2 available]                                  │
│                 ● gpt-4o         128K   chat_completions      │
│                 ○ gpt-4o-mini    128K   chat_completions      │
│                                                              │
│  [S 保存] [T 测试连接] [D 重置默认]                            │
└──────────────────────────────────────────────────────────────┘
```

**交互流程**:

| 场景 | 操作路径 |
|------|----------|
| 首次配置 | `/providers` → 选中未配置 Provider → Enter → 填写 API Key → 保存 |
| 切换模型 | `Ctrl+M` → 列表显示 `provider/model` → 选中后若未配置自动弹出 API 输入 |
| 查看状态 | `/providers` → 列表显示所有 Provider 状态 |
| 快速连接 | `/connect openai` → 直接进入 API Key 输入 |
| 移除 Provider | `/providers` → 选中 → `x` 确认移除 |
| 无头模式 | `grok -p "hello" --provider openai --api-key sk-...` |

> ⚠️ **实现备注**: Providers 模态框的完整实现涉及 `app/modals.rs`（2800+ 行）中 5 个独立的 dispatch 点：
> `draw_active_modal()`（渲染）、`handle_modal_key()`（键盘）、`handle_modal_mouse()`（鼠标）、
> `active_modal_height()`（尺寸）、`modal_can_drain()`（状态）。每个点都需要为新变体添加匹配分支。
> 当前状态：`ActiveModal::Providers` 已注册到枚举、`Action::OpenProviders` 可分派打开模态框，
> 但渲染为简单的文字列表存根，尚无完整的 `ModalWindowState` 集成、键盘交互、状态指示器、API Key 编辑表单。

**组件实现**（参照现有 modal 模式）：

| 组件 | 文件 | 参照 |
|------|------|------|
| `ActiveModal::Providers` | `views/modal.rs` | 新增变体，参照 `Settings` 模式 |
| `ProvidersModalState` | `views/providers_modal.rs` | 新增，参照 `ExtensionsModalState` 结构 |
| `render_providers_modal()` | `views/providers_modal.rs` | 使用 `ModalWindowState` + `render_modal_window()` |
| `render_provider_detail()` | 同上 | API key 编辑、模型列表展示 |
| `/providers` 命令 | `slash/commands/providers.rs` | 参照 `/mcps` 命令注册模式 |
| `Action::OpenProviders` | `app/actions.rs` | 参照 `Action::OpenExtensionsModal` |
| 模型选择器增强 | `app/agent_view/input.rs` | `Ctrl+M` 列表添加 `provider/` 前缀 |

**视觉风格**（对齐现有 UI）:

| 元素 | 样式 |
|------|------|
| 模态框框架 | `ModalWindowState` + `ModalSizing::medium()` |
| Provider 行 | `PickerEntry::Row` + `PickerRow` |
| 状态标签 | 彩色 badge：`✅` `accent_success`、`❌` `accent_error`、`⚡` `warning`、`🔓` `gray` |
| 搜索过滤 | `/` 键进入 `render_search_bar()` |
| 快捷键页脚 | `Vec<Shortcut>` → `modal_window::render_modal_window()` |
| API Key 输入 | 隐蔽输入 `●●●●`，参照 `ModalInput` 模式 |
| 测试连接 | 异步请求 → 结果反馈（成功/失败 toast） |

### 文件变更

```
xai-grok-provider/src/
├── config.rs                  [MODIFY] — 完善 ProviderConfig 反序列化

xai-grok-shell/src/
├── agent/config.rs            [MAJOR] — Route-based 模型解析
├── agent/cli_models.rs        [MODIFY] — 新增 CLI 标志

xai-grok-pager-bin/src/
├── main.rs                    [MODIFY] — --provider/--api-key/--base-url 参数

xai-grok-pager/src/
├── app/cli.rs                 [MODIFY] — PagerArgs 新增字段
├── app/actions.rs             [MODIFY] — 新增 OpenProviders 等 Action
├── app/dispatch/router.rs     [MODIFY] — Action 分派
├── app/modals.rs              [MODIFY] — ActiveModal 新增 Providers 变体
├── app/agent_view/input.rs    [MODIFY] — Ctrl+M 列表添加 provider 前缀
├── slash/commands/mod.rs      [MODIFY] — 注册 /providers 命令
├── slash/commands/providers.rs[NEW]   — /providers 命令实现
├── settings/defs.rs           [MODIFY] — 新增 Provider 相关设置项
└── views/providers_modal.rs   [NEW]   — Provider 管理模态框
```

### 子任务

| ID | 任务 | 关键细节 | 参考 |
|----|------|----------|------|
| 5.1 | 实现 `[provider.*]` TOML 段落解析 | 合并到 `ProviderConfig` | Arch §7.1 |
| 5.2 | 新增 CLI `--provider` / `--api-key` / `--base-url` | `PagerArgs` 中新增 `global = true` 字段 | — |
| 5.3 | `--model` 格式扩展 | 支持 `provider/model`，向后兼容裸名 | — |
| 5.4 | 实现 startup 流程中的 Provider 初始化 | `ProviderRegistry` + 6 层配置合并 + env 自动检测 | Arch §9 |
| 5.5 | 重写 `resolve_model_list()` 以包含 Provider 层 | `[model.*]` > `[provider.*]` > 内嵌 Provider 默认值 | Arch §6.1 |
| 5.6 | 创建 TUI Providers 模态框 | `views/providers_modal.rs` + `ActiveModal::Providers` + `app/modals.rs` 5 处 dispatch | 需修改 `draw_active_modal`、`handle_modal_key`、`handle_modal_mouse`、`active_modal_height`、`modal_can_drain` |
| 5.7 | 实现 `/providers` 斜杠命令 | `slash/commands/providers.rs` | `/mcps` 模式 |
| 5.8 | 模型选择器增强 | `Ctrl+M` 列表显示 `provider/model`，未配置时自动弹出配置 | — |
| 5.9 | 保留 `[endpoints]` 向后兼容 | 旧配置映射到 `XaiProvider` | Arch §8 |
| 5.10 | 保留环境变量自动检测 | `XAI_API_KEY` → xAI, `OPENAI_API_KEY` → OpenAI 等 | — |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| `[provider.*]` TOML 解析 | 100% | 全字段覆盖 |
| CLI 参数解析 | 100% | clap 测试 |
| 模型解析优先级 | 100% | 多层覆盖链逐一验证 |
| 向后兼容：旧 `[endpoints]` 配置 | 100% | 解析结果与原流程一致 |
| 向后兼容：旧 `[model.*]` 配置 | 100% | 优先级不变 |
| 向后兼容：环境变量 | 100% | set/unset 每项 |
| auto_detect 正确率 | 100% | 已知 URL 模式全覆盖 |
| Providers 模态框渲染 | 90% | 快照测试 |
| `/providers` 命令参数补全 | 100% | Provider ID 列表 |

### 门禁

- 所有现有 `config.rs` 测试通过（回归）
- 旧配置（`[endpoints]` + `[model.*]`）产生与改造前完全相同的 `SamplerConfig`
- 新增 CLI 参数不破坏现有 CLI 行为
- Providers 模态框不干扰现有 TUI 快捷键

---

## Phase 6: 集成与迁移

### 目标

将新 Provider 系统连接到启动流程、Session 和 ACP。xAI 特性门控。端到端验证。

### 文件变更

```
xai-grok-pager-bin/src/main.rs     [MODIFY] — 初始化 ProviderRegistry
xai-grok-shell/src/session/
└── acp_session_impl/
    ├── sampler_turn.rs            [MODIFY] — 从 Model 获取协议
    └── session_setup.rs           [MODIFY] — 使用 Route-based 配置
xai-grok-shell/src/agent/
├── config.rs                      [MODIFY] — 最终化模型解析
└── models.rs                      [MODIFY] — 注册 Provider 模型
```

### 子任务

| ID | 任务 | 关键细节 |
|----|------|----------|
| 6.1 | 在 `main.rs` 启动流程中创建并初始化 `ProviderRegistry` | 调用 `register_all()`，合并用户配置 |
| 6.2 | 将 `ProviderRegistry` 注入到 session actor 的构造函数 | `Arc<ProviderRegistry>` |
| 6.3 | 修改 `session_setup.rs` 使用 Route-based `SamplerConfig` | 从 `Model.route` 提取 |
| 6.4 | 实现 xAI 特性门控：当 Provider != xAI 时跳过 | OAuth 刷新、`x-grok-*`、doom-loop、`x_search` |
| 6.5 | 验证 `grok login` + OAuth 流程 | 端到端测试 |
| 6.6 | 验证非 xAI Provider 不触发 xAI 特定行为 | 集成测试 |

### 测试要求

| 测试范围 | 最低覆盖率 | 方法 |
|----------|-----------|------|
| 端到端：xAI Provider 与现有 OAuth 流程 | 100% | E2E 集成测试 |
| 端到端：OpenAI Provider 模拟 HTTP | 100% | mockito HTTP mock |
| xAI 特性门控正确性 | 100% | 每个门控点单独验证 |
| Session 启动使用新路径 | 100% | 所有 session 测试通过 |

### 门禁

- `grok login` + `grok -p "hello"`（xAI 生产环境）正常
- `grok --provider openai --api-key test --base-url http://localhost:9999 -p "hello"` 正常退出
- 所有现有测试通过
- `cargo clippy --workspace` 零新增警告

---

## Phase 7: 清理与收尾

### 目标

删除废弃代码，补充缺失文档，性能分析，边界情况加固。

### 子任务

| ID | 任务 | 关键细节 |
|----|------|----------|
| 7.1 | 删除废弃代码 | `ExtraHeaderProvider`、`is_cli_chat_proxy_url`（降级为 xAI provider 内部）、旧 `stream/` 目录 |
| 7.2 | 补充 public API 文档 | 所有 `pub` 项加 doc comment |
| 7.3 | 性能分析 | 对比旧新 `resolve_model_to_sampling_config` 性能 |
| 7.4 | 边界情况加固 | 空 Provider 列表、网络错误、未知模型 ID |
| 7.5 | 新增 `#[non_exhaustive]` 到 ProviderId、LLMEvent、FinishReason | 演进安全 |
| 7.6 | `cargo audit` 安全检查 | — |

### 门禁

- `cargo doc --no-deps` 零 warning（缺失文档链接）
- `cargo audit` 零漏洞
- `cargo clippy --workspace` 零警告
- `docs/model-adapter-architecture.md` 与实现完全同步

---

## Phase 8: 多 Provider 模型动态获取

### 目标

将所有 provider 的 hardcoded `known_models` 替换为统一的动态 API 获取管道。每个 provider 在启动时从其模型列表端点获取最新模型，注入模型解析管线。

### 架构变更

- 删除 `ProviderDefaults.known_models` 字段和 `ProviderModelDef` 结构体
- 删除 `Provider::known_models()` trait 方法
- 新增 `ProviderDefaults.model_list_endpoint: Option<String>`（None → 从 base_url 自动推导）
- 新增 `ProviderDefaults.model_list_format: ModelListFormat`（`OpenAiCompatible | OllamaTags`）
- 新增 `fetch_provider_models_blocking()` 函数，在 `configure_providers()` 之后调用
- 从 `resolve_model_list()` 中删除 Layer 2b 硬编码注入

### 任务

| ID | 任务 | 产出 | 参考 |
|----|------|------|------|
| 8.1 | 删除 `known_models` 和 `ProviderModelDef` | 从 `types.rs` 删除字段 + 结构体 | Arch §3.2 |
| 8.2 | 删除 `Provider::known_models()` trait 方法 | 从 `provider.rs` 删除 | Arch §3.3 |
| 8.3 | 清理 6 个 provider 文件 | 删除 `known_models` 构造 + trait impl | xai.rs, openai.rs, anthropic.rs, opencode.rs, ollama.rs, openai_compatible.rs |
| 8.4 | 新增 `model_list_endpoint` + `model_list_format` 到 `ProviderDefaults` | 更新 `types.rs` | Arch §3.2 |
| 8.5 | 配置 6 个 provider 的模型列表端点 | 更新各 provider 的 `defaults()` | Arch §6.4 |
| 8.6 | 实现 `fetch_provider_models_blocking()` | 在 `xai-grok-shell/src/agent/models.rs` 中 | Arch §6.4 |
| 8.7 | 实现 Ollama `/api/tags` 响应解析器 | 从 `{"models":[{"name":"..."}]}` → `ModelEntryConfig` | Arch §6.4 |
| 8.8 | 从 `resolve_model_list()` 中删除 Layer 2b | 删除 `provider_known_models()` 函数和调用 | config.rs |
| 8.9 | 集成 prefetch 缓存 | 每个 provider 独立缓存键 `"{pid}|{url}"`，TTL 300s | models.rs |
| 8.10 | 清理测试 | 删除/更新 `all_providers_have_known_models` 等 | provider_e2e.rs, types.rs |

### 各 Provider 模型列表端点

| Provider | endpoint_url | format | auth |
|----------|-------------|--------|------|
| xAI | `None` → `{base_url}/models` | OpenAiCompatible | Bearer |
| OpenAI | `None` → `{base_url}/models` | OpenAiCompatible | Bearer |
| Anthropic | `None` → `{base_url}/models` | OpenAiCompatible | x-api-key |
| OpenCode | `None` → `{base_url}/models` | OpenAiCompatible | Bearer or none |
| Ollama | `"http://localhost:11434/api/tags"` | OllamaTags | None |
| OpenAiCompatible | `None` → `{base_url}/models` | OpenAiCompatible | Bearer |

### 门禁

- `cargo check --workspace` 零错误
- `cargo clippy --workspace` 零警告
- `cargo test -p xai-grok-provider` 66/66 ✅（更新测试后）
- `cargo test -p xai-grok-shell` 所有前置测试通过
- `cargo doc --no-deps` 零 warning
- 删除 `ProviderModelDef` 后不留下任何悬空引用

---

## 依赖图

```
Phase 0 ──→ Phase 1 ──→ Phase 2 ──→ Phase 5 ──→ Phase 6 ──→ Phase 7 ──→ Phase 8
                │            │                           ↑
                │            └── Phase 3 ────────────────┘
                │                         ↑
                └────────── Phase 4 ──────┘
```

- Phase 1、2、3、4 可**部分并行**（Phase 2 需要 Phase 1 的类型层；Phase 3 需要 Phase 1 的 AuthFn）
- Phase 5 需要 Phase 2 + 3 + 4
- Phase 6 需要 Phase 5
- Phase 7 需要 Phase 6
- Phase 8 需要 Phase 1 + 7（依赖 ProviderDefaults 类型和完整的 provider 注册流程）

---

## 全局门禁汇总

| 门禁 | 检查命令 | 适用阶段 |
|------|----------|----------|
| 编译通过 | `cargo check --workspace` | 全阶段 |
| 零 clippy 警告 | `cargo clippy --workspace` | 全阶段 |
| 零测试失败 | `cargo test --workspace` | 全阶段 |
| 新代码覆盖率 ≥ 90% | `cargo tarpaulin -p xai-grok-provider` | Phase 1-4 |
| 集成测试通过 | `cargo test -p xai-grok-shell --test integration` | Phase 5-6 |
| 性能无倒退 | 对比 `resolve_model_to_sampling_config` 基准 | Phase 7 |
| 零安全漏洞 | `cargo audit` | Phase 7 |
| 文档完整 | `cargo doc --no-deps 2>&1 | grep "warning" | wc -l` | Phase 7 |
| 无硬编码模型列表 | `grep -r "known_models.*ProviderModelDef"` 返回空 | Phase 8 |

---

## 分支策略

```
main
  └── feat/provider-adapter   ← 所有开发在此分支
        ├── Phase 1 → commit
        ├── Phase 2 → commit
        ├── Phase 3 → commit
        ├── Phase 4 → commit
        ├── Phase 5 → commit
        ├── Phase 6 → commit
        ├── Phase 7 → commit
        └── Phase 8 → commit
```

每个 Phase 完成后创建一个标记的 commit。commit message 格式：
`phase-N: <简短描述>`

各 Phase 内部的子任务允许多次 commit，但需保证每个 commit 后 `cargo check --workspace` 通过。

### Phase 收尾流程

每个 Phase 完成后，必须执行以下收尾序列：

1. **门禁检查**：`cargo check --workspace`（Windows 上使用 `.\check.ps1` 代替）+ `cargo clippy --workspace` + `cargo test --workspace`
2. **更新 PROGRESS.md**：追加 Phase 摘要（完成内容、关键结果、阻塞项）
3. **提交**：`git add PROGRESS.md && git commit -m "progress: phase N <名称>"`
4. **提交 Phase**：若还有未提交的代码变更，一并进行

---

## 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 现有测试因重构中断 | 中 | 高 | 每次重构后立即运行 `cargo test` |
| 与 xAI 生产 API 不兼容 | 低 | 高 | xAI Provider 保留完整 OAuth + 所有头部 |
| 新类型系统导致编译时间增加 | 中 | 低 | 仅新增 1 个 crate |
| 第三方 Provider 协议变更 | 低 | 中 | Protocol 值结构允许独立更新 |
