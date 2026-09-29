# Reference Knowledge — Reference Body Refreshable Scan Pipeline

**Status**: Normative (V1.58 P-last promote)
**Document class**: Master
**Created**: 2026-06-22 (Draft at P1)
**Promoted to Master**: 2026-06-22 (V1.58 P-last)
**Scope**: Reference body externalization — refreshable scan pipeline for DF-44
**Coordinates with**: [acp-capability-set.md](acp-capability-set.md) §4, [capability-registry.md](capability-registry.md) §2.8, [daemon-runtime.md](daemon-runtime.md) §10, [entity-scope-model.md](entity-scope-model.md)

---

## 0. Document position

This Draft spec defines the `nexus.reference.refresh` capability and the
reference body refreshable scan pipeline introduced in V1.58 P1 (DF-44).
It covers the refresh policy model, DB schema for refresh tracking,
capability admission contracts, and the refresh-scheduler hook.

The static registration of reference sources was shipped in V1.26.
V1.58 P1 adds the refreshable pipeline core (capability + DB migration +
scheduler). V1.58 P3 adds the CLI subcommand and cross-cut E2E tests.

---

## 1. Scope

- Refresh policy model: `on_change` / `scheduled` / `offline` semantics.
- DB schema for refresh tracking: `reference_sources.last_refreshed_at`,
  `refresh_policy`, `refresh_status` columns and supporting indexes.
- `nexus.reference.refresh` capability: admission, handler binding, output shape.
- Refresh-scheduler hook (retained implementation, §3): periodic stale-source scan + dispatch.
- Integration points: `capability::Registry` (orchestration),
  `crates/nexus-core/src/execution/schedules/refresh.rs` (retained scheduler,
  unwired — §3).

Non-goals: CLI subcommand (`nexus42 reference refresh`) deferred to P3;
cross-cut E2E tests deferred to P3; `entity-scope-model.md` unchanged
(reference refresh is reference-source-scoped, not KB-scoped). **Durable roadmap:** DR-07.

---

## 2. Refresh policy model

Every reference source has a `refresh_policy` that governs when and how
its body content is refreshed:

| Policy | Semantics |
| --- | --- |
| `offline` | Source is explicitly static. Refresh is blocked (`policy_blocked`). Default for newly registered sources. |
| `on_change` | Source participates in every refresh sweep. The handler fetches the URL, compares the content hash, and updates the body if the hash differs. |
| `scheduled` | Source is evaluated on a periodic cadence. The handler checks `last_refreshed_at` against a configurable stale-threshold interval. If the source is stale, it fetches and updates; otherwise returns `not_modified`. |

The refresh lifecycle status (`refresh_status`) tracks the current state:

| Status | Meaning |
| --- | --- |
| `fresh` | Source was successfully refreshed and content is current. |
| `stale` | Source has not been refreshed yet or is past its stale threshold. |
| `refreshing` | A refresh is currently in progress (prevents concurrent refresh). |
| `error` | The last refresh attempt failed (network timeout, HTTP error, etc.). |
| `NULL` | Source has never been refreshed (initial state for new registrations). |

---

## 3. Refresh scheduler contract

The refresh-scheduler implementation in `crates/nexus-core/src/execution/schedules/refresh.rs`
exposes `spawn_refresh_scheduler` (which spawns a periodic `tokio` task when
called) and `run_one_refresh_tick` (one sweep tick). The capability is owned by
`crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`;
tool registration and creator-context wiring live in
`crates/nexus-core/src/execution/capabilities.rs`.

**Retained implementation, not a running service.** These are the current code
homes, but no current boot path spawns the scheduler: `spawn_refresh_scheduler`
has zero call sites in `crates/` and `apps/` (the daemon boot path that spawned
it was deleted with the v1.193 P2 host removal). Nor does any production caller
invoke `nexus.reference.refresh`; only
`crates/nexus-orchestration/tests/cross_reference_refresh_e2e.rs` exercises the
capability. Registered reference sources therefore do **not** refresh
automatically today; periodic refresh is unwired pending an explicit
boot/composition decision.

