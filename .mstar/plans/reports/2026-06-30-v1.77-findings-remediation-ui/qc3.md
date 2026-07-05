---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-30-v1.77-findings-remediation-ui"
verdict: "Approve"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-30

## Scope
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Working branch (verified)**: `iteration/v1.77`
- **Review range / Diff basis**: `git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0` (10 implementation commits; merge-base `ba71d916` = origin/main, tip `a2571381` = the integration HEAD before QC report commits. NOTE: qc1/qc2 report commits may now sit atop `a2571381` — review the **implementation** diff `ba71d916..a2571381`, not QC report commits.)
- **plan_ids covered this round**: TWO —
  - P0: `2026-06-30-v1.77-findings-remediation-ui` (Track A lead, M)
  - P1: `2026-06-30-v1.77-slate-clear` (Track B companion, S-M)
- plan_id: 2026-06-30-v1.77-findings-remediation-ui
- Files reviewed: 25 implementation files in the assigned range; P0 focus on `apps/web/src/api/queries.ts`, `apps/web/src/lib/nexus/query-keys.ts`, `apps/web/src/pages/findings-page.tsx`, `apps/web/src/components/findings/finding-detail-panel.tsx`, `apps/web/src/lib/findings-lifecycle.ts`, `apps/web/src/components/status-badge.tsx`, and related tests/client types.
- Commit range: `ba71d916...a2571381` (implementation HEAD before QC report commits)
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git diff --stat ba71d916...a2571381`; `git diff --numstat ba71d916...a2571381`; targeted `read`/`grep`; `pnpm --filter web build`; `pnpm --filter web test -- findings-mutation finding-detail-panel findings-lifecycle`.
- Deep review: triggered (S1: assigned implementation range is 29 files / 2018 insertions; S6: web client + query cache + UI + tests; assignment-specific signals: new mutation surface and unbounded-list refetch risk).
- Lenses applied: Performance Lens, Reliability Lens, Testing Lens.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

- **W-QC3-P0-001 (medium, Warning)** — `useUpdateFinding` invalidates the global `queryKeys.findings.lists()` prefix on every finding mutation, so any active findings list query across every Work/filter is marked stale and refetched after each status change, assignment change, or inline text edit. For the current Findings page this at least refetches every loaded page of the active infinite query; if multiple findings lists are mounted as active later, it also creates cross-Work refetch churn unrelated to the changed finding. The optimistic cache update already targets the mutated Work (`queryKeys.findings.list(vars.workId)`), so the settle invalidation should be narrowed to the mutated Work's list prefix and paired with direct cache updates from the server response where possible. If status-filter membership requires a refetch, refetch only the affected Work/filter scope rather than all findings lists globally.
  - Source Type: deep-lens: Performance Lens
  - Source Reference: `apps/web/src/api/queries.ts:247-254` (work-scoped snapshot/update) vs `apps/web/src/api/queries.ts:288-292` (global `queryKeys.findings.lists()` invalidation); `apps/web/src/lib/nexus/query-keys.ts:32-39` (hierarchical findings keys)
  - Confidence: High
  - Suggested fix: replace the settle invalidation with a work-scoped prefix such as `queryKeys.findings.list(vars.workId)` (or a predicate over `['findings','list', vars.workId, ...]`), then update the changed finding from the returned `FindingDetailResponse` before any necessary work-scoped refetch.

### 🟢 Suggestion

- **S-QC3-P0-001 (low, Suggestion)** — Add a regression test that mounts two active findings list queries for different Work IDs and verifies updating one Work does not refetch the other. The existing mutation tests prove optimistic update, rollback, and refetch-on-settle for a single list, but they intentionally do not catch cross-Work invalidation breadth.
  - Source Type: deep-lens: Testing Lens
  - Source Reference: `apps/web/src/api/findings-mutation.test.tsx:58-139`
  - Confidence: Medium

## Source Trace
- Finding ID: W-QC3-P0-001
- Source Type: deep-lens: Performance Lens
- Source Reference: `apps/web/src/api/queries.ts:247-292`; `apps/web/src/lib/nexus/query-keys.ts:32-39`
- Confidence: High

- Finding ID: S-QC3-P0-001
- Source Type: deep-lens: Testing Lens
- Source Reference: `apps/web/src/api/findings-mutation.test.tsx:58-139`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

Revalidation: Approve

## Detailed Review Notes (qc3 lens)

### Query-key granularity and invalidation
- Findings keys are hierarchical: `['findings','list', workId, query]` and `['findings','detail', workId, findingId]`.
- `onMutate` uses the correct Work-scoped prefix for snapshots and optimistic updates: `queryKeys.findings.list(vars.workId)`.
- `onSettled` widens back to `queryKeys.findings.lists()`, which drops the Work ID and invalidates all findings list queries. This is broader than the mutation's data dependency and is the Request Changes driver.

### Optimistic-flow reliability
- Rollback path is present and explicit: `onError` restores every snapshotted list and surfaces an error toast.
- Pending state disables status/assignment/edit controls while the mutation is in flight, so the UI does not leave the author on a hung editable state during a daemon/network failure.
- Targeted test coverage exists for the 422 rejection path: `findings-mutation.test.tsx` verifies optimistic status flips back and the list refetches after rejection.

### Stale-count refresh
- The web client has no `NexusClient` method or TanStack query for `GET /v1/local/findings/stale`; `grep` under `apps/web/src` found no `stale_count`, `findings/stale`, or `StaleFindingsResponse` consumer.
- Therefore this P0 web implementation does not currently render a stale-count signal that can become stale after resolving a finding. If a future UI adds such a signal, it should get its own `queryKeys.findings.stale(...)` entry and be invalidated by status-changing mutations.

### Bundle size / dependency impact
- No heavy new dependency was added; the new P0 modules use React/TanStack Query and existing UI primitives.
- `pnpm --filter web build` succeeded. Largest production chunk remains `dist/assets/tiptap-CXIgA64u.js` at 437.82 kB (gzip 146.48 kB), below the 500 kB warning threshold; findings additions did not introduce a new oversized chunk.

### Verification evidence
- `pnpm --filter web build` — pass (`tsc --noEmit && vite build`).
- `pnpm --filter web test -- findings-mutation finding-detail-panel findings-lifecycle` — pass (3 files, 21 tests).

**Conclusion (qc3)**: The optimistic error path and bundle footprint are acceptable, but the global findings-list invalidation creates avoidable refetch churn and should be narrowed before approval from the performance/reliability lens.

## Revalidation (after targeted fix)

- Re-review date: 2026-06-30
- Fix commits reviewed: `da68e7b4` (`fix(v1.77): narrow useUpdateFinding invalidation to mutated Work scope`)
- W-QC3-P0-001: RESOLVED — `apps/web/src/api/queries.ts:288-295` now invalidates `queryKeys.findings.list(vars.workId)` on settle, not the global `queryKeys.findings.lists()` prefix.
- Regression evidence: `apps/web/src/api/findings-mutation.test.tsx:140-208` mounts `w1` and `w2`, mutates `w1`, waits for `w1` to refetch, and asserts `w2` remains at one fetch.
- Verification: `pnpm --filter web run test -- findings-mutation` — pass (40 files, 285 tests; relevant file included).
- Updated verdict: Approve
