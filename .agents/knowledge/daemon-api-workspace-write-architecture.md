# V1.15 Daemon Local API & Workspace 写入架构设计

**Status**: Active
**Created**: 2026-05-10
**Author**: fullstack-dev (research + design task)
**Plan ref**: `.agents/plans/2026-05-10-v1.15-orchestration-first-pipeline.md`
**Compass ref**: `.agents/iterations/v1.15-delivery-compass-v1.md`

---

## 背景

V1.15 正在将 CLI 驱动的工作流（`research`、`manuscript`、`publish`）迁移到 orchestration-first preset-driven 架构。T6 已移除这三个 CLI 命令组及其对应的 daemon API 路由。

核心架构问题是：当 `nexus42d` 通过 preset-driven scheduling 驱动工作流时，daemon 的 local API 是否还需要？workspace 写入应该如何设计？

**关键发现**：经过完整审计，当前 daemon local API 的所有路由都是 **基础设施** 层面的——即使在 preset-driven 模型下，它们仍然是必需的。workspace 写入通过 ACP tool execution API 代理，由 preset 内的 ACP session 发起，这是正确的架构路径。

---

## 1. 当前 Daemon Local API 清单

### 1.1 保留的 API（基础设施）

| 路由 | 方法 | 用途 | 保留原因 |
|------|------|------|----------|
| `/v1/local/runtime/health` | GET | 健康检查 | Daemon liveness 探针；所有上层操作的前置检查 |
| `/v1/local/runtime/status` | GET | 运行时状态 | CLI `doctor`/`status` 诊断基础设施 |
| `/v1/local/daemon/status` | GET | Daemon 生命周期快照 | HSM 状态查询（starting/running/degraded/stopping/failed）；调度器依赖 |
| `/v1/local/monitoring/pool` | GET | DB 连接池状态 | 运维监控；QC-W3 遗留；诊断时有用 |
| `/v1/local/workspace` | GET | Workspace 信息 | CLI 和 daemon 都需要知道 workspace 是否已初始化 |
| `/v1/local/workspace/init` | POST | 初始化 workspace | workspace 生命周期管理；preset 运行的前提 |
| `/v1/local/creators` | GET | 列出 creators | 认证后查询；schedule 创建和 session 归属需要 creator_id |
| `/v1/local/references` | GET | 列出 reference 源 | research preset 需要 reference 发现能力 |
| `/v1/local/sync/status` | GET | 同步状态 | 离线同步基础设施；用户需要知道 outbox 状态 |
| `/v1/local/sync/push` | POST | 推送同步 | 离线-first sync 管道；preset 产出物上传到 platform 的路径 |
| `/v1/local/sync/pull` | POST | 拉取同步 | 从 platform 拉取 bundles；workspace 与 platform 对齐 |
| `/v1/local/sync/resolve` | POST | 冲突解决 | 同步冲突的手动/自动解决 |
| `/v1/local/sync/replay` | GET | 重放条目 | 查找可重放的 outbox 条目 |
| `/v1/local/world/fork` | POST | World 分叉 | 世界历史不可变；fork 是创建分支的原子操作；sync 基础设施 |
| `/v1/local/world/snapshot` | POST | World 快照 | 创建世界时间点快照；sync 基础设施 |
| `/v1/local/explore/browse` | POST | 浏览探索 | 平台内容的只读代理；灵感收集阶段需要 |
| `/v1/local/explore/search` | POST | 搜索探索 | 平台内容的只读代理；research preset 需要 |
| `/v1/local/acp/tool/execute` | POST | ACP 工具执行 | **核心 workspace 写入路径**；preset 内 ACP session 的 agent 通过此 API 读写 workspace 文件 |
| `/v1/local/acp/sessions` | GET | 列出 ACP sessions | Session 生命周期管理 |
| `/v1/local/acp/sessions/{id}` | DELETE | 删除 ACP session | Session 清理 |
| `/v1/local/memory/pending-review` | POST | 创建待审查条目 | Session 结束时的 capture 管道；agent 产出物的 review 流程 |
| `/v1/local/memory/pending-review` | GET | 列出待审查 | 查询待 review 的 session 产出 |
| `/v1/local/memory/pending-review/count` | GET | 计数待审查 | 快速计数 |
| `/v1/local/memory/pending-review/{id}` | DELETE | 删除待审查 | 清理已处理的 review |
| `/v1/local/orchestration/sessions` | GET | 列出编排 sessions | **Preset 控制面**；查看当前运行/暂停/完成的 preset session |
| `/v1/local/orchestration/sessions` | POST | 创建编排 session | **Preset 入口**；从 preset ID 创建新 session，是 preset-driven 工作流的起点 |
| `/v1/local/orchestration/sessions/{id}` | GET | 获取 session 详情 | 查询 preset 执行状态 |
| `/v1/local/orchestration/sessions/{id}/signal` | POST | 发送 session 信号 | 控制 preset 执行（pause/resume/cancel/advance） |
| `/v1/local/orchestration/capabilities` | GET | 列出 capabilities | 查询可用 capability（`workspace.open`、`acp.prompt` 等） |
| `/v1/local/orchestration/presets` | GET | 列出 presets | 查询可用的 embedded/user/system presets |
| `/v1/local/orchestration/presets/{id}:reload` | POST | 重载 preset | 热重载 user preset（开发时有用） |
| `/v1/local/orchestration/schedules` | POST | 添加 schedule | **Preset 调度入口**；创建定时/依赖 schedule |
| `/v1/local/orchestration/schedules` | GET | 列出 schedules | 查询 schedule 状态 |
| `/v1/local/orchestration/schedules/{id}` | GET | 检查 schedule | 详情查询 |
| `/v1/local/orchestration/schedules/{id}` | DELETE | 删除 schedule | 清理 terminal schedule |
| `/v1/local/orchestration/schedules/{id}/core-context` | PATCH | 编辑 core context | 向 preset 传递用户输入/指令 |
| `/v1/local/orchestration/schedules/{id}/core-context` | GET | 获取 core context | 查看当前 context |
| `/v1/local/orchestration/schedules/{id}/core-context-history` | GET | Core context 历史 | 查看 context 演变 |
| `/v1/local/orchestration/schedules/{id}/signal` | POST | 发送 schedule 信号 | start/pause/resume/cancel/advance |

