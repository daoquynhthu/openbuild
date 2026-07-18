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
