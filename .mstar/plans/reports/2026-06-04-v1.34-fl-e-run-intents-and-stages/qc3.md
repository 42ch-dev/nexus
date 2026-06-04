---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-04-v1.34-fl-e-run-intents-and-stages"
verdict: "Request Changes"
generated_at: "2026-06-05"
---

# Code Review Report — QC #3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-05T00:00:00Z

## Scope
- plan_id: 2026-06-04-v1.34-fl-e-run-intents-and-stages
- Review range / Diff basis: merge-base: origin/main..HEAD on feature/v1.34-fl-e-run-intents-and-stages; 3 P1 commits:
  - `655d71c` T1 (works stage columns + DDL)
  - `d379f86` T2+T4 (CLI stage + status)
  - `e0e1861` T3 (stage→preset allowlist)
- Working branch (verified): feature/v1.34-fl-e-run-intents-and-stages
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages
- Files reviewed: 8
- Commit range: 655d71c^..e0e1861
- Tools run: cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db

## Findings

### 🔴 Critical

#### R-01: Spec §2 invariant #4 "At most one active FL-E stage schedule per Work" is not implemented

**Source**: `crates/nexus42/src/commands/creator/run.rs` (T2 commit), `crates/nexus-local-db/src/works.rs` (T1 commit)

**Issue**: The plan's primary spec (creator-workflow-fl-e §2) defines invariant #4: "At most one active FL-E stage schedule per Work." After reviewing all 3 commits, there is **no implementation** of this invariant anywhere in the codebase:

- No database constraint or unique index on `(work_id, stage_status='active')`
- No transaction-level check in `update_work_stage()` or `patch_work()` that prevents setting `stage_status='active'` when another schedule for the same work is already active
- No daemon API middleware or handler that enforces this
- The CLI `stage_advance` simply PATCHes `stage_status: "active"` without checking for existing active schedules

**Impact**: Under concurrent access (daemon scheduling + manual CLI `stage advance`), a single Work could end up with multiple active stage schedules, violating the spec invariant and causing unpredictable orchestration behavior.

**Fix**: Add either (a) a database-level check/constraint, or (b) an explicit transaction in `update_work_stage()` that queries for existing active schedules on the same work and rejects if found. Given SQLite's limited partial unique index support, a transaction-level guard is more practical:

```rust
BEGIN;
SELECT COUNT(*) FROM works WHERE work_id = ? AND stage_status = 'active';
-- if > 0, ROLLBACK with error
UPDATE works SET current_stage = ?, stage_status = 'active', updated_at = ? WHERE work_id = ?;
COMMIT;
```

---

#### R-02: Stage advance has read-modify-write race condition (TOCTOU)

**Source**: `crates/nexus42/src/commands/creator/run.rs:stage_advance()` (T2 commit)

**Issue**: The `stage_advance()` function performs a classic read-modify-write sequence:

1. `GET /v1/local/works/{work_id}` → reads `current_stage` and `stage_status`
2. Local validation (order check, completion check)
3. `PATCH /v1/local/works/{work_id}` → writes new stage

Between step 1 and step 3, another process (daemon scheduler, another CLI instance) could modify the work's stage, rendering the validation stale. The `--force` flag only skips validation but does not address the race.

**Impact**: Two concurrent `stage advance` calls, or a daemon auto-scheduler + manual CLI, could:
- Skip over stages (race between two advances)
- Advance from a stage that was just changed by another process
- Violate the linear stage order guarantee

**Fix**: The PATCH endpoint should support conditional updates based on expected current stage/status (optimistic locking), or the daemon should provide an atomic "advance stage" endpoint that performs validation + update in a single transaction.

A lightweight fix: add `expected_current_stage` and `expected_stage_status` fields to `PatchWorkRequest`; the handler rejects with `409 Conflict` if the actual state differs.

---

### 🟡 Warning

#### R-03: DDL migration uses two separate ALTER TABLE statements without `IF NOT EXISTS`

**Source**: `crates/nexus-local-db/migrations/20260606_works_stage_columns.sql` (T1 commit)

**Issue**: The migration adds two columns with two separate `ALTER TABLE` statements:

```sql
ALTER TABLE works ADD COLUMN current_stage TEXT NOT NULL DEFAULT 'intake' ...;
ALTER TABLE works ADD COLUMN stage_status TEXT NOT NULL DEFAULT 'pending' ...;
```

While sqlx's migration tracking prevents double-running, if a user manually restores a backup or copies a `.db` file with inconsistent state, re-running migrations could fail with "duplicate column name." SQLite does not support `IF NOT EXISTS` on `ADD COLUMN`, but the migration could be wrapped in a defensive check.

**Impact**: Low for normal operation (sqlx tracks `_sqlx_migrations` table), but medium for disaster recovery or manual DB manipulation.

**Fix**: Wrap each ALTER in a `PRAGMA table_info(works)` check, or document that migrations must only be run via `sqlx migrate` and never manually.

---

#### R-04: `get_work_stage()` is dead code (unused outside tests)

**Source**: `crates/nexus-local-db/src/works.rs` (T1 commit)

**Issue**: `get_work_stage()` performs a lightweight `SELECT current_stage, stage_status` query, presumably to avoid the overhead of `get_work()` (which fetches all 16+ columns). However, it is **not used** by any production code:

- CLI `stage_list()` uses `get_work()` via the daemon API
- CLI `stage_advance()` uses `get_work()` via the daemon API
- No daemon handler calls `get_work_stage()` directly

**Impact**: Dead code adds maintenance burden and may mislead future developers into thinking there's an optimized path. If the intent was performance (avoiding full row fetches), that optimization was never realized.

