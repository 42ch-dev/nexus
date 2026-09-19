-- no-transaction
-- v1.191 P1 Task 3: Actor holder registry and the offline governance cutover.
-- (14-digit version: sqlx orders numerically and the September rebuilds use
-- 14-digit versions up to 20260914000001.)
-- Spec: .mstar/specs/holder-governance.md §§2, 3, 5.
--
-- One offline cutover, not dual-write compatibility:
--   * `knowledge_holders` — one row per stored Creator/Character identity,
--     exclusive-subject CHECK + per-subject unique partial index, subject FKs
--     ON DELETE RESTRICT.
--   * `knowledge_import_quarantine` — unresolved/unknown foreign governance
--     (durable §6); outside the ordinary KB stores, search/MCA/compute/export.
--   * `kb_key_blocks.holder_entry_id` / `.disclosure` — the native governance
--     pair, with an FK to the registry ON DELETE RESTRICT and a governance
--     index; `creator_only` and its CHECK leave the table.
--   * `narrative_worlds.knowledge_revision` / `characters.knowledge_revision`.
--
-- Backfill equivalence (durable §5.3): shared rows stay NULL/NULL; every
-- `creator_only = true` row becomes `owner-private` under the holder of its
-- **stored World controlling Creator** (`narrative_worlds.owner_creator_id`),
-- never the shell's active Creator. The legacy `extensions.nexus.creator_only`
-- key is not unknown passthrough and is removed.
--
-- Every mutation — registry, registry backfill, table rebuild, revision
-- columns — happens in the single migration transaction the runner opens
-- (`apply_fk_suspension_tx`), so any abort below rolls the whole cutover back
-- with no partial schema.
--
-- The holder ids are BLAKE3 digests of the domain-separated subject (§2.1),
-- which SQLite cannot compute: `nexus-local-db` forbids `unsafe` and the
-- function would have to be a connection-local UDF. The migration runner
-- therefore stages `<subject_kind, subject_id, holder_entry_id>` rows in the
-- TEMP table `_v1191_holder_digest_staging` inside this same transaction
-- (`holders::stage_holder_digests_in_tx`), and this script aborts when that
-- staging is missing or incomplete. A migration path without the runner hook
-- fails loudly here instead of silently building a registry-less database.

PRAGMA foreign_keys=OFF;

-- ── Preflight: fail before changing any data ────────────────────────────────
--
-- Each guard is a TEMP table with one `NOT NULL` column; the counting INSERT
-- writes NULL exactly when the class is violated, so the abort message names
-- the failing class (`NOT NULL constraint failed: <guard>.violations`) and the
-- whole migration transaction rolls back. A CHECK would report only the
-- constraint expression, which is useless to the operator.

-- The digest staging is required and must cover every subject.
CREATE TEMP TABLE _preflight_holder_digest_staging_present (violations INTEGER NOT NULL);
INSERT INTO _preflight_holder_digest_staging_present
SELECT
    CASE WHEN EXISTS (
        SELECT 1 FROM sqlite_temp_master
        WHERE type = 'table' AND name = '_v1191_holder_digest_staging')
    THEN 0 ELSE NULL END;

-- Every stored Creator (including archived identities) is staged, nonempty.
CREATE TEMP TABLE _preflight_staging_creators_complete (violations INTEGER NOT NULL);
INSERT INTO _preflight_staging_creators_complete
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM creators c
WHERE c.creator_id = ''
   OR NOT EXISTS (
       SELECT 1 FROM _v1191_holder_digest_staging s
       WHERE s.subject_kind = 'creator' AND s.subject_id = c.creator_id);

-- Every stored Character (including archived identities) is staged, nonempty.
CREATE TEMP TABLE _preflight_staging_characters_complete (violations INTEGER NOT NULL);
INSERT INTO _preflight_staging_characters_complete
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM characters ch
WHERE ch.character_id = ''
   OR NOT EXISTS (
       SELECT 1 FROM _v1191_holder_digest_staging s
       WHERE s.subject_kind = 'character' AND s.subject_id = ch.character_id);

