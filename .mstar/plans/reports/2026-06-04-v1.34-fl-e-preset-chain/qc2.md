---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
verdict: "Request Changes"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-05

## Scope
- plan_id: 2026-06-04-v1.34-fl-e-preset-chain
- Review range / Diff basis: `merge-base: origin/main..HEAD` on `feature/v1.34-fl-e-preset-chain`; 4 P2 commits:
  - `6714243` T1 — schedule create with preset for all 4 stages
  - `6e692cb` T2 — preset inputs consume work fields
  - `bd48ddb` T3 — full stage chain integration test (11 cases)
  - `1115699` T4 — FL-E chain in preset README
- Working branch (verified): feature/v1.34-fl-e-preset-chain
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
- Files reviewed: 5 (stage_gates.rs, lib.rs, creator/run.rs, fl_e_chain_demo.rs, embedded-presets/README.md) + cross-referenced contracts (AddScheduleRequest), daemon handlers (works.rs, schedules.rs), local-db schema, spec §4, P1 stage_gates baseline
- Commit range (P2 feature changes): 89f4622..1115699 (the 4 assigned commits; later QC report commits on branch are out of scope for this code review)
- Tools run:
  - `git rev-parse --show-toplevel` → /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
  - `git branch --show-current` → feature/v1.34-fl-e-preset-chain
  - `git merge-base origin/main HEAD` → 5b71318aa8cd2e91e3115820dec7eac71869f261
  - `git log --oneline 89f4622..HEAD` (captured P2 4 commits + later metadata commits)
  - `git diff 89f4622..6714243 -- crates/nexus-orchestration/src/stage_gates.rs`
  - `git diff 6714243..6e692cb -- crates/nexus-orchestration/src/stage_gates.rs -- crates/nexus42/src/commands/creator/run.rs`
  - `git diff 89f4622..1115699 -- crates/nexus-orchestration/embedded-presets/README.md`
  - `git show --stat 6714243 6e692cb bd48ddb 1115699`
  - `cargo test -p nexus-orchestration --test fl_e_chain_demo 2>&1 | tail -15` → 11 tests passed (see evidence below)
  - `cargo test -p nexus-orchestration --lib preset_for_stage 2>&1 | tail -10` → unit tests for T1/T2 helpers passed
  - `cargo clippy -p nexus-orchestration -- -D warnings 2>&1 | tail -5` → clean (Finished dev profile)
  - Multiple `read` + `grep` on stage_gates.rs, run.rs, contracts http.rs, daemon works.rs + schedules.rs, local-db migration, spec creator-workflow-fl-e.md §4, plan, qc1.md (for context only)
  - `ls .mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/`

## Findings

### 🔴 Critical

- **C-001 (correctness, contract violation) — T1/T2 schedule creation for the 4 FL-E stages (research/produce/review/persist) uses a request shape that cannot be deserialized by the daemon and therefore never enqueues a preset driver.**  
  Evidence (from the 4 commits):  
  - `crates/nexus42/src/commands/creator/run.rs:581-591` (T2 diff) hard-codes camelCase + extra field:  
    ```json
    { "creatorId": "...", "presetId": pid, "label": build_..., "presetInput": { "work_id": ..., "fl_e_stage": ..., "creative_brief": ..., "inspiration_log": ... } }
    ```
  - `crates/nexus-contracts/src/local/schedule/http.rs:22-38` (pre-existing, unchanged by P2): `AddScheduleRequest` expects `creator_id: String, preset_id: String, ...` (no `preset_input` / `presetInput`, no camelCase support).  
  - `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:66-115` (add_schedule): takes `body.creator_id`, `body.preset_id`; never sees the Work fields; if seed present it only seeds core_context — stage schedule_body sends neither seed nor a supported input field.  
  - CLI then does `if err { eprintln!("Warning: failed to create stage schedule: {e}"); }` (non-fatal, lines ~603-607) and proceeds to report success + `stage_status=active`.  
  - Result: `stage_advance` for any of the 4 stages advances the Work (via shared gate + PATCH) but the "schedule create with preset" (T1) and "preset inputs consume work fields" (T2) never happen. The preset chain (spec §4, §5.3, plan T1/T2) is not triggered.  
  - `preset_for_stage` and `build_preset_input` (T1/T2) are only called from unit tests and the broken CLI path; `default_preset_for_stage` (in validation.rs, allowlist) is correct per spec §4 but never reached for stage schedules at runtime.  
  - Daemon direct PATCH path (`works.rs:417-421`, `patch_work_stage:325-333`) calls the shared gate (with force=true) and `advance_work_stage_atomic` but contains **zero** code that creates a stage schedule. Only the CLI `stage_advance` subcommand attempts it (and that attempt is broken).  

  This is a direct violation of the P2 acceptance ("End-to-end stage chain runnable", "schedule metadata 含 work_id + fl_e_stage") and spec §5.3 power-user + "schedules created via creator run / stage advance must record...". The 4 stages are never "各自被正确触发" in the real system.

