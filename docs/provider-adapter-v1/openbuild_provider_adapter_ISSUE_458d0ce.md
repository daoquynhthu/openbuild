# OpenBuild Provider Adapter V1 — Active Issue Register

> **审计基线**：`feat/provider-adapter` @ `458d0ce00e8024a5f56915a5d0575ae951b1f2e4`
>
> **用途**：本文件是当前 Provider Adapter V1 未关闭问题的唯一权威清单（single source of truth）。
>
> **配套修复指南**：`docs/provider-adapter-v1/production-closure-remediation-guide.md`。该指南只解释严重问题的处理顺序和验证方法，不得修改本文件中的编号、优先级或关闭标准。

## 状态与优先级

- `OPEN`：静态审计已确认，尚未修复。
- `IN_PROGRESS`：已有修复分支，但尚未通过关闭门禁。
- `BLOCKED`：存在已记录的外部阻塞；不得以此标记替代修复。
- `CLOSED`：必须满足该条目的全部关闭标准，并附新鲜的命令输出或 E2E 证据。
- `P0`：阻断生产发布；任一未关闭即为 `NO-GO`。
- `P1`：高风险正确性、可审计性或跨平台缺陷；V1 发布前必须关闭。
- `P2`：发布工程与维护性缺陷；进入 RC 前必须关闭或形成经批准的明确延期记录。

## 全局约束

1. 不得通过删除测试、缩小 CI 范围、增加 `ignore`/`cfg`、吞掉错误或恢复静默回退关闭问题。
2. 不得将一个问题标记为“不可操作”后直接跳过；若架构约束导致无法修复，必须先提交设计偏差并更新本清单。
3. 每次只关闭一个根因；一个提交同时关闭多个问题时，必须分别提供独立证据。
4. 所有 Provider 必须经过同一生产链：

   ```text
   parsed configuration
   → ResolvedProviderSet
   → transactional RegistrySnapshot
   → ResolvedModelExecution
   → prepare_sampler_config
   → PreparedSamplerConfig
   → Sampler
   ```

5. xAI 可以有专用 Provider、OAuth 和 session resolver，但不得拥有绕过上述链路的专用执行通道。

---

# P0 — 发布阻断问题

## OBPA-001 — 异步生产路径中嵌套 Tokio `block_on`

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs:1090-1209`
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs:811,923`
- `crates/codegen/xai-grok-shell/src/agent/handlers/model_switch.rs:13,116`
- `crates/codegen/xai-grok-shell/src/agent/config.rs:4512-4549`
- `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs:60-76`

同步 helper 在内部调用 `tokio::runtime::Handle::current().block_on(...)`，而它又被 ACP session 创建、模型切换等异步生产函数调用。在 Tokio runtime 工作线程内嵌套 `block_on` 可能直接 panic；测试中另建 runtime 无法证明生产路径安全。

### 建议修复

- 将采样配置准备链改为端到端 `async`，从 ACP/session/model-switch 调用点一直 `await` 到 `prepare_sampler_config`。
- 删除生产代码中的 `Handle::block_on`、临时 `Runtime::new().block_on` 和同步 wrapper。
- 同步边界只能位于真正的进程入口或专用阻塞线程，不得位于 Agent/Provider 业务层。
- 增加运行在 `#[tokio::test(flavor = "multi_thread")]` 中的 session 创建与模型切换回归测试。

### 关闭标准

- 生产 Provider/Sampler 链中 `rg "Handle::current\(\).*block_on|Runtime::new\(\).*block_on"` 无匹配。
- ACP 新会话、模型切换、辅助模型解析在真实 Tokio runtime 内完成且无 panic。

---

## OBPA-002 — Provider 路由与认证错误通过 `panic!` 终止进程

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs:1185-1193`
- `crates/codegen/xai-grok-shell/src/agent/config.rs:4540-4548`

Provider-bound 模型解析失败时使用 `panic!`，使缺失密钥、无效 endpoint、热重载不一致等可恢复配置错误升级为进程崩溃。该行为破坏 CLI/ACP 的结构化错误契约。

### 建议修复

- 将相关 helper 返回类型改为 `Result<_, ProviderResolutionError>` 或统一应用错误类型。
- 在 ACP、TUI、模型切换和辅助模型调用点把错误转换为用户可读、可重试、不可泄露密钥的错误消息。
- 明确区分配置错误、认证错误、协议错误、目录错误和内部不变量错误；只有真正不可能发生的内部不变量才允许 `debug_assert!`。

### 关闭标准

- 生产 Provider 路由链不存在基于用户配置或凭据状态触发的 `panic!`。
- 对缺失密钥、未知 Provider、无效 URL、未知协议均有结构化错误测试。

---

## OBPA-003 — Registry 重建丢弃绝大多数 Provider 配置

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/registry.rs:213-240`

