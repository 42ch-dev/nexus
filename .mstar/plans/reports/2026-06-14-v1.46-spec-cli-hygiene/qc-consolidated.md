---
report_kind: qc_consolidated
plan_id: "2026-06-14-v1.46-spec-cli-hygiene"
verdict: "Request Changes"
generated_at: "2026-06-15"
pm_decision_round: 1
review_range: "merge-base: 1f92016f → tip: acabca53 (8 commits on iteration/v1.46)"
---

# V1.46 P1 QC Consolidated Report

## Reviewer Decision Matrix

| Reviewer | Seat focus | Verdict | 🔴 Critical | 🟡 Warning | 🟢 Suggestion |
|---|---|---|---|---|---|
| `@qc-specialist` (#1) | Architecture coherence & maintainability | **Request Changes** | 0 | 1 (W-1) | 3 (S-1, S-2, S-3) |
| `@qc-specialist-2` (#2) | Security & correctness | **Approve** | 0 | 0 | 2 (S-1, S-2) |
| `@qc-specialist-3` (#3) | Performance & reliability | **Approve** | 0 | 0 | 4 (S-1, S-2, S-3, S-4) |
| **Total** | — | **Request Changes** | **0** | **1** | **9** |

Per `mstar-review-qc` gate rule: Critical>0 or Warning>0 ⇒ **Request Changes**. Gate fails — code/spec fix round required.

## CI gates (all three reviewers)

- `cargo clippy --all -- -D warnings` — clean
- `cargo test --all` — 99 `test result: ok` blocks, 0 failures (incl. renamed `completion_guard_message_cites_spec_paths` at `schedules.rs:1583-1591`, and 4 renamed `preset_gates` remediation tests)
- `cargo +nightly fmt --all --check` — exit 0
- 4 mechanical ACs (plan §4 + §6) — all PASS

## PM Disposition (this round)

### Fix in this round (1 item — dispatch to `@fullstack-dev`)

| ID | Source | File | Action |
|---|---|---|---|
| **W-1** | qc1 | `.mstar/knowledge/specs/cli-command-ia.md:67` | Drop the AC-filter-gaming annotation `(Removed in V1.45: replaces creator run start; see changelog)` from the active `creator bootstrap` row. Replace with a non-stale phrasing (e.g. `Composite Work onboarding (V1.45 generic runner; see changelog)`) so the line passes AC2 without relying on the exclusion filter. The hard-delete note on line 70 already records that `start` was removed; the active-entry row does not need to restate it. |

After fix, AC2 must STILL pass: `rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'` → zero hits. (The new phrasing has no stale command token, so it passes the filter organically.)

### Defer to low-severity open residuals (9 items — registered in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-spec-cli-hygiene"]`)

| ID | Source | Where | Decision | Target | Note |
|---|---|---|---|---|---|
| R-V146P1-QC1-S1 | qc1 S-1 | `creator-run-preset-entry.md:110` | defer | V1.46+ | Filter-gaming sibling of W-1; Shipped Master requires spec amend path per `knowledge/specs/AGENTS.md` (Master normative changes need plan/ADR). Borderline parse; clarifying clause prevents firm misread. |
| R-V146P1-QC1-S2 | qc1 S-2 | `crates/nexus42/src/errors.rs:262` (`daemon_not_reachable_quickstart`) | defer | V1.46+ | Stale function name retains "quickstart" suffix; mechanical rename ripples to call sites + tests. Pure rename refactor. |
| R-V146P1-QC1-S3 | qc1 S-3 | `creator-workflow.md` §3.2 table | defer | V1.46+ | Anachronistic V1.33 column now shows V1.41+ command. Restore historical value or relabel column. |
| R-V146P1-QC2-S1 | qc2 S-1 | `schedules.rs:1583-1591` test | defer | V1.46+ | Completion-guard test is a string-snapshot of the multi-line error. Brittle to copy changes; consider semantic assertion (severity/keyword instead of full-string match). |
| R-V146P1-QC2-S2 | qc2 S-2 | `preset_gates.rs` 4 remediation tests | defer | V1.46+ | Remediation_* tests are `contains(...)` snapshots of 3 helper functions. Same brittleness as S-1. |
| R-V146P1-QC3-S1 | qc3 S-1 | `preset_gates.rs` intake_status remediation | defer | V1.46+ | Remediation suggests `creator bootstrap --preset creative-brief-intake` but the command does not complete intake on an existing Work. Verify remediation command actually achieves the intended action. |
| R-V146P1-QC3-S2 | qc3 S-2 | (test file) | defer | V1.46+ | Remediation tests assert spec filenames but not command validity. See S-1 above. |
| R-V146P1-QC3-S3 | qc3 S-3 | (test name) | defer | V1.46+ | Stale test name referencing deleted quickstart. |
| R-V146P1-QC3-S4 | qc3 S-4 | runtime messages | defer | V1.46+ | Runtime strings cite repo-internal `.mstar/...` paths (consider user-facing alternative for product users). |

### Already covered by qc1 S-1 in filter-gaming scope (not separate)

- n/a (S-1 covers the pattern)

## Plan status update

- `.mstar/status.json` → `plans[2].status`: `Todo` → **`InReview`** (this round = QC fix cycle)
- New `residual_findings["2026-06-14-v1.46-spec-cli-hygiene"][]` array added (9 open low-severity residuals).

## Next steps (PM)

1. **Fix dispatch**: assign W-1 to `@fullstack-dev` on a new topic branch `feature/v1.46-p1-qc-fixes` from `iteration/v1.46` HEAD (`acabca53` + qc-consolidated commit). Implementer merges back to `iteration/v1.46` after the fix is validated.
2. **Targeted re-review**: after fix lands, dispatch `qc-specialist` only (N=1) in ONE message with `QC re-review: targeted — reviewers: qc-specialist`. `qc-specialist-2` and `qc-specialist-3` stay **Approve** (no rework).
3. **QA**: after targeted re-review passes, dispatch `@qa-engineer` for final verification with same `plan_id` + revised `Review range / Diff basis` (cover the fix round).
4. **Done**: PM marks P1 `Done` after QA pass + leaves residuals open per their targets.

## Scope discipline reminder (downstream)

- Fixers and re-reviewers MUST NOT widen scope beyond the 1 listed item. Pre-existing filter-gaming sibling in `creator-run-preset-entry.md:110` is a Shipped Master amend (deferred to residual per knowledge/AGENTS.md Master amendment rules).
- 9 low-severity residuals are tracked in status.json — do not address them in this round; they have V1.46+ targets.

## Evidence

- qc1: `.mstar/plans/reports/2026-06-14-v1.46-spec-cli-hygiene/qc1.md` (committed `bba8bfe3`)
- qc2: `.mstar/plans/reports/2026-06-14-v1.46-spec-cli-hygiene/qc2.md` (committed `9c9a3e76`)
- qc3: `.mstar/plans/reports/2026-06-14-v1.46-spec-cli-hygiene/qc3.md` (committed `916b5022`)
- Plan: `.mstar/plans/2026-06-14-v1.46-spec-cli-hygiene.md`
- Compass: `.mstar/iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md`