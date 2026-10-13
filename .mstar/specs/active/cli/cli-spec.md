# Nexus CLI Spec

**Status**: Normative  
**Document class**: Master  

`creator run`、`creator bootstrap`、`creator works intake|resume-chain`、`preset run`、`creator character run|soul reflect`、reference refresh、`compute run` 与 `capability list` 没有 CLI 入口——它们的完整 production ports、runtime catalog 或 terminal/capture observation 不作为完整直连 CLI 操作提供。底层 core Work/execution/Host/capture/SOUL/reference/compute API 与既有 TS routes 保留；不得宣传未交付的等价 TS workflow。TS 暴露 schedule add/signal（非全部旧 schedule 操作）、reference list/get（非 refresh），且不暴露 compute run。

保留的本地表面：`creator demo-seed`、`creator world event-add`、chronology `set|show|advance`、Work/pool inspiration、holder-governed Character CRUD/bindings/knowledge/memory/ToM、reading 与 directives。`creator kb --scope world`、World create `--name`、`system preset`、隐藏顶层 `sync`、`preset validate --offline` 均不存在；canonical 拼写是 World KB、`--title`、`preset`、`platform sync`、positional `preset validate <path>`。无 no-op alias。Graph/entity patch 保留 `--expected-version`；Character edits 保留显式 revision CAS。

Ordinary CLI 使用默认 `cli` cohort；可选 `connect-host` 独立。Connect runtime 保留 scoped compute/WASM 与共享 module cache，无 ACP/agent-host、scheduler startup、HTTP/SPA 或 Node。Core Model B/MCP libraries 保留；无 TS launcher 替代已删除的 Model A child descriptor。见 [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) §3.2 的依赖与生命周期合同。

## 0. 文档定位

本稿是 nexus-platform `v1-spec/architecture.md` 的下钻文档，聚焦 Nexus 本地 CLI / runtime 的产品行为、运行模型与集成边界。

当前冻结实现前提：

- 平台仍采用 TypeScript / Next.js / Vercel AI SDK 路线
- 本地 runtime 调整为 `Rust-first`
- 原因是 ACP 已成为 CLI 主协议，而 ACP 官方当前支持 Kotlin、Java、Python、Rust、TypeScript SDK，不支持 Go SDK

本稿覆盖：

- CLI 的目标与非目标
- 命令体系
- 本地 runtime 生命周期（无 daemon 宿主命令组；见 §6.3）
- ACP-first 的能力面
- skills-second 的兼容路径
- 平台登录与会话模型
- 本地工作区结构
- 本地 SQLite 职责
- 结构化同步模型
- 失败恢复和实现分期

本稿不覆盖：

- 平台 HTTP API 的字段级 schema
- ACP 的最终 wire-level 协议细节
- 服务端数据库表结构

### 0.1 品牌、CLI 名称与版本节奏

- **产品名**：对外统一为 **Nexus**。
- **CLI 可执行名**：**`nexus42`**（与 **42ch / Creative Hub** 品牌同源；下文命令示例一律使用 `nexus42`）。无本地 daemon single-binary runtime mode（无 `nexus42 daemon start` / hidden `daemon-run`）；不引入第二个 CLI 产品二进制（`nexus-runtime` 仍是同一 `nexus42` crate 的独立 Connect-host 二进制，见 [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) §3.1–§3.2；[daemon-runtime.md](../../archived/runtime/2026-09-21-daemon-runtime.md) 为退役宿主记录）；Electron/TS 持有长期运行的服务进程。
- **`v1-notes/ideas/` 与 `v1-notes/` 的扩展需求**：视为路线图输入。CLI 必须保证：**协议与 schema 的可扩展字段**、保留的 `/v1/daemon/*` wire 家族（由 TS 服务提供，见 §6.7）/ ACP 能力面的**演进位**、以及已写入合同的能力（如 **`research.*` 等 ACP 能力名**、context assembly、`manuscript_phase` 等）的**最小可用实现或安全默认（no-op）**，避免把后续实现空间钉死。

### 0.2 V2 重定位（pre-release）

本节记录 V2（pre-release）重定位方向。与旧叙述冲突时，以本稿 §6.0A–§6.0B 为准。

- **定位**：Nexus CLI 是 **ACP-first 控制面** + **Creator 本地知识面**，不是执行逻辑聚合层。
- **执行边界**：推理、工具调用、文件输出等执行能力统一经 ACP capability invocation。
- **运行边界**：编排运行态由 TS/Electron 与 `nexus-core` execution 持有；CLI 不提供 daemon 控制面（控制、声明与可观测经 CLI one-shot 命令与 TS 服务）。
- **知识边界**：`SOUL` / `memory` 归属 Creator；CLI `creator kb`（work-scope 索引；无 `--scope` flag）仅表示活跃 Creator + workspace 下的**本地工作资料索引**；World/narrative KB 归属 `nexus-kb` + `nexus-narrative`，User/global knowledge 归属 `nexus-knowledge`。

---

## 1. 冻结定位

CLI 在 Nexus 中的冻结定位如下：

1. **CLI 是 platform connector + local runtime + agent bridge。**
2. **CLI 不是通用 LLM client，不应把特定模型 SDK 作为主路径写死。**
3. **CLI 的主集成协议是 ACP，skills 是兼容层。**
4. **完整小说正文默认本地私有；CLI 默认同步的是结构化变更，而不是全文。**
5. CLI 不通过后台常驻进程长期运行；长期运行由 Electron/TS 承担；CLI 保持 one-shot / 直连 core；平台登录能力保留在 `platform auth`。
6. **CLI / runtime 需要能够面向 ACP Registry 中的兼容 agents 做发现、选择和连接。**

---

## 2. 设计目标

### 2.1 功能目标

- 为 Nexus 用户提供唯一稳定的本地入口
- 管理本地工作区、缓存、同步状态和运行时上下文
- 为本地 agent 提供标准能力面
- 为平台提供可靠的结构化同步出入口
- 支持用户显式发布内容，但不默认上传全文

### 2.2 体验目标

- 单二进制安装
- 首次上手路径尽量短
- 普通用户不需要理解底层协议名词
- 高级用户可以脚本化、自动化和嵌入自己的 agent 工作流

### 2.3 系统目标

- 可恢复
- 可审计
- 幂等优先
- 本地优先
- 与模型厂商解耦

---

## 3. 非目标

v1 中，CLI 不负责以下事情：

- 提供 `nexus42 llm ...` 这类直接模型调用能力作为主路径
- 充当完整写作编辑器
- 静默把本地全文同步到云端
- 取代用户现有的本地 agent 运行环境
- 在 v1 内冻结 ACP 的最终线协议细节

---

## 4. 心智模型

可以把 Nexus CLI 理解为一个本地控制平面：

1. **Workspace manager**
   - 管理本地目录结构、配置、缓存和索引
2. **Sync engine**
   - 将本地结构化变更推送到平台
   - 将平台结构化状态拉回本地
3. **Agent bridge**
   - 通过 ACP 把稳定能力面暴露给本地 agent
4. **Session guard**
   - 维护用户平台登录态、profile 和运行环境

这意味着用户日常真正使用的是：

- 自己熟悉的本地 agent
- 本地文件与草稿
- 一个持续运行的 Nexus 本地 host —— 该长期运行进程不是 CLI 进程，而是 Electron 应用内的 TS 服务（`apps/nexus-service`，Electron 持有其生命周期）；`nexus42` 保持 one-shot / 直连 core

而不是“在 CLI 里重新对话一次”。

### 4.1 ACP 在 Nexus 中的角色

在 Nexus 的本地架构里，ACP 不是一个可选增强，而是主协议层：

- Nexus runtime 通过 ACP 与兼容 agent 协商能力
- 对本地 agent，首选 `JSON-RPC over stdio`
- 对远程 agent 的 `HTTP / WebSocket` 支持保留给 future-compatible 场景
- Nexus 优先从 ACP Registry 发现 agent，而不是内置某一个模型供应商

---

## 5. 权威边界

### 5.1 本地权威

- 小说全文
- 草稿和私有笔记
- agent 配置
- 工作目录与文件组织

### 5.2 平台权威

- World / Creator 元数据
- Key Block 结构化状态
- Timeline / Fork 图
- Explore 所需索引
- 订阅和权限信息

### 5.3 CLI 同步边界

CLI 默认只同步：

- 结构化 delta
- 摘要
- 引用锚点
- 命令与审计信息

CLI 不默认同步：

- 完整章节正文
- 推理链全文
- 用户私有工作文件

---

## 6. 用户可见命令面

命令设计遵循两个原则：

- 顶层命令尽量面向用户意图，而不是内部模块
- 关键命令必须稳定、可脚本化

**Command-surface lock:** the visible top-level command groups are `creator` / `acp` / `platform` / `system` (plus feature-gated `connect`). Hidden retained groups are `preset`, `compute`, `capability`, `ops` and `desktop`; a leaf inside a retained group stays only with a complete direct core, cloud or Connect operation. There are no top-level `nexus42 sync` or `system preset` aliases, and no dormant stubs or compatibility spellings. Authority for *which leaf stays* is the retain/remove rules in [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) §6.


### 6.0A 与 Platform Creator 三层标识兼容约束（V2 权威约束）

对齐 nexus-platform `v1-spec/platform/creator-agent-registration-v1.md` §「三层标识体系」，CLI 在 V2 命令重组中 **必须** 保持以下不变量：

1. **内部主键始终是 `creator_id`**（`creator.id`）；CLI 不得把 `display_name` 当身份主键。
2. `creator use` 可接受 `creator_id` 或 `handle` 输入，但落盘活跃主体时必须规范化为 `creator_id`。
3. `creator status` / `creator list` 需稳定显示三层标识：`creator_id`（权威）、`handle`（可路由别名）、`display_name`（UI 展示）。
4. 任何 `creator soul|memory|kb` 子命令，均绑定当前活跃 `creator_id`，不得以 `display_name` 隐式路由。
5. `sync` / `context` / `publish` 等 Creator-context 请求，仍遵守 nexus-platform `v1-spec/platform/auth-session-model-v1.md` §4 的身份头约束（Creator API key 路径 vs User+`X-Creator-Id` 路径）。

### 6.0B V2 命令信息架构（权威）

V2 命令面按以下顶层执行（pre-release 允许破坏性调整）。

- `nexus42 creator`：**Creator 身份 hub** — Work（`works` 子命令组）、workspace、SOUL、memory、kb/knowledge、world（默认用户创意入口）。无 `creator run` / `creator bootstrap` 执行入口；Work/World/Character 的直连 core 读写保留
- `nexus42 daemon`：不存在（无 daemon 命令组，无 launcher / status / ui 别名 — §6.3）
- `nexus42 acp`：独立 ACP 能力面（探测、协商、调用、诊断）
- `nexus42 platform`：User 会话 — auth、**sync**、explore、context、publish（cloud 边界；local-only 时可跳过）
- `nexus42 system`：本机配置、诊断、completion、identity（无 `system preset` 子命令；preset 面为隐藏顶层 `preset` 组 — 见 §6.3A）

**Sync surface**：

- **Canonical**：`nexus42 platform sync pull|push|status|resolve`（§6.5）
- 无顶层 `nexus42 sync ...` alias，无 no-op 兼容拼写。

设计约束：

- 无 CLI daemon 运行控制；`acp` 保留 registry/probe/agent/session/policy/permission/run，不提供 `acp status|doctor` loopback 诊断。
- `creator` 统一承载 Creator 本地知识资产：`soul` / `memory` / `kb`（work-scope 索引；无 `--scope` flag）作为 Creator 子命令，不再分散为平级心智入口。
- `creator kb` 只承载 work-scope 文件索引（work 是唯一 scope；无 `--scope` flag）；World-scoped narrative KB 必须走 canonical `creator world kb ...`（`nexus-kb` + `nexus-narrative`）；User/global knowledge 不属于 `creator kb`，应走 `nexus-knowledge` 对应的 CLI 入口。

### 6.1 `nexus42 system`（系统命令组）

- `nexus42 system version`
- `nexus42 system doctor`
- `nexus42 system completion`
- `nexus42 system config get|set|unset|path`
- `nexus42 system debug dump-workspace|replay-delta`

说明：原 `version/doctor/completion/config/debug` 统一归并到 `system` 顶层。

### 6.2 `nexus42 creator`（身份 + 本地知识资产）

### 6.2A Creator 身份模型（与平台合同）

与 **User** 会话并列，平台将 **Creator** 作为独立认证主体：**独立注册**（可无 User）、**统一** `creator_id` + **`creator_api_key`**（HTTPS，`Authorization: Bearer`）+ **`api_key_ref`**（落库引用 / ACP 侧，不明文存完整密钥）（见 nexus-platform `v1-spec/platform/auth-session-model-v1.md` §1.1、§2.2–§2.5）、再经 **Pairing** 可选绑定 User。HTTP 资源见 nexus-platform `v1-spec/platform/platform-api-v1.md` §1.1、§1.5、§3.0、§3、§3A。