`ProviderRegistry::prepare()` 对内置 Provider 只传递 `id`，对 OpenAI-compatible Provider 只传递 `id` 和 `base_url`。以下已解析字段没有进入 `Provider::configure()`：

- `api_key`
- `env_key`
- `protocol`
- `extra_headers`
- `allow_insecure_http`
- `model_list_path`
- `model_list_format`
- 内置 Provider 的 `base_url`

结果是配置文件、热重载和 revision 看似成功，但真实请求仍使用默认值。

### 建议修复

- 定义唯一的 `ResolvedProviderConfig → ProviderConfig` 无损转换。
- 内置 definition 与通用 factory 均接收同一份完整 resolved config。
- 在 registry commit 前验证所有字段已被 Provider 消费或明确拒绝，禁止静默忽略。
- 为每个字段增加 TOML → snapshot → route/catalog 的字段保真测试。

### 关闭标准

- 每个公共 Provider 配置字段都有生产消费点和端到端测试。
- 未支持字段在解析或 prepare 阶段显式报错，而不是被忽略。

---

## OBPA-004 — 解析层提前丢弃 `env_key` 与 `model_list_format`

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/resolution.rs:28-87`

`ProviderRuntimeConfig` 没有 `env_key` 字段；`resolve_one()` 又固定设置 `model_list_format: None`。因此即使后续 registry 转换修复，这两个字段也已经在解析阶段不可恢复。

### 建议修复

- 在 `ProviderRuntimeConfig`/`ProviderPublicConfig` 中保留完整、类型化的 `env_key` 和 `model_list_format`。
- 使用已经存在但未进入生产路径的转换逻辑，或删除重复转换并建立一个权威入口。
- 为配置优先级（profile、legacy、TOML、CLI）增加字段级表驱动测试。

### 关闭标准

- `env_key` 与 `model_list_format` 可从 TOML/CLI 进入 snapshot，并分别影响认证候选和模型目录解析。

---

## OBPA-005 — Provider 级密钥与请求级凭据覆盖未进入认证上下文

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4699-4711`

生产代码创建 `RequestCredentialContext` 时：

- request override 为 `None`；
- model inline key 由旧 `ResolvedCredentials` 提供；
- provider inline key固定为 `None`。

因此 `[provider.*].api_key` 即使被解析，也不能进入最终请求；E2E 通过手工传入 model key 绕过了该缺口。

### 建议修复

- 让 `ResolvedModelExecution` 或其关联 Provider snapshot 可提供 provider credential policy 与 inline secret 引用。
- 在请求准备时按冻结优先级注入 request/model/provider/env/session 凭据。
- 禁止把 provider 密钥复制回通用 `SamplerConfig.api_key` 后再由下游猜测认证方式。

### 关闭标准

- 仅配置 `[provider.custom].api_key` 时，请求携带正确认证头。
- request override、model inline、provider inline、env、session 的优先级均有冲突测试。

---

## OBPA-006 — Route 完整请求 URL 被旧 `credentials.base_url` 覆盖

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4691-4698`
- `crates/codegen/xai-grok-shell/src/agent/provider_resolution.rs:150-190`

生产路径把 `credentials.base_url` 作为 `resolve_model_execution()` 的 URL override 传入；后者将该字符串直接解析为完整请求 URL，而不是基础 URL。Route 已编译的 `/responses`、`/chat/completions` 或 `/messages` 路径可能被替换为裸 `/v1`，或被旧 xAI endpoint 覆盖。

### 建议修复

- 删除生产路径中的旧 base URL override；Provider Route 必须是请求 URL 的唯一权威。
- 若确有 CLI endpoint override，必须在 Route 编译前作为 Provider 配置参与 endpoint 组装，而不是在编译后替换完整 URL。
- 区分 `BaseUrl` 与 `RequestUrl` 类型，禁止同为 `String`/`Url` 的模糊参数。

### 关闭标准

- OpenAI Chat、Responses、Anthropic Messages、自定义兼容 Provider 在真实 production helper 中命中正确完整路径。
- CLI base URL override 不会丢失 route path。

---

## OBPA-007 — OpenAI-compatible factory 尚未实现通用 Provider 契约

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/providers/openai_compatible_factory.rs:23-157`

