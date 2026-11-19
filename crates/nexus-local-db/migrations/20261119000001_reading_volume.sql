-- Old reading rows have no volume provenance. Keep them in volume 1,
-- the default for chapter URLs and reading clients that omit a volume.
-- Runs after 20260912000001_core_writer_protocol.sql.
CREATE TABLE reading_progress_with_volume (
    creator_id       TEXT NOT NULL,
    work_id          TEXT NOT NULL,
    volume           INTEGER NOT NULL CHECK (volume >= 1),
    chapter          INTEGER NOT NULL,
    scroll_progress  INTEGER NOT NULL DEFAULT 0
                     CHECK (scroll_progress >= 0 AND scroll_progress <= 10000),
    updated_at       TEXT NOT NULL,
    PRIMARY KEY (creator_id, work_id, volume, chapter),
    FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

INSERT INTO reading_progress_with_volume (creator_id, work_id, volume, chapter, scroll_progress, updated_at)
SELECT creator_id, work_id, 1, chapter, scroll_progress, updated_at FROM reading_progress;

DROP TABLE reading_progress;
ALTER TABLE reading_progress_with_volume RENAME TO reading_progress;

CREATE INDEX idx_reading_progress_work_volume_chapter
    ON reading_progress(work_id, volume, chapter);

ALTER TABLE reading_annotations ADD COLUMN volume INTEGER NOT NULL DEFAULT 1 CHECK (volume >= 1);
DROP INDEX idx_reading_annotations_work_chapter;
CREATE INDEX idx_reading_annotations_work_volume_chapter
    ON reading_annotations(creator_id, work_id, volume, chapter);

-- Reinstall the writer fences and change notifications dropped with the table.
CREATE TRIGGER guard_reading_progress_insert
BEFORE INSERT ON reading_progress
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

CREATE TRIGGER guard_reading_progress_update
BEFORE UPDATE ON reading_progress
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

CREATE TRIGGER guard_reading_progress_delete
BEFORE DELETE ON reading_progress
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

CREATE TRIGGER outbox_reading_progress_insert
AFTER INSERT ON reading_progress
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'reading_progress', NEW.creator_id || ':' || NEW.work_id || ':' || NEW.volume || ':' || NEW.chapter, NULL, 'insert', nexus_writer_id());
END;

CREATE TRIGGER outbox_reading_progress_update
AFTER UPDATE ON reading_progress
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'reading_progress', NEW.creator_id || ':' || NEW.work_id || ':' || NEW.volume || ':' || NEW.chapter, NULL, 'update', nexus_writer_id());
END;

CREATE TRIGGER outbox_reading_progress_delete
AFTER DELETE ON reading_progress
FOR EACH ROW
BEGIN
  INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id)
  VALUES ('', 'reading_progress', OLD.creator_id || ':' || OLD.work_id || ':' || OLD.volume || ':' || OLD.chapter, NULL, 'delete', nexus_writer_id());
END;
