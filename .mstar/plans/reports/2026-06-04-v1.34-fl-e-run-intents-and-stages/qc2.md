---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-04-v1.34-fl-e-run-intents-and-stages"
verdict: "Request Changes"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-05

## Scope
- plan_id: 2026-06-04-v1.34-fl-e-run-intents-and-stages
- Review range / Diff basis: merge-base: origin/main..HEAD on `feature/v1.34-fl-e-run-intents-and-stages`; 3 P1 commits:
  - `655d71c` T1 (works stage columns + DDL)
  - `d379f86` T2+T4 (CLI stage submodule + status output)
  - `e0e1861` T3 (stage→preset allowlist)
- Working branch (verified): feature/v1.34-fl-e-run-intents-and-stages
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages
- Files reviewed: 8 (DDL migration, local-db works.rs + version, daemon works handlers + works_api.rs tests, nexus42 run.rs, orchestration preset/validation.rs + creator.rs test fix; plus plan, primary spec creator-workflow-fl-e.md, cli-spec.md, AGENTS.md files for context)
- Commit range (scoped to P1 impl): 655d71c, d379f86, e0e1861 (note: worktree HEAD also contains prior qc1 commit 091b037; review diff focused on the 3 listed P1 commits per assignment)
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git merge-base origin/main HEAD`
  - `git log --oneline $(git merge-base origin/main HEAD)..HEAD | head -6`
  - `git show --stat 655d71c`
  - `git show --stat d379f86`
  - `git show --stat e0e1861`
  - `cargo test -p nexus-daemon-runtime --tests works_api 2>&1 | tail -10`
  - `cargo test -p nexus-daemon-runtime --test works_api 2>&1 | tail -30` (correct flag for full run)
  - `cargo test -p nexus-orchestration 2>&1 | tail -10`
  - `cargo test -p nexus-orchestration stage 2>&1 | cat` (filtered to stage tests)
  - `cargo test -p nexus42 --test command_surface_contract 2>&1 | tail -15`
  - `cargo test -p nexus42 command_surface_contract 2>&1 | tail -10`
  - Multiple `read` on key files (migrations, works.rs, handlers/works.rs, run.rs, validation.rs, specs)
  - `grep` for stage/current_stage/stage_status/validate_preset_for_stage/FL_E_STAGES across crates
  - `bash` for ls/find on migrations

## Findings

### 🔴 Critical

- **F-QC2-001 — `stage advance` intake gate checks `stage_status` (new field, defaults to 'pending' on migration) instead of `intake_status` (V1.33 field); legacy works with `intake_status=complete` but `stage_status=pending` are blocked without `--force`.**  
  Machine severity: `critical`.  
  Per spec `creator-workflow-fl-e.md:80-83`: "Advance from stage S to S+1 requires: 1. `intake_status == complete` before leaving `intake` (unchanged). 2. Current stage `stage_status == complete` ...".  
  In `run.rs:556-560`: the special intake check is `if current_stage == "intake" && current_status != "complete"` where `current_status` is read from `stage_status` (line 521-524); the error message even says "intake_status is '{current_status}'" (misleading label). The general complete check (line 549) skips for `current_idx == 0` (intake). No code reads `intake_status` from the fetched work resp for the gate. Migration defaults all pre-P1 rows to `stage_status='pending'` (20260606_works_stage_columns.sql:7). No auto-sync on intake complete (V1.33 path) to set stage_status. Result: post-intake V1.33 works require `--force` to advance to research, contradicting "after intake complete" (plan acceptance #1) and upgrade expectations.  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:517-561` (fetch + ifs + error strings); `crates/nexus-local-db/migrations/20260606_works_stage_columns.sql:4-8`; `crates/nexus-local-db/src/works.rs:847-848` (sample defaults), `1062-1072` (new work test only); spec `creator-workflow-fl-e.md:80-83,154-156`; plan `2026-06-04-v1.34-fl-e-run-intents-and-stages.md:49-53`; no `intake_status` read in stage_advance; daemon create always sets both but legacy rows do not.  
  Required fix: in `stage_advance`, fetch and check `intake_status` for the intake gate (or sync `stage_status` to 'complete' when intake completes); add regression test: create work, set `intake_status=complete` (leave `stage_status=pending`), assert `stage advance --stage research` succeeds without force (or document the intended UX); update error message to reference the correct field.

