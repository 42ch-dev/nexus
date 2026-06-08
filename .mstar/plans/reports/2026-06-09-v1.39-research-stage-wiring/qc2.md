---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-09-v1.39-research-stage-wiring"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (gates enforcement, creator isolation, run_intent impact, auto-chain helper reuse for research stage, status hint injection surface, test coverage of AC1–AC4)
- Report Timestamp: 2026-06-09

## Scope
- plan_id: 2026-06-09-v1.39-research-stage-wiring
- Review range / Diff basis: merge-base: 1b68d6ca (iteration/v1.39 HEAD with P0 closed) + tip: ea129914 (feature/v1.39-research-stage-wiring HEAD); equivalent to `git diff 1b68d6ca...ea129914` (run in the Review cwd). 1 commit, 4 files, +209 / -2.
- Working branch (verified): feature/v1.39-research-stage-wiring
- Review cwd (verified): .worktrees/v1.39-p05 (read-only — do NOT modify; do NOT push; do NOT open PR)
- Files reviewed: 4
- Commit range: ea129914 (single commit on the review branch)
- Tools run: git rev-parse / branch / log / diff --stat; cargo clippy --all -- -D warnings (lock contention observed, reached Finished dev profile with no new diagnostics in captured tail); cargo test -p nexus-orchestration --lib -- research (14 passed); cargo test -p nexus-orchestration --test auto_chain (21 passed); cargo test -p nexus-local-db (all packages ok); cargo test -p nexus-daemon-runtime (6 passed, 4 failed in fl_e_schedule_api.rs); cargo +nightly fmt --all -- --check (EXIT_CODE=0, clean); full context reads of plan, iteration compass §0.1/§1.1, creator-workflow.md, orchestration-engine.md, novel-workflow-profile.md, research preset.yaml, auto_chain.rs, supervisor.rs (P0 context), and the exact 1-commit diff.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1: Daemon-runtime FL-E schedule API integration tests now fail for the "research" preset (4 tests).**  
  Command `cargo test -p nexus-daemon-runtime` shows exactly 4 failures, all in `crates/nexus-daemon-runtime/tests/fl_e_schedule_api.rs`:
  - `schedule_create_seeds_core_context_from_preset_input`
  - `schedule_create_with_correct_dto_shape`
  - `schedule_list_isolation_by_creator`
  - `schedule_with_empty_creator_id_is_isolated_from_legitimate_creators`
  All assert "Expected status code to be 201 (Created), received 422 (Unprocessable Entity)" on `POST /v1/local/orchestration/schedules` with `preset_id: "research"`.
  Root cause (confirmed by diff + preset.yaml + preset_gates.rs): this P0.5 change added `gates` to the research preset (intake_status == complete + work_ref required) and changed run_intents from work_init to knowledge_ingest. The tests directly create schedules for the "research" preset without a Work record satisfying those gates. The 422 is the correct enforcement of the new security/correctness contract.
  Impact: High for the FL-E test surface (P0 area). These are not new product defects; they are test debt exposed by the intentional tightening. Per review baseline, CI/test command failures are treated as >= Warning until fixed or explicitly waived in consolidated decision.
  Evidence: exact panic messages and Diff blocks from the verification command output; cross-checked against the added gates in preset.yaml (lines 27-34) and the three new failing-path tests in preset_gates.rs.

