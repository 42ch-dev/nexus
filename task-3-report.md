# Task 3 report — bounded run-event registry and session SSE

**Branch:** feat/v1.188-p4-workflow-cancel-settlement
**Range:** `027167df5..427e59b86` (commit `427e59b86`)

## Files

- `crates/nexus-daemon-runtime/src/run_events.rs` (new) —
  `RunEventRegistry` with per-run rings bounded at 256 records / 1 MiB and a
  512 KiB frame cap, 64 live + 64 terminal rings; `RunEventSink` (weak
  back-reference) for publishers; `subscribe` honoring `Last-Event-ID`
  (`epoch:sequence` cursor) with replay, `gap` frames for evicted ranges,
  `history_unavailable` (with inspect URL) for unknown/evicted runs, and
  rejection of malformed/future cursors.
- `crates/nexus-daemon-runtime/src/lib.rs` — `pub mod run_events;`.
- `crates/nexus-daemon-runtime/src/prompt_executor.rs` —
  `HostPromptExecutor::new_with_run_events`; publishes every `HostEvent` for
  the run (message deltas and non-message events) into the registry; enforces
  a 4 MiB bound on accumulated prompt text.
- `crates/nexus-daemon-runtime/src/workspace/mod.rs` — `WorkspaceState` holds
  the shared `Arc<RunEventRegistry>`; lazy-attach path passes it to the
  coordinator and prompt executor.
- `crates/nexus-daemon-runtime/src/boot.rs` — boot path wires the registry
  into `HostPromptExecutor` and `WorkflowRunCoordinator`.
- `crates/nexus-daemon-runtime/src/api/mod.rs` — route registration:
  `GET /v1/daemon/orchestration/sessions/{session_id}/events`.

The coordinator-side registration hooks (`with_run_events`,
`run_event_sinks`, register-before-spawn, terminal `publish_durable_run_state`)
and the `session_events` handler body landed in the T2 range (see task-2
report); this commit supplies the registry implementation and all remaining
wiring.

## Behavior

- Rings are registered before drive spawn, so no early events are lost when
  admission succeeds; a saturated live quota (64) skips eventing for that run
  rather than blocking the drive.
- Durable terminal settlement is mirrored into the ring as a `run_state`
  record, then the ring moves to terminal retention.
- Daemon-local transport only: no durable SQLite event log; after restart a
  subscriber gets `history_unavailable` with the session inspect URL.

## Deferred validation (parent, post-wave)

- `cargo test -p nexus-daemon-runtime` (run_events unit tests:
  ring eviction emits `gap` on subscribe-without-cursor)
- Manual/e2e: `GET .../sessions/{id}/events` with and without
  `Last-Event-ID` against a live and a terminal run.

## Risks

- `subscribe` replays a snapshot and the current handler streams that snapshot
  then closes (keep-alive only); there is no long-lived live tail in this
  slice — subscribers poll with `Last-Event-ID`.
- Registry is per-daemon-process memory; bounds are hard caps, eviction is
  silent except for `gap` frames on subscribe.
