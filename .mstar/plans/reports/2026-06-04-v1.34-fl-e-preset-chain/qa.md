---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
verdict: "Pass w/ notes"
generated_at: "2026-06-05"
---

# QA Report

## Scope

- plan_id: 2026-06-04-v1.34-fl-e-preset-chain
- Review range / Diff basis: `merge-base: origin/main..HEAD` on `feature/v1.34-fl-e-preset-chain`; 全部 P2 commits in scope: `6714243`, `6e692cb`, `bd48ddb`, `1115699`, `55e96dd`, `a6f7b23`, `649e549`, `23ea43a`, `bf1ebd4`, plus QC report/revalidation commits.
- Working branch (verified): `feature/v1.34-fl-e-preset-chain`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`
- Mode: Default QA re-verification after previous Fail + PM lifecycle corrections.
- Assignment scope: P2 topic branch; implementation/fix commits, harness lifecycle commits, canonical QC reports with revalidation updates, residual registration, and required automated verification. QA changed only this report file.

## Verdict

**Pass w/ notes.**

Functional verification, required test/lint gates, QC final verdict alignment, residual lifecycle registration, git branch isolation, and spec alignment all pass fresh re-verification. The remaining note is non-blocking for this QA scope: the plan Markdown header still says `Status: Todo` and task checkboxes remain unchecked, but the assignment explicitly marks the plan Markdown update as PM-owned and out of QA scope; `.mstar/status.json` is now corrected to `InReview`.

## 1. Plan Scope and Scope Consistency

### Evidence

```text
$ git rev-parse --show-toplevel && git branch --show-current && git rev-parse --short HEAD
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
feature/v1.34-fl-e-preset-chain
bf1ebd4
```

```text
$ python3 - <<'PY'
import json
from pathlib import Path
plan_id='2026-06-04-v1.34-fl-e-preset-chain'
status=json.loads(Path('.mstar/status.json').read_text())
row=next((p for p in status.get('plans',[]) if p.get('id')==plan_id), None)
print(json.dumps({k: row.get(k) for k in ['id','status','file','working_branch','merge_target']} if row else None, indent=2))
PY
{
  "id": "2026-06-04-v1.34-fl-e-preset-chain",
  "status": "InReview",
  "file": ".mstar/plans/2026-06-04-v1.34-fl-e-preset-chain.md",
  "working_branch": "feature/v1.34-fl-e-preset-chain",
  "merge_target": "feature/v1.34-creator-workflow-and-agent-tools"
}
```

Plan Markdown remains PM-owned out-of-scope note:

```text
.mstar/plans/2026-06-04-v1.34-fl-e-preset-chain.md
6: **Status**: Todo
44: - [ ] T1: Stage advance → schedule create with `fl_e_stage` metadata
45: - [ ] T2: Preset input templates consume Work fields
46: - [ ] T3: Integration test: full stage chain on demo Work
47: - [ ] T4: Update orchestration embedded preset README if needed
```

### Result

**Pass w/ notes.** Status row is now `InReview` as required. Plan Markdown still says `Todo`, but the assignment explicitly says PM will update it separately and QA should verify functional scope.

## 2. CI / Lint / Test Gate

### Scoped cargo test

```text
$ cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory 2>&1 | tail -30
test crates/nexus-daemon-runtime/src/test_utils.rs - test_utils::create_test_workspace (line 38) ... ignored
test crates/nexus-daemon-runtime/src/db/pool.rs - db::pool::PoolConfig (line 42) - compile ... ok

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.11s

   Doc-tests nexus_local_db

running 2 tests
test crates/nexus-local-db/src/lib.rs - open_pool (line 138) - compile ... ok
test crates/nexus-local-db/src/lib.rs - run_migrations (line 175) - compile ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

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

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.85s
```

### Scoped clippy

```text
$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

### Result

**Pass.** Required scoped cargo test and clippy gates completed successfully.

## 3. Key Functional Verification

### FL-E chain demo

