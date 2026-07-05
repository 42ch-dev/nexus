# Nexus Local DB Schema Spec

**Status**: Normative  
**Document class**: Master  
**V1.40 Shipped amendments:** §4.1.2 `kb_key_blocks` validation intent — application-layer validation in `nexus-kb::validation` module (ValidationMode::Novel enforces `body.attributes.novel_category`); `narrative_worlds` rows via `creator world create` (V1.40 P0); `kb_extract_jobs` artifact locator columns (`source_kind`, `source_locator`, `profile_hint`, `work_id`) added V1.40 P3.  
**V1.42 Draft amendment:** `work_chapters` PK migration — composite primary key `(work_id, volume, chapter)` with `volume INTEGER NOT NULL DEFAULT 1`; backfill existing rows `volume = 1`; drop legacy `(work_id, chapter)` PK. Normative detail: [novel-writing/workflow-profile.md §4.5.4](novel-writing/workflow-profile.md). Plan: [2026-06-11-v1.42-multi-volume.md](../../plans/2026-06-11-v1.42-multi-volume.md).  
**Last updated**: 2026-06-22 — V1.56 P0 workspace_sessions  

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
| `narrative_worlds` | Workspace-local World projections for narrative read paths | `nexus-narrative` domain; `nexus-local-db` migrations/storage mechanics |
| `narrative_timeline_events` | Workspace-local timeline event projections used by `NarrativeGateway` | `nexus-narrative` domain; `nexus-local-db` migrations/storage mechanics |
| `kb_key_blocks` | World-scoped narrative KB KeyBlocks persisted in workspace `state.db` | `nexus-kb` domain; `nexus-local-db` migrations/storage mechanics |
| `kb_source_anchors` | Multi-anchor rows attached to `kb_key_blocks` for `KbStore::attach_source_anchor` / `get_anchors` | `nexus-kb` domain; `nexus-local-db` migrations/storage mechanics |
| `knowledge_entries` | User-scoped knowledge entries for Moment context assembly (V1.27) | `nexus-knowledge` domain; `nexus-local-db` migrations/storage mechanics |

#### 4.1.1 `reference_sources`

`reference_sources` is the registry table for User-scoped local reference units. The canonical body text is externalized to `body.md` under the active Creator root; see [reference-store-layout.md](reference-store-layout.md).

Required V1.26 columns:

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `reference_source_id` | `TEXT` | yes | Registry primary key and `references/units/<id>/` directory name. |
| `workspace_id` | `TEXT` | yes | Workspace binding in `state.db`. |
| `source_type` | `TEXT` | yes | Contract enum value such as `file`, `url`, `pdf`, or `note`. |
| `source_mutability` | `TEXT` | yes | `NOT NULL DEFAULT 'static'`; allowed values: `static`, `refreshable`. |
| `uri` | `TEXT` | yes | `nexus42://references/units/<id>` or original import URI. |
| `title` | `TEXT` | yes | Human-readable title. |
| `tags` | `TEXT` | no | Serialized tag list if present. |
| `content_hash` | `TEXT` | no | Hash of canonical `body.md` when available. |
| `content_path` | `TEXT` | no | Relative path from Creator root, e.g. `references/units/<id>/body.md`. |
| `content` | `TEXT` | no | **Deprecated** inline body column; `NULL` for new rows. |
| `scan_status` | `TEXT` | yes | Scan lifecycle status. |
| `created_at` | `TEXT` | yes | Creation timestamp. |
| `updated_at` | `TEXT` | no | Last registry update timestamp. |

New code MUST write canonical body text to `content_path` on disk instead of `content`. Listing references MUST be satisfiable from registry metadata without reading full body text.

**DF-43 ownership boundary (V1.55 P0)**: `nexus-local-db` is the sole production persistence owner for reference sources. The `nexus-knowledge` crate provides domain types (`ReferenceSource`), traits (`KnowledgeStore`), and adapter seams (`From<ReferenceSourceRow> for nexus_knowledge::ReferenceSource` in `nexus-local-db/src/reference_source.rs`) — it does **not** introduce its own SQLite/file-backed truth source. All production DB writes go through `nexus-local-db` DAO functions (`register`, `list`, `get_by_id`).

#### 4.1.2 Narrative + World KB persistence (V1.26 draft)

These tables live in the same workspace `state.db` as `reference_sources` and support the V1.26 persistent `NarrativeGateway` and `KbStore` adapters. Domain semantics remain owned by `nexus-narrative` and `nexus-kb`; `nexus-local-db` owns migration ordering and SQLite mechanics.

**V1.40 P1 migration intent** ([`2026-06-10-v1.40-world-kb-taxonomy`](../plans/2026-06-10-v1.40-world-kb-taxonomy.md)): add validation for narrative `block_type` vocabulary (`foundation`, `background`, `character`, `location`, `society`, `rules`, `economy`) on `kb_key_blocks.block_type` — via CHECK constraint, application-layer enum, or companion lookup table. Minimum `body_json` shape per category is enforced in `nexus-kb`, not in raw SQL. **V1.40 P1 shipped**: application-layer validation in `nexus-kb::validation` module; `ValidationMode::Novel` enforces `body.attributes.novel_category` on insert/update. **V1.40 P0** may add `narrative_worlds` rows via `creator world create` without altering `kb_key_blocks` DDL beyond existing indexes.

