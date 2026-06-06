# QA Report

## Scope tested

- Agent: `qa-engineer`
- Mode: Default QA (full verification)
- plan_id: `2026-06-04-v1.34-fl-e-run-intents-and-stages`
- Review range / Diff basis: `merge-base: origin/main..HEAD on feature/v1.34-fl-e-run-intents-and-stages`
- Working branch verified: `feature/v1.34-fl-e-run-intents-and-stages`
- Review cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages`
- Scope in: P1 topic branch changes, 3 feature commits, 8 fix-wave-2 commits, harness/status commit, QC reports/revalidations, plan/status/spec/compass alignment, CI/lint/test evidence, residual registration position, git state, integration-branch separation, and spec alignment.
- Scope out: business-code changes, P0/P3-P5 implementation behavior, manual live daemon/demo execution beyond automated tests and static/spec alignment.

## Findings

### Summary

| ID | Severity | Status | Owner | Summary |
| --- | --- | --- | --- | --- |
| QA-F1 | Note | Functional gate passed | PM / QA | Scoped tests and clippy passed with fresh command output. |
| QA-F2 | Warning | Non-blocking documentation hygiene | PM | Plan Markdown still shows `Status: Todo` and T1-T4 unchecked, while `.mstar/status.json` row is `InReview`; implementation/tests show T1-T4 are delivered. |
| QA-F3 | Warning | Non-blocking residual lifecycle hygiene | PM | qc3 residuals R-03/R-04/R-07/R-08/R-09 are documented in `qc3.md`, but not in plan front matter/body, `.mstar/notes.json`, or root `status.json.residual_findings[plan_id]`. Assignment says PM owns this; recommend P5 hygiene tracking. |
| QA-F4 | Warning | Non-blocking git/report hygiene | PM / QC | Expected topic-only commit count was 18, but actual count is 23 because multiple qc2 revalidation/report commits remain in topic history. Working tree is clean and final report files are present. |

### Per-assignment checklist result

1. **Plan scope and scope consistency** — **Pass w/ warning**.
   - `status.json` row for `2026-06-04-v1.34-fl-e-run-intents-and-stages` is `InReview` and points to the correct branch/spec/merge target.
   - T1-T4 are implemented in code/tests: DDL/API stage fields, CLI `creator run stage`, stage→preset allowlist, and `creator run status` stage fields.
   - Warning: the plan Markdown itself remains stale (`Status: Todo`; T1-T4 unchecked).

2. **CI/lint/test green** — **Pass**.
   - Scoped `cargo test` and `cargo clippy -- -D warnings` both passed.

3. **Key functional test coverage** — **Pass**.
   - `nexus-daemon-runtime --tests`: 25 passed, including stage PATCH/audit/intake-stage tests.
   - `nexus42 --tests`: 15 passed; `command_surface_contract`: 29 passed.
   - `nexus-orchestration`: passed; targeted stage validation tests: 8 passed; stage gate tests: 10 passed.

4. **QC consistency** — **Pass**.
   - `qc1.md` Revalidation Verdict: `Approve`.
   - `qc2.md` Revalidation Verdict: `Approve`.
   - `qc3.md` frontmatter/verdict and final revalidation verdict: `Approve w/ residuals`.

5. **Residual lifecycle** — **Pass w/ warning**.
   - qc3 residuals are documented in `qc3.md` lines 300-381 and disposition table lines 422-440.
   - No `status.json.residual_findings["2026-06-04-v1.34-fl-e-run-intents-and-stages"]` entry exists, consistent with the assignment note that P1 has no explicit residual registration yet.
   - Warning: plan Markdown and `.mstar/notes.json` do not currently carry R-03/R-04/R-07/R-08/R-09. This is PM-managed; recommend P5 hygiene handles or explicitly accepts these qc3 deferred notes.

6. **Git state** — **Pass w/ warning**.
   - Working tree clean before report write.
   - Topic-only log contains all expected feature/fix/harness/QC commits, but actual count is 23 rather than assignment-expected 18 due duplicate qc2 report/revalidation commits.
   - Full diff stat covers P1 implementation plus report/status artifacts.

7. **Integration branch state** — **Pass**.
   - `feature/v1.34-creator-workflow-and-agent-tools` latest commit is `f7bc294 harness(v1.34-p0): mark P0 Done, compact to plans-done.json`.
   - `git branch --contains 655d71c` only lists `feature/v1.34-fl-e-run-intents-and-stages`, so integration branch does not contain the P1 feature commit yet.

8. **Spec alignment (SSOT)** — **Pass**.
   - Stage enum matches spec §3.1: `intake`, `research`, `produce`, `review`, `persist`.
   - Stage status enum matches spec §3.2 via DB CHECK: `pending`, `active`, `complete`, `skipped`, `failed`.
   - Stage→preset mapping matches spec §4, with `persist` accepting `kb-extract` and `memory-review` to reflect the spec's dual persist path while keeping `kb-extract` as default.
   - CLI surface matches cli-spec §6.2E: `stage list <work_id>`, `stage advance <work_id> --stage <id> [--force]`, and `status <work_id>` includes stage fields.

## Reproduction steps

Run from repo root:

```bash
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages

