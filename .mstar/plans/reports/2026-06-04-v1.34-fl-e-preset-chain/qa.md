---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
verdict: "Fail"
generated_at: "2026-06-05"
---

# QA Report

## Scope

- plan_id: 2026-06-04-v1.34-fl-e-preset-chain
- Review range / Diff basis: merge-base: origin/main..HEAD on feature/v1.34-fl-e-preset-chain
- Working branch (verified): `feature/v1.34-fl-e-preset-chain`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`
- Mode: Default QA (full verification)
- Assignment scope: all P2 topic-branch changes and fix waves through `HEAD`; no business-code changes made by QA.

## Verdict

**Fail**

The code/test gates and key FL-E schedule functionality passed fresh verification, and QC1/QC2 are now `Approve`. However, final sign-off cannot pass because required harness artifacts and QC/residual lifecycle state are inconsistent with the assignment:

1. The plan file and `status.json` still show this plan as `Todo`, not `InReview`.
2. Plan tasks T1-T4 remain unchecked in the plan Markdown.
3. `qc3.md` still has frontmatter `verdict: "Request Changes"` and final `**Verdict**: Request Changes`, not `Approve w/ residuals`.
4. No P2 residual entries are registered under root `.mstar/status.json` `residual_findings["2026-06-04-v1.34-fl-e-preset-chain"]`.
5. The report directory currently has three QC reports, not the expected five total report files described by the assignment.
6. The P2-only git log contains more commits than the assignment's expected 13 because `qc2.md` has multiple repeated hash-fill commits; this is not necessarily functional risk, but it fails the exact commit-count expectation as written.

## 1. Plan Scope and Scope Consistency

### Evidence

```text
$ python3 - <<'PY'
import json
from pathlib import Path
status=json.loads(Path('.mstar/status.json').read_text())
plan_id='2026-06-04-v1.34-fl-e-preset-chain'
row=next((p for p in status['plans'] if p.get('id')==plan_id), None)
print('status row:', json.dumps({k: row.get(k) for k in ['id','status','file','working_branch','merge_target']}, indent=2))
print('p2 residuals:', json.dumps(status.get('residual_findings', {}).get(plan_id), indent=2))
PY
...
status row: {
  "id": "2026-06-04-v1.34-fl-e-preset-chain",
  "status": "Todo",
  "file": ".mstar/plans/2026-06-04-v1.34-fl-e-preset-chain.md",
  "working_branch": "feature/v1.34-fl-e-preset-chain",
  "merge_target": "feature/v1.34-creator-workflow-and-agent-tools"
}
p2 residuals: null

Plan task/status lines:
3: **Plan ID**: `2026-06-04-v1.34-fl-e-preset-chain`  
6: **Status**: Todo  
10: **Working branch**: `feature/v1.34-fl-e-preset-chain`  
11: **Merge target**: `feature/v1.34-creator-workflow-and-agent-tools`  
44: - [ ] T1: Stage advance → schedule create with `fl_e_stage` metadata
45: - [ ] T2: Preset input templates consume Work fields
46: - [ ] T3: Integration test: full stage chain on demo Work
47: - [ ] T4: Update orchestration embedded preset README if needed
```

### Result

**Fail.** The code changes and tests indicate T1-T4 have implementation coverage, but the plan artifact itself still says `Status: Todo`, and all four task checkboxes remain unchecked. `status.json` also says `Todo`, despite the assignment stating PM had updated the status row to `InReview`.

Spec §4 does contain the five-stage preset mapping:

```text
96: ## 4. Preset chain (normative mapping)
100: | `intake` | `creative-brief-intake` | Shipped V1.33 |
101: | `research` | `research` | May append references to Work context |
102: | `produce` | `novel-writing` | Uses `creative_brief` + `inspiration_log` |
103: | `review` | `reflection-loop` | `llm_judge` gates per orchestration-engine |
104: | `persist` | `kb-extract` (via queue) + CLI memory review | No new persist-only preset required V1.34 |
```

## 2. CI / Lint / Test Gate

### Command: scoped cargo test

```text
$ cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory 2>&1 | tail -30
test crates/nexus-daemon-runtime/src/test_utils.rs - test_utils::create_test_workspace (line 38) ... ignored
test crates/nexus-daemon-runtime/src/db/pool.rs - db::pool::PoolConfig (line 42) - compile ... ok

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.12s

   Doc-tests nexus_local_db

