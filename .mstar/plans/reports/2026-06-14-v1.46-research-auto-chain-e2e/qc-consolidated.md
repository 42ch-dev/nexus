---
report_kind: qc_consolidated
plan_id: "2026-06-14-v1.46-research-auto-chain-e2e"
verdict: "Request Changes"
generated_at: "2026-06-15"
pm_decision_round: 1
review_range: "merge-base: 1d776d23 → tip: 87f00619 (2 commits on iteration/v1.46)"
---

# V1.46 P3 QC Consolidated Report

## Reviewer Decision Matrix

| Reviewer | Seat focus | Verdict | 🔴 Critical | 🟡 Warning | 🟢 Suggestion |
|---|---|---|---|---|---|
| `@qc-specialist` (#1) | Architecture coherence & maintainability | **Approve** | 0 | 0 | 2 (S-1, S-2) |
| `@qc-specialist-2` (#2) | Security & correctness | **Approve** | 0 | 0 | 0 |
| `@qc-specialist-3` (#3) | Performance & reliability | **Request Changes** | 0 | 1 (W-1) | 2 (S-1, S-2) |
| **Total** | — | **Request Changes** | **0** | **1** | **4** |

Per `mstar-review-qc` gate rule: Critical>0 or Warning>0 ⇒ **Request Changes**. Gate fails — code fix round required.

## Pre-existing clippy status (PM-override, independently verified by all 3 QCs)

- **PM-override**: `R-V145-PRE-CLIPPY-001` (`.mstar/status.json` `residual_findings["pre-existing-v1.45-orchestration-clippy"]`), `decision: risk-accepted`.
- **Independent verification** by all 3 QCs against `origin/main` HEAD `63b36a32`: same ~60+ `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` failures reproduce in `tasks/mod.rs` + `worker/registry.rs` + 2 test files. PM-override is **VALID**.
- These pre-existing failures are **NOT** raised as V1.46 P3 findings.

## CI gates (all three reviewers, on `iteration/v1.46` HEAD `cae189c5`)

- `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed, 0 failed
- `cargo test --all` → all green
- `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` → clean (P3 file in isolation)
- `cargo +nightly fmt --all --check` → clean
- Pre-existing clippy independently verified (above)

## PM Disposition (this round)

### Fix in this round (1 item — dispatch to `@fullstack-dev`)

| ID | Source | File | Action |
|---|---|---|---|
| **W-1** | qc3 | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:255-270` | Replace `format!("{g:?}").contains(...)` substring checks in `research_preset_loads_and_structurally_valid` with **typed pattern matching** on `nexus_contracts::local::orchestration::preset_gate::Gate` variants and public fields. Assert that one `Gate::WorkField { field: "intake_status", op: GateOp::Equals { value: "complete" } }` exists and that a `Gate::WorkField { field: "work_ref", op: GateOp::Required }` (or equivalent) exists. This fix also resolves qc1 S-2 (Debug-substring match brittleness). |

### Defer to low-severity open residuals (3 items — registered in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-research-auto-chain-e2e"]`)

| ID | Source | Where | Decision | Target | Note |
|---|---|---|---|---|---|
| R-V146P3-QC1-S1 | qc1 S-1 | `research_supervisor_e2e.rs:184` (assertion) + `:123` (seed SQL) | defer | V1.46+ | `preset_version = 2` magic number duplicated. Couple enforced only by author discipline. Consider exporting the version constant from the preset loader. |
| R-V146P3-QC3-S1 | qc3 S-1 | `research_supervisor_e2e.rs:108-135` (raw SQL insert) | defer | V1.46+ | Raw SQL fixture duplicates schema knowledge. Wrap in DAO-style helper or reuse `auto_chain::insert_driver_schedule` with a `preset_id` parameter. At minimum, add migration-file reference comment. |
| R-V146P3-QC3-S2 | qc3 S-2 | `research_supervisor_e2e.rs:138-145` (`schedule_status` helper) | defer | V1.46+ | `fetch_one(...).unwrap()` panics on missing row. Return `Option<String>` and let caller `expect` with descriptive message. |

### Already addressed (R-V139P5-S1 closure tracked in P-last)

- T1+T2 implementation addresses **R-V139P5-S1** (supervisor+boot E2E). Lifecycle closure in `status.json` deferred to P-last per plan §4 AC4.

## Plan status update

- `.mstar/status.json` → `plans[4].status`: `Todo` → **`InReview`** (this round = QC fix cycle)
- New `residual_findings["2026-06-14-v1.46-research-auto-chain-e2e"][]` array added (3 open low-severity residuals)

## Next steps (PM)

1. **Fix dispatch**: assign W-1 to `@fullstack-dev` on a new topic branch `feature/v1.46-p3-qc-fixes` from `iteration/v1.46` HEAD (`cae189c5`). Implementer merges back after fix validated.
2. **Targeted re-review**: dispatch `qc-specialist-3` only (N=1) in ONE message with `QC re-review: targeted — reviewers: qc-specialist-3`. qc-specialist + qc-specialist-2 stay **Approve** (no rework).
3. **QA**: after targeted re-review passes, dispatch `@qa-engineer` for final verification.
4. **Done**: PM marks P3 `Done` after QA pass + leaves 3 residuals open per their targets.

## Scope discipline reminder (downstream)

- Fixers and re-reviewers MUST NOT widen scope beyond W-1. The 3 Suggestions are explicitly out of this round's scope.
- 4 P0 + 9 P1 + 5 P2 = 18 existing open residuals remain tracked; do not address them in this round.
- Pre-existing clippy (R-V145-PRE-CLIPPY-001) is the V1.46 P-last hygiene round's responsibility, not P3.

## Evidence

- qc1: `.mstar/plans/reports/2026-06-14-v1.46-research-auto-chain-e2e/qc1.md` (committed `4135a467`)
- qc2: `.mstar/plans/reports/2026-06-14-v1.46-research-auto-chain-e2e/qc2.md` (committed `412f37cb`)
- qc3: `.mstar/plans/reports/2026-06-14-v1.46-research-auto-chain-e2e/qc3.md` (committed `f215772e`)
- Plan: `.mstar/plans/2026-06-14-v1.46-research-auto-chain-e2e.md`
- Compass: `.mstar/iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md`
- Pre-existing residual: `.mstar/status.json` `residual_findings["pre-existing-v1.45-orchestration-clippy"]` (`R-V145-PRE-CLIPPY-001`, `cae189c5`)