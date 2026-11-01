-- v1.227 NXM-2: memory receipt tombstones (spoke-memory §6.10/MEM-R021).
--
-- §6.10 retires a receipt *body* after `receipt_retention_seconds` and keeps
-- only a content-free dedup tombstone — namespace key, `payload_digest`,
-- `receipt_id`, terminal status — for `dedup_retention_seconds`. The round-A
-- CHECK required every settled receipt to carry exactly one payload, so a body
-- could be hidden at read time but never removed from the row.
--
-- A tombstone is a settled row with no payload at all (`result_json` and
-- `error_json` both NULL); every other settled row still carries exactly one.
-- SQLite cannot alter a CHECK, so the table is rebuilt with SQLite's documented
-- rebuild sequence. The indexes and the writer-protocol guard/outbox families
-- are re-created *after* the row copy, so the copy emits no outbox events.

CREATE TABLE memory_receipts_tombstone (
    receipt_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    op TEXT NOT NULL,
    -- NULL iff the op is the sole §7.0.5 exemption (`memory.subjects.ensure`):
    -- ensure is not deduped at all, so two calls may both record a receipt.
    idempotency_key TEXT,
    payload_digest TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('committed', 'pending', 'failed')),
    result_json TEXT,
    error_json TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    -- §6.10 payload contract: a pending receipt carries no payload and no
    -- terminal stamp; a settled receipt carries one payload (served body) or
    -- none (body evicted, content-free tombstone), never both.
    CHECK (
        (status = 'pending' AND result_json IS NULL AND error_json IS NULL AND completed_at IS NULL)
        OR (
            status IN ('committed', 'failed')
            AND completed_at IS NOT NULL
            AND NOT (result_json IS NOT NULL AND error_json IS NOT NULL)
        )
    ),
    CHECK (
        (idempotency_key IS NULL) = (op = 'memory.subjects.ensure')
    )
) WITHOUT ROWID;

INSERT INTO memory_receipts_tombstone
    (receipt_id, space_id, requester_principal_id, op, idempotency_key, payload_digest,
     status, result_json, error_json, created_at, completed_at)
SELECT receipt_id, space_id, requester_principal_id, op, idempotency_key, payload_digest,
       status, result_json, error_json, created_at, completed_at
FROM memory_receipts;

DROP TABLE memory_receipts;

ALTER TABLE memory_receipts_tombstone RENAME TO memory_receipts;

-- The dedup namespace (§7.0.5). Partial: a keyless (ensure) receipt is outside
-- the namespace by definition, so it neither collides nor dedupes.
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_receipts_dedup_namespace
    ON memory_receipts (space_id, requester_principal_id, op, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_memory_receipts_space_status
    ON memory_receipts (space_id, status);

-- Retention sweep (§6.10): eviction and tombstone cleanup are range predicates
-- on the receipt's age, across every namespace and the keyless rows too.
CREATE INDEX IF NOT EXISTS idx_memory_receipts_created_at
    ON memory_receipts (created_at);

-- ── Writer-protocol guards (cooperative core-table shape) ──

CREATE TRIGGER IF NOT EXISTS guard_memory_receipts_insert
BEFORE INSERT ON memory_receipts
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

CREATE TRIGGER IF NOT EXISTS guard_memory_receipts_update
BEFORE UPDATE ON memory_receipts
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

CREATE TRIGGER IF NOT EXISTS guard_memory_receipts_delete
BEFORE DELETE ON memory_receipts
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

CREATE TRIGGER IF NOT EXISTS outbox_memory_receipts_insert
AFTER INSERT ON memory_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_receipts', NEW.receipt_id, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_receipts_update
AFTER UPDATE ON memory_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_receipts', NEW.receipt_id, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_receipts_delete
AFTER DELETE ON memory_receipts
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_receipts', OLD.receipt_id, NULL, 'delete', nexus_writer_id());
END;

-- ── Terminal immutability (§6.10/MEM-R021) ──
--
-- A settled receipt is the durable answer a replay and `memory.receipts.get`
-- are served from, so a later writer must not be able to rewrite it: no
-- `committed` → `failed` downgrade, no replacement payload under the same
-- status, no `payload_digest` relabel and no key change. The only admitted
-- transitions of a settled row are the ones the store performs — the journal's
-- quarantined → `committed` re-drive, and the retention sweep's one-way body
-- eviction (payload to NULL) — which is what these triggers allow.
CREATE TRIGGER IF NOT EXISTS immutable_settled_memory_receipts_update
BEFORE UPDATE ON memory_receipts
FOR EACH ROW
WHEN OLD.status IN ('committed', 'failed')
BEGIN
  SELECT RAISE(ABORT, 'MEMORY_RECEIPT_SETTLED_IMMUTABLE')
  WHERE NEW.receipt_id IS NOT OLD.receipt_id
     OR NEW.space_id IS NOT OLD.space_id
     OR NEW.requester_principal_id IS NOT OLD.requester_principal_id
     OR NEW.op IS NOT OLD.op
     OR NEW.idempotency_key IS NOT OLD.idempotency_key
     OR NEW.payload_digest IS NOT OLD.payload_digest
     OR NEW.created_at IS NOT OLD.created_at
     OR (NEW.status <> OLD.status AND NOT (OLD.status = 'failed' AND NEW.status = 'committed'))
     OR (
          NEW.status = OLD.status
          AND (
               (NEW.result_json IS NOT NULL AND NEW.result_json IS NOT OLD.result_json)
            OR (NEW.error_json IS NOT NULL AND NEW.error_json IS NOT OLD.error_json)
          )
        );
END;

-- `INSERT OR REPLACE` over a settled key resolves the conflict by deleting the
-- victim, and that victim's DELETE trigger does not fire while
-- `recursive_triggers` is OFF. This BEFORE INSERT guard refuses the replacement
-- attempt itself; the store never inserts over an existing receipt (it probes
-- first), so a legitimate write never trips it.
CREATE TRIGGER IF NOT EXISTS immutable_settled_memory_receipts_insert
BEFORE INSERT ON memory_receipts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM memory_receipts existing
    WHERE existing.receipt_id = NEW.receipt_id
      AND existing.status IN ('committed', 'failed')
)
BEGIN
  SELECT RAISE(ABORT, 'MEMORY_RECEIPT_SETTLED_IMMUTABLE');
END;
