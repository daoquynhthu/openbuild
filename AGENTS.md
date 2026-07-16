# Agent Workspace Rules — Grok Build Model Adapter Refactoring

> 本文档是代理工作的权威要求。所有自动生成的代码和行为必须遵循以下规则。

---

## 1. 项目状态

- **分支**: `feat/provider-adapter`
- **架构文档**: `docs/model-adapter-architecture.md` — 目标架构的唯一权威参考
- **实施计划**: `docs/implementation-plan.md` — 施工路线图，包含 Phase 划分子任务、测试要求、门禁
- **本文件**: `AGENTS.md` — 代理工作规范，包含代码风格、工作流、质量要求
- **语言**: Rust (edition 2024, toolchain 1.92.0)
- **Rust 格式化**: `rustfmt.toml` 仅开启 `use_field_init_shorthand = true`，其余全默认

---

## 2. 文件规范

### 2.1 路径规则

- 新类型/结构体必须放在已有的 crate 中（通常是 `xai-grok-provider`）
- Provider 实现文件放 `xai-grok-provider/src/providers/<name>.rs`
- 测试文件放对应 crate 的 `tests/` 目录或源文件内的 `#[cfg(test)] mod tests`
- **禁止**在根目录或 docs/ 之外新建文件

### 2.2 模块导出规则

- 新 crate 在 `lib.rs` 中用 `pub mod` 导出模块
- `mod.rs` 或目录模块用 `pub mod` 链式导出
- 禁止使用 `#[path = "..."]` 属性（除非文件已在现有代码中使用）

---

## 3. 代码风格

### 3.1 通用规则

- **不要添加注释**，代码应是自解释的
- 使用 `use_field_init_shorthand = true`（已配置）
- 变量命名使用 `snake_case`，类型使用 `PascalCase`
- 枚举变体使用 `CamelCase`
- 函数参数类型标注，返回类型标注

### 3.2 错误处理

- 使用 `thiserror` 定义错误类型
- Error 枚举变体加 `#[error("...")]` 和 `#[from]`
- 函数返回 `Result<T, E>`，E 为自定义错误类型
- 避免裸 `unwrap()` 和 `expect()`，改用 `?` 操作符

### 3.3 可见性与封装

- `pub` 只暴露必要的 API
- struct 字段默认私有，必要时提供 `pub` getter
- 内部模块（如 `protocols/`、`providers/`）对外表现为 `pub(crate)`
- 为所有 `pub` 项添加 doc comment（`///` 或 `//!`）

### 3.4 序列化

- 使用 `serde` 的 `#[derive(Serialize, Deserialize)]`
- 枚举序列化使用 `#[serde(rename_all = "snake_case")]`
- `Option` 字段加 `#[serde(default, skip_serializing_if = "Option::is_none")]`
- 新类型加 `#[non_exhaustive]` 保护

### 3.5 异步

- 使用 `tokio` 运行时，`#[async_trait]` 用于 trait 方法
- 异步函数使用 `async fn`，而非手动 `Pin<Box<dyn Future>>`
- `Stream` 类型使用 `futures::Stream` 或 `tokio_stream::StreamExt`

### 3.6 禁止的模式

| 禁止 | 替代方案 | 原因 |
|------|----------|------|
| `std::fs::canonicalize` | `dunce::canonicalize` | Windows `\\?\` 路径问题 |
| `std::path::Path::canonicalize` | `dunce::canonicalize` | 同上 |
| `tokio::fs::canonicalize` | `spawn_blocking + dunce` | 同上 |
| `use crate::` 当跨 crate 引用 | 使用 crate 名称 | 避免模块耦合 |
| `impl Into<String>` 参数 | `impl Into<String>` 允许 | 但优先用 `&str` + `.to_owned()` |
| 裸 `unwrap()` 在生产代码 | `?` 或 `.context()` | 仅测试代码允许 |

---

## 4. 工作流规则

### 4.1 每次操作前

- 读取 `docs/implementation-plan.md` 确认当前 Phase 和子任务
- 读取 `docs/model-adapter-architecture.md` 确认类型定义和接口设计
- 如果是修改已有文件，先 `git diff` 查看当前变更

### 4.2 每次操作后

- 运行 `cargo check -p <affected-crate>` 确认编译
- 运行 `cargo clippy -p <affected-crate>` 确认零新警告
- 如果涉及编译测试，运行 `cargo test -p <affected-crate>`
- `git add -A && git commit -m "phase-N: <msg>"`

### 4.3 commit 规范

```
<type>: <简短描述>

<可选：详细说明>
```

types:
- `phase-N`: 对应实施计划的 Phase 编号
- `feat`: 新功能
- `refactor`: 重构
- `test`: 测试
- `docs`: 文档
- `chore`: 杂项（CI、配置）

### 4.4 禁止

- 跳过 `cargo check` 直接 commit
- 在非 `feat/provider-adapter` 分支提交
- 修改 `docs/model-adapter-architecture.md` 或 `docs/implementation-plan.md` 而不先确认用户的意图
- 引入不在 `Cargo.toml` 中的第三方依赖而不先确认

---

## 5. 质量门禁

执行以下命令序列作为"门禁检查"：

```powershell
# 1. 编译
cargo check --workspace

# 2. 代码风格
cargo clippy --workspace -- -D warnings

# 3. 测试
cargo test --workspace

# 4. 文档（无缺失文档警告）
cargo doc --no-deps 2>&1 | Select-String "warning"
```

在以下几种情况必须运行门禁检查：
- 每个 Phase 完成后
- commit 前（至少运行 `cargo check` 和 `cargo clippy`）
- 引入新的第三方依赖时

---

## 6. 引用文档

以下文档是权威参考，代理应随时查阅：

| 文档 | 内容 | 优先级 |
|------|------|--------|
| `docs/model-adapter-architecture.md` | 目标架构定义（13 个章节） | ★★★ |
| `docs/implementation-plan.md` | 实施计划和任务分解（8 个 Phase） | ★★★ |
| `AGENTS.md` | 代理工作规范（本文） | ★★ |
| `clippy.toml` | Clippy 配置和禁止的模式 | ★★ |
| `Cargo.toml` | 工作区成员和依赖版本 | ★ |

---

## 7. 响应格式

当代理完成任务时，应至少返回以下信息：
- 完成了哪个 Phase/子任务
- 修改了哪些文件（路径列表）
- 运行的关键命令结果（`cargo check` / `cargo clippy` 状态）
- 如果有问题无法解决，描述阻塞原因