`workspace_id` is retained on `narrative_worlds` as the logical workspace binding. The current local schema has no `workspaces` catalog table, so this draft does **not** add a physical workspace FK. If a workspace catalog is introduced later, `narrative_worlds.workspace_id` should become the FK target without changing child table ownership.

```sql
CREATE TABLE IF NOT EXISTS narrative_worlds (
    world_id TEXT PRIMARY KEY CHECK (world_id LIKE 'wld_%'),
    workspace_id TEXT NOT NULL,
    owner_creator_id TEXT NOT NULL,
    title TEXT NOT NULL,
    slug TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived', 'paused')),
    visibility TEXT NOT NULL,
    time_policy TEXT NOT NULL,
    canon_revision INTEGER,
    current_timeline_head_id TEXT,
    current_time_pointer TEXT,
    root_fork_branch_id TEXT,
    world_rules_json TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    FOREIGN KEY (owner_creator_id) REFERENCES creators (creator_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_narrative_worlds_workspace_id
    ON narrative_worlds (workspace_id);
CREATE INDEX IF NOT EXISTS idx_narrative_worlds_owner_creator_id
    ON narrative_worlds (owner_creator_id);
CREATE INDEX IF NOT EXISTS idx_narrative_worlds_status
    ON narrative_worlds (status);

CREATE TABLE IF NOT EXISTS narrative_timeline_events (
    timeline_event_id TEXT PRIMARY KEY CHECK (timeline_event_id LIKE 'evt_%'),
    world_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('canon', 'provisional', 'rejected')),
    sequence_no INTEGER NOT NULL CHECK (sequence_no >= 0),
    title TEXT,
    summary TEXT,
    caused_by_event_ids_json TEXT,
    affected_key_block_ids_json TEXT,
    source_command_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (world_id) REFERENCES narrative_worlds (world_id) ON DELETE CASCADE,
    UNIQUE (world_id, branch_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_world_id
    ON narrative_timeline_events (world_id);
CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_world_branch_sequence
    ON narrative_timeline_events (world_id, branch_id, sequence_no);
CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_status
    ON narrative_timeline_events (status);

CREATE TABLE IF NOT EXISTS kb_key_blocks (
    key_block_id TEXT PRIMARY KEY CHECK (key_block_id LIKE 'kb_%'),
    world_id TEXT NOT NULL,
    block_type TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('provisional', 'confirmed', 'deprecated', 'merged', 'deleted')),
    revision INTEGER,
    body_json TEXT,
    source_anchor_json TEXT,
    created_from_command_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    FOREIGN KEY (world_id) REFERENCES narrative_worlds (world_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_id
    ON kb_key_blocks (world_id);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_status
    ON kb_key_blocks (world_id, status);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_type
    ON kb_key_blocks (world_id, block_type);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_canonical_name
    ON kb_key_blocks (world_id, canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_active_unique
    ON kb_key_blocks (world_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');

CREATE TABLE IF NOT EXISTS kb_source_anchors (
    key_block_id TEXT NOT NULL,
    anchor_ordinal INTEGER NOT NULL,
    source_anchor_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (key_block_id, anchor_ordinal),
    FOREIGN KEY (key_block_id) REFERENCES kb_key_blocks (key_block_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_kb_source_anchors_key_block_id
    ON kb_source_anchors (key_block_id);
```

#### 4.1.3 Knowledge entries persistence (V1.27)

`knowledge_entries` stores user-scoped knowledge for the `User Knowledge` domain in Moment context assembly. Each entry belongs to a single user and carries a JSON tags array for filtering.

```sql
CREATE TABLE IF NOT EXISTS knowledge_entries (
    entry_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    content TEXT NOT NULL,
    reference_uri TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_entries_user_id
    ON knowledge_entries (user_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_entries_tags
    ON knowledge_entries (tags_json);
CREATE INDEX IF NOT EXISTS idx_knowledge_entries_user_tags
    ON knowledge_entries (user_id, tags_json);
```

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `entry_id` | `TEXT` | yes | Primary key; generated UUID |
| `user_id` | yes | Owner scope for user isolation |
| `tags_json` | `TEXT` | yes | JSON array of tag strings, e.g. `["rust","tutorial"]` |
| `content` | `TEXT` | yes | Knowledge content text |
| `reference_uri` | `TEXT` | no | Optional URI for provenance |
| `created_at` | `TEXT` | yes | Creation timestamp |
| `updated_at` | `TEXT` | yes | Last update timestamp |

Column notes:

