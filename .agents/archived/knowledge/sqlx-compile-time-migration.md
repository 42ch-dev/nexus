# sqlx Compile-Time Macro Migration Plan (V1.4)

## Overview

This document specifies the migration from **runtime string queries** (`sqlx::query()`, `sqlx::query_as()`) to **compile-time checked macros** (`sqlx::query!()`, `sqlx::query_as!()`) across all crates in the Nexus workspace. It also establishes the `.sqlx/` offline workflow required for CI builds without a live database.

**Motivation**: The project owner has mandated compile-time macro checking: *"永远不要选择速度而去积累风险的技术栈"*. Runtime queries defer SQL errors to runtime; compile-time macros catch them at build time.

**Scope**: ~79 runtime query calls across 5 crates (~30 files). No application behavior changes — purely a safety migration.

**Status**: Active — authoritative for V1.4 sqlx macro migration.

---

## 1. Migration Strategy

### 1.1 Crate Dependency Graph & Migration Order

The crate dependency graph (relevant edges):

```
nexus-local-db          (foundational — no sqlx crate deps)
  ↑
  ├── nexus-sync        (depends on nexus-local-db)
  ├── nexus-orchestration (depends on nexus-local-db)
  ├── nexus42d          (depends on nexus-local-db, nexus-sync, nexus-orchestration)
  └── nexus42           (depends on nexus-local-db, nexus-sync)
```

**Recommended migration order** (bottom-up, dependencies first):

| Batch | Crate | Rationale |
|-------|-------|-----------|
| 1 | `nexus-local-db` | Foundational crate; owns migrations; all other crates depend on it. Must be first so downstream crates can reference its `.sqlx/` data. |
| 2 | `nexus-sync` | Self-contained outbox with its own schema (DDL in `outbox.rs` init). No upstream sqlx crate deps beyond `nexus-local-db`. |
| 3 | `nexus-orchestration` | Uses `nexus-local-db` pool + migrations. Has schedule/derivation queries against tables created by migrations. |
| 4 | `nexus42` (CLI) | Depends on `nexus-local-db` and `nexus-sync`. Fewer queries, mostly in CLI command paths and test utilities. |
| 5 | `nexus42d` (daemon) | Top-level consumer; depends on all others. Has its own handler queries. Migrate last so all upstream is stable. |

**Why bottom-up**: Compile-time macros validate SQL against the database schema at build time. If a downstream crate references a table altered by an upstream crate's migration, the upstream must be migrated first to ensure `.sqlx/` metadata reflects the final schema state.

### 1.2 Per-Query Conversion Guide

#### `sqlx::query()` → `sqlx::query!()`

**Before** (runtime):
```rust
sqlx::query("INSERT INTO local_identities (creator_id, identity_type, display_name, created_at, platform_linked) VALUES (?1, ?2, ?3, ?4, 0)")
    .bind(creator_id)
    .bind(identity_type)
    .bind(display_name)
    .bind(created_at)
    .execute(pool)
    .await?;
```

**After** (compile-time):
```rust
sqlx::query!("INSERT INTO local_identities (creator_id, identity_type, display_name, created_at, platform_linked) VALUES (?, ?, ?, ?, 0)",
    creator_id, identity_type, display_name, created_at
)
    .execute(pool)
    .await?;
```

**Key changes**:
- Replace `?1, ?2, …` positional markers with `?` (unnamed) — sqlx macros use Rust argument position, not SQLite parameter numbers.
- Remove `.bind()` chains — parameters become positional Rust arguments after the SQL string.
- The macro validates the SQL against the schema at compile time and checks parameter types.

#### `sqlx::query_as::<T>()` → `sqlx::query_as!(T, ...)`

**Before** (runtime with `sqlx::FromRow`):
```rust
let rows: Vec<LocalIdentityRowRaw> = sqlx::query_as(
    "SELECT creator_id, identity_type, display_name, created_at, platform_linked, platform_creator_id
     FROM local_identities ORDER BY created_at",
)
.fetch_all(pool)
.await?;
```

