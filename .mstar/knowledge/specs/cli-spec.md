# Nexus CLI Spec

## 0. 文档定位

本稿是 nexus-platform `v1-spec/architecture.md` 的下钻文档，聚焦 Nexus 本地 CLI / runtime 的产品行为、运行模型与集成边界。

当前冻结实现前提：

- 平台仍采用 TypeScript / Next.js / Vercel AI SDK 路线
- 本地 runtime 调整为 `Rust-first`
- 原因是 ACP 已成为 CLI 主协议，而 ACP 官方当前支持 Kotlin、Java、Python、Rust、TypeScript SDK，不支持 Go SDK

本稿覆盖：

- CLI 的目标与非目标
- 命令体系
- daemon / local runtime 生命周期
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
- **CLI 可执行名**：**`nexus42`**（与 **42ch / Creative Hub** 品牌同源；下文命令示例一律使用 `nexus42`）。本地 daemon 采用 **single-binary runtime mode**（由 `nexus42 daemon start` 进入内部 daemon 进程模式，例如 `daemon-run`），不再要求独立对外产品二进制名。
- **`v1-notes/ideas/` 与 `v1-notes/` 的扩展需求**：视为路线图输入。CLI 必须保证：**协议与 schema 的可扩展字段**、Local API / ACP 能力面的**演进位**、以及已写入合同的能力（如 **`research.*` 等 ACP 能力名**、context assembly、`manuscript_phase` 等）的**最小可用实现或安全默认（no-op）**，避免把后续实现空间钉死。

### 0.2 V2 重定位（pre-release）

本节记录 V2（pre-release）重定位方向。与旧叙述冲突时，以本稿 §6.0A–§6.0B 为准。

- **定位**：Nexus CLI 是 **ACP-first 控制面** + **Creator 本地知识面**，不是执行逻辑聚合层。
- **执行边界**：推理、工具调用、文件输出等执行能力统一经 ACP capability invocation。
- **运行边界**：daemon runtime 负责编排运行态，CLI 负责控制、声明与可观测。
- **知识边界**：`SOUL` / `memory` 归属 Creator；CLI `creator kb --scope work` 仅表示活跃 Creator + workspace 下的**本地工作资料索引**；World/narrative KB 归属 `nexus-kb` + `nexus-narrative`，User/global knowledge 归属 `nexus-knowledge`。

---

## 1. 冻结定位

CLI 在 Nexus 中的冻结定位如下：

1. **CLI 是 platform connector + local runtime + agent bridge。**
2. **CLI 不是通用 LLM client，不应把特定模型 SDK 作为主路径写死。**
3. **CLI 的主集成协议是 ACP，skills 是兼容层。**
4. **完整小说正文默认本地私有；CLI 默认同步的是结构化变更，而不是全文。**
5. **CLI 必须可后台长期运行，并具备平台登录能力。**
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
- 一个持续运行的 Nexus helper

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

**Big-bang command-surface lock**：顶层 CLI 一次性收敛为六组：`daemon` / `acp` / `creator` / `sync` / `platform` / `system`。除这六组外，不再新增平行顶层命令面。

### 6.0A 与 Platform Creator 三层标识兼容约束（V2 权威约束）

对齐 nexus-platform `v1-spec/platform/creator-agent-registration-v1.md` §「三层标识体系」，CLI 在 V2 命令重组中 **必须** 保持以下不变量：

1. **内部主键始终是 `creator_id`**（`creator.id`）；CLI 不得把 `display_name` 当身份主键。
2. `creator use` 可接受 `creator_id` 或 `handle` 输入，但落盘活跃主体时必须规范化为 `creator_id`。
3. `creator status` / `creator list` 需稳定显示三层标识：`creator_id`（权威）、`handle`（可路由别名）、`display_name`（UI 展示）。
4. 任何 `creator soul|memory|kb` 子命令，均绑定当前活跃 `creator_id`，不得以 `display_name` 隐式路由。
5. `sync` / `context` / `publish` 等 Creator-context 请求，仍遵守 nexus-platform `v1-spec/platform/auth-session-model-v1.md` §4 的身份头约束（Creator API key 路径 vs User+`X-Creator-Id` 路径）。

### 6.0B V2 Big-bang 命令信息架构（权威）

V2 命令面按以下顶层执行（pre-release 允许破坏性调整）：

- `nexus42 daemon`：daemon runtime 生命周期与编排运行控制（`schedule` 为已发货控制面；`orchestrate` 子命令仍为 Deferred/stubbed 兼容入口）
- `nexus42 acp`：独立 ACP 能力面（探测、协商、调用、诊断）
- `nexus42 creator`：身份 + 本地知识资产（含 `profile|workspace|soul|memory|kb`）
- `nexus42 sync`：本地与平台的数据同步与冲突处理
- `nexus42 platform`：平台增值能力入口（跨 scope 检索/增强能力）
- `nexus42 system`：本机配置与诊断

设计约束：

