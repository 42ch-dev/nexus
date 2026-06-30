---
report_kind: qc-consolidated
plan_id: 2026-06-30-v1.77-findings-remediation-ui
iteration: V1.77
wave: initial
verdict: Request Changes
generated_at: 2026-06-30
---

# QC Consolidated — P0 findings-remediation-ui

## Tri-review verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
|----------|-------|---------|----------|---------|------------|
| qc1 (`qc-specialist`) | Architecture + maintainability | Approve | 0 | 0 | 4 |
| qc2 (`qc-specialist-2`) | Security + correctness | Approve | 0 | 0 | 2 |
| qc3 (`qc-specialist-3`) | Performance + reliability | **Request Changes** | 0 | **1** | 1 |

## Blocking item (must fix this round)

- **W-QC3-P0-001** (medium, Warning) — `useUpdateFinding` invalidates the global `queryKeys.findings.lists()` prefix on every mutation → refetches all active findings lists across every Work/filter. Fix: narrow settle invalidation to the mutated Work's list scope (`queryKeys.findings.list(vars.workId)`) + update the changed finding from the returned `FindingDetailResponse`. Source: `apps/web/src/api/queries.ts:288-292`.

## Non-blocking suggestions (defer to V1.78 `tbd-v1.78-qc-followup`)

- S-QC1-P0-001..004 (qc1): `useFinding` forward-staging marker; `finding-detail-panel.tsx` 305-line split; client adjacency golden test; `getQueriesData`/`setQueryData` asymmetry note.
- S-QC2-P0-001..002 (qc2): client-side validation guard (defense-in-depth); detail-panel rollback assertion.
- S-QC3-P0-001 (qc3): cross-Work invalidation-breadth regression test.

## Consolidated verdict

**Request Changes** — one unresolved Warning (W-QC3-P0-001) from qc3.

## Next

Targeted fix (frontend-dev) → qc3 targeted re-review (same `qc3.md`, add `## Revalidation`) → if Approve, consolidated → Approve → QA.
