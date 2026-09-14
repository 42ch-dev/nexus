-- V1.189 P4-T2 (LIFE-3): durable JS-provider operation journal.
--
-- The in-memory `JsProviderState` cannot survive a process exit. This journal
-- is the Rust-owned durable record of JS-provider operations so that, after the
-- predecessor standalone Node process exits and a new process opens the same
-- home, a previously active non-resumable operation is still queryable by the
-- same operation id — reported as `interrupted` rather than 404 or a fabricated
-- terminal.
--
-- Columns:
--   operation_id — the provider operation id (opaque string, PRIMARY KEY)
--   session_id   — owning provider session id (opaque string)
--   provider_id  — admitted provider id
--   status       — running | finished | failed | interrupted | cancelled
--   sequence     — monotonic journal order (retained; newest pruned last)
--   updated_at   — last write timestamp
CREATE TABLE IF NOT EXISTS js_provider_operation_journal (
    operation_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    status TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_js_provider_journal_sequence
    ON js_provider_operation_journal (sequence);

CREATE INDEX IF NOT EXISTS idx_js_provider_journal_status
    ON js_provider_operation_journal (status);

-- Writer-protocol guards. The journal is engine-owned state written by the
-- native engine-owner core pool, so it is admitted only for the single engine
-- owner (or the migration writer), matching the `orchestration_sessions` class.
CREATE TRIGGER IF NOT EXISTS guard_js_provider_operation_journal_insert
BEFORE INSERT ON js_provider_operation_journal
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
             r.mode = 'migration'
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_js_provider_operation_journal_update
BEFORE UPDATE ON js_provider_operation_journal
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
             r.mode = 'migration'
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;

CREATE TRIGGER IF NOT EXISTS guard_js_provider_operation_journal_delete
BEFORE DELETE ON js_provider_operation_journal
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
             r.mode = 'migration'
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );
END;
