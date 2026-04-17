# Daemon lifecycle API vs cli-spec §10.1 (TD-9)

> **⚠ Superseded by [daemon-lifecycle-api-v2.md](../../knowledge/daemon-lifecycle-api-v2.md) (2026-04-17; archived here the same day).**
> v2 is the authoritative design for the full 6-state `statig` HSM (TD-9 closure). This v1 document is preserved only to keep historical plan references reachable — do not cite this file in new work.

**Source plan:** `v1-tech-debt-cleanup` (Batch D, Task 15 / TD-9)  
**Status:** Superseded by v2 — retained for historical reachability

## Spec expectation (summary)

`cli-spec-v1` §10.1 describes a **six-state** daemon lifecycle (`Stopped`, `Starting`, `Running`, `Degraded`, `Stopping`, `Failed`) with explicit transitions and observability expectations.

## What shipped (V1.1-era milestone)

- **Endpoint:** `GET /v1/local/daemon/status` (unguarded; same class as `GET /v1/local/runtime/`*).
- **JSON shape:** `lifecycle_state` (today always `"running"` while the Local API is listening), `version` (crate package version), `implementation_scope` (human-readable scope note for automation authors).

**Implementation:** `crates/nexus42d/src/api/handlers/runtime.rs` (`daemon_status`), router wiring in `crates/nexus42d/src/api/mod.rs`.

## Gap (explicit)

- No `DaemonState` enum covering all six states.
- No transition guards, `Degraded` health integration, or graceful `Stopping` → `Stopped` orchestration.
- `lifecycle_state: "running"` is a **probe**, not a full state machine.

## Follow-up (V1.2+)

Introduce an internal state machine owned near daemon startup (`main`/supervisor), drive transitions from listen/bind/shutdown paths, and expand the endpoint (or add `/v1/local/daemon/status` versioning) without breaking existing clients.

## Related

- [architecture-alignment-review-v1.md](architecture-alignment-review-v1.md) (TD-9 source)