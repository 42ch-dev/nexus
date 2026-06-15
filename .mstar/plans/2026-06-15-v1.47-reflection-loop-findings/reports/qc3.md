---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-15-v1.47-reflection-loop-findings"
verdict: "Request Changes"
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
- Files reviewed: 34 changed files, focused on `crates/nexus-orchestration/src/auto_chain.rs`, `crates/nexus-orchestration/src/schedule/supervisor.rs`, `crates/nexus-local-db/src/findings.rs`, `crates/nexus-orchestration/tests/review_findings.rs`, `crates/nexus-orchestration/embedded-presets/novel-chapter-review/preset.yaml`
- Commit range: `594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD`
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current`
  - `git diff --stat 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD`
  - `cargo test -p nexus-orchestration --test review_findings`
  - `cargo test -p nexus-daemon-runtime --test findings_api`
  - `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus-daemon-runtime -p nexus42 -- -D warnings`
  - `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

#### W-1: Unconditional schedule-row lookup in the supervisor terminal hot path adds overhead for every terminal schedule
The supervisor calls `auto_chain::persist_review_findings_for_schedule` unconditionally inside `on_schedule_terminal(Completed)` (lines 396-405 of `crates/nexus-orchestration/src/schedule/supervisor.rs`). The function then executes a `SELECT preset_id, work_id, creator_id FROM creator_schedules WHERE schedule_id = ?` query to decide whether the schedule is a review schedule. For the common case where the completed schedule is `novel-writing` (produce), `kb-extract` (persist), `research`, etc., this is a wasted indexed read on the critical terminal-transition path. The overhead is small per call but multiplies across every auto-chain stage terminal for every Work.

- **Evidence**: `crates/nexus-orchestration/src/schedule/supervisor.rs:396-405`; `crates/nexus-orchestration/src/auto_chain.rs:120-162`.
- **Proposed fix**: Make the hook conditional on the schedule's `preset_id`. The supervisor already fetches `creator_id` from the schedule row; it could also fetch `preset_id` and only call the review-finding hook when `preset_id == "novel-chapter-review"`. Alternatively, pass the already-known preset ID into the hook and return immediately without a DB round-trip.
- **Severity (JSON)**: `medium`

#### W-2: Review → finding path is not idempotent and has no retention cap, risking unbounded duplicate findings
`findings::create_finding_from_review` (and the supervisor hook that calls it) mints a fresh `finding_id` (`fnd_{uuid}`) and inserts a row on every invocation. There is no unique constraint or deduplication key on `(work_id, chapter, review_schedule_id)` or any equivalent pass identifier. Re-running `creator run novel-chapter-review <work_id>` on the same chapter, or re-processing a terminal event, creates additional finding rows for the same conceptual review pass. The codebase also has no retention/cleanup policy for findings, so this growth is unbounded over the lifetime of a Work.

- **Evidence**: `crates/nexus-local-db/src/findings.rs:611-641`; `crates/nexus-orchestration/src/auto_chain.rs:223-244`; migration `202606150001_findings_kind_rule_suggestion.sql` adds no unique index.
- **Proposed fix**: Add a unique key on `(work_id, chapter, source_schedule_id)` (or store the originating `schedule_id` on the finding row and enforce uniqueness), or document the intentional "one row per review run" semantics and add a retention/cleanup policy. At minimum, the hook should record the originating `schedule_id` so duplicates are observable and future slices can deduplicate.
- **Severity (JSON)**: `high`

### 🟢 Suggestion

#### S-1: Plan verification command in §6 matches zero tests
The plan's Verification block lists:

```bash
cargo test -p nexus-orchestration -p nexus42 -- reflection 2>&1 | tail -30
```

No test name in the codebase contains `reflection` after the preset rename, so this command filters out all tests and exits with `0 passed; 0 failed`. Future QC/QA runs could falsely believe the reflection/review tests passed when they were not executed at all.

- **Evidence**: Plan file §6; `cargo test -p nexus-orchestration -p nexus42 -- reflection` output (0 tests run).
- **Proposed fix**: Update the verification command to `cargo test -p nexus-orchestration --test review_findings` or `cargo test -p nexus-orchestration -- review`.
- **Severity (JSON)**: `low`

#### S-2: Hook read/write operations are not transactional
`persist_review_findings_for_schedule` reads the schedule row, reads the Work row, and then inserts a finding row using separate pool acquisitions. The current implementation only performs a single `INSERT`, so the practical risk is low, but any future extension that updates multiple tables (e.g. parsing the review report and inserting many findings, or updating Work counters) should be wrapped in a single transaction to keep the terminal hook atomic and failure-safe.

- **Evidence**: `crates/nexus-orchestration/src/auto_chain.rs:120-245`.
- **Proposed fix**: Wrap the Work-read + finding-insert in a `sqlx::Transaction` passed down to `create_finding_from_review` when the hook grows beyond a single row write.
- **Severity (JSON)**: `low`

## Source Trace

- **F-001** (W-1): `git diff 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD -- crates/nexus-orchestration/src/schedule/supervisor.rs crates/nexus-orchestration/src/auto_chain.rs` → manual reasoning + runtime command `cargo test -p nexus-orchestration --test review_findings`.
- **F-002** (W-2): `git diff ... crates/nexus-local-db/src/findings.rs crates/nexus-local-db/migrations/202606150001_findings_kind_rule_suggestion.sql` → manual reasoning.
- **F-003** (S-1): Plan file `.mstar/plans/2026-06-15-v1.47-reflection-loop-findings.md` §6 + test run output.
- **F-004** (S-2): `git diff ... crates/nexus-orchestration/src/auto_chain.rs` → manual reasoning.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

Both warnings are substantive and should be addressed or explicitly risk-accepted before merge:
- W-1 is a perf/reliability regression on the terminal hot path.
- W-2 is a data-growth / idempotency risk that directly affects the new feature's correctness over time.

The acceptance criteria are functionally met (tests pass, findings are created, driver invariant is preserved), so the changes are close to merge-ready once the hot-path overhead and duplicate-growth concerns are resolved or tracked as open residuals.
