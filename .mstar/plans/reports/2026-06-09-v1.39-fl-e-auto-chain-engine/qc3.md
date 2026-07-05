---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-09-v1.39-fl-e-auto-chain-engine"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report — qc-specialist-3 (Performance + Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-09T20:00:00Z

## Revalidation — W-D, W-E targeted re-review (initial wave: qc1, qc2, qc3; fix wave 2)
- Reviewer (this re-review): @qc-specialist-3 (qc-specialist-3)
- Date: 2026-06-09T20:00:00Z
- Scope of re-review: W-D + W-E only
- Diff basis: 1e10e3ef..84db9a0e (verbatim)

### Findings from Revalidation

#### 🔴 Critical
_None._

#### 🟡 Warning
_None._

#### 🟢 Suggestion
- **S-1 (W-D): Test coverage gap in `patch_work_stage` reordering** — The test `stage_advance_failure_does_not_apply_non_stage_fields` validates `advance_work_stage_atomic` in isolation but does NOT exercise the actual `patch_work_stage` reordering. It never calls `apply_non_stage_fields`, so it cannot prove that the reorder prevents non-stage field persistence on failure. A more robust test would call `patch_work_stage` (or the public `patch_work` endpoint) with a request containing both a stage change and a non-stage field change, and verify the non-stage field is untouched when the stage advance is rejected. → Add an integration test or make `patch_work_stage` `pub(crate)` for direct unit testing.

- **S-2 (W-E): Empirical index usage not verified** — The test `test_auto_chain_resume_index_exists` verifies the index DDL exists but does not confirm the SQLite query planner actually uses it for `find_resumable_works`. On small tables, the planner may prefer a table scan. → Add an `EXPLAIN QUERY PLAN` assertion in the test (or a manual verification script) to confirm index usage at typical table sizes.

### Updated Verdict (W-D + W-E only)
**Approve** — Both fixes are directionally correct and safe to merge. W-D fail-fast reordering addresses the non-atomic PATCH issue; W-E partial index correctly targets the boot resume query. No new Critical or Warning findings block the gate.

## Scope
- plan_id: 2026-06-09-v1.39-fl-e-auto-chain-engine
- Review range / Diff basis: c7a3fac1..c143da1f (initial wave scope)
- Working branch (verified): feature/v1.39-fl-e-auto-chain-engine
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0
- Files reviewed: 3 (works.rs handler, works.rs DB, migration SQL)
- Commit range (fix wave 2): 1e10e3ef..84db9a0e
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-daemon-runtime, cargo test -p nexus-local-db, cargo +nightly fmt --all -- --check

## Findings
### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion
- **S-1 (W-D): Test coverage gap in `patch_work_stage` reordering** — As noted in Revalidation. The existing test validates the atomic function in isolation but does not prove the handler-level reordering prevents partial persistence. → Integration test or `pub(crate)` unit test.

- **S-2 (W-E): Empirical index usage not verified** — As noted in Revalidation. The partial index DDL is correct but planner behavior is untested. → Add `EXPLAIN QUERY PLAN` verification.

## Source Trace
- Finding ID: S-1
- Source Type: manual-reasoning
- Source Reference: crates/nexus-daemon-runtime/src/api/handlers/works.rs:927-974 (test code)
- Confidence: High

- Finding ID: S-2
- Source Type: manual-reasoning
- Source Reference: crates/nexus-local-db/src/works.rs:1639-1667 (test code)
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Verification Evidence

### Clippy
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s.
```
(Zero warnings with `-D warnings`)

### Tests — nexus-daemon-runtime
```
test result: ok. 180 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Tests — nexus-local-db
```
test result: ok. 150 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Formatting
```
cargo +nightly fmt --all -- --check → no output (clean)
```

## W-D Detailed Assessment

**Fail-fast reordering**: The change moves `advance_work_stage_atomic` before `apply_non_stage_fields`. If the stage transition fails (e.g., active FL-E schedule constraint violation at line 1123 of `works.rs`), the function returns early before touching non-stage fields. This IS a valid fail-fast pattern.

**Documented non-atomicity**: The `// NOTE:` at line 553-558 clearly states the two operations are not in a single transaction and explains why. This is sufficient documentation.

**New failure mode analysis**: If `advance_work_stage_atomic` succeeds but `apply_non_stage_fields` fails (e.g., DB connection lost), the work record will have an updated stage but unchanged non-stage fields. This is a real inconsistency, but:
1. `apply_non_stage_fields` calls `works::patch_work` which is a simple UPDATE; failure modes are limited to DB connectivity issues.
2. The caller receives an error and can retry.
3. This is LESS severe than the OLD behavior where non-stage fields could be persisted but stage transition rolled back, leaving the work in an inconsistent "title updated but stage not advanced" state.

**Test robustness**: The test is trivially passing. It calls `advance_work_stage_atomic` directly with invalid input, verifies it fails, and checks the title wasn't changed. But the title was never changed because `apply_non_stage_fields` was never called. The test does not validate the reordering in `patch_work_stage`.

**Latency impact**: The reorder adds a re-fetch (`get_work` at line 585) after both operations. This is a third DB round-trip in the worst case (advance + apply + refetch). However, the baseline was already 2+ round-trips, and PATCH work stage is not a hot path. No significant regression.

## W-E Detailed Assessment

**Partial index correctness**: The index `(auto_chain_enabled, auto_chain_interrupted, status) WHERE auto_chain_enabled = 1` correctly targets the `find_resumable_works` query which filters on all three columns. The left-to-right column order matches typical SQLite composite index usage.

**Query planner**: SQLite may skip the index on very small tables, but this is expected. As the table grows, the planner will use the index. The partial condition (`auto_chain_enabled = 1`) keeps the index small since only a subset of works will have auto-chain enabled.

**Migration safety**: `CREATE INDEX IF NOT EXISTS` is safe under sqlx migration re-run. Per `nexus-local-db/AGENTS.md`, DDL is permitted with runtime queries (not compile-time macros).

**Index maintenance**: SQLite automatically maintains the index on every UPDATE/INSERT/DELETE. No write path can skip it.

**Scale estimate**: With 10k works and assuming ~10% have auto-chain enabled, the partial index covers ~1k rows. A SELECT using the index would scan only those rows, then filter by `driver_schedule_id IS NOT NULL` and `status != 'completed'`. The index lookup is O(log n) for the tree traversal plus O(m) for the leaf scan where m is the matching subset. This is fast. The per-Work fetch + INSERT in the boot resume loop still dominates runtime.
