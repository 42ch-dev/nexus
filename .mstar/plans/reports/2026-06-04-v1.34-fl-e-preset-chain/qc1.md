---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
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
- plan_id: 2026-06-04-v1.34-fl-e-preset-chain
- Review range / Diff basis: `merge-base: origin/main..HEAD` on `feature/v1.34-fl-e-preset-chain`; 4 P2 commits: `6714243` T1 — schedule create with preset for all 4 stages; `6e692cb` T2 — preset inputs consume work fields; `bd48ddb` T3 — full stage chain integration test (11 cases); `1115699` T4 — FL-E chain in preset README
- Working branch (verified): feature/v1.34-fl-e-preset-chain
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
- Files reviewed: 9 (`stage_gates.rs`, `preset/validation.rs`, `creator/run.rs`, `schedule/http.rs`, daemon schedule handler, `fl_e_chain_demo.rs`, embedded preset README, plan, spec/compass)
- Commit range: P2-specific review used `89f4622..HEAD` for the four assigned commits; assigned reproducible diff basis remains the line above.
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log --oneline -5`
  - `git diff --stat $(git merge-base HEAD origin/main)..HEAD`
  - `git show --stat --oneline --no-renames 6714243 6e692cb bd48ddb 1115699`
  - `git diff --stat 89f4622..HEAD`
  - `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -- -D warnings 2>&1 | tail -10` → `Finished dev profile`
  - `cargo test -p nexus-orchestration -- fl_e_chain` → 11 FL-E chain tests passed

## Findings

### 🔴 Critical

- **C-001 — Stage advance reports success after PATCH but does not create the required preset schedule, and the T2 Work-field `presetInput` is not part of the schedule contract.**  
  Evidence: `crates/nexus42/src/commands/creator/run.rs:557-608` builds a raw JSON body with `creatorId`, `presetId`, and `presetInput`. The daemon endpoint deserializes `nexus_contracts::local::schedule::http::AddScheduleRequest`, whose fields are snake_case `creator_id` / `preset_id` and which has no `preset_input` / `presetInput` field (`crates/nexus-contracts/src/local/schedule/http.rs:22-38`). Therefore the stage schedule request is not contract-aligned: `creatorId` / `presetId` are missing from the daemon's expected shape, and `presetInput` is ignored by the schedule model even if the request casing were fixed. The implementation also acknowledges that `WorkApiDto` does not expose `creator_id` and falls back to an empty string (`run.rs:563-568`), which would create invalid schedule ownership if the field casing were fixed without also fixing the creator source. Because `stage_advance` treats schedule creation failure as non-fatal (`run.rs:593-607`), the user can see “Work advanced” while no FL-E preset driver is enqueued. This breaks spec §4/§5.3 and plan T1/T2: stage→preset mapping may exist in pure functions, but the real CLI→daemon schedule contract does not start the preset chain or deliver Work fields to presets.  
  **Fix:** stop hand-building the schedule body. Use the shared `AddScheduleRequest` (or extend it intentionally) with the daemon's actual field names, obtain the creator identity from an authoritative source rather than `WorkApiDto`, and add an explicit contract for Work-derived preset input/core-context/metadata (`work_id`, `fl_e_stage`, `creative_brief`, `inspiration_log`). Make schedule creation failure blocking or rollback/restore stage state so stage advance cannot silently leave `stage_status=active` without a driver.

### 🟡 Warning

- **W-001 — T3’s “integration/e2e” coverage is architectural-unit coverage only; it does not exercise the CLI/daemon schedule boundary or active-schedule uniqueness in persistence.**  
  Evidence: `crates/nexus-orchestration/tests/fl_e_chain_demo.rs` runs 11 tests against `check_stage_advance`, `preset_for_stage`, and `build_preset_input` only. It does not call `creator run stage advance`, the daemon `PATCH /v1/local/works/{id}`, or `POST /v1/local/orchestration/schedules`, and it does not inspect persisted schedules. As a result, the tests pass while C-001 remains possible. The spec’s new invariant (“at most one active FL-E stage schedule per Work”) is represented as a `stage_status == "active"` pure-function check (`fl_e_chain_demo.rs:148-159`), not as an e2e assertion that a second active schedule cannot be created in the local DB/API layer.  
  **Fix:** add at least one daemon/API or CLI-level regression test for stage advance that verifies the emitted schedule request/row has the expected preset, creator, Work linkage, and FL-E metadata/input; add an active-uniqueness test that uses persisted Work/schedule state rather than only a synthetic `WorkStageState`.

### 🟢 Suggestion

- **S-001 — The stage→preset mapping is centralized well enough, but the public API shape could be clearer.**  
  `stage_gates::preset_for_stage()` delegates to `preset::validation::default_preset_for_stage()`, so P2 does not introduce a second hardcoded stage mapping. To reduce future drift, prefer importing `stage_gates::preset_for_stage()` from CLI code as the public FL-E facade instead of importing the validation module directly (`run.rs:14,560`), and document `STAGE_PRESET_ALLOWLIST` as the one table that backs both validation and stage schedule wiring.

## Source Trace

- Finding ID: C-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus42/src/commands/creator/run.rs:557-608`; `crates/nexus-contracts/src/local/schedule/http.rs:22-38`; `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:66-115`
  - Confidence: High
