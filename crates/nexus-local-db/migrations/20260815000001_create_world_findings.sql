-- V1.165 P1 (DR-68, AR-1): world-scoped check findings home.
-- DDL verbatim from .mstar/iterations/v1.165/specs/v1.165-pipeline-closeout-locks.md §AR-1.
-- Spoke `Finding` vocabulary persists verbatim; no CHECK (spoke schema documents,
-- does not enforce; mirrors legacy discipline). No creator_id column — isolation
-- is world ownership; the stamped extensions.nexus.creator_id rides extensions_json.

CREATE TABLE IF NOT EXISTS world_findings (
    finding_id         TEXT PRIMARY KEY,             -- spoke Finding.finding_id (fnd_*)
    world_id           TEXT NOT NULL,                -- FK → narrative_worlds.world_id
    schema_version     INTEGER NOT NULL DEFAULT 1,   -- spoke Finding.schema_version
    severity           TEXT NOT NULL,                -- spoke vocabulary verbatim (info|warning|error) — open string, no CHECK (spoke schema documents, does not enforce; mirrors legacy discipline)
    status             TEXT NOT NULL DEFAULT 'open', -- spoke vocabulary verbatim (open|resolved|dismissed)
    title              TEXT NOT NULL,
    description        TEXT NOT NULL DEFAULT '',
    kind               TEXT,                         -- spoke optional checker kind
    target_entry_id    TEXT,                         -- spoke optional entry id
    source_anchor_json TEXT,                         -- spoke optional structured SourceAnchor, verbatim JSON
    suggested_fix      TEXT,                         -- spoke optional remediation text
    text_position_json TEXT NOT NULL DEFAULT '{}',   -- spoke Map, verbatim JSON
    extensions_json    TEXT NOT NULL DEFAULT '{}',   -- verbatim ExtensionMap (incl. stamped nexus.world_id / creator_id)
    created_at         INTEGER NOT NULL,             -- Unix epoch (legacy `findings` convention)
    updated_at         INTEGER NOT NULL,             -- Unix epoch
    FOREIGN KEY (world_id) REFERENCES narrative_worlds(world_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_world_findings_world_created ON world_findings(world_id, created_at);
CREATE INDEX IF NOT EXISTS idx_world_findings_world_target  ON world_findings(world_id, target_entry_id);
