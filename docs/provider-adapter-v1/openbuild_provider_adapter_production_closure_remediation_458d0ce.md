# Provider Adapter V1 — Production Closure Remediation Guide

> **适用基线**：`feat/provider-adapter` @ `458d0ce00e8024a5f56915a5d0575ae951b1f2e4`
>
> **权威关系**：`ISSUE.md` 是问题编号、优先级、状态和关闭标准的唯一权威来源。本文件只为 OBPA-001～OBPA-012 及其直接依赖提供实施指导，不创建新问题，不得用本文件中的步骤完成度替代 ISSUE 状态。

## 1. 使用规则

1. 严格按本文阶段顺序实施，不得并行修改共享请求链。
2. 每个提交只处理一个 OBPA 编号；必要的机械迁移可拆成多个提交，但不得混入其他问题。
3. 发现本文未覆盖的前置缺陷时：
   - 停止当前实现；
   - 在 `ISSUE.md` 新增编号；
   - 标明它阻塞的原问题；
   - 经审查后再修改本文顺序。
4. 不得通过以下方式“解决”问题：
   - 增加 `block_on`、线程或第二 runtime；
   - `panic!`、`.unwrap()`、`.ok()`、忽略错误；
   - 恢复 legacy fallback；
   - 手工向 `SamplerConfig` 填入 URL、协议或密钥；
   - 新增 Windows ignore/cfg；
   - 只修改测试以符合现有错误行为。
5. 每阶段结束必须运行该阶段门禁；失败时不得进入下一阶段。

## 2. 冻结目标架构

### 2.1 单一生产链

```text
ProviderConfig sources
  ├─ built-in defaults/profile defaults
  ├─ legacy migration
  ├─ TOML
  └─ startup CLI overrides
          ↓
ResolvedProviderSet
          ↓  prepare + validate, no I/O secrets
RegistrySnapshot (atomic revision)
          ↓
Canonical ModelRef(provider/model)
          ↓
ResolvedModelExecution
  ├─ request URL
  ├─ protocol
  ├─ route/static/provider headers
  ├─ auth policy/candidates
  ├─ model/generation/limits
  └─ provider credential reference
          ↓ async request-time resolution
RequestCredentialContext + RequestHeaderOverrides
          ↓
PreparedSamplerConfig
          ↓
SamplingClient
```

### 2.2 明确禁止的第二权威

以下对象不得再决定 endpoint、protocol 或 auth：

- `ModelEntry.info.base_url`
- `ResolvedCredentials.base_url`
- `ApiBackend` 推导协议
- 裸 `SamplerConfig`
- xAI auxiliary fallback
- 同步模型发现 helper

### 2.3 配置字段保真表

| 输入字段 | Snapshot/Route 消费者 | 失败策略 |
|---|---|---|
| `base_url` | endpoint builder | 非法/不安全 URL 阻止 commit |
| `protocol` | route builder | 未知协议阻止 commit |
| `api_key` | provider inline secret | 仅请求时解析 |
| `env_key` | auth candidates | 空列表且 required 时请求失败 |
| `extra_headers` | provider header layer | 非法名称/值阻止 commit |
| `allow_insecure_http` | endpoint policy | 仅显式允许时接受 HTTP |
| `model_list_path` | catalog source | 格式不合法阻止 commit |
| `model_list_format` | catalog decoder | 未知格式阻止 commit |

任何字段如果在 V1 不支持，必须在解析时显式拒绝，不能接受后忽略。

---

# Phase A — 建立可失败的真实生产回归测试

**覆盖问题**：OBPA-001～012 的测试前置。

## A1. 增加生产入口测试夹具

创建仅用于测试的 launcher fixture，必须通过真实 App/ACP session 构建，不得直接调用 `execution_to_sampler_config()`。

最小用例：

1. TOML 定义自定义 Provider；
2. bootstrap App；
3. 创建 ACP session；
4. 选择 `provider/model`；
5. 发出推理请求；
6. mock server 检查 URL、body、headers；
7. 切换模型；
8. 修改配置并触发 reload；
9. 再发请求验证 revision 与优先级。

## A2. 先写失败测试

必须先得到以下可重复失败：