Factory 当前固定：

- `chat_completions` 协议；
- `/chat/completions` 路径；
- Bearer 认证；
- 仅 DeepSeek/Groq/OpenRouter 三个 profile 的硬编码环境变量；
- `ModelSourceSpec::Dynamic`，但不消费模型列表配置。

它忽略用户指定的 protocol、env key、inline key、extra headers、insecure HTTP、模型列表路径和格式。无 profile 的自定义 Provider 会得到空 env-key 候选。

### 建议修复

- 将通用 factory 改为完全由 `ResolvedProviderSpec` 构造 routes、认证候选、headers 和 model source。
- profile 只能提供默认值；用户配置覆盖后必须进入同一类型化配置。
- 对 Chat Completions、Responses 和明确支持的兼容协议建立 route builder；未知协议必须失败。
- 删除 `#![allow(dead_code)]` 和未消费的 profile metadata，或把它们接入生产逻辑。

### 关闭标准

- 任意命名的兼容 Provider 能使用自定义 env key、inline key、headers、base URL、协议和模型目录。
- 两个自定义 Provider 的配置、密钥和模型目录完全隔离。

---

## OBPA-008 — 热重载丢失 CLI 与 legacy 配置优先级

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/app.rs:1467-1484`

启动 bootstrap 接收 legacy migration 与 CLI override，但创建热重载 coordinator 时把两者都设置为 `None`。首次配置文件变更后，运行时 snapshot 会丢失启动参数和旧配置迁移层，造成 endpoint、认证或默认 Provider 在运行中无意改变。

### 建议修复

- 将不可变的 startup resolution context 保存在 App/ProviderRuntime 中，由每次 rebuild 重用。
- 明确区分可热重载配置与进程级 CLI 覆盖；CLI 覆盖在进程生命周期内始终保持最高优先级。
- 增加“启动后修改 TOML，但 CLI override 仍生效”的集成测试。

### 关闭标准

- 热重载前后同一配置优先级保持不变。
- legacy migration 只在规定的迁移生命周期内工作，且不会因一次 reload 消失。

---

## OBPA-009 — Catalog Service 没有生产刷新者，动态模型发现未闭环

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/provider_catalog.rs`
- `crates/codegen/xai-grok-shell/src/agent/app.rs:1509-1523`
- `crates/codegen/xai-grok-provider/src/runtime.rs`

生产代码订阅 Catalog revision，但 `refresh_provider()`、`refresh_all()`、snapshot load/save 等调用主要存在于测试。`ProviderRuntime::rebuild()` 只更新 registry/revision，并未启动目录刷新；配置变化日志仍表示 rebuild/refresh pending。

### 建议修复

- 为 ProviderRuntime 建立明确的 catalog worker 生命周期：bootstrap load、按 Provider 并发刷新、TTL、取消、失败保留旧快照、原子持久化。
- registry commit 后只刷新新增或配置发生变化的 Provider；删除 Provider 时同步移除其模型。
- 不得在模型解析或 UI 渲染路径中同步发起网络请求。

### 关闭标准

- 新增/修改 Provider 后模型目录自动刷新并发布 revision。
- 网络失败、超时、取消和旧缓存回退有生产集成测试。

---

## OBPA-010 — Legacy Sampler 仍是正式生产回退路径

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4673-4733`

当 registry 不存在或 model 没有 `provider_id` 时，`sampling_config_for_model_with_registry()` 直接调用旧 `sampling_config_for_model()`。这使 Provider Runtime 不是唯一请求权威，并允许未绑定模型绕过 route、auth、header 和 endpoint 校验。

### 建议修复

- 所有生产模型在进入请求准备前必须归一化为 canonical `provider/model`。
- 将旧模型表转换为显式的 legacy migration Provider，而不是保持第二套 Sampler 构造器。
- registry 缺失、模型未绑定或引用歧义必须返回错误。

### 关闭标准

- 生产调用链不存在 `sampling_config_for_model()` 回退。
- 所有 Agent、Subagent、compact、trace、tool 和 auxiliary inference 均要求 resolved execution。

---

## OBPA-011 — 辅助模型仍可构造隐式 xAI Responses 请求绕过 Runtime

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4551-4607`

