---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-04-v1.34-fl-e-run-intents-and-stages"
verdict: "Approve w/ residuals"
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

---

## Revalidation

**Fix wave 2 commits revalidated**: `c3834ce..6cd1409` (8 commits)  
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages`  
**Working branch**: `feature/v1.34-fl-e-run-intents-and-stages`  
**Revalidation date**: 2026-06-05

### Evidence

```
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages

$ git branch --show-current
feature/v1.34-fl-e-run-intents-and-stages

$ git log --oneline -10
6cd1409 fix(fl-e): R-FL-E-07 PATCH stage wrapped in atomic transaction (TOCTOU-safe)
03dbfa5 fix(fl-e): R-FL-E-06 persist allowlist dual-path kb-extract + memory-review
34fda67 fix(fl-e): R-FL-E-05 PATCH stage uses shared gates with CLI
991e2f8 fix(fl-e): R-FL-E-04 audit log on --force stage skip
f7f0b59 fix(fl-e): R-FL-E-01 stage advance creates schedule + active protection
bcf3563 fix(fl-e): R-FL-E-03 strict linear gate without --force
e80db53 fix(fl-e): R-FL-E-02 intake gate uses intake_status not stage_status
c3834ce fix(fl-e): R-FL-E-08 dedup FL_E_STAGES constant to single source in nexus-contracts
```

### Critical Findings Revalidation

#### R-01: Spec §2 invariant #4 "At most one active FL-E stage schedule per Work"

**Status**: ✅ **RESOLVED**

**Evidence**:
- Commit `f7f0b59` adds `has_active_fl_e_schedule()` (`crates/nexus-local-db/src/works.rs:818-834`) — lightweight `SELECT stage_status` guard that returns `true` when `stage_status = 'active'`.
- Commit `6cd1409` adds `advance_work_stage_atomic()` (`works.rs:848-931`) — wraps read-check-update in a single `sqlx::Transaction`:
  1. `SELECT` current work state inside transaction
  2. Explicit check: `if current.stage_status == "active" && target_status == "active"` → returns `ConstraintViolation` error
  3. `UPDATE works SET current_stage = ?, stage_status = ?` inside same transaction
  4. `tx.commit().await?`
- The shared `stage_gates::check_stage_advance()` (Gate 4, `crates/nexus-orchestration/src/stage_gates.rs:91-99`) also rejects advance when `stage_status == "active"` for non-force paths.
- Both CLI and daemon paths enforce this invariant: CLI via `stage_gates` pre-check, daemon via atomic transaction check.

**Reliability assessment**: The transaction-level guard is the correct fix for SQLite (no partial unique index support). Two concurrent `advance` calls will be serialized by SQLite's single-writer lock; the second transaction will see the first's `active` status and reject with `ConstraintViolation`. The `pool.begin()` call defaults to `BEGIN DEFERRED` in sqlx, but because the UPDATE upgrades the lock and SQLite serializes writers, the TOCTOU window is closed.

#### R-02: Stage advance TOCTOU race condition

**Status**: ✅ **RESOLVED**

**Evidence**:
- Commit `6cd1409` replaces the read-modify-write sequence with `advance_work_stage_atomic()`:
  - Before: CLI read work → validate → PATCH work (3 separate round-trips, race window between read and PATCH)
  - After: Daemon `patch_work_stage()` calls `advance_work_stage_atomic()` which does `SELECT ... → check → UPDATE` in one transaction
- The CLI `stage_advance()` (`run.rs:500-645`) now delegates to the daemon PATCH endpoint, so the atomic path is used for all stage advances.
- `ConstraintViolation` from the DB is mapped to `NexusApiError::Conflict(409)` in the daemon handler (`works.rs:388-390`), giving the client a clear retry signal.

**Reliability assessment**: The race is eliminated at the database transaction level. The only remaining gap is the `get_work()` call after `tx.commit()` to return the updated record (line 926-930), which is outside the transaction but only affects the response payload, not the persisted state. Acceptable.

### Warning Findings Revalidation

#### R-03: DDL migration uses two separate ALTER TABLE statements without defensive check

**Status**: ⚠️ **PARTIALLY ADDRESSED — Residual**

**Evidence**:
- Migration file `20260606_works_stage_columns.sql` unchanged — still two `ALTER TABLE ADD COLUMN` statements without `IF NOT EXISTS` equivalent.
- No `PRAGMA table_info(works)` defensive check added.
- Under normal operation, sqlx's `_sqlx_migrations` table prevents double-running.

**Assessment**: The disaster-recovery / manual-DB-manipulation scenario remains unhandled. For a pre-release project (v < 1.0), this is acceptable but should be tracked. Risk: LOW (requires manual migration bypass).

#### R-04: `get_work_stage()` is dead code (unused outside tests)

**Status**: ⚠️ **UNRESOLVED — Residual**

**Evidence**:
- `get_work_stage()` (`works.rs:789-804`) is still defined but has **zero production callers**:
  - Not used in `nexus42` CLI
  - Not used in `nexus-daemon-runtime`
  - Not used in `nexus-orchestration`
  - Only referenced in 2 unit tests (`test_get_work_stage`, `test_get_work_stage_nonexistent`)
- The fix wave did not implement either recommendation (a) use it in `stage_advance()` to reduce data transfer, or (b) remove it.

**Assessment**: Dead code adds maintenance burden but no runtime risk. Should be cleaned up in a future iteration.

#### R-05: CLI `stage_advance()` hardcodes preset hints independently of orchestration allowlist

**Status**: ✅ **RESOLVED**

**Evidence**:
- Commit `c3834ce` deduplicates `FL_E_STAGES` to `nexus-contracts` single source of truth.
- Commit `34fda67` / `03dbfa5` — CLI now imports and uses `default_preset_for_stage()` from `nexus-orchestration::preset::validation` (`run.rs:14`, `run.rs:560`).
- The hardcoded `match target_stage { ... }` block from the original implementation is removed.
- `default_preset_for_stage()` is backed by `STAGE_PRESET_ALLOWLIST` (validation.rs:1530-1537), so changes to the allowlist automatically propagate to CLI hints.

**Assessment**: Deduplication complete. No risk of CLI hint / allowlist drift.

#### R-06: No audit logging for stage transitions

**Status**: ✅ **RESOLVED**

**Evidence**:
- Commit `991e2f8` adds `tracing::info!(target: "fl_e.audit", ...)` to daemon `patch_work` handler.
- Commit `6cd1409` refines this to `patch_work_stage()` (`works.rs:397-403`) with the message "FL-E stage updated via PATCH (atomic)".
- CLI `stage_advance()` (`run.rs:545-555`) logs force-override events to the same `fl_e.audit` target with structured fields: `work_id`, `from_stage`, `to_stage`, `from_status`, `force = true`.
- Both daemon and CLI use the **same structured log target** (`fl_e.audit`), enabling centralized filtering.

**Assessment**: Audit trail is now complete and structured. Operational visibility restored.

### Suggestion Findings Revalidation

#### R-07: `validate_preset_for_stage()` uses linear scan instead of constant-time lookup

**Status**: ⚠️ **UNRESOLVED — Residual**

**Evidence**:
- `allowed_presets_for_stage()` (validation.rs:1554-1558) still uses `STAGE_PRESET_ALLOWLIST.iter().find(...)` (O(n) where n=5).
- No `phf::Map`, `lazy_static! HashMap`, or equivalent constant-time structure added.

**Assessment**: With n=5 stages, the scan is ~5 pointer comparisons — effectively free. If the allowlist grows beyond ~50 entries per stage, this should be revisited. Not a blocker for v1.34.

#### R-08: Migration filename uses future date

**Status**: ⚠️ **UNRESOLVED — Residual**

**Evidence**:
- Migration file is still named `20260606_works_stage_columns.sql` (June 6, 2026).
- Current date is June 5, 2026.
- sqlx migrations are ordered lexicographically by filename; this only matters if another migration with a date between June 5 and June 6 is added.

**Assessment**: Harmless but confusing. Should be renamed to actual creation date in a cleanup pass.

#### R-09: `patch_work` could benefit from a dedicated `advance_stage` atomic operation

**Status**: ⚠️ **UNRESOLVED — Residual**

**Evidence**:
- No `POST /v1/local/works/{id}/advance-stage` endpoint added.
- The daemon still uses the generic `PATCH /v1/local/works/{id}` endpoint for stage changes, with stage-specific logic branching inside `patch_work_stage()`.
- The atomic semantics are achieved via `advance_work_stage_atomic()` in the DB layer, but the API surface remains generic.

**Assessment**: The functional requirement (atomic advance) is met. A dedicated endpoint would improve API clarity and centralize validation, but is architectural debt rather than a functional gap. Defer to v1.35+ if desired.

### Performance Assessment (Re-evaluation)

| Area | Before | After | Assessment |
|------|--------|-------|------------|
| DDL migration | Two `ALTER TABLE` with `DEFAULT` | Unchanged | For existing large `works` tables: SQLite rewrites the entire table per `ALTER TABLE`. Two ALTERs = two full rewrites. With 10k rows ~200-300ms total. Acceptable for one-time migration. |
| Stage advance round-trip | 3 HTTP calls (GET + validation + PATCH) | 1 HTTP call (PATCH with atomic DB tx) | ✅ Eliminated extra round-trip. The atomic function fetches state inside the transaction, removing the CLI→daemon read-before-write. |
| `get_work_stage()` | Dead code | Still dead code | No production impact. |
| Preset validation | O(n) scan, n=5 | O(n) scan, n=5 | Negligible. |

### Reliability Assessment (Re-evaluation)

| Concern | Status | Evidence |
|---------|--------|----------|
| Cross-stage order validation + `stage_status` race | ✅ Mitigated | `advance_work_stage_atomic()` serializes check+update in one DB transaction. SQLite single-writer ensures no interleaving. |
| DDL migration failure at daemon startup | ⚠️ Partial | sqlx migrate runs at startup. If migration fails (e.g., duplicate column from manual restore), daemon panics with sqlx error. No graceful degradation. Residual tracked. |
| Audit log structure | ✅ Structured | `target: "fl_e.audit"` with key-value pairs (`work_id`, `current_stage`, `stage_status`, `force`). Filterable by log aggregators. |

### Test Results

```
$ cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db
=> ALL PASSED (0 failures)

