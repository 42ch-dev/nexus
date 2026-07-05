---
report_kind: qc-consolidated
plan_id: "2026-06-16-v1.48-serial-hardening"
generated_at: "2026-06-16"
pm_decision: "Request Changes → fix wave (P4-fix1)"
verdict_summary: "Approve: qc1, qc2 (lenient — should be Request Changes per gate); Request Changes: qc3"
---

# V1.48 P4 (serial-hardening) — QC Tri-Review Consolidated

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
|----------|-------|---------|----------|---------|------------|
| qc-specialist (qc1) | architecture/maintainability | Approve | 0 | 0 | 3 |
| qc-specialist-2 (qc2) | security/correctness | Approve (lenient — should be Request Changes per gate rule) | 0 | 2 | 4 |
| qc-specialist-3 (qc3) | performance/reliability | **Request Changes** | 0 | 3 | 2 |

**Consolidated verdict**: **Request Changes** (per mstar-review-qc gate: unresolved Warnings).

## Warnings to Fix This Round

| ID | Source | Issue | Decision |
|----|--------|-------|----------|
| W-1 | qc2 | `sync_frontmatter_status` uses non-atomic `std::fs::write`; partial writes can corrupt the file on crash | **Fix** (P4-fix1): write-temp + rename |
| W-1 | qc3 | `ReconcileReport.preserved` counter is misleading for status-conflict rewrites; full-file rewrite is also write-amplification for large Works | **Fix** (P4-fix1): rename counter to `resynced` (or count under `updated`); consider a lighter in-place frontmatter edit (not blocking — defer ring to S-2) |
| W-2 | qc3 | `RuntimeLockGuard` is leaked on the daemon `reconcile_chapters` error path; P4's added file I/O increases exposure | **Fix** (P4-fix1): restructure handler with `match` that releases lock on both arms (V1.42.1 hotfix pattern) |

## Warnings Deferred as Residuals

| ID | Source | Issue | Residual target | Severity |
|----|--------|-------|------------------|----------|
| W-2 | qc2 | `creator works reconcile-chapters` has no `--dry-run` or confirmation surface before mass FS mutation of chapter frontmatter | V1.49 UX plan (P4 was M effort; this is feature work, not bug) | `low` |
| W-3 | qc3 | `reconcile-chapters` is synchronous and holds Work runtime lock for full filesystem walk + DB + file I/O; at 50ms/chapter × 100 chapters ≈ 5s lock-held time | V1.49 (batched / async chunked processing; needs design + handler refactor) | `medium` |

## Suggestions (non-blocking, ack only)

- S-1 (qc1): `sync_frontmatter_status` matcher `line.trim_start().starts_with("status:")` would also match a nested YAML `status:` key (low risk; chapter frontmatter is flat per spec templates)
- S-2 (qc1): `update_status` called in the no-status-change path with `&db_status` + word_count delta; name mildly misleading (pre-existing API; clarify comment)
- S-3 (qc1): Plan verification step `cargo test -p nexus42 -- reconcile` filters to 0 tests (non-blocking; reconcile coverage is in `nexus-local-db`)
- S-1 (qc2): positive findings on path-safety, CLI handler delegation, parameterised queries
- S-1 (qc3): Add explicit idempotency assertion to hermetic test #5 (run reconcile twice + assert `created == 0 && updated == 0 && preserved == N`)
- S-2 (qc3): `sync_frontmatter_status` normalises line endings (CRLF → LF); low priority for pre-1.0 local-first

## PM Action

- Dispatch **P4-fix1** fix wave to `@fullstack-dev-2` covering W-1 (qc2) + W-1 (qc3) + W-2 (qc3).
- Defer W-2 (qc2) as **low** residual and W-3 (qc3) as **medium** residual.
- Re-dispatch **targeted re-review** to `@qc-specialist-2` and `@qc-specialist-3` after the fix wave.
- qc-specialist is clear; no re-review needed from them this round.
