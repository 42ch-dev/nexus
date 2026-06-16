---
report_kind: qc_consolidated
plan_id: "2026-06-14-v1.46-author-desk-status-ux"
verdict: "Request Changes"
generated_at: "2026-06-14"
pm_decision_round: 1
review_range: "merge-base: de30a702 → tip: c9fb1abb (5 commits on iteration/v1.46)"
---

# V1.46 P0 QC Consolidated Report

## Reviewer Decision Matrix

| Reviewer | Seat focus | Verdict | 🔴 Critical | 🟡 Warning | 🟢 Suggestion |
|---|---|---|---|---|---|
| `@qc-specialist` (#1) | Architecture coherence & maintainability | **Request Changes** | 0 | 2 (W-1, W-2) | 3 (S-1, S-2, S-3) |
| `@qc-specialist-2` (#2) | Security & correctness | **Approve** | 0 | 0 | 1 (S-1) |
| `@qc-specialist-3` (#3) | Performance & reliability | **Request Changes** | 0 | 3 (F-001, F-002, F-003) | 3 (S-001, S-002, S-003) |
| **Total** | — | **Request Changes** | **0** | **5** | **7** |

Per `mstar-review-qc` gate rule (Critical>0 or Warning>0 ⇒ Request Changes): **gate fails — code fix round required.**

## CI gates (all three reviewers)

- `cargo clippy --all -- -D warnings` — clean (zero warnings in scope)
- `cargo test -p nexus42 --lib -- 'works::tests'` — 40 passed, 0 failed
- `cargo +nightly fmt --all --check` — clean
- No CI failures attributable to review scope.

## PM Disposition (this round)

### Fix in this round (5 items — dispatch to `@fullstack-dev`)

| ID | Source | File | Action |
|---|---|---|---|
| F-001 | qc3 | `crates/nexus42/src/commands/creator/works/mod.rs:374-392` | Parallelize findings + stale fetch via `tokio::join!` (avoid ~35 s stacked worst-case on JSON hot path) |
| F-002 | qc3 | `works/mod.rs:385-388` | Short timeout on stale fetch (e.g. add `STALE_FETCH_TIMEOUT = 5 s` mirroring `FINDINGS_FETCH_TIMEOUT`); reuse the findings-timeout client or a dedicated one |
| F-003 | qc3 | `works/mod.rs:1073-1112`, `1129-1157` | Add consumer-visible `findings_truncated: true` (or `findings_total_count`) when `findings.len() == FINDINGS_FETCH_LIMIT` (50); cover with new test |
| S-1 | qc1 | `works/mod.rs:1310-1312` | Remove dead `let _ = work_id;` and its 2-line comment (cheap cleanup while in file) |
| S-001 | qc3 | `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md:69-71` | Correct plan §6 Verification command: `cargo test -p nexus42 --lib -- 'works::tests'` (current `cargo test -p nexus42 -- works_status` matches zero tests) |

### Defer to P1 (spec sweep, no residual needed)

| ID | Source | Where | Note |
|---|---|---|---|
| W-1 | qc1 | `.mstar/knowledge/specs/novel-writing/author-experience.md` §4.1 line 146 (table) | Reconcile "Required: yes" with omission-on-unreachable three-state contract. P1 `2026-06-14-v1.46-spec-cli-hygiene` is chartered for spec sweep (compass §1.1 row 3) and already touches §4.1. |
| W-2 | qc1 | `novel-writing/author-experience.md` §4.1 line 147 (table) | Clarify `findings_stale` is creator-global scope (not work-scoped); mirror human-path stale-banner separation. Same P1 sweep applies. |

### Track as open low-severity residuals (registered in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-author-desk-status-ux"]`)

| ID | Source | Where | Decision | Target |
|---|---|---|---|---|
| R-V146P0-QC1-S2 | qc1 S-2 | `works/mod.rs` production (1264-1313) vs test helper (1485-1529) | defer | V1.46+ |
| R-V146P0-QC2-S1 | qc2 S-1 | `works/mod.rs:1638-1752` (`enrich_*` tests) + helper at 1411 | defer | V1.46+ |
| R-V146P0-QC3-S2 | qc3 S-002 | `works/mod.rs:380-388` (silent degradation paths) | defer | V1.46+ |
| R-V146P0-QC3-S3 | qc3 S-003 | `works/mod.rs:379-389` (skip stale when findings fail) | defer | V1.46+ |

### Already covered by F-002 fix (no separate residual)

- qc1 S-3 (timeout asymmetry) — addressed by F-002 short-timeout introduction

## Plan status update

- `.mstar/status.json` → `plans[1].status`: `Todo` → **`InReview`** (this round = QC fix cycle)
- New `residual_findings["2026-06-14-v1.46-author-desk-status-ux"][]` array added (4 open residuals; W-1/W-2 deliberately excluded since they migrate to P1 scope).

## Next steps (PM)

1. **Fix dispatch**: assign the 5 "Fix in this round" items to `@fullstack-dev` on a new topic branch `feature/v1.46-p0-qc-fixes` from `iteration/v1.46` HEAD (`c9fb1abb` + qc-consolidated commit). Implementer merges back to `iteration/v1.46` after the fix is validated.
2. **Targeted re-review**: after fix lands, dispatch `qc-specialist` + `qc-specialist-3` (N=2) in ONE message with `QC re-review: targeted — reviewers: qc-specialist, qc-specialist-3`. `qc-specialist-2` stays **Approve** (no rework).
3. **QA**: after targeted re-review passes, dispatch `@qa-engineer` for final verification with same `plan_id` + `Review range / Diff basis` (revised to cover fix round).
4. **Done**: PM marks P0 `Done` after QA pass + clears/archives residuals (or leaves open per their target).

## Scope discipline reminder (downstream)

- Fixers and re-reviewers MUST NOT widen scope beyond the 5 listed items + scope-amend lines for F-003 spec cross-ref.
- Pre-existing quickstart refs in the human stale banner remain P1 scope (not flagged here).

## Evidence

- qc1: `.mstar/plans/reports/2026-06-14-v1.46-author-desk-status-ux/qc1.md` (committed `f981eaf2`)
- qc2: `.mstar/plans/reports/2026-06-14-v1.46-author-desk-status-ux/qc2.md` (committed `dc607d8d`)
- qc3: `.mstar/plans/reports/2026-06-14-v1.46-author-desk-status-ux/qc3.md` (committed `932524d8`)
- Plan: `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md`
- Compass: `.mstar/iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md`