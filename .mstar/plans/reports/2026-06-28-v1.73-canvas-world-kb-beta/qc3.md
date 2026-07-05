---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-28-v1.73-canvas-world-kb-beta"
verdict: "Approve"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: ark-code-latest (volcengine-plan/ark-code-latest)
- Review Perspective: Performance and reliability risk (Seat 3)
- Report Timestamp: 2026-06-29

## Scope
- plan_id: 2026-06-28-v1.73-canvas-world-kb-beta
- Review range / Diff basis: `git diff 87ab75bb...d04a6b4e`
- Working branch (verified): iteration/v1.73
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 95 changed in assigned range, focused on World KB read/write paths, pagination, OCC/CAS reliability, canvas data hooks/projection, virtualization, release cache wiring, and relevant tests/migrations/schemas.
- Commit range: merge-base `87ab75bb341f5779dff39375b99d5ffec162ea10` through implementation tip `d04a6b4ee78849a9603a2a96317adefcfe70b211` (current branch also contains sibling QC report commit `0d9b53b6`, excluded from implementation review).
- Tools run: `cargo test -p nexus-daemon-runtime --test world_kb_patch` (9/9 passed); `pnpm --filter web build` (passed; Vite emitted existing >500 kB chunk warning); git diff/status/log; manual source review; Context7 lookup for `Swatinem/rust-cache` cache-key behavior.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- (W-01) `GET /v1/local/worlds/{world_id}/kb/candidates` advertises cursor pagination, but the handler applies the cursor after fetching only the first `limit + 1` pending rows, so page 2 can be empty and candidates beyond the first page become unreachable. In `get_candidates`, `list_pending_for_world(..., Some(limit + 1))` is called before cursor filtering, and `next_cursor` is set to the first non-returned row; the next request skips through that row inside the same truncated first window and has no rows left to return. This violates the schema/plan expectation for a cursor-paginated candidates endpoint and will drop pending review work once a world has more than one page of candidates. Fix by moving cursor semantics into the storage query (ordered keyset such as `(created_at, job_id)` with `LIMIT limit + 1`) or by otherwise fetching enough rows after the cursor, and add a regression test with `limit = 2` and at least 4 pending candidates that verifies page 1 and page 2 both return data.

### 🟢 Suggestion
- (S-01) The graph endpoint is protected by a 500-row safety cap (`LIST_BY_WORLD_LIMIT` / `GRAPH_ENTITY_CAP`), but the `WorldKbGraphResponse` contract has no pagination or truncation metadata. If the cap is intentional for β, expose an explicit truncation indicator or document the low-hundreds ceiling so users do not mistake a partial graph for complete state.
- (S-02) The dependency-free alt-view virtualization is a reasonable small-slice implementation, but current tests exercise only three rows. Add a focused test for 100+ rows and scroll/window behavior when this surface becomes a primary accessibility path.
- (S-03) `pnpm --filter web build` succeeds and the World KB split chunk is small (`world-kb-page-B4W32dCD.js` 28.84 kB / gzip 8.82 kB), but Vite still warns that the shared `index` chunk is >500 kB. This is not introduced by the World KB page, but it remains worth tracking for future route-level/manual chunk work.

## Source Trace
- Finding ID: W-01
- Source Type: manual code review + contract/test gap
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:833-868` (`get_candidates` fetches `limit + 1` rows before cursor scan and sets `next_cursor` to the first non-returned row); `crates/nexus-local-db/src/kb_extract_job.rs:894-916` (`list_pending_for_world` always returns the first clamped `LIMIT` rows for the world); `schemas/local-api/canvas/world-kb/world-kb-candidates-response.schema.json:6-14` (cursor-paginated response contract); `crates/nexus-daemon-runtime/tests/world_kb_patch.rs:409-442` (only single-candidate happy path, no multi-page regression).
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

## Performance / Reliability Review Notes
- Backend entity writes use per-row CAS and the required stale-version tests pass. Atomic adopt/merge paths keep candidate state flips in the same transaction as the entity insert/update, which is reliable for the reviewed race class.
- Candidates storage has the right covering index for the intended list query: `idx_kb_extract_jobs_promotion_status_world ON kb_extract_jobs (promotion_status, world_id, created_at)`. The failure is not index coverage; it is cursor application after a pre-cursor limited fetch.
- World KB UI avoids polling and uses manual refresh plus mutation invalidation. This is acceptable for local-first β reliability and avoids unnecessary background daemon load.
- Desktop release Rust caching now mirrors the existing desktop-build pattern and uses `Swatinem/rust-cache` with both workspace roots listed; no blocking cache reliability issue found.

## Validation Evidence
- `cargo test -p nexus-daemon-runtime --test world_kb_patch` → passed: 9 tests, 0 failed.
- `pnpm --filter web build` → passed: TypeScript check + Vite build completed; Vite warning only for existing large shared chunk.

**Verdict**: Request Changes

## Revalidation (W-01 fix @ 35e4404e)

### Scope Rechecked
- Targeted re-review scope: W-01 only (candidates cursor pagination fix at `35e4404e` on top of initial review HEAD `d04a6b4e`).
- Verified checkout: `/Users/bibi/workspace/organizations/42ch/nexus` on `iteration/v1.73`, `git log -1 --oneline` = `35e4404e fix(world-kb): W-01 candidates cursor pagination — apply keyset filter in SQL (qc3)`.
- Reviewed fix diff via `git show 35e4404e` across:
  - `crates/nexus-local-db/src/kb_extract_job.rs`
  - `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs`
  - `crates/nexus-daemon-runtime/tests/world_kb_patch.rs`

### Evidence
- `cargo test -p nexus-daemon-runtime --test world_kb_patch` → passed: 10 tests, 0 failed, including `get_candidates_multi_page_cursor_reaches_all_rows`.
- Storage now applies keyset pagination in SQL: `list_pending_for_world_after` adds `AND (created_at, job_id) > (?, ?)` before `ORDER BY created_at ASC, job_id ASC LIMIT ?`, so the cursor advances the DB result set before `limit + 1` truncation.
- Handler now decodes opaque `kbp:<created_at>|<job_id>` cursors, fetches `limit + 1` rows after that tuple, returns only `limit` rows, and encodes `next_cursor` from the last visible row on the current page.
- Regression coverage seeds 4 pending candidates, uses `limit = 2`, verifies page 1 returns rows 1-2, page 2 returns rows 3-4, no row is lost or duplicated, and a terminal cursor does not repeat page 2.

### Disposition
- W-01 is resolved. The original reliability failure (page 2+ unreachable because the handler scanned for the cursor inside only the first `limit + 1` rows) is fixed by moving cursor semantics into the SQL query.
- No new Critical or Warning issue found in the targeted diff. The cursor token format intentionally changes to an opaque prefixed keyset cursor; in this pre-1.0 API, the old bare-`job_id` cursor was already broken for multi-page traversal and now fails fast as `400 invalid_input` instead of silently mis-paginating.
- Original Suggestions S-01 through S-03 remain visible as non-blocking follow-up ideas; they do not block this targeted revalidation.

**Updated Verdict**: Approve
