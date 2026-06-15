---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-15-v1.47-serial-completion-hardening"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-15

## Scope
- plan_id: 2026-06-15-v1.47-serial-completion-hardening
- Review range / Diff basis: `merge-base: c549eec7215215dc4d67a724602db827f26f9927 + tip: HEAD` (i.e. `git diff c549eec7215215dc4d67a724602db827f26f9927..HEAD` from the worktree)
- Working branch (verified): feature/v1.47-serial-completion-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p2-serial
- Files reviewed: 3 implementation files (plan doc + 2 test files); 1 concurrent-reviewer artifact (`qc2.md`) excluded from scope
- Commit range (implementation): `4b83852b..06ba126f` (3 commits); HEAD advanced to `c144b64e` during review due to concurrent qc2 report commit on the shared branch — implementation commits unchanged
- Tools run: `cargo +nightly fmt --all -- --check`, `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings`, `cargo test` scoped per Assignment

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- (none)

### 🟢 Suggestion

- **S-1 — Test #3 docstring slightly overstates §6.1 condition coverage.**
  `spec_4_5_7_completion_requires_all_section_6_1_conditions` (in
  `crates/nexus-local-db/src/work_chapters.rs`) declares in its top docstring:
  *"completion fires only when every row is `finalized`, `current_chapter >=
  total_planned_chapters`, and `intake_status == complete` (§6.1) … then each
  condition individually violated returns false."* In practice only **two** of
  the three §6.1 conditions are violated in the test body (`intake_status`, and
  one-row-unfinalized). The `current_chapter >= total_planned_chapters`
  condition is **set** via raw SQL (`UPDATE works SET current_chapter = 2`) but
  **never violated as a failure mode** — because the V1.44 P2 volume-aware
  `is_work_completed` (`work_chapters.rs:767`) deliberately replaced the flat
  `current_chapter >= total` check with a row-count + finalized-count proxy
  (comment at lines 802–805) so that chapter numbers resetting across volumes
  do not break completion. The inline comment at the raw-SQL site is honest
  about this ("current_chapter is not directly checked here"), but the top
  docstring could mislead a future maintainer into believing `current_chapter`
  is an enforced gate.
  -> Recommend either (a) tightening the docstring to state that the
  volume-aware implementation uses row-count as the proxy for the
  `current_chapter >= total` clause, or (b) raising a spec-reconciliation
  residual so §6.1 of `novel-workflow-profile.md` documents the V1.44 P2
  divergence (spec text still literally says `current_chapter >=
  total_planned_chapters`). This is a pre-existing spec↔implementation seam
  surfaced — not introduced — by P2; low impact.

- **S-2 — R-V138P1-01 lifecycle mirror still pending PM action at merge.**
  The plan's T5 memo dispositions R-V138P1-01 as *"guard sufficient — close"*
  with sound evidence (guard `reject_produce_when_novel_complete` @
  `crates/nexus42/src/commands/creator/run.rs:502`, called at `run.rs:716`
  before `build_schedule_for_stage`; 3 unit tests @ `run.rs:949–994`; auto-chain
  `NoAction` for completed Works @ `auto_chain.rs:273`; no gaps found). The
  memo correctly states *"PM to mirror lifecycle:resolved in status.json at
  merge time."* At review time, R-V138P1-01 remains `decision: "defer"` with no
  `lifecycle`/`resolution` fields in
  `residual_findings["2026-06-08-v1.38-novel-writing-parameterization"]`. This
  is **not** a defect in this plan — QC never owns `status.json` residual
  lifecycle writes (`mstar-plan-artifacts`), and the handoff is explicit. AC2
  is satisfied at the evidence level. Flagging only as a merge-time
  confirmation: PM/QA must execute the mirror so AC2 closes in the machine
  SSOT, otherwise the residual will appear still-open after the iteration
  lands.

## Source Trace

- Finding ID: S-1
  - Source Type: manual-reasoning + git-diff + spec-read
  - Source Reference: `crates/nexus-local-db/src/work_chapters.rs` (new test
    `spec_4_5_7_completion_requires_all_section_6_1_conditions`, docstring vs
    test body); `is_work_completed` @ `work_chapters.rs:767–823`; spec
    `.mstar/knowledge/specs/novel-workflow-profile.md` §6.1 (line 807+) and
    §4.5.2
  - Confidence: High

- Finding ID: S-2
  - Source Type: git-diff + json-read
  - Source Reference: plan §5 "T5 Residual Disposition R-V138P1-01";
    `.mstar/status.json` `residual_findings["2026-06-08-v1.38-novel-writing-parameterization"]`
    (id `R-V138P1-01`, `decision: "defer"`, no `lifecycle`/`resolution`)
  - Confidence: High

## Architecture Notes (Reviewer #1 focus)

- **Spec↔test alignment (§4.5.7 #1–#3):** All three acceptance tests map
  cleanly to their spec items and are present at the correct seams:
  - #1 `spec_4_5_7_chapter_selection_returns_lowest_eligible`
    (`nexus-local-db/work_chapters.rs`) — exercises the full §4.5.2 selection
    ladder (not_started → outlined → draft → finalize → None) in one flow. The
    "outlined beats later draft" assertion correctly reflects the
    implementation's `status IN ('not_started','outlined','draft')` +
    `MIN(chapter)` query (`next_chapter` @ `work_chapters.rs:589`) and the
    §4.5.2 normative note that an outlined row must not be skipped in favour of
    a later chapter.
  - #2 `spec_4_5_7_current_chapter_advances_only_on_finalize`
    (`nexus-orchestration/capability/builtins/novel_chapter_transition.rs`) —
    drives the real `NovelChapterTransition` capability with a pool and asserts
    the §4.5.2 work-level invariant (`current_chapter` advances only on
    `finalized`, takes the just-finalized number, never skips ahead on draft).
    Placing this test in the capability that *enforces* the invariant
    (`novel_chapter_transition.rs:234`) is the correct seam — it tests the
    enforcement point end-to-end, not just the DB column.
  - #3 `spec_4_5_7_completion_requires_all_section_6_1_conditions`
    (`nexus-local-db/work_chapters.rs`) — asserts the positive §6.1 flow and
    two negative branches (see S-1 for the third).

- **Test seam design:** Tests #1/#3 target pure DB-layer functions
  (`next_chapter`, `is_work_completed`) in `nexus-local-db`; test #2 targets
  the orchestration capability that mutates `works.current_chapter`. This
  split is coherent with the §4.1.2 truth model (DB is SSOT) and avoids
  cross-crate coupling in the wrong direction.

- **T1 baseline review memo quality:** Line-number citations are accurate
  (verified guard @ `run.rs:502`, call site @ `run.rs:716` *before*
  `build_schedule_for_stage`, auto-chain `NoAction` @ `auto_chain.rs:273`,
  `WorkComplete` paths in `evaluate_after_persist` / `…_volume_aware`). The
  memo correctly distinguishes pre-existing coverage (T10.1–T10.8) from the
  canonical §4.5.7 tests added here, and explicitly notes T2–T4 change **no**
  runtime behavior. AC3 is satisfied by the pre-existing guard + auto-chain
  paths, re-confirmed green here.

- **AC3 (no spurious `novel-writing` schedule when `next_chapter=None`):**
  Verified by `reject_produce_when_novel_complete_*` (3/3 pass) and the full
  `auto_chain` suite (23/23 pass, incl. `work_already_completed_no_action`,
  `persist_complete_last_chapter_marks_work_complete`). No runtime path
  enqueues a produce schedule for a completed Work.

- **Diff is test-only:** The two source-file diffs are pure additions under
  `#[cfg(test)] mod tests` (work_chapters.rs +228 under tests; novel_chapter_transition.rs
  +173 under tests, lines 442–613). No production/runtime code changed, consistent
  with the plan's stated non-goal of not duplicating the V1.39 P5 guard.