辅助模型路由失败后，代码手工构造 `ModelEntry`，绑定 xAI bearer、Responses backend 和旧 endpoint，再调用 legacy sampler。该路径造成品牌特权、双权威和错误隐藏。

### 建议修复

- 把 image describe、auto-mode classifier、compaction 等辅助模型全部建模为 canonical model reference。
- 辅助模型与主模型使用同一个 registry snapshot、credential context 和 route compiler。
- 若辅助模型不可用，应返回可配置的“禁用/回退到主模型”策略；不得手工构造 xAI 请求。

### 关闭标准

- 搜索不到生产代码中的手工 xAI auxiliary `ModelEntry`/Sampler 构造。
- 非 xAI Provider 可配置并运行辅助模型。

---

## OBPA-012 — Provider 配置回滚不是原子操作且忽略失败

**状态**：OPEN
**优先级**：P0

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/provider_config_coordinator.rs:253-266`

初始保存使用原子替换，但 registry commit 失败后的回滚使用 `std::fs::write()`，并以 `let _ =` 丢弃结果。崩溃或磁盘错误可能留下截断配置；调用方只看到 registry 错误，无法判断磁盘是否已恢复。

### 建议修复

- 回滚必须调用与正常保存相同的跨平台 `atomic_replace` primitive。
- 保存旧内容前记录文件存在性、权限与必要元数据；回滚失败返回组合错误并进入明确的 degraded state。
- 增加 commit 失败、回滚写失败、进程中断和 watcher 并发测试。

### 关闭标准

- 正向写入与回滚均满足同一原子持久化契约。
- 不存在忽略回滚 I/O 错误的代码。

---

# P1 — 高风险问题

## OBPA-013 — `PreparedSamplerConfig` 之后仍直接修改 `SamplerConfig.api_key`

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4788`
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs:1110,2401-2404`
- `crates/codegen/xai-grok-shell/src/agent/subagent/mod.rs:926`
- 其他读取/修改点分布于 ACP、sampler turn 与 subagent 链

请求准备后仍可直接替换密钥，使认证头、auth scheme、日志字段和 bearer resolver 可能不同步。Prepared contract 不能靠约定维持。

### 建议修复

- 将凭据刷新建模为重新执行 `prepare_sampler_config`，或建立类型安全的 credential refresh API。
- 使已准备配置中的认证材料不可直接公开写入。
- 清理下游通过 `.api_key` 判断认证类型、比较或记录前缀的逻辑。

---

## OBPA-014 — Sampler 仍公开接受未准备的 `SamplerConfig`

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

`SamplingClient::from_prepared()` 已存在，但 `Client::new(SamplerConfig)` 仍是广泛可用的生产入口。调用者可绕过 route compiler、header conflict 检查和 credential policy，类型系统未强制 prepared boundary。

### 建议修复

- 将低层 `new(SamplerConfig)` 限制为 crate-private、测试或显式 legacy 模块。
- 对外生产 API 只接受经过验证的 prepared/request plan 类型。
- 若受 crate 依赖环限制，移动公共 prepared DTO 到 sampling-types crate，而不是保留可绕过入口。

---

## OBPA-015 — “禁止手工构造”静态扫描可被 `Default + 字段修改` 绕过

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

PROGRESS 记录曾通过把 `SamplerConfig { ... }` 改为 `SamplerConfig::default()` 后逐字段赋值，使静态扫描得到零匹配，但语义上的旁路仍然存在。当前门禁只检测语法形态，不能证明请求来自 prepared chain。

### 建议修复

- 使用 API 可见性和类型边界阻止旁路，不依赖正则扫描结构体字面量。
- 静态门禁应扫描禁止调用的构造函数、可变字段写入和 legacy module 引用。
- 增加架构测试，证明所有生产 inference entrypoint 最终消费 `ResolvedModelExecution`。

---

## OBPA-016 — 请求级 Header Override 在生产路径固定为空

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-shell/src/agent/config.rs:4712-4716`