### 1.2 被 Preset 取代的 API

| 路由 | 方法 | 原用途 | 替代方式 | 处理建议 |
|------|------|--------|----------|----------|
| （已移除）`research *` 相关路由 | * | CLI `nexus42 research` 工作流 | `embedded-presets/research` preset + schedule API | T6 已完成移除 |
| （已移除）`manuscript *` 相关路由 | * | CLI `nexus42 manuscript` 工作流 | `embedded-presets/novel-writing` preset + schedule API | T6 已完成移除 |
| （已移除）`publish *` 相关路由 | * | CLI `nexus42 publish` 工作流 | Sync API（push）+ platform publish | T6 已完成移除 |

**结论**：所有需要移除的 workflow-specific 路由已在 T6 中移除。当前剩余的全部路由都是 preset-driven 架构所需的基础设施。

### 1.3 需要新增的 API（Preset 支持）

当前 **不需要新增 API 路由**。原因如下：

1. **Workspace 写入路径已存在**：ACP tool execution API（`POST /v1/local/acp/tool/execute`）支持 `fs/write_text_file`，preset 内的 ACP session 通过此路径写文件。
2. **Preset 控制面已完整**：session CRUD + signal + schedule CRUD + core-context 管理 + capability/preset 查询。
3. **Sync 管道已就绪**：push/pull/resolve/replay 覆盖了 workspace-to-platform 同步需求。

**潜在未来需求**（不在 V1.15 scope）：
- 如果 preset 需要直接（非 ACP-agent 中介）写 workspace 文件，可考虑新增 `POST /v1/local/workspace/artifacts` API。但 V1.15 的设计是所有 workspace 写入都通过 ACP agent tool，不需要此扩展。
- 如果 `context_update` hook 需要触发 workspace 文件操作（当前只支持 `append`/`struct_merge`/`llm_summarize` 操作 core context），可考虑 `workspace_write` op。但这与 "agent 是唯一的 workspace writer" 原则冲突，不建议。

---

## 2. 当前 Workspace 写入点审计

