-- Add work_id column to creator_schedules for indexed previous-preset lookup (V1.37 QC fix).
-- Pre-1.0: adding a column is backwards-compatible.
ALTER TABLE creator_schedules ADD COLUMN work_id TEXT;

-- Best-effort backfill from label (pattern: "work: <work_id>" or just the work_id substring).
-- SQLite doesn't support UPDATE with regex natively; backfill happens in Rust at insert time.
-- Rows inserted before this migration will have NULL work_id; the LIKE fallback still works.

-- Index for previous-preset completion lookups: WHERE preset_id = ? AND status = 'completed' AND work_id = ?
CREATE INDEX IF NOT EXISTS idx_creator_schedules_preset_status_work
    ON creator_schedules(preset_id, status, work_id);
