---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-13-v1.44-review-master-cli-surface"
verdict: "Request Changes"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk (Reviewer #3)
- Report Timestamp: 2026-06-13

## Scope
- plan_id: `2026-06-13-v1.44-review-master-cli-surface`
- Review range / Diff basis: `9d471bdc..c54b1aa6` (verbatim from Assignment)
- Working branch (verified): `iteration/v1.44`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 9
  - `crates/nexus42/src/commands/creator/run.rs`
  - `crates/nexus42/tests/review_master_cli.rs`
  - `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`
  - `crates/nexus42/src/commands/creator/works/mod.rs` (comparison baseline)
  - `docs/novel-writing-quickstart.md`
  - `.mstar/knowledge/specs/novel-quality-loop.md`
  - `.mstar/knowledge/specs/cli-spec.md`
  - `.mstar/knowledge/specs/novel-workflow-profile.md`
  - `.mstar/status.json`
- Commit range (identical to Review range): `9d471bdc..c54b1aa6`
- Tools run:
  - `cargo clippy -p nexus42 -- -D warnings`
  - `cargo test -p nexus42 --test review_master_cli`
  - `cargo +nightly fmt --all --check`
  - `git log --oneline 9d471bdc..c54b1aa6`

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

#### W-1: `review-master` default list caps at 50 open findings and filters `target_executor=master` client-side
- **Location**: `crates/nexus42/src/commands/creator/run.rs`, `handle_review_master`, lines 770–780.
- **Issue**: The command requests `/v1/local/works/{work_id}/findings?status=open&limit=50`, then filters to `target_executor == "master"` in the CLI. The daemon endpoint (`crates/nexus-daemon-runtime/src/api/handlers/findings.rs`) supports `status`, `severity`, `limit`, and `offset` query params but **not** `target_executor`; the DB query defaults to `ORDER BY created_at DESC LIMIT ?`. Because filtering happens after the fetch, master-targeted findings that fall outside the newest 50 open rows are silently omitted. The printed count (“N open”) and the `--auto-schedule` input `open_findings` can therefore be wrong for works with more than 50 open findings.
- **Impact**: Primary command behavior (list open master findings) is unreliable for long-lived or high-volume works; `--auto-schedule` may enqueue review-master with an incomplete finding list.
- **Evidence**:
  - `run.rs:772`: `"/v1/local/works/{work_id}/findings?status=open&limit=50"`
  - `findings.rs:181`: `let limit = filters.limit.unwrap_or(100);`
  - `findings.rs:197`: `ORDER BY created_at DESC`
  - `works/mod.rs:764`/`791`: existing `creator works status` already uses `FINDINGS_FETCH_LIMIT = 50` with explicit truncation signaling, but still client-side filters.
- **Fix options** (pick one):
  1. Add `target_executor` filter to `ListFindingsQuery` and `FindingListFilters` so the daemon returns only master findings, eliminating the client-side cap issue.
  2. If client-side filtering must remain, paginate with `offset` until all open findings are consumed, or at least surface truncation clearly (e.g., “showing master findings among the newest 50 open rows”).
  3. Add a dedicated daemon summary endpoint that returns the count and top-N master findings server-side.

#### W-2: `--auto-schedule` reads creator-scoped stale count but enqueues a single schedule under the supplied `work_id`
- **Location**: `crates/nexus42/src/commands/creator/run.rs`, `handle_review_master`, lines 941–1034.
- **Issue**: `GET /v1/local/findings/stale` returns all stale open findings for the **active creator**, not for the `work_id` argument. The handler then enqueues one `novel-review-master` schedule keyed to the user-supplied `work_id`, using `master_findings` from that work (already capped per W-1) as `input.open_findings`. If stale findings exist for other works, the schedule will not cover them; conversely, if this work has no stale master findings but another work does, the command still enqueues review-master for this work. The spec ([novel-quality-loop.md §3.4](.mstar/knowledge/specs/novel-quality-loop.md)) says “enqueue novel-review-master when 96h stale findings exist” but is ambiguous about work-scoping.
- **Impact**: Potential mis-attribution of stale-finding trigger and incomplete remediation in multi-work creators.
- **Evidence**:
  - `run.rs:949`: `"/v1/local/findings/stale"`
  - `findings.rs:349–396`: `list_stale_findings_handler` filters by `creator_id` only.
- **Fix options**:
  1. Scope the stale query to `work_id` (extend endpoint or add `?work_id=...`) and enqueue only when this work has stale findings.
  2. Keep global trigger semantics but document clearly in help/spec that `--auto-schedule` is creator-global and may enqueue for the named work even when the stale findings belong to another work.
  3. Iterate over each stale finding’s `work_id` and enqueue one schedule per affected work.

### 🟢 Suggestion

#### S-1: `review-master` uses the default 30 s daemon request timeout for findings list
- **Location**: `crates/nexus42/src/commands/creator/run.rs`, `handle_review_master`, line 770.
- **Issue**: The findings list uses the default `DaemonClient` (30 s request timeout). The existing `creator works status` path uses a dedicated 5 s timeout (`FINDINGS_FETCH_TIMEOUT`) so a slow findings query does not block the status command. `review-master` is similarly read-only and summary-oriented; a 30 s hang degrades perceived responsiveness.
- **Fix**: Use `DaemonClient::with_timeouts(..., FINDINGS_FETCH_TIMEOUT)` for the findings list, matching the status command pattern.

#### S-2: `handle_review_master` is large and mixes presentation, API orchestration, and input serialization
- **Location**: `crates/nexus42/src/commands/creator/run.rs`, lines 761–1038.
- **Issue**: The single handler contains the list rendering, the `--finding-id` schedule path, and the `--auto-schedule` path. While acceptable for a first slice, future extensions (pagination, additional filters, work-scoped stale query) will increase complexity. It also duplicates the work-info fetch and `AddScheduleRequest` construction across `--finding-id` and `--auto-schedule`.
- **Fix**: Extract small helpers for (a) fetching work context, (b) building `novel-review-master` input, and (c) printing summary/JSON. Keep the current behavior unchanged.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning + static-analysis | `run.rs:770–780`, `findings.rs:179–197`, `works/mod.rs:780–798` | High |
| W-2 | manual-reasoning + static-analysis | `run.rs:941–1034`, `findings.rs:349–396` | High |
| S-1 | manual-reasoning + static-analysis | `run.rs:770`, `daemon_client.rs:43`, `works/mod.rs:761–798` | High |
| S-2 | manual-reasoning | `run.rs:761–1038` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

Rationale: Both warning findings are within P1 scope and affect the reliability of the primary `review-master` surface. W-1 (pagination/client-side filtering) can silently produce an incorrect master-finding count and incomplete `--auto-schedule` input. W-2 (global stale count vs. single-work schedule) creates ambiguous multi-work behavior. Once these are addressed or explicitly accepted as residual with spec/implementation alignment, the P1 surface can move forward.