running 2 tests
test crates/nexus-local-db/src/lib.rs - run_migrations (line 175) - compile ... ok
test crates/nexus-local-db/src/lib.rs - open_pool (line 138) - compile ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s

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

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.75s
```

### Command: scoped clippy

```text
$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

### Result

**Pass.** Fresh scoped test and clippy gates completed successfully.

## 3. Key Functional Verification

### FL-E chain demo

```text
$ cargo test -p nexus-orchestration --test fl_e_chain_demo 2>&1 | tail -15
running 11 tests
test fl_e_chain_force_allows_backwards ... ok
test fl_e_chain_force_allows_incomplete_current ... ok
test fl_e_chain_e2e_resolve_presets_and_inputs ... ok
test fl_e_chain_force_allows_skip ... ok
test fl_e_chain_reject_advance_while_active ... ok
test fl_e_chain_reject_backwards_produce_to_research ... ok
test fl_e_chain_reject_intake_to_produce_skip ... ok
test fl_e_chain_reject_same_stage ... ok
test fl_e_chain_happy_path_all_stages_advance ... ok
test fl_e_chain_preset_input_fields_propagate_across_stages ... ok
test fl_e_chain_reject_unknown_stage ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### FL-E schedule API

```text
$ cargo test -p nexus-daemon-runtime --test fl_e_schedule_api 2>&1 | tail -15
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on artifact directory
    Finished `test` profile [unoptimized + debuginfo] target(s) in 13.30s
     Running tests/fl_e_schedule_api.rs (target/debug/deps/fl_e_schedule_api-02f93b2eca43fe36)

running 5 tests
test schedule_create_with_correct_dto_shape ... ok
test schedule_create_without_seed_no_core_context ... ok
test schedule_create_seeds_core_context_from_preset_input ... ok
test schedule_with_empty_creator_id_is_isolated_from_legitimate_creators ... ok
test schedule_list_isolation_by_creator ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

This includes the fix wave 2 cases and the fix wave 3 empty-creator isolation regression.

### nexus42 tests / command surface