- `daemon` 与 `acp` 分离：前者负责运行控制，后者负责能力执行协议面。
- `creator` 统一承载 Creator 本地知识资产：`soul` / `memory` / `kb --scope work` 作为 Creator 子命令，不再分散为平级心智入口。
- `creator kb` 采用显式 scope 语义：本地默认 `work`；未来 `world` 必须路由到 World-scoped narrative KB（`nexus-kb` + `nexus-narrative`）；User/global knowledge 不属于 `creator kb`，应走 `nexus-knowledge` 对应的 CLI 入口。

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
- **独立注册**：对齐 **`POST /api/v1/creators/register`**；注册前可用 **`nexus42 acp probe`** 采集能力与传输元数据（[`registry-integration.md`](./registry-integration.md) §2.1）。

### 6.2B `nexus42 creator` 身份子命令（权威）

| 子命令 | 作用 |
| --- | --- |
| `nexus42 creator register` | 调用 **`POST /api/v1/creators/register`**；提交描述、能力声明、agent 材料；落盘 **Creator** 凭证（与 User token 分存储） |
| `nexus42 creator status` | 展示当前激活 **`creator_id`**、Pairing、**`creator_api_key`** 是否已配置 / 有效、可选 **`api_key_ref`** 摘要（不明文打印密钥） |
| `nexus42 creator use <creator_id_or_handle>` | 将活跃主体设为给定 Creator（内部规范化落盘为 `creator_id`） |
| `nexus42 creator list` | 在 User 已登录时拉取可见 Creator 列表 |
| `nexus42 creator pair` | 建立当前 Creator 与当前 User 的 Pairing |
| `nexus42 creator unpair` | 撤销当前 Creator 与 User 的 Pairing |
| `nexus42 creator logout` | 清除本地 Creator 凭证（不默认清除 User 会话） |
| `nexus42 creator credentials rotate` | 轮换 `creator_api_key` 或重绑 `api_key_ref` |

实现约束：

- **默认操作主体**：未显式指定时，daemon / sync 使用的 **`creator_id`** 必须与 `creator use` 当前活跃主体一致；本地 `state.db` 解析自 **当前活跃 Creator + 当前活跃 workspace_slug**。
- **凭证隔离**：User refresh/access 与 `creator_api_key` 分桶存储。

### 6.2C `nexus42 creator workspace`（本地 workspace 子命令）

`workspace_slug` 在同一 `creator_id` 下唯一，并映射到 `"$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/"`。默认 slug 为 `default`。

| 子命令 | 作用 |
| --- | --- |
| `nexus42 creator workspace list` | 列出当前活跃 Creator 下的 `workspace_slug` |
| `nexus42 creator workspace create <workspace_slug>` | 新建 workspace 登记与 operational 树 |
| `nexus42 creator workspace use <workspace_slug>` | 切换活跃 workspace |
| `nexus42 creator workspace init` | 在当前 `creator + workspace` 上下文登记创作根与 operational 元数据 |
| `nexus42 creator workspace clone <world-ref>` | **Deprecated** — world cloning is platform-only; not available locally. Hidden from `--help` |
| `nexus42 creator workspace link` | 绑定本地项目与平台 World |
| `nexus42 creator workspace unlink` | 解绑本地项目与平台 World |
| `nexus42 creator workspace status` | 当前 workspace 总览 |

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
- `nexus42 creator kb ...`：知识资产索引（默认 `--scope work`；`--scope world` 路由至 World-scoped narrative KB）
- `nexus42 creator world ...`：World 浏览与 narrative 状态查询（read-only; platform fork 不在本地范围 — PD-01）
- `nexus42 creator knowledge ...`：User knowledge / reference 管理入口（`nexus-knowledge`）
- `nexus42 creator demo-seed ...`：演示数据填充（world + KB seed）

**V1.29 additions** (compass: [v1.29](../../iterations/v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md)):

- `nexus42 creator memory pending-list` — list items in `memory_pending_review` awaiting review
- `nexus42 creator memory pending-show <id>` — show detail of a single pending memory item
- `nexus42 creator memory pending-dismiss <id>` — dismiss a pending memory item (no promotion)
- `nexus42 creator soul refresh-experience` — deterministic one-shot SOUL `## Experience` aggregation via embedded preset; updates `SOUL.md` Experience section
- `nexus42 creator kb queue-extract <work-entry-id> --world-id <id>` — enqueue a work entry for KB extraction into a World (idempotent)
- `nexus42 creator kb extract-status [--job-id]` — check extraction job status (all jobs or specific)

`creator kb` scope 约束（对齐 [`entity-scope-model.md`](./entity-scope-model.md) §5.3）：

- **`--scope work`（默认，V1.23 必须保留；V1.24 KCA-003 C2 强化为唯一已实现 scope）**：表示活跃 `creator_id` + 活跃 `workspace_slug` 下的 **CLI local work KB index**。当前实现通过 daemon local API `/v1/local/kb/entries` 优先处理，失败时回退到 `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/...` 下的本地文件 / `index.json` 工作索引。它是工作资料/文件索引，**不是** `nexus-kb` 的 World graph，**也不是** `nexus-knowledge` 的 User/global knowledge index。V1.24 的 daemon handler (`handlers/kb.rs`) 和 CLI (`creator kb`) 均已明确标注为 work-scope only。
- **`--scope world`（V1.27+ shipped）**：要求可解析的 `world_id`（显式 flag 或当前 workspace binding），并路由到 `nexus-narrative` + `nexus-kb`。该路径查询的是 World-scoped narrative KB assets（KeyBlocks、SourceAnchors、graph/query primitives），不得回退到 `--scope work` 文件索引。
- **User/global knowledge（未来目标）**：不得塞进 `creator kb` 或 `creator kb --scope user`。User-scoped global knowledge/reference material 应通过 `nexus-knowledge` 的 CLI 入口暴露；在六组顶层命令锁定下，推荐入口为 `nexus42 platform knowledge ...`（或等价的 platform/user knowledge 子命令），并由 `nexus-knowledge` 处理存储、标签检索与供 Moment assembly 读取的切片。