- `world_id`, `timeline_event_id`, and `key_block_id` preserve current domain ID prefixes (`wld_`, `evt_`, `kb_`).
- `*_json` columns store serialized domain value objects or arrays where current traits only need full-object reconstruction (`world_rules`, event cause/affected ID arrays, KeyBlock body, and SourceAnchor values).
- `narrative_timeline_events` is required by `NarrativeGateway::get_timeline`, `get_event`, and `get_narrative_context`.
- `kb_source_anchors` is required because `KbStore::attach_source_anchor` permits multiple anchors per KeyBlock; `kb_key_blocks.source_anchor_json` remains the optional embedded `KeyBlock.source_anchor` value.
- The partial unique index on `kb_key_blocks` implements the existing active uniqueness rule: one active `(world_id, canonical_name, block_type)` tuple, while `deleted`, `merged`, and `deprecated` rows no longer block replacement.

#### 4.1.4 `works` lifecycle lock columns (V1.41 Draft — DF-60)

Additive columns on existing `works` table. Normative: [novel-writing/multi-work-lifecycle.md](novel-writing/multi-work-lifecycle.md).

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `novel_completion_status` | `TEXT` | no | `completed` when §6 criteria met (V1.38 residual) |
| `completion_locked_at` | `TEXT` | no | DB mirror of `.completion-lock.json` |
| `runtime_lock_holder` | `TEXT` | no | `pid:<pid>:<uuid>` while mutating command holds Work |
| `runtime_lock_acquired_at` | `TEXT` | no | ISO-8601 |
| `lineage_from_work_id` | `TEXT` | no | Set on new Work from `creator bootstrap --from-work` |

#### 4.1.5 Novel work pool tables (V1.41 Draft — DF-61)

Creator-scoped; not Work rows. Normative: [novel-writing/work-pool.md](novel-writing/work-pool.md). Validation rules for KB taxonomy: see [nexus-kb::validation](../../../crates/nexus-kb/src/validation.rs) (World KB — separate concern).

**`novel_pool_entries`**

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `entry_id` | `TEXT` | yes | PK; `npe_` prefix |
| `creator_id` | `TEXT` | yes | FK to creator scope |
| `work_id` | `TEXT` | no | Bound after scaffold |
| `title` | `TEXT` | yes | Display title |
| `status` | `TEXT` | yes | `active` \| `queued` \| `completed` |
| `created_at` | `TEXT` | yes | ISO-8601 |
| `updated_at` | `TEXT` | yes | ISO-8601 |

Partial unique index: one `active` row per `creator_id`.

Partial unique index: one row per `(creator_id, work_id)` where `work_id IS NOT NULL`.

**`inspiration_items`**

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `item_id` | `TEXT` | yes | PK; `npi_` prefix |
| `creator_id` | `TEXT` | yes | |
| `rel_path` | `TEXT` | yes | Path to `{workspace}/Pool/Ideas/<slug>.md` |
| `title` | `TEXT` | yes | |
| `status` | `TEXT` | yes | `open` \| `promoted` \| `archived` |
| `promoted_work_id` | `TEXT` | no | |
| `created_at` | `TEXT` | yes | |

### 4.2 Daemon-only tables（由 daemon profile 管理）

| Table | 作用 | Owner |
| --- | --- | --- |
| `outbox` | 同步命令队列 | Daemon |
| `auth_tokens` | OAuth token 本地存储 | Daemon |
| `device_code_sessions` | 设备授权会话 | Daemon |
| `acp_tool_audit_log` | ACP 工具调用审计 | Daemon |
| `acp_sessions` | ACP 会话持久化 | Daemon |
| `workspace_sessions` | Workspace session persistence for `workspace.open`/`workspace.commit` with file-level OCC content hashes (V1.56 P0) | Daemon |

#### 4.2.1 `workspace_sessions` (V1.56 P0)

Persists `workspace.open`/`workspace.commit` sessions in `SQLite`. Replaces the V1.55 in-memory `WorkspaceSessionManager`.

```sql
CREATE TABLE IF NOT EXISTS workspace_sessions (
    session_id TEXT PRIMARY KEY CHECK (session_id LIKE 'ws_%'),
    workspace_root TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    existed INTEGER NOT NULL DEFAULT 0,
    file_hashes_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    consumed INTEGER NOT NULL DEFAULT 0 CHECK (consumed IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_workspace_sessions_expires_at
    ON workspace_sessions (expires_at);

CREATE INDEX IF NOT EXISTS idx_workspace_sessions_consumed_expires
    ON workspace_sessions (consumed, expires_at);
```

| Column | Type | Required | Notes |
| --- | --- | --- | --- |
| `session_id` | `TEXT` | yes | PK; `ws_<uuid>` format |
| `workspace_root` | `TEXT` | yes | Absolute path to workspace creative root |
| `relative_path` | `TEXT` | yes | Relative path within workspace |
| `existed` | `INTEGER` | yes | Whether the target path existed at open time |
| `file_hashes_json` | `TEXT` | yes | JSON `{relative_path: sha256_hex}` for OCC |
| `created_at` | `TEXT` | yes | RFC 3339 creation timestamp |
| `expires_at` | `TEXT` | yes | RFC 3339 expiry timestamp (created_at + TTL) |
| `consumed` | `INTEGER` | yes | 0 = active, 1 = committed (consumed) |

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
