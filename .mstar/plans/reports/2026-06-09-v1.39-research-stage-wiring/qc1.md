---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-09-v1.39-research-stage-wiring"
verdict: "Request Changes"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-09T12:00:00Z

## Scope
- plan_id: 2026-06-09-v1.39-research-stage-wiring
- Review range / Diff basis: merge-base: 1b68d6ca + tip: ea129914 (equivalent to `git diff 1b68d6ca...ea129914`)
- Working branch (verified): feature/v1.39-research-stage-wiring
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p05
- Files reviewed: 4
- Commit range: 1b68d6ca..ea129914 (1 commit: `ea129914 feat(orchestration,v1.39-p05): wire research stage into auto-chain (P0.5 T1+T2+T5+T6)`)
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration --lib -- research, cargo test -p nexus-orchestration --test auto_chain, cargo test -p nexus-local-db, cargo test -p nexus-daemon-runtime, cargo +nightly fmt --all -- --check

## Findings
### 🔴 Critical
- **C-1: Regression — 4 daemon-runtime integration tests fail on P0.5 branch** → The new `gates` on the `research` preset (`intake_status == complete` + `work_ref required`) cause `schedule_create_seeds_core_context_from_preset_input`, `schedule_create_with_correct_dto_shape`, `schedule_list_isolation_by_creator`, and `schedule_with_empty_creator_id_is_isolated_from_legitimate_creators` to fail with 422 (Unprocessable Entity). These tests create research schedules via the daemon API without setting up Work records, so the gate evaluator cannot find `intake_status` or `work_ref`. On the base commit (`1b68d6ca`), all 10 tests pass. The research preset's gates must be conditional — they should only be enforced when a `work_id` is provided and the schedule is created through the auto-chain path. Schedules created via the generic API without Work context should not be rejected.

  **Evidence**:
  ```
  # Base commit (1b68d6ca): 10 passed, 0 failed
  # P0.5 commit (ea129914): 6 passed, 4 failed
  cargo test -p nexus-daemon-runtime --test fl_e_schedule_api
  ```

  **Fix**: Either (a) make gate evaluation conditional on the presence of a `work_id` in the schedule request (skip gates when no work context is provided), or (b) update the 4 failing tests to insert Work records with `intake_status=complete` and `work_ref` set before creating research schedules. Option (a) is architecturally cleaner — gates should not reject schedules that don't have a Work to gate against.

### 🟡 Warning
- **W-1: `run_intents` change from `work_init` to `knowledge_ingest` — consumer impact not assessed** → The research preset previously declared `run_intents: [work_init, work_continue]`. P0.5 changes this to `[knowledge_ingest, work_continue]`. While this aligns with the spec (creator-workflow.md §3.1 maps research to `knowledge_ingest`), no impact analysis was performed on consumers that might depend on research appearing in `work_init` filtering. The `creator run start` command filters presets by `work_init ∈ run_intents` — removing `work_init` from research means `creator run start` can no longer directly start a research preset. This is likely intentional (research should follow intake, not replace it), but the plan should explicitly document this behavioral change and confirm no existing workflows rely on it.

- **W-2: `version: 2` bump on research preset — no versioning policy documented** → The preset version was bumped from 1 to 2. The orchestration-engine spec §7.2 says `version` is "bumped on breaking changes to this preset." The `run_intents` change and new `gates` are indeed breaking changes. However, there is no documented versioning policy explaining what v2 signals to consumers, whether v1 is still loadable, or how the loader handles version mismatches. This is a maintainability gap — future preset authors need clear guidance on when and how to bump versions.

- **W-3: T5 status hint format inconsistent with existing status output** → The new status hints use a `key: value` inline format (`research: in progress (stage: research)`, `research: complete — references ready for produce`, `research: done (current stage: {current_stage})`). The existing status output uses structured sections (e.g., "Work Status", "Chapters:"). The inline format creates visual inconsistency and may confuse users expecting structured output. Consider integrating research status into the existing structured sections rather than adding a standalone inline line.