命名与行为建议：

- **V1.23 最小落地**：保持 `nexus42 creator kb` 作为现有命令组；所有无 `--scope` 调用按 `--scope work` 解释，并在 help/文案中写明“local work index”。
- **推荐别名 / 迁移方向**：由于 `creator kb` 与 crate `nexus-kb` 的语义碰撞风险为高，建议在 V1.23 或下一 pre-release 引入更直观的别名，例如 `nexus42 creator assets ...` 或 `nexus42 creator work-index ...`，作为 `creator kb --scope work` 的首选用户文案；`creator kb --scope work` 可暂留为兼容别名，避免打断现有脚本。
- **不推荐硬改为泛化 KB**：不要把 `creator kb` 解释成“所有知识入口”。World KB、User knowledge、Creator memory 三者的 owning crate 与 entity scope 不同，CLI 只能做路由，不能在 `nexus42` 内实现第二套领域模型。

### 6.2E KB / knowledge 术语禁用简写

在本规格及后续架构 / 实现文档中，`KB` 一词必须按 [`entity-scope-model.md`](./entity-scope-model.md) §5.4 限定语义后使用：

- **World KB** / **narrative KB**：指 `nexus-kb` 所有的 World-scoped narrative KB graph（KeyBlocks、SourceAnchors、graph insertion/query），由 `nexus-narrative` 协调 World/Timeline/Event 语境。
- **User knowledge** / **global knowledge index**：指 `nexus-knowledge` 所有的 User-scoped global knowledge/reference material。
- **CLI local work KB index** / **local work index**：指 `nexus42 creator kb --scope work` 当前的活跃 Creator + workspace 本地文件索引。

禁止在存在歧义的上下文中单独写“KB”来同时指代以上三者；CLI help、错误提示、spec、ADR、计划任务均应使用限定词。

### 6.2F V1.23 KB / knowledge target CLI model

V1.23 结束时，KB / knowledge 相关 CLI 路由目标应固定为：

| User intent | CLI command model | Required scope inputs | Owning crates / modules | Behavior |
| --- | --- | --- | --- | --- |
| Manage local work files / notes as workspace assets | `nexus42 creator kb ...` (default `--scope work`); preferred alias candidate `nexus42 creator assets ...` | active `creator_id`, active `workspace_slug` | `nexus42` command router + daemon local API / local workspace storage; later storage may move behind local-domain crates | List/search/show/add/remove local work index entries only. Must not create World KeyBlocks or User knowledge rows. |
| Manage narrative knowledge inside a World | `nexus42 creator kb ... --scope world --world-id <world_id>` or workspace-bound equivalent | active `creator_id`, `workspace_slug`, explicit/resolved `world_id` | `nexus-narrative` + `nexus-kb` | Route to World-scoped narrative KB graph. Must preserve KeyBlock / SourceAnchor provenance and narrative ownership. No silent fallback to work index. |
| Manage User/global reference knowledge | `nexus42 creator knowledge ...` | authenticated User / Pairing context; optional Creator only as acting context, not owner | `nexus-knowledge` | Store/search/list user-scoped global knowledge/reference material. May be pulled into Moment assembly; promotion into World KB is an explicit cross-scope operation. |
| Browse World narrative state | `nexus42 creator world ...` (read-only) | active `creator_id`, workspace_slug, explicit/resolved `world_id` | `nexus-narrative` | Query World state, timelines, manuscripts. Read-only — no local fork or write mutations (PD-01: fork is platform-only). |
| Seed demo data | `nexus42 creator demo-seed ...` | active `creator_id`, workspace_slug | `nexus-creator` + `nexus-narrative` + `nexus-kb` | Populate demo world + KB entries for testing. |
| Assemble direct platform cloud context | `nexus42 platform context assemble` | `--world-id`; optional workspace/creator and include/limit flags | Future direct platform context assembly path | **Deferred (V1.26).** Platform cloud assembly is not yet available; CLI exits with clear guidance to use `assemble-moment`. It must not call the retired daemon context-assemble Local API. |
| **Assemble local four-domain Moment snapshot (single SSOT)** | `nexus42 platform context assemble-moment` | optional `--world-id`, `--user-id`, `--branch-id`, `--event-id`; **frozen flags:** `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit` | `assemble_moment` in `nexus-moment-context-assembly` reading Stage-0 context plus local narrative, World KB, and User knowledge slices | **Shipped (local, V1.26+).** Single assembly SSOT — replaces the retired `assemble-local` path. Runs in-process and calls `assemble_moment`; narrative and World KB are read through persistent local stores, while User knowledge reads from SQLite (V1.27+). No platform cloud assembly and no daemon context-assemble route. |

