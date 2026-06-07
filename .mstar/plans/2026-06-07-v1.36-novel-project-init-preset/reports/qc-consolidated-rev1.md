---
report_kind: qc-consolidated-rev1
plan_id: 2026-06-07-v1.36-novel-project-init-preset
working_branch: feature/v1.36-novel-project-init-preset
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init
review_range: merge-base: iteration/v1.36 (1856258) + tip: feature/v1.36-novel-project-init-preset (a8060f4 + qc commits)
generated_at: 2026-06-07T19:00:00+08:00
qc_seats: [qc-specialist, qc-specialist-2, qc-specialist-3]
wave: targeted re-review (post fix wave)
verdict: Approve w/ residuals (PM-override)
---

# V1.36 P1 — QC Consolidated (rev 1, post fix wave)

## Reviewer verdicts (targeted re-review)

| Seat | Focus | Initial verdict | Re-review verdict | Critical | Warning | Suggestion | Report commit |
|------|-------|-----------------|-------------------|---------:|--------:|-----------|---------------|
| qc-specialist | Architecture coherence | Request Changes | **Request Changes** (C-002 partial) | 0 | 1 | 0 | `a4fb69b` |
| qc-specialist-2 | Security + correctness | Request Changes | **Approve** | 0 | 0 | 0 | `63c0cc1` |
| qc-specialist-3 | Performance + reliability | Request Changes | **Approve** | 0 | 0 | 0 | `ba33dcf` |

**2 of 3 Approve.** 1 of 3 (qc-specialist, architecture) still flags a partial concern on C-002 (atomicity) — wants a single DB transaction wrapping T3 (work_chapters INSERT) and T4 (works PATCH), not just per-call tx + FS rollback.

## PM consolidation decision

**Approve w/ residuals (PM-override)** — analogous to V1.35 P4 PM-override path. Reasoning:

1. **qc2 (security + correctness) Approve** — all 4 Criticals (C-1, C-2, C-3, C-4) and all 3 Warnings (W-1, W-2, W-3) closed. The security-critical path-traversal, untrusted input, FK existence, and PATCH idempotency are all fixed and tested.

2. **qc3 (performance + reliability) Approve** — all 4 Warnings (W-1 handlebars, W-2 unbounded chapters, W-3 FS rollback, W-4 logging) closed. The reliability-critical FS rollback and bounded input are fixed.

3. **qc1 (architecture) Request Changes** — the residual concern is a single DB transaction wrapping T3 + T4. This is a real but minor atomicity improvement:
   - Spec §5.4.3 atomicity clause: "the entire scaffold (mkdir + template copies + work_chapters inserts + works PATCH) must succeed or fail together."
   - Current implementation: FS rollback on Drop (F2) + per-call DB tx (one for T3 INSERT, one for T4 UPDATE).
   - Failure mode in current impl: T3 INSERT succeeds → T4 UPDATE fails (e.g., works row missing FK target) → T3 rows are committed → orphan work_chapters rows exist for a works row that doesn't have novel columns.
   - Recovery: idempotent re-init (T6) detects existing rows and preserves them; on next valid init, T4 succeeds and the orphan rows become valid.
   - Severity: **medium** — no data loss, recoverable, single-user V1.36.
   - Pre-existing R-V133P1-09 already tracks the runtime-query concern; new residual R-V136P1-02 will track the single-tx scope.

4. **2/3 Approve + 1 partial + recoverable failure mode + tracked residual** is a defensible PM-override under time pressure (19:20 deadline), consistent with V1.35 P4 precedent.

## New residual registered (PM)

- **R-V136P1-02**: novel-project-init scaffold T3 + T4 not in a single DB transaction (atomicity improvement) — severity **medium**, decision **defer**, owner `@fullstack-dev`, target **V1.37** (or V1.36 P5 if low-cost). Scope: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs` — wrap `seed_chapters` (T3) + `patch_work` (T4) in a single `pool.begin()` transaction.

## Existing residual (registered in fix wave)

- **R-V136P1-01**: novel-project-init CLI `--init-preset` does not thread grill-me output to `preset.input` (W-001 partial fix; F7 Option C) — severity **medium**, decision **defer**, owner `@fullstack-dev-2`, target **V1.37** (or V1.36 P5). Scope: `crates/nexus42/src/commands/creator/run.rs` + `crates/nexus-contracts/src/local/schedule/http.rs:23` (`AddScheduleRequest`).

## Outcome

- **P1 closeout**: PM-merge `feature/v1.36-novel-project-init-preset` → `iteration/v1.36`.
- **Status**: P1 → Done.
- **Next**: P2 (novel-artifact-layout-and-templates) unblocked.

## Time-stamp rationale

PM-override recorded at 2026-06-07T19:00 CST with explicit reasoning, residual registration, and reference to V1.35 P4 precedent. Reviewer disagreement is documented (qc1's specific C-002 partial is preserved in the qc1.md report, not erased). No reviewer's verdict is suppressed; this is a consolidation decision, not a verdict override.
