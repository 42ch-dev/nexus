---
plan_id: 2026-06-20-v1.53-capability-registry-unification
working_branch: feature/v1.53-capability-registry-unification
review_cwd: main worktree
review_range: 71dc6b1d..69594902
consolidation_date: 2026-06-20
gate_verdict: Request Changes
gate_state: pending-fix-wave
---

# QC Consolidated — V1.53 P0 CapabilityRegistry Unification

**Plan**: `2026-06-20-v1.53-capability-registry-unification`
**Branch**: `feature/v1.53-capability-registry-unification`
**Range**: `71dc6b1d..69594902`
**Date**: 2026-06-20

## Reviewer verdicts

| QC | Reviewer index | Focus | Verdict |
|---|---|---|---|
| qc1 | 1 | architecture/maintainability | **Request Changes** |
| qc2 | 2 | security/correctness | Approve with Notes |
| qc3 | 3 | performance/reliability | Approve with Notes |

## Gate verdict (PM consolidation)

**Request Changes** — qc1 reported two medium findings that directly affect P0 acceptance ("registry is the runtime SSOT for id/admission/handler/..."). qc2 and qc3 approve the implementation but raise accepted follow-ups that are recorded as residuals.

## Findings summary

### Must-fix (qc1 Request Changes, blocking P0 acceptance)

- **R-V153P0QC1-001** (medium): Registry is not the runtime SSOT for supported IDs/admission because `TOOL_ALLOWLIST` remains a separate runtime list (`host_tool_executor.rs:45-50, 171-173`). P1 adding 5 new tools would require updating both `TOOL_ALLOWLIST` and `host_tool_registry()`, creating drift risk.
  - **Action**: Fix in fix-wave. Either (a) derive the allowlist from registry rows, or (b) add a strict cross-validation test that fails on any mismatch.
- **R-V153P0QC1-002** (medium): `handler_test_vector.test_fn_name = "schedule_status_returns_ids"` is stale — no matching test function exists (`capability_registry.rs:372-375, 491-510`). Spec §2.1 mandates every `test_fn_name` corresponds to an actual test function.
  - **Action**: Fix in fix-wave. Either rename to an existing schedule-status test name or add the missing test; add static accepted-name enforcement.

### Accepted as residuals (qc2 + qc3 + fullstack-dev self-report)

These are recorded in `status.json.residual_findings[2026-06-20-v1.53-capability-registry-unification]` and are deferred to follow-up plans:

| ID | Severity | Title | Target |
|---|---|---|---|
| R-V153P0QC2-001 | medium | Parity coverage narrow (only 2 of 8 tools have parity tests) | P1 (DF-46 read slice) will add coverage |
| R-V153P0QC2-002 | medium | Cross-validation only checks prefix, not catalog↔registry id bijection | Add test in P1 or follow-up |
| R-V153P0QC3-001 | medium | Per-dispatch registry allocation on schedule hot path | Deferred optimization plan + benchmark |
| R-V153P0-001 | low | Caching opportunity (merged with R-V153P0QC3-001) | Same plan |
| R-V153P0QC2-003 | low | No concurrent dispatch test | Future hardening |
| R-V153P0QC2-004 | low | No separate Schedule caller-kind admission test | Future hardening |
| R-V153P0QC3-002 | low | Missing dispatch-latency benchmark | Same optimization plan |
| R-V153P0QC3-003 | low | Admission vectors could be `&'static [AdmissionGate]` instead of `Vec` | Same optimization plan |
| R-V153P0-002 | low | DaemonToolDispatchAdapter documentation (by-design delegation) | Document in spec/code comment |

### Nits (acknowledged, not residual)

- HRTB on `RegistryHandlerFn` would benefit from a tiny example showing why `for<'a>` is required (qc1)
- Verification command block in qc3 assignment references outdated path (`crates/.../host_tool_executor.rs` vs `crates/.../api/handlers/host_tool_executor.rs`) — template correction for future reviews

## Next steps

1. **Fix-wave**: `@fullstack-dev` addresses R-V153P0QC1-001 + R-V153P0QC1-002 in a single commit on the same branch.
2. **Targeted qc1 re-review**: same `qc1.md` (no `-rev2` per V1.45 spec hygiene) with focus on the two fixed findings.
3. **qc2 / qc3**: no re-review (already approved).
4. **After qc1 re-approves**: PM marks P0 Done; merge `feature/v1.53-capability-registry-unification` → `iteration/v1.53`; dispatch P1.