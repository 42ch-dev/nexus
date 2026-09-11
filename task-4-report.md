# Task 4 report — proof bundle and QA evidence

**Branch:** feat/v1.188-p4-workflow-cancel-settlement
**Range:** `427e59b86..50565379b` (commit `50565379b`)

## Files

- `task-1-report.md` … `task-4-report.md` — per-task reports (this file set).
- `qa-r-v1186p3-004-evidence.md` — closure-recommendation evidence for
  residual R-V1186P3-004. Recommendation only: the residual register
  (`.mstar/projects/harness/residuals.json`) was not modified; QA/PM owns
  register mutation.

No source code changed in this range.

## Public/restart/race/replay proof status (honest)

- **Race (deterministic, in-tree):** covered by barrier/injection stores —
  `RaceInjectingStore` (concurrent cancel-owner settle, two-phase),
  `BoundaryFaultStore`, engine `ChildSettleCasLossStore`, and
  `signal_continue_loses_cas_to_cancel_returns_state_conflict`; plus the new
  `cancel_fence_accepts_failed_driver_failed_when_cancel_requested` unit
  test. Executed only to `cargo check` level in this dispatch; test execution
  is deferred to the parent (below).
- **Public API:** `signal_run` idempotent-cancel classification is unit-level
  code proof; end-to-end API runs are deferred.
- **Restart:** SSE history is intentionally daemon-local; restart behavior
  (`history_unavailable` + inspect URL) is implemented and unit-covered at
  the registry level, not exercised end-to-end here.
- **Replay:** `Last-Event-ID` replay, gap framing, and cursor rejection are
  implemented in `run_events.rs`; end-to-end SSE replay not exercised here.

## Deferred validation (parent, post-wave — exact commands)

```bash
cd .worktrees/v1.188-p4-workflow-cancel-settlement
cargo test -p nexus-orchestration --test workflow_run_state
cargo test -p nexus-daemon-runtime preset_run::
cargo test -p nexus42 --test workflow_execution_cli
cargo test -p nexus42 --test workflow_restart_cli
```

## Risks

- Acceptance's "public/restart/race/replay proof" is met at code + unit-test
  level only; the four commands above must pass on the integrated head before
  QA closes R-V1186P3-004.
- Local verification in this dispatch: `cargo check -p nexus-orchestration
  -p nexus-daemon-runtime` (clean). No test suite was run, per the concurrent
  -wave rule.