| 模块 | 写入目标 | 触发方式 | 是否已 preset-driven |
|------|----------|----------|---------------------|
| `nexus42` CLI `init workspace` | `Stories/`、`References/`、`.nexus42/` 目录 | 用户执行 `nexus42 init workspace` | ❌ **硬编码目录结构**（T8.5 目标） |
| `nexus42` CLI `init workspace` | `~/.nexus42/creators/.../meta.json` | 同上 | ✅ 运营数据，非 workflow |
| `nexus42` CLI `init workspace` | `~/.nexus42/creators/.../state.db` | 同上 | ✅ SQLite 初始化 |
| `nexus42` CLI `init workspace` | `~/.nexus42/skills/` | 同上（skill sync） | ✅ O1-B skill 同步 |
| `nexus42d` workspace init handler | workspace 目录结构（via `state.init_workspace()`） | `POST /v1/local/workspace/init` | ❌ 需要对齐 T8.5 |
| `nexus42d` ACP tool execute | workspace 内任意文件（`fs/write_text_file`） | ACP agent 发起 tool request | ✅ **这是 preset-driven 写入路径** |
| `nexus42d` sync push | `state.db` outbox 表 | `POST /v1/local/sync/push` | ✅ 基础设施 |
| `nexus42d` sync pull | `state.db` outbox 表 | `POST /v1/local/sync/pull` | ✅ 基础设施 |
| `nexus42d` memory pending-review | `state.db` memory_pending_review 表 | `POST /v1/local/memory/pending-review` | ✅ session-end capture |
| `nexus42d` orchestration sessions | 内存中的 engine state | `POST /v1/local/orchestration/sessions` | ✅ preset 控制面 |
| `nexus42d` orchestration schedules | `state.db` creator_schedules/core_context_versions | `POST /v1/local/orchestration/schedules` | ✅ preset 调度 |
| `nexus42d` ACP tool audit log | `state.db` acp_tool_audit_log 表 | 每次 tool execute 后自动写入 | ✅ 审计 |

### 关键发现

**`Stories/` 和 `References/` 的创建是唯一需要移除的硬编码 workspace 写入。** 具体位置：

- `crates/nexus42/src/commands/init.rs` 第 104-109 行：
  ```rust
  let stories_dir = creative_root.join("Stories");
  let references_dir = creative_root.join("References");
  std::fs::create_dir_all(&stories_dir)?;
  std::fs::create_dir_all(&references_dir)?;
  ```
- `crates/nexus42d/src/api/handlers/workspace.rs` → `state.init_workspace()` 中的等价逻辑（如果存在）

T8.5 已覆盖此改动。移除后，这些目录由 preset policy 在 ACP session 中通过 `fs/write_text_file` 按需创建（父目录不存时 ACP handler 会 `create_dir_all`）。

---

## 3. 目标架构：Preset-Driven Workspace 写入

### 3.1 设计原则

1. **Agent 是唯一的 workspace writer**：所有用户可见的 workspace 文件写入（`Stories/`、`References/`、`report.md` 等）都通过 ACP agent 的 tool 执行完成，不通过 CLI 直接写或 daemon 直接写。
2. **Preset 定义写入策略**：何时写、写到哪里、写什么内容，全部由 preset YAML 定义（通过 prompt template 指导 agent 行为）。
3. **Daemon 是写入管道**：`nexus42d` 通过 ACP tool execution API 提供 workspace 文件读写能力，包括权限检查、路径验证和审计日志。
4. **CLI 是触发器**：`nexus42` CLI 通过 orchestration session/schedule API 触发 preset 执行，不直接写业务文件。
5. **Core context 是 preset 的输入**：用户通过 schedule core-context API 向 preset 传递指令，preset 通过 context_update hook 演化 context，agent 根据 context 决定写入行为。

### 3.2 Workspace 写入流程

以 `novel-writing` preset 的 drafting 阶段为例：

```
用户 → CLI: nexus42 schedule add --preset novel-writing
CLI → daemon: POST /v1/local/orchestration/schedules (add_schedule)
daemon → scheduler: ScheduleSupervisor.insert_pending()
用户 → CLI: nexus42 schedule signal <id> start
CLI → daemon: POST /v1/local/orchestration/schedules/{id}/signal (start)
daemon → scheduler: ScheduleSupervisor 触发 tick()
scheduler → engine: start_session_with_preset(loaded_preset)
engine → state machine: 进入 gathering → brainstorming → outlining → drafting
  drafting state:
    engine → inner_graph (drafting_graph):
      draft_intro node:
        engine → ACP host: 创建 ACP session (writer role, writer-system.md prompt)
        ACP agent → CLI (acp_worker): tool request: fs/write_text_file
        CLI → daemon: POST /v1/local/acp/tool/execute
        daemon: permission check → path validation → write file → audit log
        agent 继续执行...
      draft_body node (depends_on draft_intro):
        同上流程
    engine: graph_complete → advance to done
  done state (terminal):
    session status → completed
```

