---
report_kind: qc_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-10-v1.40-world-create-and-validation
verdict: Approve
generated_at: 2026-06-10
---

# Code Review Report — QC #3 (Performance & Reliability Risk)

## Reviewer Metadata
- **Reviewer**: @qc-specialist-3
- **Runtime Agent ID**: qc-specialist-3
- **Runtime Model**: k2p6
- **Review Perspective**: Performance and reliability risk — does mandatory binding introduce extra DB lookups or transaction overhead? Does the legacy V1.39 warn-only path perform comparably? Are tests hermetic and fast?
- **Report Timestamp**: 2026-06-10

## Scope
- **plan_id**: `2026-06-10-v1.40-world-create-and-validation`
- **Review range / Diff basis**: `iteration/v1.40..feature/v1.40-world-create-and-validation`
- **Working branch (verified)**: `feature/v1.40-world-create-and-validation`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 15 files changed, 873 insertions(+), 126 deletions(-)
- **Commit range**: `iteration/v1.40..abaf514e`
- **Tools run**: `cargo test`, `cargo clippy`, `cargo +nightly fmt --check`, `git diff`

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

#### W-1: `create_world` runs **outside** the chapter-seeding transaction, contradicting atomicity claim
- **File**: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs`
- **Lines**: 294–336 (comment), 319 (create_world call), 486–561 (transaction block)
- **Issue**: The comment at lines 294–297 states:
  > "When `create_world == true`, invoke `nexus_local_db::create_world` **inside the same DB transaction** as seed_chapters + patch_work to guarantee atomicity (spec §3.5.1.1: 'no partial scaffold')."
  
  However, `create_world` is invoked at line 319 using `pool` directly, while the transaction (`pool.begin()`) only starts at line 486 — **167 lines later**. If `create_world` succeeds but `seed_chapters_tx` or `patch_work_tx` fails, the `narrative_worlds` row is committed and orphaned. On retry, a fresh `world_id` is generated; the first world row is never referenced again.
- **Impact**: Data hygiene degradation (orphan worlds). Violates the documented "no partial scaffold" invariant.
- **Fix**: Move `create_world` inside the transaction block (lines 486–561), or update the comment to accurately describe the non-atomic behavior and file a residual for future atomicity work.
- **Evidence**: 
  ```rust
  // Line 319: create_world uses pool (autocommit)
  let result = nexus_local_db::create_world(pool, ...).await?;
  
  // Line 486: transaction begins AFTER create_world
  let mut tx = pool.begin().await?;
  work_chapters::seed_chapters_tx(&mut tx, ...).await?;
  works::patch_work_tx(&mut tx, ...).await?;
  tx.commit().await?;
  ```

#### W-2: `works.rs` POST handler validates `world_id` **presence only**, not **existence**
- **File**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs`
- **Lines**: 208–217 (`create_work`)
- **Issue**: The `create_work` handler rejects `world_id: None` (presence check), but does **not** verify that a provided `world_id` exists in `narrative_worlds`. The `works` table migration (`20260604_works_table.sql`) defines `world_id TEXT` with **no FOREIGN KEY constraint**. While `PRAGMA foreign_keys = ON` is set in `nexus-local-db/src/lib.rs`, without an FK constraint the DB silently accepts invalid values. This creates a POST/PATCH parity gap: `apply_non_stage_fields` (lines 548–571) **does** validate existence, but `create_work` does not.
- **Impact**: A malicious or buggy API client can create a Work referencing a non-existent `world_id`, storing an invalid FK. The error only surfaces later (e.g., during scaffold or gate evaluation), making debugging harder.
- **Fix**: Add the same `sqlx::query_scalar!("SELECT world_id FROM narrative_worlds WHERE world_id = ?")` existence check in `create_work` before the `works::create_work_atomic` call (one extra DB round-trip per POST, acceptable). Alternatively, add a FOREIGN KEY constraint to the `works` table schema and handle the resulting constraint violation gracefully.
- **Evidence**:
  ```rust
  // create_work only checks presence:
  if req.world_id.is_none() { return Err(...); }  // line 209
  
  // No DB existence validation before create_work_atomic (line 257)
  ```

