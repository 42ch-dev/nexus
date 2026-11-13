-- v1.241 P3.5 Task 1: append-only applied identity (`strategy_applies`).
--
-- Plan: .mstar/plans/v241-p35-applied-identity.md §Architecture.
--
-- One row per apply: which approval was installed, for which strategy, at what content hash, by
-- whom, and which row it superseded. There is no update or delete path and no mutable "current"
-- pointer: the current apply of a strategy is the row with no successor, so rollback is just
-- another apply naming the previous row. `strategy_approvals` is untouched — an apply references
-- an existing frozen snapshot and never rewrites it.
--
-- What the schema enforces: `approval_id` and `supersedes_apply_id` are foreign keys to rows that
-- must exist, an apply cannot supersede itself, and the `pinned_approval_*` triggers pin the row
-- to the approval it names — `(strategy_id, content_hash)` must be the named approval's own pair,
-- so an apply pairing A's approval with B's strategy, or carrying a hash the approval does not
-- have, is unrepresentable rather than merely unwritten. The pair check lives in a trigger because
-- a composite foreign key would need a UNIQUE key on `strategy_approvals`, and that table is
-- frozen by this plan.
--
-- What the write statement must own: "one current apply per strategy". A repeat apply of an
-- approval that is already current is a typed conflict, but a repeat apply of a superseded
-- approval is legitimate (that is rollback), so no uniqueness here can express it. The insert
-- reads the current row and inserts in one statement; nothing in this table can make two current
-- rows visible without a second successor-less row being written.
--
-- Writer admission: cooperative core table — the same guard trio and outbox shape as the other
-- cooperative core tables. A raw/unregistered pool is fenced with `WRITER_FENCED`, and every
-- admitted mutation publishes one `core_changes` event.

CREATE TABLE IF NOT EXISTS strategy_applies (
    apply_id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL,
    approval_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    applied_by TEXT NOT NULL,
    supersedes_apply_id TEXT,
    FOREIGN KEY (approval_id) REFERENCES strategy_approvals(approval_id) ON DELETE RESTRICT,
    FOREIGN KEY (supersedes_apply_id) REFERENCES strategy_applies(apply_id) ON DELETE RESTRICT,
    -- A NULL supersede passes (the expression is NULL), which is exactly the first-apply case.
    CHECK (supersedes_apply_id IS NULL OR supersedes_apply_id <> apply_id)
) WITHOUT ROWID;

-- ── Row-to-approval pins: strategy_applies ──

CREATE TRIGGER IF NOT EXISTS pinned_approval_strategy_applies_insert
BEFORE INSERT ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_APPROVAL_MISMATCH')
  WHERE NOT EXISTS (
      SELECT 1
      FROM strategy_approvals a
      WHERE a.approval_id = NEW.approval_id
        AND a.strategy_id = NEW.strategy_id
        AND a.content_hash = NEW.content_hash
  );
END;

CREATE TRIGGER IF NOT EXISTS pinned_approval_strategy_applies_update
BEFORE UPDATE ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_APPROVAL_MISMATCH')
  WHERE NOT EXISTS (
      SELECT 1
      FROM strategy_approvals a
      WHERE a.approval_id = NEW.approval_id
        AND a.strategy_id = NEW.strategy_id
        AND a.content_hash = NEW.content_hash
  );
END;

-- ── Writer-protocol guards: strategy_applies ──

CREATE TRIGGER IF NOT EXISTS guard_strategy_applies_insert
BEFORE INSERT ON strategy_applies
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_applies_update
BEFORE UPDATE ON strategy_applies
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_applies_delete
BEFORE DELETE ON strategy_applies
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

-- ── Outbox: strategy_applies ──

-- An apply has no revision of its own: change-log consumers poll by `sequence`, so the row carries
-- no invented revision (`operation_receipts` publishes NULL for the same reason).
CREATE TRIGGER IF NOT EXISTS outbox_strategy_applies_insert
AFTER INSERT ON strategy_applies
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_applies', NEW.apply_id, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_applies_update
AFTER UPDATE ON strategy_applies
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_applies', NEW.apply_id, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_applies_delete
AFTER DELETE ON strategy_applies
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_applies', OLD.apply_id, NULL, 'delete', nexus_writer_id());
END;