以 `research` preset 为例：

```
用户 → daemon: POST /v1/local/orchestration/schedules (research preset)
...
scanning state → extracting state → synthesizing state:
  ACP agent (researcher role) 通过 fs/write_text_file 写入:
    {$workspace_dir}/.nexus42/references/{run_id}/report.md
    {$workspace_dir}/.nexus42/references/{run_id}/artifacts/...
```

### 3.3 关键设计决策

| 决策 ID | 描述 | 选项 | 建议 | 理由 |
|---------|------|------|------|------|
| D1 | Workspace 文件写入由谁执行？ | A: CLI 直接写 B: Daemon API 直接写 C: ACP agent 通过 tool API 写 | **C** | ACP agent 是可编程的执行单元，preset 通过 prompt 控制其行为；tool API 已有路径验证和审计 |
| D2 | Preset 内 `context_update` hook 是否应支持文件写入？ | A: 是，新增 `workspace_write` op B: 否，保持 hook 只操作 core context | **B** | hook 是 engine 内部操作，不应直接 I/O；文件写入应由 ACP agent 完成，保持单一职责 |
| D3 | `inner_graph` 节点如何触发 workspace 写入？ | A: 节点直接调用 daemon API B: 节点创建 ACP session，agent 通过 tool 写入 | **B** | 当前架构已是如此：`InnerGraphNodeTask` 创建 ACP session，ACP agent 执行 tool |
| D4 | 研究产出物的路径策略由谁定义？ | A: 硬编码在 daemon B: preset prompt 模板中指导 agent | **B** | Compass 明确 "artifacts are policy-driven"；research preset YAML 的 output contract 描述已在 preset 头部注释中，prompt 模板指导 agent 写入该路径 |
| D5 | `Stories/` 和 `References/` 目录是否需要骨架创建？ | A: CLI init 创建空目录 B: preset 按需通过 agent 创建 | **B** | Compass §0 明确 "Workspace init MUST NOT create Stories/ or References/ directories"；T8.5 已覆盖 |
| D6 | 是否需要新增 workspace write API？ | A: 新增 `POST /v1/local/workspace/artifacts` B: 复用现有 ACP tool API | **B** | ACP tool API 已完整覆盖文件读写，新增 API 会引入重复路径和权限检查逻辑 |
| D7 | `nexus42d` 的 workspace init handler 是否需要调整？ | A: 保持与 CLI init 一致的骨架创建 B: 改为最小化 init（只创建 `.nexus42/`） | **B** | 与 CLI init 的 T8.5 改动对齐；业务目录由 preset 驱动 |

---

## 4. 模块职责边界

### 4.1 nexus-orchestration（Preset Engine）

**职责**：
- Preset 加载、验证、graph 构建（`preset/`、`loader.rs`）
- Orchestration engine：session 生命周期、signal 处理（`engine/`）
- Schedule supervisor：调度、依赖解析、concurrency（`schedule/`、`scheduler/`）
- Capability registry：capability 定义和查询（`capability/`）
- Worker manager：ACP worker 进程管理（`worker/`）
- Embedded skills 打包和同步（`embedded_skills/`、`skill_sync/`）
- Inner graph task 执行：创建 ACP session 代理（`tasks/`）

**不负责**：
- 直接 workspace 文件 I/O
- HTTP 路由和请求处理
- CLI 交互

**依赖方向**：
```
nexus-orchestration → nexus-contracts (类型)
nexus-orchestration → nexus-home-layout (路径)
nexus-orchestration → graph-flow (图引擎)
nexus-orchestration → nexus-local-db (schedule/core-context 持久化)
```

### 4.2 nexus42d（Daemon/Supervisor）

**职责**：
- HTTP API server（axum）：所有 `/v1/local/*` 路由
- Lifecycle HSM：daemon 启停和状态管理（`lifecycle/`）
- Workspace state：workspace 初始化、状态查询（`workspace/`）
- ACP tool execution：权限检查、路径验证、文件读写执行、审计日志（`api/handlers/acp.rs`）
- Platform proxy：sync、world、explore 路由代理到 platform API
- Orchestration HTTP handlers：session/schedule/preset/capability 的 HTTP 适配层

**不负责**：
- Preset 加载和验证（委托 `nexus-orchestration`）
- 业务逻辑决策（由 preset engine 驱动）

