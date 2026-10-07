-- v1.207 P3 (RN-OGA-5): durable operation receipts.
--
-- Spec: .mstar/iterations/v1.207/specs/connect-replay-and-operation-receipts.md
-- §B.2 (this table, its CHECKs, and the engine-owned writer-protocol family)
-- + §B.3 (the frozen Recover handshake that reads it).
--
-- One row per logical operation, first-writer-wins on `operation_id`:
-- the Recover path asks this table BEFORE any re-apply, so a retry/replay
-- after cancel, timeout or crash is answered from the stored receipt
-- instead of being applied a second time. `request_fingerprint` is what
-- distinguishes a replay (same fingerprint — the caller may resume from
-- the receipt) from a genuine id collision (different fingerprint — a
-- typed `operation_id_conflict` refusal, never a silent dedupe).
--
-- Engine-owned under the core writer protocol, exactly like
-- `compute_sessions`: the guard family admits only the migration writer and
-- the single engine owner, and the outbox family mirrors every mutation onto
-- `core_changes` (`resource_kind = 'operation_receipts'`).
--
-- The payload CHECK is the durable half of §B.2's "terminal payload, exactly
-- one set on terminal settlement": `finished` carries `result_json` and no
-- `error_json`, `failed` carries `error_json` and no `result_json`, and the
-- payload-free terminals (`cancelled`, `interrupted`) carry neither.
-- `terminal_at` is set exactly when the receipt is terminal, and a `running`
-- receipt can never carry a terminal payload — so a receipt cannot be
-- laundered back into an unsettled shape, and the no-downgrade trigger below
-- refuses a settled receipt being put back to `running` at all.

CREATE TABLE IF NOT EXISTS operation_receipts (
    operation_id TEXT PRIMARY KEY,
    consumer TEXT NOT NULL CHECK (consumer IN ('compute_run', 'connect_invoke')),
    subject_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'finished', 'failed', 'cancelled', 'interrupted')),
    request_fingerprint TEXT NOT NULL,
    result_json TEXT,
    error_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    terminal_at TEXT,
    sequence INTEGER NOT NULL,
    CHECK (
        (status = 'running' AND result_json IS NULL AND error_json IS NULL AND terminal_at IS NULL)
        OR (status = 'finished' AND result_json IS NOT NULL AND error_json IS NULL AND terminal_at IS NOT NULL)
        OR (status = 'failed' AND error_json IS NOT NULL AND result_json IS NULL AND terminal_at IS NOT NULL)
        OR (
            status IN ('cancelled', 'interrupted')
            AND result_json IS NULL AND error_json IS NULL AND terminal_at IS NOT NULL
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_operation_receipts_sequence
    ON operation_receipts (sequence);

CREATE INDEX IF NOT EXISTS idx_operation_receipts_consumer_subject
    ON operation_receipts (consumer, subject_id);

-- Writer-protocol guards: admitted only for the single engine owner or the
-- migration writer, mirroring `guard_compute_sessions_*`.
CREATE TRIGGER IF NOT EXISTS guard_operation_receipts_insert
BEFORE INSERT ON operation_receipts
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

CREATE TRIGGER IF NOT EXISTS guard_operation_receipts_update
BEFORE UPDATE ON operation_receipts
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

CREATE TRIGGER IF NOT EXISTS guard_operation_receipts_delete
BEFORE DELETE ON operation_receipts
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

-- Terminal is never downgraded back to `running` (§B.2): the schema refuses
-- the downgrade itself, so no writer path — including a future direct one —
-- can resurrect a settled receipt. Settling only ever transitions a
-- `running` row (the store's UPDATE is gated on `status = 'running'`), so
-- this trigger never fires for a legitimate settlement.
CREATE TRIGGER IF NOT EXISTS no_downgrade_operation_receipts
BEFORE UPDATE ON operation_receipts
FOR EACH ROW
WHEN OLD.status IN ('finished', 'failed', 'cancelled', 'interrupted')
     AND NEW.status = 'running'
BEGIN
  SELECT RAISE(ABORT, 'OPERATION_RECEIPT_TERMINAL_DOWNGRADE');
END;

-- Outbox family: one `core_changes` event per receipt mutation, mirroring
-- `outbox_compute_sessions_*`.
CREATE TRIGGER IF NOT EXISTS outbox_operation_receipts_insert
AFTER INSERT ON operation_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'operation_receipts', NEW.operation_id, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_operation_receipts_update
AFTER UPDATE ON operation_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'operation_receipts', NEW.operation_id, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_operation_receipts_delete
AFTER DELETE ON operation_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'operation_receipts', OLD.operation_id, NULL, 'delete', nexus_writer_id());
END;
