---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-09-v1.39-research-stage-wiring"
verdict: "Request Changes"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-09

## Scope
- plan_id: `2026-06-09-v1.39-research-stage-wiring`
- Review range / Diff basis: `merge-base: 1b68d6ca` + `tip: ea129914`; equivalent to `git diff 1b68d6ca...ea129914`
- Working branch (verified): `feature/v1.39-research-stage-wiring`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p05`
- Files reviewed: 4
- Commit range: `1b68d6ca...ea129914` (1 commit: `ea129914`)
- Tools run: `cargo clippy --all -- -D warnings`, `cargo test -p nexus-orchestration --lib -- research`, `cargo test -p nexus-orchestration --test auto_chain`, `cargo test -p nexus-local-db`, `cargo test -p nexus-daemon-runtime`, `cargo +nightly fmt --all -- --check`

## Findings

### 🔴 Critical
- None

### 🟡 Warning

#### W-1: Research preset `exit_when: manual` stalls unattended auto-chain
The `synthesizing` state in the research preset (`embedded-presets/research/preset.yaml:86`) declares `exit_when: kind: manual`. The orchestration engine's `StateCompositeTask` evaluates this to `NextAction::WaitForInput` (`tasks/mod.rs:733-737`), which pauses the session. The auto-chain supervisor (`supervisor.rs:341`) only triggers continuation when a schedule reaches `ScheduleStatus::Completed`. A paused session does not call `on_schedule_terminal` with a terminal status, so the daemon stalls at research and requires a manual `resume` or `advance` signal to reach `done`.

This breaks AC1: *"Auto-chain from intake includes research before first produce without manual advance."* The diff does not add any auto-resume logic for manual gates in auto-chain mode.

**Fix**: Change the research preset's `synthesizing` `exit_when` to `graph_complete` (if the enter actions alone are sufficient) or `llm_judge` (if a quality gate is desired), so the state reaches terminal completion without user intervention. Alternatively, add daemon-side auto-resume signaling for `manual` gates when `auto_chain_enabled` is true.

#### W-2: Research output not wired into produce stage preset input
The research preset's output contract writes artifacts to `{$workspace_dir}/.nexus42/references/{$run_id}/report.md` (`preset.yaml:4-5`). However, `build_preset_input` (`stage_gates.rs:86-92`) — the shared context surface consumed by all downstream stages — does not include a research report path or any research-derived enrichment. The produce stage (`novel-writing`) receives `creative_brief` and `inspiration_log` from the `WorkRecord`, but the research preset does not update these Work fields (its states use `creator.inject_prompt` and `acp.prompt`, not a Work-update capability).

This risks AC3: *"Subsequent `novel-writing` prompt context includes research-derived material (via existing assembly paths)."* Without explicit propagation, produce may draft chapters without research context.

**Fix**: Either (a) add a `research_report_path` field to `WorkFields` / `build_preset_input` and populate it from the research session output, or (b) have the research preset update the Work's `creative_brief` / `inspiration_log` with synthesized findings so produce sees them via the existing input surface.

#### W-3: Missing research-specific observability
The auto-chain supervisor logs generic per-Work outcomes (`supervisor.rs:367-373`: *"auto-chain: enqueued next step"*), but there is no stage-differentiated tracing. When operating unattended, operators cannot determine from logs whether a Work is stalled at research, producing, reviewing, or persisting. The research path adds a new stage where stalls are especially likely (see W-1), yet logging remains unchanged from P0.

**Fix**: Add `tracing::info!` lines in `process_auto_chain_after_terminal` that include the `next_stage` value (e.g., *"auto-chain: advancing work {work_id} to research"*), and similarly log research completion before enqueueing produce.

### 🟢 Suggestion

#### S-1: Consider propagating research report path to produce
As noted in W-2, the produce preset has no handle on where research wrote its output. Even if the current template convention is to scan `References/` independently, an explicit path in the preset input is more reliable and avoids filesystem races. This is a low-cost addition to `WorkFields` + `build_preset_input`.

#### S-2: Status hint computation is O(1) and acceptable
The research status hint added in `crates/nexus42/src/commands/creator/run.rs` (T5) performs simple string comparisons on `current_stage` and `stage_status` from the response JSON on every `status` call. The cost is negligible (O(1), no I/O). Caching is unnecessary unless `creator run status` becomes a high-frequency polling endpoint.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `preset.yaml:86`, `tasks/mod.rs:733-737`, `supervisor.rs:341` | High |
| W-2 | manual-reasoning | `preset.yaml:4-5`, `stage_gates.rs:86-92`, research preset state definitions | High |
| W-3 | manual-reasoning | `supervisor.rs:367-373`, `auto_chain.rs:367-373` | High |
| S-1 | manual-reasoning | Cross-reference of `preset.yaml` output contract with `stage_gates.rs` input surface | Medium |
| S-2 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` T5 diff | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

The implementation correctly wires the research stage into the auto-chain decision map (`evaluate_next_step`), adds the required gates to the research preset, and passes all static checks and tests. However, the research preset's `manual` exit gate will stall the daemon's unattended auto-chain (W-1), and the research-to-produce artifact pipeline is not explicitly connected (W-2). Both issues affect acceptance criteria and should be resolved before approval.

## Verification Evidence

```bash
# clippy (all crates)
$ cargo clippy --all -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.18s

# research-filtered lib tests
$ cargo test -p nexus-orchestration --lib -- research
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 491 filtered out; finished in 0.00s

# auto_chain integration tests
$ cargo test -p nexus-orchestration --test auto_chain
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s

# nexus-local-db tests
$ cargo test -p nexus-local-db | grep "test result"
test result: ok. 150 passed; 0 failed; ... finished in 3.95s

# nexus-daemon-runtime tests
$ cargo test -p nexus-daemon-runtime | grep "test result"
test result: ok. 180 passed; 0 failed; ... finished in 2.08s

# formatting check
$ cargo +nightly fmt --all -- --check
(no output = pass)
```

## Cross-Plan Notes (P0 + P0.5)

- **No double-enqueue path**: `evaluate_next_step` requires `stage_status == "complete"` to advance. After enqueuing research, `set_driver` flips `stage_status` to `"active"`, which prevents `evaluate_next_step` from re-enqueueing research until the schedule completes. This holds for all FL-E stages.
- **Gate evaluation cost in auto-chain**: The auto-chain path (`enqueue_auto_chain_schedule` → `build_auto_chain_schedule` → `build_schedule_for_stage`) does **not** call `evaluate_gates`. The 2 research gates are therefore evaluated **only** on manual `stage advance` or direct schedule creation paths, not on every auto-chain step. Per-enqueue cost in the auto-chain is zero; per-manual-enqueue cost is O(1) (in-memory struct field access + JSON comparison).
- **Test hermeticity**: All 6 new tests are hermetic (no daemon, no shared mutable state). The 3 `auto_chain.rs` tests construct `WorkRecord` directly; the 3 `preset_gates.rs` tests use `MockPreviousLookup` + `tempfile::tempdir()`. Total execution time is well under 1s.
