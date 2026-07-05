---
report_kind: qc-consolidated
plan_id: 2026-06-17-v1.49-serial-reliability
generated_at: 2026-06-18T00:45:00+08:00
review_range: cb2d3fde..17414d6
working_branch: iteration/v1.49
qc_reports:
  - .mstar/plans/reports/2026-06-17-v1.49-serial-reliability/qc1.md (qc-specialist, Approve — 0/0/4)
  - .mstar/plans/reports/2026-06-17-v1.49-serial-reliability/qc2.md (qc-specialist-2, Approve — 0/0/2)
  - .mstar/plans/reports/2026-06-17-v1.49-serial-reliability/qc3.md (qc-specialist-3, Approve — 0/0/5)
verdict: Approve
---

# V1.49 P3 — Serial Reliability QC Consolidated Report

## Verdict: **Approve** (no blockers)

All 3 QC reviewers approve. Total: 0 Critical, 0 Warning, 11 unique Suggestion findings (all non-blocking; most are documentation/ergonomics).

## Findings Roll-up

| Severity | qc1 | qc2 | qc3 | Total | Consolidated |
|----------|-----|-----|-----|-------|--------------|
| 🔴 Critical | 0 | 0 | 0 | 0 | — |
| 🟡 Warning | 0 | 0 | 0 | 0 | — |
| 🟢 Suggestion | 4 | 2 | 5 | 11 unique | tracked; non-blocking |

## Notable Suggestions (recorded for completeness)

- **qc1 S-1** — canonical-guard test gap (lexical + canonical; missing symlink and URL-encoded coverage)
- **qc1 S-2** — apply-phase DB writes are non-transactional (intentional; small diff set, single-writer model)
- **qc1 S-3** — extract path-validation helper
- **qc1 S-4** — `CreateChapter` double-write (minor; in code comments)
- **qc2 S-1** — path-guard hardening for URL-encoded/Unicode sources
- **qc2 S-2** — reconcile-diff cost surface (documented under local-first single-writer model)
- **qc3 S-1** — prune default DRY (3-place default; 90 days hardcoded in 3 spots)
- **qc3 S-2** — `too_many_lines` precedent (matches existing `work_chapters`/`rules_runtime` pattern)
- **qc3 S-3** — parse-on-String coupling (256 KiB cap is sufficient)
- **qc3 S-4** — apply-error test scope (narrower than V1.48 P4-fix1 test, intentionally — refocuses on apply-phase error)
- **qc3 S-5** — tracing volume (acquired + released log lines could fold into one)

All Suggestions are non-blocking; defer to V1.50 or P-last as appropriate.

## Notable design decisions (recorded)

- **Lock-scope split**: `reconcile_from_filesystem` split into read-only `compute_reconcile_diff` (unlocked) + write-only `apply_reconcile_diff` (under lock). Stale-diff trade-off accepted under local-first single-writer daemon model. Tracing emits `acquired_at` / `held_ms` for operator observability.
- **Prune default**: 90-day retention; new additive `count_resolved_findings_older_than` DAO function (no existing function modified).
- **Path guard**: lexical + canonical check in `load_and_parse_review_report`; test `load_and_parse_review_report_rejects_path_outside_work_dir` covers `Works/<work_ref>/../../../etc/passwd` shape.
- **P3 surface vs P0 surface**: P3 is **purely additive** to `findings.rs` (no modification to the V1.49 P0 state machine); no P1/P2 logic refactored.

## Residual registration — pre-fix

None (no new residuals from QC).

## Residual closure (PM will archive after QA pass)

- **R-V148P4-W3** (medium, originally 2026-06-16-v1.48-serial-hardening) — reconcile lock duration. **Closure evidence**: `compute_reconcile_diff` unlocked walk + `apply_reconcile_diff` under lock + `test_reconcile_chapters_read_phase_runs_unlocked` + `test_reconcile_chapters_releases_lock_on_error` (apply-phase error preserves V1.48 P4-fix1 guarantee) + tracing `acquired_at`/`held_ms`.
- **R-V148P0-W1** (low, originally 2026-06-16-v1.48-findings-producer) — review-report path guard. **Closure evidence**: lexical + canonical guard in `load_and_parse_review_report` + `load_and_parse_review_report_rejects_path_outside_work_dir` test.

## Pre-existing residuals (NOT in this wave)

- **R-V149P0-01** (medium, defer V1.50) — CLI `?status=open` gap
- **R-V149P0-03** (low, defer V1.50) — pre-existing clippy `--all` failure. **QC3 reports `cargo clippy --all -- -D warnings` clean on `17414d6`** (0.21s incremental; 0 errors). Confirmed machine-specific drift, NOT a V1.49 regression. **PM action**: consider closing at P-last if all V1.49 QC3 verifications stay clean.
- **R-V149P1-01** (low, defer V1.49 P5) — overlay §3 schema reconciliation
- **R-V149P1-02** (low, defer V1.50) — pre-existing flake in `fallback_warn_includes_chapter_field`

## Next step

PM dispatches `@qa-engineer` for the QA pass on the same `Review cwd` + `plan_id` + `Review range` (`cb2d3fde..17414d6`). QA verifies:
1. All 4 P3 acceptance criteria hold.
2. All 11 Suggestions are non-blocking (no new functional regressions).
3. CI gates (with note about R-V149P0-03 — confirmed clean on `17414d6`).
4. No new regressions in the 4 crates in scope.

After QA passes, PM:
- Archives R-V148P4-W3 + R-V148P0-W1.
- Marks P3 `Done`.
- Transitions to P-last (hygiene + closeout + Profile B).
