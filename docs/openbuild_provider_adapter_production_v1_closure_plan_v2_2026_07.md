# OpenBuild Provider Adapter V1 真正生产闭环实施计划（V2）

> **For agentic workers:** 本文件是从当前 `feat/provider-adapter` 工作区状态推进到 Provider Adapter V1 生产闭环的唯一执行权威。必须严格按任务 ID 顺序，一次只执行一个工作包。不得跳过、合并、替换、重解释或自行“优化”本计划。任何未预见前置条件都必须触发停线报告，不得由代理自行改变路线。

**Goal:** 将当前“新 Provider 架构与旧执行链并存、跨平台测试大量被排除”的中间态，收敛为一个供应商中立、单运行时权威、请求链可证明、配置可热重载、模型目录异步可靠，并在 Linux、Windows、macOS 上通过真实行为测试和发布门禁的第一版生产系统。

**Architecture:** 全进程只允许一个注入式 `ProviderRuntime`。配置输入经唯一 precedence resolver 形成 `ResolvedProviderSet`，事务性发布为 `RegistrySnapshot`；模型选择经唯一 route compiler 生成不含 secret 的 `ResolvedModelExecution`；请求准备器在发送前用 `RequestCredentialContext` 解析认证并产出唯一可执行的 `PreparedSamplerConfig`；Sampler 只接受该准备结果，不再根据旧字段、品牌名或 URL 重新推导行为。模型发现由异步 `ProviderCatalogService` 管理，启动路径不得联网。文件持久化、进程、信号、终端、PTY 和路径行为均通过平台适配层实现共同契约，禁止用整模块 `cfg(not(windows))` 代替实现。

**Tech Stack:** Rust 1.92.0，edition 2024，Tokio，reqwest 0.12，serde，toml/toml_edit，indexmap，现有 mock/PTY 测试设施，GitHub Actions。除非仓库所有者书面批准，不得增加第三方依赖。

---

## 0. 为什么必须使用这份 V2 计划

上一版计划存在三个执行层缺陷，本版逐项消除：

1. **任务包过大。** 弱代理把多个根因、多个文件群和多个测试层放进一个子任务，随后以“任务过于复杂”为理由跳过部分。本计划规定每个工作包只允许一个可验证行为改变，默认最多修改 3 个生产文件和 2 个测试文件。
2. **代理替换计划。** 弱代理自行判断某项“不必要”“可后置”或“已有替代方案”，从而改变架构。本计划引入不可变计划、任务状态机、偏差报告和禁止替代条款。代理只有执行权，没有重新规划权。
3. **隐藏前置步骤。** 上一版直接要求全工作区门禁，但仓库预存大量编译、Clippy、测试及跨平台失败，导致门禁实际不可执行。本计划先建立三平台真实基线和失败账本，再采用“任务级无新增回归 → 子系统归零 → 平台归零 → 发布归零”的分层门禁。所有预存失败仍必须在发布前关闭，不能以“pre-existing”永久豁免。

---

# 1. 文档权威与不可变规则

## 1.1 权威顺序

执行本计划时，文档优先级如下：

1. 本文件；
2. 本文件冻结的架构决策和接口；
3. `docs/model-adapter-architecture.md` 中与本文件不冲突的部分；
4. `AGENTS.md`；
5. 当前测试中与上述条款不冲突的行为；
6. `docs/openbuild_provider_adapter_production_v1_plan_2026-07-17.md`，仅作为历史记录；
7. `docs/implementation-plan.md`、`PROGRESS.md`、`ISSUE.md` 和 `docs/provider-adapter-v1/final-audit.md`，仅作为历史证据，不得作为完成声明。

执行代理不得修改本计划。只有仓库所有者可以发布 V2.x 修订版。若代码现实与本计划冲突，代理必须停线，不得自行修改计划以适配代码。

## 1.2 允许的执行分支

- 必须在包含 `.git` 的真实 checkout 中工作；压缩包解压目录不能直接执行本计划。
- 默认分支：`feat/provider-adapter`。
- 若所有者指定新分支，必须从当前目标提交创建，并在基线文件中记录 base SHA。
- 禁止 `git init`、禁止重写历史、禁止 force push、禁止删除未提交用户改动。

## 1.3 供应商中立性

- xAI 可以有专有 Provider 实现和 OAuth/session resolver，但不得拥有专有执行架构。
- 除 `providers/xai.rs`、xAI auth adapter 和明确标记的 legacy migration 模块外，通用层不得以 `provider_id == "xai"` 改变路由、模型解析、缓存、重试、UI、持久化或错误语义。
- 新写入的模型引用必须统一为 `provider/model`。
- 未知裸模型必须报歧义或未找到。只有被明确识别为旧 xAI persisted state 的数据可以迁移为 `xai/model`。
- `openai-compatible` 不得默认读取 `XAI_API_KEY`。

---

# 2. V1 生产闭环的精确定义

只有以下全部成立，才能称为 V1：

## 2.1 配置闭环

```text
built-in implementation defaults
< selected profile defaults
< legacy migration input
< [provider.*] / [model.*] TOML
< startup CLI configuration overrides
→ typed diagnostics
→ deterministic ResolvedProviderSet
```

- 所有错误带配置路径；不得 `filter_map(...ok()?)` 静默丢弃。
- 环境变量在此阶段只合并“候选变量名”，不读取 secret value。
- request credential override 不属于配置解析；它只在 Phase 8 的单次请求上下文中生效。
- 优先级有单一实现和单一测试矩阵。
- 任意命名的 OpenAI-compatible provider 可同时存在多个实例。

## 2.2 运行时闭环

```text
ResolvedProviderSet
→ ProviderRuntime::rebuild
→ one immutable RegistrySnapshot revision
→ shared launcher/TUI/session/reloader/catalog identity
```

- 启动、TUI、CLI、session、subagent、reloader、catalog 必须持有同一个 `Arc<ProviderRuntime>`。
- 成功 rebuild 恰好增加一次 revision；失败保持旧快照和 revision 不变。

## 2.3 请求闭环

```text
provider-qualified model
→ configured provider
→ selected route
→ Endpoint::render
→ ResolvedModelExecution (no secret)
→ prepare_sampler_config(execution, RequestCredentialContext)
→ request-time credential resolution
→ merged validated headers
→ PreparedSamplerConfig
→ Sampler
→ protocol decoder
```

- Provider-bound 请求不得进入 legacy sampler fallback。
- 未知协议、无效 URL、缺失认证、header 冲突均为 typed hard failure。
- `ResolvedModelExecution` 只包含已解析的 provider/route/protocol/URL/defaults/policy，不含 secret 或最终认证 header。
- `PreparedSamplerConfig` 是生产 Sampler 的唯一输入，包含最终 URL、最终 headers、模型与生成参数；其构造器只能是 Phase 8 的 `prepare_sampler_config`。
- Sampler 不得根据品牌、host、`api_backend` 或旧 endpoint 字段重新推导路由。

## 2.4 模型目录闭环

- 启动只读取内存/磁盘快照，不做同步网络请求。
- 后台刷新有并发上限、connect/total timeout、status check、auth、stale fallback、真实 TTL、取消和 revision event。
- 持久化保留模型内容，使用真实 wall-clock timestamp 和 Windows-safe 原子替换。

## 2.5 UI/CLI 闭环

- `/providers` 和 `providers` CLI 使用同一个 runtime view。
- 保存只经过一个 effect/transaction；写盘成功后 rebuild，失败时 UI 不宣称成功。
- UI 展示 configured、credential source status、catalog state、model count 和 revision，而不是把“definition 已注册”误称为“已配置”。

## 2.6 跨平台闭环

- Linux、Windows、macOS 都通过 workspace check、Clippy、测试和 package smoke。
- 平台无关行为测试在三平台运行。
- 平台差异通过接口和对应 contract test 表达，不得通过整模块排除隐藏。
- 所有当前新增的 Windows 排除逐条分类并处理；最终不允许“临时缺失 Windows 实现”项。

### V1 冻结平台支持矩阵

| 能力 | Linux | macOS | Windows |
|---|---|---|---|
| Provider/runtime/config/catalog | 必须支持 | 必须支持 | 必须支持 |
| Session persistence/hot reload | 必须支持 | 必须支持 | 必须支持 |
| ACP/session lifecycle | 必须支持 | 必须支持 | 必须支持 |
| PTY | native PTY | native PTY | `portable-pty`/ConPTY |
| Process tree termination | process group/signals | process group/signals | repository Windows Job Object abstraction |
| Active command shell | POSIX shell | POSIX shell | `Pwsh`、Windows PowerShell、Git Bash、`cmd.exe`，按现有 `ShellKind` 契约 |
| Interactive path completion | POSIX quoting | POSIX quoting | 四种 `ShellKind` 分别测试 quoting/replacement |
| Generated CLI completion script | Bash/Zsh/Fish/Elvish/PowerShell 中现有支持项 | 同左 | PowerShell；`cmd.exe` 无生成脚本，CLI 必须显式拒绝而不是伪成功 |
| Clipboard/link opening | native adapter | native adapter | native adapter |

`bash` 名称的现有 tool 在 V1 中解释为“通过 active `ShellKind` 执行命令”的兼容 API；Windows 必须走 PowerShell/Git Bash/cmd 对应 adapter，不得假装 POSIX Bash，也不得从 registry 隐藏该通用命令工具。只有真正依赖 POSIX utility 的子能力可以 capability-gate，并且 UI/description 必须与实际能力一致。

## 2.7 发布闭环

- clean checkout、locked dependency、无公网测试、文档零 warning、安装/启动/配置迁移/mock 请求 smoke 全绿。
- 重新静态审计 A-01～A-18 全部 Closed。
- 不允许保留“43 个已知失败”“docs 因磁盘跳过”等发布例外。

---

# 3. 当前工作区的审计基线

本计划从 `openbuild-feat-provider-adapter(1).zip` 对应源码状态出发。执行时必须用真实 Git checkout 重新确认，不得把下表当作动态验证结果。

| ID | 当前问题 | 必须关闭阶段 |
|---|---|---|
| A-01 | launcher 未 `ProviderRuntime::rebuild`，route compiler 失败后回退 legacy | Phase 6、7 |
| A-02 | provider `api_key/env_key/extra_headers` 未成为请求权威 | Phase 4、8 |
| A-03 | `fetch_provider_models_blocking` 仍在生产路径 | Phase 9 |
| A-04 | ConfigReloader 未注入 runtime | Phase 10 |
| A-05 | TUI 使用独立 legacy registry，保存不 rebuild | Phase 10 |
| A-06 | 任意 OpenAI-compatible provider/profile 不可达 | Phase 4 |
| A-07 | Catalog 并发、timeout、status、auth、stale、persist、cancel 均有缺陷 | Phase 9 |
| A-08 | `Inline/Public` auth 语义不成立 | Phase 8 |
| A-09 | route compiler 绕过 `Endpoint::render` | Phase 7 |
| A-10 | 149 行 Windows 排除，50 个文件，行为未证明 | Phase 2、11～13 |
| A-11 | CI/E2E 绕过真实生产链，分支/tag 覆盖不足 | Phase 14、15 |
| A-12 | Registry 三锁、并发 revision、未知 config、selector 校验不完整 | Phase 5 |
| A-13 | 未知裸模型全局回退 xAI；compatible 默认 XAI key | Phase 4 |
| A-14 | signed-policy integration seam 可能只修编译未修行为 | Phase 3 |
| A-15 | Provider TOML 解析静默丢错 | Phase 4 |
| A-16 | TUI 非原子写入且双保存路径 | Phase 10、11 |
| A-17 | `check.ps1` 可在 Clippy 失败时输出全绿 | Phase 3 |
| A-18 | 文档与真实代码状态失真 | Phase 1、16 |

已知结构事实：

- 工作区约 80 个成员、83 个 `Cargo.toml`；
- 当前源码中存在 `ProviderRuntime`、`ProviderCatalogService`、`RegistrySnapshot` 和 route compiler；
- `main.rs` 仍调用 `configure_providers()`；
- `xai-grok-pager`、`providers_cmd` 仍可自行创建 registry；
- 当前 CI Windows job 未运行 Clippy、shell tests、PTY/package smoke；
- 当前 `check.ps1` 默认跳过关键 crate，并把 Clippy 失败设为非致命；
- 当前跨平台差分至少新增 149 行 `not(target_os = "windows")` 条件。

---

# 4. 弱代理强制行为协议

## 4.1 工作包大小上限

每个任务卡默认必须同时满足：

- 只关闭一个根因或一个平台行为；
- 只新增一个失败测试组；
- 最多修改 3 个生产文件；
- 最多修改/新增 2 个测试文件；
- 最多一个 focused commit；
- 不允许“顺手清理”相邻模块；
- 若需要超过限制，本计划必须明确写出例外。代理不得自行扩大。

## 4.2 一次只执行一个任务

任务状态机：

```text
NOT_STARTED
→ PRECHECKED
→ RED_REPRODUCED
→ IMPLEMENTED
→ TARGET_GATE_GREEN
→ BASELINE_DELTA_CHECKED
→ COMMITTED
→ EVIDENCE_RECORDED
```

不得跨状态。上一任务没有 `EVIDENCE_RECORDED`，不得开始下一任务。

## 4.3 动态修复队列必须先机械展开并冻结

P2、P3、P11、P12、P13 中由 baseline/exclusion ledger 生成的队列，不允许代理边修边决定任务边界。进入对应 phase 前必须：

