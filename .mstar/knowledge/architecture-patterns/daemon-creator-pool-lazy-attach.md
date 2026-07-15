---
module: nexus-daemon-runtime
date: 2026-07-16
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-15-v1.119-setup-continue-unblock
tags: [daemon, creator-pool, bootstrap, setup-wizard, lazy-attach, tauri, first-run]
applies_when: "Editing creator DB pool lifecycle, Setup Continue path, or any handler that accesses creator data before daemon restart"
---

# Daemon creator pool lazy-attach pattern

## Context

The Nexus daemon (`nexus-daemon-runtime`) boots without a creator SQLite pool when `active_creator_id` is absent from `~/.nexus42/config.toml`. This is **by design** (V1.118 AC-P0-1: boot-without-creator), not a bug. However, the Setup wizard's Continue path calls `ensureSetupBootstrap()` (Tauri command) which **only writes** `active_creator_id` to config.toml — it does not notify the daemon or open the pool. Any subsequent HTTP request that needs the pool will fail with HTTP 409 `uninitialized` unless the handler lazily attaches the pool.

## Guidance

### When you need the creator pool after `ensureSetupBootstrap` but before daemon restart

Call `state.ensure_creator_pool().await` before `state.pool_or_uninit()?`. This is idempotent:
- If pool is already open: fast no-op (`OnceLock::get` check)
- If pool is closed but `active_creator_id` exists: opens + migrates the DB
- If no `active_creator_id`: returns error (correct — caller should bootstrap first)

### Pattern (from `patch_creator`)

```rust
// In handler, before pool access:
if state.pool().is_none() {
    if let Some(_id) = read_active_creator_id(state.nexus_home()).ok().flatten() {
        state.ensure_creator_pool().await;
    }
}
// Now pool_or_uninit() will succeed if bootstrap was done
```

### Do NOT attempt web-only fixes

The following were verified as dead ends during V1.119 P0 investigation:
- `client.setActiveCreator()` → 404 `not_found` (fresh creator not in `auth_store` or `creator_identity_cache.json`)
- `client.createCreator()` → 405 (POST route not registered)
- `desktop.startDaemon()` (restart) → racy, conflicts with Tauri `.setup()` sidecar ownership (V1.105 D2)

## Why This Matters

Without the lazy-attach, the Setup wizard's Continue button fails on every clean first run. The error surfaces as HTTP 409 `uninitialized` ("Workspace not initialized"), which is a `soft_display_name` class error — not migration-class. The web-side error classifier (`classifySetupContinueError`) correctly classifies this as soft (no Reset shown), but the user cannot advance until the pool is attached.

## When to Apply

- Any new Tier-1 handler (not behind `require_active_creator` middleware) that accesses creator data
- Any Setup wizard code path that runs after `ensureSetupBootstrap` but before daemon restart
- Any test that simulates a clean first run (no `active_creator_id` at daemon boot)

## Examples

### The V1.119 P0 fix

**File:** `crates/nexus-daemon-runtime/src/api/handlers/creators.rs`, `patch_creator`

Before the fix, `patch_creator` called `state.pool_or_uninit()?` directly, which returned `Uninitialized` (HTTP 409) on clean first run because the pool was `None`.

After the fix, `patch_creator` calls `state.ensure_creator_pool().await` first, which opens + migrates the DB, then `pool_or_uninit()` succeeds.

### Migration error propagation

When `ensure_creator_pool()` runs `Schema::init` and migration fails, the error message now contains "migration" (via `CreatorDbOutcome.open_error`), so the web classifier's `/migration/i` regex can detect it and show Reset (AC-P0-3).

## Prevention

- Any new handler that accesses `state.pool()` or `state.pool_or_uninit()` should either:
  1. Be behind `require_active_creator` middleware (Tier-2), OR
  2. Call `ensure_creator_pool().await` before pool access (Tier-1)
- Tests should include a "clean first run" scenario (no `active_creator_id` at boot)