git rev-parse --show-toplevel
git branch --show-current
git status --short

cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory 2>&1 | tail -30
cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
cargo test -p nexus-daemon-runtime --tests 2>&1 | tail -10
cargo test -p nexus42 --tests 2>&1 | tail -10
cargo test -p nexus-orchestration 2>&1 | tail -10
cargo test -p nexus42 --test command_surface_contract 2>&1 | tail -20
cargo test -p nexus-orchestration --lib 'preset::validation::stage_tests' 2>&1 | tail -15
cargo test -p nexus-orchestration --lib stage_gates 2>&1 | tail -20

git log --oneline feature/v1.34-fl-e-run-intents-and-stages ^feature/v1.34-creator-workflow-and-agent-tools
git diff --stat $(git merge-base HEAD origin/main)..HEAD
git log --oneline -1 feature/v1.34-creator-workflow-and-agent-tools
git branch --contains 655d71c --format='%(refname:short)'
```

## Evidence

### Checkout alignment

```text
$ git rev-parse --show-toplevel && git branch --show-current && git status --short
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages
feature/v1.34-fl-e-run-intents-and-stages
```

`git status --short` produced no output before report writing.

### 1. Plan scope and status evidence

Plan Markdown (`.mstar/plans/2026-06-04-v1.34-fl-e-run-intents-and-stages.md`) evidence:

```text
Line 3: **Plan ID**: `2026-06-04-v1.34-fl-e-run-intents-and-stages`
Line 6: **Status**: Todo
Line 10: **Working branch**: `feature/v1.34-fl-e-run-intents-and-stages`
Line 11: **Merge target**: `feature/v1.34-creator-workflow-and-agent-tools`
Line 42: - [ ] T1: DDL migration + `works` API extensions
Line 43: - [ ] T2: CLI `run stage` submodule + tests
Line 44: - [ ] T3: Stage→preset allowlist validation
Line 45: - [ ] T4: Extend `creator run status` output
```

`status.json` evidence:

```text
Line 175: "id": "2026-06-04-v1.34-fl-e-run-intents-and-stages"
Line 176: "status": "InReview"
Line 178: "working_branch": "feature/v1.34-fl-e-run-intents-and-stages"
Line 179: "merge_target": "feature/v1.34-creator-workflow-and-agent-tools"
Line 181: "iteration_compass": ".mstar/iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md"
Line 182: "primary_spec": ".mstar/knowledge/specs/creator-workflow.md"
Line 187-196: fix_wave_2_commits contains c3834ce, e80db53, bcf3563, f7f0b59, 991e2f8, 34fda67, 03dbfa5, 6cd1409
Line 197-200: qc_reports list qc1/qc2/qc3 re-review verdicts
```

T1-T4 implementation evidence:

- T1: `crates/nexus-local-db/migrations/20260606_works_stage_columns.sql` adds `current_stage` and `stage_status`; `crates/nexus-daemon-runtime/src/api/handlers/works.rs` exposes `current_stage` / `stage_status` in `WorkApiDto`.
- T2: `crates/nexus42/src/commands/creator/run.rs` implements `stage_list()` and `stage_advance()`.
- T3: `crates/nexus-orchestration/src/preset/validation.rs` implements `STAGE_PRESET_ALLOWLIST`, `default_preset_for_stage`, and `validate_preset_for_stage` with stage tests.
- T4: `WorkApiDto` includes stage fields; CLI status output reads work DTOs containing these fields.

### 2. CI/lint/test full scoped gate

```text
$ cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory 2>&1 | tail -30
test crates/nexus-daemon-runtime/src/test_utils.rs - test_utils::create_test_workspace (line 38) ... ignored
test crates/nexus-daemon-runtime/src/db/pool.rs - db::pool::PoolConfig (line 42) - compile ... ok

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.13s

   Doc-tests nexus_local_db