**Fix**: Either (a) use `get_work_stage()` in `stage_advance()` to reduce data transfer, or (b) remove it and its tests to reduce surface area. Given the race condition in R-02, option (a) alone is insufficient; the real fix is an atomic advance endpoint.

---

#### R-05: CLI `stage_advance()` hardcodes preset hints independently of orchestration allowlist

**Source**: `crates/nexus42/src/commands/creator/run.rs:stage_advance()` (T2 commit)

**Issue**: The CLI displays a "typical preset" hint after advancing:

```rust
let preset_hint = match target_stage {
    "intake" => "creative-brief-intake",
    "research" => "research",
    // ...
};
```

This mapping duplicates `STAGE_PRESET_ALLOWLIST` in `crates/nexus-orchestration/src/preset/validation.rs` (T3 commit). If the allowlist changes, the CLI hint will be out of sync.

**Impact**: User confusion if preset names change or new presets are added to the allowlist.

**Fix**: Export `default_preset_for_stage()` from `nexus-orchestration` and consume it in the CLI, or expose the allowlist via a daemon API endpoint that the CLI queries.

---

#### R-06: No audit logging for stage transitions

**Source**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs`, `crates/nexus42/src/commands/creator/run.rs`

**Issue**: Stage transitions (especially `--force` overrides) are not logged to any audit or structured log:

- `patch_work` handler has no `tracing::info!` for stage/status changes
- CLI `stage_advance()` only prints to stdout via `println!()` — not captured by daemon logs
- No distinction between "wrong-order rejected" (validation failure) and "--force accepted" (override) in any persistent log

**Impact**: Operational blind spot. If a work enters an unexpected stage, there is no trail to determine whether it was a daemon scheduler, a user CLI command, or a `--force` override.

**Fix**: Add `tracing::info!` in `patch_work` handler when `current_stage` or `stage_status` changes, including old→new values. Add `tracing::warn!` for `--force` overrides in the CLI (if CLI logs are forwarded) or better, move the force flag to the daemon API so it can be logged centrally.

---

### 🟢 Suggestion

#### R-07: `validate_preset_for_stage()` uses linear scan instead of constant-time lookup

**Source**: `crates/nexus-orchestration/src/preset/validation.rs` (T3 commit)

**Issue**: `allowed_presets_for_stage()` iterates over `STAGE_PRESET_ALLOWLIST` (5 items) with `.iter().find()`, then `validate_preset_for_stage()` does `.contains()` on the resulting slice (1 item per stage).

**Impact**: Negligible for 5 stages × 1 preset each, but if the allowlist grows to many presets per stage, the O(n×m) scan becomes unnecessary overhead.

**Fix**: Use a `phf::Map` or `lazy_static!` `HashMap` for O(1) stage→preset lookup. This is a 5-minute refactor with no behavior change.

---

#### R-08: Migration filename uses future date

**Source**: `crates/nexus-local-db/migrations/20260606_works_stage_columns.sql`

**Issue**: The migration file is named `20260606_...` (June 6, 2026), but the commit was authored on June 5, 2026. This is harmless but slightly confusing.

**Fix**: Rename to `20250605_...` or the actual creation date. sqlx migrations are ordered by filename, so this only matters if another migration is added between now and June 6.

---

#### R-09: `patch_work` could benefit from a dedicated `advance_stage` atomic operation

**Source**: Architecture (cross-cutting)

**Issue**: The current design uses the generic `PATCH /v1/local/works/{id}` endpoint for stage advances. This is flexible but prevents atomic validation-and-update semantics.

**Fix**: Consider adding a dedicated `POST /v1/local/works/{id}/advance-stage` endpoint in a future iteration. This would centralize validation logic, eliminate the CLI-side race condition, and provide a single hook for audit logging.

---

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| R-01 | manual-reasoning | `works.rs`, `run.rs`, spec §2 | High |
| R-02 | manual-reasoning | `run.rs:stage_advance()` | High |
| R-03 | manual-reasoning | `20260606_works_stage_columns.sql` | Medium |
| R-04 | static-analysis | `works.rs:get_work_stage()` callers (none) | High |
| R-05 | manual-reasoning | `run.rs` vs `validation.rs` | High |
| R-06 | manual-reasoning | `works.rs` handlers, `run.rs` output | High |
| R-07 | static-analysis | `validation.rs:allowed_presets_for_stage()` | High |
| R-08 | manual-reasoning | Migration filename | High |
| R-09 | manual-reasoning | Architecture review | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 3 |

### Test Results
```
cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db
=> ALL PASSED (0 failures)
```

All unit and integration tests pass, including 7 new local-db stage tests, 4 new daemon API stage tests, and 7 new orchestration preset validation tests. However, **tests do not cover concurrent access or the "at most one active schedule" invariant**, which are runtime reliability concerns that unit tests alone cannot catch.

### Key Reliability Gaps

1. **Missing invariant enforcement**: The spec's "at most one active schedule" guarantee is entirely unimplemented.
2. **Race condition**: CLI-stage-advance is vulnerable to TOCTOU races between validation and PATCH.
3. **No audit trail**: Stage transitions (especially forced ones) leave no persistent log.

### Performance Assessment

- DDL migration: Two `ALTER TABLE` statements with `DEFAULT` values are safe for SQLite (rewrites table but single-transaction). For >1k rows, estimate <100ms on modern SSD.
- `get_work_stage()`: Dead code; no production impact.
- `validate_preset_for_stage()`: O(n×m) with n=5, m=1 — effectively constant time. No production concern.

**Verdict**: Request Changes

The two Critical findings (R-01 missing invariant, R-02 race condition) are reliability defects that violate the spec and create data consistency risks under concurrency. These must be addressed before merge. The Warnings (R-03 through R-06) should be fixed or explicitly deferred with residual tracking. Suggestions are optional.
