---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-08-v1.38-multi-chapter-selection-status"
verdict: "Request Changes"
generated_at: "2026-06-08"
---

# Code Review Report — QC3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-08T12:00:00Z

## Scope
- plan_id: `2026-06-08-v1.38-multi-chapter-selection-status`
- Review range / Diff basis: `merge-base(3f72b085, HEAD)..HEAD` on `iteration/v1.38` (commit `2abbaa1a merge(v1.38-p0)` brings in feature commit `ffeb0adc feat: V1.38 P0 multi-chapter selection and status foundation`).
- Working branch (verified): `iteration/v1.38`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10
- Commit range: `3f72b085..2abbaa1a`
- Tools run: `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` (exit 0)

## Acceptance Criteria Review

| AC | Plan §6 | Status | Evidence |
|----|---------|--------|----------|
| AC1 | Work with ch1 finalized and 2..N not started selects ch2 | Pass | `test_next_chapter_selects_lowest_not_started` (work_chapters.rs:1101) |
| AC2 | Existing draft chapter resumes, no later chapter created | Pass | `test_next_chapter_resumes_draft` (work_chapters.rs:1127) — verifies not_started wins over draft, then draft resumes when no not_started remain |
| AC3 | Outlined chapter is not skipped | Pass | `test_next_chapter_outlined_not_skipped` (work_chapters.rs:1187) — outlined selected after not_started, not skipped |
| AC4 | `current_chapter` updates only on finalize | Pass | `novel_chapter_transition.rs:224` — `advance_current_chapter` called only when `to_status == "finalized"` |
| AC5 | Completion fires only when all finalized + current_chapter >= total + intake complete | Pass | `is_work_completed` (work_chapters.rs:581) checks all three conditions; `test_completion_all_finalized`, `test_completion_blocked_by_draft`, `test_completion_blocked_by_intake` |
| AC6 | `creator run status` shows progress, rows, next action | Pass | `run.rs:576-691` — per-chapter rows, progress count, next action hint per status |
| AC7 | Tests cover ch2..N selection and one-chapter compat | Pass | `test_one_chapter_v136_compatible` (work_chapters.rs:1372) + 6 other selection/completion tests |

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

#### W-1: Missing composite index for `next_chapter()` query pattern
- **Location**: `crates/nexus-local-db/migrations/202606070001_work_chapters.sql:20`
- **Issue**: The migration creates `CREATE INDEX work_chapters_by_status ON work_chapters(status)` — a single-column index on `status`. The `next_chapter()` function (work_chapters.rs:511) issues three queries of the form:
  ```sql
  SELECT chapter FROM work_chapters
  WHERE work_id = ? AND status = '...'
  ORDER BY chapter ASC LIMIT 1
  ```
  With only the `status` index, SQLite cannot efficiently satisfy the `work_id = ? AND status = ?` predicate. The PK `(work_id, chapter)` covers the `work_id` filter and `ORDER BY chapter`, but the `status` filter requires a row-by-row scan of all chapters for that work. For typical novels (≤100 chapters) this is negligible, but it scales linearly with chapter count and is a clear mismatch between the index and query pattern.
- **Fix**: Add a composite index `(work_id, status, chapter)` or at minimum `(work_id, status)` to support the `next_chapter()` lookup efficiently. The existing `work_chapters_by_status` index is useful for aggregate queries but not for per-work status lookups.
- **Source**: manual-reasoning + migration file read

#### W-2: Write-on-read anti-pattern in `GET /v1/local/works/{id}`
- **Location**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:322-365`
- **Issue**: The `get_work` handler (a GET endpoint) mutates `works.status` to `"completed"` when `is_work_completed()` returns true. This is a side effect on a read operation. While the code logs the promotion via `tracing::info!` and warns on failure, consumers may be surprised that a GET changes resource state. It also means two identical GET requests can return different status values (first returns `active`, second returns `completed`).
- **Trade-off**: The implementation note (line 320-321) acknowledges this as "auto-promote" and defers a cleaner transition-time promotion. For a local-first daemon with low request rates, the practical risk is low, but it violates REST semantics and can complicate caching, retry logic, and client expectations.
- **Fix**: Move the completion promotion to the finalize transition path (`novel_chapter_transition.rs` or schedule completion hook) so the write happens when the state actually changes, not on observation. Alternatively, document the behavior explicitly in the API contract if retaining the lazy promotion.
- **Source**: manual-reasoning + code review

### 🟢 Suggestion

#### S-1: No cap on `WorkApiDto.chapters` vector size
- **Location**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:387-404`
- **Issue**: `enrich_with_chapters()` calls `list_chapters()` which returns ALL rows for a work. A Work with an unusually high `total_planned_chapters` (e.g. 1000+) would produce an unbounded JSON response and CLI output. There is no pagination, truncation, or cap.
- **Rationale**: At expected novel scales (≤100 chapters) this is not a practical concern. The risk is low because `total_planned_chapters` is user-controlled during init and the local DB is not exposed to untrusted network clients.
- **Fix (optional)**: Add a `LIMIT` to `list_chapters` or truncate the `chapters` vector in the DTO builder with a comment documenting the cap. Consider paging if multi-volume works with 100+ chapters become common.
- **Source**: manual-reasoning

#### S-2: `next_chapter()` could be a single CTE instead of three round-trips
- **Location**: `crates/nexus-local-db/src/work_chapters.rs:511-560`
- **Issue**: The three-tier selection (`not_started` → `outlined` → `draft`) uses three sequential queries. In SQLite, this could be expressed as a single CTE or `UNION ALL` with `LIMIT 1`, reducing query round-trips from 3 to 1.
- **Rationale**: For a local SQLite connection, round-trip latency is negligible. The current implementation is clearer and easier to test. Only optimize if profiling shows it matters.
- **Fix (optional)**: Refactor to a single query if the composite index (W-1) is added and further optimization is desired.
- **Source**: manual-reasoning

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `crates/nexus-local-db/migrations/202606070001_work_chapters.sql:20` + `work_chapters.rs:511-560` | High |
| W-2 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:322-365` | High |
| S-1 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:387-404` | Medium |
| S-2 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs:511-560` | Medium |

## Diff Scope Check

| Deferred Boundary | Touched? | Evidence |
|-------------------|----------|----------|
| Auto-chain / auto-reenqueue | No | `stage_gates.rs` only adds `chapter` to preset input; no on_complete hooks |
| World KB (DF-63) | No | No `world_id` validation, no KB queries, no `world_refs` enforcement |
| Quality loop (DF-64/65/66/67) | No | No findings table, no rules files, no 96h escalation |
| Multi-volume PK migration | No | PK stays `(work_id, chapter)`; `volume` remains nullable |
| Platform publish | No | No publish surfaces touched |
| Multi-work switch | No | No multi-work scheduler changes |
| Selection pool | No | No pool abstraction added |

**Result**: Diff stays within V1.38 P0 scope. No Critical boundary violation.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

### Rationale
Two Warning-level findings remain unresolved:

1. **W-1 (Index mismatch)**: The `work_chapters_by_status` index does not support the `next_chapter()` query pattern. This is a latent performance issue that will grow with chapter count.
2. **W-2 (Write-on-read)**: GET `/v1/local/works/{id}` mutates `works.status`. This violates read semantics and can surprise consumers.

Both are fixable without architectural changes:
- W-1: Add `CREATE INDEX work_chapters_work_status ON work_chapters(work_id, status, chapter)` in a new migration.
- W-2: Either document the lazy promotion explicitly in the API spec, or move the promotion to the finalize transition path.

The Suggestion-level items (S-1, S-2) are optional optimizations and do not block approval.