生产调用始终传入 `RequestHeaderOverrides::new()`。即使 header merge 类型与冲突校验存在，也没有请求级调用方能提供真实 override。

### 建议修复

- 把请求级 header override 作为明确的请求上下文参数传入。
- 区分 route static、provider extra、request override 和 auth header 四层，禁止用同一 map 混合。
- 增加冲突、覆盖、跨请求隔离 E2E。

---

## OBPA-017 — 凭据环境变量读取仍绕过统一 Credential Context

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/auth.rs:123,131,508-509`
- `crates/codegen/xai-grok-provider/src/providers/mod.rs:46`

部分路径直接调用 `std::env::var`，绕过 `EnvironmentReader`，导致测试注入、审计、环境变量优先级和密钥读取策略不统一。

### 建议修复

- 所有环境凭据只能由 `RequestCredentialContext.environment` 读取。
- Provider configure 阶段不得读取 secret value，只能登记候选变量名称。
- 增加测试确保 bootstrap/rebuild 不读取密钥，只有请求准备时读取。

---

## OBPA-018 — 未知 `kind` 会警告后回退为 Builtin

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/resolution.rs:88-110`

错误拼写或未来不支持的 `kind` 只记录 warning，然后按同名 builtin 解析。该行为可能把配置错误转换成错误 Provider 或延迟到更深层失败。

### 建议修复

- 未知 kind 必须产生配置错误并阻止 snapshot commit。
- 错误信息列出支持值和具体 Provider ID。

---

## OBPA-019 — 自定义 Provider 缺少 `kind` 时被静默视为 OpenAI-compatible

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/resolution.rs:105-114`

任意非内置 ID 在缺少 `kind` 时只 warning 并按 OpenAI-compatible 处理。这掩盖配置遗漏，也使 schema 难以演进。

### 建议修复

- V1 要求自定义 Provider 显式声明 `kind = "openai_compatible"`。
- 仅在专门的 legacy migration 层允许隐式推断，并生成一次性迁移诊断。

---

## OBPA-020 — 重复 Provider 配置只保留第一项而不失败

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/resolution.rs:118-145`

检测到重复 Provider 时只生成 diagnostic 并保留第一项。用户可能误以为后一个配置已覆盖，实际使用旧 endpoint 或密钥策略。

### 建议修复

- 重复 Provider ID 必须使配置解析失败。
- 若多来源合并是合法需求，应在进入 `resolve_provider_set` 前按明确优先级合并，不能在最终集合中静默丢弃。

---

## OBPA-021 — `FactoryProvider::defaults()` 在生产类型中为 `unimplemented!`

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/providers/openai_compatible_factory.rs:42-46`

Provider trait 的方法在生产对象上直接 `unimplemented!`。任何未来调用、诊断 UI 或通用 registry 逻辑访问 defaults 都会 panic。

### 建议修复

- FactoryProvider 保存完整 `ProviderDefaults` 并返回引用；或重构 trait，使 factory-created provider 不需要未实现方法。
- 禁止在生产 trait implementation 中保留 `unimplemented!`/`todo!`。

---

## OBPA-022 — Provider 配置 Schema 不拒绝未知字段

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

E2E 使用不存在的字段：

```toml
implementation = "openai-compatible"
```

正式字段实际为：

```toml
kind = "openai_compatible"
```

解析仍成功，说明未知字段被忽略。配置拼写错误可产生“加载成功但行为未改变”的假成功。

### 建议修复

- 对 Provider/Model 配置结构启用 `deny_unknown_fields` 或等价的显式未知键诊断。
- 在保留扩展字段需求时，建立命名空间化 extension map，而不是全局宽松解析。
- 修复所有文档和测试中的无效字段。

---

## OBPA-023 — Provider E2E 不是生产忠实链路

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `crates/codegen/xai-grok-shell/tests/test_provider_chain_e2e.rs:1-190`

测试手工创建 `ModelEntry`、手工传入 API key、直接调用 helper，并延期验证 Authorization header。它没有证明 TOML 中的 provider key、真实 model catalog、production URL override 或 session credential 链工作。部分 acceptance 名称（例如 Ollama）实际只覆盖通用 custom URL。

### 建议修复

- 测试必须从真实 App/ACP/launcher 入口创建会话并选择 canonical model。
- 密钥只能来自测试 TOML/env/session，不得作为 helper 参数手工注入。
- mock server 必须断言完整 URL、协议 body、所有认证/静态/额外 headers 和零错误请求。

---

## OBPA-024 — E2E 未覆盖异步 session、模型切换、热重载和 Catalog 生命周期

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

现有 E2E 通过同步测试函数和独立 runtime 执行，无法发现 OBPA-001；也没有覆盖 ACP new session、模型切换、配置保存/reload、catalog refresh、旧请求继续使用旧 snapshot 等真实组合路径。

### 建议修复

- 建立 Tokio multi-thread 生产入口 E2E。
- 覆盖 bootstrap → session → request、switch model、reload provider、catalog revision、并发旧会话与新会话。

---

## OBPA-025 — Windows 排除扫描器存在语法盲区

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `scripts/provider-v1/scan-platform-exclusions.py`

脚本报告零排除，但不能识别常见形式：

- `#[cfg(not(windows))]`
- `#[cfg(unix)]`
- `#[cfg(all(test, unix))]`
- `#[cfg_attr(...windows..., ignore)]`

