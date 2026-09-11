# Task 1 report — deterministic race regression (storage boundary)

**Branch:** feat/v1.188-p4-workflow-cancel-settlement
**Range:** `95615fea2..828b9fd13` (commit `828b9fd13`)

## Scope note

This first commit also contains the `settle_run` settlement foundation
(`run_state.rs` types/trait, `sqlite.rs` CAS + `Observed` mapping, `engine.rs`
caller migration) that T2 consumes. T1 and the T2 orchestration foundation
share one commit because the trait change is atomic: `settle_cancelled` /
`settle_failed` were removed from `WorkflowStateStore`, so every implementor
and caller had to move in the same commit to keep the tree compiling.

## Files

- `crates/nexus-orchestration/src/run_state.rs` — added
  `TerminalSettlementTarget`, `SettlementResult::{Applied, Observed}`,
  `durable_cancel_outcome_accomplished(record)`; replaced trait methods
  `settle_cancelled` / `settle_failed` with `settle_run`.
- `crates/nexus-orchestration/src/storage/sqlite.rs` — terminal settlement CAS
  now maps a lost CAS whose durable winner is already `Completed`/`Failed`/
  `Cancelled` to `SettlementResult::Observed(winner)` instead of a bare
  `TerminalState` error; `Completed` reuses `run_commit_transition_cas`.
- `crates/nexus-orchestration/src/engine.rs` — cancel settle loop and
  child-rollback settle call `settle_run`; `ChildSettleCasLossStore` test
  double migrated to `settle_run`.
- `crates/nexus-orchestration/src/lib.rs` — exports for the new types.
- `crates/nexus-orchestration/tests/workflow_run_state.rs` — migrated the two
  `settle_cancelled` call sites to `settle_run(...,
  TerminalSettlementTarget::Cancelled)`. No new test in this file.

## Behavior

- Single settlement authority: all terminal writes go through `settle_run`;
  the old `settle_cancelled` / `settle_failed` methods are deleted.
- A settlement CAS loser that finds a durable terminal winner observes it
  (`Observed`) instead of surfacing a conflict; still-live revision/graph
  contention still returns `RevisionMismatch`.

## Race regression coverage (honest)

Deterministic race coverage for the cancel/drive-failure window rests on the
pre-existing barrier/injection stores in this tree — `RaceInjectingStore`
(two-phase concurrent cancel-owner settle) and `BoundaryFaultStore` in
`crates/nexus-daemon-runtime/src/preset_run.rs` tests, plus the engine-side
`ChildSettleCasLossStore` barrier test. The new assertion added by this plan
(`cancel_fence_accepts_failed_driver_failed_when_cancel_requested`) landed in
the T2 commit range because it tests the daemon-side fence helper.

## Deferred validation (parent, post-wave)

- `cargo test -p nexus-orchestration --test workflow_run_state`
- `cargo test -p nexus-daemon-runtime preset_run::`

## Risks

- `Observed` reclassifies some former 409 paths as success; callers must not
  treat `Observed` as proof their own cleanup ran (documented on the enum).
- No project-wide build/test run in this dispatch per assignment; compile
  evidence is `cargo check -p nexus-orchestration -p nexus-daemon-runtime`.