1. 运行 `scripts/provider-v1/materialize-work-packets.py`；
2. 以 ledger 的稳定排序生成每个任务卡，写入对应 `phase-NN.md`；
3. 每张卡固定：ledger ID、一个首因 fingerprint、一个 production symbol 或一个 test function、允许文件、精确 red command、精确 green command、前置任务 ID；
4. 记录 ledger SHA-256 和生成文件 SHA-256；
5. phase 执行期间不得修改已生成任务卡。新首因只能写 deviation，由所有者发布计划补丁或追加经过批准的新 ledger row。

唯一允许合并多个失败 ID 的条件是：它们的首个 causal diagnostic 在去除绝对路径和行号后字节完全相同，并且指向同一 production symbol。满足条件也必须由脚本生成一个 foundation task，代理不能口头认定“同一根因”。

## 4.4 每个任务开始前必须执行

```bash
git status --short
git rev-parse HEAD
git diff --stat
git diff -- <task-owned-files>
```

并确认：

- 当前 HEAD 与上一任务记录一致；
- 没有未知用户改动；
- 任务前置 gate 已通过；
- 本任务修改范围与任务卡一致。

## 4.5 禁止代理自行判断的事项

代理不得：

- 宣称某任务“已经等价完成”而不执行任务指定测试；
- 用另一实现替换冻结接口；
- 把失败标记为 pre-existing 后永久跳过；
- 新增 `#[ignore]`、`#[cfg(not(target_os = "windows"))]`、`#[allow]` 或缩小 CI 范围来获得绿色；
- 修改测试预期以适配错误实现；
- 删除失败测试；
- 把同步网络、全局 registry 或 legacy fallback 留作“临时兼容”；
- 因任务复杂而只完成其中一部分后标记完成；
- 修改本计划、任务顺序、冻结架构或验收标准；
- 在没有动态输出时宣称 workspace、Windows 或 release gate 通过。

## 4.6 发现隐藏前置条件时

必须停止当前任务，创建 `docs/provider-adapter-v1/deviations/<TASK-ID>-blocked.md`，内容固定为：

```markdown
# <TASK-ID> Blocked

## Exact command
<command>

## Exit code
<code>

## Minimal error excerpt
<first causal error, not entire log>

## Reproduction
<deterministic steps>

## Why this prerequisite is not already assigned
<reference plan section and evidence>

## Files inspected
<paths only>

## Proposed owner decision
- amend plan with a new prerequisite work packet; or
- confirm an existing packet owns the failure.
```

创建报告后停止。不得自行实施“建议修复”。

## 4.7 三次失败停线

同一任务最多允许三个不同、可证伪的根因假设。第三次实现尝试仍失败时，必须：

- 恢复到任务开始 commit；
- 保留测试/log evidence；
- 写 `<TASK-ID>-blocked.md`；
- 停止执行。

不得叠加第四个补丁。

## 4.8 Commit 规则

格式：

```text
<task-id>: <single observable behavior>
```

例如：

```text
P6-006: reject provider route errors without legacy fallback
```

提交前：

- `git diff --check`；
- 任务指定格式、check、Clippy、测试；
- baseline delta 脚本；
- `git diff --name-only` 必须完全属于任务卡允许范围。

禁止把 `PROGRESS.md` 与源代码放在同一提交。每个 phase gate 后单独提交 phase evidence。

---

# 5. 门禁体系：先真实基线，再逐层归零

## 5.1 G0：基线捕获门禁

G0 不要求仓库绿色；要求失败可复现、可编号、可归属。

- 三平台命令和日志完整；
- 每个失败有 ledger ID；
- 每个 ledger ID 映射到本计划具体任务；
- 未知失败数为 0；
- 没有因命令提前停止而遗漏后续 crate。

## 5.2 G1：任务门禁

每个任务必须：

- 新增测试先红后绿；
- affected crate `cargo check --all-targets` 通过；
- affected crate `cargo clippy --all-targets -- -D warnings` 通过；
- 指定测试通过；
- 与 G0 相比不得新增失败、ignore、platform exclusion、allow 或 warning。

如果 affected crate 在 G0 已有其他失败，必须使用精确 test target/feature 证明本任务通过，并由 baseline delta 工具确认失败集合没有扩大。不得把 affected crate 整体失败当作跳过测试的理由。

## 5.3 G2：子系统门禁

一个 phase 结束时，其所有目标 crate 必须零失败。不能带着“已记录失败”离开负责该子系统的 phase。

## 5.4 G3：平台门禁

- Linux platform ledger = 0；
- Windows platform ledger = 0；
- macOS platform ledger = 0；
- Windows exclusion ledger 中不允许 `missing implementation` 或 `platform-neutral test disabled`；
- 真正 Unix-only 测试必须有契约说明和 Windows counterpart/明确不适用证据。

## 5.5 G4：发布门禁

三平台 clean checkout 上：

```text
fmt 0 failure
workspace check --all-targets 0 failure
workspace clippy --all-targets -D warnings 0 failure
workspace tests 0 failure
provider real-chain E2E 0 failure
PTY smoke 0 failure
package/install smoke 0 failure
docs 0 warning
ignored production tests 0
```

任何历史失败、磁盘跳过、平台跳过、手工验证待办均使 G4 失败。

---

# 6. 计划产物和证据目录

执行过程中必须产生以下版本化文件：

```text
docs/provider-adapter-v1/execution-v2/
├── baseline-environment.md
├── baseline-failure-ledger.md
├── baseline-command-matrix.md
├── architecture-contract.md
├── provider-neutrality-audit.md
├── windows-test-exclusion-ledger.md
├── cross-platform-contract-matrix.md
├── release-gate-matrix.md
├── final-audit-v2.md
└── phases/
    ├── phase-00.md
    ├── ...
    └── phase-17.md

artifacts/provider-v1/                 # CI artifact，不提交二进制/巨型日志
├── linux/
├── windows/
└── macos/
```

仓库只提交精简证据摘要、命令、exit code、失败 ID 和 artifact URL/commit SHA；不得提交完整 `target/`、密钥或超过仓库策略的大日志。

---

# 7. 阶段依赖图

```text
P0 Environment
 └─ P1 Truthful baseline + task materializer
     └─ P2 Per-exclusion classification + frozen repair cards
         └─ P3 Gate infrastructure + compile/clippy debt
             └─ P4 Typed config + generic identities
                 └─ P5 Transactional registry
                     └─ P6 Single runtime bootstrap
                         └─ P7 Route/endpoint authority
                             └─ P8 Auth/header authority
                                 └─ P9 Async catalog
                                     └─ P10 TUI/CLI/reload closure
                                         └─ P11 Filesystem/path portability
                                             └─ P12 Process/ACP/terminal portability
                                                 └─ P13 Pager/tools portability
                                                     └─ P14 Real production-chain E2E
                                                         └─ P15 CI/package/release gates
                                                             └─ P16 Dead-code/docs cleanup
                                                                 └─ P17 Independent final audit + RC
```

P11～P13 必须顺序执行：P12 消费 P11 的 path/atomic contracts，P13 消费 P11 的路径能力和 P12 的 shell/process/PTY 能力。禁止并行推进这三个阶段。P14 必须等待 P13 G2 通过。

---

# Phase 0 — 恢复可执行、可复现的真实工作环境

**目标：** 消除“压缩包无 Git”“工具链不完整”“磁盘不足”“不同平台命令不一致”等隐藏前置条件。

## P0-001：验证真实 Git checkout

**Files:** 无修改。

**Commands:**

```bash
git rev-parse --is-inside-work-tree
git branch --show-current
git rev-parse HEAD
git status --short
git remote -v
```

**Pass:** work tree 为 true；分支符合授权；记录 HEAD；用户改动已识别。`.git` 缺失立即停线，禁止 `git init`。

## P0-002：记录三平台环境契约

**Files:** Create `docs/provider-adapter-v1/execution-v2/baseline-environment.md`。

分别记录：OS build、CPU arch、Rust host、Rust 1.92.0、Cargo、protoc、Git、PowerShell/Bash、可用磁盘、locale、默认 shell、Windows long-path 状态。

**Commands:**

Linux/macOS：

```bash
uname -a
rustc -Vv
cargo -V
rustup show active-toolchain
protoc --version
git --version
df -h .
locale
```

Windows PowerShell：

```powershell
Get-ComputerInfo | Select-Object WindowsProductName,WindowsVersion,OsBuildNumber,OsArchitecture
rustc -Vv
cargo -V
rustup show active-toolchain
protoc --version
git --version
Get-Volume | Select-Object DriveLetter,SizeRemaining,Size
[System.Globalization.CultureInfo]::CurrentCulture.Name
```

**Pass:** 三平台均有证据。只拥有单平台环境时，必须通过 GitHub Actions 取得另外两平台证据，不得猜测。

## P0-003：固定工具链和依赖锁

**Files:** 只在实际不一致时修改 `rust-toolchain.toml`；默认不修改。

**Commands:**

```bash
rustup toolchain install 1.92.0 --profile minimal --component clippy,rustfmt
cargo fetch --locked
cargo metadata --locked --format-version 1 > artifacts/provider-v1/cargo-metadata.json
```

**Pass:** `Cargo.lock` 无变化；metadata 成功；禁止切换 stable 最新版绕过错误。

## P0-004：建立磁盘和 linker 防护

**Files:** Create `docs/provider-adapter-v1/execution-v2/baseline-command-matrix.md`。

规则：

- 任一平台执行全工作区 `check/clippy/test/doc/package` 前，当前 `CARGO_TARGET_DIR` 所在卷可用空间必须 ≥45 GiB；不足时必须先将 `CARGO_TARGET_DIR` 指向满足阈值的卷并把实际路径/空间写入环境证据。未满足阈值不得启动全量命令，也不得以磁盘不足把失败标记为代码失败。
- CI 使用独立 job/shard，禁止把所有平台/所有测试链接到单一 Windows job。
- Windows PDB/LNK1318 不能用删除测试解决；应通过 job 分片、release debuginfo 设置仅限 CI test profile、target dir 清理或 runner 容量解决。
- 任何清理命令前记录 `git status`，只允许删除 `target/` 和明确的 CI cache。

## P0-005：创建证据目录和忽略规则

**Files:** Modify `.gitignore`；Create execution-v2 directories。

只忽略 `artifacts/provider-v1/**`，保留 `.gitkeep` 或 README。不得忽略 baseline ledger、测试报告摘要或 deviations。

## P0-006：捕获初始静态指纹

**Files:** Create `docs/provider-adapter-v1/execution-v2/static-fingerprint.md`。

**Commands:**

```bash
rg -n "configure_providers|register_route|store_config|fetch_provider_models_blocking|sampling_config_for_model_with_registry" crates > artifacts/provider-v1/legacy-paths.txt
rg -n '#\[cfg\(.*not\(target_os = "windows"\)' crates > artifacts/provider-v1/windows-exclusions.txt
rg -n '#\[ignore' crates > artifacts/provider-v1/ignored-tests.txt
rg -n '#\[allow\(' crates > artifacts/provider-v1/allow-attributes.txt
rg -n '\b(unwrap|expect)\(' crates/codegen/xai-grok-{provider,shell,sampler,pager,pager-bin} > artifacts/provider-v1/unwrap-expect.txt
```

摘要记录数量和 SHA-256。此任务不修代码。

### Phase 0 Gate

- P0-001～P0-006 全部完成；
- 工具链、锁文件、三平台执行通道可用；
- 没有源代码修改；
- 证据文件可追溯到 base SHA。

---

# Phase 1 — 建立真实三平台失败账本，而不是假定绿色

**目标：** 把所有预存编译、Clippy、测试、文档、PTY 和 package 失败显式化，并绑定到后续工作包。

## P1-001：创建诊断型 baseline workflow

**Files:** Create `.github/workflows/provider-v1-baseline.yml`。

要求：

- `workflow_dispatch` only；不得作为发布绿色门禁；
- matrix：ubuntu-latest、windows-latest、macos-latest；
- 每个命令独立 step，`continue-on-error: true`，最终 job 仍上传全部日志；
- 使用 Rust 1.92.0、protoc、`--locked`、`--all-targets`、`--keep-going`；
- 上传 `cargo metadata`、check、clippy、test list、test result、disk before/after；
- 不访问真实 provider 网络；
- 不使用 secrets。

## P1-002：Linux workspace check 基线

**Command:**

```bash
cargo check --workspace --all-targets --locked --keep-going 2>&1 | tee artifacts/provider-v1/linux/check.log
```

把每个独立 root error 写入 ledger，格式 `BL-LNX-CHECK-###`。

## P1-003：Windows workspace check 基线

**Command:**

```powershell
cargo check --workspace --all-targets --locked --keep-going 2>&1 | Tee-Object artifacts/provider-v1/windows/check.log
```

编号 `BL-WIN-CHECK-###`。不得只运行 provider 相关 4 个 crate。

## P1-004：macOS workspace check 基线

与 P1-002 相同，编号 `BL-MAC-CHECK-###`。

## P1-005：三平台 Clippy 基线

三平台执行：

```bash
cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings
```

分别编号 `BL-<OS>-CLIPPY-###`。Clippy 失败必须是 fatal evidence，禁止像当前 `check.ps1` 一样降级。

## P1-006：测试可编译性与测试列表基线

先执行：

```bash
cargo test --workspace --all-targets --locked --no-run --keep-going
cargo test --workspace --all-targets --locked -- --list
```

将“测试目标无法编译”和“测试运行失败”分开编号。

## P1-007：分 shard 运行测试并收集全部失败

不得依赖一次 `cargo test --workspace` 在首个大目标失败后结束。按以下 shard：

1. provider/sampler/config/auth；
2. shell/session/ACP；
3. pager/pager-render/PTY；
4. tools/workspace/file/process；
5. 其余 workspace。

每个 shard 记录 package、test target、test name、platform、exit code、首个 causal assertion/panic。

## P1-008：文档与 package 基线

