---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-17-v1.49-author-desk-ux
verdict: Approve
generated_at: 2026-06-16T23:45:00Z
review_range: c993ad15..1fa8002
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (focus per role parameters)
- Report Timestamp: 2026-06-16T23:45:00Z

## Scope
- plan_id: 2026-06-17-v1.49-author-desk-ux
- Review range / Diff basis: c993ad15..1fa8002
- Working branch (verified): iteration/v1.49
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 10 (per Assignment + diff --stat)
- Commit range: 1fa80021 (merge) + 4 P2 feature commits + harness/status
- Tools run: git rev-parse/branch/diff/log, Read (core impl + tests + overlay §8), Grep (call sites, path validation, intake), cargo check (scoped, non-blocking for review)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1 (Correctness / policy gap — intake re-trigger without gates)**: `handle_intake` (CLI) + schedule POST path explicitly documents and implements "the `creative-brief-intake` preset declares no gates, so the existing schedule-add handler accepts it on any existing Work bound via `input.work_id`". Each call creates a fresh schedule row (new `schedule_id`). There is no check that the Work's `intake_status` is not already `complete`, nor that a prior intake schedule is still pending/running. A user/script can queue duplicate intake runs. This is **intentional per overlay §8.1** and the implementer comment, but it is a correctness surface risk (accidental duplicate work, or in a future multi-creator scenario, resource consumption). The existence GET + creator binding from config prevents cross-creator abuse, but does not prevent re-trigger on one's own Work.
  - Evidence: `crates/nexus42/src/commands/creator/works/mod.rs:1031-1091` (handle_intake + explicit GET + AddScheduleRequest with `input.work_id`), schedules.rs:206-221 (work_id resolution for gates), author-experience.md:219 ("the preset declares no gates... accepts it on any existing Work").
  - Fix direction: Future hardening could add a warning (or `--force`) when `intake_status=complete` or a non-terminal intake schedule already exists for the Work. Not a silent bug for P2.

- **W-2 (Correctness / TOCTOU on dry-run preview)**: The `--dry-run` path in `reconcile_chapters` (daemon) deliberately skips `RuntimeLockGuard::acquire` (per design and overlay §8.2). The report is computed from a point-in-time FS walk + DB snapshot; counters are proven accurate for that snapshot (see test), but between the preview and a later mutating call, another CLI/daemon instance can mutate chapter rows or files. The mutating path still acquires the lock and does the real work, but the preview is not a stable "plan" the user can rely on under concurrency.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1540-1559` (dry-run branch, no lock), `work_chapters.rs:608-638` (all writes gated on `!dry_run` while counters always incremented), `runtime_lock.rs:371-509` (test proving zero mutations + report equivalence + no lock acquired + subsequent mutate works), author-experience.md:235 ("no runtime lock acquire").
  - This is an explicit documented trade-off for "preview without side effects." Acceptable for local-first single-user, but worth surfacing as a Warning for users who script around the report.

### 🟢 Suggestion
- **S-1 (Test coverage — negative paths)**: Current tests for the new surfaces (`creator_works.rs`) are only help-text / clap-parse surface tests. No wiremock or hermetic negative tests for: oversized/malformed `work_id` containing `..` / control chars / NUL, daemon unreachable, malformed JSON from daemon, or `work_ref` that would trigger `PathEscape`. The daemon handler does call `is_valid_work_ref` + `verify_stories_dir_in_workspace` before any FS work, and the DB layer has the canonicalize prefix check — but those paths are not exercised by negative tests in scope.
  - Evidence: `crates/nexus42/tests/creator_works.rs:168-244` (only help + flag presence), `runtime_lock.rs` (positive dry-run + lock-release scenarios only).
  - Recommendation: Add at least one negative case per new surface (e.g., `works intake 'wrk_../evil'` should fail cleanly without leaking paths; reconcile with invalid work_ref should hit the `INVALID_WORK_REF` 400).

- **S-2 (Defense-in-depth hygiene)**: `is_valid_work_ref` (handler) and `verify_stories_dir_in_workspace` (DB layer) are good layered checks. Consider promoting the slug validation to a shared newtype (`WorkRef`) with `TryFrom<String>` so future call sites cannot forget the check. Not required for P2 correctness.

- **S-3 (Overlay fidelity)**: §8.1 and §8.2 in `author-experience.md` accurately reflect the shipped behavior (existence check + remediation cite, dry-run skips lock, report counters match mutate, `--yes` policy, no driver cancellation). No drift detected.

## Source Trace
- Finding ID: QC2-2026-06-17-P2-001 (W-1 intake policy gap)
- Source Type: manual-reasoning + code review + overlay text
- Source Reference: works/mod.rs:1022 (comment), 1068 (input binding), schedules.rs:325-358 (gate bypass only for force), author-experience.md:219
- Confidence: High

- Finding ID: QC2-2026-06-17-P2-002 (W-2 dry-run TOCTOU)
- Source Type: code review + test evidence
- Source Reference: works.rs:1540 (dry-run early return), work_chapters.rs:530 (dry_run param), runtime_lock.rs:464-477 (lock not acquired), author-experience.md:235
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

**Rationale**: Zero Critical findings. The two Warnings are explicit, documented design choices (no gates on creative-brief-intake re-trigger; dry-run deliberately skips the runtime lock for preview safety) rather than hidden correctness or security bugs. Creator scoping is preserved (config `active_creator_id` + explicit Work GET before schedule), path validation is present on every reconcile path (handler + DB canonicalize), lock hygiene on the mutating path follows the V1.42.1 hotfix pattern, and the overlay §8 text matches the implementation. Negative-path test coverage is thin (surface only), but the production guards are in place. Suitable for merge; the Warnings can be tracked as future hardening residuals if desired.

## De-duplication Note
This review focused exclusively on the security/correctness items enumerated in the Assignment (handle_intake auth/ownership, intake scheduling without gates, dry-run race/lock, ReconcileReport fidelity, CLI arg validation, path safety/TOCTOU, error disclosure, negative tests, idempotency, overlay §8 correctness). Architecture/maintainability concerns are left to qc1; performance/reliability to qc3. No sibling qc1.md/qc3.md were present at review time.
