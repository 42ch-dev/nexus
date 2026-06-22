# Nexus Daemon Runtime Architecture

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative |
| **Document class** | Master |
| **Normative scope** | Architecture boundaries, process model, subsystem responsibilities, pre-release constraints |
| **Related** | [cli-spec.md](./cli-spec.md), [local-runtime-boundary.md](./local-runtime-boundary.md), [agent-host.md](./agent-host.md) |

---

## 1. Objective

Converge on **one user-facing binary** (`nexus42`) with **daemon runtime** as an internal process mode — not a separate product binary (daemon runtime).

Pre-release posture: no compatibility migration layer required; local state may be wiped (see nexus-platform `v1-spec/adr/adr-023-pre-release-cli-breaking-refactor-v1.md` if needed).

---

## 2. Normative layering

```text
nexus42 (CLI — entry, routing, UX)
  ├─ nexus-daemon-runtime (library — lifecycle, subsystems, local API)
  │    ├─ local DB / workspace handles
  │    ├─ schedule / worker supervision
  │    ├─ loopback Local API (/v1/local/*) — local product only
  │    └─ AgentHostSubsystem → nexus-agent-host (see agent-host)
  └─ nexus-cloud-sync (CLI-only; platform HTTP + optional legacy-sync)
```

Platform sync and registration **must not** live in daemon-runtime. See [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md).

**Rules**:

1. Only **`nexus42`** is a user-facing executable artifact.
2. **Daemon** is started via CLI (`nexus42 daemon start`, foreground or background); background mode may use a hidden internal entry (implementation detail in knowledge SSOT).
3. **Local API** remains loopback HTTP and/or Unix socket; clients must not assume a separate daemon product binary.

---

## 3. Subsystem responsibilities

| Subsystem | Owns | Does not own |
| --- | --- | --- |
| CLI | Parsing, one-shot commands, spawning daemon mode, user errors | Long-lived agent protocol details |
| Daemon runtime | `SQLite` handles, Local API listener, orchestration/agent-host, workspace session persistence (`workspace_sessions` DB table, V1.56 P0), graceful shutdown | Platform HTTP, sync outbox, creator registration |
| Agent host | Managed agent sessions (see agent-host) | Platform HTTP |
| Cloud sync (CLI) | Platform HTTP, legacy bundle sync (`nexus-cloud-sync`) | Daemon Local API |

---

## 4. Process model

### 4.1 Foreground

`nexus42 daemon start --foreground` runs the runtime in the current process until shutdown.

### 4.2 Background

Default `nexus42 daemon start`: preflight → spawn internal daemon-run mode → parent exits after startup gate. **Semantics** are normative; exact argv names are implementation SSOT.

### 4.3 Control plane

`status`, `stop`, `restart` coordinate via runtime health and process supervision (parity with prior daemon product behavior).

---

## 5. ACP role invariant

Daemon runtime is a **local supervisor**. It is **not** an ACP Agent or ACP Server and must **not** be advertised via ACP Registry as an agent. ACP Client role stays on the Nexus control plane path ([local-runtime-boundary](./local-runtime-boundary.md) §1).

---

## 6. Observability & errors

- User-facing logs refer to **Nexus daemon runtime**, not legacy daemon runtime product naming.
- Errors are owned by layer: CLI (misuse) → runtime (orchestration) → API handlers (request validation).

---

## 7. Acceptance criteria (architecture level)

1. Specs and docs do not **require** a standalone daemon runtime product binary.
2. Health endpoint reachable after foreground and background start.
3. Stop/restart leaves no orphan runtime without documented force path.
4. Agent-host subsystem can start under Managed-only rules ([agent-host](./agent-host.md)).

---

## 8. Verification matrix

1. `nexus42 daemon start --foreground` boots and serves health endpoint
2. Default background start returns and runtime stays alive
3. `status` sees running runtime
4. `stop` terminates runtime cleanly
5. `restart` replaces process and health returns
6. ACP-related runtime paths continue to function
7. Schedule supervisor boot and shutdown hooks remain valid

## 9. Implementation batches

### Batch 1: Runtime extraction

- Create `nexus-daemon-runtime`; migrate modules from legacy daemon runtime layout

### Batch 2: Single-binary wiring

- Wire `nexus42 daemon` to runtime / internal-run mode

### Batch 3: Remove old daemon crate

- Remove daemon runtime workspace member and references

### Batch 4: Naming and hardening

- Unify user-facing wording and logs; finalize reliability edge cases

---

## V1.57 P1 Draft overlay: Host tool executor — 3-caller entry points

**Status**: Draft (V1.57 P1)  
**Plan**: `2026-06-22-v1.57-daemon-refactor-and-caller-adapters`

