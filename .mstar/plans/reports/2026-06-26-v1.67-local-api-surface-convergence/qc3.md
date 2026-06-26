---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-26-v1.67-local-api-surface-convergence"
verdict: "Request Changes"
generated_at: "2026-06-26T13:39:03Z"
focus: "performance_reliability"
---
# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-26T13:39:03Z

## Scope
- plan_id: 2026-06-26-v1.67-local-api-surface-convergence
- Review range / Diff basis: P0 feat commit `ea94b028`, merged at integration HEAD. `git show ea94b028`; diff basis vs `origin/main`. Scope = FE1-ORCH + CASING + F-P3 `items` + F-F1 sort + UI/CLI.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 17
- Commit range (if not identical to Review range line, explain): `origin/main...HEAD` (merge base `b06d075512972846d0a8039ade96966ee8119820`, tip `b349836112edf1b3380a1e15bbbacbe76aa5f6ca`), focused on P0 feature commit `ea94b028`.
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git rev-parse HEAD`; `git status --short`; `git show --stat --oneline --name-only ea94b028`; `git merge-base origin/main HEAD`; `git diff origin/main...HEAD -- <focused files>`; `cargo test -p nexus-daemon-runtime --test works_api --test fl_e_schedule_api`; `cargo test -p nexus-daemon-runtime sort --lib`.

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **W-QC3-001 — DB-backed Works and Schedules sort by materializing the full result set in memory before slicing the cursor page.**  
  Evidence: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:492-516` explicitly sets `limit: Some(1_000_000)`, fetches all filtered Works, then runs `records.sort_by(...)` before computing `start/end`; `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:633-667` builds a SQL query without `LIMIT/OFFSET`, calls `fetch_all`, maps all rows, then runs `items.sort_by(...)` before slicing at `:669-679`. This is `O(N log N)` CPU plus full-list memory on every page request, including page 1, and it bypasses the DB ordering/pagination path even though both resources are SQLite-backed and already have sortable persisted columns.  
  Impact: large local Work/Schedule histories can make list endpoints slow or memory-heavy; schedule lists can grow over time through orchestration/cron usage. Offset cursor semantics are also coupled to a newly sorted full snapshot on every request, so inserts/updates between pages are more likely to shift rows than when the database owns a deterministic `ORDER BY ... LIMIT ... OFFSET ...` query.  
  Fix: push F-F1 sort keys for Works/Schedules into the SQL layer (`ORDER BY` allowlist + `LIMIT ? OFFSET ?`, preferably with a deterministic tie-breaker such as ID/created_at) and keep in-memory sorting only for genuinely bounded non-DB lists (sessions/capabilities) or explicitly documented small sets.

- **W-QC3-002 — F-F1 `<resource>_sort_invalid` errors are constructed but not exposed in the public error envelope.**  
  Evidence: `crates/nexus-daemon-runtime/src/api/sort.rs:39-45` returns `NexusApiError::BadRequest { code: format!("{resource}_sort_invalid"), ... }`, but `NexusApiError::error_code()` only passes through a fixed set of BadRequest codes and maps all others to `"bad_request"` (`crates/nexus-daemon-runtime/src/api/errors.rs:220-232`). The compass locked unknown sort keys to `<resource>_sort_invalid`; clients currently receive the generic `bad_request` code instead.  
  Impact: UI/CLI clients cannot reliably distinguish invalid sort grammar from other bad requests, and the new additive sort contract is not actually observable through the canonical error code.  
  Fix: make sort-invalid codes public in `error_code()` (for example by allowing suffix `_sort_invalid` or by adding a typed error variant), and add one endpoint-level assertion that the JSON body includes the resource-specific code.

- **W-QC3-003 — Sort behavior lacks executable regression coverage for order, invalid-key codes, and cursor stability.**  
  Evidence: repo search found no runtime tests matching `sort_invalid`, `?sort`, or sort-order assertions in `crates/nexus-daemon-runtime`; existing updated tests mainly assert the new `items` key and pagination envelope (`tests/works_api.rs:210-218`, `tests/fl_e_schedule_api.rs:285-299`, `:341-349`, `:414-435`). The scoped test command passed, but it does not exercise F-F1 semantics.  
  Impact: regressions like W-QC3-001 (DB-backed full-list sort) and W-QC3-002 (generic error code) can pass CI while breaking the new contract. Cursor semantics under sorted lists also remain unguarded.  
  Fix: add targeted tests for Works and Schedules: valid multi-key sort order, unknown sort key response code, and a small paginated sorted list proving `next_cursor` advances through server-order pages. Add bounded tests for Sessions/Capabilities if those remain in-memory.

### 🟢 Suggestion
- **S-QC3-001 — Clarify and/or enforce no-sort defaults for in-memory resources.** `apps/web/src/api/queries.ts:113` states Capabilities default server order is by name, but `list_capabilities` only sorts when `sort` is supplied (`capabilities.rs:25`, `:47-59`); with no sort terms it preserves registry insertion order. Either apply `name` as the handler default or remove the default-order claim. Sessions similarly have no explicit default tie-breaker.

## Source Trace
- Finding ID: W-QC3-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:492-516`; `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:633-679`; compass §5 item #4.
  - Confidence: High
- Finding ID: W-QC3-002
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-daemon-runtime/src/api/sort.rs:39-45`; `crates/nexus-daemon-runtime/src/api/errors.rs:220-232`; compass §5 item #4.
  - Confidence: High
- Finding ID: W-QC3-003
  - Source Type: test-gap review + grep
  - Source Reference: `grep` for `sort_invalid|sort=|?sort`; `crates/nexus-daemon-runtime/tests/works_api.rs:210-218`; `crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs:285-299`.
  - Confidence: High
- Finding ID: S-QC3-001
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/api/queries.ts:113`; `crates/nexus-daemon-runtime/src/api/handlers/orchestration/capabilities.rs:25,47-59`.
  - Confidence: Medium

## Positive Notes
- F-P3 response-shape adaptation appears to preserve cursor metadata in reviewed call sites: Web hooks consume `res.items` plus `res.pagination` for infinite queries, and backend responses reviewed return `{ items, pagination }`.
- The new 503/422 variants in `NexusApiError` degrade through canonical `IntoResponse` without `unwrap()`/`expect()` on the error path; `preset_gates_failed` uses `serde_json::to_value(...).unwrap_or_else(...)`, so serialization failure does not panic.
- Scoped regression tests passed: `cargo test -p nexus-daemon-runtime --test works_api --test fl_e_schedule_api` (45 tests passed). `cargo test -p nexus-daemon-runtime sort --lib` passed but selected 0 tests, reinforcing the missing sort-specific coverage warning.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