```bash
cargo doc --workspace --no-deps --locked
cargo package -p xai-grok-provider --allow-dirty --locked
cargo package -p xai-grok-sampler --allow-dirty --locked
cargo build -p xai-grok-pager-bin --release --locked
```

这里只记录基线，不宣称发布成功。

## P1-009：建立 `baseline-failure-ledger.md`

每条必须包含：

```text
ID | platform | command | package/target | failure class | first causal error |
reproducible | owner task | status
```

允许的 failure class：

- compile-contract；
- clippy；
- unit-behavior；
- integration-behavior；
- platform-adapter；
- test-harness；
- resource/tooling；
- docs/package。

禁止使用“misc”“pre-existing unknown”。

## P1-010：把每个失败绑定到具体任务

- 已知 Provider 架构失败绑定 P4～P10；
- filesystem/path 绑定 P11；
- process/ACP/terminal 绑定 P12；
- pager/tools/render 绑定 P13；
- E2E/CI/package 绑定 P14～P15；
- 未能绑定的失败使 Phase 1 gate 失败。

## P1-011：实现任务卡机械生成器

**Files:** Create `scripts/provider-v1/materialize-work-packets.py`; Create `scripts/provider-v1/tests/test_materialize_work_packets.py`。

固定 CLI：

```text
Phase 2 classification cards: exclusion ledger only
python scripts/provider-v1/materialize-work-packets.py \
  --mode classify \
  --exclusions docs/provider-adapter-v1/execution-v2/windows-test-exclusion-ledger.md \
  --phase 2 \
  --output docs/provider-adapter-v1/execution-v2/phases/phase-02-classify.md

Phase 3 repair cards: baseline only
python scripts/provider-v1/materialize-work-packets.py \
  --mode repair \
  --baseline docs/provider-adapter-v1/execution-v2/baseline-failure-ledger.md \
  --phase 3 \
  --output docs/provider-adapter-v1/execution-v2/phases/phase-03.md

Phase 11-13 repair cards: baseline + classified exclusion ledger
python scripts/provider-v1/materialize-work-packets.py \
  --mode repair \
  --baseline docs/provider-adapter-v1/execution-v2/baseline-failure-ledger.md \
  --exclusions docs/provider-adapter-v1/execution-v2/windows-test-exclusion-ledger.md \
  --phase <11|12|13> \
  --output docs/provider-adapter-v1/execution-v2/phases/phase-<NN>.md
```

算法必须：

- 解析结构化表格，缺字段或重复 ledger ID 直接失败；`classify/Phase 2` 只接受 exclusions，`repair/Phase 3` 只接受 baseline，`repair/Phase 11～13` 两者都必需；
- 按 owner phase、prerequisite、file、line、test/symbol、ledger ID 稳定排序；
- 默认每个 ledger row 生成一张任务卡；
- 只有 normalized causal fingerprint 与 production symbol 都完全相同时才生成显式 foundation group；
- 输出精确 ownership、red/green command 和 baseline-delta command；
- 在文件头写输入 SHA-256，不覆盖含不同 SHA 的既有任务文件；
- `--check` 模式验证生成结果未被手工修改。

测试 fixture 必须覆盖 classify/repair 两种模式、稳定输出、重复 ID、缺字段、非法跨 phase owner、错误合并、SHA mismatch 和 `--check` tamper detection。

### Phase 1 Gate

- 三平台全命令均有日志；
- 每个失败有唯一 ID；
- ledger 无未归属条目；
- 不允许“43 failed, documented”式汇总；必须列出 43 个测试名或由生成器按严格 fingerprint 形成的精确 foundation group；
- task materializer tests 全绿，Phase 3 工作包已生成并记录输入/输出 SHA；
- 此 phase 不修产品源代码，只建立真相基线和执行工具。

---

# Phase 2 — 每一处跨平台排除独立分类并冻结修复任务

**目标：** 把当前至少 149 行 Windows exclusion 转换为稳定 ID、逐条分类、逐条修复的不可遗漏队列。此 phase 不修改产品行为。

## P2-001：实现并运行平台排除扫描器

**Files:** Create `scripts/provider-v1/scan-platform-exclusions.py`; Create `scripts/provider-v1/tests/test_scan_platform_exclusions.py`; Create `docs/provider-adapter-v1/execution-v2/windows-test-exclusion-ledger.md`。

扫描范围包括 Rust 的 `#[cfg(...windows...)]`、`#[cfg_attr(...windows...)]`、test module/function 上的平台条件，以及 workflow/test script 中对 Windows target 的排除。每条 row 使用稳定 ID：

```text
WX-<first 12 hex of sha256(relative_path + symbol_path + normalized_cfg_expression)>
```

初始字段固定为：

```text
ID | file | line | symbol/test | cfg expression | production symbol exists on Windows |
classification | owner phase | repair task | rationale evidence | status
```

首次扫描只填事实字段，classification/owner/repair task 留空。测试覆盖多行 cfg、module cfg、function cfg、cfg_attr、重复扫描稳定 ID、文件行号变化 ID 不变和删除项检测。

## P2-002：冻结逐条分类决策树和 owner 映射

每个 row 按以下顺序判断，禁止跳步：

1. 该 symbol 是否属于 2.6 Windows 必须支持矩阵？是：不得分类 Unix-only。
2. 生产入口在 Windows 是否存在或应存在？存在但测试被关：`CROSS_PLATFORM_CONTRACT` 或 `PLATFORM_ADAPTER_CONTRACT`。
3. 入口应存在但 adapter 缺失：`MISSING_WINDOWS_IMPLEMENTATION`。
4. cfg 仅用于躲避编译/断言失败：`INVALID_EXCLUSION`。
5. 只有能力不在 V1 Windows 矩阵、Windows 无入口、直接依赖不可移植 OS primitive 且有 shared logic test 时，才可暂定 `UNIX_ONLY_FEATURE`。

owner phase 由相对路径机械映射：

- filesystem/path/worktree/config/catalog/session-file → P11；
- process/signals/ACP/shell-completion/terminal/PTY → P12；
- pager-render/images/OSC8/scrollback/shortcuts/tools/clipboard → P13；
- 不匹配映射立即 BLOCKED，不得由代理选最近阶段。

## P2-003：生成并冻结逐条分类任务卡

运行 P1-011 `--mode classify --phase 2`。每个 WX row 生成且只生成一张 `P2-C-<WX-ID>` 卡；卡片 ownership 仅为 ledger row 和该 symbol 的只读源码。记录输入/输出 SHA，随后 `--check` 必须通过。

## P2-004：执行 classification queue

严格按 `phase-02-classify.md` 顺序执行。每张卡只处理一个 WX row，只能填写 classification、owner phase、rationale evidence 和 provisional repair task。不得修改 Rust、workflow 或测试代码。无法由决策树唯一判断时写 deviation 并停线，禁止填“unknown/暂时跳过”。

## P2-005：机械验证分类完整性

扩展扫描器 `--validate-ledger`：验证所有 row 已分类、owner 映射正确、repair task ID 唯一、源代码扫描与 ledger 一一对应、没有丢失/重复/stale row。任何差异非零退出。

## P2-006：逐条反证 `UNIX_ONLY_FEATURE`

对每个 provisional Unix-only row，由 materializer 生成一张 `P2-U-<WX-ID>` 审查卡并冻结 SHA。每张卡必须证明：

- 不在 Windows 必须支持矩阵；
- Windows registry/UI/CLI 无入口；
- 直接依赖不可移植 OS primitive，而不是 adapter 缺失；
- 有 Windows capability test 证明不可见；
- shared business logic test 仍在 Windows 运行。

任一证明缺失，改分类为其他三类之一；不得保留 provisional。一个卡只审一行。

## P2-007：生成并冻结 P11～P13 repair cards

classification ledger 最终验证后，运行 materializer `--mode repair` 分别生成 phase-11/12/13 文件。每个非 Unix-only WX row 必须精确映射一个 repair task；一个 task 默认只拥有一个 WX row。只有 normalized causal fingerprint 和同一 production symbol 完全一致时，生成器才可建立 foundation task。

## P2-008：建立排除数单调下降门禁

记录 base scan count 和 ledger SHA。之后每个任务必须运行 scan + `--validate-ledger` + baseline delta：

- 新增 exclusion 数=0；
- `MISSING_WINDOWS_IMPLEMENTATION` 只能减少；
- `INVALID_EXCLUSION` 在责任 phase 结束时=0；
- 已关闭 row 的源码 cfg 必须消失或转换为有证明的精确 capability cfg；
- ledger/task-card SHA 不得被手工修改。

### Phase 2 Gate

- 初始每一处排除都有稳定 WX ID；
- classification queue 和 Unix-only reverse-audit queue 全部完成；
- ledger 无空 classification、无 unknown、无 provisional、无 stale row；
- 每个非 Unix-only row 绑定一个且仅一个 P11～P13 repair card；
- P11、P12、P13 task files 均通过 materializer `--check`；
- 无产品源代码行为改变。

---

# Phase 3 — 先修复门禁基础设施和全平台编译前置债务

**目标：** 在 Provider 架构继续改动前，让三平台 `workspace check --all-targets` 与 Clippy 成为可信且可执行的基础门禁。

## P3-001：建立可复用平台命令和 baseline-delta 脚本

**Files:** Create:

```text
scripts/provider-v1/check-linux.sh
scripts/provider-v1/check-macos.sh
scripts/provider-v1/check-windows.ps1
scripts/provider-v1/baseline-delta.py
```

脚本只封装本计划命令，不改变 gate 范围；任一命令失败立即非零。`baseline-delta.py` 比较 failure IDs、ignored tests、cfg exclusions、allow attributes 和 warnings。加入脚本级 fixture tests，证明新增失败/skip/allow 时返回非零，集合不变或减少时返回 0。

## P3-002：修正 `check.ps1` 的语义

**Files:** Modify `check.ps1`; Add one PowerShell test script under `scripts/provider-v1/tests/`。

冻结行为：

- 保留文件名 `check.ps1`，开头明确打印 `Targeted developer check`；
- 默认精确覆盖 provider、sampler、config、shell、pager、pager-bin；
- Clippy 失败必须非零退出；
- 不得最后无条件打印“All checks passed”；
- protoc 先用 PATH，再允许可配置 `PROTOC`，不得绑定单一 WinGet 路径；
- 输出实际成功 crate 列表和未运行范围。

**Gate:** 测试脚本用 PATH 注入的 fake cargo 让 Clippy step 返回 1，断言 `check.ps1` 非零且不打印 success；恢复全部 step=0 后断言通过。不得故意向 Rust 源码加入 lint。

## P3-003：修复 signed-policy integration test seam

**Files:** `xai-grok-config/Cargo.toml`; `xai-grok-config/src/signed_policy.rs`; `xai-grok-shell/tests/signed_managed_config/common.rs`; one associated test file。

冻结实现：新增非默认 feature `integration-test-seams`。embedded key override API 仅在该 feature 下公开；shell integration test target 显式启用此 feature。production/default build 不暴露 setter。先写期望测试证明 override 确实影响 `with_embedded_keys()`，当前实现必须红，再修复使绿。

## P3-004：执行 compile-ledger repair queue

执行 P1-011 已生成并冻结的 `P3-R-BL-<OS>-CHECK-<SEQ>` 任务卡。例如 `BL-WIN-CHECK-001` 的唯一任务 ID 是 `P3-R-BL-WIN-CHECK-001`。每个实例都是独立任务卡和独立提交，不得合并。规则：

- 一个 root error group 一个 commit；
- 先在失败平台复现；
- 只修 causal error，不批量 fmt 无关文件；
- Windows 失败不得用新增 cfg exclusion 修复；
- 三平台 affected package check 通过后关闭 ledger ID。

代理只按 `phase-03.md` 顺序执行；不得编辑任务卡或重新分组。

## P3-005：执行 Clippy-ledger repair queue

执行 P1-011 已生成的 `P3-R-BL-<OS>-CLIPPY-<SEQ>` 任务卡。同一 causal lint 是否形成 foundation group 只由生成器的 fingerprint 规则决定；代理不得自行合并。每个平台分别复跑证据。禁止 `#[allow]`，除非 lint 来自不可控生成代码且仓库所有者明确批准。

## P3-006：定义共享 atomic writer contract 与可测试 backend seam

**Files:** Create `xai-grok-paths/src/atomic_write.rs`; Modify `xai-grok-paths/src/lib.rs`; Modify `xai-grok-paths/Cargo.toml`; Add `xai-grok-paths/tests/atomic_write_contract.rs`。不得在 Catalog、TUI、session 各写一套。

冻结公开 API 与内部 seam：

```rust
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), AtomicWriteError>;

pub(crate) trait AtomicReplaceBackend {
    fn create_unique_temp(&self, target: &Path) -> Result<(PathBuf, File), AtomicWriteError>;
    fn replace_existing(&self, temp: &Path, target: &Path) -> Result<(), AtomicWriteError>;
    fn sync_parent(&self, parent: &Path) -> Result<(), AtomicWriteError>;
    fn cleanup_temp(&self, temp: &Path);
}
```

`AtomicWriteError` 至少区分 `CreateTemp`、`Write`、`Flush`、`SyncFile`、`Replace`、`SharingViolation`、`SyncParent`、`Cleanup`，并保存不含文件内容的 target/temp path context。契约固定为：同目录唯一临时文件、`write_all`、`flush`、文件同步、replace-existing、父目录同步、失败清理、不得使用固定 `.tmp`、不得先删除目标。先以 deterministic fake backend 写红绿 contract tests，覆盖每一步失败注入和 cleanup；本任务不实现真实 OS backend。

## P3-007：实现 Unix atomic writer backend

只修改 Unix backend 与同一 contract tests。冻结步骤：