- **C-002 (correctness, test gap) — T3 "full stage chain integration test (11 cases)" does not cover the runtime schedule creation, presetInput delivery, or active-schedule uniqueness in persistence.**  
  Evidence: `crates/nexus-orchestration/tests/fl_e_chain_demo.rs:30-222` (the entire T3 commit) only exercises the pure functions `check_stage_advance`, `preset_for_stage`, `build_preset_input` against synthetic `WorkStageState` / `WorkFields`.  
  - No call to `creator run stage advance`, no `DaemonClient`, no `POST /v1/local/orchestration/schedules`, no inspection of `creator_schedules` rows or core_context_versions.  
  - The "active schedule uniqueness" case (fl_e_chain_reject_advance_while_active) only checks the in-memory `stage_status == "active"` gate; it does not prove that a second active schedule cannot be inserted for the same work_id in the real DB.  
  - `cargo test -p nexus-orchestration --test fl_e_chain_demo` (11 passed) is therefore necessary but not sufficient evidence for T3 acceptance.  

  Combined with C-001, the e2e claim in plan/spec is unverified.

### 🟡 Warning

- **W-001 (security / correctness, auth scoping) — Schedule creation (including the stage path) does not force `active_creator` to match the `creator_id` supplied in the request body.**  
  Evidence:  
  - All work/memory paths (`works.rs:283,414`, `memory.rs` etc.) do `let creator_id = read_active_creator_id(...)?;` then scope `get_work(creator_id, work_id)` and `patch_work`.  
  - `schedules.rs:67-115` (add_schedule) and the insert path take `body.creator_id` verbatim with no re-read of active_creator and no cross-check.  
  - In the stage_advance path the protection is only indirect (the preceding GET/PATCH /works will fail with NotFound/Auth if the work does not belong to the current active creator; the creator_id is then copied from the work response).  
  - A direct (or future agent-tool) `POST /v1/local/orchestration/schedules` with an arbitrary creator_id can therefore create a schedule "owned" by a different creator than the caller's active session. This widens the attack surface for the new stage-schedule creation path that P2 introduces.  
  - Related: in run.rs:563-568 the code already acknowledges "WorkApiDto does not expose creator_id; fall back to empty string" — so even if the casing were fixed, a stage schedule could be created with empty creator_id.

- **W-002 (correctness) — No structured `work_id` / `fl_e_stage` (or the T2 Work fields) ever reaches persisted schedule state or preset core context.**  
  Evidence:  
  - creator_schedules table (20260419 migration) has only `label`, `creator_id`, `preset_id`, ... — no work_id / fl_e_stage columns.  
  - The only place the values appear is the human label ("FL-E stage: research (work: wrk_xxx)") and the JSON that is never deserialized.  
  - core_context is only seeded when `seed` is present in AddScheduleRequest; stage schedule_body sends none.  
  - Therefore the claim "schedule metadata 含 `work_id` + `fl_e_stage`" and "preset inputs consume work fields" (T1/T2) is not true for any persisted schedule created by the new path.

