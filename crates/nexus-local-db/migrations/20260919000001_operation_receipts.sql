-- v1.207 P3 (RN-OGA-5): durable operation receipts.
--
-- Spec: .mstar/iterations/v1.207/specs/connect-replay-and-operation-receipts.md
-- §B.2 (this table, its CHECKs, and the cooperative writer-protocol family)
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
-- Writer admission (spec §B.2, ruled 2026-10-07 third amendment): this is a
-- **cooperative core table**, not an engine-owned one. It is shared by the
-- compute lane (engine writer) and the Connect host (cooperative DIRECT
-- writer), so the guard family admits the migration writer, the epoch-matched
-- single engine owner, AND the cooperative DIRECT admission — byte-for-byte
-- the `guard_knowledge_entries_*` shape.
--
-- Receipt integrity does not rest on the admission class: it is enforced BELOW
-- the guard and is writer-agnostic — terminal immutability fires for every
-- admitted writer, first-writer-wins is the `operation_id` PRIMARY KEY plus the
-- store's fingerprint-conflict check (arbitrated by SQLite's single-writer
-- serialization), the payload CHECK binds all writers, a raw/unregistered pool
-- stays fenced (no registration row ⇒ WRITER_FENCED), and a DIRECT
-- registration pins `migration_epoch` so a writer spanning a migration is
-- fenced by the cooperative-quiescence protocol. `compute_sessions` stays
-- engine-owned — its session state machine wants the single-owner CAS
-- discipline.
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
) WITHOUT ROWID;
-- WITHOUT ROWID (QC1-C001 residual): a rowid column is an ALIAS for the row's
-- storage id, and neither an `INSERT OR REPLACE` carrying an explicit rowid nor
-- an `UPDATE OR REPLACE … SET rowid = …` fires the victim's DELETE trigger while
-- `recursive_triggers` is OFF — so a rowid alias would be a second, unfenced way
-- to delete a settled receipt. The table's identity is `operation_id` alone
-- (nothing in the store, the outbox family or any caller reads a rowid; the
-- `sequence` column is assigned by the store, not by AUTOINCREMENT), so removing
-- the alias removes the bypass class outright rather than guarding around it.

CREATE INDEX IF NOT EXISTS idx_operation_receipts_sequence
    ON operation_receipts (sequence);

CREATE INDEX IF NOT EXISTS idx_operation_receipts_consumer_subject
    ON operation_receipts (consumer, subject_id);

-- Writer-protocol guards: the cooperative core-table shape — admitted for the
-- migration writer, the cooperative DIRECT admission, or the epoch-matched
-- single engine owner (mirroring `guard_knowledge_entries_*`; see the
-- writer-admission note in this file's header).
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
             r.mode IN ('direct', 'migration')
             OR (
               r.mode = 'engine'
               AND r.engine_epoch IS NOT NULL
               AND r.engine_epoch = g.engine_epoch
               AND r.engine_epoch = nexus_engine_epoch()
             )
           )
     );

  -- A settled answer is never deleted-and-reinserted (QC1-C001). An
  -- `INSERT OR REPLACE` (or `REPLACE INTO`) over a terminal id resolves the
  -- conflict by DELETING the existing row, and that victim's DELETE trigger
  -- does NOT fire while `recursive_triggers` is OFF (the default) — so the
  -- immutability triggers alone cannot see it. This BEFORE INSERT guard fires
  -- for the replacement attempt itself, BEFORE the victim is destroyed, and
  -- refuses it. The legitimate `INSERT … ON CONFLICT(operation_id) DO NOTHING`
  -- replay of a running row is unaffected (its conflict row is `running`), and
  -- the store never inserts over an existing row at all (it reads first).
  SELECT RAISE(ABORT, 'OPERATION_RECEIPT_TERMINAL_IMMUTABLE')
  WHERE EXISTS (
      SELECT 1
      FROM operation_receipts existing
      WHERE existing.operation_id = NEW.operation_id
        AND existing.status IN ('finished', 'failed', 'cancelled', 'interrupted')
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

-- The receipt KEY is immutable (QC1-C001). `UPDATE … SET operation_id = <other>`
-- would otherwise MOVE a receipt onto another id — and `UPDATE OR REPLACE` onto
-- a terminal id resolves that conflict by deleting the terminal victim, whose
-- DELETE trigger does not fire while `recursive_triggers` is OFF. Refusing any
-- key change closes both the settlement hijack (a `running` source claiming a
-- settled id) and the victim deletion at the schema boundary. The legitimate
-- settlement never touches the key, so it always passes.
CREATE TRIGGER IF NOT EXISTS immutable_receipt_key_operation_receipts_update
BEFORE UPDATE ON operation_receipts
FOR EACH ROW
WHEN NEW.operation_id IS NOT OLD.operation_id
BEGIN
  SELECT RAISE(ABORT, 'OPERATION_RECEIPT_KEY_IMMUTABLE');
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
