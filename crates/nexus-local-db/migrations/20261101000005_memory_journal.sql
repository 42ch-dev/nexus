-- v1.227 NXM-3: durable intent journal for the provider's cross-store writes
-- (spoke-memory adoption plan §5.2, D-10, conformance C-113).
--
-- A SQLite transaction cannot make a markdown file write atomic with a row
-- write, so the provider journals its intent first: the planned file targets
-- (path + content + content digest) are committed in the SAME guarded
-- transaction as the effect rows and the receipt. That commit is the
-- operation's point of no return. The files are then staged hidden, published
-- by an atomic rename, and only then is the entry marked `applied`.
--
-- The row carries no content the receipt does not already allow (D-9): the
-- journaled file bodies are the provider's own projected content, never
-- receipt payload, and they are scrubbed the moment the intent settles so a
-- deleted or superseded body is not retained (MEM-R014). `result_json` is the
-- content-free receipt outcome recovery needs to terminalize the receipt.
-- `seq` is the settlement order: file effects for one target must publish in
-- commit order, never in wall-clock (whole-second) or uuid order. The store
-- assigns it (`COALESCE(MAX(seq), 0) + 1`, the same shape as
-- `operation_receipts.sequence`), never SQLite.
--
-- `state` is the recovery surface:
--
--   pending      committed intent not yet published; a reopen settles it and
--                objects of the subject stay out of recall/list/get meanwhile
--   applied      the file effect is published and consistent with the row; the
--                intent content is scrubbed
--   quarantined  the intent could not be settled (foreign target content,
--                unsafe path, I/O failure); the subject's reads answer the
--                terminal INTERNAL_ERROR instead of serving half-committed
--                content
--   deferred     a bounded recovery pass did not reach it (§5.2 step 5); still
--                recoverable, but the subject reads terminally until a later
--                pass settles it
--
-- Writer admission: the table is a cooperative core table with the same guard
-- and outbox trigger family as the control/content tables, so a raw pool is
-- fenced with `WRITER_FENCED` and every state change publishes one
-- `core_changes` event.

CREATE TABLE IF NOT EXISTS memory_journal (
    seq INTEGER PRIMARY KEY,
    receipt_id TEXT NOT NULL UNIQUE,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    op TEXT NOT NULL,
    idempotency_key TEXT,
    state TEXT NOT NULL CHECK (state IN ('pending', 'applied', 'quarantined', 'deferred')),
    targets_json TEXT NOT NULL,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) WITHOUT ROWID;
-- WITHOUT ROWID: on a rowid table `seq INTEGER PRIMARY KEY` is an ALIAS for the
-- row's storage id, and neither an `INSERT OR REPLACE` carrying an explicit
-- rowid nor an `UPDATE OR REPLACE … SET rowid = …` fires the victim's DELETE
-- trigger while `recursive_triggers` is OFF — so an admitted writer could
-- silently destroy a committed cross-store intent and its outbox event. The
-- table's identity is `seq` assigned by the store (nothing reads a rowid), so
-- removing the alias removes the bypass class outright, exactly as
-- `operation_receipts` does.

-- The isolation lookup: read paths ask whether any intent for a subject is
-- still unsettled. The predicate is repeated as a literal in the queries
-- (`state <> 'applied'`) so the planner can prove it and use this index; a bind
-- parameter cannot be matched against a partial-index predicate at prepare time.
CREATE INDEX IF NOT EXISTS idx_memory_journal_unsettled
    ON memory_journal (space_id, subject_id, created_at)
    WHERE state <> 'applied';

-- The bounded recovery pass pages a whole space by settlement order.
CREATE INDEX IF NOT EXISTS idx_memory_journal_recovery
    ON memory_journal (space_id, seq)
    WHERE state <> 'applied';

-- ── Writer-protocol guards (cooperative core-table shape) ──

CREATE TRIGGER IF NOT EXISTS guard_memory_journal_insert
BEFORE INSERT ON memory_journal
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

CREATE TRIGGER IF NOT EXISTS guard_memory_journal_update
BEFORE UPDATE ON memory_journal
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

CREATE TRIGGER IF NOT EXISTS guard_memory_journal_delete
BEFORE DELETE ON memory_journal
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

-- ── Outbox family: one event per journal mutation ──

CREATE TRIGGER IF NOT EXISTS outbox_memory_journal_insert
AFTER INSERT ON memory_journal
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_journal', NEW.receipt_id, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_journal_update
AFTER UPDATE ON memory_journal
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_journal', NEW.receipt_id, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_memory_journal_delete
AFTER DELETE ON memory_journal
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'memory_journal', OLD.receipt_id, NULL, 'delete', nexus_writer_id());
END;
