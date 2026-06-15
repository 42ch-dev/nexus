---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-15-v1.47-reflection-loop-findings"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-15T00:00:00Z

## Scope
- plan_id: `2026-06-15-v1.47-reflection-loop-findings`
- Review range / Diff basis: `merge-base: 594b00b51c43681ec779f9ad6fef09333ffc2ed8 + tip: HEAD` (i.e. `git diff 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD`)
- Working branch (verified): `feature/v1.47-reflection-loop-findings`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p0-reflection`
- Files reviewed: 47 changed files, focused on `crates/nexus-orchestration/src/schedule/supervisor.rs`, `crates/nexus-orchestration/src/auto_chain.rs`, `crates/nexus-local-db/src/findings.rs`, `crates/nexus-local-db/migrations/202606150002_findings_source_schedule_unique.sql`, `crates/nexus-orchestration/tests/review_findings.rs`, `.mstar/plans/2026-06-15-v1.47-reflection-loop-findings.md`
- Commit range: `594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD`
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current`
  - `git diff --stat 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD`
  - `git show 2c125252 -- crates/nexus-orchestration/src/schedule/supervisor.rs`
  - `git show 6fcfa322 -- crates/nexus-orchestration/src/auto_chain.rs crates/nexus-local-db/src/findings.rs crates/nexus-orchestration/tests/review_findings.rs crates/nexus-local-db/migrations/202606150002_findings_source_schedule_unique.sql crates/nexus-daemon-runtime/src/api/handlers/findings.rs`
  - `git show 7c4dae34 -- .mstar/plans/2026-06-15-v1.47-reflection-loop-findings.md`
  - `cargo test -p nexus-orchestration --test review_findings`
  - `cargo test -p nexus-daemon-runtime --test findings_api`
  - `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus-daemon-runtime -p nexus42 -- -D warnings`
  - `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None. (Prior W-1 and W-2 are resolved; see Revalidation.)

### 🟢 Suggestion
- None. (Prior S-1 is resolved; S-2 remains deferred and acceptable; see Revalidation.)

## Revalidation

### W-1: Unconditional schedule-row lookup overhead — resolved
- **Fix commit**: `2c125252`
- **What changed**: `ScheduleSupervisor::on_schedule_terminal` now fetches `creator_id` **and** `preset_id` in a single query (`SELECT creator_id as "creator_id!", preset_id as "preset_id!" FROM creator_schedules WHERE schedule_id = ?`) and guards the `auto_chain::persist_review_findings_for_schedule` hook with `preset_id == "novel-chapter-review"`.
- **Evidence**: `git show 2c125252 -- crates/nexus-orchestration/src/schedule/supervisor.rs` lines 335-425. For non-review terminal schedules, the review-finding hook is skipped entirely and no extra schedule-row lookup occurs inside `persist_review_findings_for_schedule`.
- **Disposition**: Resolved. The supervisor no longer pays the review-hook lookup cost for `novel-writing`, `kb-extract`, `research`, or other non-review presets.

### W-2: Review → finding not idempotent + no retention — resolved (idempotency), retention deferred
- **Fix commit**: `6fcfa322`
- **What changed**:
  - Migration `202606150002_findings_source_schedule_unique.sql` adds `source_schedule_id TEXT` and a partial unique index `findings_unique_review_per_chapter ON findings (work_id, chapter, source_schedule_id) WHERE source_schedule_id IS NOT NULL`.
  - `ReviewVerdictFinding` gained `source_schedule_id: Option<String>`.
  - `findings::create_finding_from_review` uses an idempotent `INSERT ... ON CONFLICT (work_id, chapter, source_schedule_id) WHERE source_schedule_id IS NOT NULL DO NOTHING` when `source_schedule_id` is `Some`; on conflict it fetches and returns the existing `finding_id`.
  - `auto_chain::persist_review_findings_for_schedule` passes `source_schedule_id: Some(schedule_id.to_string())`.
  - `daemon-runtime` manual API path passes `source_schedule_id: None` (no idempotency guard for manual CRUD).
- **Evidence**:
  - Migration file and `findings.rs` dynamic INSERT/fetch logic in `git show 6fcfa322`.
  - Test `ac5_idempotent_review_repeat_no_duplicate_finding` in `crates/nexus-orchestration/tests/review_findings.rs` passed (`cargo test -p nexus-orchestration --test review_findings`).
  - `cargo test -p nexus-daemon-runtime --test findings_api` passed, confirming the manual API path still works with `source_schedule_id: None`.
- **Cross-DB note**: The partial unique index syntax is valid for SQLite and PostgreSQL. The runtime fetch-by-triple query uses `chapter IS ?`, which is correct for SQLite's nullable `chapter` column. The crate is currently SQLite-only (`SqlitePool`), so this is acceptable for the target DB.
- **Retention**: The unbounded-growth aspect of W-2 is intentionally deferred to V1.48+ and is now tracked in the plan §7 Follow-ups as "Findings retention / cleanup policy (unbounded growth risk; qc3 W-2 residue)".
- **Disposition**: Resolved for the idempotency/duplicate-finding risk. Retention policy is an accepted follow-up.

### S-1: Plan §6 verification command broken — resolved
- **Fix commit**: `7c4dae34`
- **What changed**: The broken command `cargo test -p nexus-orchestration -p nexus42 -- reflection` was replaced with `cargo test -p nexus-orchestration --test review_findings`, with an inline note explaining the preset rename.
- **Evidence**: `git show 7c4dae34 -- .mstar/plans/2026-06-15-v1.47-reflection-loop-findings.md`. The updated command was executed and ran 5 tests (0 filtered out).
- **Disposition**: Resolved.

### S-2: Hook read/write not transactional — deferred (acceptable today)
- **Status**: No code change for this item.
- **Rationale**: The hook still performs a single `INSERT` (idempotent) per terminal event and does not update multiple tables. The plan §7 Follow-ups captures the deferred richer-synthesis work, and the single-write nature keeps the practical failure mode low. Per assignment, this is acceptable for V1.47 P0.
- **Disposition**: Deferred / acceptable.

## Source Trace

- **R-001** (W-1): `git show 2c125252 -- crates/nexus-orchestration/src/schedule/supervisor.rs` → manual reasoning + test `cargo test -p nexus-orchestration --test review_findings`.
- **R-002** (W-2): `git show 6fcfa322 -- crates/nexus-local-db/src/findings.rs crates/nexus-local-db/migrations/202606150002_findings_source_schedule_unique.sql crates/nexus-orchestration/tests/review_findings.rs` → manual reasoning + passing `ac5_idempotent_review_repeat_no_duplicate_finding`.
- **R-003** (S-1): `git show 7c4dae34 -- .mstar/plans/2026-06-15-v1.47-reflection-loop-findings.md` + test run output.
- **R-004** (S-2): `git diff 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD -- crates/nexus-orchestration/src/auto_chain.rs` → manual reasoning; no fix required for P0.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

The fix round addresses the performance hot-path concern (W-1) and the idempotency/correctness risk (W-2) with a targeted partial unique index and conditional hook. The plan verification command (S-1) is now accurate and executable. The transactional hook suggestion (S-2) remains acceptable for the single-INSERT P0 scope. All scoped tests, nightly formatting, and clippy pass without introduced regressions.
