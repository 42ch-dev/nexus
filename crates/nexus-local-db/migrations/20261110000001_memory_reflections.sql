-- v1.230 N4-R: append-only reflection envelopes (spoke-memory §7.25–§7.27).
--
-- One row per generated reflection. Everything the wire `Reflection` serves is
-- pinned at generation: the request's selection/filter shapes travel verbatim
-- (published array shapes preserved; an absent filter is SQL NULL, never a
-- `null`-substituted array), the four `ReflectionBasis` members are one JSON
-- object, and the generator names its policy version and model. Rows are
-- immutable: a reflection is never updated, deleted or swept — §7.26 staleness
-- is recomputed at read time against the pinned basis, so `stale_cache` is only
-- the write-time flag the insert recorded.
--
-- The envelope body lives only in SQLite (no journal intent: there is no file
-- half), and generation reuses the §6.10 `memory_receipts` machinery — this
-- table carries no receipt columns of its own.
--
-- `seq` is the newest-first read order: `created_at` is a whole-second stamp,
-- so two envelopes generated in the same second would otherwise tie and fall
-- back to the random `reflection_id` tiebreak, which is not a deterministic
-- newest-first listing. The store assigns it (`COALESCE(MAX(seq), 0) + 1`, the
-- same shape as `memory_journal`), never SQLite — the table is WITHOUT ROWID,
-- so a rowid alias would reintroduce the silent-delete trigger bypass the
-- journal migration documents.
--
-- Writer admission: a cooperative core table with the same guard trio as the
-- other memory tables, so a raw pool is fenced with `WRITER_FENCED` on every
-- mutation shape. The family is append-only in practice (no provider update or
-- delete path exists), but the writer-protocol contract is per-table guard
-- coverage, and an outbox event rides every admitted insert.

CREATE TABLE IF NOT EXISTS memory_reflections (
    seq INTEGER NOT NULL UNIQUE,
    reflection_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    selection_json TEXT NOT NULL,
    filter_json TEXT,
    cognition_filter_json TEXT,
    basis_json TEXT NOT NULL,
    engine_policy_version TEXT NOT NULL,
    model TEXT,
    schema_version INTEGER NOT NULL CHECK (schema_version >= 1),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    extensions_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    body_text TEXT NOT NULL,
    body_format TEXT NOT NULL CHECK (body_format IN ('markdown', 'json')),
    cost_unit_kind TEXT,
    cost_units REAL,
    -- §7.26: write-time cache only; reads recompute against the pinned basis.
    stale_cache INTEGER NOT NULL DEFAULT 0,
    -- The wire `cost` is one optional object: both halves or neither.
    CHECK ((cost_unit_kind IS NULL) = (cost_units IS NULL))
) WITHOUT ROWID;

-- §7.26 list/state reads over a subject's reflections: newest-first by the
-- store-assigned insertion order.
CREATE INDEX IF NOT EXISTS idx_memory_reflections_subject_seq
    ON memory_reflections (space_id, subject_id, seq);

-- Identity lookups scoped to one subject.
CREATE INDEX IF NOT EXISTS idx_memory_reflections_subject_reflection
    ON memory_reflections (space_id, subject_id, reflection_id);

-- ── Writer-protocol guards (full cooperative core-table shape) ──

CREATE TRIGGER IF NOT EXISTS guard_memory_reflections_insert
BEFORE INSERT ON memory_reflections
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

CREATE TRIGGER IF NOT EXISTS guard_memory_reflections_update
BEFORE UPDATE ON memory_reflections
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

CREATE TRIGGER IF NOT EXISTS guard_memory_reflections_delete
BEFORE DELETE ON memory_reflections
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

CREATE TRIGGER IF NOT EXISTS outbox_memory_reflections_insert
AFTER INSERT ON memory_reflections
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_reflections', NEW.reflection_id, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;
