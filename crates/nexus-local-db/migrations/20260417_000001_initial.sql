-- Nexus Local DB: Complete Schema (dev-mode single-file migration)
--
-- All tables for both CLI and daemon in one idempotent file.
-- Uses CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS.
-- Pragmas (journal_mode=WAL, foreign_keys=ON) are set per-connection
-- in open_pool(), not in migrations.
--
-- Incremental migration history will be added at T9 (pre-release).

-- ============================================================================
-- Shared Tables (CLI and daemon both depend)
-- ============================================================================

CREATE TABLE IF NOT EXISTS workspace_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS creators (
    creator_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cached_at TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reference_sources (
    reference_source_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL DEFAULT 'local',
    source_type TEXT NOT NULL,
    uri TEXT NOT NULL,
    title TEXT NOT NULL,
    tags TEXT,
    content_hash TEXT,
    content TEXT,
    scan_status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS local_identities (
    creator_id TEXT PRIMARY KEY,
    identity_type TEXT NOT NULL,
    display_name TEXT,
    created_at TEXT NOT NULL,
    platform_linked INTEGER NOT NULL DEFAULT 0,
    platform_creator_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_local_identities_creator_id ON local_identities (creator_id);

-- ============================================================================
-- Daemon-only Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    sent_at TEXT,
    error TEXT
);

CREATE TABLE IF NOT EXISTS auth_tokens (
    user_id TEXT PRIMARY KEY,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS acp_tool_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name TEXT NOT NULL,
    path TEXT NOT NULL,
    outcome TEXT NOT NULL,
    agent_id TEXT,
    session_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS acp_sessions (
    session_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_active TEXT NOT NULL,
    workspace_hint TEXT NOT NULL DEFAULT '',
    metadata TEXT NOT NULL DEFAULT '{}'
);

-- ============================================================================
-- SOUL Lifecycle (V1.2)
-- ============================================================================

CREATE TABLE IF NOT EXISTS soul_meta (
    creator_id TEXT NOT NULL PRIMARY KEY,
    file_path TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    personality_hash TEXT,
    experience_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_soul_meta_creator_id ON soul_meta (creator_id);

-- ============================================================================
-- Memory Pipeline (V1.2)
-- ============================================================================

CREATE TABLE IF NOT EXISTS memory_pending_review (
    pending_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    creator_id TEXT NOT NULL,
    world_id TEXT,
    task_kind TEXT NOT NULL DEFAULT 'unknown',
    raw_digest TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS memory_fragments (
    fragment_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    keywords TEXT NOT NULL DEFAULT '[]',
    summary TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    ttl TEXT
);

CREATE INDEX IF NOT EXISTS idx_memory_pending_review_creator_id ON memory_pending_review (creator_id);
CREATE INDEX IF NOT EXISTS idx_memory_fragments_creator_id ON memory_fragments (creator_id);

-- ============================================================================
-- Seed db_schema_version for backwards compatibility
-- ============================================================================

INSERT OR IGNORE INTO workspace_meta (key, value)
VALUES ('db_schema_version', '4');
