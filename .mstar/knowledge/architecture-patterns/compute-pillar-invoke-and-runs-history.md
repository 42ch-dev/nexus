# Compute pillar: daemon invoke route + Runs history semantics

**Status:** shipped (V1.147) · **Category:** architecture-patterns · **Applies to:** nexus-daemon-runtime, nexus-wasm-host, nexus-local-db, apps/web

## Problem

Nexus needs an ACP-agent-capable compute lane: world-scoped module runs that are persisted as
reviewable history ("Runs"), inspectable on the World Timeline, and clearable by the user —
without granting the daemon agent/server powers, without syncing full blobs, and without
mutable history.

## Decision

1. **Route + projection model**: `POST /v1/daemon/worlds/:world_id/compute/run` (ACP client
   call → direct lane) writes a canon-only projection `compute_result` (never full memory/
   session state). Related: accept/discard/list/detail + `DELETE /compute/runs` (world-gated).
2. **Canon append**: Accept paths write via `append_event_canon_with_extensions_in_tx`
   (single TX, canon + extensions), keeping accept atomic with the event write.
3. **Retention: retain-by-default + manual Clear.** Runs history is a product surface (undo/
   review), not disposable cache. Delete = explicit user action, `world_id` required,
   terminal statuses only (`applied|discarded|failed`), ownership-guarded; never touches
   spoke rows. **No auto-TTL by default**; re-open auto-TTL only with volume evidence.
4. **Timeline integration**: per-world events route (`GET /worlds/:world_id/timeline/events`)
   with `compute_result` + canon merge on read (Narrative merge). Canvas projection keeps
   `isProjectionCapped` honesty (500-cap, 100/page hard cap) instead of silent truncation.
5. **Validation honesty**: wasm-host returns `InputValidationFailed` → daemon 422 with
   per-entry `details.invalid_entries` (serialized under `details`, not top-level), so the UI
   can surface per-entry failures without breaking strict readers.
6. **Sandbox honesty**: 503/502 sandbox-not-ready vs compute-failed are distinct; no fake
   success. **Empty-clear**: clearing a world with zero runs returns ok-with-count-0 (no
   confusing error).
7. **UI**: Run Studio as promoted primitives (`@42ch/nexus-ui`) + thin web wrapper; Timeline
   invoke pre-fills Module+Run, `?module=&run=` deep-link, ⌘K jump commands. DELETE surfaced
   as Clear with world-gated confirm.

## Consequences / invariants

- Wire contracts move in lockstep (schema bump + codegen idempotent; generated TS/Rust
  committed in same change).
- History is append-only canon; world history immutability preserved (fork, not mutation).
- Compute never syncs full memory — only structured projection deltas.
- Contract gap rule: never add a wire field the client can't read (422 details were moved
  under `details` precisely for forward-reader safety).

## Related

- `daemon-api-surface-conventions.md` §12.3 (route table), `compute-module-abi.md`,
  `wasm-host.md`, `web-ui.md` (forward pointer)
- Tracker rows: `DF-V1122-COMPUTABLE-UI`, `DF-V1122-COMPUTE-ON-TIMELINE` (shipped archive)