- **Concurrent tri-review note:** qc-specialist-2 committed `qc2.md`
  (`c144b64e`) to the shared `Working branch` during this review, advancing
  HEAD past the Assignment's verified `06ba126f`. The three implementation
  commits (`4b83852b`, `73ee447a`, `06ba126f`) are unchanged; this review's
  scope is those commits plus the plan doc. The shared-branch tri-review
  pattern is acceptable per `mstar-branch-worktree` (reviewers commit reports
  to the same feature branch); qc2.md content is out of this reviewer's scope.

## Validation Evidence

```
$ cargo +nightly fmt --all -- --check
(no output — clean)

$ cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s
(no warnings)

$ cargo test -p nexus-local-db --lib -- work_chapters   → 28 passed; 0 failed
   (incl. spec_4_5_7_chapter_selection_returns_lowest_eligible … ok
          spec_4_5_7_completion_requires_all_section_6_1_conditions … ok)

$ cargo test -p nexus-orchestration --lib -- current_chapter → 1 passed; 0 failed
   (spec_4_5_7_current_chapter_advances_only_on_finalize … ok)

$ cargo test -p nexus-orchestration --lib -- completion      → 7 passed; 0 failed
$ cargo test -p nexus-orchestration --lib -- auto_chain::    → 23 passed; 0 failed
$ cargo test -p nexus42 --lib -- reject_produce_when_novel_complete → 3 passed; 0 failed
```

No P2-introduced clippy items; no P2-introduced test failures. Pre-known
baseline items (per Assignment) were not re-flagged:
`master_decision_timeout::repeated_sweeps_remain_stable` flake and pre-existing
baseline clippy items.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: **Approve**

All three acceptance criteria are met:
- **AC1** — 3 new tests cover §4.5.7 #1–#3 at the correct architectural seams;
  all pass.
- **AC2** — R-V138P1-01 dispositioned "close" with traceable evidence in the
  T5 memo; the `status.json` lifecycle mirror is correctly deferred to PM at
  merge (S-2 is a merge-time confirmation, not a plan defect).
- **AC3** — No spurious `novel-writing` schedule when `next_chapter=None`;
  guard + auto-chain paths verified green; diff is test-only (no runtime
  change).

No Critical or Warning findings. The two Suggestions are low-impact
maintainability/spec-accuracy observations (one pre-existing spec↔impl seam
surfaced by a test docstring; one residual-lifecycle mirror handoff) and do
not block approval.
