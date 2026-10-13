# Outbox Consolidation — Single-Writer Contract & Schema Ownership

**Status**: Normative (Master)
**Document class**: Master
**Author**: @fullstack-dev-2 (Track B canonical owner)
**Date**: 2026-06-22
**Scope**: Consolidation of dual-outbox architecture into a single unified outbox schema in `nexus-local-db`. Defines single-writer rule, schema ownership boundary, migration path, and flush/compact semantics.
**Coordinates with**:
- [orchestration-engine.md](../orchestration/orchestration-engine.md) §5.2 (capability roster — `outbox.flush` / `outbox.compact` rows move from Deferred wiring → Shipped)
- [daemon-runtime.md](../../archived/runtime/2026-09-21-daemon-runtime.md) §11 (historical record; flush/compact invocation path)

---

## 1. Problem Summary

The local DB has exactly one outbox: `outbox_entries` — the full delivery-state machine (staged → ready → sent → acked/conflicted/failed) with retry via exponential backoff and partial-apply persistence, DDL in migration `20260420_outbox_tables.sql`, managed by `nexus-cloud-sync::outbox::Outbox`.

The former daemon legacy `outbox` command-queue table does not exist; it is dropped by migration `20260812_drop_legacy_outbox.sql` (see §3.3, §6).

---

## 2. Single-Writer Rule

### 2.1 Per-event-type exclusivity

Each outbox event type has exactly **one** authorized writer subsystem:

| Event type | Authorized writer | Table | Crate |
|---|---|---|---|
| Sync push commands | `nexus-cloud-sync::outbox::Outbox::append()` | `outbox_entries` | `nexus-cloud-sync` |
| Sync pull bundles | `nexus-cloud-sync::outbox::Outbox::stage()` / `stage_if_absent()` | `outbox_entries` | `nexus-cloud-sync` |
| Outbox flush (local drain) | `nexus-orchestration::capability::builtins::OutboxFlush` | `outbox_entries` | `nexus-orchestration` |
| Outbox compaction | `nexus-orchestration::capability::builtins::OutboxCompact` | `outbox_entries` | `nexus-orchestration` |

### 2.2 Exclusivity contracts

- **No daemon subsystem** may read or write `outbox_entries` directly — all access is routed through `nexus-cloud-sync::outbox::Outbox` or the orchestration capability layer.
- The legacy `outbox` table does not exist (see §3.3); no subsystem references it.
- Cross-writer violations are guarded by runtime enforcement (see §2.3).

### 2.3 Runtime enforcement

The following enforcement guards exist:

1. **Orchestration capability layer**: `OutboxFlush` and `OutboxCompact` operate exclusively on `outbox_entries` via the injected `sqlx::SqlitePool`. They never reference the legacy `outbox` table — which does not exist.
2. **Cloud-sync outbox**: `nexus-cloud-sync::outbox::Outbox` only operates on `outbox_entries` and `partial_apply_states`.


---

## 3. Schema Ownership Boundary

### 3.1 Migration-managed schema (authoritative)

The `outbox_entries` and `partial_apply_states` tables are defined in:

**`crates/nexus-local-db/migrations/20260420_outbox_tables.sql`**

This migration is run by `nexus_local_db::run_migrations()` which is called by:
- `nexus-cloud-sync::outbox::Outbox::init_pool_with_schema()` — sync client initialization
- `nexus_local_db::init_pool()` — general DB initialization

The tables are **not** created inline by any Rust code.

### 3.2 Table ownership

| Table | Owner crate | Migration | Notes |
|---|---|---|---|
| `outbox_entries` | `nexus-local-db` (schema) / `nexus-cloud-sync` (runtime) | `20260420_outbox_tables.sql` | Delivery-state machine; accessed by sync client + orchestration capabilities |
| `partial_apply_states` | `nexus-local-db` (schema) / `nexus-cloud-sync` (runtime) | `20260420_outbox_tables.sql` | Persisted partial-apply state for SYNC-R12 resume |
| `outbox` (legacy) | none (dropped) | `20260417_000001_initial.sql` (DDL remains as a frozen record) | Dropped by `20260812_drop_legacy_outbox.sql`; no runtime owner |

### 3.3 Legacy-table drop

The legacy `outbox` table is dropped by `crates/nexus-local-db/migrations/20260812_drop_legacy_outbox.sql` (`DROP TABLE IF EXISTS outbox;`). No data migration exists — the tables are independent schemas with no shared data.

---

## 4. Flush Semantics (outbox.flush)

### 4.1 Contract

`outbox.flush` drains pending outbox entries by marking them as flushed/delivered. In local-only mode (platform paused), this is a DB-only operation:

1. Accept optional `limit` (integer, default 0 = unlimited).
2. Select entries in `staged` or `ready` state, ordered by `created_at` ASC.
3. Transition each entry to `acked` (delivered) state with current timestamp.
4. Return `{ flushed: <count> }`.

