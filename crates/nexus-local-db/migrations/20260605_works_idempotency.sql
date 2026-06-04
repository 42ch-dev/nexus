-- V1.33: Works idempotency table (per plan §7.2)
CREATE TABLE IF NOT EXISTS works_idempotency (
    creator_id        TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    work_id           TEXT NOT NULL,
    created_at        TEXT NOT NULL,  -- ISO 8601
    UNIQUE(creator_id, client_request_id)
);
