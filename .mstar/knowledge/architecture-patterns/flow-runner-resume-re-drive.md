---
module: nexus-orchestration, nexus-core, graph-flow
date: 2026-09-02
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-09-02-v1.180-p2-core-reliability-sweep
tags:
  - orchestration
  - checkpoint
  - resume
  - graph-flow
  - bl-04
  - dr-06
applies_when:
  - "Adding restart-resume for long-running preset tasks (converge/merge chains)"
  - "Classifying persisted sessions for re-drive (join-marker based class checks)"
  - "Wiring boot-time background work to shutdown (CancellationToken + notify)"
  - "Writing ops E2E tests for daemon-driven orchestration flows"
---

# Resume re-drive over graph-flow task-boundary checkpoints (BL-04 slice)

## Context

Long-running preset tasks (converge/merge chains) already persist a
checkpoint at every task boundary: `graph-flow` 0.2.3 `FlowRunner::run`
executes **one** task then `save()`s (`load → execute-one → save`,
`runner.rs:380-390`), and `SqliteSessionStorage` upserts
`orchestration_sessions` per step (`current_task_id` + `context_json`
including `_converge_arrivals_*` / `_join_wait_start_*` keys). Boot
recovery also already exists (`boot.rs` → `list_non_terminal_sessions` →
`recover_sessions` → `reconstruct_runner`).

The actual v1.180 gap was **no production driver**: HTTP routes only
create/list/get/signal (signal flips status without stepping); the
supervisor admits but never executes; `on_schedule_terminal` had no
production caller. The BL-04 minimal slice therefore = a minimal
daemon-local **preset-run driver** (`drive_preset_run`, bounded step loop
over `OrchestrationEngine::run_step`, honoring Cancel / WaitForInput
signal semantics) + a **boot resume re-drive**
(`resume_driven_sessions`) spawned in background after
`recover_sessions`, with a **no-re-execution guarantee** for completed
edges.

## Guidance

1. **Resume = filter + re-drive, not a new step loop.** `resume_driven_sessions`
   reads recovered session summaries, classifies each
   (`ReDriven` / `SkippedTypedFailed` / `SkippedNotConvergeMergeClass` /
   `SkippedNoRunner` / `SkippedUnreadable`), and re-drives live sessions
   through the same `drive_preset_run` with `resume_waiting: true`. The
   no-re-execution property comes mechanically from `FlowRunner::run`
   resuming at `current_task_id` — parked joins never reach
   `enter_actions`, so instrumented host-tool edges stay at their
   pre-restart count.

2. **Class checks must test values, not key presence.** `graph_flow::Context::set`
   (0.2.3) **never removes keys**; join gates "clear" state by writing
   `Value::Null` (`tasks/mod.rs` deadline 1215-1216, success-leave
   1689-1692). A session with a join-tracking key whose value is `Null`
   is **finished**, not live. Classifying by key presence re-drives
   completed sessions and auto-advances post-join `llm_judge`/`manual`
   waits on every boot. Require a **non-Null** join-key value for
   class-positive (`is_converge_merge_chain`, shared
   `nexus-orchestration::resume_rules` since the v1.182 P1 QC wave; consumed
   by both `preset_run.rs` and the `ops inspect` CLI).

3. **Typed-failure must filter before re-drive.** `SqliteSessionStorage::save`
   inserts `status='running'` and `ON CONFLICT` never updates it — a
   typed-failed session stays "running" in the DB forever. The failure
   discriminator lives in `context_json` (`_run_status` / `_run_error`,
   written by the driver's `persist_failure`); boot resume **must**
   short-circuit on those keys before stepping (`SkippedTypedFailed`), or
   every restart re-ticks dead joins.

4. **Boot background work must be cancellable and shutdown-tied.** Spawn a
   `CancellationToken`, a watcher that awaits `shutdown_notify().notified()`
   (broadcast by `request_shutdown`), and thread `Some(&token)` into the
   driver's cancel slot (checked before every step, incl. the first).
   Comments must match code — "Bounded + cancellable" with `cancel: None`
   is a review blocker (found and fixed in QC wave 1).

5. **Downtime counts against join deadlines.** `_join_wait_start_*` is
   wall-clock persisted; resume must NOT re-baseline it — elapsed
   includes the downtime, pinned by an E2E assertion
   (`elapsed >= downtime` against a smaller deadline).

## Why This Matters

The checkpoint infrastructure was already shipped; the missing piece was
an honest driver + resume filter. The failure mode of a naive resume
(class-by-presence) is silent double-execution at every daemon restart —
worse than no resume, because it is invisible until side effects repeat.

## When to Apply

- Extending resume coverage to more session classes (scope the class
  filter first; today only converge/merge chains with live join keys).
- Adding new boot-time background tasks (use the same
  CancellationToken + shutdown-notify watcher pattern).
- Adding coverage for resume/restart behavior at the in-crate level
  (`crates/nexus-core/src/execution/workflow.rs` resume tests) or the ops
  E2E level (`apps/nexus42/tests/`, when present).

## Examples

- Driver + resume: `crates/nexus-core/src/execution/workflow.rs`
  (`drive_preset_run`:203, `ResumeDecision`:670, `resume_driven_sessions`:782).
- Resume classifiers: `crates/nexus-orchestration/src/resume_rules.rs`
  (`classify_recovery`, `is_converge_merge_chain`:235) — shared by the core drive and the `ops inspect` CLI.
- Boot wiring: `crates/nexus-core/src/execution/workflow.rs`
  (`recover_persisted`:1304 spawns the `CancellationToken` + shutdown-notify watcher, then drives via `recover_driving`:2075).
- Resume tests (in-crate, not ops E2E): `crates/nexus-core/src/execution/workflow.rs`
  (`resume_re_drives_converge_merge_chain_session`:4715, `resume_skips_typed_failed_session`:4578,
  `resume_skips_non_converge_merge_class_session`:4623, `resume_skips_session_without_runner`:4653,
  `resume_skips_unreadable_session`:4688).
- Promotion source: v1.180 `long-run-checkpoint.md` (`{ITERATION_DIR}/v1.180/specs/long-run-checkpoint.md`).