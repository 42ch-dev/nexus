# Outbox schema: `outbox_entries` and migrations

This document describes the **sync outbox** (`outbox_entries` in `nexus-sync`) and how it relates to the **daemon** queue table, plus the planned **v1.1 → v1.2** schema evolution. It complements the module-level summary in `crates/nexus-sync/src/outbox.rs`.

## `outbox_entries` vs daemon `outbox`

The `outbox_entries` table in `nexus-sync` is **intentionally different** from the daemon’s `outbox` table (`nexus42d` / `nexus-local-db` schema). The daemon `outbox` is a simple command queue; `outbox_entries` is a bundle-level sync outbox with idempotency keys, retry tracking, and delivery state management. They serve different purposes and must **not** be merged without an explicit consolidation design (see architecture alignment TD-8).

## Current schema (v1.0 / v1.1)

The `outbox_entries` table currently has **no** `schema_version` column. The `OutboxEntry` contract uses `LATEST_SCHEMA_VERSION` at read time, so all rows are assumed to match the current schema version. This remains safe while the on-disk table structure matches that assumption across releases.

## v1.1 → v1.2 migration plan

When the outbox schema evolves (for example new columns for bundle metadata or conflict tracking), add a `schema_version` column to distinguish rows written by different schema generations.

### Step 1: Add `schema_version` column

```sql
ALTER TABLE outbox_entries ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;
```

- Existing rows default to version `1` (matching `LATEST_SCHEMA_VERSION` at migration time).
- New writes use the current `LATEST_SCHEMA_VERSION` from the bundle envelope.

### Step 2: Write `schema_version` on insert

Update `append()` and `stage()` to include `schema_version` in `INSERT` statements, using `LATEST_SCHEMA_VERSION`.

### Step 3: Read `schema_version` on query

Update `replay()` and `get()` to read `schema_version` from the database instead of assuming `LATEST_SCHEMA_VERSION` everywhere, so older rows can be handled explicitly.

## Future migrations

- Increment `schema_version` when the bundle structure changes in a backward-incompatible way.
- Use migration SQL plus Rust code for backward compatibility of existing rows.
- If `partial_apply_states` JSON shape evolves, consider a `schema_version` column there as well.

## Migration safety

- Prefer **additive** schema changes (new columns with defaults) so existing data keeps working.
- Avoid removing or renaming columns in a minor bump.
- `CREATE TABLE IF NOT EXISTS` is idempotent for **new** databases but does **not** add columns; use explicit `ALTER TABLE` in a dedicated migration path.

## See also

- `crates/nexus-sync/src/outbox.rs` — implementation and short inline summary
- `crates/nexus-local-db` — shared local DB schema and `DB_SCHEMA_VERSION`