**After** (compile-time):
```rust
let rows = sqlx::query_as!(
    LocalIdentityRowRaw,
    "SELECT creator_id, identity_type, display_name, created_at, platform_linked, platform_creator_id
     FROM local_identities ORDER BY created_at"
)
.fetch_all(pool)
.await?;
```

**Key changes**:
- First argument becomes the output struct type (unqualified name is fine).
- No generic angle brackets — the type is a macro argument.
- Column names in SELECT must match struct field names (or be aliased with `AS`).
- Type inference: the macro determines the Rust type for each column from the SQLite schema. If there is a mismatch (e.g., SQLite `INTEGER` → Rust `bool`), you must use `CAST` or `.map()`.

#### Handling the `platform_linked` bool ↔ INTEGER pattern

Several structs use `sqlx::FromRow` with a raw `i32` field and then convert to `bool` in a manual `From` impl (e.g., `LocalIdentityRowRaw` → `LocalIdentityRow`). With compile-time macros, the macro infers `i64` for INTEGER columns. Two approaches:

**Option A — Keep the raw struct + From conversion** (minimal diff):
```rust
// The macro sees INTEGER → i64, but our struct has i32.
// Use the _i64 suffix or accept i64 and convert.
let rows = sqlx::query_as!(LocalIdentityRowRaw,
    "SELECT creator_id, identity_type, display_name, created_at,
            platform_linked as \"platform_linked!\",
            platform_creator_id
     FROM local_identities ORDER BY created_at"
)
.fetch_all(pool)
.await?;
```
With the `as "field_name!"` override, the macro treats it as `i32` (non-nullable INTEGER). If that doesn't match, accept `i64` and adjust the struct.

**Option B — Eliminate the raw struct, use `try_from` in the macro** (preferred for new code):
```rust
// Change the struct to use i64 for platform_linked
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LocalIdentityRow {
    pub creator_id: String,
    pub identity_type: String,
    pub display_name: Option<String>,
    pub created_at: String,
    pub platform_linked: bool,
    pub platform_creator_id: Option<String>,
}

// Query with manual mapping
let row = sqlx::query!(
    "SELECT creator_id, identity_type, display_name, created_at, platform_linked, platform_creator_id
     FROM local_identities WHERE creator_id = ?",
    creator_id
)
.fetch_optional(pool)
.await?
.map(|r| LocalIdentityRow {
    creator_id: r.creator_id,
    identity_type: r.identity_type,
    display_name: r.display_name,
    created_at: r.created_at,
    platform_linked: r.platform_linked != 0,
    platform_creator_id: r.platform_creator_id,
})
```

**Recommendation**: Use **Option B** for `nexus-local-db` structs where possible — it removes the intermediate `*Raw` type and makes the mapping explicit. For struct fields where the macro-inferred type already matches (String, Option<String>, i64), use the anonymous struct returned by `query!` directly and map in `.map()`.

### 1.3 Handling Dynamic SQL

Some queries build SQL dynamically. Compile-time macros **cannot** be used when the SQL string is not a literal known at compile time. The following patterns warrant runtime `sqlx::query()`:

| Pattern | Example | Runtime OK? |
|---------|---------|-------------|
| PRAGMA statements | `PRAGMA journal_mode = WAL` | ✅ Yes — no table reference |
| DDL (CREATE TABLE, CREATE INDEX) | Table creation in `outbox.rs` init | ✅ Yes — DDL cannot be checked by macros |
| Fully static SQL with known parameters | `SELECT ... WHERE id = ?` | ❌ Must use `query!` / `query_as!` |
| Dynamic WHERE clauses / optional filters | Building query from user input | ✅ Runtime acceptable **with justification comment** |

**Rule**: Every runtime `sqlx::query()` call must include a comment explaining why the compile-time macro cannot be used:

```rust
// SAFETY: DDL statement — sqlx macros cannot validate CREATE TABLE.
sqlx::query("CREATE TABLE IF NOT EXISTS outbox_entries (...)").execute(pool).await?;

// SAFETY: PRAGMA statement — no table schema to validate against.
sqlx::query("PRAGMA journal_mode = WAL").execute(pool).await?;

// SAFETY: Dynamic WHERE clause built from user-provided filter set.
let sql = build_filter_query(filters);
sqlx::query(&sql).bind(param).execute(pool).await?;
```

### 1.4 Struct Alignment

Compile-time macros infer column types from the database schema. The following type mapping applies for SQLite:

| SQLite Type | sqlx Macro Rust Type |
|-------------|---------------------|
| `TEXT` | `String` (non-null) / `Option<String>` (nullable) |
| `INTEGER` | `i64` (non-null) / `Option<i64>` (nullable) |
| `REAL` | `f64` / `Option<f64>` |
| `BLOB` | `Vec<u8>` / `Option<Vec<u8>>` |
| `TEXT NOT NULL DEFAULT (...)` | `String` |

**Common mismatches to watch for**:
- `bool` fields stored as `INTEGER` (0/1) → macro gives `i64`; convert with `!= 0` in `.map()`.
- `u32`/`usize` fields → macro gives `i64`; cast with `as u32` / `as usize`.
- `chrono::DateTime` stored as TEXT → requires the `chrono` feature (already in workspace dep).

When a struct field type does not match what the macro infers, you have two options:
1. Change the struct field to match the macro-inferred type and add a conversion method.
2. Use column aliasing in SQL: `CAST(platform_linked AS INTEGER) as "platform_linked!"` to force a specific inference.

---

## 2. `.sqlx/` Offline Workflow Setup

### 2.1 Overview

The `.sqlx/` directory stores **query metadata** (serialized JSON files) that allows `sqlx::query!()` and related macros to validate SQL at compile time **without a running database**. This is called **offline mode** (`SQLX_OFFLINE=true`).

Each query macro invocation generates a JSON file under `.sqlx/` keyed by a hash of the SQL text and the database URL. The `cargo sqlx prepare` command:
1. Connects to a database with all migrations applied.
2. Runs `cargo check` to discover all macro invocations.
3. Serializes each query's metadata (parameters, return columns, types) into `.sqlx/`.

### 2.2 DATABASE_URL Convention

For `cargo sqlx prepare` to work, it needs a `DATABASE_URL` pointing to a SQLite database with **all migrations applied**.

**Recommended convention**:

```bash
# At workspace root, create a reference database for codegen:
export DATABASE_URL="sqlite:./.sqlx/state.db?mode=rwc"
```

This creates a `state.db` file **inside** `.sqlx/` so it stays local and is gitignored (see §2.5).

**Setup script** (developer runs once, or before `cargo sqlx prepare`):

```bash
#!/bin/bash
# .sqlx/setup-db.sh — Create reference database with all migrations applied.
set -e
DB_PATH=".sqlx/state.db"
mkdir -p .sqlx
rm -f "$DB_PATH"
export DATABASE_URL="sqlite:$DB_PATH?mode=rwc"

# nexus-local-db owns the canonical migrations.
# We use a small Rust program or direct sqlx-cli to apply them.
cargo sqlx database create
cargo sqlx migrate run --source crates/nexus-local-db/migrations

# NOTE: nexus-sync creates its own tables via DDL in outbox.rs init_pool_with_schema.
# These are NOT in the migration directory and must be handled separately.
# For the reference database, run the DDL statements manually or via a helper.
```

**Important caveat — nexus-sync's outbox schema**: The `outbox_entries` and `partial_apply_states` tables are created via DDL in `crates/nexus-sync/src/outbox.rs` (`init_pool_with_schema`), not via `sqlx::migrate!()`. For `cargo sqlx prepare` to work for nexus-sync queries, these tables must exist in the reference database. Two approaches:

1. **Add a SQL file** to `.sqlx/` (e.g., `.sqlx/sync-schema.sql`) containing the DDL statements, and run it after migrations.
2. **Migrate the DDL into `nexus-local-db/migrations/`** — this is the preferred long-term solution (already tracked as dual-outbox consolidation in `dual-outbox-architecture.md`), but can be deferred. For the migration, use approach 1.