Implementation task C4 should therefore treat `creator kb` as a routing/name-alignment task, not as permission for `nexus42` to own KB/domain storage long-term.

### 6.3 `nexus42 daemon`（运行态控制命令组）

- `nexus42 daemon start|stop|restart|status|logs|doctor`
- `nexus42 daemon schedule add|edit|remove|list|inspect|context|context-history|start|pause|resume|cancel|advance|timeline`

说明：

- daemon runtime 是本地 supervisor，不是 ACP Agent/Server。
- `daemon` 负责运行态控制，不承载 ACP 协议协商职责。
- **Shipped:** `daemon schedule ...` is wired to the daemon orchestration schedules Local API (`/v1/local/orchestration/schedules/*`) via `commands/daemon/schedule.rs`.
- **Session control ownership:** `daemon schedule ...` is the primary orchestration CLI surface. It exercises the full sessions control plane through schedule operations: `current_session_id` points at the active orchestration session, and schedule signals cascade through the supervisor to the active session as described in [`creator-schedule-and-core-context.md`](./creator-schedule-and-core-context.md) §3.3.
- **Removed:** `daemon orchestrate ...` is not a shipped compatibility surface. Do not document `daemon orchestrate run` in new plans or runbooks; use `daemon schedule ...` for shipped orchestration control unless a future plan intentionally introduces a new session-control wrapper.

### 6.2D `nexus42 creator run` (Work experience — V1.33 target)

High-level **user-facing** entry for narrative Work lifecycle. Hides `daemon schedule` details for the default product path. Normative model: [work-experience-model.md](./work-experience-model.md).

| Command | Purpose |
| --- | --- |
| `nexus42 creator run start --idea "<text>"` | Create a **Work** (`work_id`), run Creative Brief Intake, then primary preset (default `novel-writing`) |
| `nexus42 creator run continue <work_id> [--note "<text>"]` | Append inspiration/direction; optionally resume/attach `work_continue` preset |
| `nexus42 creator run list` | List Works in active workspace |
| `nexus42 creator run status <work_id>` | Work status, intake, linked schedules, world binding |

Rules:

- Only presets declaring `run_intents` including `work_init` may be used as the **first** run on a new Work (see [orchestration-engine.md](./orchestration-engine.md) §7.7).
- `work_continue` presets require completed intake unless `--force` (audited).
- `creator run` creates/updates schedules via daemon Local API; it does **not** replace `daemon schedule` for power users.

**Shipped baseline (pre-V1.33):** Work commands are **not** yet in the CLI tree — use `daemon schedule add --preset novel-writing --seed "..."` as interim. V1.33 plan `2026-06-04-v1.33-work-model-and-creator-run` owns implementation.

### 6.3A Preset management and validation surfaces

**System / maintenance** (not the default user creative entry):

| Command | Purpose |
| --- | --- |
| `nexus42 system preset list` | List embedded + user + system presets with `run_intents` (V1.33 expands beyond `_system.*` only) |
| `nexus42 system preset validate <path>` | Validate preset bundle via shared orchestration facade (V1.33) |

**Power-user orchestration** (unchanged):

- `nexus42 daemon schedule add --preset <id> --creator <id> [--seed "..."]` — starts preset-driven workflows through schedules.

**Local API** (shipped):

- `GET /v1/local/presets`
- `POST /v1/local/presets`
- `POST /v1/local/presets:validate`
- `POST /v1/local/presets/{id}:reload`

There is **no** top-level `nexus42 preset ...` command group. User creative entry is **`creator run`** (V1.33); validation/listing is **`system preset`**.

### 6.4 `nexus42 acp`（能力协议命令组）

- `nexus42 acp status|doctor|probe`
- `nexus42 acp registry list|inspect`
- `nexus42 acp agent use|list`
- `nexus42 acp skills export|verify`

**Embedded skills（安装 / 升级）**：实现应将 `nexus-orchestration/embedded-skills/` 同步到 `$HOME/.nexus42/skills/`，并通过 `{$workspace_dir}/.agents/skills/` 暴露/链接，使 ACP `recommended_skills` 可被首轮会话解析。

失败语义：`recommended_skills[]` 缺失、越权或不可读时，session 初始化必须返回可操作错误，不得静默降级。

### 6.5 `nexus42 sync`（结构化同步命令组）

- `nexus42 sync pull|push|status|retry|resolve`

默认策略：

- 以显式 `pull/push` 为主
- 自动后台同步作为可选增强

操作主体：`sync` 的 `creator_id` 与 `workspace_slug` 必须对应当前活跃上下文；HTTP 优先 `Authorization: Bearer <creator_api_key>`，User 代操时使用 `Authorization: Bearer <user_access_token>` + `X-Creator-Id`。