独立扫描仍发现多处匹配，因此当前 exclusion ledger 的“归零”证据无效。

### 建议修复

- 使用 Rust 语法解析或覆盖完整 cfg 语法的 scanner，而不是单一正则。
- 输出稳定 ID、文件、行号、cfg 表达式、测试/生产分类和责任状态。
- 用 fixture 测试 scanner 对各种嵌套表达式的识别。

---

## OBPA-026 — 残留平台排除缺少逐项不适用证明或 Windows 对应测试

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

当前仍存在 LSP、PTY、process scope、worktree、shell completion、session 等测试/模块的 Windows 或 Unix 条件编译。部分可能是真正平台专属，但没有统一证据证明共享契约在 Windows 上已覆盖。

### 建议修复

- 对每个排除项分类为：真正 Unix-only、缺失 Windows implementation、跨平台契约测试缺失、无效排除。
- 真正 Unix-only 项保留 `cfg(unix)`，同时为跨平台接口增加 Windows capability/unsupported contract test。
- 禁止仅为通过 Windows CI 新增 ignore 或模块级排除。

---

## OBPA-027 — CI 尚不能证明三平台生产闭环

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `.github/workflows/provider-adapter.yml`

Windows/macOS 只运行少量 crate；Windows 缺少 Clippy、Shell/Agent、Provider chain E2E、PTY/ACP、安装包 smoke；Linux workspace job不运行 `cargo test --workspace --all-targets`。workflow 只针对 `feat/provider-adapter`，不能保证合并到主分支后的 gate。

### 建议修复

- 三平台运行 workspace check、Clippy、test build 和明确的行为测试矩阵。
- Linux 执行完整 workspace tests；Windows/macOS 至少执行所有跨平台 crate、Shell production E2E 和打包 smoke。
- workflow 同时保护目标主分支与发布 tag。

---

## OBPA-028 — Baseline workflow 全面 `continue-on-error`，不能作为门禁

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `.github/workflows/provider-v1-baseline.yml`

check、Clippy、test build、docs 等关键步骤均允许失败。该 workflow 适合采集历史失败，不应被引用为通过证据。

### 建议修复

- 明确将 baseline workflow 标记为非门禁、只产出 ledger。
- 生产 gate 必须 fail-fast/fail-closed；所有要求通过的命令不得 `continue-on-error`。

---

## OBPA-029 — 项目状态文档与当前代码事实冲突

**状态**：OPEN
**优先级**：P1

### 问题定位和描述

- `docs/provider-adapter-v1/final-audit.md`
- `PROGRESS.md`
- 旧 `ISSUE.md`

文档声称无阻塞发现函数、所有生产路径使用 route compiler、E2E 完整、Phase 14 接受矩阵完成；当前源码仍存在 legacy sampler、同步 block_on、配置丢失、无 catalog producer 和 E2E 旁路。错误的“完成”声明会指导代理跳过必要工作。

### 建议修复

- 本文件作为唯一 active issue source；历史审计必须标记审计 commit 与过期状态。
- `final-audit.md` 在所有 P0/P1 关闭前改名或显式标记为历史/未通过。
- PROGRESS 只记录事实与命令结果，不得把部分测试外推为生产闭环。

---