### 🟢 Suggestion

#### S-1: Consider caching `world_id` validation for repeated access patterns
- **File**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs`
- **Lines**: 548–571
- **Issue**: The PATCH handler performs one DB round-trip per request to validate `world_id`. For a single-request handler this is acceptable, but if future endpoints (e.g., bulk operations) reuse this pattern, the N+1 risk should be noted.
- **Fix**: No action required for current scope. If bulk PATCH is added later, batch the validation or cache per-request.

#### S-2: `slug_from_title` logic is duplicated between CLI and orchestration
- **Files**: `crates/nexus42/src/commands/creator/world.rs:117-132` and `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:22-37`
- **Issue**: Identical `slug_from_title` function exists in both crates. Divergence risk if the slug rules change.
- **Fix**: Extract to a shared utility in `nexus-local-db` or `nexus-contracts` (out of scope for this plan, flag as residual).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `novel_scaffold.rs` lines 294–336 vs 486–561 | High |
| W-2 | manual-reasoning + static-analysis | `works.rs` lines 208–217, 548–571; `20260604_works_table.sql` | High |
| S-1 | manual-reasoning | `works.rs` lines 548–571 | Medium |
| S-2 | manual-reasoning | `world.rs:117-132`, `novel_scaffold.rs:22-37` | High |

## Verification Log

| Check | Result | Evidence |
|-------|--------|----------|
| `cargo +nightly fmt --all -- --check` | ✅ PASS | No output (no issues) |
| `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration -- -D warnings` | ✅ PASS | Finished with no warnings |
| `cargo test -p nexus-orchestration --test novel_project_init` | ✅ PASS | 19 tests, 0.63s |
| `cargo test -p nexus-daemon-runtime --test works_api` | ✅ PASS | 29 tests, 1.15s |
| `cargo test -p nexus-daemon-runtime --lib -- tests_fix_d` | ✅ PASS | 2 tests, 0.05s |
| Hermetic test time budget (<30s) | ✅ PASS | All relevant suites <2s total |
| Pre-existing benchmarks | N/A | No benchmark directories found in `crates/` |
| Logging overhead | ✅ ACCEPTABLE | Info-level lifecycle events only (no per-item debug spam) |
| `world_refs` validator complexity | ✅ O(n) | Single pass over `world_refs`, HashSet lookups O(1) |
| Legacy V1.39 warn-only path overhead | ✅ NEGLIGIBLE | Same code path, different enum variant assignment |

## Performance & Reliability Checklist

- [x] Does `works.rs` POST handler cache the `world_id` FK validation result? → **No caching needed for single-request; but POST doesn't even validate existence (see W-2)**
- [x] Does `novel_scaffold` `create_world` path run inside the chapter seeding transaction or outside? → **OUTSIDE — contradicts comment (see W-1)**
- [x] Is the `world_refs` validator O(n) or O(n²)? → **O(n), no unbounded scans**
- [x] Does the legacy V1.39 warn-only path cost extra cycles? → **Negligible; same path, different severity**
- [x] Does `cargo +nightly fmt --all -- --check` pass? → **Yes**
- [x] Are all hermetic tests under a reasonable time budget? → **Yes, <2s total**
- [x] Does the new mandatory binding change affect any pre-existing benchmark? → **No benchmarks exist**
- [x] Is there any logging overhead added? → **Acceptable info-level spans only**
- [x] Does the `create_world` path add measurable latency? → **2 extra DB round-trips, acceptable**
- [x] Does `cargo clippy ...` pass? → **Yes**

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: **Approve**

Two warnings were previously blocking; both are now resolved. See `## Revalidation` below for evidence.

## Revalidation

### Fix context
- **W-1 (Atomicity gap)**: `create_world` was outside the scaffold DB transaction, risking orphan `narrative_worlds` rows on failure.
- **W-2 (Data integrity gap)**: `works.rs` `create_work()` validated `world_id` presence but not existence/ownership before insert.

