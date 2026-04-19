# Dual outbox architecture (TD-8)

**Source plan:** `v1-tech-debt-cleanup` (Batch D, Task 14 / TD-8)  
**Status:** Active — design record; **full consolidation not delivered in V1.1-era milestone**

## Problem statement

Two distinct outbox concepts exist in the OSS tree:

1. **Daemon / local DB** — `nexus-local-db` schema includes an `outbox` table used for local daemon persistence and sync-adjacent bookkeeping (see `crates/nexus-local-db/src/schema.rs`).
2. **Sync client** — `nexus-sync` owns `Outbox` and an `outbox_entries`-style model with delivery state, retries, and hooks (see `crates/nexus-sync/src/outbox.rs`).

This split predates a single “unified outbox” migration and carries **schema and operational duplication risk** if both paths are used for the same logical events without a clear boundary.

## Current decision (milestone cap)

**No schema merge or daemon→`nexus_sync::Outbox` delegation** was executed under the V1.1-era C+D milestone. Reasons:

- Touching daemon SQLite migrations and sync client storage together is a **large, cross-cutting** change (data migration, rollback, CI fixtures).
- Correct unification requires a **product-owned** cutover plan (which subsystem is authoritative, idempotency keys, replay semantics).

## Recommended follow-up (V1.2+)

1. Document the **single writer** rule per event type (daemon local vs sync push pipeline).
2. Either **delegate** daemon persistence to sync’s outbox abstraction where both are needed, or **narrow** one table to a strictly local concern and rename for clarity.
3. Add migration tests and a rollback story before enabling in production.

## Related

- [architecture-alignment-review-v1.md](architecture-alignment-review-v1.md) (TD-8 source)