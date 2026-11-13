-- v1.239 P3 T1: mutable candidate strategy drafts (`strategy_candidates`).
--
-- Plan: .mstar/plans/v239-p3-compose-ops.md §Candidate storage (Q4) and D17.
--
-- One active draft per strategy: `strategy_id` is the primary key and
-- `revision` is the SINGLE compare-and-swap authority. `document_json` holds
-- the complete `WorkflowDocument` with `state: candidate`; the store mirrors
-- the new row revision into the embedded `document.revision` in the same
-- statement, and the CHECKs below make a row whose document disagrees
-- unrepresentable. The canonical content hash is recomputed from
-- `document_json` through `nexus-workflow-ir`, so it is deliberately NOT a
-- column. `created_at` / `updated_at` describe the draft and never take part
-- in identity.
--
-- A missing row is not an error: reads project the `workflow-ir` import at
-- revision 0 and persist nothing, and the first compose with
-- `base_revision: 0` inserts revision 1.
--
-- Writer admission: cooperative core table — the same guard and outbox shape
-- as the other cooperative core tables. A raw/unregistered pool is fenced with
-- `WRITER_FENCED`, and every mutation publishes one `core_changes` event.

CREATE TABLE IF NOT EXISTS strategy_candidates (
    strategy_id TEXT PRIMARY KEY,
    document_json TEXT NOT NULL CHECK (json_valid(document_json)),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (json_extract(document_json, '$.revision') = revision),
    CHECK (json_extract(document_json, '$.id') = strategy_id),
    CHECK (json_extract(document_json, '$.state') = 'candidate')
) WITHOUT ROWID;

-- ── Writer-protocol guards: strategy_candidates ──

CREATE TRIGGER IF NOT EXISTS guard_strategy_candidates_insert
BEFORE INSERT ON strategy_candidates
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_candidates_update
BEFORE UPDATE ON strategy_candidates
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_candidates_delete
BEFORE DELETE ON strategy_candidates
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

-- ── Outbox: strategy_candidates ──

CREATE TRIGGER IF NOT EXISTS outbox_strategy_candidates_insert
AFTER INSERT ON strategy_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_candidates', NEW.strategy_id, CAST(NEW.revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_candidates_update
AFTER UPDATE ON strategy_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_candidates', NEW.strategy_id, CAST(NEW.revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_candidates_delete
AFTER DELETE ON strategy_candidates
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_candidates', OLD.strategy_id, CAST(OLD.revision AS TEXT), 'delete', nexus_writer_id());
END;
