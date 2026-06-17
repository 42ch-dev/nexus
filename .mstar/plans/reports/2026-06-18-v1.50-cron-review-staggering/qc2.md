---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-18-v1.50-cron-review-staggering"
verdict: "Approve"
generated_at: "2026-06-17T13:59:28Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: security + correctness
- Report Timestamp: 2026-06-17T13:59:28Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-review-staggering
- Review range / Diff basis: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..44fe074408d7d5f571f50c4d91069d29f2b6c2b3
- Working branch (verified): feature/v1.50-cron-review-staggering
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-review-staggering
- Files reviewed: 5 (diff stat)
- Commit range: 4 commits (12495be8, b7e438b5, f211aced, 44fe0744)
- Tools run: git diff/log, cargo test -p nexus-orchestration --test cron_supervisor (22 passed), --test review_cron_e2e (2 passed), source inspection of cron_supervisor.rs / auto_chain.rs / schedule/supervisor.rs / works_schedule_migration.rs

## Reviewer-Specific Checks (qc-specialist-2 focus)

### 1. Review cron fires `novel-review-master` correctly; schedule label prefix unique (no collision with V1.39 SLA-launched review)
- `cron_supervisor.rs`: `ROLE_REVIEW`, `CronRoles.review`, `role_preset("review")` → `NOVEL_REVIEW_MASTER_PRESET_ID`.
- `try_fire_role` reuses the uniform `enqueue_cron_schedule` path for all three roles.
- Enqueued label: `format!("cron:{role}:{work_id}")` → `cron:review:<work_id>`.
- Schedule ID prefix: `CRON<ts><counter>` (distinct from V1.39 stale-findings path which uses `RVM<ts>` + label `auto-review-master: <work_id>`).
- Test `cron_fires_review_role_enqueues_review_master` asserts both the `CRON` prefix and exact `cron:review:...` label.
- **No collision risk.** The two provenance schemes are orthogonal and distinguishable in `creator_schedules`.

### 2. Per-Work gating: same as brainstorm/write; race-free?
- Shared `gate_reason(row)` (called from `evaluate_work` before any role-specific logic):
  - `intake_status != "complete"` → "intake_incomplete"
  - `runtime_lock_holder.is_some()` → "runtime_locked"
  - `completion_locked_at.is_some()` → "completion_locked"
- All three roles (including `review`) go through the identical gate check in the same loop.
- Gating tests (`cron_review_respects_per_work_gating`, plus the three pre-existing for brainstorm/write) cover all three conditions.
- No TOCTOU in the evaluator itself (single-tick snapshot + DB-level idempotency guard below). The existing supervisor admission and runtime lock already serialize execution.
- **Race-free at the cron-fire decision point.** Same contract as T-A P1.

### 3. Idempotency: re-fire of same cron minute — does it dedup correctly?
- Shared `has_active_role_schedule(pool, work_id, preset_id)`:
  ```sql
  SELECT COUNT(*) FROM creator_schedules
  WHERE work_id = ? AND preset_id = ? AND status IN ('pending','running','paused')
  ```
- `review` uses the exact same preset (`novel-review-master`), so the guard applies identically.
- Test `cron_review_respects_idempotency` seeds an active `novel-review-master` row and asserts `fired=0`, `skipped_idempotent=1`.
- Additional test `cron_idempotent_skip_second_fire_same_minute` (pre-existing) and `cron_refires_after_prior_schedule_terminal` cover the lifecycle.
- **Correct dedup.** Re-fire of the same minute (or while a prior review-master is active) is suppressed.

### 4. T-B P1 hook (`extract_kb_candidates_for_review`) fires on completion; cron-launched path correctly triggers it
- The hook lives in `schedule::supervisor::on_schedule_terminal` and is keyed purely on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID` (not on label or provenance).
- Cron path produces `preset_id = "novel-review-master"` → hook is reached.
- End-to-end test `review_cron_e2e::review_cron_fire_triggers_kb_extraction_hook`:
  - Seeds Work + review cron config
  - Fires cron → enqueues schedule
  - Simulates terminal by UPDATE
  - Asserts `kb_extract_jobs` rows with `promotion_status='pending'` are inserted
- Negative leg (`review_cron_no_review_role_enqueues_nothing`) also present.
- **Chain is complete.** Cron-launched review now exercises the T-B P1 hook that previously only fired for stale-findings / manual `creator run` paths.

### 5. R-V150P2CRONRV-01 (migration renumber) — file-name change matches migration_id inside SQL; safe for dev DBs?
- The change is a pure `git mv` of the *T-B P1* migration:
  - `202606180002_kb_extract_jobs_extend.sql` → `202606180003_kb_extract_jobs_extend.sql`
- The SQL file contains **no** embedded migration identifier, pragma, or `CREATE TABLE` that would carry an internal ID. The authoritative identifier for sqlx is the **filename** recorded in `_sqlx_migrations`.
- Why the renumber occurred (from plan + diff):
  - T-A P1 introduced `202606180002_works_schedule_json_partial_idx.sql` (the partial index for the new `schedule_json` scan).
  - T-B P1's `kb_extract_jobs` extension originally landed as 002 in the same cycle → intra-plan filename collision.
  - Resolution (this range): keep the partial-idx migration as 002; give kb one 003.
- Companion fix (R-V150P2CRONRV-02): `works_schedule_migration.rs` rollback simulation now drops the partial index *before* attempting `DROP COLUMN schedule_json` (SQLite requirement).
- Safety for dev DBs:
  - Pre-release (V1.50 < 1.0). No user DBs have a released binary that applied "202606180002" for the kb content.
  - A dev DB that had previously run the old-named file in a local checkout would see the migration as "already applied" under the old name. After this rename the file is new (003); sqlx will apply it on next `run_migrations`.
  - No data loss or duplicate schema change because the *content* never changed — only the filename used for tracking.
- **Correct and safe.** The renumber resolves the documented collision without altering semantics.

### 6. Stable error codes; test coverage
- No new error types or `thiserror` variants introduced in the changed paths. All failure modes remain the pre-existing `warn!` + summary counters (`skipped_*`) or `AutoChainError` / `SupervisorError` already used by brainstorm/write.
- Coverage:
  - `cron_supervisor` tests: **22 passed** (18 from T-A P1 + 4 new review-specific: fire+provenance, gating, idempotency, graceful-skip).
  - `review_cron_e2e`: **2 passed** (full chain + negative "no review role").
  - The 4 new review tests + the e2e directly exercise the reviewer checklist items above.
- **Sufficient.** The critical paths (fire, gate, idempotency, provenance, hook trigger) are covered by hermetic tests that run in CI.

## Findings
### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- (minor) The test `cron_review_respects_idempotency` seeds a schedule with label `'preexisting'` and a hard-coded schedule_id. Consider using a more descriptive label or the real `cron:review:...` shape in future for readability; not a correctness issue.

## Source Trace
- Finding ID: N/A (no blocking findings)
- Source Type: manual code review + test execution + spec cross-check
- Source Reference: `cron_supervisor.rs:203-208` (role iteration), `280` (enqueue), `154-159` (label), `510-525` (idempotency query), `schedule/supervisor.rs:485-502` (hook), `auto_chain.rs:1594` (cron label), `review_cron_e2e.rs:1-285` (e2e), migration rename in diff
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 (non-blocking) |

**Verdict**: Approve