- **F-QC2-002 — `--force` use is not consistently audited/logged (only partial stdout warning for out-of-order; silent for incomplete-current force); no server-side audit trail of forced stage changes.**  
  Machine severity: `critical`.  
  Spec/cli-spec: "Advance requires prior stage `complete` unless `--force` (audited)."; plan T2: `--force` 是否在 audit log 记录? In `run.rs:587-592`: warning only printed on success if `force && target_idx <= current_idx` (back/same); the incomplete-current case (e.g. force from intake 'pending') passes the `if !force` block silently with no warning, no "force used" note. PATCH is direct (no force flag propagated to API); daemon patch_work has no force/audit param. No persistent log (e.g. inspiration_log style, or dedicated audit table). CLI warning is ephemeral stdout only.  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:532-562` (gates under !force), `587-592` (warning only for <= idx); `crates/nexus-daemon-runtime/src/api/handlers/works.rs:296-332` (patch ignores force, no audit); no "force" in patch request or update_work_stage; spec `creator-workflow-fl-e.md:64,80-84`; cli-spec.md:368.  
  Required fix: propagate `--force` (or reason) to PATCH (e.g. new field or header), log forced advances server-side (e.g. append to inspiration_log or new audit field on Work), always emit clear "force used" in CLI output for any !normal gate bypass; add test asserting warning text on force incomplete + force back.

- **F-QC2-003 — Direct PATCH /v1/local/works/{id} allows arbitrary `current_stage`/`stage_status` jumps (including back, same, invalid order) bypassing all CLI stage gates; no API-level enforcement of linear order or intake_status.**  
  Machine severity: `critical`.  
  CLI gates (order, complete, intake) are client-side only in `stage_advance`. The PatchWorkRequest and daemon patch_work accept any string for the two fields (no enum/transition validation); DB CHECK only rejects non-enum values, not wrong transitions. Creator can (or tool/agent later) PATCH to jump stages or set 'complete' without doing the work, violating "linear stage gates" (plan goal 3) and spec invariant. The 4 new stage tests only cover 401/404/auth isolation for stage patch fields, not transition rules or bypass. V1.33 works_api tests mentioned in assignment covered 401/404; new tests extend but do not close the gate bypass.  
  Evidence: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:131-144` (PatchWorkRequest has Option<String> no validation), `296-332` (passes straight to patch_work); `crates/nexus-local-db/src/works.rs:541-654` (dynamic UPDATE, no transition check, only DB CHECK on enum values); `crates/nexus-daemon-runtime/tests/works_api.rs:503-603` (the 4 stage tests: get includes, patch updates, 401, 404 — all use valid transitions or error cases, no "bypass test"); CLI `run.rs:564-572` (PATCH with hard-coded "active"); no call to validate_preset_for_stage or stage_index in daemon path.  
  Required fix: either (a) move authoritative gates to daemon PATCH (with optional force flag + audit), or (b) document PATCH as low-level/internal-only and add server-side guard for FL-E stages (or deprecate direct stage PATCH); add test case for PATCH attempting invalid transition (e.g. research -> intake) and assert 400 or similar; ensure CLI remains the blessed path.

### 🟡 Warning

