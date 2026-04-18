# Local DB Refactor Design (V2)

**Source**: Inline design spec for local SQLite (`state.db`) refactoring.
**Status**: Active design baseline
**Plan**: 2026-04-08-local-db-refactor
**Supersedes**: [local-db-refactor-v1.md](../archived/knowledge/local-db-refactor-v1.md) (archived)
**Updated**: 2026-04-18 — WS8 T9 revision: engine migrated from `rusqlite` + `deadpool-sqlite` to `sqlx`.

**On-disk path (per-workspace):** Defined only in **v1-spec** — `adr/adr-014-…`, `cli-sync/local-db-schema-v1.md` §0, `cli-sync/cli-spec-v1.md` §13 (resolve `specs_root.v1-spec` via `.agents/local-paths.json`). [local-fs-layout-creator-workspace-v1.md](local-fs-layout-creator-workspace-v1.md) is a non-normative pointer. Module ownership and schema rules **in this document** are unchanged.

---

## 1. Design Goals

1. **Single ownership of local DB capabilities** — Create independent module `nexus-local-db`
2. **CLI/daemon reuse via unified API** — No more duplicated logic
3. **Decoupled version lines** — DB schema version vs contract schema_version clearly separated
4. **Preserve semantics** — Maintain V1 product semantics (local-first, structured sync, ACP client-only)

---

## 2. Two Version Lines (Must Be Separated)

### 2.1 DB Schema Version

- **Purpose**: Local SQLite structure migrations only
- **Key name**: `db_schema_version`
- **Storage location**: `workspace_meta` table — maintained as a **mirror** key for backwards compatibility and human inspection
- **Authoritative tracking**: sqlx's `_sqlx_migrations` table (records every applied migration with version + timestamp)
- **Maintenance**: By sqlx migration runner (`sqlx::migrate!()`); `workspace_meta` key updated post-migration

### 2.2 Contract Schema Version

- **Purpose**: Network contract compatibility only
- **Field name**: `schema_version` (from generated contracts)
- **Maintenance**: From `nexus-contracts` generated constants
- **Forbidden**: Do NOT add `wire_schema_version` key to local DB

**Key constraint**: These two version lines can evolve independently, no synchronization required.

---

## 3. Table Classification and Ownership

### 3.1 Shared Tables (CLI and daemon both depend)


| Table               | Purpose                                                        | Owner  |
| ------------------- | -------------------------------------------------------------- | ------ |
| `workspace_meta`    | Local runtime metadata (versions, workspace path, phase, etc.) | Shared |
| `creators`          | Creator local cache                                            | Shared |
| `reference_sources` | Reference material scan index and status                       | Shared |


### 3.2 Daemon-only Tables


| Table                  | Purpose                       | Owner  |
| ---------------------- | ----------------------------- | ------ |
| `outbox`               | Sync command queue            | Daemon |
| `auth_tokens`          | OAuth token local storage     | Daemon |
| `device_code_sessions` | Device authorization sessions | Daemon |
| `acp_tool_audit_log`   | ACP tool invocation audit     | Daemon |
| `acp_sessions`         | ACP session persistence       | Daemon |


---

## 4. Modularization Plan

### 4.1 Independent Crate

**Crate**: `crates/nexus-local-db`

### 4.2 Module Responsibilities

1. Shared schema constants and key constants
2. Shared table DDL (`workspace_meta`, etc.) — expressed as `.sql` migration files
3. Schema initialization via `sqlx::migrate!()` (idempotent)
4. Migration runner (`sqlx::migrate!().run(&pool).await`)
5. Version read/write and health check APIs (async, pool-based)
6. Pool bootstrap and lifecycle management (`open_pool`)

### 4.3 CLI / daemon Responsibilities

- CLI and daemon only:
  - Pass runtime role/mode (e.g., `Cli` or `Daemon`)
  - Call module APIs for initialization, migration, version reading
- CLI/daemon no longer maintain duplicated shared DDL and version write logic
- Both sides receive `Arc<SqlitePool>` from `nexus-local-db`

---

## 5. API Shape (Specification Level)

```rust
pub enum RuntimeRole {
    Cli,
    Daemon,
}

pub struct SchemaVersions {
    pub db_schema_version: u32,
    pub schema_version: u32, // From nexus-contracts
}

pub async fn open_pool(db_path: &Path) -> Result<SqlitePool>;
pub async fn run_migrations(pool: &SqlitePool) -> Result<()>;
pub async fn seed_versions(pool: &SqlitePool) -> Result<()>;
pub async fn read_versions(pool: &SqlitePool) -> Result<SchemaVersions>;
pub async fn validate(pool: &SqlitePool) -> Result<()>;
```

**Key constraints**:

- `schema_version` MUST come from generated contracts constants, never hardcode
- All functions are async, taking `&SqlitePool` (or `&SqliteConnection` for transactional helpers)
- API shape is a specification suggestion, implementation details can vary

---

## 6. Initialization and Runtime Parameters

- Pool creation: `SqlitePoolOptions::new().max_connections(1)` (SQLite single-writer) with `PRAGMA journal_mode = WAL` and `PRAGMA foreign_keys = ON` set as pool connection hooks
- Migration: `sqlx::migrate!()` executes all pending `.sql` files in timestamp order
- Seed: `seed_versions()` writes initial `workspace_meta` rows (idempotent via `INSERT OR IGNORE`)
- All CREATE TABLE statements in migration `.sql` files MUST use `IF NOT EXISTS` (ensure idempotent init)

---

## 7. Migration Strategy

### 7.1 Migration Format

- Migrations are **timestamped `.sql` files** under `crates/nexus-local-db/migrations/`
- Naming convention: `{timestamp}_{description}.sql` (e.g., `20260417_000001_initial_schema.sql`)
- `sqlx::migrate!()` macro discovers and applies them in sorted order

### 7.2 Migration Execution Rules

1. `sqlx::migrate!().run(&pool).await` reads `_sqlx_migrations` table to determine which have been applied
2. Executes pending migrations in timestamp order
3. Records each migration in `_sqlx_migrations` on success
4. Must abort on any failure and return diagnosable error

### 7.3 Idempotency and Recoverability

- `open_pool` + `run_migrations` is idempotent — safe to call on every startup
- Already-applied migrations are skipped (tracked by `_sqlx_migrations` table)
- Must NOT advance migration state on failure

---

## 8. CI / Quality Gates

1. CLI/daemon both depend on local DB independent module
2. `db_schema_version` consistent and readable (mirror in `workspace_meta`)
3. Contract `schema_version` from generated contracts
4. Migration sequence continuous, no gaps (enforced by sqlx migration runner)
5. CLI/daemon produce identical shared schema on same empty DB init
6. `SQLX_OFFLINE=true` for CI (pre-checked `.sqlx/` query metadata)

---

## 9. User Profile Consistency

- Maintain single user profile for auth/session layer
- Local DB module does NOT introduce any second user profile concept
- CLI/daemon difference is runtime role only, not user profile

---

## 10. Current State (Post-WS8)

### 10.1 Post-WS8 State (V1.4 WS8 complete)

- **Engine**: `sqlx` (sqlite + runtime-tokio + macros + migrate + chrono + uuid) — unified across all workspace crates
- **Pool**: `Arc<sqlx::SqlitePool>` exposed by `nexus-local-db`, shared by CLI, daemon, and sync modules
- **Migrations**: `.sql` files under `crates/nexus-local-db/migrations/`, driven by `sqlx::migrate!()`
- **All callers async**: 34 files (~193 references) ported from `rusqlite`/`deadpool-sqlite` to async sqlx
- **Legacy deps removed**: `rusqlite` and `deadpool-sqlite` removed from workspace dependencies
- **Decision SSOT**: [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md) §2.3 + §3.3

### 10.2 Archived: Pre-WS8 State

- **CLI schema**: `crates/nexus42/src/db/mod.rs`
  - Defined `DB_SCHEMA_VERSION` and `WIRE_SCHEMA_VERSION`
  - Initialized shared tables (workspace_meta, creators, reference_sources)
  - Comment states "Keep in sync with daemon schema"
- **Daemon schema**: `crates/nexus42d/src/db/schema.rs`
  - Defined same `DB_SCHEMA_VERSION` and `WIRE_SCHEMA_VERSION`
  - Initialized all tables (shared + daemon-only)
  - Includes PRAGMA settings and tests

#### Problems (resolved by WS8)

- **Duplication**: Same constants and DDL maintained in two places → unified under `nexus-local-db`
- **Manual sync**: Relied on comments, prone to drift → single migration pipeline
- **Version key confusion**: Used `wire_schema_version` key name (non-compliant) → corrected
- **Two DB engines**: `rusqlite` (sync) + `sqlx` (async) at the same file risked lock contention → unified on `sqlx`

---

## 11. Non-goals

- Do not define platform-side database structure
- Do not replace JSON Schema as wire contract source of truth
- Do not introduce complex database framework beyond `sqlx`

---

## References

- Inline design spec for `state.db` refactoring
- Aligned with V1 product semantics: local-first, structured sync, ACP client-only
- V1.4 WS8 plan: [2026-04-17-v1.4-ws8-local-db-sqlx-migration.md](../2026-04-17-v1.4-ws8-local-db-sqlx-migration.md)
- Crate selection SSOT: [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md) §2.3 + §3.3