```text
$ cargo test -p nexus-orchestration --test fl_e_chain_demo 2>&1 | tail -15
running 11 tests
test fl_e_chain_force_allows_backwards ... ok
test fl_e_chain_force_allows_incomplete_current ... ok
test fl_e_chain_force_allows_skip ... ok
test fl_e_chain_reject_advance_while_active ... ok
test fl_e_chain_e2e_resolve_presets_and_inputs ... ok
test fl_e_chain_reject_backwards_produce_to_research ... ok
test fl_e_chain_preset_input_fields_propagate_across_stages ... ok
test fl_e_chain_reject_intake_to_produce_skip ... ok
test fl_e_chain_reject_same_stage ... ok
test fl_e_chain_happy_path_all_stages_advance ... ok
test fl_e_chain_reject_unknown_stage ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### FL-E schedule API

```text
$ cargo test -p nexus-daemon-runtime --test fl_e_schedule_api 2>&1 | tail -15
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.36s
     Running tests/fl_e_schedule_api.rs (target/debug/deps/fl_e_schedule_api-02f93b2eca43fe36)

running 5 tests
test schedule_with_empty_creator_id_is_isolated_from_legitimate_creators ... ok
test schedule_create_seeds_core_context_from_preset_input ... ok
test schedule_create_without_seed_no_core_context ... ok
test schedule_list_isolation_by_creator ... ok
test schedule_create_with_correct_dto_shape ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

### nexus42 command-surface tests

```text
$ cargo test -p nexus42 --tests 2>&1 | tail -5
test r3_local_truth_chain ... ok
test r3_context_assemble_moment_executes_without_placeholder_skip ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

### Result

**Pass.** Required key functional tests passed: 11 FL-E chain tests, 5 schedule API tests, and 15 nexus42 tests.

## 4. QC Consistency

### Report history evidence

```text
$ git log --oneline .mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/ | head -10
5914163 qc(v1.34-fl-e-preset-chain): qc3 revalidation — Approve w/ residuals
60944c3 qa(v1.34-fl-e-preset-chain): final verification report
594ec41 qc(v1.34-fl-e-preset-chain): revalidate qc1 fix wave 3
0af4a4b qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
ba39562 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
864954f qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
9846f61 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
03e5a43 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
32289d7 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
51d2286 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)
```

Canonical QC files present:

```text
.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qc1.md
.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qc2.md
.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qc3.md
```

Targeted revalidations were appended to the canonical report files (no `qcN-rev2.md` files), matching Morning Star targeted re-review convention. The report history includes the initial three QC report commits plus revalidation commits for qc1/qc2/qc3, exceeding the assignment's 5+ report-update requirement.

### Final verdict evidence

```text
$ python3 - <<'PY'
from pathlib import Path
for name in ['qc1.md','qc2.md','qc3.md']:
    p=Path('.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain')/name
    text=p.read_text()
    lines=[line for line in text.splitlines() if 'verdict:' in line.lower() or 'Verdict' in line]
    print(f'--- {name} ---')
    for line in lines[-8:]:
        print(line)
PY
--- qc1.md ---
verdict: "Approve"
**Verdict**: Request Changes
### Revalidation Verdict
**Verdict remains: Request Changes.** The main schedule DTO shape, seed propagation, rollback behavior, error codes, audit logs, and daemon API coverage are improved, and W-001/S-001 are resolved. However, C-001 is not fully closed because CLI `stage_advance` still lacks an authoritative creator identity source for the schedule request.
### Revalidation Verdict
**Verdict: Approve.** Fix wave 3 fully resolves the remaining QC1 Critical (`C-001`) by sourcing CLI stage-advance `creator_id` from `config.active_creator_id` instead of `WorkApiDto`, and the daemon-side empty-creator isolation regression is covered by a hermetic API test. Required targeted tests and scoped clippy passed; no new Critical/Warning findings were introduced in the reviewed fix commit.
--- qc2.md ---
verdict: "Approve"
**Verdict**: Request Changes
**Verdict (post-revalidation)**: All fix wave 2 changes land the required corrections for the original 2 Critical + 3 Warning. 4 hermetic e2e tests pass exercising the real daemon schedule API and `creator_schedules` inserts. No new Criticals. Per `mstar-review-qc` rules (0 unresolved Critical; no high-impact unresolved Warning with disagreement), **Approve**.
--- qc3.md ---
**Verdict**: Request Changes
**Verdict**: Fix correctly addresses the stuck-state reliability gap.
**Verdict**: Partial fix — valuable API contract tests added, but concurrent TOCTOU test still missing. Acceptable as residual (low risk: transaction wrapper is present and unit-tested).
**Verdict**: Partial fix — highest-risk path (stage advance) fully covered; intake/novel-writing paths remain uncovered. Acceptable as residual.
**Verdict**: Fix fully addresses programmatic error consumption.
**Verdict**: Unchanged from original review. Low-risk residual (const data is controlled).
**Verdict**: Intentionally deferred per original Suggestion classification.
**Verdict**: `Approve w/ residuals`
```

Frontmatter current final verdicts:

```text
qc1.md: verdict: "Approve"
qc2.md: verdict: "Approve"
qc3.md: verdict: "Approve w/ residuals"
```

Historical `Request Changes` text remains in prior review/revalidation sections, but each canonical report's current frontmatter and latest revalidation verdict are non-blocking.

### Result

**Pass.** qc1 final verdict is `Approve`, qc2 final verdict is `Approve`, and qc3 final verdict is `Approve w/ residuals`. No current final QC verdict remains `Request Changes`.

## 5. Residual Lifecycle

### Evidence

```text
$ jq '.residual_findings["2026-06-04-v1.34-fl-e-preset-chain"] | length' .mstar/status.json
4
```

```text
$ python3 - <<'PY'
import json
from pathlib import Path
plan_id='2026-06-04-v1.34-fl-e-preset-chain'
status=json.loads(Path('.mstar/status.json').read_text())
entries=status.get('residual_findings',{}).get(plan_id,[])
for e in entries:
    print(f"{e.get('id')}: {e.get('source')} | {e.get('severity')} | {e.get('title')}")