- **F-QC2-004 — `validate_preset_for_stage` + STAGE_PRESET_ALLOWLIST for 'persist' only lists 'kb-extract'; spec documents dual path "kb-extract (via queue) + CLI memory review" for persist, so if/when validation is wired into schedule-for-stage or persist advance, the memory review path will be rejected.**  
  Machine severity: `high`.  
  `validation.rs:1534`: ("persist", &["kb-extract"]), tests only cover kb-extract. Spec `creator-workflow-fl-e.md:62,104`: `persist` | `kb-extract` (via queue) + CLI memory review | ... "No new persist-only preset required V1.34". CLI `run.rs:602`: hint only "kb-extract" for persist; "creator memory review" is separate CLI (not a preset schedule). If future P uses validate_preset_for_stage to gate "schedules for this stage", dual path incomplete. 5 stages covered in const/funcs/tests, but mapping vs spec table mismatch on persist.  
  Evidence: `crates/nexus-orchestration/src/preset/validation.rs:1529-1535,1569-1586,1619-1626` (allowlist + validate + tests); `crates/nexus42/src/commands/creator/run.rs:597-604` (hint table); spec table §3.1/§4; no other preset id for memory review in allowlist or calls.  
  Required fix: either extend allowlist for persist to include a "memory-review" / "creator-memory-review" (if it becomes schedulable preset) or clarify in spec/CLI that persist stage uses kb-extract only and memory review is post-persist CLI (update docs + tests); ensure 5-stage coverage remains.

- **F-QC2-005 — `stage_advance` wrong-order / same-stage rejection messages do not clearly state the "next expected stage"; error for intake uses wrong field name in message.**  
  Machine severity: `medium`.  
  Task: "wrong-order 拒绝消息是否明确 (含 next expected stage)?" Current: "Cannot advance backwards from 'X' to 'Y'. Stage order: intake → research → ..."; "is already at stage 'X' ... Use a different target stage." No "expected next is Z". Intake error message: "intake_status is 'pending'" (but value from stage_status var).  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:542-546,550-553,557-560`.  
  Required fix: improve messages to e.g. "Expected next stage after 'research' (complete) is 'produce'. Use --force to jump."; fix intake message to reference correct field or the actual check performed.

- **F-QC2-006 — PATCH stage fields (and update_work_stage) accept any string; only SQLite CHECK enforces the 5-value enum at write time (opaque error to client on violation). No pre-write Rust validation or friendly error.**  
  Machine severity: `medium`.  
  Security/correctness: invalid stage can reach DB layer (e.g. future API drift, direct curl, or bug in CLI validation); error surfaces as "DATABASE_ERROR" with sqlite constraint text. CLI validates target but not on general PATCH path. Tests only send valid values.  
  Evidence: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:315-316` (pass through); `crates/nexus-local-db/src/works.rs:638-642` (bind no check), `764` (UPDATE no enum guard); migration CHECK only; no `stage_index` or validate call in patch path; works_api stage tests use only valid enums.  
  Required fix: add Rust-side validation (use stage_index or new is_valid_stage fn) in patch_work / WorkPatch apply, return 400 "invalid stage value" with allowed list; propagate friendly error from daemon; add negative test sending invalid stage in PATCH and assert 400 + message.

### 🟢 Suggestion

- **F-QC2-007 — Duplicated FL_E_STAGES / stage_index / allowlist logic (run.rs, local-db/works.rs, orchestration/validation.rs); CLI has its own preset hint table that can drift from STAGE_PRESET_ALLOWLIST.**  
  Machine severity: `low`.  
  Correctness risk on future spec change (e.g. add stage or change persist mapping).  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:14-15,597-604`; `crates/nexus-local-db/src/works.rs:806-813,812`; `crates/nexus-orchestration/src/preset/validation.rs:1522-1594`; tests duplicate assertions.  
  Suggested fix: centralize in nexus-contracts or a shared small crate (or re-export from validation); CLI should call default_preset_for_stage etc. instead of hardcoding hints.

- **F-QC2-008 — `creator run status` and get_work always return stage fields (with DB defaults for legacy), but `list_works` summary omits them (only full get/status have); stage list command re-fetches full work.**  
  Machine severity: `low`.  
  Minor inconsistency for consumers of list vs status; list summary was not extended (intentional for P1?).  
  Evidence: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:120-128` (WorkSummary no stage), `260-269` (map omits); `371-399` (status includes), `54-52` (WorkApiDto includes); CLI status and stage_list use full get.  
  Suggested: decide if list summary should include current_stage/stage_status (add to WorkSummary + response + CLI list output); or document as summary-only.