- **W-003 (correctness, regression surface) — V1.33 `creator run start` / intake + novel-writing paths use the identical broken schedule-body pattern (camelCase + presetInput).**  
  Evidence: run.rs:174-181 (intake) and 217-223 (novel). P2 simply duplicated the anti-pattern for the 4 new stages. If a future fix makes schedule creation fatal, the old V1.33 paths will also break.

### 🟢 Suggestion

- **S-001 (maintainability) — Centralize the stage schedule body construction.**  
  Currently the JSON is hand-built in three places (intake, novel, stage_advance). After the contract is fixed, introduce a small helper (perhaps in nexus-orchestration or a shared CLI util) that produces a correctly-shaped `AddScheduleRequest` (or the wire JSON) given work fields + preset. This would have made the T1/T2 wiring obviously wrong in review.

- **S-002 (test hygiene) — Promote at least one of the T3 cases to a real integration test that exercises the CLI/daemon boundary.**  
  The 11 pure-function cases are valuable but, as W-001 notes, they do not protect the contract that actually starts the preset chain. Add a test (even if it requires a test daemon harness) that asserts the emitted schedule row has the expected preset, creator, Work linkage, and that preset input fields are visible in the resulting core_context (once the contract supports it).

- **S-003 (consistency with P1) — The shared `check_stage_advance` gate is correctly used by both CLI and daemon PATCH (good).**  
  The T1 addition of `preset_for_stage` + label + re-export is a clean facade. Consider also exporting `default_preset_for_stage` (or the whole validation module) via the stage_gates re-exports so callers do not reach into `preset::validation` directly (minor drift risk).

## Source Trace

- Finding ID: C-001  
  - Source Type: git-diff + contract review + handler audit  
  - Source Reference: 6714243 (T1 stage_gates + label), 6e692cb (T2 run.rs presetInput + build call + CLI schedule_body), `crates/nexus-contracts/src/local/schedule/http.rs:22-38`, `schedules.rs:101` (body.creator_id), `works.rs:325` (shared gate call in PATCH), `run.rs:560-608` (the if-let + post + warn)  
  - Confidence: High

- Finding ID: C-002  
  - Source Type: test inspection  
  - Source Reference: bd48ddb (full fl_e_chain_demo.rs), `cargo test -p nexus-orchestration --test fl_e_chain_demo` (11 passed, all unit)  
  - Confidence: High

- Finding ID: W-001  
  - Source Type: auth + scoping audit  
  - Source Reference: works.rs:283/414 (active read + scope), schedules.rs:67 (no active read), run.rs:502-540 (the GET/PATCH before schedule)  
  - Confidence: High

- Finding ID: W-002  
  - Source Type: schema + data-flow  
  - Source Reference: local-db/migrations/20260419_creator_schedules.sql:5-21 (no work_id/fl_e_stage), schedule creation paths, core_context seed logic only on "seed" field  
  - Confidence: High

- Finding ID: W-003 / S-001..S-003  
  - Source Type: manual-reasoning + diff + prior patterns  
  - Source Reference: run.rs intake/novel blocks (pre-P2), stage_gates.rs:26-28 (the delegation), 1115699 (README), spec §4 table, P1 stage_gates baseline  
  - Confidence: Medium-High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The pure helpers (`preset_for_stage`, `build_preset_input`, `check_stage_advance` extensions, label builder) and the 11-case unit test are locally correct and match spec §4 and the plan tasks on paper. However, the actual runtime schedule-creation path introduced by T1/T2 is contract-mismatched, never delivers the promised Work fields or structured metadata, is not exercised by the T3 "integration" test, and is absent from the daemon PATCH path. Consequently the core promise of P2 — "stage advance for the 4 stages correctly triggers the corresponding preset with work_id + fl_e_stage + creative_brief/inspiration_log" — is not achieved. The shared gate from P1 is used correctly, which is a positive, but does not compensate for the missing delivery of the preset driver.