```text
$ cargo test -p nexus42 --tests 2>&1 | tail -5
test r3_local_truth_chain ... ok
test r3_context_assemble_moment_executes_without_placeholder_skip ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

### Result

**Pass.** The new tests cover the expected functional surfaces and passed freshly.

## 4. QC Consistency

### Evidence from report frontmatter and final verdict sections

- `qc1.md` frontmatter: `verdict: "Approve"`; latest revalidation says `**Verdict: Approve.**`
- `qc2.md` frontmatter: `verdict: "Approve"`; revalidation says `**Approve**`.
- `qc3.md` frontmatter: `verdict: "Request Changes"`; final report says `**Verdict**: Request Changes`.

`qc3.md` contains four warnings and two suggestions, but it was not updated to `Approve w/ residuals` after fix waves.

### Result

**Fail.** QC1 and QC2 align with approval after revalidation, but QC3 does not match the assignment requirement that it remain `Approve w/ residuals` with only warnings. The current checked-in `qc3.md` still blocks by its own verdict text.

## 5. Residual Lifecycle

### Evidence

```text
p2 residuals: null
```

Root `.mstar/status.json` has no `residual_findings["2026-06-04-v1.34-fl-e-preset-chain"]` entry.

### Assessment

- QC3's four warnings (`W-1` non-atomic stage advance + schedule create, `W-2` missing concurrency/API test, `W-3` missing schedule audit logs, `W-4` lost CLI error codes) are not registered as P2 residuals.
- QC1/QC2's minor shared-facade concern appears resolved by fix wave 2 (`build_schedule_for_stage` facade and CLI usage), so no separate open residual is required for S-1 based on current code/test evidence.
- The fix wave 3 note that `creator run start` may share the active-creator issue should be tracked outside this P2 gate, likely in P5 hygiene or a new V1.33 behavior-bug plan, because this QA scope is P2 stage advance.

### Result

**Fail.** Residual lifecycle is incomplete for QC3 warnings. Per harness rules, open residuals are canonical under root `residual_findings[plan_id]`; they are currently absent.

## 6. Git State

### P2 topic branch commits versus integration branch

```text
$ git log --oneline feature/v1.34-fl-e-preset-chain ^feature/v1.34-creator-workflow-and-agent-tools
594ec41 qc(v1.34-fl-e-preset-chain): revalidate qc1 fix wave 3
649e549 fix(fl-e): R-FL-E-P2-05 CLI stage advance uses active_creator from auth context not Work DTO
0af4a4b qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
ba39562 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
864954f qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
9846f61 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
03e5a43 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
32289d7 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
51d2286 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
242d82a qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
09e5791 qc(v1.34-fl-e-preset-chain): revalidate qc2 (fix wave 2: R-FL-E-P2-01/03/04 + R-FL-E-P2-02) — 2C+3W resolved; Approve
96e6bef qc(v1.34-fl-e-preset-chain): revalidate qc1 fix wave 2
a6f7b23 fix(fl-e): R-FL-E-P2-02 hermetic e2e tests for daemon schedule API
55e96dd fix(fl-e): R-FL-E-P2-01/03/04 correct DTO shape, shared facade, atomicity + error codes
454f126 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (post-verification)
2cd6390 qc(v1.34-fl-e-preset-chain): add qc2.md — security and correctness review (4 commits)
1b260b8 qc(v1.34-fl-e-preset-chain): qc3.md — performance & reliability review (4 commits)
d6b539d docs(qc): review FL-E preset chain
1115699 docs(orchestration): T4 FL-E chain in preset README
bd48ddb test(fl-e): T3 full stage chain integration test
6e692cb feat(fl-e): T2 preset inputs consume work fields
6714243 feat(fl-e): T1 schedule create with preset for all 4 stages
```

### Worktree status before QA report write

```text
$ git status
On branch feature/v1.34-fl-e-preset-chain
nothing to commit, working tree clean
```

### Diff stat

```text
$ git diff --stat $(git merge-base HEAD origin/main)..HEAD
 .mstar/archived/plans-done.json                    |  63 ++-
 .../2026-06-04-v1.34-agent-tool-registry-spec.json |  15 +
 ...26-06-04-v1.34-fl-e-run-intents-and-stages.json |  24 +
 .../2026-06-04-v1.34-residual-convergence.json     |  30 ++
 ...026-06-04-v1.33-work-model-and-creator-run.json | 106 ++--
 .../residuals/v1.32-post-qc-tech-debt.json         |  42 ++
 .mstar/knowledge/specs/agent-nexus-tool-bridge.md  | 349 ++++++++++++-
 .mstar/knowledge/specs/orchestration-engine.md     |   4 +-
 .../2026-06-04-v1.34-agent-tool-registry-spec.md   |  10 +-
 .../2026-06-04-v1.34-fl-e-preset-chain/qc1.md      | 220 ++++++++
 .../2026-06-04-v1.34-fl-e-preset-chain/qc2.md      | 314 ++++++++++++
 .../2026-06-04-v1.34-fl-e-preset-chain/qc3.md      | 188 +++++++
 .../qa.md                                          | 437 ++++++++++++++++
 .../qc1.md                                         | 178 +++++++
 .../qc2.md                                         | 360 ++++++++++++++
 .../qc3.md                                         | 440 ++++++++++++++++
 .../2026-06-04-v1.34-residual-convergence/qa.md    | 245 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc1.md   | 233 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc2.md   | 140 ++++++
 .../2026-06-04-v1.34-residual-convergence/qc3.md   | 114 +++++
 .mstar/status.json                                 | 366 ++++++--------
 .../nexus-contracts/src/local/orchestration/mod.rs |  25 +
 crates/nexus-daemon-runtime/src/api/errors.rs      |   9 +-
 .../nexus-daemon-runtime/src/api/handlers/works.rs | 158 +++++-
 .../tests/fl_e_schedule_api.rs                     | 331 ++++++++++++
 crates/nexus-daemon-runtime/tests/works_api.rs     | 365 ++++++++++++++
 .../migrations/20260606_works_stage_columns.sql    |   8 +
 crates/nexus-local-db/src/error.rs                 |   5 +
 crates/nexus-local-db/src/lib.rs                   |   5 +-
 crates/nexus-local-db/src/version.rs               |   2 +-
 crates/nexus-local-db/src/works.rs                 | 553 ++++++++++++++++++++-
 .../nexus-orchestration/embedded-presets/README.md |  63 +++
 .../src/capability/builtins/creator.rs             |   9 +-
 crates/nexus-orchestration/src/lib.rs              |   4 +
 .../nexus-orchestration/src/preset/validation.rs   | 339 +++++++++----
 crates/nexus-orchestration/src/stage_gates.rs      | 544 ++++++++++++++++++++
 .../nexus-orchestration/tests/fl_e_chain_demo.rs   | 222 +++++++++
 .../tests/run_intents_validation.rs                | 206 ++++++++
 crates/nexus42/src/commands/creator/run.rs         | 399 ++++++++++++++-
 39 files changed, 6700 insertions(+), 425 deletions(-)
