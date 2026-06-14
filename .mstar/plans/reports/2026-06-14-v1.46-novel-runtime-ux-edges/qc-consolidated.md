---
report_kind: qc_consolidated
plan_id: "2026-06-14-v1.46-novel-runtime-ux-edges"
verdict: "Request Changes"
generated_at: "2026-06-15"
pm_decision_round: 1
review_range: "merge-base: ab3312e2 → tip: 008e6bd8 (7 commits on iteration/v1.46)"
---

# V1.46 P2 QC Consolidated Report

## Reviewer Decision Matrix

| Reviewer | Seat focus | Verdict | 🔴 Critical | 🟡 Warning | 🟢 Suggestion |
|---|---|---|---|---|---|
| `@qc-specialist` (#1) | Architecture coherence & maintainability | **Approve** | 0 | 0 | 2 (S-1, S-2) |
| `@qc-specialist-2` (#2) | Security & correctness | **Approve** *(seat-level judgment: Warning low practical risk)* | 0 | 1 (manifest description sanitization) | 0 |
| `@qc-specialist-3` (#3) | Performance & reliability | **Request Changes** | 0 | 1 (W-001: per-chapter `exists()` latency) | 2 (S-001, S-002) |
| **Total** | — | **Request Changes** | **0** | **2** | **4** |

**Strict-gate reading**: qc2 raised 1 Warning with seat-level Approve (defense-in-depth, local-only input). Per `mstar-review-qc` strict rule, Warning>0 ⇒ Request Changes. **PM honors qc2's seat-level judgment** (low practical risk, defense-in-depth consistency item) and treats the qc2 Warning as a deferred residual rather than a blocking fix. The blocking fix is qc3 W-001.

## CI gates (all three reviewers)

- `cargo clippy --all -- -D warnings` — clean (zero warnings)
- `cargo test -p nexus42` — 840 passed, 0 failed (818 baseline + 22 new: 6 T1 + 11 T2 + 5 T3 e2e)
- `cargo +nightly fmt --all --check` — clean
- 5 mechanical ACs all verified (chapter ⚠ hint, 3 preset `--help` injections, test coverage, residual R-V139P5-N1 + R-V145B1-002 addressed)

## PM Disposition (this round)

### Fix in this round (1 item — dispatch to `@fullstack-dev`)

| ID | Source | File | Action |
|---|---|---|---|
| **W-001** | qc3 | `crates/nexus42/src/commands/creator/works/mod.rs:1386-1415, 1443-1459` (per-chapter `exists()` loop in `print_chapter_table`) | Add perf mitigation. Pick one (or a small combination) of qc3's 4 options: (1) add `tracing::debug!` / `tracing::info!` span around the hint loop recording chapter count + elapsed_ms, (2) cap hint rendering at a reasonable threshold (e.g. first 50 chapters + summary line "+ N more"), (3) concurrent via `tokio::task::spawn_blocking` (the caller is async), (4) document in `novel-author-experience.md` or crate AGENTS. **Recommended: option (1) + (2)** — observability for the common case + cap to prevent tail latency. Add a test for the cap behavior. |

### Defer to low-severity open residuals (5 items — registered in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-novel-runtime-ux-edges"]`)

| ID | Source | Where | Decision | Target | Note |
|---|---|---|---|---|---|
| R-V146P2-QC2-W | qc2 Warning (qc2 marked Approve) | `crates/nexus42/src/commands/creator/run.rs` (`format_preset_run_help`) | defer | V1.46+ | Manifest description text rendered verbatim without `sanitize_for_terminal`. Defense-in-depth; local user YAML only. Apply same sanitization T1 uses for chapter-hint copy. |
| R-V146P2-QC1-S1 | qc1 S-1 | `crates/nexus42/src/commands/creator/run.rs:231-235` (`std::process::exit(0)` in library module) | defer | V1.46+ | Mitigated by extracted pure helpers + e2e tests. Low priority. |
| R-V146P2-QC1-S2 | qc1 S-2 | config-resolution dedup | defer | V1.46+ | Pre-existing duplication; new code is cleaner; out of P2 scope. |
| R-V146P2-QC3-S1 | qc3 S-001 | `run.rs:231-235` (stdout flush before `exit(0)`) | defer | V1.46+ | Line-buffered terminals typically flush on newline; low practical risk. |
| R-V146P2-QC3-S2 | qc3 S-002 | `run.rs:214` (`CapabilityRegistry::with_builtins()` rebuilt per help intercept) | defer | V1.46+ | Negligible today; future scalability. |

### Already addressed (R-V139P5-N1 + R-V145B1-002 closure tracked in P-last)

- T1 implementation addresses **R-V139P5-N1** (chapter body_path hint). Closure in `status.json` deferred to P-last per plan §4.5.
- T2 implementation addresses **R-V145B1-002** (cli_args in --help for first-slice presets). Closure in `status.json` deferred to P-last per plan §4.5.

## Plan status update

- `.mstar/status.json` → `plans[3].status`: `Todo` → **`InReview`** (this round = QC fix cycle)
- New `residual_findings["2026-06-14-v1.46-novel-runtime-ux-edges"][]` array added (5 open low-severity residuals)

## Next steps (PM)

1. **Fix dispatch**: assign W-001 to `@fullstack-dev` on a new topic branch `feature/v1.46-p2-qc-fixes` from `iteration/v1.46` HEAD (`008e6bd8` + qc-consolidated commit). Implementer merges back after fix validated.
2. **Targeted re-review**: dispatch `qc-specialist-3` only (N=1) in ONE message with `QC re-review: targeted — reviewers: qc-specialist-3`. qc-specialist + qc-specialist-2 stay **Approve** (no rework).
3. **QA**: after targeted re-review passes, dispatch `@qa-engineer` for final verification with same `plan_id` + revised `Review range / Diff basis` (cover the fix round).
4. **Done**: PM marks P2 `Done` after QA pass + leaves 5 residuals open per their targets.

## Scope discipline reminder (downstream)

- Fixers and re-reviewers MUST NOT widen scope beyond W-001. The 4 Suggestions are explicitly out of this round's scope.
- 4 P0 + 9 P1 = 13 existing open residuals remain tracked in their respective `residual_findings` arrays; do not address them in this round.

## Evidence

- qc1: `.mstar/plans/reports/2026-06-14-v1.46-novel-runtime-ux-edges/qc1.md` (committed `d7651d63`)
- qc2: `.mstar/plans/reports/2026-06-14-v1.46-novel-runtime-ux-edges/qc2.md` (committed `4074c531`)
- qc3: `.mstar/plans/reports/2026-06-14-v1.46-novel-runtime-ux-edges/qc3.md` (committed `71705298`)
- Plan: `.mstar/plans/2026-06-14-v1.46-novel-runtime-ux-edges.md`
- Compass: `.mstar/iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md`