# P2 — 发布工程与维护性问题

## OBPA-030 — Clean-checkout 发布验证尚未完整执行

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

`final-audit.md` 明确记录完整 workspace test、format、docs 等验证仍为 pending/blocked。当前没有从全新 checkout 对最终 commit 运行完整 gate 的证据。

### 建议修复

- 在 Linux、Windows、macOS 干净 checkout 上使用锁定工具链和 `--locked` 重跑发布矩阵。
- 记录 commit SHA、runner、命令、退出码、测试数量与 artifact hash。

---

## OBPA-031 — `git diff --check` 失败，存在空白/换行污染

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

对 `origin/main...HEAD` 执行 `git diff --check` 失败，约二十余文件存在 trailing whitespace 或 CRLF 类差异。这会制造无意义 diff，并可能影响脚本和跨平台维护。

### 建议修复

- 只在独立提交中清理空白，不与功能修复混合。
- 增加 `.gitattributes` 与 CI `git diff --check` 门禁，明确文本文件换行策略。

---

## OBPA-032 — 当前 RC Tag 已严重落后于 HEAD

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

当前 HEAD 相比 `provider-adapter-v1-rc.1` 已有大量提交和文件变化，旧 tag 不能代表当前产品形态，也不能作为回滚或验收基线。

### 建议修复

- 所有 P0/P1 关闭并通过 clean-checkout gate 后重新打不可变 RC tag。
- 发布说明列出迁移、已知限制、测试矩阵和 artifact checksums。

---

## OBPA-033 — 大型测试目标的磁盘、PDB 与链接资源预算未工程化

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

历史上 Windows 大型测试目标曾因磁盘/PDB/链接压力失败；当前解决方式主要是清理磁盘。随着 workspace 增长，这仍会使 CI 不稳定并诱导代理跳过完整测试。

### 建议修复

- 对单元、集成、PTY E2E 和发布 smoke 分层，但保持总门禁完整。
- 配置 CI 缓存、target 清理、测试 shard 和足够磁盘；不得以永久跳过替代资源治理。

---

## OBPA-034 — Provider factory 使用全局 dead-code 许可掩盖未完成逻辑

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

- `crates/codegen/xai-grok-provider/src/providers/openai_compatible_factory.rs:1`

文件级 `#![allow(dead_code)]` 掩盖未使用的 profile 字段、`profile_meta()` 等逻辑，与 protocol/model format 未接入生产的问题一致。

### 建议修复

- 完成 OBPA-007 后删除文件级 allowance。
- 对确有暂存用途的单个条目使用最小范围注解并附删除条件。

---

## OBPA-035 — Provider 核心 API 的 deprecated/legacy 标记与生产用途不一致

**状态**：OPEN
**优先级**：P2

### 问题定位和描述

部分 route/compiler helper 被标记 deprecated 或 migration-only，却仍由主要生产路径调用；同时注释把实际生产 fallback 描述为待删除阶段任务。API 状态不准确会误导后续维护者。

### 建议修复

- 完成唯一生产链迁移后删除 legacy API。
- 对仍为生产权威的 API 移除错误的 deprecated 标记，并写明稳定契约。
- 禁止长期保留“阶段编号式 TODO”作为运行时行为说明。

---

# 优先级执行顺序

1. **异步与错误边界**：OBPA-001、OBPA-002。
2. **配置保真与通用 Provider**：OBPA-003、004、005、007、017、018、019、020、021、022。
3. **请求权威**：OBPA-006、010、011、013、014、015、016。
4. **运行时一致性**：OBPA-008、009、012。
5. **生产忠实测试**：OBPA-023、024。
6. **跨平台证明**：OBPA-025、026、027、028。
7. **发布与文档治理**：OBPA-029～035。

# V1 发布门禁

V1 RC 只能在以下条件同时满足时创建：

- 所有 P0、P1 为 `CLOSED`；
- P2 全部关闭，或每个延期项均有用户批准的版本目标与不影响生产正确性的证据；
- 三平台 clean-checkout CI 通过；
- Provider production E2E 从真实会话入口验证 URL、协议、认证、headers、热重载、Catalog 与失败闭合；
- `git diff --check`、format、workspace check、Clippy、workspace tests、docs 和发布 smoke 全部通过；
- 新 RC tag 指向完成上述验证的同一 commit。