- **HTTP 层（合同一致）**：**同一路由**的 JSON body / 成功响应 **不因**使用 User 还是 Creator 凭证而变；差别只在头。**User** 调用 Sync / Context / Publish 等 **Creator-context** 接口时须带 **`X-Creator-Id`**，且与 body 内 `creator_id`（若有）一致（`platform-api` §1.5、`auth-session-model` §4.0–§4.1）。**Creator** 仅带 **`Authorization: Bearer <creator_api_key>`** 即可解析 `creator_id`，**不要**求 `X-Creator-Id`（§4.2）。业务层始终按 **Creator 对世界与资源的权限**校验。
- **独立注册**：对齐 **`POST /api/v1/creators/register`**；注册前可用 **`nexus42 acp probe`** 采集能力与传输元数据（[`registry-integration.md`](../agents/registry-integration.md) §2.1）。

### 6.2B `nexus42 creator` 身份子命令（权威）

| 子命令 | 作用 |
| --- | --- |
| `nexus42 creator register` | 调用 **`POST /api/v1/creators/register`**；提交描述、能力声明、agent 材料；落盘 **Creator** 凭证（与 User token 分存储） |
| `nexus42 creator status` | 展示当前激活 **`creator_id`**、Pairing、**`creator_api_key`** 是否已配置 / 有效、可选 **`api_key_ref`** 摘要（不明文打印密钥） |
| `nexus42 creator use <creator_id_or_handle>` | 将活跃主体设为给定 Creator（内部规范化落盘为 `creator_id`） |
| `nexus42 creator list` | 在 User 已登录时拉取可见 Creator 列表 |
| `nexus42 creator pair` | 命令面保留；Pairing 依赖尚未可用的 platform API——当前始终报错 `creator pairing requires the platform API (not yet available)`，不建立 Pairing |
| `nexus42 creator unpair` | 命令面保留；Pairing 依赖尚未可用的 platform API——当前始终报错 `creator unpairing requires the platform API (not yet available)`，不撤销 Pairing |
| `nexus42 creator logout` | 清除本地 Creator 凭证（不默认清除 User 会话） |
| `nexus42 creator credentials rotate` | 轮换 `creator_api_key` 或重绑 `api_key_ref` |

实现约束：

- **默认操作主体**：未显式指定时，Creator-context 调用（如 `sync`）使用的 **`creator_id`** 必须与 `creator use` 当前活跃主体一致；本地 `state.db` 解析自 **当前活跃 Creator + 当前活跃 workspace_slug**。
- **凭证隔离**：User refresh/access 与 `creator_api_key` 分桶存储。

### 6.2B.1 Idempotent local bootstrap (AR-89)

`creator register --local --name <n>` 与 `system identity create --persistent
[--name <n>]` 共享同一 bootstrap helper（AR-88），重跑时按 **display_name
三腿决策树** 收敛（PL-5），绝不静默换身份：

| 输入 | 匹配（persistent `local_identities` 中 trim 后 byte-exact `==` 的 display_name 行数） | 行为 | 退出码 | stdout 诚实性 |
| --- | --- | --- | --- | --- |
| 具名 | 0 | mint（新 `ctr_local*` + 激活 + workspace `creators` 行） | 0 | "Created persistent identity: …" |
| 具名 | 1 | converge 该 id（`ensure_creator_row` 修复缺失行 + 必要时激活 = session selection） | 0 | **不得**出现 "Created"；no-op 声明已收敛 / repair 声明 "Workspace creators row materialized for `<id>`" |
| 具名 | 2+ | 失败：`creator_name_collision` | **1** | stderr 前缀 `creator_name_collision:` + 名称 + 匹配数 + 现有 id 列表 + 提示 `nexus42 system identity use <id>` |
| 无名 | 活跃 id 解析为 persistent 本地身份 | converge 该 id（修复行；激活已成立） | 0 | 不得出现 "Created" |
| 无名 | 无活跃 persistent 身份 | mint 无名 persistent（行 display_name = creator_id） | 0 | "Created persistent identity: …" |

- **匹配键**：`str::trim` + byte-exact `==`，**无** case-fold、**无** Unicode
  归一化、**无** 前缀/子串匹配（AR-89 #1）。R3 空白名拒绝仍在 helper 前门。
- **错误面**：专用 `CliError::CreatorNameCollision`（非 `Other`），机器可解析
  stderr 前缀 `creator_name_collision:`，exit 1（默认映射，无新 exit 词汇）。
- **匿名** `--anonymous` 不进入决策树（AR-88 #5，不收敛）。
- 唯一允许的激活切换：新 mint 的 id、单一 name 匹配的 id、无名路径上
  已活跃的 id（AR-89 #2 — "never silent takeover" pin）。

### 6.2B.2 `creator list` local-identity visibility + `--json` (AR-90)

`creator list` 合并平台缓存/auth 行与 persistent `local_identities` 行
（SSOT，**非** identity-cache；按 `creator_id` 去重、排序不变），新增
**ORIGIN** 列（`local` | `platform`）：

- **本地行**（`origin = local`）：display_name 取自 `local_identities`
  （权威）；HANDLE 渲染 `-`（不 overload HANDLE，PL-6）；匿名 / ephemeral
  身份**不出现**在本列表（`system identity list` 是身份调试面）。
- **平台行**（`origin = platform`）：id / handle / display_name / active
  语义与来源（identity cache ∪ auth store）完全不变，byte-stable。
- **`--json`**（本命令新增，无 JSON 模式先例）：机器合同 DTO **逐字**
  输出 —— JSON **数组**，元素恰为
  `{ "creator_id": string, "handle": string|null, "display_name":
  string|null, "active": boolean, "origin": "local"|"platform" }`；
  键名 `origin`（非 `kind`，与 capability catalog wire vocabulary 一致）；
### 6.2B.3 Validation bounds, TOCTOU fence, list degradation

- **display_name 前门校验**（helper 唯一副本，qc2 S#4 / qc1 S#1/S#6）：拒绝
  空白-only（R3）、含控制字符（含内嵌换行 / NUL，会破坏人形表格与 collision
  stderr 行）、超过 **64 字节**（`MAX_CREATOR_NAME_LENGTH`，与平台 register
  的 WS-B T4 同界）的名称。byte-exact 匹配语义不变（无 case-fold / 无
  Unicode 归一化）。
- **TOCTOU 兜底**（qc3 S-002）：`local_identities` 上唯一部分索引
  `(display_name) WHERE identity_type = 'persistent' AND display_name IS
  NOT NULL` —— 并发 0-match 双 mint 时第二个 INSERT 以
  `SQLITE_CONSTRAINT` 失败，helper 映射为与 2+ 决策树同形的
  `creator_name_collision:`（诚实碰撞，非静默重复）。无名 persistent
  （display_name = NULL）不受约束。迁移对历史重复名确定性收敛（每名保留
  最小 creator_id 行，其余 display_name 置 NULL —— 行不删除，仅不再按名
  匹配）。
- **`creator list` 本地源降级**（qc3 S-003）：当 `~/.nexus42/state.db` 存在
  但锁定 / 损坏 / 不可读时，跳过 local 行并向 stderr 输出
  `warning: local identities unavailable (...); showing platform rows only.`，
  平台行照常渲染，退出码仍为 0（含 `--json`，stderr 不污染 stdout 合同）。
- **no-op 写自由**（qc3 F-002）：完全收敛的 no-op 重跑对两个 `state.db`
  均**零写入**（`mode=ro` 只读校验，跳过 `Schema::init` 的 migrations +
  seed_versions）；仅在 mint / repair / session-selection 时才写。

### 6.2C `nexus42 creator workspace`（本地 workspace 子命令）

`workspace_slug` 在同一 `creator_id` 下唯一，并映射到 `"$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/"`。默认 slug 为 `default`。

| 子命令 | 作用 |
| --- | --- |
| `nexus42 creator workspace list` | 列出当前活跃 Creator 下的 `workspace_slug` |
| `nexus42 creator workspace create <workspace_slug>` | 新建 workspace 登记与 operational 树 |
| `nexus42 creator workspace use <workspace_slug>` | 切换活跃 workspace |
| `nexus42 creator workspace init` | 在当前 `creator + workspace` 上下文登记创作根与 operational 元数据 |

**Retired leaves — not part of the current surface.** The group's parser surface is exactly the four rows above: `CreatorWorkspaceCommand` (`apps/nexus42/src/commands/creator/mod.rs`) declares `List` / `Create` / `Use` / `Init` only. Invoking any removed spelling is clap's "unrecognized subcommand" error (exit 2), never a hidden or deprecated stub; `retired_creator_workspace_clone_is_unknown` (`apps/nexus42/tests/integration.rs`) pins `list` / `create` / `use` / `init` as the retained help surface and the clone spellings as unknown.

说明：

- `init` 默认不创建固定业务树（`Stories/` / `References/`）；用户可见产出由 preset 策略创建。
- `workspace_slug` 创建后永久绑定其 `creator_id`，禁止改绑。

**C2 闭合**：

- 同一运行时仅一个活跃 `creator_id`。
- 同一活跃 Creator 下仅一个活跃 `workspace_slug`（默认 `default`）。
- 支持多 world 并发，但每个 job/sync 请求必须显式携带 `world_id`。

### 6.2D `nexus42 creator soul|memory|kb|world|knowledge`（知识资产子命令）

- `nexus42 creator soul ...`：维护 `SOUL.md`（`Personality` / `Experience`）
- `nexus42 creator memory ...`：长期记忆与回顾沉淀管理
- `nexus42 creator kb ...`：**work-scope** 知识资产索引（唯一 scope；World-scoped narrative KB 走 canonical `nexus42 creator world kb ...` — 见 §6.2G.2）
- `nexus42 creator world ...`：World 创建、浏览与 narrative 状态查询（`create` 返回 `world_id`；`list`/`show` read-only；platform fork 不在本地范围 — PD-01）
- `nexus42 creator knowledge ...`：User knowledge / reference 管理入口（`nexus-knowledge`）
- `nexus42 creator demo-seed ...`：演示数据填充（world + KB seed）

补充叶子：

- `nexus42 creator memory pending-list` — list items in `memory_pending_review` awaiting review
- `nexus42 creator memory pending-show <id>` — show detail of a single pending memory item
- `nexus42 creator memory pending-dismiss <id>` — dismiss a pending memory item (no promotion)
- `nexus42 creator soul refresh-experience` — deterministic one-shot SOUL `## Experience` aggregation via embedded preset; updates `SOUL.md` Experience section
- `nexus42 creator kb queue-extract <work-entry-id> --world-id <id>` — enqueue a work entry for KB extraction into a World (idempotent)
- `nexus42 creator kb extract-status [--job-id]` — check extraction job status (all jobs or specific)

**World context debugging**: To debug the World context block injected into `novel-writing` prompts, use the canonical World KB surface `nexus42 creator world kb list|show --world-id <id>`. No new subcommand is needed; the prompt-time block is assembled by `nexus-moment-context-assembly` (`build_chapter_kb_block`) and passed as `world_kb_block` template var.

`creator kb` scope 约束（对齐 [`entity-scope-model.md`](../architecture/entity-scope-model.md) §5.3）：

- **work scope（唯一 scope）**：表示活跃 `creator_id` + 活跃 `workspace_slug` 下的 **CLI local work KB index**。实现直接读 `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/...` 下的本地文件 / `index.json` 工作索引——无 HTTP 回退。它是工作资料/文件索引，**不是** `nexus-kb` 的 World graph，**也不是** `nexus-knowledge` 的 User/global knowledge index。World-scoped narrative KB 只通过 canonical `nexus42 creator world kb ...` 暴露（§6.2G.2）。
- **User/global knowledge（未来目标）**：不得塞进 `creator kb` 或 `creator kb --scope user`。User-scoped global knowledge/reference material 应通过 `nexus-knowledge` 的 CLI 入口暴露；在六组顶层命令锁定下，推荐入口为 `nexus42 platform knowledge ...`（或等价的 platform/user knowledge 子命令），并由 `nexus-knowledge` 处理存储、标签检索与供 Moment assembly 读取的切片。**Durable roadmap:** DR-52 (user/global knowledge entry surface).

命名与行为建议：