### 🟢 Suggestion
- **S-1: Test coverage — `enqueue_auto_chain_schedule` for research stage not tested at integration level** → The 3 new unit tests in `auto_chain.rs` (T6) test `build_auto_chain_schedule` and `evaluate_next_step` for the research stage, but there is no integration test verifying that `enqueue_auto_chain_schedule` with `stage="research"` actually produces a valid schedule that passes gate evaluation and gets enqueued. The `enqueue_helper_success_path` test uses `stage="research"` but the test Work already has `intake_status="complete"` and `work_ref` set, so it doesn't exercise the new gate path. Consider adding a test that verifies the full auto-chain flow from intake completion through research enqueue.

- **S-2: Research preset `run_intents` — consider keeping `work_init` for standalone use** → The spec (creator-workflow.md §3.1) maps research to `knowledge_ingest` and `work_continue`, but removing `work_init` entirely prevents users from running research as a standalone preset via `creator run start`. If research is ever useful as a standalone entry point (e.g., for non-novel Works that only need reference gathering), keeping `work_init` alongside `knowledge_ingest` would preserve flexibility without conflicting with the auto-chain path. This is a product decision, not a code defect.

- **S-3: Status hint strings — consider i18n/extraction** → The three new status strings are hardcoded English in `run.rs`. If the CLI ever supports localization, these should be extracted. Low priority for pre-1.0.

## Source Trace
- Finding ID: C-1
- Source Type: test-failure
- Source Reference: `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api` on ea129914 vs 1b68d6ca
- Confidence: High

- Finding ID: W-1
- Source Type: git-diff + spec-analysis
- Source Reference: `git diff 1b68d6ca...ea129914 -- crates/nexus-orchestration/embedded-presets/research/preset.yaml` + creator-workflow.md §3.1
- Confidence: Medium

- Finding ID: W-2
- Source Type: git-diff + spec-analysis
- Source Reference: `git diff 1b68d6ca...ea129914 -- crates/nexus-orchestration/embedded-presets/research/preset.yaml` + orchestration-engine.md §7.2
- Confidence: Medium

- Finding ID: W-3
- Source Type: manual-reasoning
- Source Reference: `crates/nexus42/src/commands/creator/run.rs` lines 660-690
- Confidence: Medium

- Finding ID: S-1
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/auto_chain.rs` tests module
- Confidence: Low

- Finding ID: S-2
- Source Type: spec-analysis
- Source Reference: creator-workflow.md §3.1 + orchestration-engine.md §7.8
- Confidence: Low

- Finding ID: S-3
- Source Type: manual-reasoning
- Source Reference: `crates/nexus42/src/commands/creator/run.rs` lines 670-688
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

### Top 3 Architecture Observations

1. **Gate regression is the blocking issue.** The new research preset gates are correct in intent (research should require intake complete + work_ref), but they are enforced unconditionally at the daemon API level. Schedules created without Work context (valid for testing, CLI exploration, and non-Work use cases) are incorrectly rejected. The gate evaluator should be conditional on the presence of a `work_id` — skip gates when no Work is associated with the schedule request.

2. **The `run_intents` change is spec-aligned but under-documented.** The shift from `work_init` to `knowledge_ingest` correctly reflects the spec (research is not an entry point; it follows intake). However, the plan should explicitly note this as a behavioral change and confirm no existing workflows depend on `creator run start` with the research preset.

3. **The auto-chain wiring is architecturally clean.** The `evaluate_next_step` function already handled the research stage correctly (it advances from intake→research→produce via the generic stage-index logic). The P0.5 changes add the right tests and the `build_auto_chain_schedule` for research correctly includes `creative_brief` and `inspiration_log` in the seed, confirming the shared context surface with produce. The integration with P0's `enqueue_auto_chain_schedule` helper works cleanly — the schedule ID format (`ACH{timestamp}`) and Work record state management are consistent.

### Clarification Needed
None — the findings are self-contained and actionable.
