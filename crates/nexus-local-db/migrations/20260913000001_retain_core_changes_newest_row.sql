-- v1.189 P1 QC fix F-9: byte-budget retention must never delete the row just inserted.

DROP TRIGGER IF EXISTS retain_core_changes_bytes;

CREATE TRIGGER IF NOT EXISTS retain_core_changes_bytes
AFTER INSERT ON core_changes
FOR EACH ROW
WHEN (
  SELECT COALESCE(SUM(
    length(world_id) + length(resource_kind) + length(resource_id)
    + COALESCE(length(resource_revision), 0) + length(change_kind) + length(writer_id)
  ), 0) FROM core_changes
) > 8388608
BEGIN
  DELETE FROM core_changes
  WHERE sequence IN (
    SELECT sequence FROM (
      SELECT sequence,
             SUM(
               length(world_id) + length(resource_kind) + length(resource_id)
               + COALESCE(length(resource_revision), 0) + length(change_kind) + length(writer_id)
             ) OVER (ORDER BY sequence DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS cum_desc
      FROM core_changes
    )
    WHERE cum_desc > 8388608
      AND sequence != NEW.sequence
  );
END;
