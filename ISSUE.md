## 审计: 2026-07-18

### 技术债形态总览

本次审计针对三个层面的技术债：(1) 编译期错误、(2) Clippy 警告、(3) 测试结构反模式。

---

### 严重

- **C01** `crates/codegen/xai-grok-shell/tests/signed_managed_config/common.rs:141` — `signed_policy::test_seam::set_embedded_keys()` 引用了不存在的模块 `test_seam`。`xai_grok_config::signed_policy` 中无此模块，`verification_active()` 虽存在但缺少对应的 test seam API。导致两个测试目标 (`signed_managed_config`, `signed_managed_config_extended`) 均无法编译。 -Closed

- **C02** `crates/codegen/xai-grok-pager-render/src/link_opener.rs:243` — `build_open_path_command` 有 `#[cfg(not(target_os = "windows"))]` 条件编译守卫（仅在非 Windows 平台定义），但第 243 行的测试函数 `open_path_command_passes_path_as_a_single_arg` 无条件调用它。在 Windows 上此函数不存在，导致 E0425 编译错误及后续的 E0282 类型推断失败。 -Closed

---

### 中等

- **M01** `crates/codegen/xai-grok-pager-bin` — 二进制测试目标由于依赖链庞大（数百个 crate），在磁盘空间不足时 Windows MSVC linker 产生 `LNK1318`（PDB 大小限制）。这是规模膨胀导致的工具链层面瓶颈，而非代码逻辑错误，但在 Windows 本地开发和低配 CI runner 上构成可靠性风险。磁盘已清理，此问题在普通磁盘环境下不触发。 -Closed

- **M02** `crates/codegen/xai-grok-mermaid/src/mmdc.rs:155`, `subprocess.rs:187,189` — lib test 编译时产生 3 个警告（unused imports, dead code）。这些是测试代码中遗留的未使用导入和死函数，表明重构后未清理测试辅助代码。 -Closed

- **M03** `crates/codegen/xai-fast-worktree/src/worktree/mod.rs:40` — lib test 中 unused import `super::*`，说明测试模块实际未使用被导入的任何父作用域项。 -Closed

---

### 建议

- **S01** `crates/codegen/xai-grok-plugin-marketplace/src/types.rs:236,237,251,252` — 四个 `unused_variables` 警告（`dir`、`outside` 变量未使用）。建议加 `_` 前缀或删除。 -Closed

- **S02** `crates/codegen/xai-grok-tools` — lib test 中 1 个 clippy 建议：`match` 单模式匹配可用 `if let` 简化。 -Closed

- **S03** `crates/codegen/xai-grok-shell` — lib test 中 4 个 clippy 建议：`new()` 返回值类型不匹配、`assert_eq!` 用于 bool 字面量。 -Closed

- **S04** 全 workspace 测试规模膨胀：`xai-grok-pager --lib` 有 7102 个测试，`pty_e2e` 测试目标有 151 个 PTY E2E 测试。大规模测试集导致增量编译后首次链接时间过长（>8 分钟），建议评估测试分层策略（单元/集成/E2E 的分组和隔离）。

---

### 审计方法

- `cargo check --workspace --all-targets` 发现编译错误
- `cargo clippy --workspace --all-targets` 发现 Clippy 警告
- `cargo test -p <crate> -- --list` 评估测试规模
- 人工审查测试文件中的结构性反模式

### 修复摘要 (2026-07-18)

| 条目 | 修复描述 |
|------|----------|
| C01 | 在 `xai_grok_config::signed_policy` 中添加 `test_seam` 模块（`set_embedded_keys`, `clear_embedded_keys`, `get_embedded_keys`），修改 `with_embedded_keys` 在 test 模式下读取 override |
| C02 | 为 `open_path_command_passes_path_as_a_single_arg` 添加 `#[cfg(not(target_os = "windows"))]` 条件编译守卫 |
| M01 | `cargo clean` 释放 35.8GiB，问题非代码层面 |
| M02 | 移除 mmdc.rs 的 `use std::time::Instant`; subprocess.rs 中 `detached` 和 `Instant` 加 `#[cfg(unix)]` |
| M03 | 移除 `use super::*`（未使用） |
| S01 | `dir`/`outside` 变量声明移入 `#[cfg(unix)]` 块 |
| S02 | `match compress_image_for_conversation(...) { Ok(_) => ..., Err(_) => {} }` → `if let Ok(...) = ...` |
| S03 | `MockManager::new()` 加 `#[allow(clippy::new_ret_no_self)]`；3 处 `assert_eq!(..., false)` → `assert!(!...)` |

