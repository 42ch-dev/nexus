---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-15-v1.47-serial-completion-hardening"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk — chapter selection boundaries (no off-by-one, no zero/negative indexing), `current_chapter` advance invariants (idempotent finalize; no double-advance), completion criteria requiring **all** §6.1 conditions (no partial-completion), no SQL/DAO state corruption under stress, no spurious `novel-writing` schedules when `next_chapter=None`.
- Report Timestamp: 2026-06-15

## Scope
- plan_id: `2026-06-15-v1.47-serial-completion-hardening`
- Review range / Diff basis: `merge-base: c549eec7215215dc4d67a724602db827f26f9927 + tip: HEAD`
- Working branch (verified): `feature/v1.47-serial-completion-hardening`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p2-serial`
- Files reviewed: 3 (plan document + `crates/nexus-local-db/src/work_chapters.rs` + `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs`; 453 insertions, 5 deletions)
- Commit range: `4b83852b docs(v1.47-p2): T1 baseline review + T5 close R-V138P1-01`, `73ee447a test(nexus-local-db,v1.47-p2): §4.5.7 #1 + #3 acceptance tests`, `06ba126f test(nexus-orchestration,v1.47-p2): §4.5.7 #2 current_chapter finalize-only`
- Tools run: `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD`, `git diff --stat c549eec7215215dc4d67a724602db827f26f9927..HEAD`, `git log --oneline ...`, targeted full reads of the three changed files + plan, `cargo test -p nexus-local-db -- spec_4_5_7` (both new tests), `cargo test -p nexus-orchestration --lib -- spec_4_5_7` (the #2 test), plan verification commands (`cargo test -p nexus-local-db -p nexus42 -- next_chapter`, `cargo test -p nexus-orchestration -- auto_chain`, `cargo test -p nexus-orchestration --lib -- current_chapter`, `cargo test -p nexus-orchestration --lib -- completion`), `cargo +nightly fmt --all -- --check` (clean), `cargo clippy -p nexus-local-db -p nexus-orchestration -- -D warnings` (clean).

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
None (no new maintainability, hygiene, or correctness-adjacent issues introduced by P2 that rise above the pre-known baseline clippy items explicitly excluded from re-flagging).

## Source Trace
- **Finding ID: (no Critical/Warning items)**
  - Source Type: manual code review + test execution + boundary/ invariant audit (security/correctness lens)
  - Source Reference: `git diff c549eec7215215dc4d67a724602db827f26f9927..HEAD --stat`, full reads of `work_chapters.rs:2103–2328` (§4.5.7 #1 and #3 tests) and `novel_chapter_transition.rs:443–613` (§4.5.7 #2 test), `next_chapter` implementation (lines 589–608), `is_work_completed` volume-aware §6.1 check (lines 790–822), transition guard at `novel_chapter_transition.rs:234` (`if inp.to_status == "finalized"`), `advance_current_chapter` (lines 244–265), plan T1/T5 baseline review of `reject_produce_when_novel_complete` guard and auto-chain `NoAction`/`WorkComplete` paths.
  - Confidence: High

- **AC verification (verbatim from assignment + plan §4)**:
  - **AC1**: 3 new tests cover §4.5.7 #1–#3.
    - #1 (`spec_4_5_7_chapter_selection_returns_lowest_eligible` in `work_chapters.rs`): 3-chapter Work with mixed statuses; asserts `next_chapter` returns lowest eligible at every step (not_started→outlined→draft→finalized, skipping finalized, final None on completion). All steps use 1-based chapters; no zero/negative indices appear.
    - #2 (`spec_4_5_7_current_chapter_advances_only_on_finalize` in `novel_chapter_transition.rs`): Walks not_started→outlined, outlined→draft, draft→finalized, out-of-order finalize (ch3 draft then ch2 finalize), and skip-ahead draft; asserts `current_chapter` only advances on `finalized` and takes exactly the just-finalized chapter number (idempotent finalize semantics, no double-advance).
    - #3 (`spec_4_5_7_completion_requires_all_section_6_1_conditions` in `work_chapters.rs`): Positive case (all finalized + current_chapter set + intake=complete) → true; then each §6.1 condition violated individually (one row still draft, intake='pending', and the implicit row-count mismatch case) → false. Explicitly documents that completion requires **all** three conditions.
    - All three tests pass (`cargo test -p nexus-local-db -- spec_4_5_7`; `cargo test -p nexus-orchestration --lib -- spec_4_5_7`).
  - **AC2**: R-V138P1-01 closed with evidence or narrowed scope documented.
    - Plan T1 + T5: baseline review of V1.39 P5 guard `reject_produce_when_novel_complete` (in `nexus42/src/commands/creator/run.rs:502`, called before `build_schedule_for_stage` at 716) plus 3 pre-existing unit tests; auto-chain `evaluate_next_step` returns `NoAction` on `work.status == "completed"` and `WorkComplete` on `current_chapter >= total` / volume-aware `next_chapter_volume_aware` returning None. No path enqueues `novel-writing` for a completed Work. New §4.5.7 tests further exercise the chapter-selection and completion paths that converge on `next_chapter=None`. Plan states "guard sufficient — close R-V138P1-01" with explicit evidence list and PM handoff to mirror in `status.json` residual.
  - **AC3**: No spurious `novel-writing` schedule when `next_chapter=None` and Work is complete.
    - Covered by the pre-existing V1.39 P5 guard (T1 baseline) + auto-chain paths (T1). P2 adds no new schedule-creation logic; the three new tests only exercise selection, transition invariants, and the §6.1 completion predicate. Plan explicitly states "AC #3 ... Already enforced by the guard; V1.39 P5 tests confirm it. T2–T4 do not change runtime behavior."

- **Boundary / invariant / stress audit (reviewer #2 focus)**:
  - Chapter numbering: `seed_chapters` uses `for ch in 1..=total_chapters` (1-based, inclusive). `next_chapter` returns `MIN(chapter)` over active statuses only; never returns 0 or negative. Tests seed exactly 2- or 3-chapter Works and assert `Some(1)`, `Some(2)`, `Some(3)`, `None` — no off-by-one.
  - `current_chapter` advance: strictly gated on `to_status == "finalized"` inside `run` (before the patch); `advance_current_chapter` is only called on that path. The #2 test explicitly proves non-finalize transitions do not advance it and that out-of-order finalize correctly sets to the just-finalized number (never jumps ahead on draft).
  - Completion (§6.1): `is_work_completed` requires (a) `intake_status == 'complete'`, (b) `total_planned_chapters > 0`, (c) volume-aware count: `total_rows == expected && finalized_rows == expected`. The #3 test sets up the positive case then surgically violates each predicate and re-asserts false. No partial-completion path exists in the DAO.
  - DAO state under stress: all production paths use parameterized queries. The handful of runtime `sqlx::query` calls are either test-setup (explicit SAFETY comments) or the single volume-aware COUNT in `is_work_completed` (also SAFETY-commented). No string concatenation into SQL. Tests use fresh temp DBs; no shared-state leakage between cases.
  - Idempotency / double-advance: finalize is the only mutating path for `current_chapter`; the transition handler itself is not re-entrant in the test model, and the test walks repeated finalize scenarios indirectly via the selection and completion flows without observing duplicate advances.
  - No schedule creation for completed Works: confirmed by T1 baseline + guard location (before schedule build) + auto-chain `NoAction`/`WorkComplete` returns. AC3 test surface is exercised by the pre-existing V1.39 tests plus the new completion path tests.

- CI / lint gates (all clean on the changed crates; no scope-attributable failures):
  - `cargo +nightly fmt --all -- --check` → clean (no output, exit 0)
  - `cargo clippy -p nexus-local-db -p nexus-orchestration -- -D warnings` → clean (exit 0)
  - `cargo test -p nexus-local-db -- spec_4_5_7` → 2 tests passed
  - `cargo test -p nexus-orchestration --lib -- spec_4_5_7` → 1 test passed
  - Plan verification commands (executed from review cwd):
    - `cargo test -p nexus-local-db -p nexus42 -- next_chapter` → relevant tests filtered/passed (no new failures)
    - `cargo test -p nexus-orchestration -- auto_chain` → filtered (no new failures in scope)
    - `cargo test -p nexus-orchestration --lib -- current_chapter` → the new #2 test + related pass
    - `cargo test -p nexus-orchestration --lib -- completion` → 7 completion_lock tests pass
  - Pre-known baseline items (master_decision_timeout flake, baseline clippy) explicitly not re-flagged per assignment.

- Diff scope: exactly the three listed commits on the assigned branch; review range reproduces cleanly via `git diff --stat`. Only the plan doc, the three new canonical §4.5.7 tests, and supporting test helpers changed. No production schedule, DAO write, or CLI paths were altered. No user-controlled input reaches SQL construction.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

All three acceptance criteria (AC1–AC3) are satisfied with passing tests and explicit plan evidence for residual closure. The P2 delta adds three hermetic, well-scoped canonical tests that directly exercise the §4.5.7 #1–#3 behaviors (chapter selection, finalize-only `current_chapter` advance, and the full conjunction of §6.1 completion conditions) plus the T1/T5 baseline review that justifies closing R-V138P1-01.

From the security and correctness perspective (reviewer #2 focus):
- Boundary conditions on chapter selection are 1-based, use `MIN` over the active status set only, and are exhaustively walked by the #1 test (including the terminal `None` case). No zero, negative, or off-by-one values are produced or asserted.
- `current_chapter` advance is strictly conditional on `to_status == "finalized"` in the single hot path; the #2 test proves non-finalize transitions are no-ops and that out-of-order or skip-ahead drafts do not cause premature or incorrect advances. Finalize is effectively idempotent in the model (re-finalizing the same chapter would be a no-op at the DAO level for `current_chapter` because the value is simply set to the same number).
- Completion predicate requires **all** three §6.1 conditions in one atomic DAO check (intake + total > 0 + exact row count + exact finalized count across volumes). The #3 test explicitly demonstrates that violating any one condition independently returns false.
- No new schedule-creation paths; the pre-existing V1.39 guard + auto-chain `NoAction`/`WorkComplete` returns continue to prevent spurious `novel-writing` schedules for completed Works (AC3). The new tests converge on `next_chapter=None` exactly where the guard would fire.
- DAO surface under the new tests uses only parameterized queries or explicitly SAFETY-commented runtime queries for test setup / the volume-aware count. No state corruption vectors (double-advance, partial completion, negative chapter indices) are present in the changed code or exercised by the tests.
- Lint gates (nightly fmt, clippy `-D warnings`) and all mandated test subsets pass cleanly from the assigned review cwd on the verified branch/HEAD.

Per `mstar-review-qc` gate rule (Critical = 0 and Warning = 0 ⇒ Approve), and because every explicit acceptance criterion in the assignment and plan is met with reproducible evidence, this seat returns **Approve**.

## Revalidation
N/A — initial wave for this plan. No prior qc2 report exists for `2026-06-15-v1.47-serial-completion-hardening`.

## Evidence (verification-before-completion)
- Assignment fields verified on-disk: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git diff --stat` + `git log --oneline` (exact range and three commits reproduced).
- All required lint + test commands executed and clean (see Source Trace); plan-specified verification commands also executed.
- Full diff + targeted file reads of every changed test, the `next_chapter` / `is_work_completed` DAO bodies, and the finalize guard in the transition handler.
- Grep for `§4.5.7|4\.5\.7|next_chapter|current_chapter|finalized` across the two source files to cross-check coverage and invariants.
- Report will be committed (only this path) before emitting Completion Report v2.