running 2 tests
test crates/nexus-local-db/src/lib.rs - open_pool (line 138) - compile ... ok
test crates/nexus-local-db/src/lib.rs - run_migrations (line 175) - compile ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s

   Doc-tests nexus_orchestration

running 3 tests
test crates/nexus-orchestration/src/preset/mod.rs - preset::load_embedded_preset (line 82) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::MockSpawner (line 229) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::WorkerManagerSpawner (line 43) ... ignored

test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests nexus42

running 2 tests
test crates/nexus42/src/domain/runtime_guard.rs - domain::runtime_guard (line 7) ... ignored
test crates/nexus42/src/challenge/mod.rs - challenge::solve_challenge (line 128) ... ok

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.78s

$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

### 3. Key functional tests

```text
$ cargo test -p nexus-daemon-runtime --tests 2>&1 | tail -10
test creator_isolation_patch_work_returns_404_for_other_creator ... ok
test patch_work_intake_status_independent_of_stage_status ... ok
test patch_work_stage_change_is_auditable ... ok
test patch_work_stage_returns_404_for_unknown ... ok
test patch_work_updates_stage_fields ... ok
test list_works_returns_401_without_creator ... ok
test patch_work_invalid_stage_value_returns_400 ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s

$ cargo test -p nexus42 --tests 2>&1 | tail -10
test v1_29_pending_show_help ... ok
test r5_platform_guard_sync_status_works ... ok
test r2_persistent_identity_config_persists ... ok
test r2_persistent_identity_e2e ... ok
test r3_soul_validation ... ok
test r3_local_truth_chain ... ok
test r3_context_assemble_moment_executes_without_placeholder_skip ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

$ cargo test -p nexus-orchestration 2>&1 | tail -10
   Doc-tests nexus_orchestration

running 3 tests
test crates/nexus-orchestration/src/preset/mod.rs - preset::load_embedded_preset (line 82) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::MockSpawner (line 229) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::WorkerManagerSpawner (line 43) ... ignored

test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Additional targeted coverage:

```text
$ cargo test -p nexus42 --test command_surface_contract 2>&1 | tail -20
test current_state_acp_worker_is_hidden ... ok
test v2_target_acp_permission_subcommands ... ok
test v2_target_acp_run ... ok
test v133_creator_shows_run ... ok
test v2_target_acp_policy_subcommands ... ok
test v133_system_preset_list_flags ... ok
test v2_target_acp_session_subcommands ... ok
test v2_target_kb_scope_flag ... ok
test v2_target_creator_subcommands ... ok
test v2_target_acp_subcommands ... ok
test v2_target_daemon_subcommands ... ok
test v2_target_kb_subcommands ... ok
test v2_target_platform_subcommands ... ok
test v2_target_platform_explore_subcommands ... ok
test v2_target_sync_subcommands ... ok
test v2_target_system_doctor ... ok
test v2_target_system_subcommands ... ok

test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s

$ cargo test -p nexus-orchestration --lib 'preset::validation::stage_tests' 2>&1 | tail -15
running 8 tests
test preset::validation::stage_tests::persist_allowlist_accepts_both_paths ... ok
test preset::validation::stage_tests::default_preset_for_known_stages ... ok
test preset::validation::stage_tests::stage_index_unknown ... ok
test preset::validation::stage_tests::stage_index_order ... ok
test preset::validation::stage_tests::default_preset_for_unknown_stage ... ok
test preset::validation::stage_tests::validate_known_preset_for_stage ... ok
test preset::validation::stage_tests::validate_wrong_preset_for_stage ... ok
test preset::validation::stage_tests::validate_unknown_stage ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 398 filtered out; finished in 0.00s

