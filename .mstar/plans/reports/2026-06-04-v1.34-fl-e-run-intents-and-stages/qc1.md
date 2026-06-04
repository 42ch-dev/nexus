---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-fl-e-run-intents-and-stages"
verdict: "Approve"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-05

## Scope
- plan_id: 2026-06-04-v1.34-fl-e-run-intents-and-stages
- Review range / Diff basis: merge-base: origin/main..HEAD on feature/v1.34-fl-e-run-intents-and-stages; 3 P1 commits in scope: 655d71c T1, d379f86 T2+T4, e0e1861 T3
- Working branch (verified): feature/v1.34-fl-e-run-intents-and-stages
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages
- Files reviewed: 15 (8 scoped implementation/test files plus plan/spec/compass and relevant AGENTS.md files)
- Commit range: scoped P1 commits `655d71c^..e0e1861`; assignment diff basis command also inspected as `$(git merge-base HEAD origin/main)..HEAD`
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log --oneline -5`
  - `git diff --stat $(git merge-base HEAD origin/main)..HEAD`
  - `git show --stat --oneline --no-renames 655d71c d379f86 e0e1861`
  - `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -- -D warnings 2>&1 | tail -10`

## Findings

### 🔴 Critical

- **F-QC1-001 — `creator run stage advance` does not create the required FL-E stage schedule and has no active-schedule invariant protection.**  
  Machine severity: `critical`.  
  The P1 implementation only patches `current_stage` / `stage_status` and prints a manual `daemon schedule add` hint. It does not call `POST /v1/local/orchestration/schedules`, does not add `work_id` / `fl_e_stage` metadata to a schedule, and does not atomically check or prevent another active FL-E stage schedule for the same Work. This misses plan acceptance #1 and the new spec invariant “At most one active FL-E stage schedule per Work.”  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:564-612`; spec `creator-workflow-fl-e.md:46-49`, `:88-91`, `:136`; plan `2026-06-04-v1.34-fl-e-run-intents-and-stages.md:25-27`, `:49-53`.  
  Required fix: stage advance should perform the schedule enqueue path (using the normative stage→preset mapping) and guard active FL-E schedules in the daemon/DB layer with an atomic check/insert or equivalent transaction/constraint; update Work stage and schedule linkage consistently.

- **F-QC1-002 — The intake gate checks `stage_status`, not `intake_status`, so “after intake complete” can still be rejected.**  
  Machine severity: `critical`.  
  `stage_advance` fetches `current_stage` and `stage_status` only, stores the latter as `current_status`, and then reports “intake_status is …” based on `stage_status`. New Works default to `current_stage=intake`, `stage_status=pending`; the P1 diff does not link V1.33 intake completion to `stage_status=complete`. Therefore a Work with `intake_status=complete` but default `stage_status=pending` can fail `creator run stage advance --stage research`, contradicting spec acceptance #2 and plan acceptance #1.  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:517-559`; defaults in `crates/nexus-local-db/migrations/20260606_works_stage_columns.sql:4-8`; plan acceptance `:51`; spec `creator-workflow-fl-e.md:80-83`, `:154-156`.  
  Required fix: fetch and validate `intake_status` separately for the intake gate, or explicitly synchronize `stage_status` with intake completion before allowing research; add a regression test for `intake_status=complete` with `stage_status=pending/default`.

### 🟡 Warning

- **F-QC1-003 — Non-force stage advance can skip multiple stages.**  
  Machine severity: `high`.  
  The gate rejects same/backward targets but accepts any target with `target_idx > current_idx`. That permits `research -> persist` without `--force` when research is complete, while the spec states stages are linear and advance from `S` to `S+1` unless `--force`.  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:540-547`; spec `creator-workflow-fl-e.md:64`, `:80-84`.  
  Required fix: require `target_idx == current_idx + 1` when `--force` is absent; reserve larger jumps for `--force` with audit/log output and tests.

### 🟢 Suggestion

- **F-QC1-004 — FL-E stage constants are duplicated across crates.**  
  Machine severity: `low`.  
  `FL_E_STAGES` is currently declared in `nexus42`, `nexus-local-db`, and `nexus-orchestration`; the preset hint table is also re-declared in CLI while orchestration has `STAGE_PRESET_ALLOWLIST`. This is manageable in P1, but it makes future spec changes easy to drift.  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:14-15`, `:597-604`; `crates/nexus-local-db/src/works.rs:806-813`; `crates/nexus-orchestration/src/preset/validation.rs:1522-1594`.  
  Suggested fix: centralize the stage enum/mapping in one shared crate/module (or generated contracts if this becomes wire-facing), and have CLI/DB/orchestration reuse it.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| F-QC1-001 | manual-reasoning + git-diff | `run.rs:564-612`; spec §2 invariant #4, §3.3, §5.3; plan acceptance #1 | High |
| F-QC1-002 | manual-reasoning + git-diff | `run.rs:517-559`; migration defaults; spec §3.3 / §7 | High |
| F-QC1-003 | manual-reasoning + git-diff | `run.rs:540-547`; spec §3.1 / §3.3 | High |
| F-QC1-004 | maintainability review | duplicated constants in `run.rs`, `works.rs`, `validation.rs` | Medium |

## Verification Evidence

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages

$ git branch --show-current
feature/v1.34-fl-e-run-intents-and-stages

$ git log --oneline -5
e0e1861 feat(orchestration): V1.34 FL-E T3 — stage→preset allowlist validation
d379f86 feat(cli): V1.34 FL-E T2+T4 — creator run stage + status extensions
655d71c feat(local-db,daemon): V1.34 FL-E T1 — stage columns on works
f7bc294 harness(v1.34-p0): mark P0 Done, compact to plans-done.json
b1738e0 merge: P0 — V1.33+ residual convergence + V1.33 compass hygiene

