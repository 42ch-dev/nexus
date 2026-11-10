---
module: apps/nexus-service
date: 2026-07-16
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - daemon
  - creator-pool
  - bootstrap
  - setup-wizard
  - lazy-attach
  - electron
  - first-run
applies_when:
  - "Editing the Setup Continue path or any handler that accesses creator data before a creator/workspace exists"
---

# Creator data access without a boot-time pool

## Context

The Nexus service boots without a creator SQLite pool when `active_creator_id` is absent from `~/.nexus42/config.toml`. This is **by design** (boot-without-creator), not a bug. The Setup wizard's Continue path calls `ensureSetupBootstrap()` (an Electron IPC command) which **only writes** `active_creator_id` to config.toml — it does not notify the service or pre-open anything. A subsequent request that needs creator data must therefore resolve the workspace store on demand.

The current core does **not** keep a long-lived creator pool with a lazy-open entry point. Each creator write resolves its own store: `patch_creator` (`crates/nexus-core/src/creators.rs:272`) calls `active_workspace_db_path()` (`creators.rs:504-508`, which reads `active_creator_id` and returns the state DB path only if it exists), opens a short-lived pool with `nexus_local_db::init_pool`, performs the upsert, and closes it (`pool.close().await`, `creators.rs:331-338`). A missing `active_creator_id` makes the DB resolution fail closed (`CoreError::Uninitialized`).

## Guidance

- **Resolve the store per operation; do not assume a boot-held pool.** Read `active_creator_id`, resolve the state DB path (`active_workspace_db_path`), open a pool for the write, and close it. This mirrors `patch_creator`.
- **Fail closed when unresolved.** No `active_creator_id` (or a missing DB file) → `Uninitialized` (HTTP 409) before any write; do not fabricate a store. The caller should bootstrap first via the Setup wizard.
- **Never keep a global mutable pool** keyed by a config value that changes underneath it — a second creator activation would silently retarget writes.

## Why This Matters

Without per-operation resolution, the Setup wizard's Continue button would fail on every clean first run with HTTP 409 `uninitialized` ("Workspace not initialized"). The web-side error classifier (`classifySetupContinueError`) correctly classifies this as a soft error (no Reset shown), but the user cannot advance until a workspace/creator exists.

## When to Apply

- Any handler that accesses creator data before a creator/workspace has been activated.
- Any Setup wizard code path that runs after `ensureSetupBootstrap` but before a workspace exists.
- Any test that simulates a clean first run (no `active_creator_id` at boot).

## Examples

### Writing the display name without a boot pool (current)

`patch_creator` (`crates/nexus-core/src/creators.rs:272`) materializes the membership SSOT (`ensure_profile_home_ssot`), then — only when a display name is supplied — resolves the DB and writes through a short-lived pool:

```rust
let db_path = self.active_workspace_db_path().ok_or(CoreError::Uninitialized)?;
let pool = nexus_local_db::init_pool(&db_path).await.map_err(crate::error::local_db_err)?;
let upsert = upsert_creator_display_name(&pool, creator_id, display_name).await;
if let Err(err) = upsert {
    pool.close().await;
    return Err(err);
}
pool.close().await;
```

### Migration failure surfacing (do NOT expect Reset on this path)

A schema-init / migration failure inside the per-operation pool open maps through `local_db_err` to `CoreError::Internal` (`crates/nexus-core/src/error.rs:202`, the `other =>` arm; the real cause is only carried in the internal `category`), and `mapNativeError` replaces it at the sanitization boundary with a generic `500 internal` / `"Internal server error"` (`apps/nexus-service/src/errors.ts:86-91`) — the migration detail never reaches the client. And `patch_creator` runs in the **display_name** phase, which `classifySetupContinueError` (`apps/web/src/lib/setup/continue-error.ts`) classifies unconditionally as `soft_display_name`; its `/migration/i` → `migration_db` (the only class that permits Reset) applies to the **bootstrap** phase only. So a `patch_creator` migration failure surfaces as a generic soft error, never as Reset.

## Prevention

- Any new handler that touches creator data must resolve the store itself (per-operation pool) or fail closed with `Uninitialized` before writing.
- Tests should include a "clean first run" scenario (no `active_creator_id` at boot).
