-- nexus-sync outbox schema (DDL from outbox.rs init_pool_with_schema)
-- Run this against the reference database after nexus-local-db migrations.

CREATE TABLE IF NOT EXISTS outbox_entries (
    outbox_entry_id   TEXT PRIMARY KEY,
    bundle_id         TEXT NOT NULL,
    idempotency_key   TEXT NOT NULL,
    delivery_state    TEXT NOT NULL DEFAULT 'staged',
    retry_count       INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    next_retry_at     TEXT,
    command_payload   TEXT NOT NULL DEFAULT '{}',
    bundle_payload    TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_delivery_state
    ON outbox_entries(delivery_state);

CREATE INDEX IF NOT EXISTS idx_outbox_next_retry
    ON outbox_entries(next_retry_at)
    WHERE delivery_state IN ('staged', 'failed');

CREATE INDEX IF NOT EXISTS idx_outbox_bundle_id
    ON outbox_entries(bundle_id);

CREATE TABLE IF NOT EXISTS partial_apply_states (
    outbox_entry_id   TEXT PRIMARY KEY,
    state_json        TEXT NOT NULL,
    recorded_at       TEXT NOT NULL,
    retry_count       INTEGER NOT NULL DEFAULT 0
);