PY
R-P2-W2: qc3 reval (P2) | low | Missing concurrent PATCH TOCTOU test (qc3 W-2 partial)
R-P2-W3: qc3 reval (P2) | low | Intake/novel-writing schedule creation paths lack fl_e.audit tracing (qc3 W-3 partial)
R-P2-S1: qc3 reval (P2) | low | default_preset_for_stage panic risk on empty array (qc3 S-1)
R-P2-S2: qc3 reval (P2) | low | Sequential API round-trips could be batched (qc3 S-2)
```

### Result

**Pass.** Root `.mstar/status.json` now has 4 open residual findings for this plan: W-2 partial, W-3 partial, S-1, and S-2.

## 6. Git State

### P2 topic branch commits versus integration branch

```text
$ git log --oneline feature/v1.34-fl-e-preset-chain ^feature/v1.34-creator-workflow-and-agent-tools
bf1ebd4 harness(v1.34-p2): register 4 qc3 residuals (W-2 partial, W-3 partial, S-1, S-2)
5914163 qc(v1.34-fl-e-preset-chain): qc3 revalidation — Approve w/ residuals
23ea43a harness(v1.34-p2): mark P2 InReview + register 6 work/fix commits + 3 QC reports
60944c3 qa(v1.34-fl-e-preset-chain): final verification report
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
 .../2026-06-04-v1.34-fl-e-preset-chain/qa.md       | 373 ++++++++++++++
 .../2026-06-04-v1.34-fl-e-preset-chain/qc1.md      | 220 ++++++++
 .../2026-06-04-v1.34-fl-e-preset-chain/qc2.md      | 314 ++++++++++++
 .../2026-06-04-v1.34-fl-e-preset-chain/qc3.md      | 322 ++++++++++++
 .../qa.md                                          | 437 ++++++++++++++++
 .../qc1.md                                         | 178 +++++++
 .../qc2.md                                         | 360 ++++++++++++++
 .../qc3.md                                         | 440 ++++++++++++++++
 .../2026-06-04-v1.34-residual-convergence/qa.md    | 245 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc1.md   | 233 +++++++++
 .../2026-06-04-v1.34-residual-convergence/qc2.md   | 140 ++++++
 .../2026-06-04-v1.34-residual-convergence/qc3.md   | 114 +++++
 .mstar/status.json                                 | 433 ++++++++--------
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
 40 files changed, 7273 insertions(+), 426 deletions(-)
