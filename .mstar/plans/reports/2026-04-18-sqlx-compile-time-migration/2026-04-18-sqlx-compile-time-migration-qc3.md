---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-18-sqlx-compile-time-migration"
verdict: "Approve"
generated_at: "2026-04-18"
---

# QC Report #3: sqlx Compile-Time Macro Migration

## Review Context

| Field | Value |
|-------|-------|
| **Plan ID** | `2026-04-18-sqlx-compile-time-migration` |
| **Review range** | `git diff 1f4f251..b9067ce` |
| **Working branch** | `feature/v1.4` |
| **Review cwd** | `/Users/bibi/workspace/organizations/42ch/nexus` |
| **Reviewer** | qc-specialist-3 (performance & reliability) |

## Scope

All sqlx runtime `sqlx::query()` / `sqlx::query_as()` / `sqlx::query_scalar()` calls across 5 crates have been migrated to compile-time checked macros:

- `crates/nexus-local-db/src/` (5 files)
- `crates/nexus-sync/src/` (2 files)
- `crates/nexus-orchestration/src/` (4 files)
- `crates/nexus42/src/` (6 files)
- `crates/nexus42d/src/` (15 files)
- CI, AGENTS.md, .gitignore, .sqlx/ metadata (97 JSON files)

## Verdict

**Approve** — No critical or warning findings. The migration is well-executed with proper edge-case handling, appropriate SAFETY annotations for acceptable runtime query exceptions, and comprehensive documentation.

---

## Findings

### Critical: 0

None.

### Warning: 0

None.

### Suggestion: 3

#### S-1: Inconsistent SAFETY comment prefix in `nexus42d/src/api/handlers/sync.rs`

**Severity:** suggestion
**File:** `crates/nexus42d/src/api/handlers/sync.rs`
**Lines:** 357–364

**Description:**  
In `sync.rs` line 357, the SAFETY comment uses a different format from the rest of the codebase:

```rust
let last_sync_row =
    sqlx::query!("SELECT value FROM workspace_meta WHERE key = 'last_sync_at'")
        .fetch_optional(state.pool())
        .await
        .ok()
        .flatten();
let last_sync_at = last_sync_row.map(|r| r.value);
```

The SAFETY comment is missing on this query. While it's not strictly needed (the query is static), consistency with other similar patterns in the codebase would improve maintainability.

**Recommended fix:** Add a `// SAFETY:` comment consistent with other static queries in the file, or clarify in AGENTS.md whether `query!` calls against static SQL strings require SAFETY annotations.

---

#### S-2: Orphaned `debug_assert!` after ownership check in `memory.rs`

**Severity:** suggestion
**File:** `crates/nexus42d/src/api/handlers/memory.rs`
**Lines:** 349–353

**Description:**  
After the ownership verification and deletion, there's a `debug_assert!` that checks `rows_affected > 0`. The ownership check already ensures the row exists, so this assertion is technically redundant but harmless:

```rust
debug_assert!(
    affected.rows_affected() > 0,
    "pending review should exist and be owned by creator"
);
```

This is a pre-existing pattern, not introduced by the migration. No action required.

---

#### S-3: `ON CONFLICT DO UPDATE` patterns lack inline documentation

**Severity:** suggestion
**Files:** `crates/nexus-local-db/src/soul_meta.rs` (upsert), `crates/nexus-orchestration/src/storage/sqlite.rs` (upsert with ON CONFLICT)

**Description:**  
The `INSERT ... ON CONFLICT DO UPDATE` patterns are correctly migrated, but the conflict resolution behavior (upsert semantics) could benefit from brief inline documentation in the SQL comments, especially since these are more complex than simple inserts.

Example in `soul_meta.rs`:
```rust
sqlx::query!(
    "INSERT INTO soul_meta ... ON CONFLICT(creator_id) DO UPDATE SET ...",
    ...
)
```

**Recommended fix:** Consider adding a brief SQL comment (e.g., `-- upsert: insert or update on creator_id conflict`) to clarify intent. Not critical since the behavior is test-covered.

---

## Positive Observations

### Correct Edge-Case Handling

1. **Transaction-bound queries** (`outbox.rs`): All `INSERT` operations inside `begin().await?` → `tx.commit().await?` blocks correctly use `sqlx::query!()` with `&mut *tx`.

2. **INSERT ON CONFLICT** (`soul_meta.rs`, `sqlite.rs`): Properly migrated with correct parameter ordering.

3. **Complex JOIN/subquery** (`outbox.rs` replay query): Correctly converted from tuple-returning `query_as` to struct-based `query_as!` with proper field mapping.

4. **Bool↔INTEGER conversion** (`identity.rs`): The `platform_linked != 0` pattern correctly converts SQLite INTEGER to Rust bool.

5. **Optional String handling** (`workspace_meta` queries): Correct use of `sqlx::query_scalar!` for nullable single-value returns.

### SAFETY Comment Quality

All runtime query exceptions are properly annotated with `// SAFETY:` comments explaining the specific reason compile-time macros cannot be used:

| Reason | Locations |
|--------|-----------|
| Dynamic WHERE clause (user input) | `research.rs:129`, `schedules.rs:166` |
| Pool reference from `supervisor.pool()` | `schedules.rs` (multiple handlers) |
| Test-only helpers | `auth_middleware.rs`, `schedules.rs`, `pool.rs` |
| DDL / PRAGMA | `lib.rs`, `pool.rs`, `db.rs` |

### Documentation

- **AGENTS.md section** (`## sqlx Compile-Time Macros (Mandatory)`): Clear, actionable, and covers:
  - Three macro types (`query!`, `query_as!`, `query_scalar!`)
  - Acceptable runtime query exceptions (DDL, PRAGMA, truly dynamic SQL)
  - Migration workflow with `cargo sqlx prepare`
  - CI enforcement explanation

- **`.gitignore`**: Correctly ignores `.sqlx/state.db*` (WAL and SHM files)

- **`.sqlx/sync-schema.sql`**: Documents the outbox schema for offline reference DB setup

### Consistency

- **Pre-owning pattern** (`schedule/supervisor.rs`, `schedule/derivation.rs`): Consistent use of `let xxx_owned = xxx.to_owned()` before macro calls to satisfy borrow checker.
- **SAFETY comment format**: Consistent `// SAFETY:` prefix with explanatory text.
- **Named vs positional params**: Consistent use of unnamed `?` throughout (no mixed `?1, ?2` patterns).

---

## Cross-Reviewer Notes (for QC1 & QC2)

Items that benefit from multi-reviewer verification:

1. **Runtime query exceptions**: All 40+ remaining `.bind()` calls (in tests and runtime query paths) are properly annotated with SAFETY comments explaining why compile-time macros aren't applicable.

2. **`.sqlx/` metadata integrity**: 97 new JSON query metadata files were generated. No drift between committed metadata and actual queries is expected since CI `verify-sqlx-offline` is re-enabled.

3. **AGENTS.md convention adoption**: The new convention is clearly documented. Future contributors will need to follow this; consider adding to PR reviewer checklist.

---

## Recommendation

**Approve.** The migration is complete and correct. The suggestions above are minor and do not block merge.

**Effort to fix suggestions (if addressed):** L (trivial — add 2–3 comments)

---
