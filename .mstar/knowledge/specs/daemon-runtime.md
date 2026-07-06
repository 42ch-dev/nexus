# Nexus Daemon Runtime Architecture

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.65 Prepare amendment (bundled local Web UI serving + chapter-content Local API route family); **V1.66 Phase 2b amendment** (§12: Tauri sidecar mode launch/readiness/lifecycle contract); **V1.86 amendment** (§13: Local API trust-boundary security — Origin allowlist, deny-fs-without-workspace, component-wise path guard); **V1.90 amendment** (§14: Daemon API remote bind gate; normative surface renaming from Local API to Daemon API with `/v1/daemon/` path prefix); **V1.92 amendment** (§15–16: transport security (TLS) + remote client connection model) |
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
│    ├─ loopback Daemon API (/v1/daemon/*) — local product only
│    └─ AgentHostSubsystem → nexus-agent-host (see agent-host)
  └─ nexus-cloud-sync (CLI-only; platform HTTP + optional legacy-sync)
```

Platform sync and registration **must not** live in daemon-runtime. See [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md).

**Rules**:

1. Only **`nexus42`** is a user-facing executable artifact.
2. **Daemon** is started via CLI (`nexus42 daemon start`, foreground or background); background mode may use a hidden internal entry (implementation detail in knowledge SSOT).
3. **Daemon API** remains loopback HTTP and/or Unix socket; clients must not assume a separate daemon product binary.

---

## 3. Subsystem responsibilities

| Subsystem | Owns | Does not own |
| --- | --- | --- |
| CLI | Parsing, one-shot commands, spawning daemon mode, user errors | Long-lived agent protocol details |
| Daemon runtime | `SQLite` handles, Daemon API listener, orchestration/agent-host, workspace session persistence (`workspace_sessions` DB table, V1.56 P0), graceful shutdown | Platform HTTP, sync outbox, creator registration |
| Agent host | Managed agent sessions (see agent-host) | Platform HTTP |
| Cloud sync (CLI) | Platform HTTP, legacy bundle sync (`nexus-cloud-sync`) | Daemon API |

---

## 4. Process model

### 4.1 Foreground

`nexus42 daemon start --foreground` runs the runtime in the current process until shutdown.

### 4.2 Background

Default `nexus42 daemon start`: preflight → spawn internal daemon-run mode → parent exits after startup gate. **Semantics** are normative; exact argv names are implementation SSOT.

### 4.3 Control plane

`status`, `stop`, `restart` coordinate via runtime health and process supervision (parity with prior daemon product behavior).

### 4.4 Bundled local Web UI static assets (V1.64)

The daemon runtime may serve the bundled local Web UI SPA from the same loopback listener as the Daemon API. The Web UI is a local product surface, not a cloud platform application.

Normative serving model:

1. **Release build**: `apps/web/dist` is embedded into the user-facing `nexus42` binary using `rust-embed` and exposed by the daemon router through `tower-http::ServeDir`-style static serving semantics. The release artifact remains a single `nexus42` product binary; no separate web server process is introduced.
2. **SPA shell route**: the static Web UI shell (`index.html` plus assets) is unauthenticated so a local browser can load the app and present setup/auth guidance. This does not grant data access.
3. **Data boundary**: all `/v1/daemon/*` data routes remain protected according to the existing `require_api_key` model except the explicitly unguarded runtime/daemon health and status routes listed in §2/§4 acceptance. The SPA obtains data only through those Daemon API routes.
4. **Dev mode**: during frontend development, Vite serves `apps/web` and proxies `/v1/daemon/*` to a running daemon. Dev proxy behavior is a development convenience only; release behavior is daemon-served embedded static assets.
5. **Tauri readiness**: the future Tauri shell loads the same `apps/web` build output and swaps the frontend transport implementation behind the `NexusClient` boundary. The daemon runtime remains the local supervisor and is still not an ACP Agent/Server.

The router integration point is the top-level `create_router` composition in `crates/nexus-daemon-runtime/src/api/mod.rs`: static serving is added beside the unguarded runtime routes and protected Daemon API route tree, without moving the auth middleware boundary for data endpoints.

#### 4.4.1 Embed implementation (V1.64 P3)

The static assets are embedded via a `#[derive(RustEmbed)]` struct in `crates/nexus-daemon-runtime/src/static_assets.rs`:

```rust
#[derive(RustEmbed)]
#[folder = "../../apps/web/dist"]
pub struct WebAssets;
```

The struct is placed in `nexus-daemon-runtime` (not `nexus42`) because the daemon runtime library owns the axum router. The `rust-embed` macro reads `apps/web/dist` relative to the crate's `Cargo.toml` (i.e., `<repo_root>/apps/web/dist`) and triggers a rebuild when the dist changes.

#### 4.4.2 Router mount

The SPA handler `serve_embedded_app` is mounted as the top-level `Router::fallback()` inside `create_router()` — added BEFORE merging the API routes. This means explicit `/v1/daemon/*` routes (network routes + protected routes) take priority over the catch-all SPA fallback.

**Route resolution order:**
1. Unguarded runtime routes (`/v1/daemon/runtime/health`, etc.)
2. Protected Daemon API routes (`/v1/daemon/works`, etc., behind `require_api_key`)
3. SPA fallback (serves `index.html` for unmatched `GET`/`HEAD` requests)

Non-`GET`/`HEAD` requests hitting the fallback return `405 Method Not Allowed`.

#### 4.4.3 Cache headers

| Path pattern | `Cache-Control` | Rationale |
|---|---|---|
| `/assets/*` (hashed Vite output) | `public, max-age=31536000, immutable` | Content-hashed filenames guarantee cache-busting |
| `index.html` (SPA entry point) | `no-cache` | Must always revalidate so new deploys are picked up |

#### 4.4.4 Release build sequence

1. `pnpm --filter web build` — produces `apps/web/dist/` (Vite + TypeScript)
2. `cargo build --release -p nexus42` — `rust-embed` macro reads dist at compile time

The dist is NOT committed to git (`apps/web/dist/` is gitignored per the Vite scaffold). The release CI pipeline must run step 1 before step 2. A stale or missing dist at build time is a compile error (the `#[folder]` path must exist).

#### 4.4.5 CLI URL logging

On startup (both foreground and background modes), the daemon logs the Web UI URL alongside the Daemon API base URL:

- **Foreground** (`boot.rs`): `tracing::info!("Web UI available at http://{}", addr);`
- **Background** (`nexus42 daemon start` stdout):
  ```
  ✓ Daemon started successfully on port 8420
    PID: 12345
    Daemon API: http://127.0.0.1:8420
    Web UI:    http://127.0.0.1:8420/
  ```

A convenience command `nexus42 daemon ui` (alias `nexus42 daemon web`) starts the daemon in background if not already running and opens the OS default browser via `open` (macOS) / `xdg-open` (Linux) / `start` (Windows).

### 4.5 Chapter-content Local API routes (V1.65)

The daemon runtime owns the chapter-content route family consumed by the bundled
Web UI authoring surface:

```text
/v1/daemon/works/{work_id}/chapters
/v1/daemon/works/{work_id}/chapters/{n}
/v1/daemon/works/{work_id}/chapters/{n}/outline
/v1/daemon/works/{work_id}/chapters/{n}/body
```

Route responsibilities:

1. List/detail chapter metadata from `work_chapters` using the Daemon API
   `items` + cursor convention for the list route.
2. Read and atomically replace outline markdown at DB-sourced `outline_path`.
3. PATCH chapter structure metadata (`title` if supported, `slug`,
   `planned_word_count`, `volume`, and explicit status progression).
4. Read body markdown from DB-sourced `body_path` only; V1.65 introduces no body
   write route.

All outline/body file routes MUST apply the same W-002-style workspace path
guard used by host-tool body reads before reading or writing. Outline PUT uses a
sibling temp file + flush + atomic rename and updates `outline_path`/`updated_at`
through the same finalization path. The body writer remains the orchestration
host-tool path — the AI owns prose writing; there is no manual body editor
(the body-editor direction was rejected 2026-06-26; see
[canvas-strategy-surface.md](canvas-strategy-surface.md)). If a future canvas
surface flushes structured node-edits to chapter files, the write coordination
(no-raw-file-editing principle; structured/node-granular operations) will be
designed in the V1.68 canvas context; until then orchestration remains the
sole body writer and the host-tool path is unchanged.

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

---

## 12. Tauri sidecar mode (V1.66)

The Tauri desktop shell ([desktop-shell.md](desktop-shell.md)) may bundle the user-facing `nexus42` binary as a sidecar process. This does **not** create a second daemon product binary: the sidecar is still `nexus42`, launched in daemon foreground mode by the desktop app. (Compass: [v1.66 §5 #2/#3 LOCKED](../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md).)

### 12.1 Launch contract

The desktop app launches the sidecar with:

```text
nexus42 daemon start --foreground --port <resolved_port>
```

Optional flags such as `--cdn-url <https-url>` may be passed only when the desktop app has an explicit configuration source for them. **V1.66 does not add a new daemon-lifecycle Daemon API route** (`wire_contracts_changed: false`).

**Port resolution** (compass §5 #3 LOCKED; conventions in [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) §9):

1. Explicit configured port (if the Tauri bootstrapper provides one).
2. Else `NEXUS_DAEMON_PORT` when present and valid.
3. Else `8420` (the `boot.rs` default).

When an override is selected, the app passes it via `--port <resolved_port>` so CLI args and environment cannot diverge.

### 12.2 Readiness contract

The sidecar readiness signal is the existing Daemon API health probe, **not** stdout parsing:

```text
GET http://127.0.0.1:<resolved_port>/v1/daemon/runtime/health
```

The desktop app treats the daemon as ready only after the health endpoint returns a successful healthy response. Startup logs such as `Daemon API listening on …` are diagnostic only and MUST NOT be the app's readiness contract.

**Recommended bootstrap behavior**:

1. Spawn sidecar in foreground mode.
2. Poll health with bounded retry/backoff.
3. Until healthy, render desktop state `Daemon starting…`.
4. On timeout, render `Daemon did not start` with `Restart Daemon` and `Copy Diagnostics`.
5. If the port is already occupied by a healthy Nexus daemon, the app may attach to it after confirming health on the resolved port.

### 12.3 Lifecycle contract

The Tauri app owns the sidecar process while the desktop window/session is alive:

| Event | Behavior |
| --- | --- |
| App launch | Start sidecar unless a healthy daemon already responds on the resolved port |
| App ready | Expose the resolved daemon base URL to the SPA client factory |
| Sidecar crash after healthy | Restart with bounded exponential backoff |
| Repeated crash | Stop retrying; show `Daemon stopped` + diagnostics |
| App quit | Request graceful termination of the owned sidecar; escalate only after bounded timeout |
| Manual restart | Stop the owned sidecar (if present) → spawn fresh → wait for health |

**Process ownership**: foreground sidecar mode may still write the daemon PID file as part of existing CLI behavior. The desktop app must track the process handle returned by the Tauri sidecar API and prefer that handle for ownership decisions. PID-file or port-based stop paths are CLI-compat mechanisms and must not be used to kill an unrelated daemon without confirming ownership.

### 12.4 Asset serving in desktop mode

In desktop mode, Tauri serves the bundled `apps/web/dist` via `build.frontendDist` (compass §5 #4 LOCKED). The daemon's rust-embed static asset route remains normative for the browser-tab flow and standalone `nexus42 daemon ui`, but it is **not** the desktop shell's asset-serving path.

---

## 13. Local API Trust-Boundary Security (V1.86)

> **V1.90 note:** The surface was renamed to **Daemon API** and the path prefix to `/v1/daemon/*` in V1.90. The security rules described below apply unchanged to the renamed surface. References to "Local API" in this section title and in V1.86 iteration names are historical only.

This section codifies the normative security contract for the daemon's Daemon API trust boundary. It closes the three-link attack chain identified in V1.86 (permissive CORS + keyless-localhost → remote-reach; fs/* bypass without workspace → arbitrary-file R/W; string-prefix path comparison → sibling-directory escape). The normative hooks in §4.4.3 (`require_api_key` on data routes) and §4.5 (W-002-style workspace path guard) already provide authority; this section adds the Origin gate, the deny-fs-without-workspace invariant, and the component-wise path guard requirement.

**Coordinates with:** the V1.86 delivery compass ([v1.86-local-api-trust-hardening-delivery-compass-v1.md](../../iterations/v1.86-local-api-trust-hardening-delivery-compass-v1.md)), `api/path_guard.rs` (`resolve_guarded_path`), `api/auth_middleware.rs` (keyless-localhost mode), `api/mod.rs` (CORS layer configuration).

### 13.1 Origin allowlist gate

The daemon's CORS configuration is the primary browser-origin trust boundary. Per [STRATEGY.md](../../../STRATEGY.md) Guiding Principle #1 ("Local-first privacy"), cross-origin access from arbitrary websites MUST be denied by default.

#### 13.1.1 Allowlist composition

The daemon derives its allowed origins at startup from the following sources (no explicit configuration required for standard setups):

| Origin | Source | Rationale |
|--------|--------|-----------|
| `http://127.0.0.1:<port>` | Computed from the resolved daemon port (default 8420, or `NEXUS_DAEMON_PORT`) | Own listening origin — the browser SPA served by the daemon or accessed directly via `nexus42 daemon ui` |
| `tauri://localhost` | Hardcoded | Tauri v2 macOS custom protocol webview origin |
| `http://tauri.localhost` | Hardcoded | Tauri v2 Windows/Linux webview origin |
| `http://localhost:5173` | Hardcoded | Vite dev-server origin (`pnpm dev` frontend development proxy) |
| (any) | `NEXUS_DAEMON_ALLOWED_ORIGINS` env var (comma-separated list) | Escape hatch for reverse-proxy setups, custom hostnames, and corporate proxy environments |

The Vite dev origin (`http://localhost:5173`) is allowed unconditionally because the dev proxy is a development convenience operated by the same local user; it does not weaken the remote-attack surface since the dev flow requires the user to explicitly run the Vite server.

**Design invariant:** the allowlist is derived from codebase-verified client origins (not guessed). The Tauri webview origins match the Tauri v2 protocol configuration in `tauri.conf.json`; the Vite origin matches `vite.config.ts`; the own-origin is computed from the resolved port at startup.

#### 13.1.2 Request handling

| Condition | Outcome |
|-----------|---------|
| Request carries no `Origin` header | **Permitted** — non-browser clients (CLI `host-call`, `curl`, worker IPC, direct browser tab navigation to the daemon's own URL at `http://127.0.0.1:<port>`) do not send an `Origin` header. Same-origin browser requests also omit `Origin`. |
| `Origin` header value is in the allowlist | **Permitted** — the request proceeds to auth middleware (§4.4.3) and the handler |
| `Origin` header value is NOT in the allowlist | **Rejected** — `403 Forbidden` with a clear error message including the rejected origin value and a reference to `NEXUS_DAEMON_ALLOWED_ORIGINS` as the documented escape hatch |

#### 13.1.3 Defense-in-depth layering

The Origin gate uses two independent mechanisms:

1. **Configured `CorsLayer`** (tower-http): replaces the pre-V1.86 `CorsLayer::permissive()`. Handles CORS preflight (`OPTIONS`) requests correctly with the explicit allowlist. This is the primary CORS-compliant browser gate.

2. **Origin-reject middleware** (axum, applied as a separate tower layer): performs a second hard check on every non-preflight request carrying an `Origin` header. This is defense-in-depth — if the `CorsLayer` configuration were ever accidentally relaxed, the middleware still enforces the allowlist.

Both layers derive their allowlist from the same configuration source. The middleware allows `OPTIONS` preflight requests through unconditionally (the `CorsLayer` is authoritative for preflight; double-rejecting preflight breaks CORS entirely).

#### 13.1.4 Relationship to authentication

The Origin gate is **independent of** and **applied before** the auth middleware (§4.4.3). The keyless-localhost mode (`NEXUS42_DAEMON_API_KEY` unset) remains the default (deprecation is a non-goal; see V1.86 compass §1). Before V1.86, a cross-origin browser request to `http://127.0.0.1:8420` passed both permissive CORS (all origins allowed) AND keyless-localhost auth (TCP connection is loopback). After V1.86, the Origin gate rejects the cross-origin request at the first layer — the auth middleware is never reached — because the malicious site's `Origin` (e.g., `https://evil.com`) is not in the allowlist.

Non-browser clients (CLI, workers, `curl`) do not send an `Origin` header and pass the Origin gate, then proceed through auth as before.

#### 13.1.5 Observability

When keyless-localhost mode is active, the daemon MUST log the effective Origin allowlist at `INFO` level on startup (or on first protected request), so the user can inspect which origins are trusted. The log format includes each origin and its source (computed, hardcoded, or env override).

### 13.2 Deny fs/* tools without active workspace

When no active workspace is configured — i.e., `WorkspaceState::workspace_path()` returns `None` — all `fs/*` host tools (`fs/read_text_file`, `fs/write_text_file`) MUST be denied **unconditionally** during the admission pipeline, before the tool executor runs.

**Error contract:** the denial returns `403 Forbidden` with a clear, actionable message:
```
fs/* tools require an active workspace with defined bounds
```

**Rationale:** the fs/* path guard (§13.3, §4.5 W-002) requires a workspace root to enforce the containment boundary. Without a workspace root there is no boundary to enforce and any filesystem path would pass. Deny-by-default is the safe primitive; a sandbox-dir fallback is YAGNI.

**Caller audit:** all three host-tool caller entry points (CLI `host-call`, worker `agent_tool_request` IPC, schedule executor) require an active workspace context for legitimate fs/* usage. No legitimate no-workspace fs/* invocation path exists in the current architecture. This invariant is verified by grepping all `HostToolExecutor` call sites at the time of the fix and documented here so future callers respect it.

**Implementation contract:** the denial is in `admission_pipeline()` (`api/handlers/host_tool_handlers.rs`), before `execute_read_file` / `execute_write_file` run. The admission check is:
```rust
if state.workspace_path().is_none() {
    return Err(/* 403: fs/* tools require an active workspace */);
}
```

This closes the trust-boundary bypass where privileged fs/* tools could read or write arbitrary user files (`~/.nexus42/auth.json`, `~/.ssh/id_rsa`, etc.) when no workspace was configured.

### 13.3 Component-wise path guard for fs/* tools

All path validation for `fs/*` tools MUST use **component-wise** `Path::starts_with` comparison after canonicalization — never string-prefix comparison.

#### 13.3.1 The anti-pattern

String-prefix comparison (`path_str.starts_with(&workspace_str)`) is a path-traversal vulnerability. A workspace root of `/home/user/my-novel` would accept `/home/user/my-novel-evil/secret.md` because the string `/home/user/my-novel-evil/secret.md` starts with `/home/user/my-novel`.

#### 13.3.2 Normative requirement

`validate_file_path` (`host_tool_handlers.rs`) MUST delegate to the canonical `resolve_guarded_path` helper (`api/path_guard.rs`), which already implements the correct pattern for both branches:

- **Existing files** (read paths): `canonicalize(requested_path).starts_with(canonicalize(workspace_root))`
- **Write targets** (possibly non-existent): walk up to nearest existing parent, `canonicalize(parent).starts_with(canonicalize(workspace_root))`

The `resolve_guarded_path` implementation at `path_guard.rs:35-100` is the single source of truth for the component-wise guard. Its documentation at lines 60-62 explicitly calls out the string-prefix anti-pattern and why `Path::starts_with` is the correct replacement.

#### 13.3.3 Alignment with §4.5 W-002

The chapter-content routes (§4.5) already delegate to `resolve_guarded_path` for outline and body file paths. The W-002 hook ("any file path resolved from a user-supplied or DB-stored relative path must remain inside the active workspace root") is authoritative for all filesystem-accessing routes. V1.86 aligns the host-tool `fs/*` path validation with the same canonical helper, eliminating the duplicated (and vulnerable) string-prefix logic.

#### 13.3.4 TOCTOU note

The canonicalization race window between reading the workspace root and checking the target path is documented in `resolve_guarded_path` (lines 22-28) and tracked by residual `R-V166-QC2-TOCTOU`. The single-user local daemon context bounds the practical risk. The component-wise `Path::starts_with` fix does not introduce a new TOCTOU window; it replaces an already-racy-but-incorrect check with an already-racy-but-correct one.

#### 13.3.5 Coverage requirement

Both the read (existing-file) and write (non-existing-file) branches MUST be covered by automated regression tests that verify:
1. An in-workspace path is accepted by both branches
2. A sibling-directory prefix-escape path (e.g., workspace `/home/user/my-novel`, target `/home/user/my-novel-evil/foo`) is rejected by both branches
3. A parent-directory escape (`../`) is rejected by both branches

---

## 14. Daemon API Remote Bind Gate (V1.90)

This section codifies the security contract for optional non-loopback binding of the Daemon API listener. The Daemon API is local-first by default; remote access is opt-in only and subject to a two-condition gate.

### 14.1 Default: loopback only

The daemon binds to loopback (`127.0.0.1`) by default. This requires no additional configuration. The default behavior provides a local-first experience where the Daemon API is reachable only from the same machine (browser SPA, CLI, Tauri desktop shell).

### 14.2 Opt-in: non-loopback bind

Binding the daemon to a non-loopback address (any interface other than `127.0.0.1` or `::1`) is rejected unless **all three** of the following conditions are met:

| Condition | Env var / Mechanism | Effect |
|---|---|---|
| API key is set | `NEXUS42_DAEMON_API_KEY` | The daemon must have an explicit API key configured. Keyless-localhost mode is insufficient for remote access. |
| Remote bind is explicitly enabled | `NEXUS_DAEMON_REMOTE_BIND=1` | The author must explicitly opt in to non-loopback binding. |
| TLS certificate exists | auto-generated on boot (see §15.1) | The daemon must have a valid TLS certificate loaded or generated. Non-loopback bind without TLS is a fail-closed condition. |

If any condition is absent, the daemon **refuses to bind** to a non-loopback address and logs a clear error message including which condition(s) are missing and a reference to this section.

When all three conditions are met, the daemon binds via TLS (`axum_server::bind_rustls`, see §15.1) and requires the API key on all protected routes.

When both conditions are met, the daemon binds to the configured address and requires the API key on all protected routes (the keyless-localhost shortcut is disabled for non-loopback binds, even if the TCP connection originates from the same machine).

### 14.3 CORS and origin allowlist

The existing CORS configuration and origin-allowlist behavior codified in §13.1 (V1.86) continues to apply regardless of the bind address. When the daemon is bound to a non-loopback address:

- The `CorsLayer` and origin-reject middleware described in §13.1.3 use the same allowlist sources (§13.1.1).
- The own-origin entry (`http://127.0.0.1:<port>`) is still computed from the resolved port; additional origins must be added via `NEXUS_DAEMON_ALLOWED_ORIGINS` if the browser SPA is served from a different host.
- No origin is added automatically for the non-loopback bind address — the author controls the allowlist explicitly.

### 14.4 Relationship to authentication

When bound to a non-loopback address, all protected Daemon API routes require a valid `X-API-Key` header or `Authorization: Bearer <key>`. The keyless-localhost shortcut (`NEXUS42_DAEMON_API_KEY` unset) is **only** active on loopback binds; it is disabled when the listener is on a non-loopback interface, regardless of `NEXUS_DAEMON_REMOTE_BIND`.

### 14.5 Non-goals

- **New auth model** — remote access reuses the existing `NEXUS42_DAEMON_API_KEY`.
- **Dynamic port allocation** — the bind port is configured via `NEXUS_DAEMON_PORT` (default 8420) regardless of bind address.
- **Multi-user or tenant isolation** — the daemon is single-creator; the security gate protects the single author's local data, not multi-tenant isolation.

### 14.6 Verification

- `remote_bind_rejected_without_key`: non-loopback bind with `NEXUS42_DAEMON_API_KEY` unset MUST fail at boot with a clear error.
- `remote_bind_rejected_without_flag`: non-loopback bind with `NEXUS42_DAEMON_API_KEY` set but `NEXUS_DAEMON_REMOTE_BIND` unset/not `1` MUST fail at boot.
- `remote_bind_rejected_without_tls`: non-loopback bind with key and flag set but no TLS cert loaded/generated MUST fail. Error message references §15.1.
- `remote_bind_allowed_with_all_three`: non-loopback bind with key + flag + TLS cert MUST succeed on a TLS listener; protected routes require the API key.
- `loopback_bind_allowed_without_flag`: loopback bind MUST succeed without any of the three conditions (existing default behavior unchanged).

---

## 15. Transport Security — TLS (V1.92)

This section codifies the built-in TLS listener for non-loopback daemon binds. Before V1.92, remote TLS was a §14.5 non-goal (reverse proxy only); V1.92 replaces that non-goal with auto-generated self-signed TLS certificates managed by the daemon itself. The reverse-proxy path remains available but is no longer the only way to secure a remote daemon connection.

### 15.1 TLS listener

The daemon auto-generates an Ed25519 self-signed TLS certificate on first non-loopback boot and persists it for reuse across restarts. The TLS listener uses `axum-server` + `rustls` as a drop-in replacement for `tokio::net::TcpListener::bind` at the `axum::serve` call site.

**Dependencies** (added to `nexus-daemon-runtime`):

| Crate | Version | Role |
|-------|---------|------|
| `axum-server` | `0.7` | `RustlsConfig::from_pem()` + `bind_rustls()` → drop-in `Listener` |
| `rustls` | `0.23` | TLS protocol; transitive via `axum-server` |
| `rcgen` | `0.13` | Ed25519 self-signed cert generation (`KeyPair::generate_for(&PKCS_ED25519)`) |
| `rustls-pemfile` | `2` | PEM cert/key loading for boot-time reload of persisted certs |

**Cert algorithm:** Ed25519. Rationale: smaller public keys (32 bytes vs 65 for ECDSA P-256), faster signing, no curve negotiation drama for a single self-signed cert, `rcgen 0.13` supports `PKCS_ED25519` out of the box.

**Cert storage** (via `nexus-home-layout`):

| Function | Path |
|----------|------|
| `tls_dir(home)` | `~/.nexus42/tls/` |
| `tls_cert_path(home)` | `~/.nexus42/tls/cert.pem` |
| `tls_key_path(home)` | `~/.nexus42/tls/key.pem` |

Permissions: daemon creates `~/.nexus42/tls/` with mode `0o700` on first boot; writes `cert.pem` + `key.pem` with `0o600` (owner-only read).

**Listener selection logic** (P0 implementation outline):

1. If the bind address is loopback → plain `tokio::net::TcpListener::bind(&addr)` (existing behavior, unchanged).
2. If the bind address is non-loopback → load or generate TLS cert → `axum_server::bind_rustls(addr, config)`.
3. The rest of the `axum::serve(listener, app).with_graceful_shutdown(...)` block is unchanged — `bind_rustls` returns a type that satisfies `tokio::Listener`.

**Subject Alternative Name (SAN) generation policy:**

1. The generated certificate **always** includes loopback SANs: `127.0.0.1`, `::1`, and `localhost`. These are required so local clients and loopback-first generation always work.
2. For a non-loopback concrete bind host, the certificate also includes the bind host as an IP SAN if it parses as an IPv4/IPv6 address, or as a DNS SAN otherwise.
3. Wildcard bind addresses (`0.0.0.0` and `::`) are **not** added as SANs because they are not valid server names for TLS hostname validation.

**Cert lifecycle:**

1. **Boot:** check if `~/.nexus42/tls/cert.pem` + `key.pem` exist → load via `rustls-pemfile` → `RustlsConfig::from_pem`. If not → generate via `rcgen` (Ed25519 `KeyPair`, `CertificateParams` with `CommonName = "nexus42-daemon"`, `self_signed`) → persist → load.
2. **Reuse:** cert is loaded from disk on every boot; same cert survives daemon restarts **as long as its SAN list covers the current `bind_host`**.
3. **Regeneration:**
   - Explicit user action: delete `~/.nexus42/tls/` → next boot regenerates.
   - Automatic on bind-host mismatch: if the persisted cert's SAN list does **not** cover the current `bind_host`, the daemon regenerates the cert with SANs for the new bind host and logs an `INFO` message stating that the cert was regenerated because the bind host changed. No automatic rotation or expiry check is performed otherwise (self-signed local trust anchor; expiry is informational).
4. **Startup log:** `tracing::info!("TLS certificate fingerprint: SHA256:aa:bb:cc:...")` — fingerprint logged once at boot for the author to copy.

### 15.2 Remote-bind gate with TLS (§14.2 amended)

The remote-bind gate (§14.2) now requires **three** conditions (was two before V1.92):

1. API key set (`NEXUS42_DAEMON_API_KEY`)
2. Remote bind explicitly enabled (`NEXUS_DAEMON_REMOTE_BIND=1`)
3. TLS certificate exists (auto-generated at boot per §15.1)

The gate is **fail-closed**: if a non-loopback bind is requested but no TLS cert can be loaded or generated (e.g., filesystem permission error, crypto provider failure), the daemon refuses to start with a clear error message. The error format is:

```
Failed to start non-loopback listener: TLS certificate is required for remote access but could not be loaded or generated.
  Error: <os error message>
  See daemon-runtime.md §15.2 for the remote-bind gate policy.
  To use the daemon locally, pass --host 127.0.0.1 (default).
```

**Relationship to auth middleware:** when a TLS listener is active, the keyless-localhost shortcut (§14.4) is disabled on all connections — the API key is required even if the TCP connection originates from the same machine, because loopback-only detection at the connection level is not reliable over TLS.

### 15.3 Certificate fingerprint

The fingerprint is a **public trust anchor** (like an SSH host key) — it is what clients pin, not a secret. It is computed once at cert generation/boot and cached in memory.

**Computation:** SHA-256 of the **DER-encoded** certificate (not PEM, not the public key alone). The digest is formatted as colon-separated lowercase hex with a `SHA256:` prefix.

Example: `SHA256:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99`

**Observability:**
- Logged at `INFO` level at startup (once per boot).
- Served by the read-only, unauthenticated `GET /v1/daemon/runtime/cert-fingerprint` endpoint (see §15.4).
- The endpoint is in the same unguarded runtime route group as `/v1/daemon/runtime/health` and `/v1/daemon/runtime/status`. No auth required — the fingerprint is public by design.

**Security note:** the fingerprint is not a secret. An attacker who can observe the fingerprint over an insecure channel (e.g., the local network before the TLS connection is established) cannot use it to break the TLS handshake — they still do not possess the private key. This is the same trust model as SSH host keys.

### 15.4 Fingerprint endpoint

**Path:** `GET /v1/daemon/runtime/cert-fingerprint`  
**Placement:** `runtime_routes` in `api/mod.rs` — same unguarded route group as `/v1/daemon/runtime/health` and `/v1/daemon/runtime/status`.  
**Auth:** none (fingerprint is a public trust anchor — §15.3).  
**HTTP method:** `GET` only (read-only, no mutation, no side effects).

**Response contract** (JSON Schema: `schemas/daemon-api/runtime/cert-fingerprint-response.schema.json`):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `fingerprint` | string | yes | SHA-256 fingerprint in colon-hex with `SHA256:` prefix. Empty string `""` when the daemon has no TLS cert (loopback-only mode). |
| `algorithm` | string enum `"sha256"` | yes | Always `"sha256"`. Stable enum so clients can detect algorithm upgrades before parsing fingerprint value. |
| `created_at` | string (ISO 8601) | no | Timestamp of cert generation/boot. Present when cert exists; absent when loopback-only. |

**Behaviour when no cert exists (loopback-only daemon):** the endpoint returns HTTP 200 with `{ "fingerprint": "", "algorithm": "sha256" }` (no `created_at`). An empty fingerprint is the canonical signal that this daemon is not configured for TLS. This avoids a confusing 404/500 for what is a valid daemon state and allows the client setup screen to distinguish "daemon has no TLS cert → it must be local loopback" from "endpoint is unreachable."

**Handler implementation note:** the handler reads the fingerprint from a shared `Arc<Option<TlsFingerprint>>` injected into `WorkspaceState` at boot — no filesystem I/O per request. The fingerprint string is computed once at cert generation/boot and cached.

---

## 16. Remote Client Connection Model (V1.92)

This section codifies the client-side contract for connecting to a remote daemon. It is authoritative for both the web SPA (BrowserClient) and the Tauri desktop shell (TauriClient). The local same-origin mode is the backwards-compatible default; remote access is opt-in per the setup-screen flow.

### 16.1 Client transport parameterisation

The `BrowserClient` and `TauriClient` accept a base URL plus an optional `X-API-Key` header value. In local same-origin mode, the base URL is the current page's origin and no API key header is sent (keyless-localhost shortcut — §14.4). In remote mode, the base URL is the configured remote daemon endpoint and the API key header is sent on every protected request.

**Connection config shape** (client-side storage only — NOT a wire contract):

```json
{
  "endpointUrl": "https://192.168.1.42:8420",
  "apiKey": "<user-entered key>",
  "pinnedFingerprint": "SHA256:aa:bb:cc:...",
  "label": "Home server"
}
```

| Field | Required | Storage | Description |
|-------|----------|---------|-------------|
| `endpointUrl` | yes | localStorage / OS keychain | Full URL including `https://` and port. |
| `apiKey` | yes | localStorage / OS keychain | The value of `NEXUS42_DAEMON_API_KEY` on the daemon machine. |
| `pinnedFingerprint` | no | localStorage / OS keychain | The SHA-256 fingerprint pinned after TOFU confirmation (§16.2). Absent until the user explicitly trusts a certificate. |
| `label` | no | localStorage / OS keychain | User-visible connection name (e.g. "Home server"). Defaults to hostname if blank. |

The config is stored client-side only — it is never sent to the daemon as a wire payload and is not part of any Daemon API request/response schema. The daemon is unaware of the client's pinned fingerprint, label, or connection history.

### 16.2 TOFU trust model

The remote connection trust model is **Trust On First Use (TOFU)**, analogous to SSH host key pinning. The flow has three distinct phases:

**Phase 1 — First connection (no pinned fingerprint):**

1. User enters the endpoint URL in the setup screen.
2. Client fetches `GET /v1/daemon/runtime/cert-fingerprint` (no auth, no TLS verification on this fetch — the fingerprint is the verification).
3. Client displays the fingerprint in monospace with the security note: "This fingerprint is how your app makes sure it is talking to the real daemon and not someone pretending to be it. Compare it to the value printed on the daemon machine's screen. If they match, it is safe to trust."
4. User explicitly clicks "Trust this certificate and connect" — the fingerprint is pinned to the endpoint URL in client storage.
5. Client establishes the TLS connection with the pinned fingerprint as the expected cert.

**Phase 2 — Subsequent connection (pinned fingerprint matches):**

1. Client loads pinned fingerprint from storage.
2. Client establishes TLS connection and verifies the served certificate's SHA-256 fingerprint matches the pinned value.
3. No user interaction required — the pin is silently verified.

**Phase 3 — Fingerprint changed (pinned fingerprint mismatch):**

This is the highest-stakes security moment. The client MUST block and display an explicit warning:

1. Client detects that the served fingerprint does not match the pinned fingerprint.
2. Client displays a warning: "The certificate for this daemon has changed. This can happen if the daemon was reinstalled or its certificate was deliberately rotated. It can also mean someone is intercepting your connection."
3. Two explicit options are presented:
   - "Trust the new certificate and continue" — records the new fingerprint as the pinned value, replacing the old one.
   - "Cancel and keep using the old certificate (safer if you did not expect this change)" — aborts the connection, keeps the old pinned value.
4. Nothing proceeds until the user explicitly chooses. The client does NOT auto-accept the new fingerprint.

### 16.3 CSRF defence by header-key (normative rationale)

The remote connection model authenticates via a custom `X-API-Key` request header. Cross-origin JavaScript cannot set a custom header without triggering a CORS preflight, which the §13.1 Origin allowlist already gates. A separate CSRF token framework (double-submit cookie, synchronizer token, SameSite session cookie) is therefore **redundant** — it would add maintenance surface without closing an attack vector that the header-key + Origin allowlist combination already closes.

This rationale is codified here so that a future agent or contributor does not "add" a CSRF token framework thinking it is missing. The non-goal is deliberate and traceable to this section. Any future proposal to add CSRF tokens should engage with this rationale explicitly (rather than layering tokens on top silently), so reviewers can weigh the added surface against a concrete new threat.

### 16.4 Origin-allowlist evolution for remote clients

The daemon's Origin allowlist (§13.1) already covers: own-origin, Tauri webview origins, Vite dev origin, and `NEXUS_DAEMON_ALLOWED_ORIGINS` escape hatch. When a remote web-app or desktop-app client connects from a non-loopback origin:

- The connecting client's origin MUST be added to `NEXUS_DAEMON_ALLOWED_ORIGINS` — there is **no magic auto-allowlisting** of remote origins.
- The daemon does not automatically trust the remote bind address as a browser origin; the author controls the allowlist explicitly.
- A remote client that sends an `Origin` header not in the allowlist will receive `403 Forbidden` (consistent with §13.1.2), regardless of whether it holds a valid API key or a pinned TLS fingerprint.
- Tauri webview origins (`tauri://localhost`, `http://tauri.localhost`) are already hardcoded in the allowlist (§13.1.1); a remote desktop app using Tauri will use those same origins and be automatically allowed.
- A remote web-app (browser SPA connecting to a remote daemon) needs its serving origin in `NEXUS_DAEMON_ALLOWED_ORIGINS`.

### 16.5 Client key storage

| Platform | Storage mechanism | Notes |
|----------|-------------------|-------|
| Web SPA | `localStorage` | SPA trust boundary equal to the app itself. Key is always user-entered, never compiled in. |
| Tauri desktop | OS keychain (Tauri secure-store plugin) where available; fallback to app-data dir | Keychain is the preferred secure storage; fallback is a local-first trade-off for platforms without OS keychain support. |

The API key is always **user-entered** — never compiled into the binary, never stored in version control, never embedded in build artifacts. Full secret-store hardening (hardware-backed keystore, biometric unlock) is a future concern.

### 16.6 Raw-browser-tab remote navigation (explicit non-goal)

A remote daemon URL typed directly into a browser tab will encounter a self-signed certificate warning — the browser's built-in CA trust store does not recognise the daemon's self-signed cert. This is by design: the daemon is not intended for direct browser-tab navigation.

The supported remote-access path is always through the app's "Connect to Daemon" setup screen, which performs TOFU fingerprint confirmation (§16.2) and manages the pinned certificate. This non-goal is codified so it is not treated as a bug to be "fixed" by removing TLS or requiring a public CA.