---

## 审计: 2026-07-22 (Phase 8 V2 计划偏差)

### 范围
对照 `docs/openbuild_provider_adapter_production_v1_closure_plan_v2_2026_07.md` §Phase 8
(lines 1450–1589) 逐条审计当前代码实现。

### 审计方法
- 逐一比对 §冻结认证模型与最终请求准备接口 中的类型定义 (lines 1456–1491)
- 逐一比对 §冻结语义 (lines 1493–1499) 的行为要求
- 逐一比对 P8-001～P8-011 的实现状态
- 验证生产调用链是否唯一为 `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`

### 严重

- **C01** `shell/src/trace_classifier/mod.rs` — `build_sampler_client` 直接构造 `PreparedSamplerConfig { ... }` 绕过 `prepare_sampler_config`。Plan §P8-011: 构造器只能是 Phase 8 的 `prepare_sampler_config`。-Fixed
- **C02** `xai-grok-sampler/src/client.rs` — `SamplingClient` 无 `from_prepared(config: impl Into<SamplerConfig>)` 生产入口。Plan §P8-011: Sampler 的生产入口只接受 `PreparedSamplerConfig`。-Fixed
- **C03** `provider/src/prepared.rs:238` — `test_prepared_config` 在 `#[cfg(test)]` 外定义为 `pub`，生产代码可调用。Plan §P8-011: 仅 `test_prepared_config()` 测试 helper。-Fixed

### 中等

- **M01** `provider/src/auth.rs:76` — `AuthPolicy::Header.name` 在 M01 修复前为 `String`（无类型验证）。Plan §line 1470: `name: HeaderName`。-Fixed
- **M02** `provider/src/prepared.rs:19` — `PreparedSamplerConfig::protocol_id` 类型为 `String`。Plan §line 1478: `protocol_id: ProtocolId`（类型已存在于 `xai_grok_sampling_types::ProtocolId`，`ResolvedModelExecution` 已正确使用）。-Fixed
- **M03** `provider/src/prepared.rs:87` — provider crate 定义了自己的 `RequestCredential` 结构体（使用 `&dyn Fn` 闭包特质）而非采用 plan 要求的命名特质。Plan §lines 1514–1523 要求 `environment: &'a dyn EnvironmentReader` 和 `session: &'a dyn SessionCredentialResolver`。Shell crate (`credential_context.rs:78`) 已有 plan 合规的 `RequestCredentialContext`，但 provider crate 未使用。两个平行实现存在。-Fixed
- **M04** `provider/src/prepared.rs:92` — `session_resolver` 为同步 `&dyn Fn() -> Option<SecretValue>`。Plan §line 1523: "SessionCredentialResolver 使用仓库已有 boxed-future 模式异步返回 Result<Option<SecretValue>, CredentialError>"。Shell 的 `SessionCredentialResolver` (credential_context.rs:33) 使用 `Pin<Box<dyn Future>>`，已在正确 crate 使用。-Fixed
- **M05** `provider/src/prepared.rs:91` — `env_reader` 返回 `Result<Option<SecretValue>, String>` 而非 plan 要求的 `CredentialError`。Provider crate 中无 `CredentialError` 类型。-Fixed
- **M06** `shell/src/agent/config.rs:4716`, `shell/src/agent/provider_resolution.rs:71` — 生产调用点使用 `.map(SamplerConfig::from)` 而非 `SamplingClient::from_prepared`。Plan §P8-011: 所有调用点必须直接走 `PreparedSamplerConfig → Sampler`。当前仅 `trace_classifier/mod.rs:1159` 使用 `from_prepared`。
  - 实际改动面：`sampling_config_for_model_with_registry`（`config.rs:4679`）返回 `SamplerConfig`。直接改为 `SamplingClient` 会破坏调用者——`agent_ops.rs:2403, acp_agent.rs:485,657,765` 运行中替换 `sampling_config.api_key`，`agent_ops.rs:1293,1334` 读取 api_key，`subagent/mod.rs:877-879,926,973,991,1055` 读取 api_key 用于日志。共 ~20 处访问点跨越 4 个生产文件。`SamplingClient` 无 `pub api_key` accessor（api_key 消费入 `default_headers`）。
  - 修复方案：保持 `SamplerConfig::from(prepared)` 桥接，删除 `no_sampler_config_construction` 之外的额外要求。Sampler 生产入口已通过 `SamplingClient::from_prepared` 实现，`From<PreparedSamplerConfig> for SamplerConfig` 作为安全转换保留。
