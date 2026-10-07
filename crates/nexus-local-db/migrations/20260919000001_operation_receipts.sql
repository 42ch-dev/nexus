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
-- Payload contract, verbatim from §B.2 ("terminal payload, exactly one set on
-- terminal settlement"): a `running` receipt carries neither payload and no
-- terminal stamp; every terminal receipt carries a terminal stamp and
-- EXACTLY ONE of `result_json` / `error_json`. Which column a terminal uses
-- is the store's rendering (`finished` ⇒ `result_json`; `failed` /
-- `cancelled` / `interrupted` ⇒ `error_json`, the non-success reason), not a
-- schema-level status constraint.
--
-- Terminal receipts are immutable: the `immutable_terminal_operation_receipts`
-- trigger below refuses EVERY update of an already-terminal row, so the
-- settled answer a replay is served from can never be rewritten — neither
-- downgraded back to `running`, nor rewritten from `finished` to `failed`, nor
-- given a replacement payload. Only the `running` → terminal settlement
-- transition is possible (`settle_operation` gates its UPDATE on
-- `status = 'running'`), and the writer-protocol guard below still governs the
-- insert.

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
        OR (
            status IN ('finished', 'failed', 'cancelled', 'interrupted')
            AND terminal_at IS NOT NULL
            AND ((result_json IS NULL) <> (error_json IS NULL))
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

-- Terminal immutability: a settled receipt is the durable answer a replay is
-- served from, so NO subsequent mutation of it is admitted — not a downgrade
-- back to `running`, not a `finished` → `failed` rewrite, not a payload
-- replacement under the same status, and not its removal (a deleted receipt
-- would read as "no receipt" and invite exactly the double-apply §B.3 exists
-- to prevent). The only reachable transition is `running` → terminal (the
-- store's UPDATE is gated on `status = 'running'`), so these triggers never
-- fire for a legitimate settlement.
CREATE TRIGGER IF NOT EXISTS immutable_terminal_operation_receipts_update
BEFORE UPDATE ON operation_receipts
FOR EACH ROW
WHEN OLD.status IN ('finished', 'failed', 'cancelled', 'interrupted')
BEGIN
  SELECT RAISE(ABORT, 'OPERATION_RECEIPT_TERMINAL_IMMUTABLE');
END;

CREATE TRIGGER IF NOT EXISTS immutable_terminal_operation_receipts_delete
BEFORE DELETE ON operation_receipts
FOR EACH ROW
WHEN OLD.status IN ('finished', 'failed', 'cancelled', 'interrupted')
BEGIN
  SELECT RAISE(ABORT, 'OPERATION_RECEIPT_TERMINAL_IMMUTABLE');
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