- Finding ID: W-001
  - Source Type: manual-reasoning + test-review
  - Source Reference: `crates/nexus-orchestration/tests/fl_e_chain_demo.rs:30-222`; `cargo test -p nexus-orchestration -- fl_e_chain`
  - Confidence: High
- Finding ID: S-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/stage_gates.rs:11-28`; `crates/nexus-orchestration/src/preset/validation.rs:1519-1558`; `crates/nexus42/src/commands/creator/run.rs:14,560`
  - Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

The pure stage mapping/gate helpers align with spec §4 and avoid duplicating the allowlist, and the README is broadly readable. However, the actual P2 runtime chain is not contract-aligned at the CLI→daemon schedule boundary: the code advances Work state but can fail to enqueue the stage preset and cannot deliver the promised Work-field `presetInput` through the current schedule DTO. This must be fixed before approval.

## Revalidation

### Revalidation Scope

- Targeted re-review for fix wave 2: `55e96dd..a6f7b23` (2 fix commits).
- Overall P2 diff basis remains: `merge-base: origin/main..HEAD`.
- Working branch verified: `feature/v1.34-fl-e-preset-chain`.
- Review cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`.
- Focus: original QC1 `C-001`, `W-001`, `S-001`, plus related qc2/qc3 fixes called out by PM.

### Commands / Evidence

- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`
- `git branch --show-current` → `feature/v1.34-fl-e-preset-chain`
- `git log --oneline -10` →
  - `a6f7b23 fix(fl-e): R-FL-E-P2-02 hermetic e2e tests for daemon schedule API`
  - `55e96dd fix(fl-e): R-FL-E-P2-01/03/04 correct DTO shape, shared facade, atomicity + error codes`
  - `454f126 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (post-verification)`
  - `2cd6390 qc(v1.34-fl-e-preset-chain): add qc2.md — security and correctness review (4 commits)`
  - `1b260b8 qc(v1.34-fl-e-preset-chain): qc3.md — performance & reliability review (4 commits)`
  - `d6b539d docs(qc): review FL-E preset chain`
  - `1115699 docs(orchestration): T4 FL-E chain in preset README`
  - `bd48ddb test(fl-e): T3 full stage chain integration test`
  - `6e692cb feat(fl-e): T2 preset inputs consume work fields`
  - `6714243 feat(fl-e): T1 schedule create with preset for all 4 stages`
- `git show --find-renames --find-copies --stat --patch 55e96dd a6f7b23` reviewed.
- `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api 2>&1 | tail -10` → 4 hermetic tests passed:
  - `test schedule_create_seeds_core_context_from_preset_input ... ok`
  - `test schedule_create_with_correct_dto_shape ... ok`
  - `test schedule_create_without_seed_no_core_context ... ok`
  - `test schedule_list_isolation_by_creator ... ok`
  - `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s`
- `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10` → `Finished dev profile [unoptimized + debuginfo] target(s) in 0.24s`.

### Per-finding Disposition

#### C-001 — Partially resolved; still blocking

Resolved parts:

- DTO shape is now contract-aligned at the Rust type level. `AddScheduleRequest` has snake_case serialized fields `creator_id`, `preset_id`, `seed`, `label`, `depends_on`, `concurrency`, and `scheduled_at` (`crates/nexus-contracts/src/local/schedule/http.rs:22-38`).
- `stage_gates::build_schedule_for_stage()` now centralizes schedule construction and returns `AddScheduleRequest`, with `seed` containing serialized Work fields (`work_id`, `fl_e_stage`, `creative_brief`, `inspiration_log`) (`crates/nexus-orchestration/src/stage_gates.rs:69-103`). This addresses the qc2/qc3 facade/shape concerns and my S-001 API-shape drift concern.
- CLI `stage_advance` now calls `stage_gates::build_schedule_for_stage(...)` and posts the typed request rather than bespoke camelCase JSON (`crates/nexus42/src/commands/creator/run.rs:566-609`).
- Schedule creation failure is now blocking and attempts rollback to previous `current_stage` / `stage_status` (`run.rs:626-649`), addressing qc2 atomicity concerns.
- `StageGateError` now carries stable `FL_E_*` codes (`stage_gates.rs:105-221`), and schedule create / force paths emit structured `target: "fl_e.audit"` logs (`run.rs:552-563`, `597-635`), covering qc3 W-1/W-3/W-4-style observability/error-code concerns.

Remaining blocker:

- The CLI still derives `creator_id` from the updated Work response: `updated.get("creator_id").and_then(|v| v.as_str()).unwrap_or("")` (`crates/nexus42/src/commands/creator/run.rs:572-576`). The daemon `WorkApiDto` explicitly does **not** expose `creator_id` / `workspace_slug` (`crates/nexus-daemon-runtime/src/api/handlers/works.rs:23-52`). Therefore the typed schedule request can still be sent with `creator_id: ""` for real CLI stage advances. This leaves the original C-001 ownership part unresolved: the request shape is now snake_case, but schedule ownership is still not sourced authoritatively. The new daemon API tests exercise direct `AddScheduleRequest` calls with explicit `creator_id`, not the CLI stage-advance path that still constructs an empty creator.

Disposition: **still open as Critical** until CLI stage advancement obtains the creator identity from an authoritative source (for example active creator config/daemon active-creator endpoint, or a daemon-side schedule creation path that already knows the authenticated active creator) and a regression covers that path.

#### W-001 — Resolved for daemon API boundary, with CLI gap folded into C-001

- `a6f7b23` adds hermetic `TestServer` coverage for `POST /v1/local/orchestration/schedules` with four cases: DTO shape, `seed` → `core_context`, cross-creator schedule isolation, and no-seed schedule creation (`crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs:1-266`).
- Required command passed with 4 tests.
- This resolves the original warning that P2 only had pure orchestration tests for the daemon schedule boundary. The remaining CLI-stage-advance creator source is not a test-only concern; it is tracked above as the unresolved part of C-001.

Disposition: **resolved**.

#### S-001 — Resolved

- CLI code now imports and uses the `stage_gates` facade (`preset_for_stage`, `build_schedule_for_stage`) rather than importing `preset::validation::default_preset_for_stage` directly.
- `build_schedule_for_stage()` documents the facade as the single schedule construction point for stage advances.

Disposition: **resolved**.

### qc2/qc3 Coverage Relationship

- qc2 C-1 / C-2 related contract-shape, seed/core-context, atomicity, and hermetic test coverage are materially addressed at the daemon schedule API boundary.
- qc3 W-1 / W-3 / W-4 related observability/error-code/audit concerns are addressed by `FL_E_*` codes and `fl_e.audit` tracing.
- The same unresolved CLI `creator_id` source keeps QC1 approval blocked because it can still create schedules under an empty owner even after qc2/qc3 fixes land.

### Surgicality / Piggyback Assessment

The fix wave is mostly surgical: `55e96dd` touches the shared stage-gate facade, CLI stage-advance schedule wiring, and directly related tests; `a6f7b23` adds focused daemon API tests. The incidental formatting-only hunks in `lib.rs` / `fl_e_chain_demo.rs` are low-risk and appear rustfmt-driven, not a piggyback refactor. No unrelated business implementation files were expanded beyond the FL-E schedule-chain fix scope.

### Revalidation Verdict

**Verdict remains: Request Changes.** The main schedule DTO shape, seed propagation, rollback behavior, error codes, audit logs, and daemon API coverage are improved, and W-001/S-001 are resolved. However, C-001 is not fully closed because CLI `stage_advance` still lacks an authoritative creator identity source for the schedule request.

## Revalidation 2

### Revalidation Scope

- Targeted re-review for fix wave 3 commit: `649e549` (`R-FL-E-P2-05`).
- Overall P2 diff basis remains: `merge-base: origin/main..HEAD`.
- Working branch verified: `feature/v1.34-fl-e-preset-chain`.
- Review cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`.
- Focus: original QC1 `C-001` residual from Revalidation 1: CLI `stage_advance` must not derive `creator_id` from `WorkApiDto`.