1. 在目标同目录使用 `create_new(true)` 创建随机唯一临时文件；
2. `write_all(bytes)`；
3. `flush()`；
4. `sync_all()` 临时文件；
5. 同 filesystem `rename(temp, target)` 原子替换；
6. 打开父目录并 `sync_all()`；
7. 任一步失败清理仍存在的临时文件，不删除旧 target。

Linux/macOS 必须运行 create、replace、Unicode/space、concurrent temp、error injection、old-target preservation 和 parent-sync contract。不得以“平台不支持目录 fsync”静默忽略；若某文件系统明确返回不支持，必须返回 typed error 并由上层决定，不得伪成功。

## P3-008：实现 Windows atomic writer backend

只修改 Windows backend、`xai-grok-paths/Cargo.toml` 的现有 workspace `windows` dependency features 与 Windows tests。使用 `windows` crate 的 `Win32_Storage_FileSystem`：同目录 `create_new` 临时文件，`write_all`、`flush`、`sync_all` 后调用 `MoveFileExW`，flags 固定为 `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`。冻结行为：

- 不得使用 delete-then-rename；
- target 被其他进程以拒绝共享方式打开时映射为 `AtomicWriteError::SharingViolation`；
- replace 失败时旧 target 内容保持不变并清理 temp；
- path 转 UTF-16，支持 Unicode、space、UNC 和 normalized long path；
- Windows shared contract 覆盖 create、replace-existing、目标占用、concurrent temp、long path、error cleanup 和旧文件保持。

不得通过删除 replace-existing/目标占用用例、降低断言或新增 `cfg` 获得绿色。

## P3-009：定义路径 normalization 基础接口

在 `xai-grok-paths/src/normalize.rs` 冻结 `normalized_absolute(&Path) -> Result<PathBuf, PathError>`，并在该 crate 增加已有 workspace dependency `dunce`。该函数是唯一 filesystem canonicalization wrapper；加入 Windows drive/UNC/`\\?\` 与 Unix symlink 基础 tests。消费者迁移留 P11。

### Phase 3 Gate

三平台：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

必须全部通过；此外 P3-002 PowerShell failure-propagation tests、P3-003 signed-policy behavioral integration tests、P3-006～P3-008 三平台 atomic writer contract 必须全绿。A-14、A-17 在本阶段关闭。其他测试行为失败可以仍在 ledger，但测试目标必须全部可编译。未达到此 gate，不得进入 P4。

---

# Phase 4 — Typed 配置、任意 Provider identity 与品牌中立性

**目标：** `[provider.<id>]` 对 built-in 和任意 OpenAI-compatible 实例都可达；解析错误不静默；模型 identity 不偏向 xAI。

## 冻结配置语义

```toml
[provider.openai]
enabled = true
env_key = ["OPENAI_API_KEY"]
base_url = "https://api.openai.com/v1"
allow_insecure_http = false

[provider.deepseek]
kind = "openai_compatible"
profile = "deepseek"
env_key = ["DEEPSEEK_API_KEY"]

[provider.internal]
kind = "openai_compatible"
base_url = "https://llm.example/v1"
protocol = "chat_completions"
model_list_path = "/models"
model_list_format = "openai_compatible"
```

规则：built-in section 可省略 `kind`；未知 section 必须显式 `kind` 或已知 profile；不得通过域名猜品牌。

## 冻结解析结果接口

```rust
#[derive(Clone, Debug)]
pub struct ResolvedProviderSet {
    pub providers: IndexMap<ProviderId, ResolvedProviderSpec>,
}

#[derive(Clone, Debug)]
pub struct ResolvedProviderSpec {
    pub id: ProviderId,
    pub implementation: ProviderImplementation,
    pub config: ProviderRuntimeConfig,
}

pub struct ProviderConfigInput {
    // serde/TOML-facing fields; inline values exist only during resolution.
    pub api_key: Option<String>,
    // remaining non-secret config fields
}

#[derive(Clone, Debug)]
pub struct ProviderRuntimeConfig {
    pub public: ProviderPublicConfig,
    pub inline_api_key: Option<SecretValue>,
}