$ cargo test -p nexus-orchestration --lib stage_gates 2>&1 | tail -20
running 10 tests
test stage_gates::tests::allow_advance_after_complete ... ok
test stage_gates::tests::allow_skip_with_force ... ok
test stage_gates::tests::reject_intake_not_complete ... ok
test stage_gates::tests::reject_same_stage ... ok
test stage_gates::tests::reject_incomplete_current ... ok
test stage_gates::tests::reject_active_schedule_exists ... ok
test stage_gates::tests::reject_backwards ... ok
test stage_gates::tests::reject_skip_without_force ... ok
test stage_gates::tests::valid_advance_intake_to_research ... ok
test stage_gates::tests::reject_unknown_target ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 396 filtered out; finished in 0.00s
```

### 4. QC consistency evidence

`qc1.md`:

```text
frontmatter verdict: "Approve"
Revalidation Verdict: Approve
Original findings C-1 / C-2 / W-1 / S-1 disposition: Resolved
```

`qc2.md`:

```text
frontmatter verdict: "Approve"
Revalidation summary: Critical 3/3 resolved; Warning 3/3 resolved or design-accepted; Verdict: Approve
```

`qc3.md`:

```text
frontmatter verdict: "Approve w/ residuals"
Critical R-01 and R-02: Resolved
Residuals: R-03, R-04, R-07, R-08, R-09
Final verdict: `Approve w/ residuals`
```

### 5. Residual lifecycle evidence

```text
$ grep in plan Markdown for R-03/R-04/R-07/R-08/R-09/residual
Found only existing plan boilerplate and T1-T4 lines; no qc3 residual entries.

$ grep in status.json for plan_id and R-03/R-04/R-07/R-08/R-09
Found plan row at lines 175-203.
No root residual_findings entry for 2026-06-04-v1.34-fl-e-run-intents-and-stages.
R-03/R-04/R-07 matches in status.json were unrelated v1.33 metadata/residuals.

$ glob .mstar/plans/residuals/**/*
No P1 residual prose path for 2026-06-04-v1.34-fl-e-run-intents-and-stages.

$ read .mstar/notes.json
No P1 qc3 R-03/R-04/R-07/R-08/R-09 notes present.
```

### 6. Git state evidence

```text
$ git log --oneline feature/v1.34-fl-e-run-intents-and-stages ^feature/v1.34-creator-workflow-and-agent-tools
c3beca2 harness(v1.34-p1): mark P1 InReview, register 8 fix wave 2 commits + 3 QC reports
1bcb027 qc(v1.34-fl-e): revalidate qc1 after fix wave 2
301d6c8 qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related)
e601a4b qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related)
c1b2b53 qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related)
ea68e71 qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related)
79e80a6 qc(v1.34-fl-e): qc3 revalidation — Approve w/ residuals after fix wave 2
37bec53 qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related) [hash fill]
2869c4d qc(v1.34-fl-e): revalidate qc2 after fix wave 2 (C-1/2/3 + W-1/2/3 + S-1/2/3 + qc1 related)
6cd1409 fix(fl-e): R-FL-E-07 PATCH stage wrapped in atomic transaction (TOCTOU-safe)
03dbfa5 fix(fl-e): R-FL-E-06 persist allowlist dual-path kb-extract + memory-review
34fda67 fix(fl-e): R-FL-E-05 PATCH stage uses shared gates with CLI
991e2f8 fix(fl-e): R-FL-E-04 audit log on --force stage skip
f7f0b59 fix(fl-e): R-FL-E-01 stage advance creates schedule + active protection
bcf3563 fix(fl-e): R-FL-E-03 strict linear gate without --force
e80db53 fix(fl-e): R-FL-E-02 intake gate uses intake_status not stage_status
c3834ce fix(fl-e): R-FL-E-08 dedup FL_E_STAGES constant to single source in nexus-contracts
0d8cd1e qc(v1.34-fl-e): add qc2.md — security and correctness review (P1 3 commits)
4432750 qc(v1.34-fl-e): qc3.md — performance & reliability review
091b037 qc(v1.34-fl-e): add qc1 report
e0e1861 feat(orchestration): V1.34 FL-E T3 — stage→preset allowlist validation
d379f86 feat(cli): V1.34 FL-E T2+T4 — creator run stage + status extensions
655d71c feat(local-db,daemon): V1.34 FL-E T1 — stage columns on works

$ git log --oneline feature/v1.34-fl-e-run-intents-and-stages ^feature/v1.34-creator-workflow-and-agent-tools | wc -l
      23
