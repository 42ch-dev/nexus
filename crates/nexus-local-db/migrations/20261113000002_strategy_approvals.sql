-- v1.239 P3 T5: immutable approved strategy snapshots (`strategy_approvals`).
--
-- Plan: .mstar/plans/v239-p3-approve-route.md §Candidate and simulation binding (Q4).
--
-- One row per approval: the frozen document, its canonical content hash, the candidate revision
-- the approver read, the exact simulation trajectory the approval rests on, every resolved
-- pattern pin, the approver and the timestamp. `strategy_id` + `content_hash` is UNIQUE, so one
-- content approves exactly once per strategy: a repeat approve is a typed 409, never a second
-- snapshot. There is no update or delete path — a later candidate edit produces new content and
-- therefore a new row.
--
-- `document_json`, `trajectory_json` and `pins_json` are the verbatim frozen bytes. The CHECKs
-- pin the document to the row identity and the trajectory's `ir_content_hash` to `content_hash`,
-- so a row whose evidence disagrees with itself is unrepresentable; the snapshot is never
-- recomputed at read time. The canonical content hash is deliberately NOT a column beyond the
-- identity member: it is recomputed from `document_json` through `nexus-workflow-ir` by the
-- approval write and compared with the simulator's hash there.
--
-- Writer admission: cooperative core table — the same guard trio and outbox shape as the other
-- cooperative core tables. A raw/unregistered pool is fenced with `WRITER_FENCED`, and every
-- admitted mutation publishes one `core_changes` event.

CREATE TABLE IF NOT EXISTS strategy_approvals (
    approval_id TEXT PRIMARY KEY,
    strategy_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    candidate_revision INTEGER NOT NULL CHECK (candidate_revision >= 0),
    document_json TEXT NOT NULL CHECK (json_valid(document_json)),
    trajectory_json TEXT NOT NULL CHECK (json_valid(trajectory_json)),
    pins_json TEXT NOT NULL CHECK (json_valid(pins_json)),
    approver TEXT NOT NULL,
    approved_at TEXT NOT NULL,
    -- One content approves once per strategy; the repeat approve is the conflict, not a second row.
    UNIQUE (strategy_id, content_hash),
    -- SQLite passes a CHECK whose expression is NULL, so the type conjunction is what makes a
    -- missing or null member rejectable: `json_extract` alone would accept `revision: null`.
    CHECK (
        COALESCE(json_type(document_json, '$.revision'), 'absent') = 'integer'
        AND json_extract(document_json, '$.revision') = candidate_revision
    ),
    CHECK (
        COALESCE(json_type(document_json, '$.id'), 'absent') = 'text'
        AND json_extract(document_json, '$.id') = strategy_id
    ),
    CHECK (
        COALESCE(json_type(document_json, '$.state'), 'absent') = 'text'
        AND json_extract(document_json, '$.state') = 'candidate'
    ),
    -- The approval identity is the trajectory's own hash: evidence from another content cannot be
    -- stored against this row.
    CHECK (
        COALESCE(json_type(trajectory_json, '$.ir_content_hash'), 'absent') = 'text'
        AND json_extract(trajectory_json, '$.ir_content_hash') = content_hash
    ),
    CHECK (json_type(pins_json) = 'array')
) WITHOUT ROWID;

-- ── Writer-protocol guards: strategy_approvals ──

CREATE TRIGGER IF NOT EXISTS guard_strategy_approvals_insert
BEFORE INSERT ON strategy_approvals
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_approvals_update
BEFORE UPDATE ON strategy_approvals
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

CREATE TRIGGER IF NOT EXISTS guard_strategy_approvals_delete
BEFORE DELETE ON strategy_approvals
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

-- ── Outbox: strategy_approvals ──

CREATE TRIGGER IF NOT EXISTS outbox_strategy_approvals_insert
AFTER INSERT ON strategy_approvals
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_approvals', NEW.approval_id, CAST(NEW.candidate_revision AS TEXT), 'insert', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_approvals_update
AFTER UPDATE ON strategy_approvals
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_approvals', NEW.approval_id, CAST(NEW.candidate_revision AS TEXT), 'update', nexus_writer_id());
END;

CREATE TRIGGER IF NOT EXISTS outbox_strategy_approvals_delete
AFTER DELETE ON strategy_approvals
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'strategy_approvals', OLD.approval_id, CAST(OLD.candidate_revision AS TEXT), 'delete', nexus_writer_id());
END;