- Tokio multi-thread session 创建触发 nested runtime 问题；
- Provider inline key 没有出现在 Authorization/x-api-key；
- custom protocol 被固定为 Chat Completions；
- CLI base URL override 在 reload 后丢失；
- Catalog 没有刷新 revision；
- auxiliary model 绕过 Provider；
- registry commit 失败后模拟回滚写失败。

## A3. 门禁

```bash
cargo test -p xai-grok-shell --test provider_production_chain -- --test-threads=1
```

期望：测试编译，且每个目标缺陷对应的测试以明确断言失败；不能以 panic/timeout 作为模糊失败证据。

---

# Phase B — 端到端异步化与结构化错误

**关闭目标**：OBPA-001、OBPA-002。

## B1. 冻结异步接口

把以下同步接口替换为 async `Result`：

```rust
async fn prepare_sampling_config_for_model(...) -> Result<SamplerConfig, AgentError>;
async fn resolve_aux_model_sampling_config(...) -> Result<Option<SamplerConfig>, AgentError>;
```

更理想的最终返回值是 `PreparedSamplerConfig` 或 `SamplingClient`，但本阶段不得同时进行大规模类型迁移；先消除 runtime 嵌套和 panic。

## B2. 自底向上迁移

顺序必须是：

1. `prepare_sampler_config` 上游 wrapper；
2. `sampling_config_for_model_with_registry` 调用点；
3. auxiliary resolver；
4. model switch；
5. ACP new session；
6. App/launcher 边界。

每改一个调用层，立即运行该模块测试。禁止保留“同步兼容 wrapper”供生产调用。

## B3. 错误映射

建立稳定错误分类：

- `ProviderConfig`
- `ProviderNotFound`
- `ModelNotFound/Ambiguous`
- `Endpoint`
- `Protocol`
- `Credential`
- `Catalog`
- `Persistence`
- `InternalInvariant`

所有错误输出必须经过 secret redaction。

## B4. 阶段门禁

```bash
rg -n 'Handle::current\(\)|Runtime::new\(\)' crates/codegen/xai-grok-shell/src/agent
rg -n 'panic!\(' crates/codegen/xai-grok-shell/src/agent/{config.rs,mvp_agent,handlers}
cargo test -p xai-grok-shell --lib -- agent
cargo test -p xai-grok-shell --test provider_production_chain -- --test-threads=1
```

第一条在 production Provider 链必须零匹配；第二条不得包含配置/认证驱动 panic。

---

# Phase C — 无损 Provider 配置管线

**关闭目标**：OBPA-003、OBPA-004。

## C1. 定义唯一 resolved config

扩展 `ProviderRuntimeConfig`，保留：

```rust
pub struct ProviderRuntimeConfig {
    pub public: ProviderPublicConfig,
    pub inline_api_key: Option<SecretValue>,
    pub env_keys: Vec<String>,
}

pub struct ProviderPublicConfig {
    pub base_url: Option<BaseUrl>,
    pub protocol: Option<ProtocolId>,
    pub model_list_path: Option<String>,
    pub model_list_format: Option<ModelListFormat>,
    pub allow_insecure_http: bool,
    pub extra_headers: HeaderMap,
}
```

具体类型可遵循仓库现有封装，但不得退回无验证 `String` map。

## C2. 删除平行转换

- 找出 `ProviderConfigInput::into_runtime_config()` 等未使用或重复转换。
- 选择一个权威函数并让 TOML、legacy、CLI 全部调用。
- 其他转换删除，不保留 dead code。

## C3. Registry 无损 configure

`ProviderRegistry::prepare()` 必须把完整配置传给内置 definition 或 generic factory。Provider 必须报告已消费字段；未消费字段使 prepare 失败。

## C4. 字段矩阵测试

每个字段至少包含：

- 默认值；
- TOML 值；
- CLI 覆盖；
- reload 后值；
- 无效值；
- 两 Provider 隔离。

## C5. 阶段门禁

```bash
cargo test -p xai-grok-provider --all-targets
cargo test -p xai-grok-shell --lib -- provider_bootstrap
cargo test -p xai-grok-shell --lib -- provider_config
```

并检查不存在硬编码 `model_list_format: None` 或只传 `id/base_url` 的 configure。