-- Nothing is staged that is not a stored subject.
CREATE TEMP TABLE _preflight_staging_orphans (violations INTEGER NOT NULL);
INSERT INTO _preflight_staging_orphans
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM _v1191_holder_digest_staging s
WHERE (s.subject_kind = 'creator'
       AND NOT EXISTS (SELECT 1 FROM creators c WHERE c.creator_id = s.subject_id))
   OR (s.subject_kind = 'character'
       AND NOT EXISTS (SELECT 1 FROM characters ch WHERE ch.character_id = s.subject_id));

-- No pre-existing registry/quarantine object (a partial or foreign attempt).
CREATE TEMP TABLE _preflight_preexisting_registry (violations INTEGER NOT NULL);
INSERT INTO _preflight_preexisting_registry
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM sqlite_master
WHERE name IN ('knowledge_holders', 'knowledge_import_quarantine');

-- No pre-existing native governance columns (hostile/partial schema): the
-- rebuild would otherwise silently drop their values instead of arbitrating.
CREATE TEMP TABLE _preflight_preexisting_governance_columns (violations INTEGER NOT NULL);
INSERT INTO _preflight_preexisting_governance_columns
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM pragma_table_info('kb_key_blocks')
WHERE name IN ('holder_entry_id', 'disclosure');

-- The legacy bool is a boolean domain (defense in depth behind its CHECK).
CREATE TEMP TABLE _preflight_legacy_bool_domain (violations INTEGER NOT NULL);
INSERT INTO _preflight_legacy_bool_domain
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks WHERE creator_only NOT IN (0, 1);

-- A legacy extension document must be a JSON object (the key removal below
-- cannot be expressed otherwise), and it may not contradict the column.
CREATE TEMP TABLE _preflight_malformed_extensions (violations INTEGER NOT NULL);
INSERT INTO _preflight_malformed_extensions
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks
WHERE extensions_nexus_json IS NOT NULL
  AND (json_valid(extensions_nexus_json) = 0
       OR json_type(extensions_nexus_json) <> 'object');

CREATE TEMP TABLE _preflight_legacy_extension_conflict (violations INTEGER NOT NULL);
INSERT INTO _preflight_legacy_extension_conflict
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks
WHERE json_type(extensions_nexus_json, '$.creator_only') IS NOT NULL
  AND (json_extract(extensions_nexus_json, '$.creator_only') IS NULL
       OR json_extract(extensions_nexus_json, '$.creator_only') NOT IN (0, 1)
       OR json_extract(extensions_nexus_json, '$.creator_only') <> creator_only);

-- Every `creator_only = true` row must resolve its stored World controlling
-- Creator: ownership is never inferred from the active shell Creator.
CREATE TEMP TABLE _preflight_unresolvable_world_creator (violations INTEGER NOT NULL);
INSERT INTO _preflight_unresolvable_world_creator
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks kb
LEFT JOIN narrative_worlds w ON w.world_id = kb.world_id
LEFT JOIN creators c ON c.creator_id = w.owner_creator_id
WHERE kb.creator_only = 1
  AND (w.world_id IS NULL OR c.creator_id IS NULL);

-- WorldSheet legality (durable §3): a linked sheet may not become private, and
-- it must stay a World-owned `character` block in the binding's own World.
CREATE TEMP TABLE _preflight_world_sheet_privatized (violations INTEGER NOT NULL);
INSERT INTO _preflight_world_sheet_privatized
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM actor_world_bindings b
JOIN kb_key_blocks kb ON kb.key_block_id = b.world_sheet_entry_id
WHERE kb.creator_only = 1;

CREATE TEMP TABLE _preflight_world_sheet_illegal (violations INTEGER NOT NULL);
INSERT INTO _preflight_world_sheet_illegal
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM actor_world_bindings b
JOIN kb_key_blocks kb ON kb.key_block_id = b.world_sheet_entry_id
WHERE kb.owner_kind <> 'world'
   OR kb.block_type <> 'character'
   OR kb.world_id IS NULL
   OR kb.world_id <> b.world_id;