```

Expected in assignment: 18 commits. Actual: 23 commits. Extra commits are report/revalidation history, especially repeated qc2 revalidation commits.

```text
$ git diff --stat $(git merge-base HEAD origin/main)..HEAD
 .../2026-06-04-v1.34-residual-convergence.json     |  30 ++
 ...026-06-04-v1.33-work-model-and-creator-run.json | 106 ++--
 .../residuals/v1.32-post-qc-tech-debt.json         |  42 ++
 .../qc1.md                                         | 178 +++++++
 .../qc2.md                                         | 360 ++++++++++++++
 .../qc3.md                                         | 440 ++++++++++++++++
 .../2026-06-04-v1.34-residual-convergence/qa.md    | 245 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc1.md   | 233 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc2.md   | 140 ++++++
 .../2026-06-04-v1.34-residual-convergence/qc3.md   | 114 +++++
 .mstar/status.json                                 | 273 +++++-----
 .../nexus-contracts/src/local/orchestration/mod.rs |  25 +
 crates/nexus-daemon-runtime/src/api/errors.rs      |   9 +-
 .../nexus-daemon-runtime/src/api/handlers/works.rs | 158 +++++-
 crates/nexus-daemon-runtime/tests/works_api.rs     | 365 ++++++++++++++
 .../migrations/20260606_works_stage_columns.sql    |   8 +
 crates/nexus-local-db/src/error.rs                 |   5 +
 crates/nexus-local-db/src/lib.rs                   |   5 +-
 crates/nexus-local-db/src/version.rs               |   2 +-
 crates/nexus-local-db/src/works.rs                 | 553 ++++++++++++++++++++-
 .../src/capability/builtins/creator.rs             |   9 +-
 crates/nexus-orchestration/src/lib.rs              |   1 +
 .../nexus-orchestration/src/preset/validation.rs   | 339 +++++++++----
 crates/nexus-orchestration/src/stage_gates.rs      | 206 ++++++++
 .../tests/run_intents_validation.rs                | 206 ++++++++
 crates/nexus42/src/commands/creator/run.rs         | 296 ++++++++++-
 26 files changed, 4026 insertions(+), 322 deletions(-)
```

### 7. Integration branch evidence

```text
$ git log --oneline -1 feature/v1.34-creator-workflow-and-agent-tools
f7bc294 harness(v1.34-p0): mark P0 Done, compact to plans-done.json

$ git branch --contains 655d71c --format='%(refname:short)'
feature/v1.34-fl-e-run-intents-and-stages
```

### 8. Spec alignment evidence

Stage enum SSOT:

```text
crates/nexus-contracts/src/local/orchestration/mod.rs:283
pub const FL_E_STAGES: &[&str] = &["intake", "research", "produce", "review", "persist"];
```

Stage status DB enum:

```sql
ALTER TABLE works ADD COLUMN stage_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (stage_status IN ('pending','active','complete','skipped','failed'));
```

Stage→preset mapping:

```rust
pub const STAGE_PRESET_ALLOWLIST: &[(&str, &[&str])] = &[
    ("intake", &["creative-brief-intake"]),
    ("research", &["research"]),
    ("produce", &["novel-writing"]),
    ("review", &["reflection-loop"]),
    ("persist", &["kb-extract", "memory-review"]),
];
```

CLI stage command implementation evidence:

```text
crates/nexus42/src/commands/creator/run.rs
- stage_list() renders all FL_E_STAGES and current stage/status.
- stage_advance(work_id, target_stage, force, json, client) validates shared gates, PATCHes current_stage/stage_status, creates stage schedule with work_id/fl_e_stage metadata, audits --force, and prints schedule/preset evidence.
```

## Not tested

- No live daemon process was started for a manual end-to-end demo (`creator run start` → stage advance through persist); assignment requested automated verification commands and branch/spec checks.
- No SQL disaster-recovery/manual migration replay was performed for qc3 R-03; risk remains documented by QC as residual.
- No performance benchmark was run for DDL migration or stage advance; QC3 assessed these as low/negligible.
- No integration-branch merge was performed; assignment explicitly says PM has not merged P1 into integration yet.

## Recommended owners

- **PM**: Decide whether to update plan Markdown status/tasks before merge, or accept `status.json` as the active state SSOT for this gate.
- **PM / P5 hygiene owner**: Record or explicitly defer qc3 residuals R-03/R-04/R-07/R-08/R-09 in the chosen PM-managed tracker (status root residuals, plan note, or P5 hygiene record). Current state is documented only in `qc3.md`.
- **PM / QC owners**: Review duplicate qc2 revalidation/report commits in topic history. They do not change business behavior, but the topic-only count differs from the assignment's expected 18 commits.

## Verdict

**Verdict**: `Pass w/ notes`

Functional verification, scoped test/lint gates, QC revalidation consistency, integration-branch separation, and spec alignment passed with fresh evidence. Non-blocking PM-owned hygiene notes remain: stale plan Markdown checkboxes/status, qc3 residual tracking location, and duplicate QC report commits causing a 23-vs-18 topic-only commit count.