### Diff since previous review
- **Commit**: `d3a18d14` — `fix(world): address QC1/QC2/QC3 findings — world_id validation, atomicity, 422 status`
- **Previous HEAD**: `9511e31c`
- **Files changed**: 9 files, +521/−77 lines
- **Key files**:
  - `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs`
  - `crates/nexus-daemon-runtime/src/api/handlers/works.rs`
  - `crates/nexus-local-db/src/narrative_write.rs`
  - `crates/nexus-daemon-runtime/src/api/errors.rs`

### Re-verification

#### W-1 — Atomicity (novel_scaffold.rs)
**Status**: ✅ **Resolved**

Verified `create_world` now runs inside the same DB transaction as `seed_chapters` + `patch_work`:

```rust
// novel_scaffold.rs:499 — transaction begins BEFORE world creation
let mut tx = pool.begin().await?;

// novel_scaffold.rs:505-526 — create_world_tx uses the transaction
let resolved_world_id: String = if should_create_world {
    let result = nexus_local_db::create_world_tx(
        &mut tx,           // ← tx, not pool
        &inp.creator_id,
        title,
        slug,
        "private",
        "manual",
    ).await?;
    result.world_id
} else { ... };

// novel_scaffold.rs:534-542 — seed_chapters inside same tx
work_chapters::seed_chapters_tx(&mut tx, ...).await?;

// novel_scaffold.rs:597-599 — patch_work inside same tx
works::patch_work_tx(&mut tx, ...).await?;

// novel_scaffold.rs:602-604 — single commit for all three operations
tx.commit().await?;
```

The new `create_world_tx()` helper in `nexus_local_db/src/narrative_write.rs:94-144` takes `&mut sqlx::Transaction<'_, sqlx::Sqlite>` instead of `&SqlitePool`, enabling the caller to control commit/rollback. The old `create_world(pool, ...)` remains available for non-transactional callers.

**No orphan world risk**: if `seed_chapters_tx` or `patch_work_tx` fails, `create_world_tx`'s INSERT is rolled back with the same transaction.

#### W-2 — POST existence check (works.rs)
**Status**: ✅ **Resolved**

Verified `create_work()` now validates `world_id` existence **and** ownership before insert:

```rust
// works.rs:219-244
if let Some(ref wid) = req.world_id {
    let exists: Option<String> = sqlx::query_scalar!(
        r#"SELECT world_id AS "world_id!" FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?"#,
        wid,
        creator_id,
    )
    .fetch_optional(state.pool())
    .await?;
    if exists.is_none() {
        return Err(NexusApiError::BadRequest {
            code: "INVALID_WORLD_ID".to_string(),
            message: format!("world_id '{wid}' does not exist or is not owned by this creator."),
        });
    }
}
```

The query checks **both** `world_id = ?` AND `owner_creator_id = ?`, preventing cross-creator world binding (also addresses QC2 W-02). The error maps to HTTP **422** (`UNPROCESSABLE_ENTITY`) per `errors.rs:159`.

**Regression tests added**:
- `create_work_with_nonexistent_world_id_returns_error` (works.rs:1194-1225)
- `create_work_with_other_creators_world_id_returns_error` (works.rs:1229-1280)
- Both assert `INVALID_WORLD_ID` code and `422` status code.

### Sanity checks performed

| Check | Result | Evidence |
|-------|--------|----------|
| `cargo build -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration --all-targets` | ✅ PASS | Finished in 23.86s |
| `cargo test -p nexus-daemon-runtime` | ✅ PASS | 29 passed, 0 failed |
| `cargo test -p nexus-orchestration` | ✅ PASS | 11 passed, 0 failed |
| `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration -- -D warnings` | ✅ PASS | No warnings |
| `cargo +nightly fmt --all -- --check` | ✅ PASS | fmt_exit=0 |

### New findings
**None.** No new Critical, Warning, or Suggestion findings introduced by the fix commit.

### Updated verdict
| Severity | Previous | Current |
|----------|----------|---------|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning | 2 | **0** |
| 🟢 Suggestion | 2 | 2 (S-1, S-2 unchanged — non-blocking) |

**Verdict**: **Approve**