- **W-2: Hard-coded preset_version=1 in auto-chain schedule INSERT while research preset bumped to version 2.**  
  In `crates/nexus-orchestration/src/auto_chain.rs:348` (the INSERT inside `enqueue_auto_chain_schedule`):
  ```sql
  ... preset_version, ... VALUES (?, ?, ?, 1, 'pending', ...
  ```
  The research preset.yaml in this diff changes `version: 1` → `version: 2`. The schedule row always stores literal 1 for any auto-chain stage schedule.
  This is a latent correctness/observability hygiene item (any consumer comparing schedule.preset_version against the preset's declared version will see a mismatch for research after this change, and for any future stage whose YAML version is bumped). Not a runtime bypass, data corruption, or security issue — the loader and runtime currently use the preset_id + the stored version at enqueue time for the schedule row; the YAML version is primarily for the preset artifact itself.
  Not introduced by this change (the hard-code predates P0.5), but became visible because P0.5 bumped the research YAML version. No evidence that the auto-chain path or gate evaluation relies on the version number for research vs. produce.
  Evidence: diff hunk for preset.yaml (version bump), grep for the INSERT in auto_chain.rs (line 348), and absence of any version derivation from the preset definition in the enqueue helper.

### 🟢 Suggestion
- **S-1: New tests are solid for wiring/gates but AC2 (post-research KB artifact queryable) remains an integration claim.**  
  The 6 new tests (3 in auto_chain.rs, 3 in preset_gates.rs) directly exercise:
  - AC1: research appears in auto-chain before first produce (full_chain_intake_research_produce_advances + evaluate_next_step paths; research_schedule_seed_includes_context_for_produce).
  - AC3: produce seed carries the same creative_brief / inspiration_log surface that research can enrich (produce_schedule_seed_carries_research_enrichable_fields).
  - AC4: no duplicate/cancel (the chain advance tests show sequential AdvanceStage, and the generic enqueue path is shared with no research-specific fork).
  - Gate correctness: research gates pass only with intake complete + work_ref; fail cleanly on missing prerequisites.
  These are not tautological — they assert the exact data shapes (fl_e_stage, seed fields) and gate outcomes required by the plan's ACs and the creator-workflow.md stage model.
  However, AC2 ("After research completes, at least one reference/KB artifact is queryable for the Work") is a behavioral side-effect claim. The research preset contract (in preset.yaml) says it writes to `{$workspace_dir}/.nexus42/references/{$run_id}/report.md` and artifacts/. The new tests stop at "the schedule was enqueued and gates passed." There is no test in this diff that actually runs a research schedule to completion and asserts a file appears under the references tree for that work_id. This is acceptable for P0.5 unit-level wiring scope, but a higher-fidelity test (or explicit reliance on the preset's own end-to-end contract) would give stronger evidence for AC2.
  Evidence: the 6 test names and bodies in the diff; research preset output contract comments (lines 3-8); absence of any "references/" or "report.md" assertion in the new tests.

- **S-2: Status hint string surface (T5) is safe in current CLI context; future-proofing recommended.**  
  The added block in `crates/nexus42/src/commands/creator/run.rs:660-689` reads `current_stage` and `stage_status` from the server JSON response and emits one of three plain `println!` lines:
  - "research: in progress (stage: research)"
  - "research: complete — references ready for produce"
  - "research: done (current stage: {current_stage})"
  It correctly suppresses the line for an unresearched Work (still at intake, none of the three conditions match). No user-controlled content is interpolated; values come from the authenticated Local API response for the caller's own Work. In a pure CLI println path there is no HTML/JS/shell injection surface.
  Suggestion: if this status blob ever flows into a structured/JSON response, a log that is later rendered in a web UI, or any context where the stage name could be treated as markup or executed, the stage value should be HTML-escaped / treated as untrusted. Current code has no such exposure.
  Evidence: exact added block in the diff (lines 660-689); no format! or user var in the printed strings; cross-check against the "unresearched Work" case (current_stage == "intake" produces no output).

## Source Trace
- W-1: Source Type: verification command output + git diff. Source Reference: `cargo test -p nexus-daemon-runtime` tail (exact panics at fl_e_schedule_api.rs:97, :222, :324); diff shows the gates addition in preset.yaml and no test updates in daemon-runtime. Confidence: High.
- W-2: Source Type: git diff + code read. Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:348` (hard-coded 1 in INSERT); `embedded-presets/research/preset.yaml:12` (version: 2). Confidence: High.
- S-1: Source Type: git diff + test bodies + preset contract. Source Reference: new tests in auto_chain.rs:698-730 and preset_gates.rs:854-942; preset.yaml lines 3-8 (output contract). Confidence: High.
- S-2: Source Type: git diff + static analysis of the added block. Source Reference: `crates/nexus42/src/commands/creator/run.rs:660-689` (the three if branches and printlns). Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

---

## Additional Context for PM Consolidated Decision

**Security / correctness positives (qc-specialist-2 focus):**
- Research gates (`intake_status == "complete"` + `work_ref` required) are evaluated at enqueue time via the generic `evaluate_gates` / `evaluate_work_field_op` path. `Equals` is exact match; `Required` is `is_some()`. Empty string, null, or missing fields fail closed. No enum aliasing or loose coercion observed.
- Creator isolation: auto-chain paths (`find_work_for_driver` → `evaluate_next_step` → `enqueue_auto_chain_schedule`) always derive the creator_id from the Work record matched by `driver_schedule_id`. A caller cannot target another creator's Work via the auto-chain driver. Direct schedule creation via Local API still goes through the same gate evaluator (the 422s in the failing tests prove the gates are live).
- Auto-chain helper reuse is generic and correct for research: same `ACH{timestamp}` ID minting, same pending INSERT + `set_driver` call, same `build_auto_chain_schedule(stage, ...)` dispatch. No research-specific fork or missing state check. The new `full_chain_intake_research_produce_advances` test explicitly covers the research-middle step.
- Status hint (T5) has no injection surface in its current form and correctly distinguishes "research never done" (intake) from "research done/past".
- All 6 new tests pass and map to the plan's AC1–AC4. The orchestration lib + auto_chain integration tests for research are green.

**Behavior regression noted (required by assignment):**
- The mandated `cargo test -p nexus-daemon-runtime` command is red (4 failures). These are the P0 FL-E schedule API tests that assumed a research preset with no gates. The failures are the direct, expected consequence of adding the gates for security/correctness. All other verification commands required by the assignment (orchestration research tests, local-db, fmt, clippy tail) are clean or non-blocking.

**Out-of-scope items confirmed untouched:**
- No changes to V1.38 residual areas, findings/rules/logs, 96h timeout, World KB, or multi-volume PK.
- Branch discipline: review performed strictly in the assigned Review cwd on the assigned Working branch; no source modifications; no push/PR.

**Recommendation to PM:**
- The implementation change is narrow, well-tested at the unit/wiring level, and delivers the security/correctness properties requested (gates + research in chain + status visibility).
- The 4 daemon test failures are a test-maintenance item, not a defect in the changed code. They can be addressed by updating the tests to seed a Work with intake complete + work_ref (or to assert the 422 gate failure explicitly) before or in the same wave.
- If the consolidated decision treats the test breakage as acceptable "stricter contract exposed test debt" with a follow-up task, Approve is appropriate. If the wave requires all P0 FL-E integration tests green before merge, mark as Request Changes with targeted re-review after the test updates land.
- No Critical or high-risk correctness bypasses found. The gate enforcement and auto-chain reuse are the strongest parts of this slice.