- 保持 `nexus42 creator kb` 作为现有命令组；它是 work-scope 文件索引，help/文案中写明“local work index”。
- 未引入 `creator assets` / `creator work-index` 等别名；`nexus42 creator kb` 是唯一 work-scope 拼写，无 no-op 兼容拼写。
- **不推荐硬改为泛化 KB**：不要把 `creator kb` 解释成“所有知识入口”。World KB、User knowledge、Creator memory 三者的 owning crate 与 entity scope 不同，CLI 只能做路由，不能在 `nexus42` 内实现第二套领域模型。

### 6.2E KB / knowledge 术语禁用简写

在本规格及后续架构 / 实现文档中，`KB` 一词必须按 [`entity-scope-model.md`](../architecture/entity-scope-model.md) §5.4 限定语义后使用：

- **World KB** / **narrative KB**：指 `nexus-knowledge` 所有的 World-scoped narrative KB graph（KnowledgeEntries、SourceAnchors、graph insertion/query），由 `nexus-narrative` 协调 World/Timeline/Event 语境。
- **User knowledge** / **global knowledge index**：指 `nexus-knowledge` 所有的 User-scoped global knowledge/reference material。
- **CLI local work KB index** / **local work index**：指 `nexus42 creator kb` 的活跃 Creator + workspace 本地文件索引（work scope 是唯一 scope）。

禁止在存在歧义的上下文中单独写“KB”来同时指代以上三者；CLI help、错误提示、spec、ADR、计划任务均应使用限定词。

### 6.2F KB / knowledge target CLI model

KB / knowledge 相关 CLI 路由目标固定为：

| User intent | CLI command model | Required scope inputs | Owning crates / modules | Behavior |
| --- | --- | --- | --- | --- |
| Manage local work files / notes as workspace assets | `nexus42 creator kb ...` (work-scope index; no `--scope` flag) | active `creator_id`, active `workspace_slug` | `nexus42` command router + local workspace storage; later storage may move behind local-domain crates | List/search/show/add/remove local work index entries only. Must not create World KnowledgeEntries or User knowledge rows. |
| Manage narrative knowledge inside a World | `nexus42 creator world kb ... --world-id <world_id>` (canonical — §6.2G.2) | active `creator_id`, `workspace_slug`, explicit/resolved `world_id` | `nexus-narrative` + `nexus-knowledge` | Route to World-scoped narrative KB graph. Must preserve KnowledgeEntry / SourceAnchor provenance and narrative ownership. No silent fallback to work index. |
| Manage User/global reference knowledge | `nexus42 creator knowledge ...` | authenticated User / Pairing context; optional Creator only as acting context, not owner | `nexus-knowledge` | Store/search/list user-scoped global knowledge/reference material. May be pulled into Moment assembly; promotion into World KB is an explicit cross-scope operation. **Durable roadmap:** DR-52 (user/global knowledge entry surface). |
| Create / browse World narrative state | `nexus42 creator world create\|list\|show ...` | active `creator_id`, workspace_slug; `create` requires `--title`; narrative kind is implicit | `nexus-narrative` + `nexus-kb` | `create` returns `world_id` and persists World row. `list`/`show` are read-only. No local fork (PD-01: fork is platform-only). |
| Seed demo data | `nexus42 creator demo-seed ...` | active `creator_id`, workspace_slug | `nexus-creator` + `nexus-narrative` + `nexus-kb` | Populate demo world + KB entries for testing. |
| Assemble direct platform cloud context | `nexus42 platform context assemble` | `--world-id`; optional workspace/creator and include/limit flags | Future direct platform context assembly path | **Deferred.** Platform cloud assembly is not yet available; CLI exits with clear guidance to use `assemble-moment`. |
| **Assemble local four-domain Moment snapshot (single SSOT)** | `nexus42 platform context assemble-moment` | optional `--world-id`, `--user-id`, `--branch-id`, `--event-id`; **frozen flags:** `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit` | `assemble_moment` in `nexus-moment-context-assembly` reading Stage-0 context plus local narrative, World KB, and User knowledge slices | **Shipped (local).** Single assembly SSOT. Runs in-process and calls `assemble_moment`; narrative and World KB are read through persistent local stores, while User knowledge reads from SQLite. No platform cloud assembly and no daemon context-assemble route. |


### 6.3 Runtime 控制面（无 daemon 命令组）

CLI 不提供 daemon 运行控制：`nexus42 daemon …` 与隐藏 `daemon-run` 是 unknown commands，整组（生命周期控制、`schedule` 控制面、`ui`/`web` 别名）不存在。长期运行 HTTP 由 Electron 下的 TS 服务（`apps/nexus-service`）持有；Web UI 由 Electron 宿主提供。

- Schedule 控制不是 CLI 面；唯一保留的调度声明面是 `creator works cron`。
- 不存在 `daemon orchestrate` 兼容面——不要在新的 plan 或 runbook 中文档化它。
- Session 控制通过 TS 服务的 orchestration schedules 控制面（`current_session_id` + supervisor signal cascade，见 [creator-schedule-and-core-context.md](../orchestration/creator-schedule-and-core-context.md) §3.3）。
- 章节 outline 与结构编辑以 Web UI 为主入口（`/v1/daemon/works/{work_id}/chapters/*` wire 家族由 TS 服务提供）；CLI 保持既有 creator/Work/chapter 工作流的 parity，作为 power-user 与自动化表面。

### 6.2D `creator run`（无 CLI 入口）

CLI 不暴露任何 `creator run` 入口（clap `creator` 组无 `Run` leaf）；底层 preset/execution 库保留，作为 core/TS 消费方的执行能力。历史表面的权威记录：[creator-run-preset-entry.md](../../archived/creator/2026-09-21-creator-run-preset-entry.md)（退役前的 Shipped Master）。


### 6.2G `nexus42 creator world`

Normative World binding: [novel-writing/workflow-profile.md §3.5](../novel-writing/workflow-profile.md).

| Command | Purpose |
| --- | --- |
| `nexus42 creator world create --title "<text>" [--slug "<slug>"] [--description "<text>"]` | Create a World; returns `world_id` (`wld_<uuid>`). Used by `novel-project-init` grill-me "create new World" path. `--title` is the only name flag (no `--name` alias); narrative kind is the implicit default (`--kind` is not exposed). |
| `nexus42 creator world list` | List Worlds visible under active `creator_id` + `workspace_slug`. |
| `nexus42 creator world show <world_id>` | Show World metadata and summary counts (read-only). Clean not-found with remediation if missing. |

Rules:

- `create` is idempotent by name only when PM/plan defines dedup policy; default is new row per invocation pre-1.0.
- No local fork or platform merge mutations (PD-01).
- Work creation/init must bind a World: either run `nexus42 creator world create --title "..."` and pass/bind the returned `world_id`, or pick an existing id from `nexus42 creator world list`.
- New-Work World binding fails closed with remediation to `creator world create --title` or `creator world list` (not skip/stay worldless); the Work-creation capability stays with its core/TS consumers.
- `show` for a nonexistent `world_id` prints remediation pointing to `creator world create --title` or `creator world list`.


**`creator world kb adopt` surfaces LLM extraction metadata.**
When a `pending` candidate is produced by the `nexus.llm.extract` pathway
(see [llm-extract.md](../orchestration/llm-extract.md)), the adopt output surfaces two
additional fields so the author can judge extraction quality before confirming:

- `confidence`: the LLM self-reported confidence (`0.0`–`1.0`), read from
  `kb_extract_jobs.llm_confidence` (falls back to the `proposed_payload` JSON
  `confidence` key for backward compat). Heuristic rows report `confidence: -`
  (column `NULL`).
- `source_quote`: the verbatim chapter excerpt justifying the extraction, read
  from `kb_extract_jobs.llm_source_quote` (falls back to the `proposed_payload`
  JSON `source_quote` key). Heuristic rows report `source_quote: -`.

Example (LLM pathway): `confidence: 0.92 / block_type: scene / source_quote:
"...the eastern gate groaned open..."`. Example (heuristic fallback):
`confidence: - / block_type: character / source_quote: -`. The `--json` output
includes `llm_confidence` and `llm_source_quote` keys (nullable). The promotion
gate (§5.5.3 of entity-scope-model) and `ValidationMode::Novel` re-run are
unchanged.


**`creator kb rescan --work <work_ref>` cross-chapter reconciliation.**
`creator kb rescan` has two mutually-exclusive modes — the chapter-scoped
positional target and the work-scoped `--work` flag. Exactly one of the
positional `<work_ref>/<chapter>` target or the `--work <work_ref>` flag must
be supplied; supplying both (or neither) fails closed with remediation.

| Command | Purpose |
| --- | --- |
| `nexus42 creator kb rescan <work_ref>/<chapter> [--dry-run] [--json]` | Chapter-scoped rescan. Re-syncs `kb_extract_jobs` candidates + confirmed KnowledgeEntry bodies from one chapter's current text. |
| `nexus42 creator kb rescan --work <work_ref> [--dry-run] [--json]` | Work-scoped rescan. Iterates **all** chapters in `Works/<work_ref>/Stories/`, aggregates candidates by `canonical_name` across chapters, and reconciles so a recurring entity collapses to a single `pending` candidate carrying cross-chapter provenance (e.g. `source_chapters: [3,5,7]`). |

Rules (build on the §6.2G rules; see also
[world-kb-runtime-architecture.md §5.5.1](../architecture/world-kb-runtime-architecture.md)):

- **Mutual exclusivity.** `--work <work_ref>` and the positional
  `<work_ref>/<chapter>` cannot be combined. `--work` resolves the Work by
  `work_ref` / `story_ref` / `work_id` (same resolver as the positional path).
- **Author gate.** Same `require_world_owner` (`narrative_worlds.owner_creator_id`
  must match the active creator) → `403 WORLD_KB_FORBIDDEN` on mismatch. No
  new error code.
- **Reconciliation.** The DB uniqueness `(creator, canonical_name, world)` —
  enforced by migration — is what collapses N
  per-chapter same-name candidates into 1 row. The work-scoped path upserts
  **once per aggregate**; the merged row's `source_chapter_id` is the lowest
  referencing chapter and its `proposed_payload` records the full
  `source_chapters` array. `confirmed` rows are terminal (§5.5.2); only their
  KnowledgeEntry body is refreshed via `diff_and_apply`.
- **`--dry-run`.** Shows a cross-chapter reuse summary before any DB write,
  e.g. `Entity 'Aelin' referenced in chapters 3, 5, 7; existing KB row found
  → no new candidate`. The dry path is read-only and acquires **no** advisory
  lock.
- **Advisory lock (T-B P0).** The non-dry work-scoped path acquires
  `Works/<work_ref>/.lock` before the cross-chapter upsert (the same lock as
  `creator world kb adopt` and `creator works cron set`).
  Contention → `E_LOCK` exit 75 (`EX_TEMPFAIL`); I/O failure → `E_LOCK_IO`
  exit 78 (`EX_CONFIG`). Chapter-scoped rescan does **not** acquire the lock
  (single-chapter upsert).
- **Extraction pathway.** Work-scoped rescan uses the heuristic
  (`extract_candidates_from_text`), identical to the chapter-scoped path, so
  the two modes agree on the same prose. The `canonical_name` grouping key is
  the T-A P0 first-class field. Wiring the `nexus.llm.extract` LLM pathway
  into rescan is out of scope (LLM extraction is review-time/finalize-time;
  rescan is a sync tool).


**`creator world kb pending --missing-only`.**
When a `novel-writing` chapter finalizes, the supervisor writes an advisory
missing-KB log under
`Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md`
(see [novel-writing/quality-loop.md](../novel-writing/quality-loop.md) §5.5). The
`--missing-only` flag switches `creator world kb pending` from listing DB
`pending` candidates to scanning those log files for the requested World.

| Command | Purpose |
| --- | --- |
| `nexus42 creator world kb pending <world_ref> [--missing-only] [--limit N] [--json]` | Lists candidates for the World. Without `--missing-only`: lists `pending` `kb_extract_jobs` rows (default). With `--missing-only`: lists advisory `missing` candidates extracted at finalize time. |

Rules:

- `--missing-only` scans `Works/<work_ref>/Logs/kb/missing/*.md` for every Work
  bound to the given `world_ref` (`world_id`). Each log file is parsed for YAML
  frontmatter; candidates whose `world_id` matches are collected.
- Text output shows `CHAPTER`, `TYPE`, `NAME`, and a truncated `SOURCE` quote,
  prefixed with a `MISSING` label so authors can distinguish advisory finalize-time
  gaps from review-time `pending` candidates.
- `--json` output includes `chapter`, `world_id`, `canonical_name`,
  `block_type`, `source_quote`, `confidence`, and `generated_at` for each
  candidate.
- Missing candidates are **advisory only** — they are not written to
  `kb_extract_jobs` and cannot be adopted directly. The author may add them to
  the World KB through the normal `creator world kb adopt` flow after creating a
  `pending` candidate (e.g. via `creator kb rescan`).


