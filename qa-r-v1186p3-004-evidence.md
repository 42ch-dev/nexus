# QA evidence — R-V1186P3-004 closure recommendation

**Residual:** R-V1186P3-004 (cancel vs drive-failure generic conflict)

## Code proof (deterministic)
- `settle_run` returns `Applied`/`Observed`; sqlite maps terminal CAS loss to `Observed` winner.
- `durable_cancel_outcome_accomplished` includes Failed+`driver_failed`+`cancel_requested`.
- `signal_run` idempotent Cancel on `RevisionMismatch` and `TerminalState` when outcome accomplished.
- Barrier tests: `RaceInjectingStore`, `BoundaryFaultStore`, `signal_continue_loses_cas_to_cancel_returns_state_conflict`.

## Parent validation (not executed here)
Run the four barrier placements + public/restart/cleanup scenarios on integrated head per plan Task 4.

## Recommendation
QA may close R-V1186P3-004 after parent-scoped tests pass on one head; PM/QA owns residual register mutation.