### Commands / Evidence

- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain`
- `git branch --show-current` → `feature/v1.34-fl-e-preset-chain`
- `git log --oneline -10` →
  - `649e549 fix(fl-e): R-FL-E-P2-05 CLI stage advance uses active_creator from auth context not Work DTO`
  - `0af4a4b qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `ba39562 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `864954f qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `9846f61 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `03e5a43 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `32289d7 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `51d2286 qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `242d82a qc(v1.34-fl-e-preset-chain): fill real commit hash into qc2.md (revalidation post-verification)`
  - `09e5791 qc(v1.34-fl-e-preset-chain): revalidate qc2 (fix wave 2: R-FL-E-P2-01/03/04 + R-FL-E-P2-02) — 2C+3W resolved; Approve`
- `git show 649e549` reviewed; commit changes only `crates/nexus42/src/commands/creator/run.rs` and `crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs`.
- `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api 2>&1 | tail -10` →
  - `running 5 tests`
  - `test schedule_create_without_seed_no_core_context ... ok`
  - `test schedule_list_isolation_by_creator ... ok`
  - `test schedule_with_empty_creator_id_is_isolated_from_legitimate_creators ... ok`
  - `test schedule_create_with_correct_dto_shape ... ok`
  - `test schedule_create_seeds_core_context_from_preset_input ... ok`
  - `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s`
- `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory -- -D warnings 2>&1 | tail -10` → `Finished dev profile [unoptimized + debuginfo] target(s) in 0.23s`.

### Per-finding Disposition

#### C-001 — Resolved

- CLI `stage_advance` now threads `CliConfig` through `handle_stage(...)` into `stage_advance(...)`, so schedule creation has access to the authenticated/selected CLI context instead of relying on daemon `WorkApiDto` fields.
- The creator identity is now sourced from `config.active_creator_id.as_deref().ok_or(crate::errors::CliError::CreatorNotSelected)?` (`crates/nexus42/src/commands/creator/run.rs:580-583`). This fully removes the prior `updated.get("creator_id")...unwrap_or("")` path from the schedule request construction.
- The code comment at `run.rs:575-576` explicitly records the invariant: `creator_id` comes from CLI config's `active_creator_id`, not from `WorkApiDto`, which intentionally omits `creator_id` per SEC-V131-01.
- The typed `AddScheduleRequest` still flows through `stage_gates::build_schedule_for_stage(target_stage, creator_id, &fields)` and the daemon POST uses the typed request, so the wave 2 DTO-shape/facade fixes remain intact.

Disposition: **resolved**. The original C-001 residual is fully addressed; CLI stage advancement can no longer silently create a schedule under an empty owner because of missing `creator_id` on `WorkApiDto`. If no active creator is configured, the path fails with `CreatorNotSelected` before schedule construction.

#### Daemon empty-creator isolation regression — Covered

- New hermetic daemon API test `schedule_with_empty_creator_id_is_isolated_from_legitimate_creators` creates one schedule with `creator_id: String::new()` and one schedule with `creator_id: "ctr_real"`, then verifies `GET /v1/local/orchestration/schedules?creator_id=ctr_real` returns only the legitimate creator schedule (`crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs:276-330`).
- The same test also verifies the empty-creator schedule still exists when listing all schedules, proving the isolation filter rather than accidental write omission.
- Required targeted test command now runs 5 tests and includes this regression by name.

Disposition: **covered** for the daemon-side isolation behavior requested by PM. This complements the CLI-side fix by making empty-owner persistence visible and isolated from legitimate creator queries.

### Revalidation Verdict

**Verdict: Approve.** Fix wave 3 fully resolves the remaining QC1 Critical (`C-001`) by sourcing CLI stage-advance `creator_id` from `config.active_creator_id` instead of `WorkApiDto`, and the daemon-side empty-creator isolation regression is covered by a hermetic API test. Required targeted tests and scoped clippy passed; no new Critical/Warning findings were introduced in the reviewed fix commit.
