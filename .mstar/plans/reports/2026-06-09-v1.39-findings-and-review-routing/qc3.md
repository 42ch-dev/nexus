---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-09-v1.39-findings-and-review-routing"
verdict: "Request Changes"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-09T00:00:00Z

## Scope
- plan_id: 2026-06-09-v1.39-findings-and-review-routing
- Review range / Diff basis: merge-base: 111c3611 + tip: 137fefaf; equivalent to git diff 111c3611...137fefaf
- Working branch (verified): feature/v1.39-findings-and-review-routing
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p1
- Files reviewed: 14 (+1337 / -4 lines)
- Commit range: 111c3611..137fefaf (5 commits)
- Tools run: cargo clippy, cargo test (findings_api, auto_chain, research, work_chapters), cargo +nightly fmt --check

## Findings

### 🔴 Critical
- **C-1: Missing `(work_id, chapter, status)` index per normative spec**  
  The migration `202606090002_findings.sql` creates only two indexes: `(work_id, status)` and `(creator_id, status)`. However, the normative spec `novel-quality-loop.md` §2.1 explicitly requires three indexes: `(work_id, status)`, `(work_id, chapter, status)`. The missing `(work_id, chapter, status)` composite index means the supervisor's `create_finding_from_review` hook (which binds findings to chapters) and any future chapter-scoped listing will perform full table scans at scale.  
  → Fix: Add `CREATE INDEX IF NOT EXISTS idx_findings_work_chapter_status ON findings(work_id, chapter, status);` to the migration.

### 🟡 Warning
- **W-1: CLI status per-call cost — unbounded HTTP timeout and no fallback**  
  The `creator run status` command (T5, `crates/nexus42/src/commands/creator/run.rs:725-730`) makes a synchronous HTTP GET to `/v1/local/works/{work_id}/findings?status=open&limit=3` with `.unwrap_or_else(|_| serde_json::json!([]))`. While the `limit=3` cap is good, there is no explicit request timeout on this call. If the daemon DB is under contention, the status command will block indefinitely.  
  → Fix: Add a short timeout (e.g., 2s) to the findings GET call, or make it async/parallel with the main work fetch. At minimum, document the expected latency ceiling.

- **W-2: `list_findings` query may skip index for cross-creator list patterns**  
  The `list_findings` DAO query (`findings.rs:111-137`) uses `creator_id = ?` as the mandatory filter with optional `work_id`, `chapter`, `status`, `severity` via `(? IS NULL OR col = ?)` patterns. The `(creator_id, status)` index is useful when `status` is provided, but the query planner may choose a suboptimal path when `status` is NULL (common for "list all findings for this creator across all works"). At scale (1000+ findings per work, many works), this could degrade.  
  → Fix: Consider adding a standalone `(creator_id)` index or restructuring the query to use UNION of indexed branches instead of OR-with-NULL. For V1.39, at least add the index and monitor.

- **W-3: `count_open_findings_by_severity` uses runtime query (not compile-time checked)**  
  The function (`findings.rs:280-288`) uses `sqlx::query` (runtime) with a `// SAFETY` comment for the COUNT(*) aggregate. While this is explicitly allowed by `nexus-daemon-runtime/AGENTS.md` for aggregates, it bypasses sqlx's compile-time schema validation. If the `findings` table schema drifts in a future migration, this query will fail at runtime rather than at build time.  
  → Fix: Acceptable for V1.39 given the AGENTS.md exemption, but add a test that validates the query against the current schema (e.g., a DAO test that calls `count_open_findings_by_severity` and asserts non-panic).

- **W-4: Error from `create_finding_from_review` is not explicitly logged or surfaced to user**  
  The `create_finding_from_review` DAO function (`findings.rs:342-363`) returns `LocalDbError` on failure. The API handler (`create_from_review_handler`, `findings.rs:273-299`) propagates this as `NexusApiError::Internal` (500). However, there is no structured logging at the supervisor/orchestration layer that calls this hook. If the DB write fails during auto-chain review stage completion, the error is returned to the orchestration layer but may be silently swallowed if the supervisor does not log it.  
  → Fix: Add `tracing::error!` or equivalent in the API handler when `create_finding_from_review` fails, with `work_id` and `finding_id` context. Ensure the orchestration supervisor logs the error before continuing the auto-chain.

### 🟢 Suggestion
- **S-1: Add `created_at` index for `ORDER BY created_at DESC`**  
  The `list_findings` query orders by `created_at DESC` but there is no index on `created_at`. For small tables this is fine, but at 1000+ findings per work, the sort will become a filesort. A covering index `(creator_id, status, created_at DESC)` or `(work_id, status, created_at DESC)` would eliminate this.  
  → Consider adding in a future performance pass.

- **S-2: `Finding.severity` and `Finding.status` use `String` instead of enums**  
  The `Finding` struct uses `String` for `severity` and `status` fields. While the API validates inputs at the handler layer, using Rust enums (e.g., `Severity::Info | Minor | Major | Blocker`) would provide compile-time safety and prevent invalid values from propagating.  
  → Consider a type-safe refactor in V1.40 hygiene.

## Source Trace

- **C-1**: Source: manual-reasoning / doc-rule  
  Reference: `crates/nexus-local-db/migrations/202606090002_findings.sql` vs `novel-quality-loop.md` §2.1  
  Confidence: High

- **W-1**: Source: manual-reasoning  
  Reference: `crates/nexus42/src/commands/creator/run.rs:725-730`  
  Confidence: Medium

- **W-2**: Source: manual-reasoning / static-analysis  
  Reference: `crates/nexus-local-db/src/findings.rs:111-137`  
  Confidence: Medium

- **W-3**: Source: static-analysis  
  Reference: `crates/nexus-local-db/src/findings.rs:280-288`  
  Confidence: High

- **W-4**: Source: manual-reasoning  
  Reference: `crates/nexus-local-db/src/findings.rs:342-363` and `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:273-299`  
  Confidence: Medium

- **S-1**: Source: manual-reasoning  
  Reference: `crates/nexus-local-db/src/findings.rs:124`  
  Confidence: Medium

- **S-2**: Source: manual-reasoning  
  Reference: `crates/nexus-local-db/src/findings.rs:14-38`  
  Confidence: Low

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

### Rationale
The missing `(work_id, chapter, status)` index (C-1) is a **normative spec violation** (`novel-quality-loop.md` §2.1 explicitly lists this index) and will cause full-table scans for chapter-scoped finding queries, which are central to the review-stage hook. This is a performance regression at scale and must be fixed before merge.

The warnings (W-1 through W-4) are addressable without architectural changes:
- W-1: Add HTTP timeout or document latency ceiling.
- W-2: Acceptable risk for V1.39 scale, but should be monitored.
- W-3: Add a test for the runtime query.
- W-4: Add structured logging in the error path.

All verification commands passed:
- `cargo clippy`: clean
- `cargo test -p nexus-daemon-runtime --test findings_api`: 7 passed in 0.27s (hermetic, <1s)
- `cargo test -p nexus-orchestration --test auto_chain`: 21 passed in 0.40s
- `cargo test -p nexus-orchestration --lib -- research`: 17 passed
- `cargo test -p nexus-local-db --lib -- work_chapters`: 21 passed in 0.54s
- `cargo +nightly fmt --all -- --check`: clean

No source files modified during this review.
