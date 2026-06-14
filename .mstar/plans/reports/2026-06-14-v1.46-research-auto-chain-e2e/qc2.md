---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-14-v1.46-research-auto-chain-e2e"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-15T03:00:00Z

## Scope
- plan_id: 2026-06-14-v1.46-research-auto-chain-e2e
- Review range / Diff basis: merge-base: 1d776d23 (P2 Done commit, base of P3 work) → tip: 87f00619 (P3 merge) (1 commit + 1 --no-ff merge = 2 total) — equivalent `git diff 1d776d23..87f00619` or `git show --stat 1f92016f..87f00619`
- Working branch (verified): iteration/v1.46
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (project root, checked out at iteration/v1.46 HEAD cae189c5; P3 merge 87f00619 is an ancestor)
- Files reviewed: 1
- Commit range: 1d776d23..87f00619 (1 additive commit + merge commit)
- Tools run: git diff --stat, git show --stat, cargo test -p nexus-orchestration --test research_supervisor_e2e, cargo test --all, cargo +nightly clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings, cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings (on origin/main 63b36a32 for pre-existing claim verification)

## Pre-existing Clippy Verification (PM-override)
Per Assignment, independently verified the pre-existing `cargo clippy --all -- -D warnings` failures in `nexus-orchestration` (tasks/mod.rs + worker/registry.rs + 2 test files) against **origin/main HEAD 63b36a32** (V1.45 main):

- Created worktree at origin/main (63b36a32).
- Ran `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings`.
- Observed ~60 errors (doc_markdown, if_let_collapsible, manual_assert_eq, etc.) in untouched files — **identical** to the claim in `residual_findings["pre-existing-v1.45-orchestration-clippy"]` (R-V145-PRE-CLIPPY-001, decision: risk-accepted).
- **DO NOT raise these as V1.46 P3 findings**. PM-override applies.
- Worktree removed and pruned after verification.

P3 diff itself introduces **zero new clippy issues** (scoped test file is clean under `-D warnings`).

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
None.

## Source Trace
- Finding ID: N/A (clean review)
- Source Type: manual-reasoning + CI gate outputs + diff inspection + pre-existing claim verification
- Source Reference: `git diff 1d776d23..87f00619 --stat`, `cargo test -p nexus-orchestration --test research_supervisor_e2e` (5 passed), `cargo +nightly clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` (clean), origin/main clippy run (pre-existing confirmed)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

## Detailed Review Notes (Security / Correctness Perspective)

**1. Hermetic boundary enforcement (correctness + security):**
- Test uses `tempfile::Builder` + `std::mem::forget` pattern (consistent with `tests/auto_chain.rs`) to create an isolated SQLite DB that survives the test lifetime.
- `nexus_local_db::open_pool` + `run_migrations` — no network, no ACP, no live LLM.
- All external capability calls (`creator.inject_prompt`, `acp.prompt`, `judge.llm`) are resolved only by name in `CapabilityRegistry::with_builtins()`; the loader never invokes them. The `llm_judge` boundary is explicitly stubbed by calling `on_schedule_terminal(Completed)` directly — documented in-test as the exact hook the live daemon invokes once the synthesizing `llm_judge` returns GO.
- No user-controlled input surfaces; no injection, path traversal, or privilege escalation vectors exist in this test harness.

**2. State transition correctness (R-V139P5-S1):**
- `research_supervisor_tick_admits_pending_schedule`: pending → running via `tick()` (boot admission gate). Asserts both DB row and `sup.status_of()`.
- `research_supervisor_tick_drives_boot_to_terminal_done` (headline E2E):
  - Seeds Work with `current_stage="research"`, `stage_status="active"`, `driver_schedule_id` wired.
  - Boot: `tick()` → "running".
  - Terminal stub: `on_schedule_terminal(..., Completed)` → "completed" (matches preset terminal state "done").
  - `terminated_at` is stamped (proves full UPDATE path, not a partial flag flip).
  - Supervisor `status_of()` agrees with DB.
- `research_schedule_boot_resume_running_to_paused_to_resumed`:
  - Pre-crash "running" → `resume_running_as_paused("daemon_restart_*")` → "paused" (exactly 1 row affected; idempotent second pass = 0).
  - `resume_schedule()` (smart resume) → "running" when gates still pass.
- All transitions match the documented supervisor contract for a research schedule.

**3. Preset input contract assertions (T1):**
- `research_preset_loads_and_structurally_valid`:
  - id/version/kind, initial="scanning", terminal="done".
  - 4-state machine: scanning → extracting → synthesizing → done.
  - Exit contracts: `llm_judge` on scanning + synthesizing; `graph_complete` on extracting.
  - Required capabilities: `creator.inject_prompt`, `acp.prompt`, `judge.llm`.
  - Run intents: KnowledgeIngest + WorkContinue.
  - Gates: intake_status == complete, work_ref required (both asserted via string match on Debug output).
- `research_schedule_request_preset_input_contract`:
  - `preset_id = "research"`, label = "FL-E stage: research (work: ...)" (matches auto_chain enqueue path).
  - Input seed contains: work_id, fl_e_stage="research", work_ref, creative_brief, inspiration_log.
  - Explicitly asserts `references_dir` / `output_dir` are **absent** from the schedule-row seed (they are runtime-resolved from the Work's workspace at session-run time). This correctly documents the hermetic boundary.

**4. Cross-crate interface stability:**
- Relies on stable public surfaces: `ScheduleSupervisor`, `build_schedule_for_stage`, `load_embedded_preset`, `ScheduleStatus`, `WorkFields`, `nexus_local_db::works`, `nexus_contracts::local::*`.
- No private module assumptions; all imports are from the crate's documented API surface.
- Test is additive-only (no changes to production code paths), so no regression surface on supervisor or preset loader.

**5. CI gates (current HEAD cae189c5, iteration/v1.46):**
- `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed, 0 failed.
- `cargo test --all` → all green (full workspace).
- `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` → clean (no errors/warnings from the P3 file).
- Nightly fmt check (per plan) reported clean by implementer; no P3-introduced formatting drift.

**6. Scope discipline:**
- Review strictly limited to `1d776d23..87f00619`.
- Only 1 new file: `crates/nexus-orchestration/tests/research_supervisor_e2e.rs` (530 lines).
- No production code changes, no schema changes, no CLI surface, no new dependencies.
- R-V139P5-S5 (artifact E2E requiring ACP mock) is explicitly out of scope per plan and compass §0.1 #10 — correctly not attempted here.

**7. Security / correctness risk assessment:**
- Zero new attack surface: pure hermetic test DB, no external I/O, no untrusted input.
- Correctness of the supervisor lifecycle for the research preset is now proven end-to-end within the hermetic boundary (boot admission, terminal transition, runtime lock release via `on_schedule_terminal`, boot resume recovery).
- The stub boundary is precisely documented and matches the live engine's contract — future R-V139P5-S5 work can replace the stub without invalidating these assertions.
- Pre-existing clippy noise on untouched files is correctly excluded per PM-override and independent verification.

**Conclusion**: The P3 change is a clean, minimal, well-documented hermetic test addition that directly closes R-V139P5-S1 as specified. No security, correctness, or regression risk introduced. All CI gates pass on the P3 artifact. Pre-existing issues in `nexus-orchestration` are unchanged and covered by the existing residual.

**Verdict**: Approve