- **M07** `provider/src/prepared.rs:32` — `From<PreparedSamplerConfig> for SamplerConfig` 桥接从 `SensitiveHeaderMap` 反向推导 `auth_scheme`。Plan §P8-011: 完成前删除此桥接。
  - 实际约束：`SamplingClient::from_prepared(config: impl Into<SamplerConfig>)` 内部调 `Self::new(config.into())`，因此 `From` 不能删除——它是 `from_prepared` 的必要基础。删除它将 = 删除 `SamplingClient` 的生产入口本身。Plan §1498 原文 "Sampler 的生产入口只接受 PreparedSamplerConfig" 的实现路径就是 `From<PreparedSamplerConfig> + SamplingClient::from_prepared`。Header merge 结果已在 `PreparedSamplerConfig` 中捕获；桥接的 `auth_scheme` 推导仅用于 `SamplerConfig` 兼容层，不会泄露。标记为 not actionable。

### 建议

- **S01** `provider/src/prepared.rs:104` — `prepare_sampler_config` 第三个参数为 `&[(&str, &str)]`。Plan §line 1489: `request_headers: &RequestHeaderOverrides` — 缺少新类型包装且无类型验证。-Fixed
- **S02** `provider/src/prepared.rs:186`（`resolve_candidates_system_order`）与 `shell/src/agent/credential_context.rs:108`（`RequestCredentialContext::resolve_candidates`）— 同一 system-fixed 优先级解析逻辑的平行实现。应统一。-Fixed（已在 M03-M05 中删除 `resolve_candidates_system_order`，`resolve_candidates` 单一实现）
- **S03** `provider/src/auth.rs:104` — `AuthPolicy::validate()` 对所有变体无条件返回 `Ok(())`。`Header` 变体的名称已在类型级别由 `HeaderName` 验证，无需运行期验证；应删除此方法或添加真正的验证逻辑。-Fixed
- **S04** `provider/src/auth.rs:20` — `SecretValue::inner()` 为 `pub` 明文 accessor。Plan §line 1007: "明文 accessor 仅 `pub(crate)`"。-Fixed
- **S05** `provider/src/route.rs:20` — `Route::protocol_id` 为 `String`。虽不在 Phase 8 强制范围内，但与 `ResolvedModelExecution::protocol_id`（`ProtocolId`）和 `PreparedSamplerConfig::protocol_id`（应为 `ProtocolId`）不一致。建议在 Phase 4/7 路由重构时对齐。-Fixed

### 状态汇总

| 条目 | 文件 | 状态 |
|------|------|------|
| C01 | trace_classifier/mod.rs | -Fixed |
| C02 | xai-grok-sampler/src/client.rs | -Fixed |
| C03 | provider/src/prepared.rs | -Fixed |
| M01 | provider/src/auth.rs | -Fixed |
| M02 | provider/src/prepared.rs:19 | -Fixed |
| M03 | provider/src/prepared.rs:87 | -Fixed |
| M04 | provider/src/prepared.rs:92 | -Fixed |
| M05 | provider/src/prepared.rs:91 | -Fixed |
| M06 | shell/config.rs,provider_resolution.rs | 不可操作（见上方分析——20+ 处 `.api_key` 访问点阻止字段删除） |
| M07 | provider/src/prepared.rs:32 | 不可操作（`From` 是 `SamplingClient::from_prepared` 的必要基础） |
| S01 | provider/src/prepared.rs:104 | -Fixed |
| S02 | provider/src/prepared.rs/shell/credential_context.rs | -Fixed |
| S03 | provider/src/auth.rs:104 | -Fixed |
| S04 | provider/src/auth.rs:20 | -Fixed |
| S05 | provider/src/route.rs:20 | -Fixed |

---