**依赖方向**：
```
nexus42d → nexus-orchestration (engine, preset, schedule, worker)
nexus42d → nexus-contracts (类型)
nexus42d → nexus-local-db (SQLite 操作)
nexus42d → nexus-sync (outbox, SyncClient)
nexus42d → nexus-domain (验证逻辑)
```

### 4.3 nexus42（CLI Client）

**职责**：
- 用户命令行交互（clap 命令定义）
- Workspace init（最小化骨架创建）
- Auth 管理
- 通过 HTTP 调用 daemon API 执行操作
- ACP worker 子进程入口（daemon 管理的 worker）

**不负责**：
- 直接 workspace 业务文件写入（由 ACP agent 通过 daemon 完成）
- Preset 引擎逻辑
- Schedule 业务逻辑

**依赖方向**：
```
nexus42 → nexus42d (via HTTP API)
nexus42 → nexus-orchestration (skill_sync 在 init 时调用)
nexus42 → nexus-contracts (类型)
nexus42 → nexus-home-layout (路径)
```

### 4.4 依赖关系图

```
                    ┌─────────────────────────┐
                    │     nexus42 (CLI)        │
                    │  - 命令行交互             │
                    │  - workspace init        │
                    │  - HTTP client to daemon  │
                    └────────────┬──────────────┘
                                 │ HTTP API calls
                                 ▼
                    ┌─────────────────────────┐
                    │    nexus42d (daemon)     │
                    │  - HTTP API server       │
                    │  - ACP tool execution    │◄──── ACP agent tool requests
                    │  - workspace state       │
                    │  - lifecycle HSM         │
                    │  - platform proxy        │
                    └────────────┬──────────────┘
                                 │ uses
                                 ▼
                    ┌─────────────────────────┐
                    │  nexus-orchestration     │
                    │  - preset engine         │
                    │  - schedule supervisor   │
                    │  - orchestration engine  │
                    │  - capability registry   │
                    │  - worker manager        │
                    └────────────┬──────────────┘
                                 │ depends on
                    ┌────────────┴──────────────┐
                    │                           │
                    ▼                           ▼
           ┌───────────────┐          ┌────────────────┐
           │ nexus-contracts│          │ nexus-local-db  │
           │ (wire types)   │          │ (SQLite)        │
           └───────────────┘          └────────────────┘

        Workspace 写入路径（虚线 = ACP agent）:

        Preset inner_graph node
            │
            ▼
        ACP session (worker subprocess)
            │
            ▼ (tool request)
        nexus42d: POST /v1/local/acp/tool/execute
            │
            ├─ permission check
            ├─ path validation (workspace root boundary)
            ├─ fs/write_text_file → workspace_dir/...
            └─ audit log → state.db
```

---

## 5. 对 V1.15 Plan 的影响

### 5.1 T7 范围调整

**结论：无需调整。**

T7（"Add or align preset/pipeline entrypoints and help text"）的范围是 CLI UX 层面的——确保用户可以通过 `nexus42 schedule` 和 `nexus42 preset` 命令启动 preset 工作流。这与本设计的架构分析一致：

- Preset 入口通过 orchestration session/schedule API 实现
- CLI 通过 HTTP 调用 daemon API
- Help text 替代已移除的 `research`/`manuscript`/`publish` 命令组

**建议**：T7 的 help text 应明确说明 workspace 文件由 preset 策略自动创建，无需用户手动建目录。

### 5.2 T8.5 一致性检查

**结论：设计一致，确认需要执行。**

T8.5 的目标是移除 `Stories/` 和 `References/` 骨架创建。具体修改点：

1. **`crates/nexus42/src/commands/init.rs`** 第 104-109 行：
   ```rust
   // 需要移除:
   let stories_dir = creative_root.join("Stories");
   let references_dir = creative_root.join("References");
   std::fs::create_dir_all(&stories_dir)?;
   std::fs::create_dir_all(&references_dir)?;
   ```

2. **`crates/nexus42d` workspace init handler**：如果 `state.init_workspace()` 内部也创建了这些目录，需要同步移除。

3. **测试更新**：任何断言 `Stories/` 或 `References/` 存在的测试需要调整。

**设计对齐**：移除后，`Stories/` 由 `novel-writing` preset 的 drafting_graph 中的 ACP agent 通过 `fs/write_text_file` 创建（ACP handler 会自动 `create_dir_all` 父目录）。`References/` 由 `research` preset 的 synthesizing 阶段创建。

### 5.3 T9-T11 假设验证