---

# Phase D — 通用 OpenAI-compatible Provider

**关闭目标**：OBPA-007；为 OBPA-005、006、009 提供基础。

## D1. Profile 仅提供默认值

处理顺序：

```text
factory baseline
< profile defaults
< legacy migration
< TOML
< CLI
```

profile 不得硬编码成为唯一 env-key 或协议来源。

## D2. Route builder

按 resolved protocol 生成：

- Chat Completions：`/chat/completions`
- Responses：`/responses`
- 其他 V1 明确支持协议

路径应与用户 base URL 安全 join。未知协议直接报错。

## D3. Auth builder

认证候选包含 provider inline、用户 env keys、profile env defaults；是否 required 由 auth policy 决定。Factory configure 阶段不读取环境变量值。

## D4. Header 与 Catalog source

- extra headers 进入独立 provider layer；
- model list path/format 构造明确的 model source；
- 未配置动态模型目录时，必须使用静态模型列表或显式“不支持 discovery”，不能产生不可执行 Dynamic 标记。

## D5. 删除 panic/dead code

实现 `defaults()` 或重构 trait；删除文件级 `allow(dead_code)`。

## D6. 阶段门禁

使用两个独立 mock server、两个 Provider ID、不同协议、不同 env key、不同 headers、不同模型目录进行隔离 E2E。

---

# Phase E — 请求 URL、凭据与 Header 的唯一准备层

**关闭目标**：OBPA-005、OBPA-006；直接降低 OBPA-013、014、016、017 风险。

## E1. 删除 post-compile URL override

`resolve_model_execution()` 只根据 snapshot route 产出完整 `RequestUrl`。CLI base URL 必须在 Phase C 配置解析时进入 Provider config。

## E2. 组装 Credential Context

请求准备调用必须提供：

```text
request override
model inline
provider inline
environment reader
session resolver
```

不得从旧 `ResolvedCredentials` 反推 Provider policy。

## E3. 四层 Header merge

固定顺序并检测冲突：

1. route static；
2. provider extra；
3. request override；
4. auth header。

认证头不能被普通 override 静默替换。

## E4. 禁止准备后修改

本阶段至少建立 deprecation/visibility barrier，使新代码无法写 `SamplerConfig.api_key`；完整删除在后续唯一 Sampler API 阶段执行。

## E5. 阶段门禁

mock server 必须断言：

- 完整路径；
- 认证头和值；
- provider extra header；
- request override；
- 冲突时零 HTTP 请求；
- secret 不出现在 Debug/Display/log。

---

# Phase F — 热重载与 Catalog 生命周期

**关闭目标**：OBPA-008、OBPA-009。

## F1. 保存 Startup Resolution Context

ProviderRuntime 或 coordinator 持有 immutable：

- legacy migration inputs；
- CLI Provider overrides；
- process-level security policy。

每次 reload 使用同一 context 重新解析。

## F2. 事务重建

```text
read file
→ parse strict schema
→ resolve with preserved context
→ prepare registry candidate
→ validate catalog sources
→ atomic registry commit
→ schedule catalog delta refresh
```

任一步失败，旧 registry/catalog revision 保持不变。

## F3. Catalog worker

实现：

- bootstrap 持久快照读取；
- changed-provider delta refresh；
- bounded concurrency；
- connect/total timeout；
- cancellation；
- TTL；
- 同源重定向与认证剥离策略；
- 原子 snapshot persistence。

## F4. 会话一致性

明确旧会话使用固定 snapshot 还是读取最新 revision。V1 建议每次新请求读取当前 snapshot，但同一请求内不可变化；模型切换必须重新 resolve。

## F5. 阶段门禁

- CLI override reload 保留测试；
- 新增/删除/修改 Provider 的 catalog delta 测试；
- refresh 失败旧 snapshot 可用；
- 并发 reload 不产生半状态。

---

# Phase G — 删除 Legacy 与 xAI 专用旁路

**关闭目标**：OBPA-010、OBPA-011。

## G1. Canonical model migration

所有持久化新值统一为 `provider/model`。裸模型只由明确 legacy table 迁移一次；歧义必须报错。

## G2. 删除 legacy sampler fallback