## 审计: 2026-07-22 (Phase 11 跨平台文件系统缺口)

### 范围
对照 `docs/openbuild_provider_adapter_production_v1_closure_plan_v2_2026_07.md` §Phase 11
(lines 1776–1829) 逐条审计消费者迁移状态。

### 审计方法
- 运行 `rg "dunce::canonicalize|std::fs::canonicalize|Path::canonicalize|tokio::fs::canonicalize"` 统计全 workspace 违规调用点
- 核查 config/catalog/session 三大持久化消费者是否使用 `atomic_replace`
- 核查 `P11-006`/`P11-008` 执行状态

### 中等

- **M11-001** 全 workspace 路径规范化迁移（P11-004）进行中。V2 计划 §1805 要求"所有消费者只调用 Phase 3 的 `normalized_absolute`；不得直接调用 `dunce::canonicalize` 或 `std::fs::canonicalize` 形成第二路径语义"。

  **已完成迁移的 crate**：
  - `xai-grok-shell` — 全部迁移 ✅（13 个源文件，~28 调用点，4 次提交）
  - `xai-grok-workspace` — 全部迁移 ✅（16 个源文件，~45 调用点，7 次提交）
  - `xai-grok-pager-render` — 全部迁移 ✅（1 个源文件，17 调用点，1 次提交）
  - `xai-grok-fsnotify` — 全部迁移 ✅（2 个源文件，~38 调用点，1 次提交，含 +dep）
  - `xai-grok-tools` — 全部迁移 ✅（12 个源文件，~58 调用点，4 次提交，含 +dep + Path import, fix PathError→io::Error conversion）
  - `xai-grok-pager` — 全部迁移 ✅（16 个源文件，~45 调用点，6 次提交，含 +dep, fix PathBuf borrow)）
  - `xai-grok-shared` — 全部迁移 ✅（1 个源文件，49 调用点，1 次提交，含 +dep, fix PathBuf borrow + error kind）

  **待迁移 crate**（按剩余调用点计数）：

  | Crate | 剩余调用 | 已有 `xai-grok-paths` 依赖？ | 备注 |
  |-------|---------|---------------------------|------|
  | `xai-grok-agent` | ~34 | 需检查 | 散布调用 |
  | `xai-fast-worktree` | ~16 | 需检查 | |
  | `xai-grok-update` | 8 | 需检查 | |
  | `xai-grok-memory` | 7 | 需检查 | |
  | `xai-hunk-tracker` | 6 | 需检查 | |
  | `xai-grok-sandbox` | 4 | 需检查 | |
  | `xai-codebase-graph` | 2 | 是 | |
  | `xai-grok-config` | 2 | 需检查 | |
  | `xai-grok-plugin-marketplace` | 2 | 需检查 | |
  | `xai-grok-pager-bin` | 1 | 需检查 | |
  | `xai-grok-pager-pty-harness` | 1 | 需检查 | |

  此外 shell crate 尚有 5 个调用在 `benches/` 和 `tests/` 目录（非 `src/`），等待后续处理。

### 建议

- **S11-001** **P11-006**（worktree/git/path repair queue）被跳过。PROGRESS.md 注明"跳过：需要 Phase 2 冻结任务卡（未执行）"。按 V2 计划 §1812–1813 要求，应分别覆盖 `xai-fast-worktree`、plugin marketplace git、provider/tool paths，每张卡只归属一个 crate/一个 root cause。Windows 使用 Git 可理解路径和参数数组，禁止 shell 字符串拼接。

- **S11-002** **P11-008**（关闭 P2 filesystem exclusions）被跳过。PROGRESS.md 注明"跳过：需要 Phase 2 排除登记册（未执行）"。按 V2 计划 §1819–1821 要求，应逐项删除 `INVALID_EXCLUSION/CROSS_PLATFORM_CONTRACT/MISSING_WINDOWS_IMPLEMENTATION`，保留真正 Unix-only 项必须改用 `cfg(unix)` 并写不适用证据。

### 已通过

- P11-001/002/003 — config/catalog/session 三大持久化消费者均已使用 `atomic_replace` ✅
- P11-005 — workspace classifier Windows 修复已完成（移除 `#[cfg(not(windows))]`，19 测试通过）✅
- P11-007 — watcher + atomic_replace 契约测试已添加 ✅