### 4.2 Input Schema

```json
{
  "type": "object",
  "properties": {
    "limit": { "type": "integer", "minimum": 0, "default": 0 }
  },
  "required": [],
  "additionalProperties": false
}
```

### 4.3 Output Schema

```json
{
  "type": "object",
  "properties": {
    "flushed": { "type": "integer", "minimum": 0 }
  },
  "required": ["flushed"],
  "additionalProperties": false
}
```

### 4.4 Runtime behavior

- **Pool available (production)**: Execute `UPDATE outbox_entries SET delivery_state = 'acked', updated_at = ? WHERE delivery_state IN ('staged', 'ready') ORDER BY created_at ASC LIMIT ?`. Count affected rows. Return `{ flushed: N }`.
- **Pool unavailable (standalone/test)**: Return `CapabilityError::Internal("no database pool")`.
- **No entries**: Return `{ flushed: 0 }` (success, not error).

### 4.5 Test vectors

| Test | Input | Expected output |
|---|---|---|
| flush_no_entries | `{"limit": 100}` | `{"flushed": 0}` |
| flush_all_pending | `{}` | `{"flushed": N}` where N = count of staged/ready entries |
| flush_with_limit | `{"limit": 1}` | `{"flushed": 1}` when ≥1 entry exists |
| flush_no_pool_fails | `{}` (no pool) | `CapabilityError::Internal` |

---

## 5. Compact Semantics (outbox.compact)

### 5.1 Contract

`outbox.compact` removes successfully-delivered (`acked`) entries older than a configurable retention window. Default is 7 days.

1. Accept optional `retentionDays` (integer, minimum 0, default 7). A value of `0` means "remove all acked entries immediately" (matches the implementation's `.max(0)` clamp).
2. Delete entries where `delivery_state = 'acked'` AND `updated_at < (now - retention_days)`.
3. Count retained entries (remaining `acked` entries after compaction).
4. Return `{ removed: <count>, retained: <count> }`.

### 5.2 Input Schema

```json
{
  "type": "object",
  "properties": {
    "retentionDays": { "type": "integer", "minimum": 0, "default": 7 }
  },
  "required": [],
  "additionalProperties": false
}
```

### 5.3 Output Schema

```json
{
  "type": "object",
  "properties": {
    "removed": { "type": "integer", "minimum": 0 },
    "retained": { "type": "integer", "minimum": 0 }
  },
  "required": ["removed", "retained"],
  "additionalProperties": false
}
```

### 5.4 Runtime behavior

- **Pool available (production)**: Compute cutoff timestamp. Delete matching entries. Count remaining. Return `{ removed: N, retained: M }`.
- **Pool unavailable (standalone/test)**: Return `CapabilityError::Internal("no database pool")`.
- **No entries to compact**: Return `{ removed: 0, retained: 0 }` (success, not error).

### 5.5 Test vectors

| Test | Input | Expected output |
|---|---|---|
| compact_no_entries | `{"retentionDays": 7}` | `{"removed": 0, "retained": 0}` |
| compact_old_acked | `{"retentionDays": 0}` | `{"removed": N, "retained": 0}` where N = all acked entries |
| compact_future_retention | `{"retentionDays": 365}` | `{"removed": 0, "retained": M}` where M = all acked entries |
| compact_no_pool_fails | `{}` (no pool) | `CapabilityError::Internal` |

---

## 6. Legacy Outbox Table

The legacy daemon `outbox` command-queue table has no runtime owner: zero Rust reads, zero Rust writes, zero non-migration SQL references. Its DDL remains in `20260417_000001_initial.sql` only as a frozen record; the table itself is dropped by `20260812_drop_legacy_outbox.sql` (§3.3). No deprecation window or phased-removal step remains open.

---

## 7. Compliance Checklist

- [x] Sync outbox DDL is migration-managed (`20260420_outbox_tables.sql`)
- [x] `nexus-cloud-sync` calls `nexus_local_db::run_migrations()` (no inline DDL)
- [x] Single-writer rule documented per event type
- [x] Schema ownership boundary defined
- [x] Flush semantics defined with input/output schemas + test vectors
- [x] Compact semantics defined with input/output schemas + test vectors
- [x] Legacy `outbox` table audit complete (0 consumers)
- [x] Legacy `outbox` table dropped (`20260812_drop_legacy_outbox.sql`; §6)
- [x] Flush capability wired to real implementation (`OutboxFlush` in `nexus-orchestration::capability::builtins::outbox`, registered in `with_builtins_and_pool` / `with_runtime_deps`)
- [x] Compact capability wired to real implementation (`OutboxCompact` in `nexus-orchestration::capability::builtins::outbox`, registered in `with_builtins_and_pool` / `with_runtime_deps`)
- [ ] Sync CLI regression tests pass

---