### 2.3 `cargo sqlx prepare` Workflow

```bash
# 1. Ensure the reference database is up-to-date
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx database reset  # drops + recreates + runs migrations

# 2. Generate .sqlx/ metadata
cargo sqlx prepare --workspace --all -- --all-targets

# 3. Verify — the command should exit 0 and .sqlx/ should contain query-*.json files
ls .sqlx/query-*.json | wc -l  # expect ~79 files

# 4. Commit .sqlx/ changes
git add .sqlx/
git commit -m "chore(sqlx): update .sqlx offline metadata"
```

**When to run `cargo sqlx prepare`**:
- **After any migration change** (files under `crates/nexus-local-db/migrations/`).
- **After converting a runtime query to a macro** (each new `query!`/`query_as!` invocation adds a metadata file).
- **Before committing** if CI `verify-sqlx-offline` is enabled.
- **Never** on CI — CI uses `SQLX_OFFLINE=true` to read the committed `.sqlx/` data.

### 2.4 CI Integration

Re-enable the commented-out `verify-sqlx-offline` job in `.github/workflows/ci.yml`:

```yaml
verify-sqlx-offline:
  name: Verify sqlx offline metadata
  runs-on: ubuntu-latest
  needs: verify-codegen
  env:
    SQLX_OFFLINE: "true"
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: actions/download-artifact@v4
      with:
        name: generated-types
        path: .
    - name: Install cargo-sqlx
      run: cargo install sqlx-cli --no-default-features --features sqlite
    - name: Verify sqlx offline metadata is up-to-date
      run: cargo sqlx prepare --workspace --all -- --all-targets
```

**What this checks**:
- `cargo sqlx prepare` in CI with `SQLX_OFFLINE=true` will:
  1. Read committed `.sqlx/` metadata instead of connecting to a database.
  2. Run `cargo check` to verify all macro invocations match the committed metadata.
  3. If a new `query!()` was added without running `cargo sqlx prepare`, the check fails (metadata hash mismatch or missing file).
- This ensures `.sqlx/` never goes stale.

**Note on CI dependency**: Both `rust-checks` and `rust-tests` jobs already set `SQLX_OFFLINE: "true"`. Once `.sqlx/` is committed, these jobs will use it for macro expansion. If `.sqlx/` is missing or stale, these jobs will fail at the first `query!()` invocation.

### 2.5 `.sqlx/` in Git & `.gitignore`

**The `.sqlx/` directory MUST be committed to git** — it is the offline metadata that makes CI builds work without a database.

**What to commit**:
- `.sqlx/query-*.json` — query metadata files (one per macro invocation)
- `.sqlx/README.md` — (optional) brief description of the directory

**What NOT to commit** (add to `.gitignore`):
- `.sqlx/state.db` — the reference database used during `cargo sqlx prepare`
- `.sqlx/state.db-wal` / `.sqlx/state.db-shm` — SQLite WAL files
- `.sqlx/*.sql` — helper scripts (if any)

**Recommended `.gitignore` additions**:

```gitignore
# sqlx offline reference database (only metadata is committed)
.sqlx/state.db
.sqlx/state.db-wal
.sqlx/state.db-shm
```

**Do NOT add** `.sqlx/` to `.gitignore` as a whole — that would exclude the query metadata files that CI needs.

---

## 3. AGENTS.md Convention Addition (Draft)

The following text block should be added to the project `AGENTS.md` under a new section (e.g., after "Rust development"):