$ git diff --stat $(git merge-base HEAD origin/main)..HEAD
18 files changed, 2187 insertions(+), 321 deletions(-)

$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
clippy_exit=0
```

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

### Scope and commands re-run

Targeted re-review of fix wave 2 for the original QC1 findings C-1, C-2, W-1, and S-1.

- Review cwd / Worktree path verified:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages`
  - `git branch --show-current` → `feature/v1.34-fl-e-run-intents-and-stages`
- Review ranges used:
  - Fix wave 2: `c3834ce..6cd1409` (8 commits inspected with `git show <hash>`)
  - Overall P1 context: `merge-base: origin/main..HEAD`
- Commits inspected:
  - `c3834ce` — R-FL-E-08 deduplicates `FL_E_STAGES` to `nexus-contracts`.
  - `e80db53` — R-FL-E-02 changes the intake gate to use `intake_status`.
  - `bcf3563` — R-FL-E-03 enforces strict `target == current + 1` advancement without `--force`.
  - `f7f0b59` — R-FL-E-01 creates FL-E stage schedules and adds active-stage protection.
  - `991e2f8` — R-FL-E-04 adds force/audit logging.
  - `34fda67` — R-FL-E-05 moves gates into shared `check_stage_advance()`.
  - `03dbfa5` — R-FL-E-06 persists dual-path allowlist for `persist`.
  - `6cd1409` — R-FL-E-07 routes daemon stage PATCH through an atomic stage transaction.

Validation evidence:

```text
$ cargo test -p nexus-daemon-runtime --tests 2>&1 | tail -10
test creator_isolation_patch_work_returns_404_for_other_creator ... ok
test patch_work_intake_status_independent_of_stage_status ... ok
test patch_work_stage_returns_404_for_unknown ... ok
test patch_work_stage_change_is_auditable ... ok
test patch_work_updates_stage_fields ... ok
test list_works_returns_401_without_creator ... ok
test patch_work_invalid_stage_value_returns_400 ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s

$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

### Original finding dispositions

- **C-1 / F-QC1-001 — stage advance schedule creation and active protection: Resolved.**
  - Evidence: `f7f0b59` adds the CLI stage schedule creation path after a successful stage PATCH, using `default_preset_for_stage(target_stage)` and schedule metadata containing `work_id` and `fl_e_stage` (`crates/nexus42/src/commands/creator/run.rs:557-595`).
  - Evidence: `6cd1409` routes daemon stage PATCH through `patch_work_stage()` and `works::advance_work_stage_atomic(...)` (`crates/nexus-daemon-runtime/src/api/handlers/works.rs:296-420`), while `advance_work_stage_atomic()` performs the active-stage check and stage update inside one transaction (`crates/nexus-local-db/src/works.rs:836-931`).
  - Evidence: `cargo test -p nexus-daemon-runtime --tests` covers the daemon stage PATCH path (`patch_work_stage_change_is_auditable`, `patch_work_updates_stage_fields`, `patch_work_invalid_stage_value_returns_400`) and passed.

- **C-2 / F-QC1-002 — intake gate used `stage_status` instead of `intake_status`: Resolved.**
  - Evidence: `e80db53` and `34fda67` make the CLI read `intake_status` separately and pass it into shared gate state (`crates/nexus42/src/commands/creator/run.rs:514-531`).
  - Evidence: `check_stage_advance()` now gates intake advancement on `work.intake_status != "complete"`, not `stage_status` (`crates/nexus-orchestration/src/stage_gates.rs:112-121`).
  - Evidence: regression test `patch_work_intake_status_independent_of_stage_status` creates/patches a V1.33-style Work with `intake_status=complete` and default `stage_status=pending`, then advances to `research` successfully (`crates/nexus-daemon-runtime/tests/works_api.rs:808-885`); the required test suite passed.

- **W-1 / F-QC1-003 — non-force stage advance could skip multiple stages: Resolved.**
  - Evidence: `bcf3563` and `34fda67` centralize the non-force linear gate in `check_stage_advance()`, which now rejects `target_idx != current_idx + 1` unless `force` is true (`crates/nexus-orchestration/src/stage_gates.rs:77-87`).
  - Evidence: unit coverage includes `reject_skip_without_force` and `allow_skip_with_force` in the shared gate module (`crates/nexus-orchestration/src/stage_gates.rs:167-178`).

- **S-1 / F-QC1-004 — duplicated `FL_E_STAGES` constants: Resolved.**
  - Evidence: `c3834ce` moves the canonical ordered stage list into `nexus_contracts::local::orchestration` (`crates/nexus-contracts/src/local/orchestration/mod.rs:278-296`).
  - Evidence: current search found only one `pub const FL_E_STAGES` definition, in `nexus-contracts`; CLI, orchestration, and local DB now import or re-export that single source (`crates/nexus42/src/commands/creator/run.rs:13`, `crates/nexus-orchestration/src/preset/validation.rs:1524`, `crates/nexus-local-db/src/works.rs:933-937`).

### Revalidation summary

| Original finding | Severity | Disposition |
| --- | --- | --- |
| C-1 / F-QC1-001 | Critical | Resolved |
| C-2 / F-QC1-002 | Critical | Resolved |
| W-1 / F-QC1-003 | Warning | Resolved |
| S-1 / F-QC1-004 | Suggestion | Resolved |

**Revalidation Verdict**: Approve
