-- V1.39 P0: Add auto-chain checkpoint fields to works table.
-- These fields track the continuation state for the auto-chain engine:
--   auto_chain_enabled: whether auto-chain is active for this work (default true)
--   driver_schedule_id: the currently-running FL-E driver schedule (nullable)
--   auto_chain_interrupted: set true when driver is interrupted by external event
ALTER TABLE works ADD COLUMN auto_chain_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE works ADD COLUMN driver_schedule_id TEXT;
ALTER TABLE works ADD COLUMN auto_chain_interrupted INTEGER NOT NULL DEFAULT 0;

-- Fix E (W-E): Partial index for boot resume query (find_resumable_works).
-- Filters on (auto_chain_enabled, auto_chain_interrupted, status) with the
-- partial condition matching the most common filter clause.
CREATE INDEX IF NOT EXISTS works_auto_chain_resume
    ON works(auto_chain_enabled, auto_chain_interrupted, status)
    WHERE auto_chain_enabled = 1;