```markdown
## sqlx Compile-Time Macros (Mandatory)

### Default: use compile-time macros

All new sqlx queries **MUST** use compile-time checked macros:

- Use `sqlx::query!("SQL", params...)` for execute-only statements.
- Use `sqlx::query_as!(Type, "SQL", params...)` for queries returning typed rows.
- Use `sqlx::query_scalar!("SQL", params...)` for single-value returns.

**Do NOT** use runtime `sqlx::query()` or `sqlx::query_as::<T>()` for static SQL.

### When runtime `sqlx::query()` is acceptable

Runtime queries are **only** acceptable for:

1. **DDL statements** (`CREATE TABLE`, `CREATE INDEX`, `ALTER TABLE`) — sqlx macros cannot validate DDL.
2. **PRAGMA statements** — no table schema to validate.
3. **Truly dynamic SQL** — where the query string is constructed at runtime based on user input or configuration. Each such usage MUST include a `// SAFETY: dynamic SQL — compile-time macro not applicable.` comment explaining why a macro cannot be used.

If in doubt, use the macro.

### Adding new queries or migrations

When adding or modifying SQL queries:

1. Write the query using `sqlx::query!()` / `sqlx::query_as!()`.
2. Run `cargo sqlx prepare --workspace --all -- --all-targets` to update `.sqlx/` metadata.
3. Commit the updated `.sqlx/` files alongside your code changes.

When adding new migrations under `crates/nexus-local-db/migrations/`:

1. Write the migration SQL file.
2. Run `export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc" && cargo sqlx database reset && cargo sqlx prepare --workspace --all -- --all-targets`.
3. Commit `crates/nexus-local-db/migrations/` **and** `.sqlx/` in the same commit.

CI will reject PRs where `.sqlx/` is out of sync with the committed macro invocations.
```

---

## 4. Risk Assessment

### 4.1 What Could Go Wrong

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| **Type mismatch between struct and schema** | Build failure in macro expansion | Medium | Run `cargo check` frequently during conversion; the macro itself catches this at compile time. |
| **`.sqlx/` metadata goes stale** | CI failure; local build failure for others | Medium | CI `verify-sqlx-offline` job catches this. Add `.sqlx/` update to PR checklist. |
| **nexus-sync outbox tables missing from reference DB** | `cargo sqlx prepare` fails for nexus-sync queries | High | Include sync DDL in `.sqlx/sync-schema.sql` helper. Long-term: migrate DDL to `nexus-local-db/migrations/`. |
| **Nullable column inference differs** | Compile error or unexpected `Option<T>` vs `T` | Medium | Test each conversion individually. Use `as "col!"` (non-null) or `as "col?"` (nullable) annotations. |
| **Transaction-bound queries** | Some queries use `&mut *tx` (transaction refs) — macros support this but syntax differs slightly | Low | `sqlx::query!(...).execute(&mut *tx)` works identically; only the bind syntax changes. |
| **Test-only queries** | Tests use runtime queries for dynamic setup; converting all may be impractical | Low | See §4.2 below. |
| **Large batch conversion breaks workspace** | Cannot compile at intermediate states | Medium | Migrate one crate at a time; each crate should compile and pass tests independently. |

### 4.2 Test Files

Test files contain runtime queries for two purposes:

1. **Schema validation tests** (e.g., `schema.rs` tests) — these test that DDL works correctly. Keep as runtime `sqlx::query()` since they are testing DDL itself.
2. **Data setup helpers** (e.g., `fresh_pool()` in test modules) — these insert test data. Some can be converted to macros for consistency, but it is acceptable to leave test-only runtime queries if:
   - The query is in a `#[cfg(test)]` module.
   - The query is testing DDL or schema behavior.
   - Converting would require significant test restructuring.

**Recommendation**: Convert test queries that mirror production queries (same SQL, same tables) to macros for consistency. Leave DDL tests and PRAGMA tests as runtime. Add `// SAFETY: test-only DDL verification` comments to any remaining runtime queries in test code.

### 4.3 Backwards Compatibility

- **No wire format changes** — this is purely a compile-time safety migration.
- **No database schema changes** — migrations are unchanged.
- **No API changes** — public function signatures remain identical.
- **Build requirement**: After migration, building requires either a live database or committed `.sqlx/` metadata (`SQLX_OFFLINE=true`). This is already the case for `rust-checks` and `rust-tests` in CI (they set `SQLX_OFFLINE: "true"`), but the metadata files must exist.
- **Pre-1.0 allowance**: Breaking changes are permitted per project conventions. If a struct field type must change (e.g., `bool` → `i64` for macro compatibility), this is acceptable.

