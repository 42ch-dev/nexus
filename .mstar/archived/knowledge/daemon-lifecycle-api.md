# Daemon Lifecycle API — Design Specification v2

**Status**: Active — authoritative design input for the V1.4 Orchestration track (closes TD-9).
**Author**: @project-manager (brainstorm consolidation, 2026-04-17) / to be co-signed by @architect before Phase 4 implement
**Supersedes**: [daemon-lifecycle-api-legacy.md](archived/knowledge/daemon-lifecycle-api-legacy.md) — archived 2026-04-17 (v1 captured the gap and the "running-probe" minimal slice; it does not reflect the final full-FSM design).
**Coordinates with**:

- [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) — Phase 4 lands the HSM; engine is started/stopped by lifecycle state transitions (§3.2 / §10.5)
- [acp-client-tech-spec.md](acp-client-tech-spec.md) — Worker Manager graceful shutdown is keyed off `Stopping.entry`
- [architecture-alignment-review.md](architecture-alignment-review.md) — TD-9 resolution row updates to "Resolved (v2)" after Phase 4 merges

---

## Table of Contents

1. [Scope and Source of Truth](#1-scope-and-source-of-truth)
2. [State Machine Model](#2-state-machine-model)
3. [Events](#3-events)
4. [Transition Matrix](#4-transition-matrix)
5. [Entry / Exit Actions](#5-entry--exit-actions)
6. [Degraded-State Semantics](#6-degraded-state-semantics)
7. [HTTP Endpoint (`GET /v1/local/daemon/status`) — v2 surface](#7-http-endpoint-get-v1localdaemonstatus--v2-surface)
8. [Wire Compatibility with v1 Probe](#8-wire-compatibility-with-v1-probe)
9. [Library Choice — statig](#9-library-choice--statig)
10. [Implementation Plan](#10-implementation-plan)
11. [Open Questions](#11-open-questions)
12. [References](#12-references)

---

## 1. Scope and Source of Truth

The canonical behavioural requirement is `v1-spec/cli-sync/cli-spec-v1.md` §10.1 (external spec, in `nexus-platform`). This document is the **in-repo implementation contract**: how `nexus42d` realises that six-state lifecycle in Rust using `statig`, how it integrates with the orchestration engine, and how its `GET /v1/local/daemon/status` endpoint evolves from v1's minimal slice to the full FSM.

**In scope**:

- State graph and event catalogue
- Transition rules and guards
- Entry/exit actions (what happens when entering/leaving each state)
- `Degraded` semantics (when the daemon is "running but unhealthy")
- Observability hooks (transition logging, metrics)
- HTTP endpoint behaviour
- Migration from v1 single-probe shape to v2 full-FSM shape

**Out of scope**:

- Orchestration engine internals (see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md))
- ACP worker process model (see [acp-client-tech-spec.md](acp-client-tech-spec.md) §2.3)
- Platform-side signalling / control plane beyond what `GET /v1/local/daemon/status` exposes

---

## 2. State Machine Model

### 2.1 State graph (hierarchical)

```text
                    ┌────────────────────────────────────────┐
                    │                Top                      │
                    │  (implicit superstate; absorbs         │
                    │   unhandled events)                    │
                    └──────────────┬─────────────────────────┘
                                   │
        ┌──────────┬───────────────┼───────────────┬────────────┐
        │          │               │               │            │
   ┌────────┐ ┌──────────┐   ┌──────────┐   ┌──────────┐  ┌──────────┐
   │Stopped │ │ Starting │   │   Alive  │◀──┤ Stopping │  │  Failed  │
   └───┬────┘ └────┬─────┘   │(superst) │   └────┬─────┘  └──────────┘
       │           │         └──┬───┬───┘        │
       │           │            │   │            │
       │           │            ▼   ▼            │
       │           │      ┌─────────┐ ┌──────────┐
       │           │      │ Running │ │ Degraded │
       │           │      └─────────┘ └──────────┘
       │           │          │         │
       ▼           ▼          ▼         ▼
   (initial)   (starts →)  (serves)  (serves partially)
```

- `**Top**` is the implicit superstate provided by `statig`; any event not handled deeper escalates here (and is dropped with a warning log).
- `**Alive**` is an explicit superstate grouping the two "process is up and usable" states (`Running` and `Degraded`). It holds handlers for events that apply to both (e.g. `ShutdownRequested` → `Stopping`, `FatalError(kind)` → `Failed`).

### 2.2 State meanings


| State      | Meaning                                                                                                 | Entered on…                                                              |
| ---------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `Stopped`  | Process running but no runtime subsystems attached; useful only as the initial pseudo-state for testing | Initial state on process start (held for microseconds before `Starting`) |
| `Starting` | Subsystems booting (HTTP listener, DB pool, sync queue, orchestration engine, worker manager)           | Immediately after `Stopped` at process start                             |
| `Running`  | Fully operational; HTTP accepts all endpoints; orchestration engine scheduled                           | All Starting steps succeed                                               |
| `Degraded` | Running **partially**; some subsystems unhealthy but HTTP surface stays up with diagnostics             | Any non-fatal subsystem fault reported while in `Alive`                  |
| `Stopping` | Graceful shutdown in progress; no new work accepted; in-flight work drained                             | `ShutdownRequested` event (SIGINT/SIGTERM or admin request)              |
| `Failed`   | Fatal error; process about to exit with non-zero code                                                   | `FatalError` event from any `Alive`/`Starting` state                     |


### 2.3 Invariants

1. `**Stopped` is never re-entered at runtime** — it is the one-shot initial pseudo-state. Exiting it is irreversible until the next process invocation.
2. **HTTP `GET /v1/local/daemon/status` is available in `Starting`, `Running`, `Degraded`, `Stopping`** — not in `Stopped` (listener not bound yet) or `Failed` (process about to exit).
3. **Orchestration engine is running in exactly `Running` and `Degraded`** — and gracefully drained in `Stopping`.
4. **Once `Failed` is entered, no transitions happen**; process calls `std::process::exit(exit_code)` after entry action completes.

---

## 3. Events

All events are Rust `enum Event` variants handled by the HSM. Source identified per variant.


| Event                          | Source                                        | Payload                                                  |
| ------------------------------ | --------------------------------------------- | -------------------------------------------------------- |
| `ProcessStarted`               | `main` after tokio runtime up                 | `{}`                                                     |
| `SubsystemUp(kind)`            | Subsystem startup code                        | `kind: "http" | "db" | "engine" | "sync" | "worker_mgr"` |
| `SubsystemFailed(kind, err)`   | Subsystem startup code                        | `{ kind, err: String, retryable: bool }`                 |
| `HealthDegraded(kind, reason)` | Subsystem health check (periodic)             | `{ kind, reason }`                                       |
| `HealthRestored(kind)`         | Subsystem health check                        | `{ kind }`                                               |
| `ShutdownRequested(source)`    | SIGINT/SIGTERM handler OR admin API           | `source: "signal" | "admin" | "supervisor"`              |
| `ShutdownDrained`              | Internal — engine & workers done draining     | `{}`                                                     |
| `ShutdownTimeout`              | Timer in `Stopping`                           | `{ grace_ms_exceeded: u64 }`                             |
| `FatalError(kind, err)`        | Any subsystem declaring unrecoverable failure | `{ kind, err: String }`                                  |


---

## 4. Transition Matrix


| From       | Event                                                                                      | Guard                                        | To         | Entry / Exit actions fired                                                            |
| ---------- | ------------------------------------------------------------------------------------------ | -------------------------------------------- | ---------- | ------------------------------------------------------------------------------------- |
| `Stopped`  | `ProcessStarted`                                                                           | —                                            | `Starting` | `Stopped.exit` + `Starting.entry`                                                     |
| `Starting` | `SubsystemUp("http")` and all mandatory subsystems are up (tracked in state-local storage) | `all_mandatory_up()`                         | `Running`  | `Starting.exit` + `Running.entry`                                                     |
| `Starting` | `SubsystemFailed(kind, err)`                                                               | `err.retryable == false` or final retry done | `Failed`   | `Starting.exit` + `Failed.entry`                                                      |
| `Running`  | `HealthDegraded(kind, _)`                                                                  | —                                            | `Degraded` | `Running.exit` + `Degraded.entry` (but Alive.exit is **not** fired — see §5.3)        |
| `Degraded` | `HealthRestored(kind)`                                                                     | all previously-failed subsystems healthy     | `Running`  | `Degraded.exit` + `Running.entry`                                                     |
| `Alive`    | `ShutdownRequested(source)`                                                                | — (handled at superstate)                    | `Stopping` | child-state.exit + `Alive.exit` + `Stopping.entry`                                    |
| `Alive`    | `FatalError(kind, err)`                                                                    | — (handled at superstate)                    | `Failed`   | child-state.exit + `Alive.exit` + `Failed.entry`                                      |
| `Starting` | `ShutdownRequested(source)`                                                                | —                                            | `Stopping` | `Starting.exit` + `Stopping.entry`                                                    |
| `Starting` | `FatalError(kind, err)`                                                                    | —                                            | `Failed`   | `Starting.exit` + `Failed.entry`                                                      |
| `Stopping` | `ShutdownDrained`                                                                          | —                                            | `Failed`*  | `Stopping.exit` + `Failed.entry` with `exit_code = 0`                                 |
| `Stopping` | `ShutdownTimeout`                                                                          | —                                            | `Failed`*  | `Stopping.exit` + `Failed.entry` with `exit_code = 1` and `kind = "shutdown_timeout"` |


 `Failed` with `exit_code = 0` is the "graceful completion" terminal — the name is chosen for HSM simplicity (one terminal state). `/v1/local/daemon/status` distinguishes the two via its `exit_code` / `last_error` fields before the process exits.

Events not listed for a given state defer to the superstate (`Alive` → `Top`); `Top` logs-and-drops.

---

## 5. Entry / Exit Actions

### 5.1 `Starting.entry`

- Bind HTTP listener on configured port (`~/.nexus42/config.json → daemon.port`, default 8420)
  - Emits `SubsystemUp("http")` on success, `SubsystemFailed("http", …)` on error
- Open `nexus-local-db` SQLite pool; run migrations
  - Emits `SubsystemUp("db")` on success
- Initialise `nexus-sync` outbox reader
  - Emits `SubsystemUp("sync")` on success
- Instantiate `OrchestrationEngine` (but do **not** start system Schedule yet — that's `Running.entry`)
  - Emits `SubsystemUp("engine")` on success
- Start `Worker Manager` (no workers spawned until requested)
  - Emits `SubsystemUp("worker_mgr")` on success

Each subsystem bootstrap is its own tokio task; all emit events into the HSM's channel.

### 5.2 `Running.entry`

- Start `_system.maintenance` Session on the orchestration engine (per [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §9.1)
- Resume any `orchestration_sessions` rows whose status was `paused` with reason `daemon_restart` (up to a configurable cap, default 16 concurrent)
- Emit `tracing` event `daemon_lifecycle.running`
- Set `lifecycle_state` string for HTTP endpoint to `"running"`

### 5.3 `Degraded.entry` and `Running.exit` vs `Alive`

By `statig`'s superstate semantics, moving `Running → Degraded` fires:

- `Running.exit` (state-local teardown)
- `Degraded.entry`

And does **not** fire `Alive.exit` + `Alive.entry` because the source and target share the `Alive` superstate.

`Degraded.entry` actions:

- Record which subsystem reported the degradation in state-local storage (`degraded_subsystems: HashSet<SubsystemKind>`)
- Set HTTP endpoint `lifecycle_state` to `"degraded"`; populate `degraded_reasons[]`
- Keep orchestration engine running, **but** the affected capability's `run()` may itself start returning `TransientExternal` errors — that is handled per-preset (§4.4 `acp.`* capabilities check worker health; engine does not globally pause)
- Emit `tracing` event with `reason` and `kind`

`Degraded.exit` actions (on `HealthRestored(kind)` which clears all):

- Clear `degraded_subsystems`
- Emit `tracing` event `daemon_lifecycle.running` (again)

### 5.4 `Stopping.entry`

- Set HTTP endpoint `lifecycle_state` to `"stopping"`; new work requests get `503 Service Unavailable` with `Retry-After: 5`
- Stop accepting new orchestration sessions; call `engine.shutdown(grace_ms)` (default grace 20 s; configurable via CLI flag or env)
- Send `worker/shutdown { grace_ms }` to all active workers; wait for exits (up to `grace_ms / 2`)
- After engine signals drained: flush outbox, close DB pool, close HTTP listener
- Emit `ShutdownDrained` when all of the above settle
- A watchdog timer fires `ShutdownTimeout` after `grace_ms` elapsed without `ShutdownDrained`

### 5.5 `Failed.entry`

- Set HTTP endpoint `lifecycle_state` to `"failed"` (only briefly visible if listener still bound)
- Log final `tracing` event `daemon_lifecycle.failed` with `kind`, `last_error`, `exit_code`
- Call `std::process::exit(exit_code)` after a 100 ms pause (for log flush)

### 5.6 Other exit actions

- `Starting.exit`: cancel any in-flight subsystem start tasks
- `Running.exit`: no-op (engine keeps running across `Running ↔ Degraded`; only `Stopping.entry` actually stops it)
- `Stopping.exit`: no-op (next state is terminal)

---

## 6. Degraded-State Semantics

### 6.1 What can put the daemon in `Degraded`


| Subsystem      | Trigger                                                                                | Recovery signal                                      |
| -------------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `http`         | n/a — if HTTP listener dies, daemon is effectively down; treat as `FatalError("http")` | n/a                                                  |
| `db`           | >3 consecutive SQLite errors in 60 s on **read** queries                               | Single successful read after 30 s settle             |
| `sync`         | `sync.pull` or `sync.push` returning `TransientExternal` for >5 minutes continuously   | `sync.pull` or `sync.push` returning `Ok`            |
| `engine`       | `_system.maintenance` session enters `failed` status                                   | Manual admin action or `_system.maintenance.restart` |
| `worker_mgr`   | Worker spawn failures for all creators over 3 minutes (no successful spawn)            | Any successful spawn                                 |
| `acp_registry` | Registry cache fetches returning network errors for >10 minutes                        | Successful cache refresh                             |


### 6.2 What `Degraded` does **not** change

- HTTP endpoints keep serving (with degraded status populated)
- `nexus42` CLI commands continue to work to the extent the still-healthy subsystems support them
- Existing orchestration sessions continue running; individual capability calls surface errors as `TransientExternal`
- Preset `exit_when.rule` predicates can branch on `Context._degraded: true` (engine injects this when `Degraded`)

### 6.3 `Degraded` vs `Failed` decision rule

- `Degraded` = partial functionality, operator should investigate but process stays up
- `Failed` = cannot continue safely; process exits with non-zero code and operator must restart

Explicit escalation paths (`Degraded → Failed`):

- Database pool returns **write** errors for >30 s → `FatalError("db", "write path unavailable")`
- HTTP listener socket lost → `FatalError("http", "listener dead")`
- Panics in any non-task-isolated code → `FatalError("panic", …)` via panic-hook bridge

---

## 7. HTTP Endpoint (`GET /v1/local/daemon/status`) — v2 surface

### 7.1 v2 response shape

```json
{
  "schema_version": 2,
  "lifecycle_state": "running",
  "version": "0.2.0",
  "implementation_scope": "full-fsm (v2)",
  "uptime_ms": 123456,
  "degraded": {
    "subsystems": [],
    "reasons": []
  },
  "subsystems": {
    "http":        { "status": "up",   "last_check_ms": 123450 },
    "db":          { "status": "up",   "last_check_ms": 123450 },
    "sync":        { "status": "up",   "last_check_ms": 123450 },
    "engine":      { "status": "up",   "active_sessions": 3 },
    "worker_mgr":  { "status": "up",   "active_workers": 1 },
    "acp_registry":{ "status": "up",   "cache_age_ms": 3600000 }
  },
  "exit_code": null,
  "last_error": null
}
```

- `schema_version: 2` — indicates v2 wire shape; v1 response (§8) carries `schema_version: 1` or omits the field.
- `lifecycle_state` ∈ `"starting" | "running" | "degraded" | "stopping" | "failed"` (never `"stopped"` — the endpoint isn't bound yet in that state).
- When `lifecycle_state = "degraded"`, `degraded.subsystems` and `degraded.reasons` are populated; `subsystems.<kind>.status` is `"down" | "degraded"` for affected ones.
- When `lifecycle_state = "stopping"`, the response still renders but new mutating endpoints return `503`.
- `exit_code` and `last_error` populate only when `lifecycle_state = "failed"` (observable only during the ~100 ms pre-exit window).

### 7.2 Schema location

`schemas/acp-runtime/daemon-status-v2.schema.json` — new file. Codegen will produce `crates/nexus-contracts/src/generated/daemon_status_v2.rs` and `packages/nexus-contracts/src/generated/daemon-status-v2.ts` (platform SSOT).

(Naming: `acp-runtime/` host chosen to keep alongside `registry-manifest.schema.json`; alternatively a new `daemon-runtime/` namespace — final location to be decided at Phase 4 implement time. This doc does not freeze the schema path.)

---

## 8. Wire Compatibility with v1 Probe

### 8.1 v1 response shape (frozen)

```json
{
  "lifecycle_state": "running",
  "version": "0.1.0",
  "implementation_scope": "running-probe (v1)"
}
```

### 8.2 Compatibility strategy

- v1 clients look only at `lifecycle_state` and ignore unknown fields. v2 is a **superset** — safe.
- v2 clients can check `schema_version` first; if absent, treat as v1 and consume `lifecycle_state` only.
- No `?version=` query parameter or separate URL — single endpoint, additive evolution.

### 8.3 CI gate

Add a contract test: `GET /v1/local/daemon/status` in v2 code must always include `lifecycle_state: "running"` when daemon is healthy (regardless of other new fields). This protects v1 callers.

---

## 9. Library Choice — statig

### 9.1 Decision

Adopt `statig` (crates.io `statig = "0.3"`; pin to the latest 0.3.x at Phase 4 implement time) for the HSM.

### 9.2 Rationale

- Hierarchical state machines (`Alive` superstate) are a natural fit for lifecycle modelling; flat enums are not.
- `async` handlers — our entry actions involve tokio bootstrap, awaited subsystem readiness.
- Introspection hooks (`before_dispatch`, `after_transition`) map 1:1 to our `tracing` needs.
- Zero-allocation execution path — good fit for shutdown-reliability-critical code.
- Compile-time structure — daemon's state graph is **not user-configurable** (unlike presets, per [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §1.2), so compile-time is a strength.

### 9.3 What we explicitly do **not** use statig for

- Preset execution — that's `graph-flow` territory and requires runtime-constructed graphs (see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §4.1).
- Per-creator Schedule state — also runtime-configurable, handled in orchestration engine, not lifecycle HSM.

### 9.4 Adapter trait (minor insurance)

```rust
// crates/nexus42d/src/lifecycle/mod.rs
pub trait Lifecycle: Send + Sync {
    fn current_state(&self) -> LifecycleState;
    fn dispatch(&self, event: Event);
    fn subscribe(&self) -> broadcast::Receiver<Transition>;
}
```

Only impl is `StatigLifecycle` backed by `statig::awaitable::StateMachine`. Keeping the trait boundary means the daemon's HTTP handler and orchestration engine never touch statig types directly.

---

## 10. Implementation Plan

### 10.1 Phase 4 scope (see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10.5)

- Add `statig = "0.3"` to `crates/nexus42d/Cargo.toml`
- New module `crates/nexus42d/src/lifecycle/` with `state.rs` (HSM impl), `events.rs` (event enum), `actions.rs` (entry/exit handlers), `mod.rs` (`Lifecycle` trait + `StatigLifecycle`)
- Rewrite `crates/nexus42d/src/main.rs`:
  - Create `StatigLifecycle` in `main` before any subsystem start
  - Dispatch `ProcessStarted`
  - Hook `tokio::signal` for SIGINT/SIGTERM → `dispatch(ShutdownRequested)`
  - Hook Rust `panic_hook` → `dispatch(FatalError("panic", …))`
  - Wait for `Transition → Failed` then return `exit_code`
- Rewrite `crates/nexus42d/src/api/handlers/runtime.rs` (`daemon_status` handler) to read from the `Lifecycle` trait object + subsystem health registry
- Add new schema `schemas/acp-runtime/daemon-status-v2.schema.json`; regenerate contracts (`pnpm run codegen`)
- Integration test in `crates/nexus42d/tests/lifecycle.rs`:
  - Process cold start → `Running` within N seconds
  - SIGTERM → `Stopping` → clean exit (0) within grace
  - Subsystem failure → `Degraded` response fields populated
  - `FatalError` → exit code non-zero

### 10.2 Parallelism with Phase 2

Phase 4 **can** start the moment `crates/nexus-orchestration` has its first shippable commit (so `OrchestrationEngine` trait exists for `Running.entry` to call). Running it in parallel shortens overall wall-clock; see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10.1.

### 10.3 Backward-compat note

Because the v2 response is a superset of v1, no clients need to upgrade in lockstep. Platform can continue consuming v1 fields; the daemon supports both implicitly.

---

## 11. Open Questions


| ID   | Question                                                                                                                                                                                                                    | Owner                                      |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| LQ-1 | What happens if `ShutdownRequested` fires during `Starting`? Current design says `Starting → Stopping` directly (short-circuit). Confirm that partially-initialised subsystems have idempotent `shutdown()` calls.          | Phase 4 implement; add abort-on-start test |
| LQ-2 | Should `HealthDegraded(kind=...)` that fires *during* `Starting` (before `Running`) delay `Running` transition, or be buffered and replayed? Draft rule: **buffered**, replayed immediately after `Running.entry`. Confirm. | Phase 4 implement                          |
| LQ-3 | Admin-initiated shutdown endpoint — add now (`POST /v1/local/daemon/shutdown`) or defer to V1.5? Likely defer; lifecycle doc does not need it for TD-9 closure.                                                             | V1.4 vs V1.5 scope                         |


---

## 12. References

Internal:

- [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) — sibling spec; lifecycle starts/stops the engine
- [acp-client-tech-spec.md](acp-client-tech-spec.md) — worker graceful shutdown keyed off `Stopping.entry`
- [architecture-alignment-review.md](architecture-alignment-review.md) — TD-9 row moves to "Resolved (v2)"
- [daemon-lifecycle-api-legacy.md](archived/knowledge/daemon-lifecycle-api-legacy.md) — archived; do not rely on directly

External:

- statig: [https://github.com/mdeloof/statig](https://github.com/mdeloof/statig)
- Miro Samek — *Practical UML Statecharts in C/C++* — conceptual reference for HSM design

---

*End of specification. Land alongside Phase 4 of the A-track orchestration rollout.*