### 6.2G.1 World KB CLI consolidation + `--auto` flag

`creator world kb adopt` gains an `--auto` mode for high-confidence, provenance-backed candidates.

| Command | Purpose |
| --- | --- |
| `nexus42 creator world kb adopt <extract_job_id> [--json]` | Manually confirm a single pending candidate. |
| `nexus42 creator world kb adopt --auto <world_ref> [--json]` | Auto-promote all eligible pending candidates for the World (see [novel-writing/quality-loop.md](../novel-writing/quality-loop.md) §5.6). |

Rules:

- `--auto` requires `--world-ref`; the clap contract enforces exactly one of positional `<extract_job_id>` or `--auto`.
- Author gate: same `require_world_owner` check as manual adopt → `403 WORLD_KB_FORBIDDEN` on mismatch.
- Eligibility is defined in [novel-writing/quality-loop.md](../novel-writing/quality-loop.md) §5.6 (`confidence >= 0.95`, provenance-backed, `ValidationMode::Novel` clean, no duplicate canonical name).
- Each promotion is a separate transaction with a CAS version guard; failures are per-candidate and do not block other candidates.
- Text output prints promoted and skipped counts; `--json` output includes `promoted_count`, `skipped_count`, `promoted[]`, and `skipped[]` with per-row `reason`.
- Audit logs are written under `Works/<work_ref>/Logs/kb/auto-promoted/<YYYY-MM-DD>-<extract_job_id>.md` when a workspace root is bound.
- The canonical World KB surface remains `creator world kb ...`; this surface carries no aliases or deprecations.

### 6.2G.2 `creator kb --scope world` alias（不存在）

无 `--scope` flag：`creator kb` 只服务 work-scope 文件索引；World-scoped narrative KB 走 canonical `creator world kb ...`（`apps/nexus42/src/commands/creator/kb.rs` module docs，§6.2F）。无 no-op 兼容拼写。

### 6.2G.3 Reading, fork, and inspector leaves

