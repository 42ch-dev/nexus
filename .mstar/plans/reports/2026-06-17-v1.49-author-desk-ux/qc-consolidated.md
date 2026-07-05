---
report_kind: qc-consolidated
plan_id: 2026-06-17-v1.49-author-desk-ux
generated_at: 2026-06-17T22:55:00+08:00
review_range: c993ad15..1fa8002
working_branch: iteration/v1.49
qc_reports:
  - .mstar/plans/reports/2026-06-17-v1.49-author-desk-ux/qc1.md (qc-specialist, Request Changes — 1 Warning W-1)
  - .mstar/plans/reports/2026-06-17-v1.49-author-desk-ux/qc2.md (qc-specialist-2, Approve — 0/2 documented-intentional Warnings)
  - .mstar/plans/reports/2026-06-17-v1.49-author-desk-ux/qc3.md (qc-specialist-3, Approve — 0/0/5)
verdict: Request Changes
---

# V1.49 P2 — Author Desk UX QC Consolidated Report

## Verdict: Request Changes (one blocker; two Approve)

QC1 raised 1 Warning (W-1) on a clap help-text / behavior mismatch. QC2 + QC3 approved; their findings are non-blocking under documented assumptions.

## Findings Roll-up

| Severity | qc1 | qc2 | qc3 | Total | Consolidated |
|----------|-----|-----|-----|-------|--------------|
| 🔴 Critical | 0 | 0 | 0 | 0 | — |
| 🟡 Warning | 1 | 0 | 0 | 1 unique | **W-1 (blocking)** |
| 🟢 Suggestion | 4 | 2 | 5 | 11 unique | non-blocking; tracked |

Note: qc2's 2 Warnings are documented as **intentional design choices** per overlay §8 (W-1: intake-without-gates is policy; W-2: dry-run TOCTOU acknowledged because no lock). PM accepts these as documented.

### W-1 — `creator works reconcile-chapters --yes` clap help text over-promises an inline preview that `confirm_reconcile_interactive` never prints (raised by qc1)

- **Location**: `crates/nexus42/src/commands/creator/works/mod.rs::handle_reconcile_chapters` (clap help text for `--yes`)
- **Issue**: Per qc1, the `--yes` flag's help text suggests an inline preview is printed, but `confirm_reconcile_interactive` does not actually print one in the default flow. The overlay §8.2 and the handler docstring are accurate, but the clap `--help` is misleading. This is a **doc/behavior mismatch** isolated to user-visible help text.
- **Fix** (1-line change):
  - Option A: Update the `--yes` help text to "Skip the confirmation prompt for the mutating path" (no preview promise).
  - Option B: Add the preview print in `confirm_reconcile_interactive` (would change the handler behavior — more invasive).
  - **Recommended**: Option A (1-line, no behavior change).
- **Severity**: Warning (not Critical) — the actual handler behavior is correct; the help text is just misleading.

## QC2 documented-intentional warnings (recorded for completeness)

- qc2 W-1: intake re-trigger creates fresh schedule on any existing Work (preset declares no gates). Documented policy in overlay §8.1; PM accepts.
- qc2 W-2: dry-run TOCTOU acknowledged (no lock, point-in-time preview). Documented policy in overlay §8.2; PM accepts.

PM decision: accept both as documented; no fix needed.

## QC3 Suggestions (5) — non-blocking; deferred to V1.50 or P-last

- S-1: Stale test counts in completion report (84 vs 67 etc.) — cosmetic; tests are real and green
- S-2: dry-run report has no `captured_at` timestamp
- S-3: Wiremock happy-path-only for `handle_intake` POST (no 4xx/5xx/timeout coverage)
- S-4: No behavioral lib test for `handle_reconcile_chapters` (only clap-parse tests)
- S-5: Test does not assert "no auxiliary files created" via `read_dir` snapshot

## Residual registration

- **R-V149P2-01 (low)** — clap `--yes` help text over-promises an inline preview (qc1 W-1)
  - **Where**: `crates/nexus42/src/commands/creator/works/mod.rs::handle_reconcile_chapters` (clap arg help)
  - **Decision**: **fix-in-wave** (1-line trivial fix)
  - **Owner**: `@fullstack-dev` (fix wave)
  - **Target**: V1.49 P2 fix wave

## Pre-existing residuals (NOT in this wave)

- **R-V149P0-01** (medium, defer V1.50) — CLI `?status=open` gap (P0 follow-up)
- **R-V149P0-03** (low, defer V1.50) — pre-existing clippy `--all` failure. **QC3 reported this is no longer reproducing** (`cargo clippy --all -- -D warnings` clean on `1fa8002`). PM action: de-prioritize; consider closing at P-last if all V1.49 QC3 verifications stay clean.
- **R-V149P1-01** (low, defer V1.49 P5) — overlay §3 4-col vs template 5-col schema reconciliation
- **R-V149P1-02** (low, defer V1.50) — pre-existing flake in `fallback_warn_includes_chapter_field`
- **R-V147P1-01** (low) — intake re-trigger on existing Work. **Closure evidence**: P2 implementer provided `handle_intake_schedules_creative_brief_intake_on_existing_work` test (commit `0948cb87`) + overlay §8.1 update (commit `9aea7091`). **PM will archive after QA pass.**
- **R-V148P4-W2** (medium) — reconcile preview. **Closure evidence**: `reconcile_from_filesystem(dry_run=true)` + daemon `?dry_run=true` query path + `test_reconcile_chapters_dry_run_makes_zero_mutations` (commit `4ab9e0be`) + `--dry-run/--yes` CLI flags (commit `0948cb87`) + overlay §8.2 update (commit `9aea7091`). **PM will archive after QA pass.**
- **R-V148P4-W3** (medium) — reconcile lock duration. **Out of scope for P2**; in scope for P3.

## Next step

PM dispatches **targeted fix wave** to `@fullstack-dev` for W-1 (1-line clap help text fix). After fix:
- PM merges fix branch to `iteration/v1.49`.
- QC1 does **targeted re-review** (N=1; only qc1 raised blocking). Updates the same `qc1.md` (add `## Revalidation` section, update verdict).
- qc2 + qc3 stay approved.
- After re-review approves: PM dispatches `@qa-engineer` for the QA pass (extended `Review range` to cover the fix).
- After QA passes: PM marks P2 `Done`, archives R-V147P1-01 + R-V148P4-W2 + R-V149P2-01, transitions to P3.

PM notes for tracking:

- New worktree: `.worktrees/v1.49-p2-w1-help-fix` on `fix/v1.49-p2-w1-clap-help`.
- Existing worktrees remain for inspection.