-- Legacy visibility split, captured for the post-backfill equivalence check
-- (the old table and its column are gone after the rebuild).
CREATE TEMP TABLE _v1191_legacy_visibility_counts (
    owner_private INTEGER NOT NULL,
    shared INTEGER NOT NULL
);
INSERT INTO _v1191_legacy_visibility_counts
SELECT COALESCE(SUM(creator_only), 0),
       COUNT(*) - COALESCE(SUM(creator_only), 0)
FROM kb_key_blocks;

-- ── Registry ───────────────────────────────────────────────────────────────

CREATE TABLE knowledge_holders (
    holder_entry_id TEXT PRIMARY KEY
        CHECK (holder_entry_id LIKE 'hld_%'),
    creator_id TEXT
        REFERENCES creators (creator_id) ON DELETE RESTRICT,
    character_id TEXT
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (
        (creator_id IS NOT NULL AND creator_id <> '' AND character_id IS NULL)
        OR (creator_id IS NULL AND character_id IS NOT NULL AND character_id <> '')
    )
);

-- One holder per subject column (partial: the other column is always NULL).
CREATE UNIQUE INDEX idx_knowledge_holders_creator_unique
    ON knowledge_holders (creator_id) WHERE creator_id IS NOT NULL;
CREATE UNIQUE INDEX idx_knowledge_holders_character_unique
    ON knowledge_holders (character_id) WHERE character_id IS NOT NULL;

-- Backfill from the runner-staged digests (§2.1 recipe, computed in Rust).
INSERT INTO knowledge_holders (holder_entry_id, creator_id, created_at)
SELECT s.holder_entry_id, s.subject_id, datetime('now')
FROM _v1191_holder_digest_staging s
WHERE s.subject_kind = 'creator';

INSERT INTO knowledge_holders (holder_entry_id, character_id, created_at)
SELECT s.holder_entry_id, s.subject_id, datetime('now')
FROM _v1191_holder_digest_staging s
WHERE s.subject_kind = 'character';

-- ── Import quarantine (durable §6) ─────────────────────────────────────────

CREATE TABLE knowledge_import_quarantine (
    quarantine_id TEXT PRIMARY KEY,
    import_batch_id TEXT NOT NULL CHECK (import_batch_id <> ''),
    controlling_creator_id TEXT NOT NULL
        REFERENCES creators (creator_id) ON DELETE RESTRICT,
    owner_kind TEXT NOT NULL
        CHECK (owner_kind IN ('world', 'character', 'actor_world_binding')),
    world_id TEXT
        REFERENCES narrative_worlds (world_id) ON DELETE CASCADE,
    character_id TEXT
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT
        REFERENCES actor_world_bindings (binding_id) ON DELETE RESTRICT,
    quarantine_reason TEXT NOT NULL
        CHECK (quarantine_reason IN ('unresolved_holder', 'unknown_disclosure')),
    original_entry_json TEXT NOT NULL CHECK (original_entry_json <> ''),
    source_provenance_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (
        (owner_kind = 'world'
            AND world_id IS NOT NULL
            AND character_id IS NULL
            AND actor_world_binding_id IS NULL)
        OR (owner_kind = 'character'
            AND world_id IS NULL
            AND character_id IS NOT NULL
            AND actor_world_binding_id IS NULL)
        OR (owner_kind = 'actor_world_binding'
            AND world_id IS NULL
            AND character_id IS NULL
            AND actor_world_binding_id IS NOT NULL)
    )
);

CREATE INDEX idx_knowledge_import_quarantine_batch
    ON knowledge_import_quarantine (import_batch_id);
CREATE INDEX idx_knowledge_import_quarantine_controlling_creator
    ON knowledge_import_quarantine (controlling_creator_id);

-- ── kb_key_blocks: governance columns, rebuild, legacy column removal ──────
--
-- SQLite cannot drop a column's CHECK, so the table is rebuilt with the same
-- kb_key_blocks_new swap pattern as 20260905000002_actor_knowledge_owners.sql.
-- The named-column copy preserves every surviving column verbatim; only
-- `creator_only` (replaced by the governance pair) and the legacy extension
-- key change.

DROP TABLE IF EXISTS kb_key_blocks_new;