Summary:
- nexus_local_db: 172 passed
- nexus_orchestration: 21 passed
- nexus_daemon_runtime: 18 passed
- nexus42: 117 passed
- Doc-tests: all passed
Total: 328+ tests passed, 0 failed
```

**New tests added in fix wave 2**:
- `test_has_active_fl_e_schedule_false_for_new_work`
- `test_has_active_fl_e_schedule_true_after_advance`
- `test_reject_double_active_schedule`
- Daemon API tests for shared gates (`works_api.rs`)
- `persist_allowlist_accepts_both_paths` (R-FL-E-06)

### Disposition Summary

| Finding | Severity | Status | Commit |
|---------|----------|--------|--------|
| R-01 active schedule uniqueness | Critical | ✅ Resolved | `f7f0b59` + `6cd1409` |
| R-02 TOCTOU race | Critical | ✅ Resolved | `6cd1409` |
| R-03 DDL migration lock | Warning | ⚠️ Residual | — |
| R-04 `get_work_stage()` dead code | Warning | ⚠️ Residual | — |
| R-05 Preset hints hardcoded | Warning | ✅ Resolved | `c3834ce` + `34fda67` |
| R-06 Audit logging | Warning | ✅ Resolved | `991e2f8` + `6cd1409` |
| R-07 Linear scan | Suggestion | ⚠️ Residual | — |
| R-08 Migration filename | Suggestion | ⚠️ Residual | — |
| R-09 Dedicated endpoint | Suggestion | ⚠️ Residual | — |

### Verdict Rationale

Both Critical findings (R-01, R-02) are fully resolved with transaction-level guards and atomic operations. The spec invariants are now enforced at the database layer. Two Warnings (R-05, R-06) are also resolved. The remaining Warnings (R-03, R-04) and Suggestions (R-07, R-08, R-09) are low-impact technical debt that do not affect runtime correctness or the core FL-E stage workflow. All tests pass. No new Critical issues introduced by the fix wave.

**Verdict**: `Approve w/ residuals`
