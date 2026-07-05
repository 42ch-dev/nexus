---
report_kind: qc-consolidated
reviewer: project-manager
plan_id: "2026-07-01-v1.80-memory-review-reliability"
wave: "wave-1 (initial tri-review)"
generated_at: "2026-07-01"
---

# QC Consolidated Decision — P0 Memory Review Reliability

## Tri-review summary

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
|----------|-------|---------|----------|---------|------------|
| qc-specialist (qc1) | Architecture / maintainability | Approve | 0 | 0 | 5 |
| qc-specialist-2 (qc2) | Security / correctness | Approve | 0 | 2 (accepted under threat model) | 2 |
| qc-specialist-3 (qc3) | Performance / reliability | **Request Changes** | 0 | **1 (W-QC3-001, blocking)** | 1 |

## Consolidated verdict: **Request Changes**

P0 is blocked by **qc3 W-QC3-001** (the only unresolved Warning across the three seats). qc1 and qc2 both Approve; qc2's two Warnings (best-effort `delete_pending_by_id` after side-effect; partial-progress no-rollback) are the same underlying "at-least-once under local-only/small-queue threat model" trade-off that qc1 also flagged as F-P0-3 — both reviewers explicitly accepted them under the documented threat model, so they are registered as accepted low residuals, not fix-wave blockers.

## Blocking finding (fix-wave target)

**W-QC3-001 / R-V180P0-QC3-001** — Row-level timeout/failure accounting can report `has_more=false` while an unprocessed pending row remains, so the review pipeline is not reliably drained.

- Root cause: `process_review_batch` increments `processed` immediately after `timeout_at(...)` returns (before checking whether the row action completed/timed out), and `review` derives `has_more` from `processed < processing_slice`. On a timeout/failure of the only/final row, `processed == slice.len()` and `more_in_db == false` → `has_more = false` even though the row is still pending.
- Impact: incompletely closes the "client uncertain-completion handling" axis of R-V178P0-QC3-003.
- Fix: track queue advancement (rows actually completed/deleted) separately from rows attempted; on row-level timeout or action-failure that leaves the row pending, ensure `has_more=true`. Add regression coverage for (1) one-row timeout, (2) timeout/failure on the final row in a batch, (3) a perpetually failing head row.
- Owner: `@fullstack-dev` · Severity: medium · Decision: fix-in-wave.

## Residuals registered (open → `status.json` residual_findings[2026-07-01-v1.80-memory-review-reliability])

| ID | Severity | Source | Decision | Title |
|----|----------|--------|----------|-------|
| R-V180P0-QC3-001 | medium | qc3 W-QC3-001 | fix-in-wave | has_more false-completion when final row times out/fails and remains pending |
| R-V180P0-QC1-001 | low | qc1 F-P0-1 + qc3 S-QC3-001 | accept | memory_review_locks map unbounded per-creator (daemon-lifetime; local single-active-creator) |
| R-V180P0-QC2-001 | low | qc2 W (best-effort delete) + qc1 F-P0-3 | accept | best-effort delete_pending_by_id after side-effect leaves re-processing window (at-least-once under threat model) |

## Next

1. Targeted fix-wave: `@fullstack-dev` → `fix/v1.80-review-has-more-completion` (W-QC3-001 only).
2. qc3 targeted re-review (same `qc3.md`, `## Revalidation`).
3. On qc3 → Approve: QA both tracks, PM marks P0 Done.
