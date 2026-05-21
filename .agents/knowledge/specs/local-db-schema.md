# Nexus Local DB Schema Spec

## 0. 文档定位

本稿是 [`cli-spec.md`](./cli-spec.md) 与 nexus-platform `v1-spec/architecture.md` 的实现下钻，定义 **Nexus 本地 SQLite（`state.db`）** 的职责边界、模块拆分与演进策略。

**磁盘路径 SSOT**（`state.db` 落在 `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/` 等）：nexus-platform `v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md`。

本稿目标是解决一个具体问题：CLI 与 daemon 同时使用本地 DB 时，不能在两个可执行各自维护 schema 与版本逻辑，否则会持续出现漂移与重复实现。

本稿中的 `state.db` 指 **`$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/state.db`**（与 `cli-spec.md` §13.2、`shared/domain/data-model-v1.md` §5.14、`adr-014` 对齐）。**`<workspace_slug>`** 为用户可读的本地分区名（每 Creator 唯一，默认 **`default`**）。**不得**将 `state.db` 放在用户可见的 `<workspace>/` 创作根目录下。

可选：跨 workspace 的弱鉴权/公共结构化缓存可使用 `$HOME/.nexus42/shared/global_state.db`（职责边界由 ADR / cli-spec §13.2 约束；本稿不展开其表结构）。

---

## 1. 设计目标

1. 本地 DB 能力单点维护（single owner module）。
2. CLI / daemon 通过统一 API 复用本地 DB 功能，而不是复制逻辑。
3. 将 **DB schema version** 与 **contract schema_version** 明确解耦。
4. 保持 V1 现有产品语义不变（本地优先、结构化同步、ACP client-only）。

---

## 2. 非目标

- 不在本稿定义平台端数据库结构。
- 不替代 JSON Schema 作为 wire contract 真源。
- 不在 V1 一次性引入复杂数据库框架（保持 SQLite + 轻量 migration）。

---

## 3. 边界与术语

### 3.1 两条版本线（必须分离）

- **DB schema version**：仅用于本地 SQLite 结构迁移（键名：`db_schema_version`）。
- **Contract schema_version**：仅用于网络契约兼容（字段名：`schema_version`，来源于 generated contracts）。

这两条版本线允许独立演进，不要求同步 bump。

### 3.2 本地元数据键（`workspace_meta`）

- `db_schema_version`

V1 规范中，禁止继续引入语义模糊的统一键名（例如单一 `schema_version`）作为新实现基线。

说明：`schema_version` 在 V1 中保留为契约层唯一字段名；本地 DB 仅新增 `db_schema_version`，不再引入 `wire_schema_version` 键名。

### 3.3 用户 Profile 一致性

- 认证与会话层面保持单一 user profile（见 `auth-session-model-v1.md`）。
- 本地 DB 模块不引入任何第二套用户 profile 概念。
- CLI/daemon 的差异仅是运行时角色（runtime role），不是用户 profile。

---

## 4. 本地 Schema 清单（V1 基线）

### 4.1 Shared tables（CLI 与 daemon 共同依赖）

| Table | 作用 | Owner |
| --- | --- | --- |
| `workspace_meta` | 本地运行时元数据（版本、workspace路径、phase等） | Shared |
| `creators` | Creator 本地缓存 | Shared |
| `reference_sources` | 参考资料扫描索引与状态 | Shared |

### 4.2 Daemon-only tables（由 daemon profile 管理）

| Table | 作用 | Owner |
| --- | --- | --- |
| `outbox` | 同步命令队列 | Daemon |
| `auth_tokens` | OAuth token 本地存储 | Daemon |
| `device_code_sessions` | 设备授权会话 | Daemon |
| `acp_tool_audit_log` | ACP 工具调用审计 | Daemon |
| `acp_sessions` | ACP 会话持久化 | Daemon |

### 4.3 版本元数据约束

- `workspace_meta` 必须包含：
  - `db_schema_version`
- `db_schema_version` 由迁移流程维护。
- 契约层 `schema_version` 由 generated contracts 常量统一提供，不作为本地 DB 新键落盘。

---

## 5. 工具与运行时栈（V1）

### 5.1 核心工具链

- **SQLite engine**：`rusqlite`（CLI + daemon 统一）
- **Daemon 连接池**：`deadpool-sqlite`
- **Schema 常量来源**：`nexus-contracts` generated constants
- **Migration执行**：本地 DB 独立模块内的顺序 migration runner（非外部 ORM migration）