**V1.58 historical host:** the scheduler formerly lived in
`crates/nexus-daemon-runtime/src/refresh_scheduler.rs`, and the former
`crates/nexus-daemon-runtime/src/boot.rs` §4e spawned it at daemon startup.
Those deleted host paths describe the V1.58 composition, not current boot wiring.
The retained scheduler contract is:

- **Cadence**: Configurable interval (default 3600s = 1 hour). Overridable via
  `NEXUS_DAEMON_REFRESH_SCHEDULER_INTERVAL_SECS` env var.
- **Stale threshold**: Configurable (default 86400s = 24 hours). Overridable via
  `NEXUS_DAEMON_REFRESH_SCHEDULER_STALE_THRESHOLD_SECS` env var.
- **Initial delay**: First refresh cycle fires after 60s (avoids blocking daemon boot).
- **Query logic**: `find_stale_sources(pool, limit=50, stale_threshold_seconds)` returns
  sources matching:
  - `refresh_policy != 'offline'`
  - `refresh_status != 'refreshing'` (idempotent)
  - `refresh_policy = 'on_change'` OR (`refresh_policy = 'scheduled'` AND
    (`last_refreshed_at IS NULL` OR `last_refreshed_at < now - stale_threshold`))
  - Ordered by `last_refreshed_at ASC NULLS FIRST` (least-recently-refreshed first).
- **Dispatch**: For each stale source, invokes `ReferenceRefresh::run(input)` directly
  with `{"reference_source_id": "<id>"}`.
- **Observability**: `tracing` spans at each tick; `info!` per-source refresh result;
  success/failure counters per tick.
- **Graceful shutdown**: Exits cleanly when `shutdown_notify` fires; errors are
  logged and the loop continues (non-fatal).
- **Idempotency guard**: The `refresh_status = 'refreshing'` marker acts as an
  in-progress lock: `mark_refreshing` sets the status before the fetch,
  and `find_stale_sources` excludes sources with `refresh_status = 'refreshing'`.
  This prevents concurrent scheduler ticks from refreshing the same source.
  **Limitation (single-daemon only):** This guard is best-effort within a single
  daemon process. There is no cross-invocation or cross-process lock. An explicit
  CLI call (P3) arriving between the SELECT and the UPDATE, or two daemon
  instances against the same SQLite file, could both proceed to fetch. For the
  V1.58 single-daemon local model this is acceptable; a cross-process mutex or
  row-level optimistic concurrency check (OCC) on the refresh columns should be
  considered when P3 lands and multi-process scenarios are supported (DR-07).

---

## 4. DB schema for refresh tracking

Migration file: `crates/nexus-local-db/migrations/202606220003_reference_sources_refresh_tracking.sql`

Added columns to `reference_sources`:

| Column | Type | Default | Description |
| --- | --- | --- | --- |
| `last_refreshed_at` | `TEXT` | `NULL` | ISO-8601 timestamp of last successful refresh. |
| `refresh_policy` | `TEXT` | `'offline'` | Enum: `on_change`, `scheduled`, `offline`. |
| `refresh_status` | `TEXT` | `NULL` | Enum: `fresh`, `stale`, `refreshing`, `error`. |

Indexes:

- `idx_reference_sources_refresh_policy` — partial index on `refresh_policy`
  WHERE `refresh_policy != 'offline'` (the refresh scheduler queries this).
- `idx_reference_sources_refresh_status` — index on `refresh_status` for
  quick filtering.

Current tracking DAO owner: `crates/nexus-local-db/src/reference_source.rs` (refresh lifecycle methods introduced with the V1.58 migration above):

- `set_refresh_policy(source_id, policy)` — change the refresh policy.
- `mark_refreshing(source_id)` — set `refresh_status = 'refreshing'`.
- `mark_refreshed(source_id, new_body_hash)` — set `last_refreshed_at`,
  `refresh_status = 'fresh'`, `content_hash`.
