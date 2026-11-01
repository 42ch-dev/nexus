-- v1.227 NXM-2: provider-owned metadata (spoke-memory §6.10/MEM-R021).
--
-- The provider owns exactly one durable scalar it must not lose across a
-- restart: the high-water mark of its clock. Retention is one-way — a receipt
-- body past `receipt_retention_seconds` is never served again — and the reading
-- that decided it must outlive the process, or a reopened provider would seed
-- its clock from the wall clock alone and a wall clock that moved backwards
-- could re-open a window the previous process already closed.
--
-- The row is written by the retention sweep itself, in the same transaction as
-- the eviction it decided, so the clock reading and its effects commit together.
-- Key/value so the provider can own further scalars without a rebuild; the
-- `memory_*` prefix is the provider's namespace, and the family carries the same
-- cooperative core writer protocol as the content and control tables.

CREATE TABLE IF NOT EXISTS memory_provider_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
) WITHOUT ROWID;

-- ── Writer-protocol guards (cooperative core-table shape) ──

CREATE TRIGGER IF NOT EXISTS guard_memory_provider_meta_insert
BEFORE INSERT ON memory_provider_meta
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

CREATE TRIGGER IF NOT EXISTS guard_memory_provider_meta_update
BEFORE UPDATE ON memory_provider_meta
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

CREATE TRIGGER IF NOT EXISTS guard_memory_provider_meta_delete
BEFORE DELETE ON memory_provider_meta
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

-- ── Outbox family: one event per control-plane mutation ──

CREATE TRIGGER IF NOT EXISTS outbox_memory_provider_meta_insert
AFTER INSERT ON memory_provider_meta
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_provider_meta', NEW.key, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_provider_meta_update
AFTER UPDATE ON memory_provider_meta
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_provider_meta', NEW.key, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_provider_meta_delete
AFTER DELETE ON memory_provider_meta
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_provider_meta', OLD.key, NULL, 'delete', nexus_writer_id());
END;