### Host tool dispatch topology

The host tool executor (`host_tool_executor.rs`) provides three caller entry
points, all dispatching through the same `CapabilityRegistry::dispatch` path:

| Entry point | Caller | Normalization | Dispatch |
|-------------|--------|---------------|----------|
| `HostToolExecutor::execute()` | CLI `host-call` + HTTP `POST /v1/local/agent-host/internal/tool-executions` | `ToolExecuteRequest` → admission pipeline | `CapabilityRegistry::dispatch` |
| `HostToolExecutor::dispatch_from_worker()` | Worker `agent_tool_request` IPC | `{tool_name, args, request_id}` → `ToolExecuteRequest` | Same path |
| `HostToolExecutor::dispatch_for_schedule()` | Schedule executor (in-process) | `{tool_name, args, request_id}` → `ToolExecuteRequest` with `HostToolCallerKind::Schedule` | Same path |

All three entry points share a single admission pipeline (5 gates: allowlist,
active creator, workspace bounds, permissions.toml, audit log) and dispatch
through the same `CapabilityRegistry::dispatch(tool_id, input)` call.

### V1.57 P1 refactor

- `host_tool_executor.rs` reduced from 4298→349 lines (handlers extracted to
  `host_tool_handlers.rs`; tests to `host_tool_executor_tests.rs`)
- Previously-duplicated `execute_X` functions removed; handlers live in the
  registry-bound `host_tool_handlers` module
- `CdnConfig` constructor-injected (no global `RwLock`)

### V1.57 P3: Worker IPC allowlist — dynamic derivation

**Status**: Shipped (V1.57 P3)
**Plan**: `2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e`

The admission pipeline's Gate 1 (tool ID allowlist) now uses
`CapabilityRegistry::lookup()` as its dynamic SSOT instead of the static
`TOOL_ALLOWLIST` constant (see `host_tool_handlers.rs::admission_pipeline`).
This means the worker `agent_tool_request` IPC path — which normalizes
through `HostToolExecutor::dispatch_from_worker()` → `execute()` →
`admission_pipeline()` — derives its allowlist from the same registry as
CLI and HTTP entry points. All 18 shipped `nexus.*` host tool IDs are
dispatchable via worker IPC; unknown IDs return `NOT_SUPPORTED`.

Cross-caller E2E test: `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs`
verifies dispatch equivalence across all 3 caller paths for all 18 IDs
(54 invocation cases).

## V1.58 P0 Draft overlay: .sqlx cache hygiene protocol (R-V156-PROCESS-01 + R-V156P1-CACHE-01)

**Status**: Draft (V1.58 P0)
**Plans**: `2026-06-22-v1.58-workspace-occ-hardening` (T18)

The `.sqlx/` compile-time query cache must be regenerated whenever a SQL
migration or `sqlx::query!` / `sqlx::query_as!` / `sqlx::query_scalar!` macro
is added or modified — in **library code OR test code**.

### Protocol

1. **After any migration or query change**, run:
   ```sh
   DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc" cargo sqlx prepare --workspace -- --tests
   ```
   The `--tests` flag is **critical** (R-V156P1-CACHE-01): it ensures
   `sqlx::query!` macros inside `#[cfg(test)]` modules and integration test
   files are also captured. Omitting `--tests` produces a cache that compiles
   the library but fails the test binaries under `SQLX_OFFLINE=true`.

2. **Commit the regenerated `.sqlx/query-*.json` artifacts**. The `.sqlx/`
   directory is tracked in git; `state.db`, `state.db-wal`, `state.db-shm`
   are gitignored.

3. **CI verification** (offline mode — no live database required):
   ```sh
   SQLX_OFFLINE=true cargo check --workspace --tests
   ```
   This validates every `query!` macro against the committed cache. A
   failure means the cache is stale — re-run step 1.

4. **Equivalently**, `cargo sqlx prepare --workspace --check -- --tests`
   exits 0 when the cache is up-to-date and 1 when it needs regeneration.
   Note: `--check` goes **before** `--` (the plan's original
   `cargo sqlx prepare --workspace -- --tests --check` ordering is incorrect
   for sqlx-cli 0.8+).

### Common pitfall (R-V156P1-CACHE-01)

`cargo sqlx prepare --workspace` (without `--tests`) generates a cache that
omits test-only queries. The library compiles, but `cargo test --workspace`
fails under `SQLX_OFFLINE=true` with "no cached statement" errors on test
binaries. Always include `--tests`.

### Regression guard (V1.58 P0 fix-wave — QC2 H-3)