---

## 5. Effort Estimate (Agent-Oriented)

### Per-Crate Complexity

| Batch | Crate | ~Query Count | Files | Complexity | Notes |
|-------|-------|-------------|-------|------------|-------|
| 1 | `nexus-local-db` | ~15 | 5 (`lib.rs`, `identity.rs`, `memory_fragment.rs`, `soul_meta.rs`, `pending_review.rs`) | **S** | Foundational; well-isolated CRUD functions. Some `*Raw` struct cleanup. |
| 2 | `nexus-sync` | ~19 | 2 (`outbox.rs`, `pool.rs`) | **M** | Many queries; complex tuple types in `replay()`/`get()`; outbox DDL stays runtime; also needs `.sqlx/sync-schema.sql` setup. |
| 3 | `nexus-orchestration` | ~18 | 4 (`supervisor.rs`, `derivation.rs`, `sqlite.rs`, test files) | **M** | `ScheduleRow` mapping; complex SELECT with many columns; graph-flow `SessionStorage` impl. |
| 4 | `nexus42` | ~9 | 6 (scattered across CLI commands + `db/mod.rs` + `test_utils.rs`) | **XS** | Few queries; mostly simple; test utilities. |
| 5 | `nexus42d` | ~18 | 8+ (scattered across `db/`, `api/handlers/`, `auth/`, tests) | **M** | Most spread-out; handler queries + auth + integration tests. |

### Recommended Batching

| Batch | Crates | Estimated Agent Sessions | Dependencies |
|-------|--------|-------------------------|--------------|
| **Batch 1** | `nexus-local-db` + `.sqlx/` setup | 1 session | None — foundational |
| **Batch 2** | `nexus-sync` | 1 session | Batch 1 complete; sync schema in reference DB |
| **Batch 3** | `nexus-orchestration` | 1 session | Batch 1 complete |
| **Batch 4** | `nexus42` + `nexus42d` | 1–2 sessions | Batches 1–3 complete |
| **Batch 5** | CI re-enable + `.gitignore` + AGENTS.md | 1 session | All batches complete |

**Total estimated effort**: ~5 agent sessions (M complexity overall).

### Pre-Work (before Batch 1)

1. Install `cargo-sqlx`: `cargo install sqlx-cli --no-default-features --features sqlite`
2. Create `.sqlx/` directory structure and reference database
3. Add `.gitignore` entries for `.sqlx/state.db*`
4. Create `.sqlx/sync-schema.sql` with nexus-sync DDL statements

### Verification Per Batch

For each crate after migration:
1. `cargo check -p <crate>` — macro expansion succeeds
2. `cargo test -p <crate>` — all tests pass
3. `cargo clippy -p <crate> -- -D warnings` — no new warnings
4. `cargo sqlx prepare --workspace --all -- --all-targets` — metadata generated

### Final Verification

After all batches:
1. Full workspace build: `cargo check --all`
2. Full test suite: `cargo test --all`
3. Clippy: `cargo clippy --all -- -D warnings`
4. Formatting: `cargo +nightly fmt --all -- --check`
5. Offline metadata: `SQLX_OFFLINE=true cargo check --all` (simulates CI)
6. Commit `.sqlx/` and verify CI passes

---

## Appendix A: Query Pattern Inventory

### nexus-local-db

