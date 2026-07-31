-- V1.146 P2 T2 — additive migration for compute session store.
-- Spec: plan 2026-07-30-v1.146-p2-nexus-adapter-full-adapter Decision 2.
--
-- Bridges spoke's stateless ProjectRequest/ComputeRequest pair to nexus's
-- stateless WASM host: project() stages computable state; compute() reads
-- staged state + merges dynamic computable updates, builds ComputeInput,
-- fires WASM engine, optionally settles state_delta back into the entry.

CREATE TABLE IF NOT EXISTS compute_sessions (
    session_id TEXT PRIMARY KEY,
    entry_id TEXT NOT NULL,
    state_json TEXT,
    created_at TEXT NOT NULL
);