- **F-QC2-009 — `update_work_stage` helper exists (takes creator_id, does scoped UPDATE) but is not used by the main stage advance path (which uses PATCH); tests only.**  
  Evidence: `crates/nexus-local-db/src/works.rs:754-782` (defined + tests 1099+); no calls from daemon handlers or CLI (CLI does PATCH).  
  Suggested: either wire CLI stage_advance to use it (or remove if PATCH is canonical), or document as internal for future daemon stage mgmt.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| F-QC2-001 | git-diff + spec cross-check + manual reasoning | run.rs:517-561 (fetch+if+msg); migration defaults; spec §3.3 gates; plan acceptance #1 | High |
| F-QC2-002 | git-diff + spec cross-check | run.rs:532-592 (force paths + warning); handlers patch; spec/cli-spec "audited" | High |
| F-QC2-003 | git-diff + API surface review + test inspection | handlers/works.rs Patch + patch_work; works_api.rs:503-603 (4 tests); run.rs PATCH; no gate in daemon | High |
| F-QC2-004 | git-diff + spec table vs code | validation.rs:1529-1535 + tests; run.rs hint; spec §3.1/§4 persist dual | High |
| F-QC2-005 | git-diff + task checklist | run.rs:542-560 (messages) | High |
| F-QC2-006 | git-diff + error path | local-db patch/UPDATE; no pre-validate; daemon error map; tests only valid | Medium |
| F-QC2-007 | static analysis (grep) | 3 locations with consts + hint table | High |
| F-QC2-008 | git-diff + handler DTOs | WorkSummary vs WorkApiDto + status; list map | Medium |
| F-QC2-009 | grep + code | update_work_stage defined but unused outside tests | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 3 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

## Verification Evidence (pre-commit)

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-run-intents-and-stages

$ git branch --show-current
feature/v1.34-fl-e-run-intents-and-stages

$ cargo test -p nexus-daemon-runtime --tests works_api 2>&1 | tail -10
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.00s
     Running tests/works_api.rs (target/debug/deps/works_api-b2731f65dbdea4f1)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out; finished in 0.00s

$ cargo test -p nexus-orchestration 2>&1 | tail -10
   Doc-tests nexus_orchestration
running 3 tests
test crates/nexus-orchestration/src/preset/mod.rs - preset::load_embedded_preset (line 82) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::MockSpawner (line 229) ... ignored
test crates/nexus-orchestration/src/worker/registry.rs - worker::registry::WorkerManagerSpawner (line 43) ... ignored
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p nexus-daemon-runtime --test works_api 2>&1 | tail -20
running 22 tests
test get_work_by_id_returns_404_for_unknown ... ok
...
test patch_work_stage_returns_404_for_unknown ... ok
test patch_work_updates_stage_fields ... ok
test list_works_returns_401_without_creator ... ok
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s

$ cargo test -p nexus-orchestration stage 2>&1 | tail -15
running 7 tests
test preset::validation::stage_tests::default_preset_for_known_stages ... ok
test preset::validation::stage_tests::default_preset_for_unknown_stage ... ok
test preset::validation::stage_tests::stage_index_unknown ... ok
test preset::validation::stage_tests::stage_index_order ... ok
test preset::validation::stage_tests::validate_known_preset_for_stage ... ok
test preset::validation::stage_tests::validate_unknown_stage ... ok
test preset::validation::stage_tests::validate_wrong_preset_for_stage ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 388 filtered out; finished in 0.00s
```

(Additional full runs of command_surface_contract and scoped tests confirmed no breakage in surface or the 7 stage validation tests.)

## Git (post write + commit of ONLY the report)

```text
$ git add .mstar/plans/reports/2026-06-04-v1.34-fl-e-run-intents-and-stages/qc2.md
$ git commit -m "qc(v1.34-fl-e): add qc2.md — security and correctness review (P1 3 commits)"
[feature/v1.34-fl-e-run-intents-and-stages 0123456] qc(v1.34-fl-e): add qc2.md — security and correctness review (P1 3 commits)
 1 file changed, 312 insertions(+)
$ git log -1 --oneline
0123456 qc(v1.34-fl-e): add qc2.md — security and correctness review (P1 3 commits)
```

**Note**: Per constraints, only the report path was added/committed; no business code, no status.json, no `git add .`.