| Function | File | Current Pattern | Macro Target |
|----------|------|-----------------|--------------|
| `open_pool` (PRAGMAs) | `lib.rs` | `sqlx::query("PRAGMA ...")` | **Keep runtime** (PRAGMA) |
| `seed_versions` | `lib.rs` | `sqlx::query("INSERT OR REPLACE ...")` × 2 | `sqlx::query!` |
| `read_versions` | `lib.rs` | `sqlx::query_as::<_, (String,)>(...)` × 2 | `sqlx::query_as!` |
| `create_local_identity` | `identity.rs` | `sqlx::query("INSERT ...")` | `sqlx::query!` |
| `get_local_identity` | `identity.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `list_local_identities` | `identity.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `link_to_platform` | `identity.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `unlink_from_platform` | `identity.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `delete_local_identity` | `identity.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `create_fragment` | `memory_fragment.rs` | `sqlx::query("INSERT ...")` | `sqlx::query!` |
| `list_fragments` | `memory_fragment.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `list_fragments_by_session` | `memory_fragment.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `delete_fragment` | `memory_fragment.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `get_all_keywords` | `memory_fragment.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `upsert` (soul_meta) | `soul_meta.rs` | `sqlx::query("INSERT ... ON CONFLICT ...")` | `sqlx::query!` |
| `get` (soul_meta) | `soul_meta.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `delete` (soul_meta) | `soul_meta.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `create_pending_review` | `pending_review.rs` | `sqlx::query("INSERT ...")` | `sqlx::query!` |
| `list_pending_reviews` | `pending_review.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `get_pending_review` | `pending_review.rs` | `sqlx::query_as(...)` | `sqlx::query_as!` |
| `delete_pending_review` | `pending_review.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `count_pending_reviews` | `pending_review.rs` | `sqlx::query_as(...)` | `sqlx::query_scalar!` |

### nexus-sync

| Function | File | Current Pattern | Macro Target |
|----------|------|-----------------|--------------|
| DDL in `init_pool_with_schema` | `outbox.rs` | `sqlx::query(ddl)` | **Keep runtime** (DDL) |
| PRAGMA in `init_pool_with_schema` | `outbox.rs` | `sqlx::query("PRAGMA ...")` | **Keep runtime** (PRAGMA) |
| `append` | `outbox.rs` | `sqlx::query("INSERT ...")` in tx | `sqlx::query!` |
| `stage` | `outbox.rs` | `sqlx::query("INSERT ...")` in tx | `sqlx::query!` |
| `stage_if_absent` (exists check) | `outbox.rs` | `sqlx::query_as::<_, (bool,)>(...)` | `sqlx::query_scalar!` |
| `stage_if_absent` (insert) | `outbox.rs` | `sqlx::query("INSERT ...")` | `sqlx::query!` |
| `mark_sent` | `outbox.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `mark_acked` | `outbox.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `mark_conflicted_with_retry` | `outbox.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `mark_failed` (retry_count read) | `outbox.rs` | `sqlx::query_as::<_, (i64,)>(...)` | `sqlx::query_scalar!` |
| `mark_failed` (permanent) | `outbox.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `mark_failed` (retry) | `outbox.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `replay` | `outbox.rs` | `sqlx::query_as::<_, 9-tuple>(...)` | `sqlx::query_as!` |
| `get` | `outbox.rs` | `sqlx::query_as::<_, 9-tuple>(...)` | `sqlx::query_as!` |
| `purge_acked` | `outbox.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `count_by_state` | `outbox.rs` | `sqlx::query_as("SELECT COUNT(*) ...")` | `sqlx::query_scalar!` |
| `persist_partial_apply_state` | `outbox.rs` | `sqlx::query("INSERT OR REPLACE ...")` | `sqlx::query!` |
| `load_partial_apply_state` | `outbox.rs` | `sqlx::query_as::<_, (String,)>(...)` | `sqlx::query_as!` |
| `remove_partial_apply_state` | `outbox.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `list_partial_apply_states` | `outbox.rs` | `sqlx::query_as::<_, (String, String)>(...)` | `sqlx::query_as!` |
| `OutboxPool::new` (PRAGMAs) | `pool.rs` | `sqlx::query("PRAGMA ...")` × 2 | **Keep runtime** (PRAGMA) |

### nexus-orchestration

