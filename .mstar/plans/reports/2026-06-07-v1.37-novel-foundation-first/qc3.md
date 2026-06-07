---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-07-v1.37-novel-foundation-first"
verdict: "Request Changes"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: performance + reliability risk
- Report Timestamp: 2026-06-08T12:00:00Z

## Scope
- plan_id: 2026-06-07-v1.37-novel-foundation-first
- Review range / Diff basis: merge-base(iteration/v1.37)..HEAD on feature/v1.37-novel-foundation-first
- Working branch (verified): feature/v1.37-novel-foundation-first
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 23 files changed, 1921 insertions(+), 126 deletions(-)
- Tools run: git diff, cargo +nightly fmt --check, cargo clippy, rg, cargo test (no-fail-fast)

## Findings

### 🔴 Critical

#### C-1: `force_gates_audit` table missing index on `forced_at` — full table scan on every audit listing
- **Issue**: `crates/nexus-local-db/migrations/202606080001_force_gates_audit.sql` creates `force_gates_audit` with only a PRIMARY KEY on `audit_id`. `crates/nexus-local-db/src/force_gates_audit.rs:56` queries `ORDER BY forced_at DESC` without an index. As an append-only audit log, every `list_force_gates_audit` call will eventually become a full table scan.
- **Fix**: Add `CREATE INDEX IF NOT EXISTS force_gates_audit_by_creator_forced_at ON force_gates_audit(creator_id, forced_at DESC);` to the migration.

