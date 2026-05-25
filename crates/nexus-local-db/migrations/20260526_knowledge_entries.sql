-- V1.27 User knowledge persistence: knowledge entries table.
--
-- Stores User-scoped global knowledge entries for Moment context assembly.
-- Domain semantics owned by nexus-knowledge; migration/storage mechanics
-- owned by nexus-local-db.
--
-- All queries MUST include WHERE user_id = ? to enforce scope isolation.

CREATE TABLE IF NOT EXISTS knowledge_entries (
    entry_id TEXT PRIMARY KEY CHECK (entry_id LIKE 'kno_%'),
    user_id TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    content TEXT NOT NULL,
    reference_uri TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_entries_user_id
    ON knowledge_entries (user_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_entries_user_created
    ON knowledge_entries (user_id, created_at DESC);