Every leaf below is a **direct-core leaf**, issuing the same typed call through
`nexus-core`
(`apps/nexus42/src/commands/creator/{reading/mod.rs,world/fork.rs,inspector.rs}`).
Contract: human-readable default output, `--json` emits the same DTO /
projection verbatim (AR-83 #3), typed long flags, and core refusals surfaced
through the shared direct-core mapper
(`apps/nexus42/src/core.rs::map_core_error`) as the same named `[code]`
envelope, same status family, non-zero exit (PL-5). The `/v1/daemon/*` route
names kept in the Purpose column are the **retained wire family** — served
by the standalone TS service for browser/Electron
(`apps/nexus-service/src/knowledge.ts`) — not the CLI leaf's transport.

| Command | Purpose |
| --- | --- |
| `nexus42 creator reading progress get <work_id> --chapter <n> [--json]` | Read persisted scroll progress (`GET /v1/daemon/reading/progress`). |
| `nexus42 creator reading progress set <work_id> --chapter <n> --scroll <0..10000> [--json]` | Upsert scroll progress (`PUT`). `--scroll` is thousandths (0–10000), CLI-validated. |
| `nexus42 creator reading progress clear <work_id> --chapter <n> [--json]` | Delete scroll progress (`DELETE`, 204). |
| `nexus42 creator reading annotation list <work_id> --chapter <n> [--json]` | List annotations (`GET /v1/daemon/reading/annotations`). |
| `nexus42 creator reading annotation add <work_id> --chapter <n> --start <o> --end <o> --selected-text <text> --color <yellow\|blue\|green\|pink> [--note <text>] [--json]` | Create an annotation (`POST`). `--end` must be strictly greater than `--start`. |
| `nexus42 creator reading annotation patch <annotation_id> [--color <c>] [--note <text>] [--json]` | Edit an annotation's color / note (`PATCH`). Empty `--note` clears the note. |
| `nexus42 creator reading annotation remove <annotation_id> [--json]` | Delete an annotation (`DELETE`, 204). |
| `nexus42 creator world fork create <world_id> --fork-point <event_id> [--label <text>] [--parent-branch <branch_id>] [--json]` | Create a timeline fork (`POST /v1/daemon/worlds/:world_id/forks`). The parent branch is derived from the fork-point event's branch via the existing timeline-events read unless `--parent-branch` is given. The derivation scans only the first page of each status (**100 canon + 100 provisional events** — the timeline read has no cursor pagination); a fork-point older than that bound requires `--parent-branch` (the error message names the remediation). 403 foreign world / 422 bad-fork-point surface named. |
| `nexus42 creator world fork list <world_id> [--branch <branch_id>] [--json]` | **Pure projection** of the existing `GET /v1/daemon/worlds/:world_id/timeline/events?event_type=fork_created&status=canon` read (+ optional `branch_id` per F-14): canon `fork_created` markers → `{branch_id, parent_branch_id, forked_from_event_id, label}` from `extensions.fork_lineage`. **No new read route.** Lineage is branch-scoped: the World's current (root) branch carries no marker — pass `--branch` (the id printed by `fork create`) to read a fork branch's marker. |
| `nexus42 creator inspector moment <world_id> [--work <work_id>] [--stage <stage>] [--json]` | **Hidden debug group** (clap `hide`, PL-6 — absent from root `--help`, documented here + `creator inspector --help`). Prints the observe-only inspector packet (`POST /v1/daemon/inspector/moment`); `--json` emits the `MomentInspectResponse` DTO verbatim. `--stage` ∈ `intake\|research\|produce\|review\|persist\|work_maintenance\|system_maintenance\|unspecified`. `moment-directive` is NOT covered (not §5 remainder). |

Rules:

- **Data CRUD only (PL-7).** Reading leaves are not a manuscript reader /
  TUI pager; the reading surface stays web.
- **`<work_id>` is the core's canonical work reference** (`wrk_...`) —
  the reading family keys on `work_id` (same convention as `creator works`).
- **Error honesty (PL-5).** CLI-side validation (chapter ≥ 1, scroll range,
  color enum, `--end > --start`, stage enum) fails fast with named messages;
  core-side errors (404 unknown work/annotation, 403 foreign world, 422
  bad fork-point) surface via the same standard `[code]` envelope with
  non-zero exit (same envelope as the retained wire family).
- **Fork lineage is branch-scoped** ("point-lookup lineage
  per branch, not list-all-forks"): a fork branch carries exactly one canon
  `fork_created` marker; the root branch carries none. `fork list` reads one
  branch (current by default, `--branch` to target a fork branch).
- **Delete leaves print empty stdout** (`progress clear`, `annotation
  remove`). The retained HTTP family answers those deletes with 204 No
  Content (the TS service keeps that contract); the CLI's direct-core delete is
  a settled verb that renders nothing — under `--json` there is likewise no
  DTO to emit (AR-83 #3).

### 6.2G.4 Strategy patch leaves

Every leaf below is a **direct-core leaf**, issuing the same typed call through
`nexus-core` (`apps/nexus42/src/commands/preset/patch.rs` —
`CoreService::patch_strategy_state`, `patch_strategy_transition`,
`patch_strategy_prompt_template`). Contract: human-readable default output,
`--json` emits the same `StrategyPatchResponse` DTO verbatim (AR-83 #3), typed
long flags, and core refusals surfaced through the shared direct-core mapper
(`apps/nexus42/src/core.rs::map_preset_error`) with the same named `[code]`
envelope and the same status table (PL-5). The `/v1/daemon/strategies/*` paths
below are the **retained wire family** served by the standalone TS service for
browser/Electron, not the CLI leaf's transport.

| Command | Purpose |
| --- | --- |
| `nexus42 preset patch state <strategy_id> <state_id> --base-revision <N> [--label <id>] [--description <text>] [--json]` | Patch a state node (`POST /v1/daemon/strategies/:strategy_id/states/:state_id/patch`). `--label` renames the state and rewrites all references (next targets, `preset.initial`, branches); `--description` updates the description. At least one of `--label`/`--description` is required. |
| `nexus42 preset patch transition <strategy_id> --base-revision <N> --source-state <id> [--op create\|update] [--old-target <id>] [--new-target <id>] [--condition <expr>] [--transition-kind next\|branch\|default] [--json]` | Rewire a transition (`POST /v1/daemon/strategies/:strategy_id/transitions/patch`). `--op create` requires `--new-target`; `--op update` (default) requires `--old-target`. `--transition-kind` selects the create form: `next` (linear scalar), `branch` (conditional rule), `default` (conditional default target). |
| `nexus42 preset patch prompt <strategy_id> <state_id> --base-revision <N> --template-ref <path> --file <path>\|'-' [--json]` | Patch a state's prompt template (`POST /v1/daemon/strategies/:strategy_id/states/:state_id/prompt/patch`). `--file` is a template body file path, or `-` to read stdin. |

Rules:

- **CAS-guarded writes (PL-5).** Every leaf takes `--base-revision` — the
  revision observed on the last canonical read (e.g. `preset show`). A stale
  revision returns **409 `strategy_conflict`**; the CLI error renders all
  four structured fields — `current_revision`, `node_id`,
  `conflicting_path`, and `recovery_hint` — and `--help` documents the
  retry guidance: re-read the Strategy and reapply with the new revision.
  Flock contention between concurrent writers rides the same 409 family.
- **Named error surface.** The core's `PresetError` vocabulary is a closed set;
  the shared direct-core mapper surfaces exactly those codes via the standard
  `[code]` envelope with non-zero exit — never a generic "request failed":
  - 409 `strategy_conflict` (stale `--base-revision`; renders
    `current_revision` + `node_id` + `conflicting_path` + `recovery_hint`),
  - 404 `not_found` (unknown strategy / state),
  - 422 `strategy_validation_failed`,
  - 422 `strategy_self_loop` / `strategy_transition_duplicate` (passthrough
    allowlist; HTTP-422-equivalent status per the same table), and
  - 400 `bad_request` for all other 400-class preset refusals (the mapper keeps
    the same coarse public code for internal classifications such as
    `strategy_invalid`, `strategy_update_forbidden`,
    `strategy_transition_missing_old_target`, `strategy_transition_not_found`).
- **Write bodies are typed long flags** (AR-83 #4); prompt bodies come from
  `--file` or stdin (`-`), matching the `creator soul` stdin convention.
- **User presets only.** The core rejects embedded/system-shipped presets
  (read-only); the CLI surfaces the resulting refusal as `bad_request`.

### 6.2G.5 Outline / chapter / timeline patch leaves

Every leaf below is a **direct-core leaf**, issuing the same typed call through
`nexus-core` (`apps/nexus42/src/commands/creator/works/outline.rs`). Contract:
human-readable default output, `--json` emits the same DTO verbatim
(AR-83 #3), typed long flags, and core refusals surfaced through the shared
direct-core mapper (named `[code]`, non-zero exit — PL-5). The
`/v1/daemon/works/*` paths below are the **retained wire family** served by the
standalone TS service for browser/Electron, not the CLI leaf's transport.

| Command | Purpose |
| --- | --- |
| `nexus42 creator works outline show <work_ref> [--json]` | Read the canonical work outline + timeline (`GET /v1/daemon/works/:work_id/outline`). The printed `outline_revision` is the `--base-revision` for the patch leaves. |
| `nexus42 creator works outline patch <work_ref> --base-revision <N> --op <move_chapter\|link_event\|attach_to_volume> [--chapter <n>] [--volume <n>] [--event <id>] [--target-chapter <n>] [--json]` | Patch the outline structure (`POST /v1/daemon/works/:work_id/outline/patch`). `move_chapter`/`attach_to_volume` require `--chapter` + `--volume`; `link_event` requires `--event` + `--target-chapter`. |
| `nexus42 creator works chapter patch <work_ref> --n <n> --base-revision <N> [--title <text>] [--slug <slug>] [--planned-word-count <n>] [--actual-word-count <n>] [--volume <n>] [--status <not_started\|outlined\|draft\|finalized\|published>] [--content <text>\|--content-file <path>] [--json]` | Patch a chapter's **outline-node** metadata (`POST /v1/daemon/works/:work_id/chapters/:n/patch`). At least one set field is required. `--content`/`--content-file` write chapter outline prose to the chapter's `outline_path` markdown file under the same `outline_revision` CAS (never `body_path`). |
| `nexus42 creator works timeline patch <work_ref> --base-revision <N> --op <add_event\|remove_event\|attach_event_to_chapter\|link_foreshadow\|unlink_foreshadow> [--event <id>] [--title <text>] [--description <text>] [--realizes-chapter <n>] [--target-chapter <n>] [--foreshadows-event <id>] [--json]` | Patch the work timeline (`POST /v1/daemon/works/:work_id/timeline/patch`). `add_event` requires `--title`; `remove_event` requires `--event`; `attach_event_to_chapter` requires `--event` + `--target-chapter`; `link_foreshadow`/`unlink_foreshadow` require `--event` + `--foreshadows-event`. |

Rules:

- **Route-family guard (AR-84).** `chapter patch` targets the outline
  **node** route `POST /v1/daemon/works/:work_id/chapters/:n/patch` — NOT
  the chapter-**content** `PATCH /v1/daemon/works/:work_id/chapters/:n`
  (different DTO family, not §5 remainder). Leaf help names the distinction.
- **CAS-guarded writes (PL-5).** Every leaf takes `--base-revision` — the
  `outline_revision` observed on the last canonical read (`outline show`). A
  stale revision returns **409 `outline_conflict`**; the CLI error renders
  all four structured fields — `current_revision`, `node_id`,
  `conflicting_path`, and `recovery_hint` — and `--help` documents the
  retry guidance: re-read the outline and reapply with the new revision.
- **Named error surface.** The core's outline/Work error vocabulary is
  surfaced by the shared direct-core mapper via the standard `[code]`
  envelope with non-zero exit — never a generic "request failed":
  - 409 `outline_conflict` (stale `--base-revision`; renders
    `current_revision` + `node_id` + `conflicting_path` + `recovery_hint`),
  - 404 `not_found` (unknown work / chapter / event),
  - 422 `outline_validation_failed` (domain rules — slug format/uniqueness,
    volume existence, foreshadow temporal order, published-chapter guard;
    the CLI renders each `validation_summary.errors` entry), and
  - 400 `bad_request` for all other 400-class refusals (the mapper keeps the
    same coarse public code for internal classifications such as
    `work_id_mismatch`, `chapter_id_mismatch`, `invalid_chapter_number`,
    `missing_event_title`, `missing_event_id`, `self_foreshadow_forbidden`).
- **Write bodies are typed long flags** (AR-83 #4); chapter outline prose
  comes from `--content` or `--content-file <path>`.
- **No TUI / no second editor.** Leaves are single-shot commands
  (stdin/file/flags only); the web canvas stays the human authoring surface.

### 6.2G.6 KB entity patch + memory closure + findings triage

Every leaf below is a **direct-core leaf**, issuing the same typed call through
`nexus-core`
(`apps/nexus42/src/commands/creator/{world/kb*,memory.rs,works/rules_runtime.rs}`).
Contract: human-readable default output, `--json` emits the
same DTO verbatim (AR-83 #3), typed long flags, and core refusals surfaced
through the shared direct-core mapper (named `[code]`, non-zero exit — PL-5).
The `/v1/daemon/*` paths below are the **retained wire family** served by the
standalone TS service for browser/Electron, not the CLI leaf's transport.

| Command | Purpose |
| --- | --- |
| `nexus42 creator world kb entity patch --world-id <id> --entity-id <id> --expected-version <N> [--title <text>] [--body <json>] [--aliases a,b] [--block-type <t>] [--modules <json>] [--json]` | Patch a World KB entity through the daemon OCC route (`POST /v1/daemon/worlds/:world_id/kb/patch-entity`; entity id + `expected_version` ride in the **body** — there is no `entities/:id/patch` path route, F-12). Per-row OCC on `kb_key_blocks.revision`. At least one patch field is required. |
| `nexus42 creator world kb graph --world-id <id> [--include-suggested] [--json]` | Show the World KB entity graph (`GET /v1/daemon/worlds/:world_id/kb/graph`). The printed per-entity `version` is the `--expected-version` for `entity patch`. The daemon caps the projection at 500 entities / 1000 stored relationships with **no wire `truncated` flag yet**; the human output notes the cap when the relationship count hits it, and `--json` stays DTO-verbatim. |
| `nexus42 creator memory pending count [--json]` | Count pending review entries (`GET /v1/daemon/memory/pending-review/count`). |
| `nexus42 creator memory review [--json]` | Drain the pending-review queue: loops `POST /v1/daemon/memory/review` while `has_more == true` (cap 100 calls, zero-progress guard), reporting cumulative `promoted/fragmented/dropped` — matches the web `useReviewMemory` drain contract (F-16). |
| `nexus42 creator memory fragments [--json]` | List memory fragments (`GET /v1/daemon/memory/fragments`). |
| `nexus42 creator memory pending-list [--json]` | List pending review entries (`GET /v1/daemon/memory/pending-review`). |
| `nexus42 creator memory pending-show <id> [--json]` | Show a pending review entry (reads from the list). |
| `nexus42 creator memory pending-dismiss <id> [--json]` | Dismiss a pending review entry (`DELETE /v1/daemon/memory/pending-review/:id`). |
| `nexus42 creator works findings list <work_ref> [--status <s>] [--severity <s>] [--json]` | List findings for a Work (`GET /v1/daemon/works/:work_id/findings`). `--status` accepts a single status or a comma-separated list (e.g. `open,triaged`). |
| `nexus42 creator works findings set-status <finding_id> --work <work_ref> --status <s> [--target-executor <exec>] [--json]` | Set a finding's status through the work-findings PATCH (`PATCH /v1/daemon/works/:work_id/findings/:finding_id`, body `{status?, target_executor?}`). One generic verb over one route (AR-87 #3 — no `triage|resolve` sugar). |
| `nexus42 creator world findings list --world-id <id> [--json]` | List world-attached check findings (`GET /v1/daemon/worlds/:world_id/findings`). **GET-only by design** — there is NO world-findings write route; triage writes ride the work-findings PATCH above. |

Rules:

- **Dual-write guard (AR-85 #3).** `kb entity patch` writes through the
  **core** OCC path (per-row version CAS on `kb_key_blocks.revision`; the
  retained wire family exposes the same operation as
  `POST /v1/daemon/worlds/:world_id/kb/patch-entity`), while
  `creator world kb edit` writes the **local DB** directly (SQLite, no OCC,
  different code path). Two write paths exist by product decision; the OCC
  guard belongs to `entity patch`, not to `edit`. `kb entity patch` is a distinct
  verb — never an overload of `edit` (no `edit --daemon`).
- **OCC error surface (PL-5).** A stale `--expected-version` returns **409
  `world_kb_conflict`** echoing the stale version as `current_version` +
  `entity_id` (rendered by the CLI error envelope); `--help` documents the
  retry guidance: refetch the graph (`creator world kb graph`) and reapply
  with the new version. 422 `world_kb_validation_failed` (domain rules) and
  400 `bad_request` (other 400s) surface named, non-zero exit.
- **Memory closure (AR-86).** `review` drains while `has_more == true`
  (server batch ≤ 50 rows/call; CLI cap 100 calls + zero-progress guard —
  matches the web drain contract). `--json` on all core-backed memory
  verbs (`review`, `fragments`, `pending-list`, `pending-show`,
  `pending-dismiss`, `pending count`). Local-file verbs
  (`list/create/show/edit/delete`) are untouched. No leaf for
  `POST /memory/pending-review` (ingestion) or `POST /memory/soul/reflect`
  (SOUL surface) — not §5 remainder.
- **Findings transition table (AR-87).** `set-status --status` accepts
  exactly the closed lifecycle vocabulary: `open`, `triaged`, `in_review`,
  `resolved`, `wont_fix`, `duplicate`. Legal transitions:
  `open → triaged | in_review | resolved | wont_fix | duplicate`;
  `triaged → in_review | resolved | wont_fix | duplicate`;
  `in_review → resolved | wont_fix | duplicate`;
  `resolved` / `wont_fix` / `duplicate` are terminal. `from == to` is
  rejected. An illegal transition returns **422 `invalid_transition`**
  naming `from → to`; `--help` documents the table verbatim. Existing
  `findings accept` + `findings prune` leaves are untouched (different
  journeys: rule-suggestion adoption; retention).
- **World findings are read-only (AR-87 #1).** `creator world findings`
  is a GET-only read; any world-findings write route is a P1 non-goal.

### World structured-rule lifecycle

The author commands are direct-core leaves; `/v1/daemon/*` is not their
transport. `creator world rule add` retains its existing typed flags and
create vocabulary `draft | active | deprecated` (default `active`).

| Command | Contract |
| --- | --- |
| `nexus42 creator world rule list --world-id <id> [--include-archived] [--json]` | Default omits archived only; draft and deprecated remain visible. Explicit inclusion reveals retained archived rows. Human STATUS prints the stored value; JSON remains the existing rule-item array. |
| `nexus42 creator world rule deactivate --world-id <id> --rule-id <id>` | Writes `deprecated`; not an alias for archive. Deprecated remains visible and PATCH-reversible. |
| `nexus42 creator world rule archive --world-id <id> --rule-id <id>` | Writes only `status=archived` through the same core update authority. Repetition succeeds. No DELETE, purge or restore command. |

**Archived is terminal.** It is not a create
state: `add --status archived` fails with the existing `status` field error.
An archived row accepts only an archive-only repeat; other PATCH members
or any status exit fail without mutation. Revival means creating a new
rule, never reusing its id. Data, ownership and World/rule non-disclosure
guards remain intact. Only active rules evaluate.

List omission is selected at the core boundary and pushed below the SQL
501-row probe, never implemented as a CLI filter. The retained 500-item
cap and `truncated` describe the selected set. HTTP/native inclusion and
schema-description requirements are owned by
[Daemon API Surface Conventions §13](../runtime/daemon-api-surface-conventions.md#13-world-structured-rule-lifecycle).
This section specifies the accepted target behavior; it does not claim
implementation or verification has shipped.

### 6.2G.7 `graph` + `entity patch` direct-core transport

`nexus42 creator world kb graph` and `nexus42 creator world kb entity patch`
call `nexus-core` directly (`CoreService::open` with
`CoreAccess::DirectWriter`, `user_home` from the existing config resolution) —
the CLI does not proxy
`GET /v1/daemon/worlds/:world_id/kb/graph` or
`POST /v1/daemon/worlds/:world_id/kb/patch-entity`. Those wire routes
remain in place for the TS/Electron surface and other HTTP/native consumers. The
per-row OCC semantics (`expected_version` against `kb_key_blocks.revision`)
are unchanged, and the core path is OCC-guarded exactly as the wire path is.
`creator world kb edit` remains the distinct local-SQLite, non-OCC path and is
not merged or removed (the §6.2G.6 dual-write rule stands).

**Exit codes** (unchanged vocabulary): version conflict **76**, writer
busy/contended **75**, config or schema mismatch **78**, other **1**.
`--json` prints the generated DTO verbatim; human output is preserved.

Every retained leaf calls the core/cloud/Connect authority directly inside the
ordinary `cli` cohort (`default = ["cli"]`; `connect-host` is independent and
keeps its explicit scoped compute/WASM edge; core `connect-client` /
`embedded-mcp` library features remain), modeled on `nexus42 ops inspect`
(§6.3B).

### 6.2I `nexus42 creator character` identity lifecycle

Normative: [actor-product-model.md](../architecture/actor-product-model.md) §11 (developer maintenance contract).

**These are direct-core leaves.** Each leaf authors through the shared
direct-core seam (`apps/nexus42/src/commands/creator/character.rs`), projecting
the stored `KnowledgeEntryRecord`/identity row onto the generated DTO family
(`UpdateCharacterRequest`, `CharacterLifecycleRequest`, `CharacterDetail`).
Contract: all mutations require an explicit
`--expected-revision`; the CLI never performs a hidden GET-and-retry overwrite.
`--json` emits the same DTO verbatim; human output prints the returned revision
and status. Owner-wide `list`/`show` include archived Characters; run and
identity writes on archived rows return **409 `character_inactive`**.

| Command | Purpose |
| --- | --- |
| `nexus42 creator character edit <character_id> --expected-revision <n> [--display-name <text>] [--image-uri <uri> \| --clear-image-uri] [--persona <json-object> \| --clear-persona] [--json]` | Patch Character identity metadata (`PATCH /v1/daemon/characters/:character_id`). Only flags present on the CLI are sent in the JSON body; `--clear-image-uri` / `--clear-persona` map to JSON `null` clears. |
| `nexus42 creator character archive <character_id> --expected-revision <n> [--json]` | Archive (freeze) a Character (`POST /v1/daemon/characters/:character_id/archive`). Same-state CAS is a no-op and keeps current sessions. |
| `nexus42 creator character restore <character_id> --expected-revision <n> [--json]` | Restore an archived Character to active with the same `character_id` (`POST /v1/daemon/characters/:character_id/restore`). Requires at least one owned active World binding; name collision returns **409 `duplicate_character_display_name`**. |

Rules:

- **Revision CAS (PL-5).** Stale `--expected-revision` returns **409
  `character_revision_conflict`** with the current revision echoed; the CLI
  surfaces the named code and exits non-zero without retrying.
- **Lifecycle busy gate.** Archive/restore while another Character activity is
  in flight returns **409 `character_busy`**.
- **No CLI-side lifecycle logic.** The CLI maps flags to generated request
  bodies and prints responses; fences, epoch retirement, and Host cleanup run
  only in the core authority, never in the CLI.

### 6.2I.1 `nexus42 creator character binding` WorldSheet maintenance

Normative: [actor-product-model.md](../architecture/actor-product-model.md) §11.4.

**These are direct-core leaves.** The leaves issue the same typed calls through
`nexus-core` (`apps/nexus42/src/commands/creator/character.rs`) over the
generated `CharacterBindingDetail` and `UpdateCharacterBindingRequest` DTOs.
Contract: only `world_sheet_entry_id` is mutable via `binding edit`;
`--clear-world-sheet` maps to JSON `null`. The CLI never performs a hidden
GET-and-retry overwrite. `binding show` and successful edits support `--json`
for verbatim DTO output.

| Command | Purpose |
| --- | --- |
| `nexus42 creator character binding show --character-id <id> --binding-id <id> [--json]` | Fetch one binding (`GET /v1/daemon/characters/:character_id/bindings/:binding_id`). Retained reads survive Character archive. |
| `nexus42 creator character binding edit --character-id <id> --binding-id <id> --expected-revision <n> (--world-sheet-entry-id <kb_id> \| --clear-world-sheet) [--json]` | Link, relink, or clear the optional WorldSheet (`PATCH` on the same path). Requires exactly one of `--world-sheet-entry-id` or `--clear-world-sheet`. |

Rules:

- **Binding revision CAS.** Stale `--expected-revision` returns **409
  `binding_revision_conflict`**; invalid WorldSheet targets return **409
  `invalid_world_sheet`** without leaking target facts.
- **Archived Character writes.** `binding edit` on an archived Character returns
  **409 `character_inactive`**; retained `binding show` still succeeds.
- **Remove unchanged.** `binding remove` success emits empty stdout (non-JSON);
  last-binding and dependency refusals keep stable 409 codes with zero mutation.


### 6.2I.2 `nexus42 creator character knowledge` authored-content maintenance

Normative: [actor-product-model.md](../architecture/actor-product-model.md) §11.5.

**These are direct-core leaves.** The leaves issue the same typed calls through
`nexus-core` over the generated `KnowledgeEntryDetail`,
`UpdateKnowledgeEntryRequest`, and `DeleteKnowledgeEntryQuery` DTOs.
Character-scoped `show`/`edit`/`remove` pin `--character-id` and
`--entry-id`; mutations require `--expected-revision`. Summary is the existing
`body.summary` contract surface only — no second content field or owner rewrite
in the CLI.

| Command | Purpose |
| --- | --- |
| `nexus42 creator character knowledge add ... [--summary <text> \| --summary-file <path>]` | Create Character/binding-owned KE with optional summary (`POST /v1/daemon/actor-knowledge/entries`). World-owned create rejects `summary` with **422 `invalid_input`**. |
| `nexus42 creator character knowledge show --character-id <id> --entry-id <id> [--json]` | Fetch one KE detail (`GET /v1/daemon/characters/:character_id/knowledge/:entry_id`). Retained reads survive Character archive. |
| `nexus42 creator character knowledge edit --character-id <id> --entry-id <id> --expected-revision <n> [--canonical-name <text>] [--summary <text> \| --summary-file <path> \| --clear-summary] [--json]` | Patch canonical name and/or summary with revision CAS (`PATCH` on the same path). `--clear-summary` maps to JSON `null`. |
| `nexus42 creator character knowledge remove --character-id <id> --entry-id <id> --expected-revision <n> [--json]` | Delete an unreferenced KE (`DELETE` on the same path; **204**). |

Rules:

- **Knowledge revision CAS.** Stale `--expected-revision` returns **409
  `knowledge_revision_conflict`**; referenced rows return **409
  `knowledge_entry_in_use`**; wrong Character/entry scope returns **404** before
  revision/referrer disclosure.
- **Summary file bounds.** `--summary-file` reads at most **65536 UTF-8 bytes**
  without trim/truncate; invalid UTF-8 is a CLI validation error.
- **Archived Character writes.** `knowledge edit`/`remove` on an archived Character
  returns **409 `character_inactive`**; retained `knowledge show` still succeeds.
- **No CLI-side storage logic.** The CLI maps flags to generated request bodies
  and prints responses; CAS, referent inventory, and activity guards run only in
  the core authority / local-db.


### 6.2I.3 `creator character run` / `soul reflect`（无 CLI 入口）

CLI 不暴露 Character run/observation 入口：`creator character run` 与
`creator character soul reflect` 不在 clap surface。Core host capture、session
与 soul-synthesis 行为保留为 library/core 操作。保留的 Character 表面：
`create|list|show|edit|archive|restore`、`binding`、`knowledge`、`memory`、
`tom`（§6.2I–§6.2I.2）。历史 observation/exit-code 合同（session SSE 观察、
`--remember` capture、GET outcome 权威性）记录于
[actor-product-model.md §11.6](../architecture/actor-product-model.md)。

### 6.2H `nexus42 creator works` — Work management and pool

Normative: [novel-writing/multi-work-lifecycle.md](../novel-writing/multi-work-lifecycle.md), [novel-writing/work-pool.md](../novel-writing/work-pool.md).

**Tier:** Primary for multi-book operators; the `creator works` surface below is the retained Work CLI surface.

| Command | Purpose |
| --- | --- |
| `nexus42 creator works list` | List Works in active workspace |
| `nexus42 creator works status [<work_id>]` | Work status, intake, schedules, world, auto-chain, findings |
| `nexus42 creator works use <work_id>` | Set pool `active` row → CLI default `work_id` (does **not** pause other Works) |
| `nexus42 creator works completion-lock release <work_id>` | Release a Work's completion lock: DB `completion_locked_at` is cleared (SSOT), `novel_completion_status` becomes `reopened`, and the derived `.completion-lock.json` is removed |
| `nexus42 creator works pool list` | List selection pool entries (DB SSOT) |
| `nexus42 creator works pool promote <entry_id> [--set-default]` | `queued` → scaffold/bind Work; optional `--set-default` → `works use` |
| `nexus42 creator works pool archive <entry_id>` | Mark pool entry `completed` |
| `nexus42 creator works pool inspiration add --title "<text>"` | Create `{workspace}/Pool/Ideas/<slug>.md` + DB row |
| `nexus42 creator works pool inspiration list` | List inspiration items |
| `nexus42 creator works pool inspiration promote <item_id> [--set-default]` | Promote an inspiration item through `CoreService::promote_work_inspiration`: Work + pool entry created, item → `promoted`; `--set-default` additionally selects it as the pool `active` Work (the retained `works use` selection) |

**Omitted-`<work_id>` resolution (`R-V1193-P0T5-OMITTED-ID-POOL-ACTIVE`).**
Every retained arm that accepts an omitted `<work_id>` — `works status`,
`works inspire`, `works reopen`, `works reconcile-chapters`, the
`works findings` / `works rules` leaves, and `creator moment-directive`
(`set` / `show` / `clear` in Work scope, where `moment_directive.rs::resolve_work_id`
delegates here) — resolves it through one helper
(`apps/nexus42/src/commands/creator/works/mod.rs::active_work_id_core`): the
bounded `status=active, limit=1, offset=0` query over the **selection pool**
(`CoreService::list_work_pool` → the `novel_pool_entries` store that
`works use` and every pool promotion write), returning that entry's `work_id`.
It never queries the Works table's own `status` column, and no second resolver
exists. An explicit `<work_id>` always wins; with no pool `active` entry the
command refuses with remediation to `creator works use <work_id>`.

**`creator works status` extensions (novel + chain state):**

| Field | Meaning |
| --- | --- |
| `chain` | `running` \| `paused_at_<stage>_ch<N>` \| `completed` |
| `pending_resume` | Whether boot auto-resume is pending or user action needed |
| `pending_inspiration_count` | Unmerged `--note` entries awaiting next state transition |
| `findings` | Open findings summary; 96h banner when applicable |
| `completion_lock` | Whether `.completion-lock.json` is present |
| `runtime_lock_holder` | Current mutating holder, if any |

**Out of scope:** `creator work switch` / global switch mutex.


### 6.3A Preset management and validation surfaces

The retained canonical surface is the hidden top-level `preset` group
(`nexus42 preset ...`, callable but absent from root `--help` — the same
posture as `capability` / `ops`, §6.0B): `list` / `show` / `validate` /
`scaffold` / `trigger` / `patch`. `preset validate <path>` is the local
positional form; there is no `--offline` switch and no `preset run` CLI entry,
while core preset/execution libraries remain. There is no `system preset`
alias and no `daemon schedule` preset entry; the `/v1/daemon/presets*` wire
family is retained (TS service `presets.ts`), and the CLI does not proxy it.

### 6.3B `nexus42 ops` — hidden operator inspection group

`nexus42 ops inspect [SESSION_ID] [--json]` is a **hidden**
operator group (clap `hide = true`, absent from root `--help` — same posture
as `preset`, §6.0B).
Documented here plus `ops inspect --help`.

It is a **daemon-free, read-only** projection of the
`orchestration_sessions` checkpoint slice: it opens the workspace `state.db` via
`nexus_local_db::open_pool_read_only` (no migrations, no seed, no lock
upgrade) and projects the shared `nexus_orchestration::resume_rules` cascade
into a resumable verdict. Checkpoints persist **position only** — there is no
completed-stages ledger, and the output never claims one.

| Behavior | Contract |
| --- | --- |
| `nexus42 ops inspect` (no argument) | List view: at most the 200 most-recent **non-terminal** checkpoint rows plus the honest full count (`total`) |
| `nexus42 ops inspect <session_id>` | Detail view: one row (`session_id`, `creator_id`, `preset_id`, `preset_version`, `db_status`, `current_task_id`, `created_at`, `updated_at`, `run_failure`, `live_join_keys`, `resumable`, `context_readable`) |
| `[--json]` | Emits the CLI-local inspect DTO verbatim (`snake_case`); `--json` errors print `{"error": ...}` on stdout, exit 1 (stdout stays machine-clean) |
| Verdict words | `yes` / `no` / `unknown` with a `rule` (terminal_status / context_unreadable / typed_failure / not_converge_merge_class / chain_class_no_failure) and `explanation` |
| Honesty rules | `db_status` is the raw DB column, never presented as authoritative run state; corrupt `context_json` → `verdict: "unknown"` (`context_readable: false` only for corrupt bytes; valid-JSON-unexpected-shape is byte-readable with the anomaly carried in the classification) |
| Missing database | Honest empty state, exit 0 |
| Unknown session id | Error, exit 1 |

**Read-only / no-manual-resume boundary:** inspection **never** implies or
triggers resume — re-drive is not a CLI operation
([daemon-runtime.md](../../archived/runtime/2026-09-21-daemon-runtime.md) §19
records the retired daemon-boot re-drive). Rule 4's in-memory half
(the engine runner presence at boot) is carried as the separate
`runner_check` caveat, never folded into the verdict; driving a resume
requires the caller to reconstruct a runner from the persisted session
state first.

Implementation authorities: `apps/nexus42/src/commands/ops.rs`,
`crates/nexus-orchestration/src/resume_rules.rs`,
`crates/nexus-orchestration/src/storage/inspect.rs`, and
`crates/nexus-orchestration/src/storage/sqlite.rs`.

### 6.4 `nexus42 acp`（能力协议命令组）

- `nexus42 acp status|doctor|probe`
- `nexus42 acp registry list|inspect`
- `nexus42 acp agent use|list`
- *Omitted:* no `nexus42 acp skills export|verify` commands — the runtime export surface is intentionally absent (DF-50 Cancelled, pre-1.0 removal); the static committed `embedded-skills/` model covers it (see §13.2).


**Embedded skills（安装 / 升级）**：实现应将 `nexus-orchestration/embedded-skills/` 同步到 `$HOME/.nexus42/skills/`，并通过 `{$workspace_dir}/.agents/skills/` 暴露/链接，使 ACP `recommended_skills` 可被首轮会话解析。

失败语义：`recommended_skills[]` 缺失、越权或不可读时，session 初始化必须返回可操作错误，不得静默降级。

### 6.5 结构化同步（`platform sync`）

> There is no top-level `sync` group in the parser. The canonical surface is
> `nexus42 platform sync pull|push|status` (§6.6); the implementation module
> `apps/nexus42/src/commands/sync/` is reached only through that group. No
> no-op alias is retained.

默认策略：

- 以显式 `pull/push` 为主
- 自动后台同步作为可选增强

操作主体：`sync` 的 `creator_id` 与 `workspace_slug` 必须对应当前活跃上下文；HTTP 优先 `Authorization: Bearer <creator_api_key>`，User 代操时使用 `Authorization: Bearer <user_access_token>` + `X-Creator-Id`。

**架构边界（长期）**：`sync` 属于 **cloud 产品线**，由 CLI 调用 **`nexus-cloud-sync`** 完成 platform HTTP；Daemon API **不得**承载 `/v1/daemon/sync/*` 或注册代理。（见 §6.5–§6.6；crate 拆分背景：[local-cloud-crate-architecture.md](../../archived/architecture/2026-09-21-local-cloud-crate-architecture.md) §5–§6。）

### 6.6 `nexus42 platform`（平台能力命令组）

- `nexus42 platform auth login|logout|status|profiles`
- `nexus42 platform sync pull|push|status|resolve`（**canonical sync surface** — §6.5）
- `nexus42 platform context assemble` (**Deferred** — future direct platform cloud assembly)
- `nexus42 platform context assemble-moment` (local four-domain Moment assembly SSOT; frozen flags: `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit`)

> There is no `platform context assemble-local` command; `assemble-moment` is the single local context assembly command.

说明：

- `platform context assemble` is **Deferred**. Direct platform cloud assembly is not yet available; the command returns clear guidance to use `assemble-moment` instead.
- `platform context assemble-moment` is the **single local assembly SSOT**. It is a four-domain Moment assembly command that calls `assemble_moment` in-process. Narrative and World KB slices read from persistent local stores; User knowledge reads from SQLite. It is distinct from the deferred platform cloud `assemble` path.
- `publish.*` 表示内容跨平台边界动作，不与 `platform sync push` 混用。
- `manuscript.*` / `publish.*` / `research.*` 作为 ACP 或 preset contract 保留，不再作为独立顶层命令组。

### 6.7 V2 CLI / ACP / Preset 边界总结

| User intent | CLI group | ACP / preset contract |
| --- | --- | --- |
| Structured state sync | `nexus42 platform sync ...`（canonical — §6.5） | `sync.*` + bundle/delta contracts |
| Runtime orchestration control | 无 CLI 组 | No CLI orchestration-control surface exists; session control lives in the TS service's orchestration schedules control plane (`current_session_id` + supervisor signal cascade). The retained scheduling surface is `creator works cron` declaration editing |
| ACP capability negotiation | `nexus42 acp ...` | registry/probe/session capability negotiation |
| Context assembly snapshot | `nexus42 platform context assemble` (**Deferred platform cloud**); `nexus42 platform context assemble-moment` (local four-domain Moment — single SSOT) | shipped path is CLI in-process; `assemble-moment` calls local `assemble_moment` with persistent narrative / World KB stores and SQLite User knowledge. Frozen flags: `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit`. No daemon context-assemble route (KCA-002 B2); no `assemble-local` command. |
| Manuscript read/write | 无顶层独立命令组 | `manuscript.*` ACP capabilities + preset roots |
| Research / references | 无顶层独立命令组 | preset orchestration + `research.*` ACP tools |
| Content publication | `nexus42 platform publish ...`（或 preset 显式动作） | `publish.*` + confirmation policy |

---

## 7. 首次使用路径

首次使用分两条路径：**纯本地**（默认，`platform_integration = paused` 时）与 **挂载 Platform**。Creator 中心化规则见 §6.0B。

### 7.1 纯本地路径（Local-first）

1. 安装 `nexus42`
2. `nexus42 system doctor`
3. `nexus42 creator register --name "..."`（或复用已有 Creator）
4. `nexus42 creator use <creator_id_or_handle>`
5. `nexus42 creator workspace init`
6. `nexus42 acp agent use <agent>`
7. 本路径止于第 6 步：`creator bootstrap` 不是 CLI 入口，Work 创建能力保留给其 core/TS 消费方

Web UI 是 Electron 应用；不要引导用户运行 `nexus42 daemon` 或 `daemon ui`。

**不需要** `platform auth login` 或 sync。`daemon schedule` 不存在。

### 7.2 Platform 挂载路径（User-first / cloud sync）

在 §7.1 之前或之后增加 Platform 步骤（与 nexus-platform `v1-spec/architecture.md` §10.3 路径 A 对齐）：

1. `nexus42 platform auth login`
2. `nexus42 creator list` 或 `creator register`（按需）；`creator pair` 当前不可用——Pairing 依赖尚未可用的 platform API（见 §6.2B）
3. **`nexus42 platform sync pull`** 获取结构化世界基线（canonical — §6.5）

**Creator-first 变体**：先完成 §7.1 步骤 3–5，再在需要 cloud 世界时执行 `platform auth login` + `platform sync pull`（路径 B，见架构 §10.3）；`creator pair` 当前不可用——Pairing 依赖尚未可用的 platform API（见 §6.2B）。

### 7.1 UX 原则

- 面向非专业创作者，文案优先讲"本地助手"和"连接你的 AI 工具"
- 不在 first-run 流程里堆协议名词
- 错误提示优先给行动建议，而不是只给报错文本
- 如果检测到可用 ACP Registry agent，应优先引导用户选择默认 agent

> CLI copy enforces the §7.1 UX principles: `preset_gates_failed`, missing scaffold, work-completed, and open-findings errors all produce actionable one-line next steps citing `docs/novel-writing-quickstart.md` §1–§6 per `novel-writing/author-experience.md` §3 remediation table. Help-text for `creator works` uses quickstart vocabulary per §7.1.

---

## 8. 登录与 profile 模型

### 8.1 登录流程

建议采用设备授权或浏览器辅助授权：

- CLI 展示验证码与 URL
- 用户在浏览器完成授权
- CLI 轮询并获取 access token / refresh token

### 8.2 token 管理

- 优先写入系统 credential store
- 如系统不可用，可降级到本地加密文件并给出明确提醒

### 8.3 profile

profile 至少包含：

- 平台环境
- 用户身份
- 默认 Creator / World
- 默认同步策略
- 安全确认策略

建议的配置优先级：

- CLI flags
- env
- workspace config
- user global config

---

## 9. Runtime 模式

### 9.1 One-shot mode

用于：

- `auth login`
- `doctor`
- `config`
- `platform sync pull`
- `platform sync push`

特点：

- 启动即执行
- 执行后退出
- 不要求后台常驻

### 9.2 Daemon mode

> Daemon mode 不是 CLI 产品表面。长期运行 HTTP 是 Electron 下的 TS 服务；该模式的生命周期归 Electron/TS。


### 9.3 Embedded mode

可作为后续增强：

- 单进程执行 runtime + ACP + sync
- 适合 CI 或受限环境
- 不作为 v1 默认模式

---

## 10. Daemon 生命周期（无 CLI 宿主）

CLI 不持有 daemon 状态机，也不提供 `nexus42 daemon status`。常驻进程的生命周期（启动、健康、降级、重启）由 Electron 应用与其 TS 服务（`apps/nexus-service`）拥有；历史 integrated-daemon 组成的记录见 [daemon-runtime.md](../../archived/runtime/2026-09-21-daemon-runtime.md)。

---

## 11. ACP-first 能力面

这里定义的是功能契约，而不是最终线协议。

### 11.0 Registry 与连接模型

Nexus runtime 在 ACP 上应扮演 ACP client 角色，至少支持：

- 从 ACP Registry 拉取或读取 agent manifest（默认远程索引与上游仓库见 [`registry-integration.md`](../agents/registry-integration.md) §0.1）
- 根据协议版本和 capability 过滤可用 agent
- 选择默认 agent
- 通过本地 stdio 启动或连接 agent
- 完成 `initialize` 握手与 capability negotiation

这部分是 Rust-first 的直接动机之一，因为 ACP 官方已提供 Rust SDK。

冻结说明：

- CLI / 本地 runtime 不作为 ACP Agent 对外 `serve`
- 如需本地自动化控制面，应单独定义为 Nexus local API，而不是 ACP 能力面

与 **平台 Creator 独立注册** 的衔接：`nexus42 acp probe` 采集的能力与传输元数据，可供 **`POST /api/v1/creators/register`** 前置审计使用，见 §6.2A。

### 11.1 上下文能力

- `whoami`
- `workspace.info`
- `workspace.paths`

### 11.2 World 读取能力

- `world.get_snapshot`
- `world.query_state`
- `timeline.get_recent`

### 11.3 World 变更能力

- `world.propose_delta`
- `world.apply_delta`
- `timeline.append_event`
- `fork.create`

说明：

- 禁止通过普通“写入”能力静默改写既有历史
- 如果 agent 想重写过去，能力面应显式指向 `fork.create`

### 11.4 同步能力

- `sync.prepare_push`
- `sync.push`
- `sync.pull`
- `sync.status`

### 11.5 正文能力

- `manuscript.list`
- `manuscript.read_range`
- `manuscript.write`

约束：

- 只允许操作工作区白名单路径；**默认**正文树根为 **`Works/<work_ref>/Stories/`**（章节文件如 **`ch<nn>-<slug>.md`**）；**`work_ref`** 与 **`work_id`** 的映射以 **preset + 本地 DB `works` 表** 为准，不得仅靠目录名推断（见 [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md)）
- **研究型产出**默认在 **`{$workspace_dir}/.nexus42/references/<run-id>/`**（见 §6.6B）；`init` 不默认创建 `References/<creator_ref>/` 布局
- **`research.*`**（若暴露为 ACP 工具名）与 **`ReferenceSource`** 索引合同仍与 `manuscript.*` 分离，防止越权读写任意文件
- 当 `output_manuscript=false` 时，`manuscript.write` 不作为默认创作路径，但能力本身仍存在；平台托管与本地 Agent 的能力面保持一致
- 平台托管 Creator 的服务端沙箱应与本地 **同一 preset 产物布局** 同构（物理路径不同），以便同一套能力语义复用

### 11.6 发布能力

- `publish.chapter`
- `publish.story`

### 11.7 可观测性

- `trace.correlation`
- `runtime.health`

---

## 12. 本地工作区结构

### 12.0 分层原则

- **`<workspace>/`**：仅承载**用户意图可见**的创作资料（宜纳入用户自己的 Git 或同步盘）。
- **`$HOME/.nexus42/`**：承载**系统与 runtime** 数据（索引、SQLite、缓存、日志、IPC、机读配置），**不得**再放到每个 `<workspace>` 根下。
- **v1-spec 内规范真源链（本地 operational + 活跃上下文）** — 定义与变更 **只认下列文件**（冲突时按 **ADR → 本节命令面 → 下钻 spec** 顺序解释）：
  1. nexus-platform `v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md`（架构决策：目录、`workspace_slug`、`creator use` / `creator workspace` 双层指针）
  2. nexus-platform `v1-spec/adr/adr-023-pre-release-cli-breaking-refactor-v1.md`、nexus-platform `v1-spec/adr/adr-024-preset-driven-workspace-acp-skills-v1.md`（CLI 面收窄、preset 产物与 ACP skills）
  3. **本节** §0.2、§6.2B–§6.2C、§6.2C **C2**、**§12.2**（CLI 用户面与目录树）
  4. [`local-db-schema.md`](../runtime/local-db-schema.md) §0（`state.db` 路径与模块边界）
  5. nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14（`WorkspaceBinding` 与本地不变量）

### 12.1 用户工作区（`<workspace>/`）

`<workspace>` **默认不**包含固定业务子树；首次 `init` 只登记创作根与配置，**不**默认创建 `Stories/`、`References/`。用户可见目录由 **preset 产物策略** 在运行中创建。

**`novel-writing` 预设** — 小说正文布局见 [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md)：

```text
<workspace>/
  Works/
    <work_ref>/
      README.md
      Outlines/
        volume-outline.md       # 可选
        chapters/
          ch01-outline.md
        foreshadowing.md        # 空模板（F### rows）
        event-index.md          # 空模板（E### rows）
      Stories/
        ch01-<slug>.md
        ...
  .nexus42/
    references/
      <run-id>/
        report.md
        artifacts/           # 可选
```

职责（示例 preset；具体以加载的 preset 为准）：

- **`Works/<work_ref>/Stories/`**
  - 小说章节**正文**主存（sync 扫描根）；**`work_ref`** 与 **`work_id`** 以 **本地 DB `works` 表 + preset** 为准，**不得**仅靠目录名推断。
- **章节状态真源**在本地 DB `state.db` 的 **`work_chapters` 表**（见 [novel-writing/workflow-profile.md §4.1](../novel-writing/workflow-profile.md)）；无 `work-status.md` 文件。
- **`Works/<work_ref>/Outlines/`**、**`README.md`**
  - 规划与人类概要元数据；**不得**被 sync 模块当作章节正文。
- **世界设定内容**跨作品（Work）共享，归 **World KB**（见 [entity-scope-model.md §5.4](../architecture/entity-scope-model.md)）；World 内容通过 `work.world_id` 绑定，无 `Works/<work_ref>/Worldbuilding/` 子树。
- 工作区根 **`Stories/<StoryRef>/`** 不存在 — 无兼容 shim。
- **`.nexus42/references/<run-id>/`**
  - **研究 / 采风型**机读产出默认位置；`report.md` + 可选 `artifacts/`；与 **`ReferenceSource`** 索引合同衔接。
- **`{$workspace_dir}/.agents/skills/`**
  - 项目可读技能根（可符号链接到 **`$HOME/.nexus42/skills/`**），供 **ACP `recommended_skills`** 与会话首轮加载解析。

术语注意：规格与 ACP 能力名仍使用 **`manuscript`**（如 `manuscript.read_range`）；其实现路径须对齐到 **preset 声明的正文根**（上表为 **`novel-writing` 默认**）。

**非 novel 预设（essay、game-bible）** — Work profile 对应的 init preset 与布局规范（profile 选择保留给 core/TS Work 创建方；`creator bootstrap --profile` 无 CLI 入口）：

| Work profile | init preset | 布局规范 |
| --- | --- | --- |
| `novel` (default) | `novel-project-init` (显式传入) | [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md) |
| `essay` | `essay-init` (自动派生) | [essay-profile.md](../creator/essay-profile.md) |
| `game_bible` | `game-bible-init` (自动派生) | [game-bible-profile.md](../creator/game-bible-profile.md) |

game-bible 布局（`works_profile: game_bible`）：`Works/<work_ref>/Design/`（12 个 section 模板），`Logs/{design,review}/`，无 `Stories/`、`Outlines/` 或 `work_chapters`。详见 [game-bible-profile.md](../creator/game-bible-profile.md) §3。game-bible Work 自动选择 `--init-preset game-bible-init`；不自动链式推进后续预设。

### 12.1C `novel-writing` preset sync module contract

`novel-writing` preset 可以声明 `sync` 子模块，用于把 preset 产物映射到既有 Nexus 同步 / 发布合同。该子模块**不**新增第二套 DTO 或 wire type；合同来源仍是本 v1-spec 与生成的 `@42ch/nexus-contracts` 类型。

**Accepted inputs**：

- Preset metadata：`preset_id=novel-writing`、版本、`workspace_slug`、`world_id`、`creator_id`、`work_id`、`work_ref`、`work_profile: novel`（来自本地 DB `works` 表，而非仅目录名）。
- 正文产物：`Works/<work_ref>/Stories/<chapter-id>.md` 及 manifest / phase metadata（映射到 existing bundle fields such as `manuscript_phase`, source anchors, canonical hashes）。详见 [novel-writing/sync-contract.md](../novel-writing/sync-contract.md)。
- 研究产物：`.nexus42/references/<run-id>/report.md` 与可选 `artifacts/`，只把合同允许的摘要、引用锚点、`ReferenceSource` / `MemoryItem` 摘录纳入结构化 sync。
- Session hints：ACP `recommended_skills[]` 解析结果与 agent session metadata（仅作为审计 / 可复现上下文，不作为全文上传许可）。

**Outputs**：

- Default `platform sync push` 输出既有 **Bundle / Delta**：world/key-block/timeline/reference/manuscript metadata、source anchors、idempotency key、canonical hash、audit command metadata。
- Local packaging 可生成 preset manifest / staging records，但这些是本地实现细节；跨平台 wire 仍引用 `cli-spec` §14、`shared/domain/data-model-v1.md` 与 schema codegen 生成合同。
- `platform sync pull` 只回填平台结构化状态与冲突 / cursor；不得把平台内容静默覆盖到 `Works/<work_ref>/Stories/` 正文。

**Publish boundary**：

- **Default `nexus42 platform sync push` is not content publication.** 它只同步结构化 delta、摘要与引用锚点（§5.3、§14.2）。
- 完整章节 / 故事正文跨越平台边界，只能由 **`publish.*` ACP capability** 或 preset `sync.publish_*` 等等价**显式发布动作**触发，并遵循 §16 confirmation / `--yes` 规则。
- `output_manuscript=false` 时，preset sync module 仍可同步结构化状态，但不得默认读取或上传正文文件。

**Contract-source references**：

- Bundle / Delta / idempotency / conflict semantics: §14.1–§14.4 and generated `@42ch/nexus-contracts` wire types.
- Workspace binding and local state: §6.2C, §12.2, nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14.
- Publish APIs / ACP publish capabilities: §11.6, §16, nexus-platform `v1-spec/platform/platform-api-v1.md` publish routes, and generated contracts (`PublishStoryRequest`, `PublishChapterRequest`, etc.).

### 12.1B Creator SOUL 与长期记忆

**真源**：nexus-platform `v1-spec/platform/creator-memory-soul-lifecycle-v1.md`。

- **`SOUL.md`**（单文件 Markdown）：推荐路径 **`$HOME/.nexus42/creators/<creator_id>/SOUL.md`**，**必须**包含二级标题 **`## Personality`**（人格轨，人改、为锚）与 **`## Experience`**（经验轨，由长期记忆聚合生成）。  
- **规范性警告**：在 **`## Experience`** 标题下至下一个同级 `##` 或 EOF 的范围内，**用户手改会在下一次经验聚合时被覆盖**；持久内容应写入 **`## Personality`** 或 **`creators/<creator_id>/memory/long-term/*.md`**（路径与 frontmatter 见该规格 §3–§5）。  
- **CLI / 长期运行服务（Electron/TS 持有）**：负责 Session 收尾写入 **待回顾队列**、**定时回顾**、**经验段聚合**、以及 **发起新 ACP Session 前的 Context 终局合并**（与 `context-assembly` 平台响应组合）。  
- **与 §6.6**：`nexus42 platform context assemble` 是 **Deferred** 的未来平台云上下文入口；当前已发货路径是 `assemble-moment`（local four-domain，single SSOT）。这些命令**不**替代 SOUL + 本地长期记忆的合并职责。

### 12.1A 服务端沙箱（平台托管 Creator）

- 平台托管 Creator 若开启 `output_manuscript=true`，其正文工作区应与 **同一 preset** 下的本地用户工作区 **同构**（默认示例与 §12.1 一致：`Works/<work_ref>/Stories/…` + 按需的 `.nexus42/references/…`）。

```text
<sandbox_root>/
  Works/
    <work_ref>/
      Stories/
        <chapter-id>.md
        ...
  .nexus42/
    references/
      <run-id>/
        report.md
        artifacts/           # 可选
```

- 差异仅在**物理位置**：本地路径位于用户设备；平台路径位于服务端沙箱。
- 关闭 `output_manuscript` 时，可不创建 `Works/<work_ref>/Stories/` 正文文件，但仍要允许 `StoryManifest.summary_text`、World KB、Timeline 正常生成。

### 12.2 系统目录（`$HOME/.nexus42/`）

推荐目录：

```text
$HOME/.nexus42/
  config.toml              # 用户级 CLI / runtime 默认；含「当前活跃 creator_id」及 **按 creator 记忆的最后活跃 workspace_slug**（见 cli-spec §6.2B–§6.2C）
  skills/                  # 内置技能镜像（自 nexus-orchestration embedded-skills 安装/升级同步；可被 workspace `.agents/skills/` 引用）
  run/                     # pid、socket、IPC 辅助
  logs/
  cache/                   # Registry 缓存、远端快照等
  shared/
    global_state.db        # 可选：跨 workspace 弱鉴权/公共结构化缓存（表与威胁模型由实现 ADR 约束）
  creators/
    <creator_id>/
      config.toml          # Creator 级密钥引用与默认（可选）
      workspaces/
        <workspace_slug>/  # 用户可读、每 Creator 唯一；默认目录名 **default**
          meta.json        # 不可变：local_root、creator_id、workspace_slug、可选 wire workspace_id、created_at 等
          config.toml      # 与该 workspace 绑定的本地配置（可选）
          state.db         # 结构化 working copy、outbox、ReferenceSource 索引等（与 local-db-schema 一致）
```

职责：

- **`creators/<creator_id>/workspaces/<workspace_slug>/state.db`**
  - 本地结构化状态与 outbox；**不**位于 `<workspace>/` 创作根；**不**使用扁平 `$HOME/.nexus42/state.db` 作为多 workspace 长期形态（迁移工具可将旧库迁入此路径）。
- **`shared/global_state.db`（可选）**
  - 仅承载明确允许的跨 workspace 缓存；**不得**替代 per-workspace `state.db` 作为权威 working copy。
- **`cache/`**
  - 含 ACP Registry 缓存等（可与 creator/workspace 维度分子目录，仍根在 `$HOME/.nexus42/`）。
- **`run/` / `logs/`**
  - 全局或按 workspace 分子路径均可，但**根路径固定**在 `$HOME/.nexus42/`。

### 12.3 默认忽略策略

- **`$HOME/.nexus42/`**：通常不在用户项目仓库内；无需在 `<workspace>` 的 `.gitignore` 中忽略（除非用户把 home 目录当仓库）。
- **`<workspace>`**：**`.nexus42/`**（工作区下机读缓存与研究产出）通常宜加入 VCS ignore；`Stories/`、`.agents/skills/` 是否提交由用户与 preset 策略决定；若含密钥或大型二进制采风，可用常规 `.gitignore` 规则处理，与 Nexus operational 数据无关。

---

## 13. SQLite 职责

SQLite 是本地 working state，不是平台 graph 的替代品。

### 13.1 应存内容

- Key Block 的本地投影
- Timeline working copy
- staged deltas
- sync cursor
- conflict markers
- outbox
- agent session metadata

### 13.2 不应存内容

- 全量正文主存
- 平台图数据库的完整替代结构
- 长期密钥明文

### 13.3 迁移原则

- CLI 自带 migration
- forward-only
- 升级前建议自动备份

---

## 14. 结构化同步模型

### 14.1 基本单位

同步基本单位是 Delta bundle，建议包含：

- delta type
- entity references
- manuscript phase
- version info
- idempotency key
- canonical hash
- created_at

### 14.2 默认模式

- `platform sync pull` 拉取平台结构化状态
- `platform sync push` 推送本地结构化变更
- **完整正文**只有在 **`publish.*`（ACP）** 或 **preset `sync` 子模块** 定义的显式发布路径下才跨越平台边界；**不与** `platform sync push` 的默认结构化 delta 混为一谈

### 14.3 冲突处理

v1 建议先采用显式冲突暴露：

- `nexus42 platform sync status` 报告冲突（§6.5）
- 不在后台静默覆盖
- Timeline 冲突优先转向 Fork
- `partial` bundle 必须显示 `delta_results[]`，并仅重建剩余变更

### 14.4 离线行为

- 写入本地 outbox
- 网络恢复后重试
- 保持命令可追溯

---

## 15. 失败与恢复模型

### 15.1 失败类型

- 平台不可达
- token 失效
- schema mismatch
- SQLite 损坏
- agent 断连
- 本地目录权限异常

### 15.2 恢复命令

- `nexus42 system doctor`
- `nexus42 platform sync status`（canonical — §6.5）
- `nexus42 system debug dump-workspace`

### 15.3 保证

v1 至少应保证：

- 本地持久化优先于网络发送
- push 是至少一次交付，但有 idempotency key
- pull 尽量按批事务化

---

## 16. 安全与确认策略

### 16.1 默认安全策略

以下操作需要明确确认或 `--yes`：

- 解绑 world
- reset 本地状态
- **`publish.*`（ACP）或 preset 定义的等价发布动作** 所触发的平台内容变更
- 创建 fork
- 覆盖本地生成文件

### 16.2 不允许的默认行为

- 静默上传全文
- 静默重写 canon history
- 允许 agent 越权读写任意路径

### 16.3 面向普通用户的解释

用户层文案应该强调：

- Nexus 会尽量把正文留在本地
- 平台同步的是世界推进所需的结构化信息
- 如果要公开发布内容，会明确提示你

---

## 17. 待决策项

进入实现前仍需补齐：

- ACP 最终线协议与本地认证方式
- ACP Registry manifest 拉取与缓存策略
- Nexus local API：不由 CLI 暴露；`/v1/daemon/*` 由独立的 TypeScript 服务为 browser/Electron 提供，CLI 保持直连 core/cloud/Connect。
- workspace 多 world 共存：支持；以 `world_id` 显式参数隔离并发；单运行时 **一个活跃 `creator_id`（`creator use`）** + 在该 Creator 下 **一个活跃 `workspace_slug`（`creator workspace use`，默认 `default`）**（见 §6.2C C2、nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14、nexus-platform `v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md`）。
- `sync` 是否允许默认后台定时拉取

