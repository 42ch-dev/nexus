-- V1.26 narrative persistence: worlds and timeline events.
--
-- Workspace-local projections for NarrativeGateway read paths.
-- Domain semantics owned by nexus-narrative; migration/storage
-- mechanics owned by nexus-local-db.

CREATE TABLE IF NOT EXISTS narrative_worlds (
    world_id TEXT PRIMARY KEY CHECK (world_id LIKE 'wld_%'),
    workspace_id TEXT NOT NULL,
    owner_creator_id TEXT NOT NULL,
    title TEXT NOT NULL,
    slug TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived', 'paused')),
    visibility TEXT NOT NULL,
    time_policy TEXT NOT NULL,
    canon_revision INTEGER,
    current_timeline_head_id TEXT,
    current_time_pointer TEXT,
    root_fork_branch_id TEXT,
    world_rules_json TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    FOREIGN KEY (owner_creator_id) REFERENCES creators (creator_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_narrative_worlds_workspace_id
    ON narrative_worlds (workspace_id);
CREATE INDEX IF NOT EXISTS idx_narrative_worlds_owner_creator_id
    ON narrative_worlds (owner_creator_id);
CREATE INDEX IF NOT EXISTS idx_narrative_worlds_status
    ON narrative_worlds (status);

CREATE TABLE IF NOT EXISTS narrative_timeline_events (
    timeline_event_id TEXT PRIMARY KEY CHECK (timeline_event_id LIKE 'evt_%'),
    world_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('canon', 'provisional', 'rejected')),
    sequence_no INTEGER NOT NULL CHECK (sequence_no >= 0),
    title TEXT,
    summary TEXT,
    caused_by_event_ids_json TEXT,
    affected_key_block_ids_json TEXT,
    source_command_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (world_id) REFERENCES narrative_worlds (world_id) ON DELETE CASCADE,
    UNIQUE (world_id, branch_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_world_id
    ON narrative_timeline_events (world_id);
CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_world_branch_sequence
    ON narrative_timeline_events (world_id, branch_id, sequence_no);
CREATE INDEX IF NOT EXISTS idx_narrative_timeline_events_status
    ON narrative_timeline_events (status);