A lightweight integration test in `nexus-local-db`
(`tests/sqlx_cache_intact.rs::sqlx_cache_is_present_and_non_empty`) asserts
the workspace `.sqlx/` directory exists and contains at least 50
`query-*.json` artifacts. This catches accidental mass deletion (the exact
P1 incident dropped the count from 138 to 1) without being brittle to normal
query add/remove churn. It does NOT validate query correctness — that remains
the job of `SQLX_OFFLINE=true cargo check --workspace --tests` in CI. Run
locally with `cargo test -p nexus-local-db --test sqlx_cache_intact`.

## V1.58 P0 Draft overlay: Workspace OCC hardening (R-V156P0-M001..M006)

**Status**: Draft (V1.58 P0)
**Plans**: `2026-06-22-v1.58-workspace-occ-hardening` (T1–T6)
**Coordinates with**: `concurrency.md` §7 (per-row OCC)

### Path canonicalization contract (R-V156P0-M002)

`WorkspaceSessionManager::open_session` canonicalizes the workspace root via
`std::fs::canonicalize` before computing content hashes. The target path is
canonicalized and checked against the canonical workspace root prefix via
`enforce_path_boundary`. Symlinks inside the workspace are **skipped** during
hash computation (`symlink_metadata` check) so a symlink chain cannot escape
the workspace root.

### TOCTOU mitigation (R-V156P0-M005)

The commit path (`commit_workspace` HTTP handler →
`WorkspaceSessionManager::commit_session`) validates the `changes[]` manifest
and consumes the session in a single method call, closing the TOCTOU window
between `validate_changes_manifest` and `consume_session`. The underlying
`db::consume_session` atomic `UPDATE ... WHERE consumed = 0 AND expires_at > now`
is the compare-and-swap primitive; `commit_session` is the transaction guard.

#### Retry semantics (V1.58 P0 fix-wave — QC3 F-002)

**No automatic retry on CAS loss.** When two concurrent `commit_session`
calls race on the same session ID, exactly one wins (the atomic
`UPDATE ... WHERE consumed = 0` ensures single-consumer semantics); the loser
receives `SessionError::AlreadyCommitted` immediately — no backoff, no sleep,
no max-retry counter. The OCC conflict counter (`occ_conflict_total`)
increments on the losing side with a structured `tracing::warn!`
(conflict_type = "already_consumed") for observability.

This one-shot design is intentional: the validate+consume pair binds a single
logical operation, and retrying the consume in isolation would be unsound
(the session snapshot may have changed since validate ran). Higher layers
that want retry-on-conflict must implement it above the session layer
(re-open → re-validate → re-commit).

Atomicity is provided by `SQLite`'s database-level write lock: two concurrent
consumers race on `rows_affected()` — exactly one gets 1 (`Consumed`), the
other gets 0 (re-read → `AlreadyConsumed` or `Expired`).

### Async I/O (R-V156P0-M004)

Content hashing (`compute_content_hashes`, `compute_single_file_hash`) uses
`tokio::fs` + `AsyncReadExt`, not blocking `std::fs`. This prevents executor
stalls when the daemon processes large workspace directories.

V1.58 P0 fix-wave (QC2 H-1 / QC3 F-001): `canonicalize_workspace_root` (used
by `open_session` and `validate_changes_manifest`) wraps
`std::fs::canonicalize` in `tokio::task::spawn_blocking` because tokio has no
native async `canonicalize`. This closes the last blocking-syscall gap in
the async session paths. The workspace-root canonicalize is computed once
per `validate_changes_manifest` call (memoized outside the per-change loop)
to avoid O(N) syscalls for N changes (QC3 F-003).

### Metrics & tracing (R-V156P0-M006)

OCC conflicts (AlreadyConsumed race losers, content hash mismatches) emit
`tracing::warn!` with structured fields (`session_id`, `conflict_type`) and
increment the process-wide `occ_conflict_total` AtomicU64 counter (read via
`workspace::session::occ_conflict_total()`).

### Deferred suggestions (V1.58 P0 fix-wave — QC3 S-001 / S-002)

The following QC3 suggestions were reviewed and deferred (no measured need;
current implementation is correct and documented):

- **S-001 (jitter range expansion)**: the current 100–500 ms jitter range
  (in `retry_jitter_ms`) is documented as "sufficient for jitter; not
  cryptographic" and combines with exponential backoff (500 ms ×
  2^(attempt-1)). Expanding to 100–1000 ms for high-N (N ≥ 100) concurrent
  refresher scenarios is speculative without a measured contention incident
  — the daemon runtime is single-process local-first and does not currently
  approach N=100. Deferred until a surge-load incident is observed.