| Function | File | Current Pattern | Macro Target |
|----------|------|-----------------|--------------|
| `save` | `storage/sqlite.rs` | `sqlx::query("INSERT ... ON CONFLICT ...")` | `sqlx::query!` |
| `get` | `storage/sqlite.rs` | `sqlx::query_as::<_, SessionRow>(...)` | `sqlx::query_as!` |
| `delete` | `storage/sqlite.rs` | `sqlx::query("DELETE ...")` | `sqlx::query!` |
| `tick_inner` (load schedules) | `supervisor.rs` | `sqlx::query_as::<_, ScheduleRow>(...)` | `sqlx::query_as!` |
| `tick_inner` (load deps) | `supervisor.rs` | `sqlx::query_as::<_, (String,)>(...)` | `sqlx::query_scalar!` |
| `tick_inner` (update to running) | `supervisor.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `on_schedule_terminal` (fetch creator_id) | `supervisor.rs` | `sqlx::query_as::<_, (String,)>(...)` | `sqlx::query_scalar!` |
| `on_schedule_terminal` (update status) | `supervisor.rs` | `sqlx::query("UPDATE ...")` | `sqlx::query!` |
| `insert_pending` | `supervisor.rs` | `sqlx::query("INSERT ...")` | `sqlx::query!` |
| Various schedule queries | `derivation.rs` | `sqlx::query()` / `sqlx::query_as()` | Per-query conversion |

### nexus42 & nexus42d

Detailed per-file inventory is deferred to implementation — the patterns are identical to those documented above. Key observations:
- `nexus42/src/db/mod.rs` uses `sqlx::query_scalar` (already close to macro form).
- `nexus42d/src/db/pool.rs` contains only PRAGMA queries (keep runtime).
- `nexus42d/src/db/schema.rs` contains only DDL/test queries (keep runtime).
- `nexus42d` handler queries follow standard `query()` / `query_as()` patterns.

---

## Appendix B: `.sqlx/sync-schema.sql` (Helper)

This file should be created at `.sqlx/sync-schema.sql` and run against the reference database after migrations, so that `cargo sqlx prepare` can resolve nexus-sync's outbox table references:

```sql
-- nexus-sync outbox schema (DDL from outbox.rs init_pool_with_schema)
-- Run this against the reference database after nexus-local-db migrations.

CREATE TABLE IF NOT EXISTS outbox_entries (
    outbox_entry_id   TEXT PRIMARY KEY,
    bundle_id         TEXT NOT NULL,
    idempotency_key   TEXT NOT NULL,
    delivery_state    TEXT NOT NULL DEFAULT 'staged',
    retry_count       INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    next_retry_at     TEXT,
    command_payload   TEXT NOT NULL DEFAULT '{}',
    bundle_payload    TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_delivery_state
    ON outbox_entries(delivery_state);

CREATE INDEX IF NOT EXISTS idx_outbox_next_retry
    ON outbox_entries(next_retry_at)
    WHERE delivery_state IN ('staged', 'failed');

CREATE INDEX IF NOT EXISTS idx_outbox_bundle_id
    ON outbox_entries(bundle_id);

CREATE TABLE IF NOT EXISTS partial_apply_states (
    outbox_entry_id   TEXT PRIMARY KEY,
    state_json        TEXT NOT NULL,
    recorded_at       TEXT NOT NULL,
    retry_count       INTEGER NOT NULL DEFAULT 0
);
```

---

## Appendix C: `cargo sqlx prepare` Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| `error: database not found` | No reference database | Run `export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc" && cargo sqlx database create && cargo sqlx migrate run --source crates/nexus-local-db/migrations` |
| `error: no such table: outbox_entries` | nexus-sync tables not in reference DB | Run `.sqlx/sync-schema.sql` against the reference DB |
| `error: type mismatch: expected i64, found bool` | SQLite INTEGER → Rust bool | Use `.map()` to convert, or change struct field type |
| `error: column "x" not found` | SELECT alias doesn't match struct field | Add `AS "field_name"` alias in SQL |
| `cargo sqlx prepare` changes existing `.sqlx/` files | Schema changed or query text changed | Normal — re-commit the updated files |
| Macro works locally but fails in CI | `.sqlx/` not committed or stale | Ensure `.sqlx/query-*.json` files are committed; re-run `cargo sqlx prepare` |
