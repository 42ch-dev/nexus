---
report_kind: qc_consolidated
plan_id: "2026-06-14-v1.46-pool-observability"
verdict: "Request Changes"
generated_at: "2026-06-15"
pm_decision_round: 1
review_range: "merge-base: 417f81f2 → tip: 8e85432e (5 commits on iteration/v1.46)"
---

# V1.46 P4 QC Consolidated Report

## Reviewer Decision Matrix

| Reviewer | Seat focus | Verdict | 🔴 Critical | 🟡 Warning | 🟢 Suggestion |
|---|---|---|---|---|---|
| `@qc-specialist` (#1) | Architecture coherence & maintainability | **Request Changes** | 0 | 1 (W-1) | 2 (S-1, S-2) |
| `@qc-specialist-2` (#2) | Security & correctness | **Approve** | 0 | 0 | 1 (pre-existing lint hygiene) |
| `@qc-specialist-3` (#3) | Performance & reliability | **Request Changes** | 0 | 1 (W-01) | 2 (S-1, S-2) |
| **Total** | — | **Request Changes** | **0** | **2** | **5** |

Per `mstar-review-qc` gate rule: Critical>0 or Warning>0 ⇒ **Request Changes**. Gate fails — code fix round required.

## Pre-existing clippy clarification

- **Pre-P4 baseline** (`1d776d23`): 8 pre-existing `cargo clippy -p nexus-local-db --tests -- -D warnings` errors in untouched files (`findings.rs`, `kb_extract_job.rs`, `tests/v142_migration_fixes.rs`, `work_chapters.rs`, `works.rs`).
- **P4-introduced**: 2 errors in the new T3 capture test (`novel_pool_entries.rs:563,565`): `clippy::used_underscore_binding` and `clippy::significant_drop_tightening` on the `_guard` Drop pattern.
- The 8 pre-existing errors are **OUT OF SCOPE** for P4 (PM-override will be issued if/when needed; the V1.46 P-last hygiene plan can sweep them up). qc1 explicitly verified the 8-vs-2 split.
- The 2 P4-introduced errors are **in scope** and must be fixed in this round.

## CI gates (all three reviewers, on `iteration/v1.46` HEAD)

- `cargo test -p nexus-local-db` → 201 passed, 0 failed (191 lib + 8 integration + 2 doc)
- `cargo test --all` → all green
- `cargo clippy -p nexus-local-db --tests -- -D warnings` → 10 errors (8 pre-existing + 2 P4-introduced in T3 test)
- `cargo +nightly fmt --all --check` → clean
- 5 P4 ACs verified by qc2 + qc3 (instrumentation completeness, no-PII, dev-dep scoping, test reliability)

## PM Disposition (this round)

### Fix in this round (1 item — dispatch to `@fullstack-dev-2`)

| ID | Source | File | Action |
|---|---|---|---|
| **W-1 / W-01** | qc1 + qc3 (same issue) | `crates/nexus-local-db/src/novel_pool_entries.rs:563,565` (T3 capture test) | Fix 2 P4-introduced clippy errors on the `_guard` Drop pattern: (a) `used_underscore_binding` — rename `_guard` to `guard` or drop the underscore; (b) `significant_drop_tightening` — wrap the guard in a block to scope its lifetime. No logic change. |

### Defer to low-severity open residuals (4 items — registered in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-pool-observability"]`)

| ID | Source | Where | Decision | Target | Note |
|---|---|---|---|---|---|
| R-V146P4-QC1-S1 | qc1 S-1 | `novel_pool_entries.rs` (T3 capture test) | defer | V1.46+ | Only `promote_to_active` has tracing assertion. Parametrize for the other 8 paths as future hygiene. |
| R-V146P4-QC1-S2 | qc1 S-2 | `novel_pool_entries.rs:550-554` (subscriber construction) | defer | V1.46+ | Verbose UFCS subscriber construction. Idiomatic `registry().with(layer)` preferred. |
| R-V146P4-QC3-S1 | qc3 S-1 | `novel_pool_entries.rs` + `inspiration_items.rs` | defer | V1.46+ | Expand trace coverage beyond `promote_to_active`. |
| R-V146P4-QC3-S2 | qc3 S-2 | all 9 `tracing::info!` sites | defer | V1.46+ | Document expected INFO level intent in module-level doc. |

### Already addressed (R-V141P1-15 closure tracked in P-last; R-V141P1-10 waived per plan §1)

- T1+T2 implementation addresses **R-V141P1-15** (pool tracing). Lifecycle closure deferred to P-last.
- R-V141P1-10 waived per plan §1 (no code added). P-last will add the waive doc note.

## Plan status update

- `.mstar/status.json` → `plans[5].status`: `Todo` → **`InReview`** (this round = QC fix cycle)
- New `residual_findings["2026-06-14-v1.46-pool-observability"][]` array added (4 open low-severity residuals)

## Next steps (PM)

1. **Fix dispatch**: assign W-1 to `@fullstack-dev-2` on a new topic branch `feature/v1.46-p4-qc-fixes` from `iteration/v1.46` HEAD (current). Implementer merges back after fix validated.
2. **Targeted re-review**: dispatch `qc-specialist-3` only (N=1) in ONE message with `QC re-review: targeted — reviewers: qc-specialist-3`. qc-specialist (re-check W-1) and qc-specialist-2 (stays Approve) — actually per mstar-dispatch-gates, "N = listed seats only" so N=1 (qc3 only); qc1 can re-check the fix opportunistically (W-1 was qc1's own finding).
3. **QA**: after targeted re-review passes, dispatch `@qa-engineer` for final verification.
4. **Done**: PM marks P4 `Done` after QA pass + leaves 4 residuals open per their targets.

## Scope discipline reminder (downstream)

- Fixers and re-reviewers MUST NOT widen scope beyond the 2 P4-introduced clippy errors. The 8 pre-existing errors are out of P4 scope (P-last hygiene).
- 4 P0 + 9 P1 + 5 P2 + 3 P3 + 1 pre-existing P3-clippy = 22 existing open residuals remain tracked; do not address them in this round.

## Evidence

- qc1: `.mstar/plans/reports/2026-06-14-v1.46-pool-observability/qc1.md` (committed `531bb9b9`)
- qc2: `.mstar/plans/reports/2026-06-14-v1.46-pool-observability/qc2.md` (committed `6d42531b`)
- qc3: `.mstar/plans/reports/2026-06-14-v1.46-pool-observability/qc3.md` (committed `a874b6c6`)
- Plan: `.mstar/plans/2026-06-14-v1.46-pool-observability.md`
- Compass: `.mstar/iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md`