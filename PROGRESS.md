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