**T9（Stories/<StoryRef>/<chapter-id>.md policy）**：
- ✅ 假设有效。`novel-writing` preset 的 `drafting_graph` 中 `draft_intro` 和 `draft_body` 节点会创建 ACP session，ACP agent 在执行 `fs/write_text_file` 时写入 `Stories/<StoryRef>/<chapter-id>.md`。
- ✅ Workspace init 保持 skeleton-free。
- ⚠️ **需要明确**：prompt 模板（`prompts/draft-intro.md`、`prompts/draft-body.md`）需要包含明确的路径指令，告诉 agent 写入哪个文件。这是 prompt 设计问题，不是架构问题。

**T10（world/story mapping in local DB）**：
- ✅ 假设有效。`nexus-local-db` 提供 SQLite 持久化，mapping 存储在 `state.db` 中。
- ✅ 不依赖目录名推断，而是显式 DB 记录。

**T11（sync module contract）**：
- ✅ 假设有效。Sync 管道（`nexus-sync`）已完整：outbox → precheck → bundle → push。
- ✅ workspace 产出物的 sync 通过现有 `POST /v1/local/sync/push` API 完成。
- ⚠️ **需要明确**：sync module 需要知道 `Stories/<StoryRef>/` 目录下的文件结构，以便构建正确的 `LocalDelta`。这可以在 sync module 的 artifact discovery 逻辑中处理。

### 5.4 建议新增/修改的任务

| 建议编号 | 类型 | 描述 | 理由 |
|---------|------|------|------|
| S1 | **建议** | T8.5 完成后，验证 `novel-writing` preset 的 drafting 阶段能正确创建 `Stories/` 目录和文件 | 确保端到端路径可用 |
| S2 | **建议** | T6.5（research preset）的 prompt 模板需要包含明确的文件路径指令，确保 agent 写入 `{$workspace_dir}/.nexus42/references/<run-id>/report.md` | 无路径指令则 agent 不知道写到哪里 |
| S3 | **建议** | T7 的 help text 应说明 preset 自动创建 workspace 目录结构 | 减少用户困惑 |
| S4 | **注意** | `context_update` hook 在 V1.15 只支持 core context 操作（`append`/`struct_merge`/`llm_summarize`），不支持直接文件写入 | 如果未来需要 hook 触发文件操作，需要设计新机制 |

---

## 6. 未决问题 & 需要 PM 决策的事项

| 编号 | 问题 | 选项 | 影响 |
|------|------|------|------|
| Q1 | **Preset prompt 中的文件路径如何参数化？** 当前 `novel-writing` preset 的 prompt 模板需要包含 `Stories/<StoryRef>/<chapter-id>.md` 路径，但 `StoryRef` 和 `chapter-id` 从哪里来？ | A: preset YAML 定义 vars，core context 传入 B: agent 自行从 context 推断 C: 新增 preset schema 字段定义 output path template | 影响 T9 的 prompt 设计和 preset schema |
| Q2 | **Research preset 的 `run_id` 如何生成？** output contract 定义为 `.nexus42/references/<run-id>/report.md`，但 `run_id` 的生成策略未明确。 | A: 使用 session_id B: 使用时间戳 C: 使用 schedule_id | 影响 T6.5 的 prompt 和 research preset 实现 |
| Q3 | **多个 preset 实例并发写入同一个 workspace 时如何隔离？** 例如两个 novel-writing schedule 同时运行，它们的 `Stories/` 写入是否需要命名空间隔离？ | A: 使用不同的 StoryRef（由 core context 指定） B: daemon 层面加 workspace write lock C: 不隔离（依赖用户保证不冲突） | 影响并发策略和可靠性 |
| Q4 | **ACP tool execution 的 `fs/write_text_file` 是否需要限制写入路径范围？** 当前只验证路径在 workspace root 内，但不限制写入 `state.db` 或 `.nexus42/` 内部文件。 | A: 当前行为足够（workspace root 隔离） B: 新增 deny-list（如 `.nexus42/state.db`） C: 新增 allow-list（只有特定目录可写） | 影响安全策略 |
| Q5 | **Preset 执行失败时的 workspace 清理策略？** 如果 drafting 阶段写了一半文件，然后 session failed，是否清理部分写入？ | A: 不清理（保留部分产出，用户手动处理） B: 预写 temp 文件，完成后再 rename C: 记录写入清单，失败时回滚 | 影响可靠性和用户体验 |

---

*Created: 2026-05-10. Status: Active.*
