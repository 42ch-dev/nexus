-- v1.227 NXM-1 (round B): memory content metadata (spoke-memory §6.6/§6.7/§6.8).
--
-- Additive only — adoption plan D-1: the legacy content families
-- (`memory_pending_review`, `memory_fragments`, `*_soul_narratives*`,
-- `character_run_captures`, `character_memory_*`) and the bearer markdown layout
-- are not touched, neither structurally nor in content. Their readers keep the
-- exact shapes they had. What lands here are the provider's own content tables,
-- carrying from their first row the control metadata the protocol requires and
-- the legacy tables cannot express (their keys are session/character-scoped and
-- they hold neither placement, kind, provenance, revision nor tombstone state):
--
--   memory_candidates   pending/captured review queue (§6.7). `capture_scope` is
--                       the immutable admitted placement and the authorization
--                       fence for review outcomes; consumption scrubs the body
--                       and keeps the shell plus `content_digest` (MEM-R009).
--   memory_records      retained records (§6.6): untrusted content, write
--                       placement outside the content, provenance independent of
--                       placement, monotonic `revision` (MEM-R007) and a
--                       tombstone that removes content while keeping the row for
--                       audit (MEM-R014).
--   memory_profiles     trusted authored subject documents (§6.8), one row per
--                       (space, subject, profile_kind); content kinds only,
--                       never provider policy (D-12).
--
-- Writer admission: all three are cooperative core tables — the same guard and
-- outbox shape as the round-A control tables. A raw pool with no registration
-- row is fenced with `WRITER_FENCED`, and every mutation publishes one
-- `core_changes` outbox event.
--
-- Revision columns are the CAS surface (MEM-R007): compare and bump happen in
-- one statement pair inside the effect transaction; there is no bump trigger,
-- so a stale writer can never be told "matched" by a trigger its own WHERE
-- rejected. Stable identity: `candidate_id` / `record_id` are minted once and
-- never reused, and the content of a consumed candidate is dropped in place
-- under that identity.

CREATE TABLE IF NOT EXISTS memory_candidates (
    candidate_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    -- `capture_scope` (§6.7) split into its two queryable facets; the wire
    -- projection rebuilds the object from them.
    capture_visibility TEXT NOT NULL CHECK (capture_visibility IN ('shared', 'context')),
    capture_context_id TEXT,
    -- Candidate kind: kept after the scrub because the shell and the review
    -- default still name it.
    kind TEXT NOT NULL,
    -- §6.7 proposed content; NULL once consumed (the shell keeps no body).
    content_json TEXT,
    provenance_json TEXT NOT NULL,
    classification_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'consumed')),
    consumed_by_json TEXT,
    content_digest TEXT,
    occurred_at TEXT,
    -- Pending-only expiry (§6.7): an expired pending candidate is excluded from
    -- lists, never consumed.
    expires_at TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    CHECK ((capture_visibility = 'context') = (capture_context_id IS NOT NULL)),
    -- §6.7/§8.1 consumption is one-way and all-or-nothing: the body, the digest
    -- and the outcome arrive (and stay) together.
    CHECK ((status = 'consumed') = (consumed_by_json IS NOT NULL)),
    CHECK ((status = 'consumed') = (content_json IS NULL)),
    CHECK ((status = 'consumed') = (content_digest IS NOT NULL))
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_memory_candidates_subject_status
    ON memory_candidates (space_id, subject_id, status);

CREATE TABLE IF NOT EXISTS memory_records (
    record_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    content_json TEXT NOT NULL,
    scope_visibility TEXT NOT NULL CHECK (scope_visibility IN ('shared', 'context')),
    scope_context_id TEXT,
    provenance_json TEXT NOT NULL,
    import_ref_json TEXT,
    occurred_at TEXT,
    -- §6.6 tombstone; content is emptied in place when it appears (MEM-R014).
    tombstone_json TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((scope_visibility = 'context') = (scope_context_id IS NOT NULL))
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_memory_records_subject_created
    ON memory_records (space_id, subject_id, created_at DESC, record_id);

CREATE TABLE IF NOT EXISTS memory_profiles (
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    profile_kind TEXT NOT NULL,
    document_json TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (space_id, subject_id, profile_kind)
) WITHOUT ROWID;

-- ── Writer-protocol guards: memory_candidates ──

CREATE TRIGGER IF NOT EXISTS guard_memory_candidates_insert
BEFORE INSERT ON memory_candidates
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

CREATE TRIGGER IF NOT EXISTS guard_memory_candidates_update
BEFORE UPDATE ON memory_candidates
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

CREATE TRIGGER IF NOT EXISTS guard_memory_candidates_delete
BEFORE DELETE ON memory_candidates
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

-- ── Outbox: memory_candidates ──

CREATE TRIGGER IF NOT EXISTS outbox_memory_candidates_insert
AFTER INSERT ON memory_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_candidates', NEW.candidate_id, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_candidates_update
AFTER UPDATE ON memory_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_candidates', NEW.candidate_id, CAST(NEW.revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_candidates_delete
AFTER DELETE ON memory_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_candidates', OLD.candidate_id, CAST(OLD.revision AS TEXT), 'delete', nexus_writer_id());
END;

-- ── Writer-protocol guards: memory_records ──

CREATE TRIGGER IF NOT EXISTS guard_memory_records_insert
BEFORE INSERT ON memory_records
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

CREATE TRIGGER IF NOT EXISTS guard_memory_records_update
BEFORE UPDATE ON memory_records
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

CREATE TRIGGER IF NOT EXISTS guard_memory_records_delete
BEFORE DELETE ON memory_records
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

-- ── Outbox: memory_records ──

CREATE TRIGGER IF NOT EXISTS outbox_memory_records_insert
AFTER INSERT ON memory_records
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_records', NEW.record_id, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_records_update
AFTER UPDATE ON memory_records
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_records', NEW.record_id, CAST(NEW.revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_records_delete
AFTER DELETE ON memory_records
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_records', OLD.record_id, CAST(OLD.revision AS TEXT), 'delete', nexus_writer_id());
END;

-- ── Writer-protocol guards: memory_profiles ──

CREATE TRIGGER IF NOT EXISTS guard_memory_profiles_insert
BEFORE INSERT ON memory_profiles
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

CREATE TRIGGER IF NOT EXISTS guard_memory_profiles_update
BEFORE UPDATE ON memory_profiles
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

CREATE TRIGGER IF NOT EXISTS guard_memory_profiles_delete
BEFORE DELETE ON memory_profiles
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

-- ── Outbox: memory_profiles ──
--
-- The row identity is the composite primary key, rendered the way every other
-- composite-key outbox in this schema renders it (`a || ':' || b`), so a change
-- consumer can address exactly the profile row that changed instead of every
-- profile of that kind.

CREATE TRIGGER IF NOT EXISTS outbox_memory_profiles_insert
AFTER INSERT ON memory_profiles
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_profiles', NEW.space_id || ':' || NEW.subject_id || ':' || NEW.profile_kind, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_profiles_update
AFTER UPDATE ON memory_profiles
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_profiles', NEW.space_id || ':' || NEW.subject_id || ':' || NEW.profile_kind, CAST(NEW.revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_profiles_delete
AFTER DELETE ON memory_profiles
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_profiles', OLD.space_id || ':' || OLD.subject_id || ':' || OLD.profile_kind, CAST(OLD.revision AS TEXT), 'delete', nexus_writer_id());
END;
