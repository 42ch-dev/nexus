# Task 2 report — single durable settlement + idempotent cancel projections

**Branch:** feat/v1.188-p4-workflow-cancel-settlement
**Range:** `828b9fd13..027167df5` (commit `027167df5`)

## Files

- `crates/nexus-daemon-runtime/src/preset_run.rs`
  - `cancel_fence_loss_accomplished` now takes the durable `RunRecord` and
    delegates to `run_state::durable_cancel_outcome_accomplished`.
  - `signal_run`: on `RunSignal::Cancel`, a `TerminalState` error is
    reclassified to success when the reloaded durable record shows the cancel
    outcome already accomplished (`Cancelled`, `Interrupted` with
    `cancel_requested`, or `Failed` with `cancel_requested` +
    `driver_failed`); the same record-based check backs the existing
    `RevisionMismatch` path.
  - Test doubles (`RaceInjectingStore`, `BoundaryFaultStore`, delegating
    wrappers) migrated from `settle_cancelled`/`settle_failed` to
    `settle_run`.
  - New unit test
    `cancel_fence_accepts_failed_driver_failed_when_cancel_requested`
    (the deterministic race assertion for R-V1186P3-004).
  - Note: this commit also carries the coordinator-side run-events wiring
    (`with_run_events`, `run_event_sinks`, `try_register_live` before drive
    spawn, `publish_durable_run_state` after terminal settlement) because
    `preset_run.rs` could not be split across commits; the module it wires
    (`run_events.rs`) and its consumers land in the T3 range.
- `crates/nexus-daemon-runtime/src/api/handlers/orchestration/sessions.rs`
  - Test-store delegator migrated to `settle_run`.
  - Note: the `session_events` SSE handler also landed in this commit because
    the file was committed wholesale; its route registration lands in T3.

## Behavior

- Operator cancel that loses the settlement race to a driver failure no longer
  surfaces a spurious 409 when the durable row already records the cancel
  intent (`Failed` + `cancel_requested` + `driver_failed`); it returns success
  with the durable `Failed` status.
- No handler-level suppression: the decision reads only the durable record.

## Deferred validation (parent, post-wave)

- `cargo test -p nexus-daemon-runtime preset_run::`
  (incl. `cancel_fence_accepts_failed_driver_failed_when_cancel_requested`,
  `RaceInjectingStore`/`BoundaryFaultStore` race tests)
- `cargo test -p nexus-orchestration --test workflow_run_state`

## Risks

- The fence-loss reload races with a concurrent writer between CAS failure
  and `load_run`; classification uses the record as of read time, which is
  the contract (`Observed` ≠ cleanup proof).
- `signal_run`'s `TerminalState` path now performs an extra `load_run` on
  cancel-conflict only; no hot-path cost for successful signals.