#[derive(Clone)]
pub struct SecretValue(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderImplementation {
    Builtin { definition_id: ProviderId },
    OpenAiCompatible { profile: Option<CompatibleProfileId> },
}
```

约束：

- `ResolvedProviderSet` 是 precedence resolver 的唯一输出，也是 Registry `prepare` 的唯一配置输入；
- built-in identity 通过 `definition_id` 查找固定 definition；
- arbitrary identity 通过 `ProviderImplementation::OpenAiCompatible` 选择已注册 factory；
- resolver 不创建、注册或缓存 `SharedProvider`；
- env/session/request secret value 不进入这些结构；inline TOML/CLI secret 只能进入 `SecretValue`；
- `SecretValue` 必须实现 `Clone`，自定义 `Debug`/`Display` 固定输出 `[REDACTED]`，不得实现 `Serialize`/`Deserialize`，明文 accessor 仅 `pub(crate)`；`ProviderRuntimeConfig::Debug` 只能经该 redaction 输出；
- public snapshot/view/config diagnostics 只能读取 `ProviderPublicConfig` 和 credential-present 状态；
- 同一个 `ProviderId` 在集合中只能出现一次，排序必须确定。

## P4-001：配置 parser 返回 diagnostics

**Files:** `xai-grok-provider/src/config.rs`; tests。

将 `parse_provider_toml` 改为：

```rust
pub fn parse_provider_toml(
    toml: &toml::Value,
) -> Result<ParsedProviderConfig, Vec<ConfigDiagnostic>>;
```

diagnostic 包含 TOML path、provider ID、field、message。删除 `filter_map(...ok()?)`。

## P4-002：加入 `enabled/kind/profile/allow_insecure_http` schema

只完成 deserialize、validation 和 roundtrip tests，不创建实例。`allow_insecure_http` 默认 `false`，只允许显式布尔值；它是 endpoint transport policy，不得被 profile 或域名自动改为 `true`。

## P4-003：加入 protocol/model-list schema

字段：`protocol`、`model_list_path`、`model_list_format`。非法 protocol 不在 parse 阶段猜 fallback；由 validation 返回 typed diagnostic。

## P4-003A：分离 TOML 输入、runtime config 与 secret wrapper

**Files:** `xai-grok-provider/src/config.rs`; `xai-grok-provider/src/auth.rs`; tests。

实现 `ProviderConfigInput → ProviderRuntimeConfig`。`api_key` 立即转成 `SecretValue`，之后禁止回到普通 `String`。删除 runtime config 的 Serialize；Debug/Display/diagnostic/snapshot tests 使用 canary 验证无明文。model-level inline credential 在其解析点使用同一个 `SecretValue` 类型，不定义第二套 secret wrapper。

## P4-004：实现 built-in config resolution

对 xai/openai/anthropic/opencode/ollama，section ID 映射已注册 definition。只处理 built-ins。

## P4-005：定义 generic provider factory 契约并实现 OpenAI-compatible factory

Create `xai-grok-provider/src/providers/openai_compatible_factory.rs`，并只在 `providers/mod.rs` 导出。冻结接口：

```rust
pub trait ProviderFactory: Send + Sync {
    fn create(
        &self,
        spec: &ResolvedProviderSpec,
    ) -> Result<SharedProvider, ProviderError>;
}

pub type SharedProviderFactory = Arc<dyn ProviderFactory>;
```

本任务只实现 `OpenAiCompatibleProviderFactory`：输入任意 `ProviderId + resolved config/profile`，生成该 identity 独有的 provider definition/configuration source。不得复用固定 ID `openai-compatible` 代表所有实例，不得在 resolver 或 launcher 中直接注册临时 definition。加入 identity 保真、两个实例互不共享配置、非法 implementation 拒绝测试。

## P4-006：实现 profile defaults

至少 DeepSeek、Groq、OpenRouter 使用独立 profile 数据：base URL、default env key、protocol、model-list format。profile 只提供默认值，显式配置可覆盖；不得出现 `XAI_API_KEY` 默认。

## P4-007：解析并工厂化两个 custom compatible provider

本阶段不调用 Registry。测试同一 TOML 同时生成 `deepseek` 与 `internal` 两个 `ResolvedProviderSpec`，随后分别交给同一个 factory，断言 identity、endpoint、env key、routes 和 model-list policy 互不覆盖。Registry 集成由 P5-011 负责。

## P4-008：冻结配置优先级

唯一配置优先级：

```text
provider implementation defaults
< selected profile defaults
< legacy migration values
< TOML provider/model values
< startup CLI configuration overrides
```

环境变量的“名称”由配置合并，环境变量的“值”只在请求时读取。request credential override 与 generation override 不进入 `ResolvedProviderSet`。不得在 bootstrap 捕获 env/session secret value。

## P4-009：模型引用中立化

- `provider/model` 始终直接解析；
- bare model 唯一匹配时可解析；
- 多匹配报 ambiguous；
- 零匹配报 not found；
- 只有 `LegacyModelReference` 输入可把已知旧 Grok model 映射 `xai/model`。

删除通用 `Ok(("xai", bare_model))`。

## P4-010：品牌中立性静态测试

建立测试/扫描：通用 resolver、catalog、sampler、TUI state 不允许 brand-specific branch。允许列表仅包含 xAI provider/auth/migration 文件。

### Phase 4 Gate

- A-06、A-13、A-15 Closed；
- custom provider 双实例 resolver + factory contract 通过；
- config diagnostics path tests 通过；
- SecretValue canary redaction/non-serialization tests 通过；
- `openai-compatible` 无 XAI key 默认；
- 新持久化模型引用均为 provider-qualified。

---

# Phase 5 — Registry 真正事务化并删除三锁分裂

**目标：** 定义、配置和 snapshot 发布具有严格一致性；并发 rebuild 不重号、不部分发布、不吞锁错误。

## 冻结状态结构

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderFactoryKind {
    OpenAiCompatible,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProviderRouteKey {
    pub provider_id: ProviderId,
    pub local_route_id: RouteId,
}

pub struct RegistrySnapshot {
    pub revision: u64,
    pub providers: IndexMap<ProviderId, Arc<ConfiguredProvider>>,
    pub routes: IndexMap<ProviderRouteKey, Arc<Route>>,
}

struct RegistryState {
    definitions: IndexMap<ProviderId, SharedProvider>,
    factories: IndexMap<ProviderFactoryKind, SharedProviderFactory>,
    snapshot: Arc<RegistrySnapshot>,
    sealed: bool,
}

pub struct ProviderRegistry {
    state: parking_lot::RwLock<RegistryState>,
}
```

第一次成功 rebuild 后，**built-in definitions 与 factories** sealed；配置中的 provider identity 不 sealed。V1 不支持运行时加载新的实现代码或 factory，但必须支持在后续 rebuild/hot reload 中新增、删除和修改任意 `OpenAiCompatible` identity。Registry 在每次 `prepare` 时依据 `ResolvedProviderSpec.implementation`：

- `Builtin`：从 `definitions` 查找；
- `OpenAiCompatible`：从 `factories[OpenAiCompatible]` 创建该 identity 的 provider；
- implementation/factory 不存在：typed hard failure。

若未来需要动态插件加载，另立版本设计。

## P5-001：写 concurrent revision 失败测试

写“两个并发 rebuild 必须返回不同、连续 revision”的期望测试；当前实现若返回相同 revision，该测试必须红，随后本 phase 实现使其绿。

## P5-002：合并 registry state locks 并冻结锁语义

只改 `registry.rs`、provider crate 的 `Cargo.toml` 和 unit tests。使用 workspace 已有的 `parking_lot::RwLock`；移除独立 `configs` lock，config 保存于 `ConfiguredProvider`/snapshot，不维护平行权威。冻结以下行为：

- `snapshot()` 在短 read lock 内克隆当前 `Arc<RegistrySnapshot>` 后立即释放；
- `prepare()` 只在短 read lock 内克隆 definitions、factories、base revision 与当前 snapshot identity，所有 provider 创建和 validation 在锁外执行；
- `commit()` 只持有一次 write lock，检查 base revision 后原子替换 snapshot；
- 禁止 `read().ok()`、`write().ok()`、`unwrap_or_default()`、锁失败时返回空 snapshot 或旧默认值；
- 禁止在持锁期间调用 provider factory、解析配置、访问网络或触发 callback。

加入 lock-duration seam tests，证明慢 factory 不阻塞 `snapshot()`，并发 prepare 不持 write lock。

## P5-003：实现 definition/factory seal

- revision 0 可注册 built-in definition 与 factory；
- 首次成功 rebuild 后，注册新 definition/factory 返回 typed error；
- duplicate definition/factory 返回 error；
- 配置 identity 的增删不属于 definition 注册，后续 rebuild 必须允许；
- 禁止 legacy `register()` 吞错。

## P5-004：未知 implementation、definition 或 factory 必须拒绝

对 `ResolvedProviderSet` 分别加入失败测试：

- `Builtin` 指向未注册 definition；
- `OpenAiCompatible` 但 factory 未注册；
- resolver 产生未知 implementation kind（通过 test-only constructor）；
- provider ID 与 spec ID 不一致。

全部返回 path-aware typed error，revision/snapshot 不变。不得回退固定 `openai-compatible` 或猜测 built-in。

## P5-005：selector 与 route 完整校验

先冻结 selector 契约：

```rust
pub trait RouteSelector: Send + Sync {
    fn select(&self, model_id: &str) -> Result<RouteId, RouteSelectionError>;
    fn referenced_route_ids(&self) -> &[RouteId];
}
```

固定 selector 的 `referenced_route_ids()` 返回单元素；OpenAI selector 必须明确返回 chat/responses 两个候选。Registry 不允许通过 downcast、provider brand 或试探模型名推断 selector 可能返回值。每个 configured provider：

- default route 存在；
- `referenced_route_ids()` 的每个 route ID 均存在且集合非空；
- route.provider_id 与 configured provider 相同；
- route ID 只需在单个 provider 内唯一；snapshot 必须以 `ProviderRouteKey { provider_id, local_route_id }` 索引，禁止全局裸 `RouteId` 冲突；
- endpoint/auth/protocol 基础校验通过。

## P5-006：失败 rebuild 保持旧 snapshot

对 duplicate route、invalid route、unknown config、selector missing route 分别测试 revision 和 Arc snapshot 不变。

## P5-007：删除 legacy registry mutators

移除或改为 `#[cfg(test)]` migration-only：

- `store_config`；
- `register_route`；
- standalone `configure`；
- 吞锁错误的 `register`。

所有 production references 必须为 0。

## P5-008：实现只读 `prepare`

冻结类型和接口：

```rust
pub struct PreparedRegistrySnapshot {
    base_revision: u64,
    snapshot: Arc<RegistrySnapshot>,
}

pub fn prepare(
    &self,
    resolved: &ResolvedProviderSet,
) -> Result<PreparedRegistrySnapshot, ProviderError>;
```

`prepare` 按 `ResolvedProviderSpec.implementation` 选择 built-in definition 或 generic factory，完成 provider 创建、配置、route、selector、endpoint/auth validation，但绝不发布。测试成功 prepare 后 `Arc::ptr_eq(old_snapshot, current_snapshot)` 且 revision 不变；每一种 validation failure 也保持不变。

## P5-009：实现原子 `commit`

冻结接口：

```rust
pub fn commit(
    &self,
    prepared: PreparedRegistrySnapshot,
) -> Result<u64, ProviderError>;
```

`commit` 只在当前 revision 等于 `base_revision` 时原子发布，否则返回 `RevisionConflict`。本任务只实现成功 commit 恰好 revision+1、重复 commit 拒绝、stale prepared conflict 三组测试，不改 convenience API。

## P5-010：实现 `rebuild = prepare + commit`

新增 convenience API，并用两个并发 rebuild 测试 revision 连续且 snapshot 不部分发布。禁止复制 prepare validation 或绕过 commit。

## P5-011：证明 sealed registry 仍可热增删 custom identity

首次 rebuild 只含 `deepseek`，成功后 registry 已 sealed；第二次 prepare/commit 新增 `internal`；第三次删除 `deepseek`。断言：

- 不需要注册新 definition/factory；
- revision 每次恰好 +1；
- snapshot identity 集合精确匹配输入；
- retained identity 的 route/config 不被其他 identity 污染；
- 任一步失败时旧 snapshot 保持可用。

### Phase 5 Gate

- A-12 Closed；
- registry 并发/失败原子性测试全绿；
- sealed 后 custom identity 增删回归测试全绿；
- 两个 provider 使用相同 local route ID 时可共存，跨 provider lookup 不串线；
- `rg` 不存在 production legacy mutator 调用；
- provider core check/clippy/test zero failure。

---

# Phase 6 — 建立唯一 ProviderRuntime 启动链

**目标：** launcher、headless CLI、TUI、session 和测试都通过同一 bootstrap helper 构造一个 runtime；删除生产启动对 legacy registry 配置的依赖。

## 冻结接口

Create `xai-grok-shell/src/agent/provider_bootstrap.rs`：

```rust
pub struct ProviderBootstrapInput {
    pub resolved: ResolvedProviderSet,
}

pub async fn bootstrap_provider_runtime(
    input: ProviderBootstrapInput,
) -> Result<Arc<ProviderRuntime>, ProviderBootstrapError>;
```

launcher 必须先调用 Phase 4 的唯一 resolver 生成 `ResolvedProviderSet`，再调用 bootstrap。bootstrap 只负责注册 built-in definitions 和固定 generic factories，然后把完整 `ResolvedProviderSet` 交给 transactional registry；custom identity 必须由 Registry 在 `prepare` 中通过 factory 实例化，不得由 resolver/launcher 临时注册 definition。bootstrap 不得第二次解析 TOML、环境或 CLI。

## P6-001：为 launcher 单 runtime 不变量写失败测试

**Files:** Create `xai-grok-shell/tests/provider_bootstrap.rs`。

通过可注入 launcher assembly seam 写期望不变量：启动结果必须具有 revision=1、configured providers 非空、routes 非空，并且只构造一次 runtime。该测试在当前 legacy `configure_providers()` 路径上必须红；错误输出记录当前 `routes != empty/providers == empty` 作为根因证据。不得写“断言坏行为成立”的永久测试。

## P6-002：创建 bootstrap helper 骨架

**Files:** Create `provider_bootstrap.rs`; Modify `agent/mod.rs`; Test file。

只实现 built-in definitions、`OpenAiCompatibleProviderFactory` 注册和最小 `ResolvedProviderSet` rebuild。测试 snapshot revision=1、providers 非空、routes 非空、definition/factory seal 生效。

## P6-003：bootstrap 接收完整 ResolvedProviderSet

**Files:** `provider_bootstrap.rs`; tests。

消费 Phase 4 resolver 的输出，不克隆或返回 resolved secrets，不重新解析 TOML/legacy/startup CLI config。测试 resolved set 中的 built-in 和 custom identities 全部进入 snapshot，bootstrap 返回值只有 runtime Arc。

## P6-004：launcher 调用唯一 config resolver

**Files:** launcher config assembly helper；tests。

把 effective TOML、legacy migration input 和 CLI overrides 一次性交给 Phase 4 resolver；任何 diagnostic 使启动失败，不得传递部分配置。

## P6-005：验证 precedence 只执行一次

使用带计数 test seam 证明 launcher 不在 resolver、bootstrap、provider constructor 中重复解析 TOML/legacy/startup CLI config；env/session secret value 读取次数在启动阶段必须为 0。

## P6-006：launcher 使用 bootstrap helper

**Files:** `xai-grok-pager-bin/src/main.rs`; 最多一个 launcher test。

删除 main 中 `register_all + configure_providers` 组合；必须 `await bootstrap_provider_runtime()`。失败时启动返回清晰错误。

## P6-007：Pager `run()` 接收 `Arc<ProviderRuntime>`

**Files:** `xai-grok-pager/src/app/mod.rs`; call site；tests。

禁止 `Option<ProviderRegistry>` 和 `unwrap_or_else` 自建 registry。测试/工具若需要 standalone runtime，必须调用同一 bootstrap test helper。

## P6-008：`providers` CLI 使用 bootstrap helper

**Files:** `xai-grok-pager/src/providers_cmd.rs`; tests。

不得创建独立 registry。输出 revision 和真实 configured providers。

## P6-009：AgentConfig 只保存 runtime，不平行保存 registry/catalog

**Files:** `xai-grok-shell/src/agent/config.rs`; `provider_runtime.rs`; tests。

删除或逐步私有化 `provider_registry`、`provider_catalog` 独立字段；访问通过 `provider_runtime.registry/catalog`。

## P6-010：runtime identity regression test

断言 launcher helper、Pager、AgentConfig、session factory、ConfigReloader 引用 `Arc::ptr_eq == true` 的同一 runtime。此时 reloader 尚可不执行 rebuild，但 identity 必须已注入。

### Phase 6 Gate

- main、Pager、providers CLI 无 `configure_providers()` 调用；
- production call graph 中 runtime 只构造一次；
- snapshot providers/routes 均非空；
- P6 bootstrap integration tests 通过；
- A-01 的“启动不 rebuild”部分关闭，legacy fallback 部分留 P7。

---

# Phase 7 — Route、Endpoint 与 Sampler 的唯一执行权威

**目标：** route compiler 成为唯一请求策略编译器，Sampler 不再自行拼 URL、猜协议或回退旧配置。

## 冻结接口

```rust
pub struct ResolvedModelExecution {
    pub provider_id: ProviderId,
    pub route_id: RouteId,
    pub protocol_id: ProtocolId,
    pub request_url: url::Url,
    pub static_headers: http::HeaderMap,
    pub auth_policy: AuthPolicy,
    pub model_id: ModelId,
    pub generation: GenerationOptions,
    pub limits: ModelLimits,
}

pub fn resolve_model_execution(
    snapshot: &RegistrySnapshot,
    model: &ModelEntry,
    request: &RequestOverrides,
) -> Result<ResolvedModelExecution, ProviderResolutionError>;
```

`ResolvedModelExecution` 不含 plaintext secret；auth 在 P8 request-time 执行。

## P7-001：No-legacy-fallback 失败测试

对 missing provider、missing route、unknown protocol、invalid endpoint 构造 provider-bound model，直接断言 typed hard error 和 legacy-call count=0。测试在当前 fallback 路径上必须红；不得先提交“fallback 是正确行为”的测试。

## P7-002：`sampling_config_for_model_with_registry` 返回 Result

只改变签名和直接调用方编译；不得在本任务同时重写 endpoint/auth。所有调用方必须显式传播或转为用户可见 error。

## P7-003：移除 provider-bound legacy fallback

当 model 有 provider binding 或 runtime snapshot revision>0 时，route compiler error 直接返回。仅未迁移的明确 legacy xAI model 可走独立 migration adapter，且 adapter 输出普通 `ResolvedModelExecution`。

## P7-004：Endpoint::render 成为唯一 URL 构造入口并冻结 transport policy

修改 route compiler 使用 `Endpoint::render`。冻结并测试：

- `https` 对所有 host 允许；
- `http` 仅对 `localhost`、`127.0.0.0/8`、`::1` 默认允许；
- 其他 host 的 `http` 只有配置显式 `allow_insecure_http = true` 才允许，并生成一次不含 secret 的结构化 warning；
- URL userinfo/credentials、fragment、非 `http/https` scheme、空 host、空 base 全部拒绝；
- path/query 必须经 `Endpoint::render` 编码和拼接，不得字符串连接；
- redirect policy 不在此层实现，由 P9/P8 HTTP client policy 强制。

测试覆盖 IPv4/IPv6 loopback、remote HTTP allow/deny、credentials、fragment、path join、query encoding。

## P7-005：Sampler 接收完整 `url::Url`

Sampler 不再字符串拼接 base/path/query；移除未编码 query 逻辑。测试保留特殊字符和重复 query 语义。

## P7-006：协议表显式查找

- protocol ID 必须来自注册表；
- unknown 为 typed error；
- `api_backend` 仅用于 legacy config migration，不得覆盖 `protocol_id`；
- 三协议 parser 仍可内建，但通用执行层不按 provider brand match。

## P7-007：generation/default merge 单点实现

顺序固定：route defaults < model defaults < request overrides。limits 不允许 request 提高到 provider/model 上限之外。每字段独立测试。

## P7-008：删除生产直接 `SamplerConfig` 构造并建立编译边界

本任务只做调用点清单和类型边界，不解析 credential。新增临时私有迁移函数 `execution_to_unprepared_sampler_config_for_migration` 仅用于让 Phase 7 分步编译，并用 `#[deprecated(note = "removed in P8-011")]` 标记；它不得发送请求，也不得填充 auth header。生产调用点必须先得到 `ResolvedModelExecution`。

冻结最终规则：Phase 8 完成后，生产和 integration E2E 中直接构造旧 `SamplerConfig` 的引用数必须为 0；Sampler crate unit tests 可使用 `test_prepared_config()` helper。不得把临时迁移函数当作最终 adapter。

## P7-009：删除 URL/host provider inference

通用 sampler/header code 中删除按 `api.x.ai`、`openai.com` 等 host 推导 provider 行为。provider-specific headers 由 route 提供。

### Phase 7 Gate

- A-01 fallback 部分、A-09 Closed；
- production `sampling_config_for_model_with_registry` 不再返回裸 config/fallback；
- endpoint security tests 全绿；
- `rg` 无 production URL string concatenation for inference path；
- provider-bound error matrix 全部 hard fail。

---

# Phase 8 — Request-time 认证、Header 合并与 secret 安全

**目标：** Provider/TOML/model/CLI/session 的 credential 经过单一 request-time resolver；Public/None 语义明确；错误不吞；secret 不进入 snapshot/log。

## 冻结认证模型与最终请求准备接口

```rust
pub enum CredentialCandidate {
    RequestOverride,
    ModelInline,    // SecretValue in runtime model config
    ProviderInline, // SecretValue in ProviderRuntimeConfig
    ModelEnvironment(Vec<String>),
    ProviderEnvironment(Vec<String>),
    BuiltinEnvironment(Vec<String>),
    Session(SessionKind),
}

pub enum AuthPolicy {
    None,
    Bearer { candidates: Vec<CredentialCandidate>, required: bool },
    Header { name: HeaderName, candidates: Vec<CredentialCandidate>, required: bool },
}

pub struct SensitiveHeaderMap(http::HeaderMap);

pub struct PreparedSamplerConfig {
    pub provider_id: ProviderId,
    pub route_id: RouteId,
    pub protocol_id: ProtocolId,
    pub request_url: url::Url,
    pub headers: SensitiveHeaderMap,
    pub model_id: ModelId,
    pub generation: GenerationOptions,
    pub limits: ModelLimits,
}

pub async fn prepare_sampler_config(
    execution: &ResolvedModelExecution,
    credentials: &RequestCredentialContext<'_>,
    request_headers: &RequestHeaderOverrides,
) -> Result<PreparedSamplerConfig, RequestPreparationError>;
```

冻结语义：

- `ResolvedModelExecution` 不含 secret 和最终 auth header；
- `SensitiveHeaderMap` 必须 `Clone`，自定义 `Debug`/`Display` 只输出 header name 与 redacted value，不实现 serde；
- `prepare_sampler_config` 是生产代码构造 `PreparedSamplerConfig` 的唯一函数；它必须是 async，因为 session candidate 可能刷新 token；不得在同步路径中 `block_on`；
- Sampler 的生产入口只接受 `PreparedSamplerConfig`；旧 `SamplerConfig` 只能作为 crate 内部迁移类型，并在 P8-011 删除；
- 类型必须准确表达有序候选和 optional/required，禁止无值 `Inline`、用 `Public` 伪装 Bearer。

## P8-001：为 Inline/Public 语义写失败测试

写期望行为测试：inline candidate 有值时可解析、无值时按 required/optional 语义处理；public route 不产生认证 header。当前 `Inline/Public` 实现上测试必须红，随后重构使其绿。

## P8-002：替换 credential source 表达

只改 provider auth types/tests。Provider constructors 只声明 candidate 类型和顺序，不读取 env/session/value。

## P8-003：实现 request credential context

Create shell-side resolver context，冻结职责：

```rust
pub struct RequestCredentialContext<'a> {
    pub request_override: Option<&'a SecretValue>,
    pub model_inline: Option<&'a SecretValue>,
    pub provider_inline: Option<&'a SecretValue>,
    pub environment: &'a dyn EnvironmentReader,
    pub session: &'a dyn SessionCredentialResolver,
}
```

`EnvironmentReader` 只按候选变量名返回 `Result<Option<SecretValue>, CredentialError>`；测试使用 deterministic map，生产实现只在请求准备时读取 process environment。`SessionCredentialResolver` 使用仓库已有 boxed-future 模式异步返回 `Result<Option<SecretValue>, CredentialError>`；不得引入同步 `block_on`。本任务不实现 HTTP header；只返回 `SecretValue`/borrowed wrapper，禁止转成可 Debug 的普通 `String`。

## P8-004：实现统一优先级

准确测试并冻结全局顺序：request override > model inline > provider inline > model env > provider env > built-in env > session。V1 不允许 provider-specific 重排；Provider 只能声明哪些 candidate 存在。xAI session 作为最后候选，不能越过显式 key/env。

## P8-005：xAI session resolver 适配

现有 OAuth refresh 作为 `SessionKind::Xai` resolver；不得绕过 route compiler 或直接构造请求。保留现有行为回归测试。

## P8-006：OpenCode public route 改为明确无认证

使用 `AuthPolicy::None`。匿名 discovery/inference mock tests 通过；不得把 public sentinel 填入 Bearer，也不得发送空 `Authorization` header。

## P8-007：Header merge 单点实现并产出 `SensitiveHeaderMap`

顺序：transport-required headers → route mandatory static headers → provider extra headers → auth header → request-safe overrides。该函数必须返回 `SensitiveHeaderMap`，不得把最终 header map 放回 registry snapshot 或 catalog cache。冲突规则：

- 相同 key 相同 value 去重；
- user override 与 mandatory/auth 不同 value 返回 `HeaderConflict`；
- header name/value 使用 HTTP 类型验证；
- 禁止换行、控制字符。

## P8-008：删除 auth error 吞没

移除所有 `if let Ok(...)` 忽略认证错误和 `.ok()` fallback。missing required credential 在发送前返回 typed error。

## P8-009：secret redaction 审计

测试以下均不含 secret：`Debug`、Display error、tracing fields、panic、snapshot、catalog cache、TOML diagnostics、test snapshots。使用固定 canary secret 扫描测试输出和序列化结果。

## P8-010A：OpenAI Bearer 真实请求检查

单一 mock route，断言 request override/provider inline/env precedence、Bearer 格式、URL 和 protocol；本任务不测试其他 provider。

## P8-010B：Anthropic `x-api-key` 真实请求检查

断言 mandatory `anthropic-version`、`x-api-key`、冲突拒绝和无 Authorization。

## P8-010C：xAI session 真实请求检查

断言 session 只在所有显式 candidate 缺失后使用，refresh error typed 返回，不绕过 `prepare_sampler_config`。

## P8-010D：OpenCode 与 Ollama no-auth 请求检查

分别断言无 `Authorization`、无空值 header、匿名 discovery/inference 成功。若两者协议不同，使用两个独立 test function，但仍只修改同一测试文件。

## P8-010E：Custom provider `extra_headers` 请求检查

使用两个 custom identities，断言 headers 不串线、非法 header 拒绝、mandatory conflict typed 返回。

## P8-010F：缺失认证与 header conflict 失败矩阵

每个 failure case 在 HTTP send seam 前停止，mock server request count=0；验证 error 不含 canary secret。

## P8-011：切换 Sampler 到 `PreparedSamplerConfig` 并删除临时迁移类型

逐个生产调用点改为 `resolve_model_execution → prepare_sampler_config → Sampler`。删除 P7-008 临时函数和旧生产 `SamplerConfig` 构造器；Sampler crate 仅保留 `test_prepared_config()` 测试 helper。加入静态扫描测试：production、shell integration、E2E 中 `SamplerConfig {` 和旧 constructor 引用为 0。

### Phase 8 Gate

- A-02、A-08 Closed；
- credential/header 全部通过真实 mock request inspection；
- production 请求链唯一为 `ResolvedModelExecution → prepare_sampler_config → PreparedSamplerConfig → Sampler`；
- source scan 无 production `apply_auth_policy(...).ok()`/吞错；
- secret canary 零泄漏。

---

# Phase 9 — 异步 Catalog 真正替换同步模型发现

**目标：** 删除生产 blocking discovery，修复 Catalog 的并发、timeout、status、auth、stale、持久化和 cancellation。

## P9-001：冻结 catalog state machine

状态：`Empty | Loading | Fresh | Stale | Failed`；snapshot 包含 revision、provider-qualified models、wall-clock fetched_at、optional last_error。此任务只写类型和状态转换 tests。

## P9-002：配置唯一 reqwest client policy

冻结数值和行为：connect timeout **5 秒**、单次请求 total timeout **30 秒**；最多 **3 次** redirect；只允许同 origin（scheme + host + effective port 相同）redirect，跨 origin 立即返回 typed error，任何 redirect 都不得重新解析或复制不同 origin 的认证 header；单次 refresh 内 **不自动重试**；固定非品牌特权 User-Agent。使用 paused time/mock server 测 connect/total timeout 与 0/1/3/4 次 redirect，不访问公网。

## P9-003：实现有界并发

使用现有 Tokio semaphore；默认并发 **4**，配置允许范围 **1..=16**，越界在 config validation 阶段报错。测试最大同时请求数不超过限制。禁止对所有 provider 无界 `tokio::spawn`。

## P9-004：按声明 format 解析模型列表

不得根据 URL 包含 `/api/tags` 猜 parser。OpenAI-compatible 和 OllamaTags 分别测试成功/畸形/空列表。

## P9-005：HTTP status 与错误分类

2xx 才解析；401/403 auth、404 endpoint、429 rate limit、5xx transient、invalid JSON 分别记录 typed error。错误不得用空列表覆盖旧模型。

## P9-006：discovery 应用 endpoint/auth/headers

模型列表 URL 从 configured provider/model source 渲染；使用 P8 credential/header resolver。Ollama base URL override 必须生效。

## P9-007：实现 stale-while-revalidate

刷新失败时保留最后成功 models，状态变 Stale/Failed 并记录 error；只有从未成功过才为空。测试 revision 语义。

## P9-008：实现 TTL

默认 TTL **300 秒**，配置允许范围 **30..=86400 秒**，越界为 typed config error。使用 wall-clock/system time 持久化，运行时可用 `Instant` 计算；禁止把进程内 `Instant` 序列化。Fresh 未过期不请求；Force 强制；Stale 触发后台刷新。测试 29/30/299/300/301/86400/86401 秒边界和系统时钟回拨处理。

## P9-009：实现正确 cancellation

`ProviderRuntime::cancel()` 与 catalog worker 共享同一个 cancellation token。取消后的 join deadline 固定为 **2 秒**；超过 deadline 返回 typed shutdown error 并记录未泄漏 secret 的 task identity，不允许静默 detach。测试取消在 2 秒内终止 semaphore wait、in-flight request 和 persistence wait，且不发布半完成 snapshot。

## P9-010：实现跨平台 snapshot 序列化 roundtrip

保存 provider/model IDs、display names、metadata、timestamp、状态；load 后模型完整，不把历史快照伪装成刚刷新。

## P9-011：使用共享 atomic writer 持久化 catalog

只调用 Phase 3 已通过三平台 contract 的 `atomic_replace`；不得直接 `rename`，不得定义第二套 writer。加入 writer 失败时保留旧内存 snapshot 和旧磁盘 snapshot 的测试。

## P9-012：移除 blocking discovery 调用

从 `resolve_models_from_config()` 等生产路径删除 `fetch_provider_models_blocking()`。启动只合并 persisted/in-memory snapshot；后台 refresh 独立 spawn。

## P9-013：删除旧 blocking cache/function

确认 production references=0 后删除旧函数、TTL、cache。若测试依赖，迁移到 catalog tests，不保留 migration-only production code。

## P9-014：catalog revision 驱动模型目录更新

runtime 发布 catalog revision event；session/UI 订阅并重建 model view，不直接发网络请求。

### Phase 9 Gate

- A-03、A-07 Closed；
- startup no-network test 通过；
- `rg fetch_provider_models_blocking` 生产引用为 0；
- catalog mock matrix、TTL、stale、cancel、persistence roundtrip 全绿。

---

# Phase 10 — TUI、CLI、热重载与当前会话统一

**目标：** 所有配置写入和运行时观察使用同一个 ProviderRuntime；保存是原子单事务；当前会话立即看到成功 rebuild。

## 冻结协调器接口

Create `xai-grok-shell/src/agent/provider_config_coordinator.rs`：

```rust
pub struct ProviderConfigCoordinator {
    runtime: Arc<ProviderRuntime>,
    config_path: PathBuf,
    resolution_context: Arc<ProviderResolutionContext>,
    update_lock: tokio::sync::Mutex<()>,
}

impl ProviderConfigCoordinator {
    pub async fn apply_external_file(&self) -> Result<ConfigApplyOutcome, ConfigApplyError>;
    pub async fn save_patch(&self, patch: ProviderConfigPatch) -> Result<ConfigApplyOutcome, ConfigApplyError>;
}
```

`ProviderResolutionContext` 冻结保存 profile registry、legacy migration policy 和 startup CLI configuration overrides，不含 env/session secret value。所有 config-driven registry commit 必须经过此 coordinator 和同一 `update_lock`。`apply_external_file` 只读取/解析/prepare/commit，不回写文件；`save_patch` 使用 compare-and-swap 文件指纹、共享 atomic writer 和 prepare/commit。Watcher、TUI、CLI 不得直接调用 `registry.rebuild`。

## P10-001：ConfigReloader 注入 coordinator/runtime

**Files:** `xai-grok-shell/src/agent/app.rs`; `config/reloader.rs`; tests。

删除 `None, // provider_runtime`。注入同一 `Arc<ProviderConfigCoordinator>`；identity test 通过 coordinator 断言其 runtime 与 AgentConfig runtime 为同一 Arc。

## P10-002：reloader 使用完整 config resolver

不得自己 `parse_provider_toml` 后手工 map；调用 P4/P6 的 resolver，得到 diagnostics 或 `ResolvedProviderSet`。

## P10-003：外部文件热重载事务测试

只测试 `apply_external_file`：有效配置文件事件 → read/hash → resolve → prepare → commit → revision +1 → catalog refresh schedule → current session model view 更新。无效配置：revision 不变、旧请求继续、用户收到错误；coordinator 不修改外部文件。

## P10-004：Pager 删除 standalone provider registry

`provider_state` 只保存 runtime/view handle，不能初始化另一 registry。测试 `Arc::ptr_eq`。

## P10-005：删除 modal handler 的直接写盘路径

保留一个 `SaveProviderConfig` effect。`app/modals.rs` 只发 action；`app/effects` 执行 transaction。生产 `persist_provider_config()` 只能有一个调用方。

## P10-006：Provider config patch 只改目标 section

使用 toml_edit 保留 comments/order/未知字段；测试对 unrelated config 字节级或语义级不破坏。inline secret 需要明确 warning，但不得输出 key。

## P10-007A：建立 `save_patch` happy path

调用 Phase 3 `atomic_replace` 与 Phase 5 `prepare/commit`，严格执行：

```text
lock coordinator.update_lock
→ read old bytes + old SHA-256 + current revision
→ apply one typed TOML patch in memory
→ parse/resolve candidate
→ registry.prepare(candidate)
→ re-read current file SHA-256; 必须仍等于 old SHA-256
→ atomic_replace(candidate bytes)
→ registry.commit(prepared)
→ schedule catalog refresh
→ unlock
```

本任务只实现 happy path 和 revision+1/file bytes/current session 三者一致测试。

## P10-007B：prepare failure 原子性

注入 parse/validation/prepare failure；断言不写盘、不 publish、不调 catalog、旧 bytes/revision 完全不变。一个 test target，独立提交。

## P10-007C：文件 compare-and-swap conflict

在 prepare 后模拟外部编辑，使文件 SHA 与 old SHA 不同。必须返回 `ConfigFileChanged`，不覆盖外部 bytes、不 commit runtime。不得“最后写入者获胜”。

## P10-007D：atomic writer failure

注入 writer failure；断言 runtime/snapshot/revision 不变，磁盘保留 old bytes，无临时文件残留。

## P10-007E：commit consistency guard

所有正常 config commit 都受同一 update lock，因此 stale revision 在生产路径中不应发生。用 test seam 强制 commit failure：仅当当前文件 SHA 仍等于 candidate SHA 时才允许 atomic rollback old bytes；若文件已被第三方再次修改，返回 `ConsistencyEmergency`、保留第三方文件、runtime 仍旧 revision，并产生高优先级错误。不得盲目覆盖。

## P10-007F：并发保存和 watcher 去重

两个 concurrent `save_patch` 串行化，各自产生确定 revision；自身 atomic replace 触发的 watcher event 通过 file hash/revision 去重，不得再次 commit。测试最终文件、runtime、UI view 一致且 revision 增量等于成功保存次数。

## P10-008：UI view model 反映真实状态

字段：provider ID、display name、enabled/configured、credential source status（不含值）、endpoint、route count、catalog state、model count、runtime revision、last error。

## P10-009：保存成功/失败 reducer tests

成功只有在 atomic write + rebuild 完成后展示；失败保持编辑内容、显示错误、不改变运行状态。

## P10-010：`providers` CLI parity

CLI list/validate/refresh 使用同一 runtime。V1 不新增 `--json`；保持现有文本接口，测试内容全部来自 runtime view。

## P10-011：当前会话和新会话一致性

保存 provider 后当前 session 立即可选新模型；新 session 从同一 persisted config 得到相同 provider/model identity。

### Phase 10 Gate

- A-04、A-05、A-16 的双路径部分 Closed；
- runtime identity test 全绿；
- save/reload E2E revision 正确；
- production registry/runtime 创建点只有 bootstrap；
- config write call site 只有一个 effect transaction。

---

# Phase 11 — 真正的跨平台文件系统、路径和原子持久化

**目标：** 提供 Linux/Windows/macOS 一致的路径与持久化契约，关闭对应 exclusion，不以跳过测试代替。

## 已有前置接口

Phase 3 已提供并通过基础三平台 contract：

```rust
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), AtomicWriteError>;
pub fn normalized_absolute(path: &Path) -> Result<PathBuf, PathError>;
```

本阶段只迁移消费者、补充真实文件系统边界和关闭 exclusion，不得重新设计或复制这两个接口。

## P11-001：config consumer 迁移审查

运行静态扫描确认 P10 provider config transaction 只调用共享 writer，并固定执行 read-only、目标被占用、rollback tests。若扫描和测试已绿，本任务提交只包含证据文件；不得跳过命令，也不得重写实现。

## P11-002：catalog consumer 迁移审查

运行静态扫描确认 P9 snapshot 只调用共享 writer，并固定执行历史文件替换、损坏快照不覆盖、失败保留旧快照 tests。测试已存在且全绿时只提交证据，不新增重复实现。

## P11-003：session persistence 迁移

只修改 session persistence consumer 和 tests，删除直接 write/rename 路径。

## P11-004：路径 normalization 消费者迁移

所有消费者只调用 Phase 3 的 `normalized_absolute`；不得直接调用 `dunce::canonicalize` 或 `std::fs::canonicalize` 形成第二路径语义。测试 drive letter case、UNC、`\\?\`、symlink、relative、Unicode。

## P11-005：workspace classifier Windows 修复

恢复对应 Windows tests；不要把整个 classifier 设为 non-Windows。平台差异通过 fixture path 构造表达。

## P11-006：执行 worktree/git/path repair queue

执行 Phase 2 Gate 已冻结的 `P11-PATH-<ledger-id>` 任务卡。任务至少分别覆盖 `xai-fast-worktree`、plugin marketplace git、provider/tool paths；一个任务只允许一个 crate/一个 root cause。Windows 使用 Git 可理解路径和参数数组，禁止 shell 字符串拼接。不得把三个 crate 合并进一个提交。

## P11-007：文件 watcher/hot reload Windows contract

测试 atomic replace 后 watcher 收到可接受事件序列并最终只 rebuild 一次；通过 debounce/identity 实现，不依赖事件名称完全一致。

## P11-008：关闭 P2 filesystem exclusions

逐项删除 `INVALID_EXCLUSION/CROSS_PLATFORM_CONTRACT/MISSING_WINDOWS_IMPLEMENTATION`。保留真正 Unix-only 项必须改用 `cfg(unix)` 并写不适用证据。

### Phase 11 Gate

- filesystem/path exclusion ledger 责任项全部关闭；
- config/catalog/session persistence 三平台 contract tests 全绿；
- Windows replace-existing 和 Unicode/long-path tests 通过；
- A-16 原子写入部分 Closed。

---

# Phase 12 — 真正的跨平台进程、信号、ACP、Shell Completion 与终端/PTY

**目标：** 恢复被整体排除的核心行为测试，为 Windows 实现等价生命周期语义。

## P12-001：定义 process termination contract

统一语义：request graceful shutdown → bounded wait → forced termination → reap/no zombie；Windows 必须扩展仓库现有 Windows Job Object/process-handle abstraction，Unix 使用 process group/signals。不得创建第二套进程树管理器。此任务只建 trait 和共享 fake tests。

## P12-002：Unix process adapter tests

保持现有行为，迁移到 contract，不改变 Windows。

## P12-003：Windows process adapter implementation

实现 graceful/force/reap，测试 child tree、already exited、permission/error。禁止用 `cfg` 删除调用。

## P12-004：signals session contract

业务层接收 `ShutdownIntent`，平台层翻译 Ctrl-C/console close/Unix signal。共享 session tests 不依赖具体 signal API。

## P12-005：ACP test-module repair queue

先执行基础任务 `P12-ACP-support`，只使 `acp_session_tests/support.rs` 和 shared fixtures 在 Windows 可编译，不恢复其他模块。随后执行 Phase 2 Gate 已冻结的 `P12-ACP-<module-name>` 独立任务卡，例如 `P12-ACP-client_hooks_tests`、`P12-ACP-rewrite_zero_turn_prefix_tests`。规则：

- 一个任务只解除一个 module 的 Windows exclusion；
- 先在 Windows 复现该 module 的首个 causal failure；
- fixture 共性问题只能由专门 foundation task 修一次，后续 module 不重复改 foundation；
- module 内若出现任务卡未列出的第二个无关根因，当前任务立即 BLOCKED 并写 deviation；不得现场生成 `-02` 任务；
- 不得一次删除 59 个 cfg 后再集中修复。

## P12-006：ACP Windows adapter repair queue

执行 Phase 2 Gate 已冻结的 `P12-ACPA-<ledger-id>` 任务卡；每张卡只对应一个 ACP adapter capability。只针对 Windows IPC/process/path 差异写 adapter tests；不得复制整套 business tests并改变断言。

## P12-007：session persistence repair queue

执行已冻结的 `P12-PERSIST-<ledger-id>`；一个任务只恢复一个测试函数或一个共享 helper 根因。全部调用 P11 atomic/path API。禁止一次性解除 14 处后以大量失败作为中间状态提交。

## P12-008：shell completion shared-test queue

先执行 `P12-SC-foundation`，把 tokenization、candidate ranking、replacement range、escaping 与实际 shell invocation 分离。随后执行已冻结的 17 个 `P12-SC-<test-name>` 任务卡；一个任务只恢复一个场景并运行该精确 test。不得把 17 个测试作为一个子任务。

## P12-009：PowerShell completion adapter

测试空格、引号、Unicode、路径分隔符、drive/UNC；命令参数用数组，不用 `sh -c`。

## P12-010：cmd.exe active-shell adapter

V1 冻结支持 `ShellKind::Cmd` 作为最后 fallback。实现并测试 argument escaping、sequential operator、path with spaces、Unicode、exit code 和 cancellation；interactive completion 使用 cmd quoting/replacement contract。`grok completions cmd` 必须显式返回 unsupported（因为不生成 cmd completion script），但 active command execution 不得被禁用。

## P12-011：terminal streaming Windows implementation

处理 resize、EOF、cancel、partial UTF-8、CRLF；恢复对应 tests。

## P12-012：PTY harness Windows backend

真实启动最小 child process，验证输入、输出、resize、exit code、timeout cleanup。不能只 mock。

## P12-013：clipboard/link opener contract

使用平台 command/API adapter；测试路径作为单一 arg、URL escaping、无 handler 错误。Windows 必须有对应测试，不能只给 Unix test 加 cfg。

## P12-014：关闭 P2 process/ACP/terminal exclusions

逐项更新 ledger 和扫描计数。

### Phase 12 Gate

- ACP/session/shell completion/terminal/PTY 责任 exclusions 全部关闭；
- Windows shell/pager/PTY targeted tests zero failure；
- process cleanup 无 orphan；
- 不新增平台 skip。

---

# Phase 13 — Pager Render、Image、Tools 与剩余平台行为闭环

**目标：** 关闭剩余 Windows exclusions，证明 UI/render/tool 行为而非仅编译。

每个下列任务只处理一个模块群，不得合并：

## P13-001：prompt-image repair queue

执行 Phase 2 Gate 已冻结的 `P13-IMG-<ledger-id>`；一个任务只恢复一个 test/failure family。使用 platform-neutral fixtures，外部 viewer/decoder 能力通过 adapter 注入。

## P13-002：OSC8/link-render repair queue

执行分别对应 `osc8.rs`、link opener 和 link map 的已冻结任务卡，不得合并。共享 render output tests 在三平台运行；terminal capability 差异只影响启用策略，不影响核心编码正确性。

## P13-003：display-refresh/input-key repair queue

display refresh 与 input key 必须是两个独立任务；Windows console event 与 Unix terminal event 映射到共同 domain event，各自恢复业务 tests。

## P13-004：scrollback repair queue

执行已冻结的 `P13-SCROLL-<ledger-id>`；任务卡分别归属 `scrollback/render.rs`、selection 或 entry renderer。一个任务最多一个文件和一个 snapshot/newline/width 根因。

## P13-005：shortcuts/help/overlay repair queue

shortcuts help、BTW overlay 和其他 overlay 分别独立任务。平台快捷键差异使用 capability fixture，不整模块排除。

## P13-006：OpenCode grep repair task

只处理 grep Windows path semantics 和对应 tests，不修改 glob/bash。

## P13-007：OpenCode glob repair task

只处理 glob 的 separators、drive/UNC、case behavior，不通过 Unix shell 执行。

## P13-008：OpenCode command tool 的 Windows active-shell 语义

保持现有兼容 API 名称，但执行必须委托 `ShellKind::{Pwsh,PowerShell,GitBash,Cmd}`。分别测试命令链、后台操作符、Unix utility unavailable 文案、timeout/Job Object cleanup 和参数 escaping。不得在 Windows 隐藏工具，不得把 PowerShell/cmd 输入交给 POSIX parser；仅 Git Bash 使用 POSIX 规则。

## P13-009：skills/resources/tool-registry repair queue

执行已冻结的 `P13-TOOLS-<ledger-id>`，每张卡只归属一个 crate/file；恢复 platform-neutral discovery tests，路径依赖调用 P11 abstraction。

## P13-010：host clipboard repair task

只处理 host clipboard Windows adapter 和 contract tests。

## P13-011：pager PTY auxiliary repair queue

执行每个 PTY auxiliary exclusion/target 对应的已冻结任务卡，补齐 Windows adapter behavior。

## P13-012：全 exclusion ledger 清零审查

执行扫描并逐条检查剩余项。剩余 `cfg(unix)` 只允许真正 Unix-only production feature，且必须有：

- capability gate；
- Windows UI/registry 不暴露；
- shared code test；
- ledger rationale。

### Phase 13 Gate

- `MISSING_WINDOWS_IMPLEMENTATION = 0`；
- `INVALID_EXCLUSION = 0`；
- `CROSS_PLATFORM_CONTRACT disabled = 0`；
- Windows `cargo test --workspace` zero failure；
- Linux/macOS 无回归；
- A-10 Closed。

---

# Phase 14 — 真实生产链 E2E，而不是手工构造 SamplerConfig

**目标：** 测试调用与 launcher 相同的 bootstrap、runtime、catalog、route、auth、sampler 链。

## P14-001：共享 MockInferenceServer

支持：OpenAI Chat SSE、OpenAI Responses SSE、Anthropic Messages SSE、models endpoint、Ollama tags、auth/header inspection、delay/status/malformed response、request count。无公网。

## P14-002：真实 launcher helper E2E 基础

测试：TOML → bootstrap → snapshot → manual model → `resolve_model_execution` → `prepare_sampler_config` → `PreparedSamplerConfig` → Sampler → mock → decoded events。禁止手工 `SamplerConfig` 或 `PreparedSamplerConfig`。

## P14-003：OpenAI Chat 链

验证 endpoint、Bearer、model、stream events、usage。

## P14-004：OpenAI Responses 链

验证选择 responses route，不误走 chat。

## P14-005：Anthropic Messages 链

验证 `x-api-key`、`anthropic-version`、path、decoder。

## P14-006：OpenCode public 链

无认证 header，匿名 models/inference 成功。

## P14-007：Ollama custom base URL 链

无认证，custom base URL 和 `/api/tags` format 生效。

## P14-008：两个 custom compatible provider 链

同名/不同模型、不同 endpoint/key；provider-qualified 选择准确，无状态串线。

## P14-009A：missing auth hard failure

请求发送前返回 credential error；mock request count=0；legacy constructor count=0。

## P14-009B：invalid URL hard failure

Endpoint/render 阶段失败；mock request count=0；无 localhost 或 legacy fallback。

## P14-009C：unknown protocol hard failure

Protocol registry lookup 失败；mock request count=0；不得回退 Chat Completions。

## P14-009D：missing route hard failure

Provider/model 已绑定但 route 缺失时 typed error；mock request count=0。

## P14-009E：ambiguous bare model hard failure

两个 provider 同名模型时要求 provider-qualified reference；mock request count=0；不得优先 xAI。

## P14-009F：HTTP authentication failure

请求确实到达 mock，401/403 被分类为 remote auth error；request count=1；不得重试到其他 provider/route。

## P14-010：启动 no-network test

未显式 refresh 时 bootstrap/session startup 不产生任何网络请求；只加载 persisted snapshot。

## P14-011：hot reload real-chain E2E

请求 1 使用 revision N；原子改配置并 reload；请求 2 使用 revision N+1/new endpoint；请求 1 不被中途篡改。

## P14-012：TUI provider save PTY E2E

Windows、Linux、macOS 都必须通过同一逻辑流程：真实 PTY 打开 modal → 输入 config → save → revision update → model visible → mock request → clean exit。Linux 可额外做渲染 snapshot，但不得把 Windows/macOS 降为只启动不操作的 smoke。

## P14-013：legacy xAI migration E2E

旧配置/session/model ref → 显式 migration → 普通 `xai/model` 和 route chain；不得绕过 runtime。

### Phase 14 Gate

- A-11 的 E2E 部分 Closed；
- 所有 provider matrix 真实经过 bootstrap/route compiler；
- E2E 源码扫描禁止直接构造 `SamplerConfig`，sampler crate unit tests 除外；
- no-network 与 no-legacy-fallback tests 全绿。

---

# Phase 15 — CI、打包、安装和发布门禁真实化

**目标：** CI 证明三平台行为和发布产物，不再通过缩小范围或 ignored PTY 获得绿色。

## P15-001：修订 workflow triggers

`provider-adapter.yml` 对目标 feature branch、main、release branches、PR、tags `v*`、manual 触发。tag/release 必须跑 full gate。

## P15-002：Linux fast PR shards

fmt、workspace check/clippy、provider/sampler/shell、pager/tools 分 shard；所有 required。

## P15-003A：Windows workspace check job

只加入 locked workspace `check --all-targets` required job，验证失败会阻断 workflow。

## P15-003B：Windows workspace Clippy job

只加入 `clippy --all-targets -D warnings` required job；不得依赖 check job 的缓存产物宣称通过。

## P15-003C：Windows provider/sampler/config/auth shard

加入对应 package tests 和 real-chain protocol subset；zero skip。

## P15-003D：Windows shell/session/ACP shard

加入全部 shell/session/ACP tests；验证 Phase 12 恢复的 modules 实际出现在 test list。

## P15-003E：Windows pager/render/tools shard

加入 pager/render/tools tests；验证 Phase 13 exclusions 没有重新出现。

## P15-003F：Windows PTY/package/install shard

真实 ConPTY smoke、release package、临时目录安装和 mock inference；任一步失败 job 非零。

## P15-004A：macOS workspace check/Clippy jobs

分别建立 required check 与 Clippy jobs。

## P15-004B：macOS behavior test shards

覆盖 provider/sampler/shell/session/pager/render/tools 与 real-chain E2E。

## P15-004C：macOS PTY/package/install shard

真实 PTY、release package、临时目录安装和 mock inference。

## P15-005：Linux release full workspace gate

`cargo test --workspace --all-targets --locked`、docs、real-chain E2E、package smoke。

## P15-006：磁盘和时间分片

每个 job 独立 target cache key；缓存不包含未验证 binary；记录 disk；大型 pager test 分 target，但总集合必须完整。禁止因超时删除测试。

## P15-007：构建 release artifact

三平台构建实际发行二进制/包；记录 SHA-256、版本、target triple。不得只 `cargo build` 后不运行。

## P15-008：install/start smoke

在干净临时用户目录：安装/解包 → `--version` → `providers` → 加载 minimal config → mock provider request → clean shutdown。

## P15-009：config migration smoke

旧 xAI config 和当前中间版 provider config 均迁移；备份、幂等、失败不破坏原文件。

## P15-010：docs gate

`cargo doc --workspace --no-deps` 零 warning；Markdown links/config samples 由脚本验证；不允许“磁盘不足跳过”。

## P15-011：移除 baseline diagnostic workflow 或标记非门禁

保留有价值日志能力，但不能与 release workflow 混淆。所有 required status names 固定。

### Phase 15 Gate

- 三平台 required jobs 全绿；
- package/install/migration smoke 全绿；
- ignored production tests=0；
- A-11 CI 部分 Closed；A-17 必须保持 Closed，`check.ps1` failure-propagation test 在 Windows job 中再次通过。

---

# Phase 16 — 删除双架构死代码并重建可信文档

**目标：** 移除所有迁移中间态，防止未来重新走旧链；文档与实际命令同步。

## P16-001：删除 legacy provider configuration path

删除 `configure_providers` 及测试，前提是 production/test references=0。不得留 deprecated production function。

## P16-002：删除 legacy sampler fallback/constructors

只保留 sampler unit-test helper；production 构造必须由 execution adapter。

## P16-003：删除 blocking discovery 和旧 cache

确认 P9 后无引用，删除代码、静态 cache、过期 docs。

## P16-004：删除重复 registry/catalog fields/globals

provider state 只指向 runtime/view；全局若必须存在，只能是一次初始化的 runtime handle，并有 identity tests。

## P16-005：更新 architecture contract

`docs/model-adapter-architecture.md` 对齐最终类型、data flow、error semantics、platform contracts。不得写未实现 future claims。

## P16-006：重写配置参考

只修改 provider/model config reference；所有 TOML 示例进入解析测试；覆盖 custom provider/profile、auth precedence 和 public/no-auth。

## P16-007：重写迁移指南

只处理旧 xAI 配置、旧中间版 provider 配置、模型引用和配置目录迁移；示例进入 migration tests。

## P16-008：重写 hot reload、跨平台与故障排查文档

只描述已实现行为；覆盖 Windows paths、shell/PTY capability、catalog stale/error、atomic save 和恢复步骤。命令进入 docs smoke。

## P16-009：历史文档降级

旧计划、PROGRESS、final-audit 顶部标记 historical，并链接 V2 final audit。不得删除版权/审计历史。

## P16-010：静态残留扫描

必须解释或清零：

```text
configure_providers
register_route
store_config
fetch_provider_models_blocking
legacy fallback comments
provider_runtime — injected later
blocking discovery
XAI_API_KEY in generic provider
unknown bare model → xai
```

## P16-011：供应商中立性审计

输出 `provider-neutrality-audit.md`：列出所有 xAI brand branches，确认只位于 provider/auth/migration；OpenAI/Anthropic/custom provider走同一 runtime/route/sampler。

### Phase 16 Gate

- A-18 Closed；
- legacy path production references=0；
- 文档样例测试全绿；
- 静态残留无未解释项。

---

# Phase 17 — 独立最终审计、clean checkout 验证和 RC

**目标：** 不信任阶段性声明，从零 checkout 重新证明 V1。

## P17-001：创建 clean checkout

从候选 commit 新 clone/worktree，不复用旧 `target/`、环境配置或 catalog cache。记录 SHA。

## P17-002：Linux G4

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
```

再运行 real-chain E2E、PTY、package/install/migration smoke。

## P17-003：Windows G4

PowerShell 运行相同语义的全量命令和 smoke。任何 cfg skip/ignored test 计入失败。

## P17-004：macOS G4

运行相同语义全量命令和 smoke。

## P17-005：A-01～A-18 逐条再审

每条提供：源码证据、测试名、CI run URL、状态。不得仅写“已修复”。

## P17-006：failure/exclusion/ignore ledger 归零

- baseline failure ledger open=0；
- missing Windows implementation=0；
- invalid exclusion=0；
- ignored production tests=0；
- unexplained allow=0；
- docs warnings=0。

## P17-007：安全与 secret audit

使用 canary secret 跑测试/log/artifact scan；确认 release artifact、cache、diagnostics 无泄漏。检查 endpoint SSRF policy 和 redirect auth stripping。

## P17-008：最终 diff 审查

检查无无关品牌重构、无大规模格式噪声、无测试削弱、无依赖未批准、无 license/NOTICE 破坏。

## P17-009：生成 `final-audit-v2.md`

包含所有命令、exit code、test counts、platform matrix、artifact SHA、已知限制。V1 不允许列出影响核心闭环的已知限制。

## P17-010：创建 RC

仅在 P17-001～009 全部有新鲜证据后：

```bash
git tag -s v1.0.0-rc.1 -m "OpenBuild Provider Adapter V1 RC1"
```

签名/tag 命名由所有者最终确认；代理不得自行 push/tag，除非用户明确授权。

### Final Gate

只有全部满足才可报告“第一版生产闭环完成”：

- 三平台 G4 全绿；
- A-01～A-18 全 Closed；
- single runtime、no fallback、no blocking discovery、generic provider、auth/header、hot reload、TUI、catalog 全部有 real-chain E2E；
- 跨平台不是通过 test exclusion 达成；
- release artifact 在干净环境可安装、启动和完成 mock inference；
- 文档与代码一致；
- 无 secret 泄漏和未批准依赖。

---

# 8. 每个任务卡的固定执行模板

代理执行任一任务时，必须复制以下模板到 `docs/provider-adapter-v1/execution-v2/phases/phase-NN.md`：

```markdown
## <TASK-ID>: <title>

### Pre-task state
- Base SHA:
- Branch:
- Working tree:
- Entry gate evidence:

### Exact ownership
- Production files:
- Test files:
- No other files allowed:

### Red reproduction
- Command:
- Exit code:
- Expected causal failure:
- Observed causal failure:

### Implementation
- Root cause:
- Minimal change:
- Frozen interface preserved:

### Verification
- cargo fmt:
- cargo check:
- cargo clippy:
- targeted tests:
- platform tests:
- baseline delta:

### Diff discipline
- git diff --stat:
- files outside ownership: none
- new cfg/ignore/allow: none
- new dependency: none

### Commit
- SHA:
- Message:

### Result
- CLOSED or BLOCKED
- Ledger IDs closed:
```

任何字段为空，任务不能标记完成。

---

# 9. Phase 完成记录模板

```markdown
# Phase N — <name>

## Input
- Base SHA:
- Open ledger IDs:

## Completed tasks
- <ID → commit SHA>

## Gate commands
| Platform | Command | Exit | Tests | Artifact |
|---|---|---:|---:|---|

## Ledger delta
- Closed:
- Newly discovered: 0
- Remaining owned by later phases:

## Static debt delta
- Windows exclusions before/after:
- ignored tests before/after:
- allow attributes before/after:
- legacy references before/after:

## Scope audit
- Files changed:
- Unrelated changes: none
- Plan deviations: none

## Decision
- PASS / BLOCKED
```

---

# 10. 停线条件

发生任一条件立即停止，不得继续下一任务：

1. `.git` 缺失或 branch/base SHA 不明；
2. 用户改动与任务文件重叠；
3. 新增 hidden prerequisite 未在 ledger/plan 归属；
4. 需要修改本计划或冻结接口才能继续；
5. 需要新增依赖但未获批准；
6. 第三次修复尝试失败；
7. 测试只能通过新增 ignore/cfg/allow 或削弱断言；
8. Windows 修复只能通过删除功能入口；
9. secret 出现在 log/snapshot/error；
10. provider-bound 请求仍进入 legacy fallback；
11. 启动仍发生同步公网请求；
12. launcher/TUI/reloader/session runtime identity 不同；
13. phase gate 命令未实际运行或日志不完整；
14. CI 只在 feature branch 绿，而 main/tag gate 未配置；
15. release 候选仍有任何 workspace test failure 或 docs warning。

---

# 11. 最终验收矩阵

| 能力 | Linux | Windows | macOS | 真实链 E2E | 必须结果 |
|---|---:|---:|---:|---:|---|
| Runtime bootstrap | ✓ | ✓ | ✓ | ✓ | 单 Arc、revision 1 |
| Registry transaction | ✓ | ✓ | ✓ | ✓ | 失败不发布、并发 revision 唯一 |
| OpenAI Chat | ✓ | ✓ | ✓ | ✓ | 正确 endpoint/auth/events |
| OpenAI Responses | ✓ | ✓ | ✓ | ✓ | 不回退 Chat |
| Anthropic Messages | ✓ | ✓ | ✓ | ✓ | x-api-key/version header |
| OpenCode public | ✓ | ✓ | ✓ | ✓ | 明确 no-auth |
| Ollama custom URL | ✓ | ✓ | ✓ | ✓ | override 生效 |
| Two custom compatible | ✓ | ✓ | ✓ | ✓ | identity 隔离 |
| Missing auth hard fail | ✓ | ✓ | ✓ | ✓ | request count 0 |
| Invalid endpoint hard fail | ✓ | ✓ | ✓ | ✓ | request count 0 |
| Async catalog | ✓ | ✓ | ✓ | ✓ | timeout/TTL/stale/cancel |
| Atomic config save | ✓ | ✓ | ✓ | ✓ | replace/rollback |
| Hot reload | ✓ | ✓ | ✓ | ✓ | revision +1/current session update |
| ACP/session persistence | ✓ | ✓ | ✓ | smoke | 无平台跳过 |
| Shell completion | ✓ | ✓ | ✓ | smoke | shared contract + adapter |
| PTY/terminal | ✓ | ✓ | ✓ | ✓ | IO/resize/cleanup |
| Package/install/migration | ✓ | ✓ | ✓ | mock | clean environment pass |
| Docs | ✓ | same source | same source | n/a | zero warnings |

---

# 12. 非目标

V1 不要求：

- 动态加载第三方 Rust Provider 插件；
- 新增第四种 wire protocol；
- 大规模品牌/crate 全量改名；
- 真实公网 provider CI；
- 云端 secret provisioning；
- 与 Provider 闭环无关的 UI 重设计。

这些事项不得被代理加入本计划，也不得被用来推迟当前闭环。

---

# 13. 执行起点

执行代理必须从 **P0-001** 开始。即使它认为某些环境或测试已经验证，也必须重新产生当前 base SHA 对应的新鲜证据。任何旧 `PROGRESS.md`、旧 CI 截图或上一代理的“passed”声明都不能替代本计划门禁。