#### C-2: `DbPreviousPresetLookup` uses unindexed `LIKE` on `creator_schedules.label` — table scan per `previous_preset` gate
- **Issue**: `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:1142-1148` queries `SELECT COUNT(*) FROM creator_schedules WHERE preset_id = ? AND status = 'completed' AND label LIKE ?`. The existing indexes are `(creator_id, status)`, `(status)`, and `(scheduled_at)`. None cover `preset_id + status + label`. The `LIKE '%work_id%'` pattern forces a full table scan on `creator_schedules` for every `previous_preset` gate evaluated.
- **Fix**: Either (a) add an index on `(preset_id, status, label)` (though `LIKE` with leading wildcard still can't use btree indexes effectively), or (b) redesign the lookup to use a normalized `work_id` column or join table instead of substring-matching `label`.

### 🟡 Warning

#### W-1: No retention policy for append-only `force_gates_audit` table
- **Issue**: The audit table grows without bound. There is no cleanup job, no retention policy, and no partition/tiering strategy. For a local SQLite DB this will eventually bloat the file and slow down all queries touching it.
- **Fix**: Document a retention policy (e.g., "keep 90 days") and either (a) add a scheduled cleanup task, or (b) add a `PRAGMA` or CLI command for manual purge. At minimum, register a residual finding for post-1.0.

#### W-2: Gate evaluation path in schedules.rs uses runtime `sqlx::query` instead of compile-time macros
- **Issue**: `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:218-224` uses `sqlx::query_as` with a `// SAFETY: runtime sqlx::query_as — dynamic column mapping.` comment. However, the query is completely static (`SELECT work_profile, work_ref, ... FROM works WHERE work_id = ? AND creator_id = ?`). `nexus-daemon-runtime/AGENTS.md` mandates compile-time macros for all static SQL. This bypasses sqlx's compile-time validation and will silently drift if the `works` schema changes.
- **Fix**: Convert to `sqlx::query_as!()` with a properly typed struct, or use `sqlx::query!()` and map columns explicitly. Add the `.sqlx/` metadata file.

#### W-3: `patch_work_tx` dirty-writes `updated_at` even when values are unchanged
- **Issue**: `crates/nexus-local-db/src/works.rs:717-828` (`patch_work_tx`) builds a dynamic UPDATE and always sets `updated_at = ?` whenever any field is `Some`. On idempotent re-runs of `novel-project-init`, the same values are written again, dirtifying the row and bumping `updated_at`. This makes it impossible to distinguish actual changes from no-op retries.
- **Fix**: Compare old vs new values before binding, or return a `bool` indicating whether any column actually changed. At minimum, document the idempotent-dirty behavior.

#### W-4: No integration test for normal (non-force-gates) gate evaluation path
- **Issue**: `crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs` has tests for `force_gates_writes_audit_row` and `force_gates_without_reason_is_rejected`, but no test that verifies gate evaluation actually blocks a schedule when gates fail. The only gate tests are unit tests in `nexus-orchestration` using mocks.
- **Fix**: Add an integration test in `fl_e_schedule_api.rs` that schedules a preset with gates against a Work that fails the gate, asserting `422 UNPROCESSABLE_ENTITY` with a structured `PresetGatesFailed` body.

#### W-5: Gate evaluation failures are not logged at warn/error level
- **Issue**: When `evaluate_gates` returns `Err(gate_failure)`, the handler serializes it to JSON and returns `422` (`schedules.rs:298-302`), but there is no `tracing::warn!` or `tracing::info!` logging the failure. This makes gate failure rates unobservable in production.
- **Fix**: Add `tracing::warn!(preset_id, work_id, failed_gates = ?gate_failure.failed_gates, "preset gates failed")` before returning the 422.

#### W-6: `cargo +nightly fmt --check` failure
- **Issue**: `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:270` has a formatting diff (closure argument alignment).
- **Fix**: Run `cargo +nightly fmt --all` and commit.

### 🟢 Suggestion

#### S-1: Batch filesystem gate syscalls
- **Observation**: If a preset declares 5 `filesystem` gates, `evaluate_gates` issues 5 separate `Path::exists()` syscalls. This is acceptable for pre-1.0 local CLI usage, but under daemon load it could be batched into a single directory listing.
- **Fix**: Low priority. Consider collecting all `filesystem` gates and doing one `std::fs::read_dir` per parent directory.

#### S-2: Document scaffold transaction limits
- **Observation**: `seed_chapters_tx` loops `total_planned_chapters` times, issuing one `INSERT OR IGNORE` per iteration inside a transaction. For `total_planned_chapters=100`, this is 100 DML statements. SQLite handles this, but the transaction hold time grows linearly. For pre-1.0 this is acceptable, but document the limit.
- **Fix**: Add a comment in `novel_scaffold.rs` noting the linear growth and referencing a future batch-INSERT optimization.

#### S-3: `.sqlx/` query hygiene is clean
- **Observation**: Only 1 new compile-time query metadata file added (`query-8ba7e7a8...json` for `force_gates_audit` INSERT). Two stale queries were removed. This is correct.

#### S-4: Backward compatibility for old preset YAMLs is handled correctly
- **Observation**: `PresetHeader.gates` uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. Preset YAMLs without a `gates:` field deserialize to `vec![]`, and `evaluate_gates` short-circuits because `!gates.is_empty()` is false. This is correct.

#### S-5: `AddScheduleRequest` contract fields have proper defaults
- **Observation**: `input`, `force_gates`, and `reason` all use `#[serde(default)]` or `#[serde(default, skip_serializing_if = "Option::is_none")]`. Old JSON payloads without these fields deserialize correctly. This is correct.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-1 | git-diff + manual-reasoning | `crates/nexus-local-db/migrations/202606080001_force_gates_audit.sql` | High |
| C-2 | git-diff + manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:1142` | High |
| W-1 | manual-reasoning | `crates/nexus-local-db/src/force_gates_audit.rs` (no cleanup) | High |
| W-2 | git-diff + linter-rule | `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:218` + AGENTS.md | High |
| W-3 | git-diff + manual-reasoning | `crates/nexus-local-db/src/works.rs:772` | Medium |
| W-4 | git-diff + test-coverage | `crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs` (missing test) | High |
| W-5 | git-diff + manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:298` | High |
| W-6 | linter | `cargo +nightly fmt --check` output | High |
| S-1 | manual-reasoning | `crates/nexus-orchestration/src/preset_gates.rs:208` | Medium |
| S-2 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs:105` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

The two Critical findings (missing indexes) directly impact query performance and will degrade as tables grow. The Warning findings include a sqlx compile-time macro violation (W-2), missing integration test coverage for the primary gate evaluation path (W-4), and unobservable gate failures (W-5). These must be addressed before approval.