### 5.2 初始化与运行参数

- 连接后执行 `PRAGMA journal_mode = WAL`
- 连接后执行 `PRAGMA foreign_keys = ON`
- 所有建表语句必须 `IF NOT EXISTS`（保证 `init` 幂等）

### 5.3 验证与测试工具

- 单元测试：`cargo test`（schema init、version seeding、migration路径）
- CI 门禁：
  - shared schema一致性检查
  - `db_schema_version` 可读性检查
  - 契约层 `schema_version` 来源一致性检查
  - migration continuity 检查

---

## 6. 模块化方案（V1 推荐实现）

### 6.1 独立模块

新增本地 DB 独立模块（建议独立 crate）作为唯一 owner，例如：

- `crates/nexus-local-db`（建议名，可按仓库命名规范微调）

### 6.2 该模块的职责

1. 共享 schema 常量与 key 常量
2. 共享表 DDL（`workspace_meta` 等 shared tables）
3. schema 初始化（idempotent）
4. migration runner（按版本顺序执行）
5. 版本读写与健康检查 API

### 6.3 CLI / daemon 的职责

- CLI 与 daemon 仅负责：
  - 传入 runtime role / mode（例如 `Cli` 或 `Daemon`）
  - 调用模块 API 完成初始化、迁移、版本读取
- CLI / daemon 不再维护 duplicated shared DDL 与版本写入逻辑。

---

## 7. 参考 API 形状（规范级）

```rust
pub enum RuntimeRole {
    Cli,
    Daemon,
}

pub struct SchemaVersions {
    pub db_schema_version: u32,
    pub schema_version: u32,
}

pub fn init(conn: &rusqlite::Connection, role: RuntimeRole) -> Result<()>;
pub fn migrate(conn: &rusqlite::Connection) -> Result<()>;
pub fn read_versions(conn: &rusqlite::Connection) -> Result<SchemaVersions>;
pub fn validate(conn: &rusqlite::Connection) -> Result<()>;
```

说明：

- API 形状为规范建议，允许实现细节调整。
- `schema_version` 的来源必须是 generated contracts 常量，不可硬编码。

---

## 8. 迁移策略（V1）

### 8.1 迁移执行规则

1. 启动时读取当前 `db_schema_version`
2. 按 `vN -> vN+1` 顺序执行迁移
3. 每步成功后再推进版本号
4. 任一步失败必须中止并返回可诊断错误

### 8.2 幂等与可恢复

- `init` 必须幂等。
- migration 应保证可重复执行时安全（至少对成功步骤具备幂等保护）。
- 失败时不得写入错误的目标版本号。

---

## 9. CI / 质量门禁

V1 要求从“文本比较两个实现”升级到“验证共享模块契约”：

1. CLI / daemon 都依赖本地 DB 独立模块
2. `db_schema_version` 一致且可读取
3. 契约层 `schema_version` 来源于 generated contracts
4. migration 序列连续、无断档
5. CLI / daemon 对同一空库初始化后的 shared schema 一致

---

## 10. 与 v1 其他规格的对齐

- 与 [`cli-spec.md`](./cli-spec.md) 对齐：
  - 本地 runtime 由 CLI/daemon 驱动，但底层本地 DB 能力应模块化复用。
- 与 nexus-platform `v1-spec/shared/schema/codegen-strategy-v1.md` 对齐：
  - wire contract 仍由 JSON Schema 真源驱动，本稿不改变该事实。
- 与 nexus-platform `v1-spec/architecture.md` 对齐：
  - 保持“本地优先 + 结构化同步 + 可恢复”系统目标。

---

## 11. 迭代落地建议（实施顺序）

### 步骤 A（最小落地）

- 建立独立模块骨架
- 接管版本常量、metadata keys、版本写入/读取
- CLI / daemon 改为调用模块 API

### 步骤 B（核心收敛）

- shared table DDL 收敛到独立模块
- CLI / daemon 删除 duplicated shared DDL

### 步骤 C（迁移与运维）

- 完成 migration registry
- 增加 `db status` 可观测能力（版本、健康检查）
- CI 增加 migration continuity gate

---

## 12. 规范性结论

在 V1 语义下，**本地 DB 是一项独立能力，不是 CLI/daemon 的“内部细节副本”**。  
因此应采用“独立模块 + 双版本线 + 统一 migration”的治理模型，作为后续实现与评审基线。
