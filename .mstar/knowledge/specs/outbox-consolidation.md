# Outbox Consolidation — Single-Writer Contract & Schema Ownership

**Status**: Normative (Master, V1.59 P-last promote)
**Document class**: Master
**Author**: @fullstack-dev-2 (Track B canonical owner)
**Date**: 2026-06-22
**Scope**: Consolidation of dual-outbox architecture into a single unified outbox schema in `nexus-local-db`. Defines single-writer rule, schema ownership boundary, migration path, and flush/compact semantics.
**Coordinates with**:
- [orchestration-engine.md](orchestration-engine.md) §5.2 (capability roster — `outbox.flush` / `outbox.compact` rows move from Deferred wiring → Shipped)
- [daemon-runtime.md](daemon-runtime.md) §10 (flush/compact invocation path)
- [dual-outbox-architecture.md](../../archived/knowledge/dual-outbox-architecture.md) (archived problem statement — this spec resolves the three recommended follow-ups)

---

## 1. Problem Summary

Prior to V1.59, two outbox concepts coexisted:

1. **Sync outbox** (`outbox_entries` table, DDL in `20260420_outbox_tables.sql` migration, managed by `nexus-cloud-sync::outbox::Outbox`) — full delivery-state machine (staged → ready → sent → acked/conflicted/failed), retry with exponential backoff, partial-apply persistence.
2. **Daemon legacy outbox** (`outbox` table, DDL in initial migration `20260417_000001_initial.sql`) — simple command queue (`id`, `command_type`, `payload`, `status`, `created_at`, `sent_at`, `error`) with **no active Rust-level consumers** (confirmed by V1.59 T3 audit). Exists only as DDL with a single test assertion in `nexus-daemon-runtime/src/db/schema.rs`.

The split was identified in V1.1-era TD-8 ([dual-outbox-architecture.md](../../archived/knowledge/dual-outbox-architecture.md)) and deferred for a later consolidation. V1.59 P1 is that consolidation.

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
- **No sync subsystem** may read or write the legacy `outbox` table — it is deprecated (see §3).
- Cross-writer violations are guarded by runtime enforcement (see §2.3).

### 2.3 Runtime enforcement

The following enforcement guards exist:

1. **Daemon-runtime schema test** (`nexus-daemon-runtime/src/db/schema.rs`): the assertion that the legacy `outbox` table exists is annotated with a deprecation comment and `tracing::warn!` explaining the phased-removal plan. This is the **sole access point** — no production code reads or writes the legacy table.
2. **Orchestration capability layer**: `OutboxFlush` and `OutboxCompact` operate exclusively on `outbox_entries` via the injected `sqlx::SqlitePool`. They do not reference the legacy table.
3. **Cloud-sync outbox**: `nexus-cloud-sync::outbox::Outbox` only operates on `outbox_entries` and `partial_apply_states`.

No debug-mode `assert!` is added because the legacy table has **zero active write paths** — there is no code path to guard.

---

## 3. Schema Ownership Boundary

### 3.1 Migration-managed schema (authoritative)

The `outbox_entries` and `partial_apply_states` tables are defined in:

**`crates/nexus-local-db/migrations/20260420_outbox_tables.sql`**

This migration is run by `nexus_local_db::run_migrations()` which is called by:
- `nexus-cloud-sync::outbox::Outbox::init_pool_with_schema()` — sync client initialization
- `nexus_local_db::init_pool()` — general DB initialization

The tables are **not** created inline by any Rust code (WS8 R4, closed in V1.21).

### 3.2 Table ownership

| Table | Owner crate | Migration | Notes |
|---|---|---|---|
| `outbox_entries` | `nexus-local-db` (schema) / `nexus-cloud-sync` (runtime) | `20260420_outbox_tables.sql` | Delivery-state machine; accessed by sync client + orchestration capabilities |
| `partial_apply_states` | `nexus-local-db` (schema) / `nexus-cloud-sync` (runtime) | `20260420_outbox_tables.sql` | Persisted partial-apply state for SYNC-R12 resume |
| `outbox` (legacy) | `nexus-local-db` (schema only — no runtime owner) | `20260417_000001_initial.sql` | **Deprecated** — phased removal planned post-V1.59 |

### 3.3 Migration path

- **V1.59 (this plan)**: deprecate legacy `outbox` table; confirm `outbox_entries`/`partial_apply_states` are migration-managed; wire flush/compact capabilities.
- **V1.60+**: drop legacy `outbox` table after confirming no external tooling depends on it.
- **No data migration** is needed — the tables are independent schemas with no shared data.

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

## 6. Legacy Outbox Table Deprecation

### 6.1 Audit result (V1.59 T3)

The legacy `outbox` table (defined in `20260417_000001_initial.sql`) was audited for active consumers:

- **Rust code reads**: 0
- **Rust code writes**: 0
- **Test assertions**: 1 (`nexus-daemon-runtime/src/db/schema.rs` line 64 — `assert!(table_names.contains(&"outbox"))`)
- **SQL references (non-migration)**: 0

**Conclusion**: The table has **no active consumers**. It exists only as DDL with a single test assertion that confirms its presence.

### 6.2 Deprecation approach (V1.59)

1. **No DDL change** — the table is NOT dropped (per plan constraint).
2. **Deprecation annotation**: the test assertion in `nexus-daemon-runtime/src/db/schema.rs` is annotated with a deprecation comment and `tracing::warn!` noting the phased-removal plan.
3. **Spec documentation**: this document is the official record of the deprecation.

### 6.3 Phased removal plan (V1.60+)

| Phase | Action |
|---|---|
| V1.59 (current) | Deprecation marker + audit documentation |
| V1.60 | Add `_deprecated` suffix comment to migration; verify no external tooling references the table |
| V1.61+ | Drop table in a new migration; remove test assertion |

---

## 7. Compliance Checklist

- [x] Sync outbox DDL is migration-managed (`20260420_outbox_tables.sql`)
- [x] `nexus-cloud-sync` calls `nexus_local_db::run_migrations()` (no inline DDL)
- [x] Single-writer rule documented per event type
- [x] Schema ownership boundary defined
- [x] Flush semantics defined with input/output schemas + test vectors
- [x] Compact semantics defined with input/output schemas + test vectors
- [x] Legacy `outbox` table audit complete (0 consumers)
- [x] Deprecation plan documented with phased removal timeline
- [ ] Flush capability wired to real implementation (T4)
- [ ] Compact capability wired to real implementation (T4)
- [ ] Sync CLI regression tests pass (T5)

---

*Last updated: 2026-06-22 (initial Draft). Promoted to Master at V1.59 P-last.*
