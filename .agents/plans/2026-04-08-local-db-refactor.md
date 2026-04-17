# Local DB Refactor Plan

> **For agentic workers**: REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal**: Refactor local SQLite (`state.db`) into independent module `nexus-local-db` with single ownership, decoupled version lines, and unified API for CLI/daemon.

**Source**: Inline design spec (see `.agents/plans/knowledge/local-db-refactor-v2.md`)

**Priority**: P1 — Foundational refactor to eliminate schema drift and duplicated logic

---

## Problem Statement

Current state:
- CLI (`crates/nexus42/src/db/mod.rs`) and daemon (`crates/nexus42d/src/db/schema.rs`) maintain **duplicated** schema definitions
- Manual sync via comments ("Keep in sync with daemon schema") — **prone to drift**
- Version key uses non-compliant `wire_schema_version` instead of `schema_version` from contracts
- No clear ownership: CLI and daemon both maintain shared DDL

Design spec requirements:
1. Single ownership module (`nexus-local-db`)
2. Decoupled version lines: `db_schema_version` (local) vs `schema_version` (contracts)
3. Unified API for CLI/daemon
4. Migration runner with version ordering

---

## Design Baseline

See `.agents/plans/knowledge/local-db-refactor-v2.md` for:
- Two version lines separation
- Table classification (Shared vs Daemon-only)
- API shape specification
- Migration strategy
- CI quality gates

**Key constraints**:
- `schema_version` MUST come from `nexus-contracts::generated::LATEST_SCHEMA_VERSION` — **never hardcode**
- DB schema version (`db_schema_version`) is independent, managed by migration process
- All CREATE TABLE use `IF NOT EXISTS` for idempotency

---

## Execution Phases

### Phase A: Module Skeleton (Foundation)

**Estimated Effort**: S (~1 session)
**Target**: Establish module structure and constants

#### Task 1: Create `crates/nexus-local-db` crate

**Files**:
- `crates/nexus-local-db/Cargo.toml`
- `crates/nexus-local-db/src/lib.rs`
- `crates/nexus-local-db/src/version.rs`
- `crates/nexus-local-db/src/schema.rs`

**Acceptance Criteria**:
- [ ] New crate created with `rusqlite` and `nexus-contracts` dependencies
- [ ] `version.rs` defines `DB_SCHEMA_VERSION` constant
- [ ] `schema_version` re-exported from `nexus-contracts::generated::LATEST_SCHEMA_VERSION`
- [ ] Crate compiles and passes `cargo test`

---

#### Task 2: Define RuntimeRole and SchemaVersions types

**Files**:
- `crates/nexus-local-db/src/lib.rs`

**Acceptance Criteria**:
- [ ] `RuntimeRole` enum with `Cli` and `Daemon` variants
- [ ] `SchemaVersions` struct with `db_schema_version: u32` and `schema_version: u32`
- [ ] Public API exported in `lib.rs`

---

### Phase B: Shared Schema Migration (Core Convergence)

**Estimated Effort**: M (~2 sessions)
**Target**: Move shared tables DDL and logic to new module

#### Task 3: Move shared table DDL to `nexus-local-db`

**Files**:
- `crates/nexus-local-db/src/schema.rs` (DDL definitions)
- `crates/nexus42/src/db/mod.rs` (remove DDL, use imports)
- `crates/nexus42d/src/db/schema.rs` (remove shared DDL, use imports)

**Shared tables**:
- `workspace_meta`
- `creators`
- `reference_sources`

**Acceptance Criteria**:
- [ ] All shared table DDL defined once in `nexus-local-db/src/schema.rs`
- [ ] CLI and daemon import from `nexus-local-db`
- [ ] No duplicated DDL in CLI/daemon code
- [ ] Existing tests pass

---

#### Task 4: Implement `init()` with role-based initialization

**Files**:
- `crates/nexus-local-db/src/lib.rs`

**Logic**:
- If role == `Cli`: initialize shared tables only
- If role == `Daemon`: initialize shared tables + daemon-only tables
- Execute PRAGMAS: `journal_mode = WAL`, `foreign_keys = ON`
- Seed `db_schema_version` and `schema_version` in `workspace_meta`

**Acceptance Criteria**:
- [ ] `init()` accepts `RuntimeRole` and behaves accordingly
- [ ] All CREATE TABLE use `IF NOT EXISTS`
- [ ] `db_schema_version` seeded correctly
- [ ] `schema_version` from `nexus-contracts` constants (not hardcoded)
- [ ] Idempotent: safe to call multiple times
- [ ] Unit tests for both roles

---

#### Task 5: Remove deprecated `wire_schema_version` key

**Files**:
- `crates/nexus-local-db/src/schema.rs`
- Migration: rename `wire_schema_version` → `schema_version` (if exists)

**Acceptance Criteria**:
- [ ] No `wire_schema_version` key written to `workspace_meta`
- [ ] Migration handles rename from existing databases
- [ ] `schema_version` key correctly populated

---

### Phase C: Daemon-only Tables (Complete Migration)

**Estimated Effort**: M (~2 sessions)
**Target**: Consolidate daemon-only tables in module

#### Task 6: Move daemon-only table DDL to `nexus-local-db`

**Files**:
- `crates/nexus-local-db/src/schema.rs` (daemon DDL)
- `crates/nexus42d/src/db/schema.rs` (remove daemon DDL, use imports)