CREATE TABLE kb_key_blocks_new (
    key_block_id TEXT PRIMARY KEY CHECK (key_block_id LIKE 'kb_%'),
    owner_kind TEXT NOT NULL DEFAULT 'world'
        CHECK (owner_kind IN ('world', 'character', 'actor_world_binding')),
    world_id TEXT
        REFERENCES narrative_worlds (world_id) ON DELETE CASCADE,
    character_id TEXT
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT
        REFERENCES actor_world_bindings (binding_id) ON DELETE RESTRICT,
    holder_entry_id TEXT
        REFERENCES knowledge_holders (holder_entry_id) ON DELETE RESTRICT,
    disclosure TEXT
        CHECK (disclosure IS NULL OR disclosure = 'owner-private'),
    block_type TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('provisional', 'confirmed', 'deprecated', 'merged', 'deleted')),
    revision INTEGER,
    body_json TEXT,
    source_anchor_json TEXT,
    created_from_command_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    source_work_id TEXT,
    source_chapter INTEGER,
    source_provenance_kind TEXT
        CHECK (source_provenance_kind IS NULL
            OR source_provenance_kind IN (
                'manual',
                'review_time_extract',
                'finalize_time_extract',
                'cross_chapter_rescan',
                'author_explicit',
                'pack_import'
            )),
    extensions_nexus_json TEXT,
    modules_json TEXT,
    CHECK (
        (
            (owner_kind = 'world'
                AND world_id IS NOT NULL
                AND character_id IS NULL
                AND actor_world_binding_id IS NULL)
            OR (owner_kind = 'character'
                AND world_id IS NULL
                AND character_id IS NOT NULL
                AND actor_world_binding_id IS NULL)
            OR (owner_kind = 'actor_world_binding'
                AND world_id IS NULL
                AND character_id IS NULL
                AND actor_world_binding_id IS NOT NULL)
        )
        -- Shared is the absence of both columns; a disclosure always has its
        -- resolved holder, and empty strings are invalid.
        AND (holder_entry_id IS NULL OR holder_entry_id <> '')
        AND (holder_entry_id IS NOT NULL OR disclosure IS NULL)
    )
);

-- Copy + governance backfill: `creator_only = true` resolves the holder of the
-- stored World controlling Creator; shared rows stay NULL/NULL.
INSERT INTO kb_key_blocks_new
    (key_block_id, owner_kind, world_id, character_id, actor_world_binding_id,
     holder_entry_id, disclosure, block_type, canonical_name, status, revision,
     body_json, source_anchor_json, created_from_command_id, created_at,
     updated_at, source_work_id, source_chapter, source_provenance_kind,
     extensions_nexus_json, modules_json)
SELECT kb.key_block_id, kb.owner_kind, kb.world_id, kb.character_id,
       kb.actor_world_binding_id,
       CASE WHEN kb.creator_only = 1 THEN s.holder_entry_id ELSE NULL END,
       CASE WHEN kb.creator_only = 1 THEN 'owner-private' ELSE NULL END,
       kb.block_type, kb.canonical_name, kb.status, kb.revision,
       kb.body_json, kb.source_anchor_json, kb.created_from_command_id,
       kb.created_at, kb.updated_at, kb.source_work_id, kb.source_chapter,
       kb.source_provenance_kind,
       CASE
           WHEN kb.extensions_nexus_json IS NOT NULL
            AND json_type(kb.extensions_nexus_json, '$.creator_only') IS NOT NULL
           THEN json_remove(kb.extensions_nexus_json, '$.creator_only')
           ELSE kb.extensions_nexus_json
       END,
       kb.modules_json
FROM kb_key_blocks kb
LEFT JOIN narrative_worlds w ON w.world_id = kb.world_id
LEFT JOIN _v1191_holder_digest_staging s
       ON s.subject_kind = 'creator' AND s.subject_id = w.owner_creator_id;

DROP TABLE kb_key_blocks;
ALTER TABLE kb_key_blocks_new RENAME TO kb_key_blocks;