All Critical items must be resolved (real contract-aligned schedule body, delivery of Work fields into the schedule/core-context, test that actually exercises the creation path, and either daemon PATCH also wires a schedule or explicit documentation that it does not) before this plan can be considered ready for QA or merge.

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: Security and correctness QC review of P2 (FL-E preset chain) 4 commits on branch `feature/v1.34-fl-e-preset-chain` (plan 2026-06-04-v1.34-fl-e-preset-chain)  
**Status**: Done  
**Scope Delivered**: Full review of the 4 assigned commits (T1-T4), cross-referenced P1 gates, contracts, daemon handlers, local-db schema, primary spec §4, and the plan. Verified cwd/branch alignment, ran mandated `cargo test -p nexus-orchestration --test fl_e_chain_demo` (11/11 passed) + clippy + unit tests for the new helpers. Produced this qc2.md.  
**Artifacts**: `.mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qc2.md` (committed)  
**Validation**:  
- `git rev-parse --show-toplevel` + `git branch --show-current` (in assigned worktree)  
- `cargo test -p nexus-orchestration --test fl_e_chain_demo 2>&1 | tail -15` (evidence captured)  
- `cargo clippy -p nexus-orchestration -- -D warnings` (clean)  
- `cargo test -p nexus-orchestration --lib preset_for_stage` (T1/T2 units pass)  
- All git diff / show / log commands for the exact 4 commits reproduced the changes under review  
**Issues/Risks**: Two Critical (contract mismatch preventing any stage schedule from being created; T3 test does not cover the claimed integration/runtime behaviour) plus three Warnings around auth scoping for schedules and missing structured metadata. These are the same class of issues flagged by qc1; they remain present in the 4 commits.  
**Plan Update**: None (QC does not edit plans or status.json).  
**Handoff**: To PM for consolidation with qc1 + qc3 and targeted re-review or fix dispatch. The report follows mstar-review-qc template and mstar-roles qc-specialist-2 parameters.  
**Git**: 2cd6390 qc(v1.34-fl-e-preset-chain): add qc2.md — security and correctness review (4 commits)  (initial commit of report; hash captured in verification step; only report path staged/committed)

## Evidence (fresh verification before Completion Report)

```bash
# cwd/branch alignment (mandatory acceptance)
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
$ git branch --show-current
feature/v1.34-fl-e-preset-chain

# Mandated e2e test (acceptance criteria)
$ cargo test -p nexus-orchestration --test fl_e_chain_demo 2>&1 | tail -15
running 11 tests
test fl_e_chain_force_allows_incomplete_current ... ok
...
test fl_e_chain_reject_unknown_stage ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

# Clippy (no new lints from P2)
$ cargo clippy -p nexus-orchestration -- -D warnings 2>&1 | tail -5
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.63s

# T1/T2 unit tests
$ cargo test -p nexus-orchestration --lib preset_for_stage 2>&1 | tail -10
test stage_gates::tests::preset_for_stage_intake ... ok
...
test result: ok. 8 passed; ...
```

(The 4 P2 commits under review are 6714243, 6e692cb, bd48ddb, 1115699; diffs were taken against 89f4622 (P3 merge base) to isolate exactly the assigned changes.)

## Git
2cd6390 qc(v1.34-fl-e-preset-chain): add qc2.md — security and correctness review (4 commits)

(Report committed with `git add .mstar/plans/reports/2026-06-04-v1.34-fl-e-preset-chain/qc2.md && git commit ...`; only the report path was staged per QC constraints. A follow-up edit+commit filled the real hash into this section.)