```

### Result

**Partial / fail against exact assignment.** The worktree was clean before QA report write, and the branch contains the expected P2 work and fix commits. However, the P2-only commit list contains 22 commits before this QA report, not the expected 13, due to repeated `qc2.md` hash-fill commits. The diff stat also shows broader iteration files inherited from the topic branch base/diff basis; this may be expected for `origin/main..HEAD`, but it means the stat is not P2-only.

## 7. Integration Branch State

### Evidence

```text
$ git log --oneline -1 feature/v1.34-creator-workflow-and-agent-tools
89f4622 harness(v1.34-p3): mark P3 Done (spec only, no residuals)

$ git merge-base --is-ancestor 6714243 feature/v1.34-creator-workflow-and-agent-tools; printf 'P2 commit 6714243 on integration branch? exit=%s\n' "$?"
P2 commit 6714243 on integration branch? exit=1

$ git merge-base --is-ancestor 649e549 feature/v1.34-creator-workflow-and-agent-tools; printf 'P2 fix commit 649e549 on integration branch? exit=%s\n' "$?"
P2 fix commit 649e549 on integration branch? exit=1
```

### Result

**Pass.** The integration branch tip is the P3 closeout commit, and sampled P2 work/fix commits are not ancestors of the integration branch.

## 8. Spec Alignment (SSOT)

### Evidence and assessment

- Spec §4 locks the 5-stage mapping:
  - `intake` → `creative-brief-intake`
  - `research` → `research`
  - `produce` → `novel-writing`
  - `review` → `reflection-loop`
  - `persist` → `kb-extract` + CLI memory review
- Fresh `fl_e_chain_demo` passed 11/11, including stage preset resolution and Work-field propagation.
- Fresh `fl_e_schedule_api` passed 5/5, including `schedule_create_seeds_core_context_from_preset_input` and `schedule_create_with_correct_dto_shape`.
- QC1 Revalidation 2 states CLI stage advance now sources `creator_id` from `config.active_creator_id`, not from `WorkApiDto`.

### Result

**Pass for code/test evidence.** The implementation behavior appears aligned with spec §4 and the fix wave requirements, based on the required tests and QC revalidation evidence. The blocker is harness/QC artifact lifecycle, not the functional test evidence.

## Final Checklist

| Item | Result | Notes |
| --- | --- | --- |
| 1. Plan scope and status consistency | Fail | Plan/status still `Todo`; T1-T4 unchecked. |
| 2. CI/lint/test green | Pass | Scoped cargo test and clippy passed. |
| 3. Key functionality tests | Pass | 11 FL-E chain tests, 5 schedule API tests, 15 nexus42 tests passed. |
| 4. QC consistency | Fail | `qc3.md` still says `Request Changes`, not `Approve w/ residuals`. |
| 5. Residual lifecycle | Fail | No P2 root residual entries for QC3 warnings. |
| 6. Git state | Partial / Fail vs exact count | Clean before QA report write; branch has expected code commits but not expected 13 total commits. |
| 7. Integration branch state | Pass | P2 commits not in integration; integration tip is P3 closeout. |
| 8. Spec alignment | Pass | Spec table and required tests align. |

## Required Follow-ups Before P2 → Integration Merge

1. PM (or authorized harness maintainer) should update the P2 plan and `.mstar/status.json` to reflect the real review state (`InReview` if still awaiting QA re-run, or equivalent after fixes).
2. Update plan task checkboxes T1-T4 or otherwise record completion evidence in the plan.
3. Either revalidate/update `qc3.md` to `Approve w/ residuals` after fix waves, or explicitly register its warnings as residuals and document the decision.
4. Add P2 residual entries under root `.mstar/status.json` `residual_findings["2026-06-04-v1.34-fl-e-preset-chain"]` for any accepted QC3 warnings; if PM decides no P2 residuals are needed, record that decision in the plan/status artifacts.
5. Track the `creator run start` active-creator issue as a V1.33 behavior bug in P5 hygiene or a new plan, outside this P2 merge gate.

## Not Tested / Not Changed

- No manual daemon/CLI happy-path demo was run beyond the required automated tests.
- No business implementation files were modified.
- No plan/status/residual files were modified by QA because the assignment constrained this turn to writing only this `qa.md` report.
