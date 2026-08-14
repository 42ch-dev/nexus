-- V1.164 P2: MindState when-axis table (l5-mind derivative records).
-- Authority stays on holder KnowledgeEntry modules.mental/belief.
-- This table stores temporal snapshots/deltas (non-authoritative).
-- FK target is kb_key_blocks.key_block_id (the DDL column name;
-- the Rust field `entry_id` maps to it — TL-3).
CREATE TABLE IF NOT EXISTS mind_states (
    mind_state_id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL,
    holder_entry_id TEXT NOT NULL,
    canonical_name TEXT,
    occurred_at TEXT,
    sort_key TEXT,
    snapshot_json TEXT,
    deltas_json TEXT,
    source_anchor_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    extensions_json TEXT,
    FOREIGN KEY (holder_entry_id) REFERENCES kb_key_blocks(key_block_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_mind_states_holder ON mind_states(holder_entry_id);
CREATE INDEX IF NOT EXISTS idx_mind_states_holder_occurred ON mind_states(holder_entry_id, occurred_at);
