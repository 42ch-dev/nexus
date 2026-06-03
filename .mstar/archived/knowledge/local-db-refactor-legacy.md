# Local DB Refactor Design (V1)

**Source**: Inline design spec for local SQLite (`state.db`) refactoring.
**Status**: Active design baseline
**Plan**: 2026-04-08-local-db-refactor

**On-disk path (per-workspace):** Defined only in **v1-spec** — `adr/adr-014-…`, `cli-sync/local-db-schema-v1.md` §0, `cli-sync/cli-spec-v1.md` §13 (resolve `specs_root.v1-spec` via `.mstar/local-paths.json`). [local-fs-layout-creator-workspace.md](local-fs-layout-creator-workspace.md) is a non-normative pointer. Module ownership and schema rules **in this document** are unchanged.

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
- **Storage location**: `workspace_meta` table
- **Maintenance**: By migration process

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

**Suggested name**: `crates/nexus-local-db`

### 4.2 Module Responsibilities

1. Shared schema constants and key constants
2. Shared table DDL (`workspace_meta`, etc.)
3. Schema initialization (idempotent)
4. Migration runner (execute in version order)
5. Version read/write and health check APIs

### 4.3 CLI / daemon Responsibilities

- CLI and daemon only:
  - Pass runtime role/mode (e.g., `Cli` or `Daemon`)
  - Call module APIs for initialization, migration, version reading
- CLI/daemon no longer maintain duplicated shared DDL and version write logic

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

pub fn init(conn: &rusqlite::Connection, role: RuntimeRole) -> Result<()>;
pub fn migrate(conn: &rusqlite::Connection) -> Result<()>;
pub fn read_versions(conn: &rusqlite::Connection) -> Result<SchemaVersions>;
pub fn validate(conn: &rusqlite::Connection) -> Result<()>;
```

**Key constraints**:

- `schema_version` MUST come from generated contracts constants, never hardcode
- API shape is a specification suggestion, implementation details can vary

---

## 6. Initialization and Runtime Parameters

- Execute `PRAGMA journal_mode = WAL` after connection
- Execute `PRAGMA foreign_keys = ON` after connection
- All CREATE TABLE statements MUST use `IF NOT EXISTS` (ensure idempotent init)

---

## 7. Migration Strategy

### 7.1 Migration Execution Rules

1. Read current `db_schema_version` at startup
2. Execute migrations in `vN -> vN+1` order
3. Advance version number after each successful step
4. Must abort on any failure and return diagnosable error

### 7.2 Idempotency and Recoverability

- `init` must be idempotent
- Migration should be safe to re-execute (at least idempotent protection for successful steps)
- Must NOT write incorrect target version on failure

---

## 8. CI / Quality Gates

1. CLI/daemon both depend on local DB independent module
2. `db_schema_version` consistent and readable
3. Contract `schema_version` from generated contracts
4. Migration sequence continuous, no gaps
5. CLI/daemon produce identical shared schema on same empty DB init

---

## 9. User Profile Consistency

- Maintain single user profile for auth/session layer
- Local DB module does NOT introduce any second user profile concept
- CLI/daemon difference is runtime role only, not user profile

---

## 10. Current State (Pre-refactor)

### 10.1 Existing Implementation

- **CLI schema**: `crates/nexus42/src/db/mod.rs`
  - Defines `DB_SCHEMA_VERSION` and `WIRE_SCHEMA_VERSION`
  - Initializes shared tables (workspace_meta, creators, reference_sources)
  - Comment states "Keep in sync with daemon schema"
- **Daemon schema**: `crates/nexus42d/src/db/schema.rs`
  - Defines same `DB_SCHEMA_VERSION` and `WIRE_SCHEMA_VERSION`
  - Initializes all tables (shared + daemon-only)
  - Includes PRAGMA settings and tests

### 10.2 Problems

- **Duplication**: Same constants and DDL maintained in two places
- **Manual sync**: Relies on comments, prone to drift
- **Version key confusion**: Uses `wire_schema_version` key name (non-compliant)

---

## 11. Non-goals

- Do not define platform-side database structure
- Do not replace JSON Schema as wire contract source of truth
- Do not introduce complex database framework in V1 (keep SQLite + lightweight migration)

---

## References

- Inline design spec for `state.db` refactoring
- Aligned with V1 product semantics: local-first, structured sync, ACP client-only