**架构边界（长期）**：`sync` 属于 **cloud 产品线**，由 CLI 调用 **`nexus-cloud-sync`** 完成 platform HTTP；daemon Local API **不得**承载 `/v1/local/sync/*` 或注册代理。见 [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §5–§6。

### 6.6 `nexus42 platform`（平台能力命令组）

- `nexus42 platform auth login|logout|status|profiles`
- `nexus42 platform context assemble` (**Deferred** — future direct platform cloud assembly)
- `nexus42 platform context assemble-moment` (**Shipped (local)** — single four-domain Moment assembly SSOT; frozen flags: `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit`)

> **Breaking change (pre-release):** `nexus42 platform context assemble-local` is **removed**. Use `assemble-moment` as the single local context assembly command. `assemble-local` (Stage-0/TwoStage only) was superseded by the full four-domain `assemble_moment` path.

说明：

- `platform context assemble` is **Deferred** in V1.26. Direct platform cloud assembly is not yet available; the command returns clear guidance to use `assemble-moment` instead. It must not call the retired daemon context-assemble Local API.
- `platform context assemble-moment` is the **single local assembly SSOT** (shipped V1.26, hardened V1.27+). It is a four-domain Moment assembly command that calls `assemble_moment` in-process. Narrative and World KB slices read from persistent local stores; User knowledge reads from SQLite (V1.27+). It is distinct from the deferred platform cloud `assemble` path.
- `publish.*` 表示内容跨平台边界动作，不与 `sync push` 混用。
- `manuscript.*` / `publish.*` / `research.*` 作为 ACP 或 preset contract 保留，不再作为独立顶层命令组。

### 6.7 V2 CLI / ACP / Preset 边界总结

| User intent | CLI group | ACP / preset contract |
| --- | --- | --- |
| Structured state sync | `nexus42 sync ...` | `sync.*` + bundle/delta contracts |
| Runtime orchestration control | `nexus42 daemon schedule ...` (**Shipped**) | schedule commands call daemon orchestration schedules Local API and own session control via `current_session_id` + supervisor signal cascade |
| ACP capability negotiation | `nexus42 acp ...` | registry/probe/session capability negotiation |
| Context assembly snapshot | `nexus42 platform context assemble` (**Deferred platform cloud**); `nexus42 platform context assemble-moment` (**Shipped local four-domain Moment — single SSOT**) | shipped path is CLI in-process; `assemble-moment` calls local `assemble_moment` with persistent narrative / World KB stores and SQLite User knowledge. Frozen flags: `--max-tokens`, `--no-fragments`, `--hint`, `--kb-limit`, `--kb-search`, `--kb-type`, `--knowledge-limit`. Daemon context-assemble Local API is **Retired** (KCA-002 B2). `assemble-local` is **removed** in pre-release. |
| Manuscript read/write | 无顶层独立命令组 | `manuscript.*` ACP capabilities + preset roots |
| Research / references | 无顶层独立命令组 | preset orchestration + `research.*` ACP tools |
| Content publication | `nexus42 platform publish ...`（或 preset 显式动作） | `publish.*` + confirmation policy |

---

## 7. 首次使用路径

推荐的首次使用流程（**User-first**，与 nexus-platform `v1-spec/architecture.md` §10.3 路径 A 一致）：

1. 安装 `nexus42`
2. 执行 `nexus42 system doctor`
3. 执行 `nexus42 platform auth login`（若走 User-first）
4. 准备 **Creator** 凭证：由 Web 创建后 **`nexus42 creator list`**，或 **`nexus42 creator register`**（可选先 `acp probe`）
5. 执行 **`nexus42 creator use <creator_id_or_handle>`**，将 CLI 设为该 Creator（本地自动确保 **`default`** workspace 存在，见 §6.2C）
6. （可选）执行 **`nexus42 creator workspace create <workspace_slug>`** 增加第二套创作根；否则继续使用 **`default`**
7. （若有多 slug）执行 **`nexus42 creator workspace use <workspace_slug>`** 选择活跃 workspace
8. 执行 **`nexus42 creator workspace init`**（在默认或当前目录登记创作根与 operational 树；**不**默认创建 `Stories/` / `References/`；按需由 preset 创建技能根或 `.nexus42/references/` 等，见 §6.2C、§13）
9. 执行 `nexus42 daemon start`
10. 让 Nexus runtime 发现并连接你选定的本地 ACP agent
11. 执行 `nexus42 sync pull` 获取结构化世界基线

**Creator-first**：可先 **`nexus42 creator register`**（可选先 `nexus42 acp probe`），再 **`nexus42 creator use`**（落到 **`default`** workspace），再 **`nexus42 creator workspace init`**；需要私有世界或完整持久化时再 **`nexus42 platform auth login`** + **`nexus42 creator pair`**（路径 B，见架构 §10.3）。

### 7.1 UX 原则

- 面向非专业创作者，文案优先讲“本地助手”和“连接你的 AI 工具”
- 不在 first-run 流程里堆协议名词
- 错误提示优先给行动建议，而不是只给报错文本
- 如果检测到可用 ACP Registry agent，应优先引导用户选择默认 agent

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
- daemon 使用 token 时以内存持有为主，不在日志中泄露

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
- `sync pull`
- `sync push`

特点：

- 启动即执行
- 执行后退出
- 不要求后台常驻

### 9.2 Daemon mode

用于：

- 持有稳定的本地运行时上下文
- 持有本地 IPC 入口
- 管理后台同步调度
- 承载本地事件总线、agent session 与 profile

建议 v1 形态：

- 一个 workspace 对应一个 daemon
- daemon 通过 Unix socket / named pipe / loopback port 暴露本地接口
- daemon 自身不作为 ACP Agent 对外暴露；它负责作为 ACP Client 管理外部 agent 会话
- agent 会话托管采用 **managed-only Hybrid host**：可接入 ACP provider 与 native CLI provider，但统一走 host 规范化能力契约

### 9.3 Embedded mode

可作为后续增强：

- 单进程执行 runtime + ACP + sync
- 适合 CI 或受限环境
- 不作为 v1 默认模式

---

## 10. Daemon 生命周期

### 10.1 状态机

- `Stopped`
- `Starting`
- `Running`
- `Degraded`
- `Stopping`
- `Failed`

### 10.2 启动流程

daemon 启动时至少需要完成：

1. 加载 workspace config
2. 验证当前 profile
3. 打开 SQLite
4. 建立本地 IPC 入口
5. 读取 ACP Registry 或本地 agent 配置
6. 建立与选定 agent 的能力协商
7. 建立 Nexus 内部本地 API / IPC 面
8. 输出健康状态

### 10.3 降级状态

daemon 可以允许部分能力降级，例如：

- 已连接 agent 正常，但平台网络异常
- 本地 agent 未连接，但同步仍可用

`nexus42 daemon status` 应清楚展示：

- PID
- 运行时长
- 当前 profile
- 监听地址
- 最近错误

---

## 11. ACP-first 能力面

这里定义的是功能契约，而不是最终线协议。

### 11.0 Registry 与连接模型

Nexus runtime 在 ACP 上应扮演 ACP client 角色，至少支持：

- 从 ACP Registry 拉取或读取 agent manifest（默认远程索引与上游仓库见 [`references-learnings.md`](../../references-learnings.md) §0.1；集成合同见 [`registry-integration.md`](./registry-integration.md) §0.1）
- 根据协议版本和 capability 过滤可用 agent
- 选择默认 agent
- 通过本地 stdio 启动或连接 agent
- 完成 `initialize` 握手与 capability negotiation

这部分是 Rust-first 的直接动机之一，因为 ACP 官方已提供 Rust SDK。

冻结说明：

- CLI / daemon 不作为 ACP Agent 对外 `serve`
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

- 只允许操作工作区白名单路径；**默认**正文树根为 **`Stories/<StoryRef>/`**（章节文件如 **`<chapter-id>.md`**）；**`StoryRef`** 与 **`world_id`** 的映射以 **preset + 本地 DB / workspace 配置** 为准，不得仅靠目录名推断
- **研究型产出**默认在 **`{$workspace_dir}/.nexus42/references/<run-id>/`**（见 §6.6B）；历史布局 **`References/<creator_ref>/`** 仅作为兼容叙述，**不再**由 `init` 默认创建
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

## 12. Skills-second 兼容层

### 12.1 目标

为不支持 ACP 的 agent 生态输出稳定的 Nexus 能力封装。

### 12.2 原则

- skills 是映射层，不是主协议
- skill 能力名应尽量与 ACP 能力面一一对应
- skill 导出应带上能力版本
- skills 的职责是兼容非 ACP agent 生态，而不是替代 ACP Registry + ACP 握手模型

### 12.3 导出内容

- manifest
- tool / skill definitions
- 版本号
- 使用说明
- 兼容性声明

建议命令：

- `nexus42 acp skills export --format <target>`
- `nexus42 acp skills verify`

---

## 13. 本地工作区结构

### 13.0 分层原则

- **`<workspace>/`**：仅承载**用户意图可见**的创作资料（宜纳入用户自己的 Git 或同步盘）。
- **`$HOME/.nexus42/`**：承载**系统与 runtime** 数据（索引、SQLite、缓存、日志、IPC、机读配置），**不得**再放到每个 `<workspace>` 根下。
- **v1-spec 内规范真源链（本地 operational + 活跃上下文）** — 定义与变更 **只认下列文件**（冲突时按 **ADR → 本节命令面 → 下钻 spec** 顺序解释）：
  1. nexus-platform `v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md`（架构决策：目录、`workspace_slug`、`creator use` / `creator workspace` 双层指针）
  2. nexus-platform `v1-spec/adr/adr-023-pre-release-cli-breaking-refactor-v1.md`、nexus-platform `v1-spec/adr/adr-024-preset-driven-workspace-acp-skills-v1.md`（CLI 面收窄、preset 产物与 ACP skills）
  3. **本节** §0.2、§6.2B–§6.2C、§6.2C **C2**、**§13.2**（CLI 用户面与目录树）
  4. [`local-db-schema.md`](./local-db-schema.md) §0（`state.db` 路径与模块边界）
  5. nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14（`WorkspaceBinding` 与本地不变量）

### 13.1 用户工作区（`<workspace>/`）

`<workspace>` **默认不**包含固定业务子树；首次 `init` 只登记创作根与配置，**不**默认创建 `Stories/`、`References/`。用户可见目录由 **preset 产物策略** 在运行中创建。

**`novel-writing` 预设（示例）** — 默认小说正文布局：

```text
<workspace>/
  Stories/
    <StoryRef>/
      <chapter-id>.md
      ...
  .nexus42/
    references/
      <run-id>/
        report.md
        artifacts/           # 可选
```

职责（示例 preset；具体以加载的 preset 为准）：

- **`Stories/<StoryRef>/`**
  - 小说/章节等**正文**主存；**`StoryRef`** 与 **`world_id`** 的绑定以 **本地 DB / workspace 配置 + preset** 为准，**不得**仅靠目录名推断。
- **`.nexus42/references/<run-id>/`**
  - **研究 / 采风型**机读产出默认位置；`report.md` + 可选 `artifacts/`；与 **`ReferenceSource`** 索引合同衔接。
- **`{$workspace_dir}/.agents/skills/`**
  - 项目可读技能根（可符号链接到 **`$HOME/.nexus42/skills/`**），供 **ACP `recommended_skills`** 与会话首轮加载解析。

历史兼容叙述：规格与 ACP 能力名仍可能出现 **`manuscript`**（如 `manuscript.read_range`）；其实现路径须对齐到 **preset 声明的正文根**（上表为 **`novel-writing`** 默认）。

### 13.1C `novel-writing` preset sync module contract

`novel-writing` preset 可以声明 `sync` 子模块，用于把 preset 产物映射到既有 Nexus 同步 / 发布合同。该子模块**不**新增第二套 DTO 或 wire type；合同来源仍是本 v1-spec 与生成的 `@42ch/nexus-contracts` 类型。

**Accepted inputs**：

- Preset metadata：`preset_id=novel-writing`、版本、`workspace_slug`、`world_id`、`creator_id`、`StoryRef` ↔ `world_id` 绑定（来自本地 DB / workspace config，而非仅目录名）。
- 正文产物：`Stories/<StoryRef>/<chapter-id>.md` 及 manifest / phase metadata（映射到 existing bundle fields such as `manuscript_phase`, source anchors, canonical hashes）。
- 研究产物：`.nexus42/references/<run-id>/report.md` 与可选 `artifacts/`，只把合同允许的摘要、引用锚点、`ReferenceSource` / `MemoryItem` 摘录纳入结构化 sync。
- Session hints：ACP `recommended_skills[]` 解析结果与 agent session metadata（仅作为审计 / 可复现上下文，不作为全文上传许可）。

**Outputs**：

- Default `sync push` 输出既有 **Bundle / Delta**：world/key-block/timeline/reference/manuscript metadata、source anchors、idempotency key、canonical hash、audit command metadata。
- Local packaging 可生成 preset manifest / staging records，但这些是本地实现细节；跨平台 wire 仍引用 `cli-spec` §15、`shared/domain/data-model-v1.md` 与 schema codegen 生成合同。
- `sync pull` 只回填平台结构化状态与冲突 / cursor；不得把平台内容静默覆盖到 `Stories/<StoryRef>/` 正文。

**Publish boundary**：

- **Default `nexus42 sync push` is not content publication.** 它只同步结构化 delta、摘要与引用锚点（§5.3、§15.2）。
- 完整章节 / 故事正文跨越平台边界，只能由 **`publish.*` ACP capability** 或 preset `sync.publish_*` 等等价**显式发布动作**触发，并遵循 §17 confirmation / `--yes` 规则。
- `output_manuscript=false` 时，preset sync module 仍可同步结构化状态，但不得默认读取或上传正文文件。

**Contract-source references**：

- Bundle / Delta / idempotency / conflict semantics: §15.1–§15.4 and generated `@42ch/nexus-contracts` wire types.
- Workspace binding and local state: §6.2C, §13.2, nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14.
- Publish APIs / ACP publish capabilities: §11.6, §17, nexus-platform `v1-spec/platform/platform-api-v1.md` publish routes, and generated contracts (`PublishStoryRequest`, `PublishChapterRequest`, etc.).

### 13.1B Creator SOUL 与长期记忆

**真源**：nexus-platform `v1-spec/platform/creator-memory-soul-lifecycle-v1.md`。

- **`SOUL.md`**（单文件 Markdown）：推荐路径 **`$HOME/.nexus42/creators/<creator_id>/SOUL.md`**，**必须**包含二级标题 **`## Personality`**（人格轨，人改、为锚）与 **`## Experience`**（经验轨，由长期记忆聚合生成）。  
- **规范性警告**：在 **`## Experience`** 标题下至下一个同级 `##` 或 EOF 的范围内，**用户手改会在下一次经验聚合时被覆盖**；持久内容应写入 **`## Personality`** 或 **`creators/<creator_id>/memory/long-term/*.md`**（路径与 frontmatter 见该规格 §3–§5）。  
- **CLI / daemon runtime**：负责 Session 收尾写入 **待回顾队列**、**定时回顾**、**经验段聚合**、以及 **发起新 ACP Session 前的 Context 终局合并**（与 `context-assembly` 平台响应组合）。  
- **与 §6.6**：`nexus42 platform context assemble` 是 **Deferred** 的未来平台云上下文入口；当前已发货路径是 `assemble-moment`（local four-domain，single SSOT）。这些命令**不**替代 SOUL + 本地长期记忆的合并职责。

### 13.1A 服务端沙箱（平台托管 Creator）

- 平台托管 Creator 若开启 `output_manuscript=true`，其正文工作区应与 **同一 preset** 下的本地用户工作区 **同构**（默认示例与 §13.1 一致：`Stories/<StoryRef>/…` + 按需的 `.nexus42/references/…`）。

```text
<sandbox_root>/
  Stories/
    <StoryRef>/
      <chapter-id>.md
      ...
  .nexus42/
    references/
      <run-id>/
        report.md
        artifacts/           # 可选
```

- 差异仅在**物理位置**：本地路径位于用户设备；平台路径位于服务端沙箱。
- 关闭 `output_manuscript` 时，可不创建 `Stories/<StoryRef>/` 正文文件，但仍要允许 `StoryManifest.summary_text`、World KB、Timeline 正常生成。

### 13.2 系统目录（`$HOME/.nexus42/`）

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
  - 本地结构化状态与 outbox；**不**位于 `<workspace>/` 创作根；**不**再使用已废弃的扁平 `$HOME/.nexus42/state.db` 作为多 workspace 长期形态（迁移工具可将旧库迁入此路径）。
- **`shared/global_state.db`（可选）**
  - 仅承载明确允许的跨 workspace 缓存；**不得**替代 per-workspace `state.db` 作为权威 working copy。
- **`cache/`**
  - 含 ACP Registry 缓存等（可与 creator/workspace 维度分子目录，仍根在 `$HOME/.nexus42/`）。
- **`run/` / `logs/`**
  - 全局或按 workspace 分子路径均可，但**根路径固定**在 `$HOME/.nexus42/`。

### 13.3 默认忽略策略

- **`$HOME/.nexus42/`**：通常不在用户项目仓库内；无需在 `<workspace>` 的 `.gitignore` 中忽略（除非用户把 home 目录当仓库）。
- **`<workspace>`**：**`.nexus42/`**（工作区下机读缓存与研究产出）通常宜加入 VCS ignore；`Stories/`、`.agents/skills/` 是否提交由用户与 preset 策略决定；若含密钥或大型二进制采风，可用常规 `.gitignore` 规则处理，与 Nexus operational 数据无关。

---

## 14. SQLite 职责

SQLite 是本地 working state，不是平台 graph 的替代品。

### 14.1 应存内容

- Key Block 的本地投影
- Timeline working copy
- staged deltas
- sync cursor
- conflict markers
- outbox
- agent session metadata

### 14.2 不应存内容

- 全量正文主存
- 平台图数据库的完整替代结构
- 长期密钥明文

### 14.3 迁移原则

- CLI 自带 migration
- forward-only
- 升级前建议自动备份

---

## 15. 结构化同步模型

### 15.1 基本单位

同步基本单位是 Delta bundle，建议包含：

- delta type
- entity references
- manuscript phase
- version info
- idempotency key
- canonical hash
- created_at

### 15.2 默认模式

- `sync pull` 拉取平台结构化状态
- `sync push` 推送本地结构化变更
- **完整正文**只有在 **`publish.*`（ACP）** 或 **preset `sync` 子模块** 定义的显式发布路径下才跨越平台边界；**不与** `sync push` 的默认结构化 delta 混为一谈

### 15.3 冲突处理

v1 建议先采用显式冲突暴露：

- `nexus42 sync status` 报告冲突
- 不在后台静默覆盖
- Timeline 冲突优先转向 Fork
- `partial` bundle 必须显示 `delta_results[]`，并仅重建剩余变更

### 15.4 离线行为

- 写入本地 outbox
- 网络恢复后重试
- 保持命令可追溯

---

## 16. 失败与恢复模型

### 16.1 失败类型

- 平台不可达
- token 失效
- schema mismatch
- SQLite 损坏
- agent 断连
- 本地目录权限异常

### 16.2 恢复命令

- `nexus42 system doctor`
- `nexus42 sync status`
- `nexus42 sync retry`
- `nexus42 daemon restart`
- `nexus42 system debug dump-workspace`

### 16.3 保证

v1 至少应保证：

- 本地持久化优先于网络发送
- push 是至少一次交付，但有 idempotency key
- pull 尽量按批事务化

---

## 17. 安全与确认策略

### 17.1 默认安全策略

以下操作需要明确确认或 `--yes`：

- 解绑 world
- reset 本地状态
- **`publish.*`（ACP）或 preset 定义的等价发布动作** 所触发的平台内容变更
- 创建 fork
- 覆盖本地生成文件

### 17.2 不允许的默认行为

- 静默上传全文
- 静默重写 canon history
- 允许 agent 越权读写任意路径

### 17.3 面向普通用户的解释

用户层文案应该强调：

- Nexus 会尽量把正文留在本地
- 平台同步的是世界推进所需的结构化信息
- 如果要公开发布内容，会明确提示你

---

## 18. 待决策项

进入实现前仍需补齐：

- ACP 最终线协议与本地认证方式
- ACP Registry manifest 拉取与缓存策略
- Nexus local API 是否需要独立暴露，以及与 ACP Client-only 拓扑的边界
- ~~workspace 是否支持多 world 共存~~。**Closed（C2）**：支持；以 `world_id` 显式参数隔离并发；单运行时 **一个活跃 `creator_id`（`creator use`）** + 在该 Creator 下 **一个活跃 `workspace_slug`（`creator workspace use`，默认 `default`）**（见 §6.2C C2、nexus-platform `v1-spec/shared/domain/data-model-v1.md` §5.14、nexus-platform `v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md`）。
- `sync` 是否允许默认后台定时拉取
- skills export 的目标格式优先级