- 删除 production `sampling_config_for_model()` 的 fallback 职责；
- registry 缺失和 provider_id 缺失直接错误；
- 将旧模型配置导入一个显式 legacy Provider definition。

## G3. 辅助模型统一

Image describe、classifier、compaction、trace、web-search 等均由配置中的 canonical model ref 解析。回退到主模型必须是显式策略，不得手工绑定 xAI。

## G4. 收紧 Sampler API

完成 OBPA-013～015 的类型边界：生产构造只接受 prepared plan，低层 config 构造限制为内部/测试。

## G5. 阶段门禁

```bash
rg -n 'use legacy path|P7-003|sampling_config_for_model\(' crates/codegen/xai-grok-shell/src
rg -n 'ProviderId::new\("xai"\)|ApiBackend::Responses' crates/codegen/xai-grok-shell/src/agent
```

匹配必须逐项证明为 xAI Provider 实现或 legacy migration，而非请求旁路。

---

# Phase H — 原子配置保存与回滚

**关闭目标**：OBPA-012。

## H1. 单一 atomic replace primitive

正常保存、rollback、catalog snapshot、session persistence 使用同一跨平台 primitive。

## H2. 组合错误

如果 registry commit 失败且 rollback 也失败，返回包含两者的错误，标记 coordinator degraded，并阻止继续保存，直到重新读取磁盘并完成一致性恢复。

## H3. 故障注入

必须覆盖：

- temp write 失败；
- fsync 失败；
- replace 失败；
- registry commit 失败；
- rollback replace 失败；
- watcher 在 replace 中间触发；
- Windows 文件占用/共享冲突。

## H4. 阶段门禁

Linux 与 Windows 都运行真实文件系统测试；不得只用内存 mock 证明原子语义。

---

# Phase I — 跨平台与发布闭环

**关联问题**：OBPA-023～035；详细状态仍以 ISSUE.md 为准。

## I1. 修复 exclusion scanner

先让 scanner 正确列出所有 cfg/ignore，再逐项修复或证明不适用。scanner 为零之前不得宣称 Windows 排除归零。

## I2. 三平台 CI

最低门禁：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
```

若 Windows/macOS 因资源必须分 shard，所有 shard 的并集必须等于完整测试集合，且任何 shard 失败使 workflow 失败。

## I3. 生产 E2E 矩阵

至少覆盖：

- OpenAI Chat；
- OpenAI Responses；
- Anthropic Messages；
- OpenCode no-auth；
- Ollama model discovery + inference；
- 两个任意命名 compatible Provider；
- missing auth/invalid endpoint/unknown protocol 零请求；
- hot reload；
- catalog refresh；
- ACP session/model switch；
- Windows package smoke。

## I4. Clean checkout 与 RC

所有测试从新 clone 执行，`git diff --check` 为零，文档状态与代码一致后才允许新 RC tag。

---

# 3. 问题—阶段映射

| Issue | 处理阶段 |
|---|---|
| OBPA-001 | B |
| OBPA-002 | B |
| OBPA-003 | C |
| OBPA-004 | C |
| OBPA-005 | E |
| OBPA-006 | E |
| OBPA-007 | D |
| OBPA-008 | F |
| OBPA-009 | F |
| OBPA-010 | G |
| OBPA-011 | G |
| OBPA-012 | H |
| OBPA-013～017 | E/G |
| OBPA-018～022 | C/D |
| OBPA-023～024 | A/I |
| OBPA-025～028 | I |
| OBPA-029～035 | I |

# 4. 不冲突规则

1. 若本指南与 `ISSUE.md` 的优先级或关闭标准不一致，以 `ISSUE.md` 为准。
2. 本指南中的阶段完成不自动关闭 Issue；必须在 `ISSUE.md` 附上证据并单独改为 `CLOSED`。
3. 本指南不得添加“可延期”“不可操作”结论。延期只能在 `ISSUE.md` 经用户批准记录。
4. 修复导致问题边界变化时，先更新 `ISSUE.md` 描述，再同步本指南的问题—阶段映射。
5. `PROGRESS.md`、历史 final audit 和 CI artifact 只能作为证据来源，不能覆盖这两份文档。