- `mark_refresh_error(source_id, error_msg)` — set `refresh_status = 'error'`.
- `find_stale_sources(pool, limit, stale_threshold_seconds)` — find sources
  due for refresh.

---

## 5. Capability IDs and admission contracts

### `nexus.reference.refresh`

- **id**: `nexus.reference.refresh`
- **Handler**: `ReferenceRefresh::run()` in `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`
- **Registration**: Orchestration `CapabilityRegistry` (pool-aware; pool-less returns `WorkerUnavailable`). Not registered in `host_tool_registry()` (reference-source-scoped, not ACP-facing).
- **Input**: `{"reference_source_id": "<id>", "url": "<optional override>"}`
- **Output**: `{"reference_source_id", "refreshed", "content_changed", "new_content_hash", "refreshed_at", "status", "bytes_fetched"}`
- **Admission gates**:
  - Reference source must exist in `reference_sources` table (else `invalid_input`).
  - `refresh_policy != 'offline'` (else `policy_blocked`).
  - URL must be non-empty (else `error` status, not a capability error — the handler
    returns an error status in the output JSON).
  - Network timeout returns `TransientExternal` capability error.
- **Pool dependency**: Without a pool, returns `WorkerUnavailable`. The retained
  scheduler constructs the capability with its own pool when spawned (§3).

### Sibling capability IDs (deferred)

`nexus.reference.refresh_policy.get` and `nexus.reference.refresh_status` are
deferred to P3 if the user-facing surface (CLI) requires them. P1 ships only
`nexus.reference.refresh` as the core pipeline capability.

> **Durable roadmap:** DR-07 (refresh caps + CLI + E2E + cross-process OCC).

---

## 6. Integration points

- **`crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`**:
  Capability handler struct + `Capability` trait impl.
- **`crates/nexus-core/src/execution/capabilities.rs`**:
  Registers `nexus.reference.refresh` in the core tool table and wires the pool
  and active creator context before delegating to `ReferenceRefresh`.
- **`crates/nexus-core/src/execution/schedules/refresh.rs`**:
  Owns `spawn_refresh_scheduler` and `run_one_refresh_tick`; queries stale
  sources and dispatches refresh. The former V1.58 daemon boot composition is
  historical (§3), not a current startup anchor.
- **`crates/nexus-local-db/src/reference_source.rs`**:
  Refresh lifecycle DAOs (`set_refresh_policy`, `mark_refreshing`,
  `mark_refreshed`, `mark_refresh_error`, `find_stale_sources`).
- **`crates/nexus-local-db/migrations/202606220003_reference_sources_refresh_tracking.sql`**:
  DB migration adding `last_refreshed_at`, `refresh_policy`, `refresh_status`.

---

## 7. Examples

### 7.1 Refresh a source via capability invocation

```json
// Input
{"reference_source_id": "ref_abc123"}

// Output (success, content changed)
{
  "reference_source_id": "ref_abc123",
  "refreshed": true,
  "content_changed": true,
  "status": "fresh",
  "new_content_hash": "abc...def",
  "refreshed_at": "2026-06-22T12:00:00Z",
  "bytes_fetched": 4096
}

// Output (offline source)
{
  "reference_source_id": "ref_abc123",
  "refreshed": false,
  "content_changed": false,
  "status": "policy_blocked",
  "new_content_hash": "...",
  "refreshed_at": null,
  "bytes_fetched": 0
}
```

### 7.2 Scheduled refresh flow

1. V1.58 historical daemon boot flow: the former host spawned the scheduler with a 60s initial delay. The current scheduler implementation is `crates/nexus-core/src/execution/schedules/refresh.rs` (§3); this example does not assert current host startup wiring.
2. After 60s, first tick: `find_stale_sources()` queries `reference_sources`.
3. For each stale source: `ReferenceRefresh::run({"reference_source_id": "..."})`.
4. Handler fetches URL, compares hash, updates DB.
5. Results logged with tracing spans; success/failure counters emitted.
6. Next tick after configurable interval (default 3600s).