```

### Result

**Pass.** Topic branch contains all expected P2 work/fix/harness/QC artifacts. Worktree was clean before this QA report overwrite. Diff stat reflects the full `origin/main..HEAD` feature-line scope as requested.

## 7. Integration Branch State

### Evidence

```text
$ git log --oneline -1 feature/v1.34-creator-workflow-and-agent-tools && \
  git merge-base --is-ancestor 6714243 feature/v1.34-creator-workflow-and-agent-tools; printf 'P2 commit 6714243 on integration branch? exit=%s\n' "$?" && \
  git merge-base --is-ancestor 649e549 feature/v1.34-creator-workflow-and-agent-tools; printf 'P2 fix commit 649e549 on integration branch? exit=%s\n' "$?" && \
  git merge-base --is-ancestor bf1ebd4 feature/v1.34-creator-workflow-and-agent-tools; printf 'P2 harness residual commit bf1ebd4 on integration branch? exit=%s\n' "$?"
89f4622 harness(v1.34-p3): mark P3 Done (spec only, no residuals)
P2 commit 6714243 on integration branch? exit=1
P2 fix commit 649e549 on integration branch? exit=1
P2 harness residual commit bf1ebd4 on integration branch? exit=1
```

### Result

**Pass.** The integration branch does not yet contain sampled P2 work/fix/harness commits.

## 8. Spec Alignment

### Spec §4 evidence

```text
## 4. Preset chain (normative mapping)
| `intake` | `creative-brief-intake` | Shipped V1.33 |
| `research` | `research` | May append references to Work context |
| `produce` | `novel-writing` | Uses `creative_brief` + `inspiration_log` |
| `review` | `reflection-loop` | `llm_judge` gates per orchestration-engine |
| `persist` | `kb-extract` (via queue) + CLI memory review | No new persist-only preset required V1.34 |
```

Implementation string evidence:

```text
$ python3 - <<'PY'
from pathlib import Path
text=Path('crates/nexus-orchestration/src/stage_gates.rs').read_text()
for needle in ['creative-brief-intake','research','novel-writing','reflection-loop','kb-extract']:
    print(f'{needle}:', needle in text)
PY
creative-brief-intake: True
research: True
novel-writing: True
reflection-loop: True
kb-extract: True
```

Schedule metadata / active-creator evidence:

```text
$ python3 - <<'PY'
from pathlib import Path
files=['crates/nexus-daemon-runtime/src/api/handlers/works.rs','crates/nexus42/src/commands/creator/run.rs']
for f in files:
    text=Path(f).read_text()
    print('---', f)
    for needle in ['build_schedule_for_stage','fl_e_stage','active_creator_id','creator_id']:
        print(f'{needle}:', needle in text)
PY
--- crates/nexus-daemon-runtime/src/api/handlers/works.rs
build_schedule_for_stage: False
fl_e_stage: False
active_creator_id: True
creator_id: True
--- crates/nexus42/src/commands/creator/run.rs
build_schedule_for_stage: True
fl_e_stage: True
active_creator_id: True
creator_id: True
```

Additional test evidence:

- `fl_e_chain_demo` passed 11/11, covering 5-stage mapping and preset input propagation.
- `fl_e_schedule_api` passed 5/5, covering schedule creation DTO shape, persisted schedule rows, seeded core context, and empty-creator isolation.
- QC1 final revalidation states CLI stage advance now sources `creator_id` from `config.active_creator_id`, not the Work DTO.

### Result

**Pass.** The 5-stage preset mapping, schedule creation/metadata behavior, and active-creator sourcing align with spec §4 and the fix-wave requirements.

## Final Checklist

| Item | Result | Notes |
| --- | --- | --- |
| 1. Plan scope and status consistency | Pass w/ notes | `status.json` row is `InReview`; plan Markdown still `Todo` as PM-owned non-blocking note. |
| 2. CI/lint/test green | Pass | Required scoped cargo test and clippy passed. |
| 3. Key functionality tests | Pass | 11 FL-E chain tests, 5 schedule API tests, 15 nexus42 tests passed. |
| 4. QC consistency | Pass | Final qc1=`Approve`, qc2=`Approve`, qc3=`Approve w/ residuals`. |
| 5. Residual lifecycle | Pass | 4 root residual entries registered for P2. |
| 6. Git state | Pass | Branch contains all scoped P2 commits; worktree clean before report write; diff stat captured. |
| 7. Integration branch state | Pass | Sampled P2 commits are not ancestors of integration branch. |
| 8. Spec alignment | Pass | Spec §4 mapping and fix wave requirements verified by tests and source evidence. |

## Not Tested / Not Changed

- No manual daemon/CLI happy-path demo was run beyond the required automated tests.
- No business implementation files were modified by QA.
- QA changed only `.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qa.md`.
