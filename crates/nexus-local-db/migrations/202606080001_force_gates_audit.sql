-- Force-gates audit log (V1.37 §7.9.3).
-- Append-only table recording every gate-bypass event.
CREATE TABLE IF NOT EXISTS force_gates_audit (
    audit_id   TEXT PRIMARY KEY,      -- fga_<timestamp>_<random>
    preset_id  TEXT NOT NULL,
    work_id    TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    forced     BOOLEAN NOT NULL DEFAULT TRUE,
    reason     TEXT,                   -- NULL only if client omitted reason (should not happen)
    forced_at  TEXT NOT NULL           -- ISO-8601
);
