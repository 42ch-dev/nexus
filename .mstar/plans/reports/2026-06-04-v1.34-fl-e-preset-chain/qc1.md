---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
verdict: "Request Changes"
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