**Daemon-only tables**:
- `outbox`
- `auth_tokens`
- `device_code_sessions`
- `acp_tool_audit_log`
- `acp_sessions`

**Acceptance Criteria**:
- [ ] All daemon-only table DDL in `nexus-local-db`
- [ ] Daemon imports from `nexus-local-db`
- [ ] `init(Daemon)` initializes all tables
- [ ] Existing daemon tests pass

---

### Phase D: Version Management API (Observability)

**Estimated Effort**: S (~1 session)
**Target**: Provide version read/validate APIs
**Status**: ✅ Done (2026-04-08)

#### Task 7: Implement `read_versions()` API

**Files**:
- `crates/nexus-local-db/src/lib.rs`
- `crates/nexus-local-db/src/error.rs`

**Logic**:
- Query `workspace_meta` for `db_schema_version`
- Return `schema_version` from contracts constant
- Return `SchemaVersions` struct

**Acceptance Criteria**:
- [x] Read `db_schema_version` from `workspace_meta`
- [x] `schema_version` from generated contracts
- [x] Unit tests (6 tests)

---

#### Task 8: Implement `validate()` API

**Files**:
- `crates/nexus-local-db/src/lib.rs`
- `crates/nexus-local-db/src/error.rs`

**Logic**:
- Verify `workspace_meta` table exists
- Verify `db_schema_version` key exists
- Verify `schema_version` key exists
- Check version values are valid u32

**Acceptance Criteria**:
- [x] Health check for database state
- [x] Returns `Ok(())` or descriptive error
- [x] Unit tests (8 tests)

---

### Phase E: Migration Runner (Future-proofing)

**Estimated Effort**: M (~2 sessions)
**Target**: Enable schema migrations with version ordering
**Status**: ✅ Done (2026-04-08)

#### Task 9: Implement migration registry and runner

**Files**:
- `crates/nexus-local-db/src/migration.rs`
- `crates/nexus-local-db/src/lib.rs`

**Logic**:
- Read current `db_schema_version`
- Execute migrations in order (v1 → v2 → v3 ...)
- Update version after each successful migration
- Abort on failure, preserve old version

**Acceptance Criteria**:
- [x] Migration registry with version-tagged functions
- [x] Sequential execution with version update
- [x] Idempotent migrations
- [x] Failure aborts without corrupting version
- [x] Unit tests for migration path (7 tests)

---

#### Task 10: Add CLI `db status` command

**Files**:
- `crates/nexus42/src/commands/db.rs`
- `crates/nexus42/src/commands/mod.rs`
- `crates/nexus42/src/main.rs`

**Logic**:
- Call `read_versions()` and `validate()`
- Print database status: versions, health, tables

**Acceptance Criteria**:
- [x] `nexus42 db status` prints versions and health
- [x] Shows `db_schema_version` and `schema_version`
- [x] Lists existing tables
- [x] Shows pragmas (journal_mode, foreign_keys)

---

### Phase F: CI Quality Gates

**Estimated Effort**: S (~1 session)
**Target**: Add automated checks for schema consistency
**Status**: ✅ Done (2026-04-08)

#### Task 11: Add schema consistency check to CI

**Files**:
- `.github/workflows/ci.yml`

**Checks**:
- CLI/daemon both depend on `nexus-local-db`
- `db_schema_version` readable and consistent
- `schema_version` from contracts (not hardcoded)
- No duplicated DDL in CLI/daemon

**Acceptance Criteria**:
- [x] CI job verifies schema consistency
- [x] Fails if CLI/daemon have duplicated DDL
- [x] Fails if `schema_version` hardcoded
- [x] CI passes for current codebase
- [x] No clippy warnings
- [x] Documentation updated

---

## Validation Commands

**After each phase**:
```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo +nightly fmt --all -- --check
```

**Schema consistency**:
```bash
# Verify no duplicated DDL
grep -r "CREATE TABLE IF NOT EXISTS workspace_meta" crates/nexus42*/src/db/ || echo "OK"

# Verify schema_version from contracts
grep "LATEST_SCHEMA_VERSION" crates/nexus-local-db/src/
```

---

## Plan Completion Criteria

- [x] `crates/nexus-local-db` created and compiles
- [x] Shared table DDL in single location
- [x] CLI/daemon use unified API from `nexus-local-db`
- [x] `db_schema_version` and `schema_version` correctly separated
- [x] No `wire_schema_version` key
- [x] `init()` idempotent for both roles
- [x] `read_versions()` and `validate()` implemented
- [x] Migration runner with sequential execution
- [x] CLI `db status` command
- [x] CI schema consistency checks
- [x] All tests passing
- [x] 0 clippy warnings
- [x] Documentation updated

---

**Plan Status**: ✅ Done (2026-04-08)

---

## Dependencies

- **External**: None
- **Internal**: V1.1 GA complete (confirmed)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing databases | High | Migration from `wire_schema_version` key, preserve data |
| CLI/daemon behavior change | Medium | Comprehensive tests for both roles |
| Migration runner complexity | Medium | Start with simple sequential runner, extend later |

---

## Notes

This is a **foundational refactor** to eliminate schema drift and establish single ownership of local DB capabilities. The module should remain lightweight (no complex ORM) while providing clear API boundaries.

**Key principle**: Local DB is an **independent capability**, not a "copy of internal details" in CLI/daemon.

**Created**: 2026-04-08
**Status**: Done
**Updated**: 2026-04-08
**Completed**: 2026-04-08