- **S-002 (metrics overhead benchmarking)**: the four `AtomicU64` counters
  in `registry.rs` use `Ordering::Relaxed` (optimal for non-cross-thread
  data-dependency counters). Expected overhead is < 10 ns per call
  (`fetch_add` on a hot cache line). Adding a dedicated micro-benchmark is
  low value; the existing `synthetic_warm_run` bench (734 ns end-to-end)
  already confirms metrics overhead is negligible at the capability layer.
   Deferred; revisit only if profiling shows > 1% of cold path.

## 10. Refresh-scheduler hook (V1.58 P1 / P3)

### 10.1 Overview

The daemon runtime includes a background refresh-scheduler task (`refresh_scheduler::spawn_refresh_scheduler`) that periodically scans the `reference_sources` table for stale rows and dispatches `nexus.reference.refresh` for each candidate.  The scheduler is a detached `tokio::spawn` task — all errors are logged at `warn!` level and never bubble out to the daemon lifecycle.

### 10.2 Configuration

| Knob | Default | Env override | Description |
|------|---------|-------------|-------------|
| Sweep cadence | 3600 s (1 h) | `NEXUS_DAEMON_REFRESH_SCHEDULER_INTERVAL_SECS` | How often the scheduler scans for stale sources |
| Stale threshold | 86400 s (24 h) | `NEXUS_DAEMON_REFRESH_SCHEDULER_STALE_THRESHOLD_SECS` | How old a `scheduled` source must be to count as stale |
| Initial delay | 60 s | — | First cycle fires after this delay to avoid blocking daemon boot |

### 10.3 Query logic

The `find_stale_sources` DAO (`nexus_local_db::reference_source`) excludes:
- Sources with `refresh_policy = 'offline'`
- Sources with `refresh_status = 'refreshing'` (concurrent-refresh guard)

`on_change` sources are always included.  `scheduled` sources are included when `last_refreshed_at IS NULL` or older than the stale threshold.  Results are capped at 50 per tick and ordered by `last_refreshed_at ASC NULLS FIRST`.

### 10.4 Dispatch path

```
refresh_scheduler::run_one_refresh_tick
  └─ for each stale source:
       └─ ReferenceRefresh::run({ "reference_source_id": "<id>" })
            └─ get_by_id → check policy → mark_refreshing → fetch URL
                 → hash → mark_refreshed → body.md write (if creator context)
```

The scheduler path does NOT set creator context — therefore body.md on-disk writes are deferred to the CLI-initiated refresh path.

### 10.5 Error handling

- Individual source refresh failures are logged and counted; they never abort the tick.
- `find_stale_sources` query failure logs a warning and skips the tick.
- Counters: `success` / `failure` per tick, logged at `info!` level.

### 10.6 Tracing contract

- `info!` at task start, each source refresh, and tick completion.
- `warn!` on fetch failure or DB query failure.
- `debug!` when no stale sources are found.
- All messages carry the `reference_source_id` as a structured field.

---

## 11. Outbox flush/compact invocation path (V1.59 P1)

### 11.1 Overview

The orchestration engine's `outbox.flush` and `outbox.compact` capabilities are invoked through the standard capability dispatch path (see [orchestration-engine.md](orchestration-engine.md) §5.7). Both are local-only, pool-backed capabilities that operate directly on the unified `outbox_entries` table in `state.db`.

### 11.2 Dispatch path

```
CapabilityRegistry::get("outbox.flush") / get("outbox.compact")
  └─ capability.run(input)
       └─ OutboxFlush / OutboxCompact (orchestration crate)
            └─ Direct SQL on outbox_entries via injected sqlx::SqlitePool
```

### 11.3 Single-writer enforcement

The unified outbox follows a single-writer rule per event type (see [outbox-consolidation.md](outbox-consolidation.md) §2):

- **Sync push/pull commands**: written exclusively by `nexus-cloud-sync::outbox::Outbox` (`append`, `stage`, `stage_if_absent`).
- **Flush/compact operations**: written exclusively by `nexus-orchestration` capability layer (`OutboxFlush`, `OutboxCompact`).
- **Daemon runtime**: does NOT write to `outbox_entries` directly. All outbox access is routed through the capability registry.

The daemon legacy `outbox` table (initial migration `20260417_000001_initial.sql`) is deprecated with zero active consumers (V1.59 T3 audit). The daemon-runtime schema test emits a `tracing::warn!` on access documenting the phased-removal plan.

### 11.4 Runtime deps injection

Both capabilities receive the `sqlx::SqlitePool` through the standard `with_pool()` constructor pattern, registered in `CapabilityRegistry::with_builtins_and_pool()` and `CapabilityRegistry::with_runtime_deps()`. No capability requires `nexus-cloud-sync` — all outbox operations are local-only DB queries.
