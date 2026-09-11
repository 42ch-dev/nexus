# Task 1 report — Race regression

**Branch:** feat/v1.188-p4-workflow-cancel-settlement
**Base:** 95615fea2
**Head:** 828b9fd13 (task 1)

## Files
preset_run.rs boundary tests, workflow_run_state settle_run

## Risks
- Run-event registry is daemon-local; restart yields history_unavailable + inspect URL.
- Parent must run scoped validation after P1/P3/P4 wave (not run in this dispatch).

## Deferred validation (parent)
- `cargo test -p nexus-orchestration --test workflow_run_state`
- `cargo test -p nexus-daemon-runtime preset_run::` race filters
- `cargo test -p nexus42 --test workflow_execution_cli`
- `cargo test -p nexus42 --test workflow_restart_cli`