-- Recreate every pre-existing index (original names/SQL preserved), plus the
-- governance index. Owner/page container indexes are not replaced.
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_id
    ON kb_key_blocks (world_id);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_status
    ON kb_key_blocks (world_id, status);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_type
    ON kb_key_blocks (world_id, block_type);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_canonical_name
    ON kb_key_blocks (world_id, canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_active_unique
    ON kb_key_blocks (world_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_source_work_id
    ON kb_key_blocks (source_work_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_character_active_unique
    ON kb_key_blocks (character_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_binding_active_unique
    ON kb_key_blocks (actor_world_binding_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_character_id
    ON kb_key_blocks (character_id);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_actor_world_binding_id
    ON kb_key_blocks (actor_world_binding_id);
-- Governance lookup: holder-first so it also serves the RESTRICT FK, the
-- owner-private-by-holder visibility predicate and the disclosure fold.
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_holder_governance
    ON kb_key_blocks (holder_entry_id, disclosure);

-- Table-attached triggers are dropped with the old table, so the complete
-- inventory from 20260912000001_core_writer_protocol.sql is recreated verbatim
-- here: the revision bump, the three writer guards and the three outbox
-- emitters. Missing guards would let unregistered/old writers mutate the
-- rebuilt table; a missing outbox emitter would silently stop broadcasting
-- changes. (FK references to `kb_key_blocks` live in child tables and survive
-- the swap by name, and no other table has triggers referencing it.)

CREATE TRIGGER IF NOT EXISTS bump_kb_key_blocks_revision
AFTER UPDATE ON kb_key_blocks
FOR EACH ROW
WHEN NEW.revision = OLD.revision
BEGIN
  UPDATE kb_key_blocks SET revision = revision + 1 WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER IF NOT EXISTS outbox_kb_key_blocks_insert
AFTER INSERT ON kb_key_blocks
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES (COALESCE(NEW.world_id, ''), 'kb_key_blocks', NEW.key_block_id, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_kb_key_blocks_update
AFTER UPDATE ON kb_key_blocks
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES (COALESCE(NEW.world_id, ''), 'kb_key_blocks', NEW.key_block_id, CAST(NEW.revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_kb_key_blocks_delete
AFTER DELETE ON kb_key_blocks
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES (COALESCE(OLD.world_id, ''), 'kb_key_blocks', OLD.key_block_id, CAST(OLD.revision AS TEXT), 'delete', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS guard_kb_key_blocks_insert
BEFORE INSERT ON kb_key_blocks
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_kb_key_blocks_update
BEFORE UPDATE ON kb_key_blocks
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_kb_key_blocks_delete
BEFORE DELETE ON kb_key_blocks
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

-- ── Knowledge revisions (durable §4.3) ─────────────────────────────────────

ALTER TABLE narrative_worlds
    ADD COLUMN knowledge_revision INTEGER NOT NULL DEFAULT 0
        CHECK (knowledge_revision >= 0);
ALTER TABLE characters
    ADD COLUMN knowledge_revision INTEGER NOT NULL DEFAULT 0
        CHECK (knowledge_revision >= 0);

-- ── Post-backfill validation (inside the migration transaction) ────────────

CREATE TEMP TABLE _postcheck_registry_completeness (violations INTEGER NOT NULL);
INSERT INTO _postcheck_registry_completeness
SELECT
    CASE WHEN (SELECT COUNT(*) FROM knowledge_holders)
              = (SELECT COUNT(*) FROM _v1191_holder_digest_staging)
         THEN 0 ELSE NULL END;

-- Visibility equivalence: exactly the legacy split, one row at a time.
CREATE TEMP TABLE _postcheck_visibility_equivalence (violations INTEGER NOT NULL);
INSERT INTO _postcheck_visibility_equivalence
SELECT
    CASE
        WHEN (SELECT COUNT(*) FROM kb_key_blocks WHERE disclosure = 'owner-private')
             = (SELECT owner_private FROM _v1191_legacy_visibility_counts)
         AND (SELECT COUNT(*) FROM kb_key_blocks WHERE disclosure IS NULL)
             = (SELECT shared FROM _v1191_legacy_visibility_counts)
        THEN 0 ELSE NULL
    END;

-- Every private row's holder is the holder of its own World controlling Creator.
CREATE TEMP TABLE _postcheck_private_holder_binding (violations INTEGER NOT NULL);
INSERT INTO _postcheck_private_holder_binding
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks kb
WHERE kb.disclosure = 'owner-private'
  AND NOT EXISTS (
      SELECT 1
      FROM _v1191_holder_digest_staging s
      JOIN narrative_worlds w ON w.world_id = kb.world_id
      WHERE s.subject_kind = 'creator'
        AND s.subject_id = w.owner_creator_id
        AND s.holder_entry_id = kb.holder_entry_id);

-- The legacy extension key is gone (it is not unknown passthrough).
CREATE TEMP TABLE _postcheck_legacy_extension_removed (violations INTEGER NOT NULL);
INSERT INTO _postcheck_legacy_extension_removed
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM kb_key_blocks
WHERE json_type(extensions_nexus_json, '$.creator_only') IS NOT NULL;

-- No WorldSheet-linked row is private (durable §3).
CREATE TEMP TABLE _postcheck_world_sheet_disclosure (violations INTEGER NOT NULL);
INSERT INTO _postcheck_world_sheet_disclosure
SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE NULL END FROM actor_world_bindings b
JOIN kb_key_blocks kb ON kb.key_block_id = b.world_sheet_entry_id
WHERE kb.disclosure IS NOT NULL;

-- ── Writer guards for the new persistent tables ────────────────────────────
--
-- Same admission class as `kb_key_blocks` (direct | migration | current engine
-- owner). Created after the backfill above, so the migration's own registry
-- writes need no registration.

CREATE TRIGGER IF NOT EXISTS guard_knowledge_holders_insert
BEFORE INSERT ON knowledge_holders
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_knowledge_holders_update
BEFORE UPDATE ON knowledge_holders
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_knowledge_holders_delete
BEFORE DELETE ON knowledge_holders
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_knowledge_import_quarantine_insert
BEFORE INSERT ON knowledge_import_quarantine
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_knowledge_import_quarantine_update
BEFORE UPDATE ON knowledge_import_quarantine
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_knowledge_import_quarantine_delete
BEFORE DELETE ON knowledge_import_quarantine
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'WRITER_FENCED')
  WHERE COALESCE(nexus_writer_protocol(), 0) != 1
     OR NOT EXISTS (
         SELECT 1
         FROM core_writer_registration r
         INNER JOIN core_workspace_gate g ON g.pk = 1
         WHERE r.writer_id = nexus_writer_id()
           AND r.migration_epoch = g.migration_epoch
           AND r.migration_epoch = nexus_migration_epoch()
           AND (
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

-- ── Restore enforcement and verify referential integrity ───────────────────

PRAGMA foreign_keys=ON;
PRAGMA foreign_key_check;

-- Connection-local staging and preflight/post-check scratch leave no trace
-- (the runner's transaction owns them, and an aborted run rolls their creation
-- back; this is belt and braces for the pooled connection).
DROP TABLE IF EXISTS _v1191_holder_digest_staging;
DROP TABLE IF EXISTS _v1191_legacy_visibility_counts;
DROP TABLE IF EXISTS _preflight_holder_digest_staging_present;
DROP TABLE IF EXISTS _preflight_staging_creators_complete;
DROP TABLE IF EXISTS _preflight_staging_characters_complete;
DROP TABLE IF EXISTS _preflight_staging_orphans;
DROP TABLE IF EXISTS _preflight_preexisting_registry;
DROP TABLE IF EXISTS _preflight_preexisting_governance_columns;
DROP TABLE IF EXISTS _preflight_legacy_bool_domain;
DROP TABLE IF EXISTS _preflight_malformed_extensions;
DROP TABLE IF EXISTS _preflight_legacy_extension_conflict;
DROP TABLE IF EXISTS _preflight_unresolvable_world_creator;
DROP TABLE IF EXISTS _preflight_world_sheet_privatized;
DROP TABLE IF EXISTS _preflight_world_sheet_illegal;
DROP TABLE IF EXISTS _postcheck_registry_completeness;
DROP TABLE IF EXISTS _postcheck_visibility_equivalence;
DROP TABLE IF EXISTS _postcheck_private_holder_binding;
DROP TABLE IF EXISTS _postcheck_legacy_extension_removed;
DROP TABLE IF EXISTS _postcheck_world_sheet_disclosure;
