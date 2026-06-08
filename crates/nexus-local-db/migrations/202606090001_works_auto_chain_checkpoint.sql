-- V1.39 P0: Add auto-chain checkpoint fields to works table.
-- These fields track the continuation state for the auto-chain engine:
--   auto_chain_enabled: whether auto-chain is active for this work (default true)
--   driver_schedule_id: the currently-running FL-E driver schedule (nullable)
--   auto_chain_interrupted: set true when driver is interrupted by external event
ALTER TABLE works ADD COLUMN auto_chain_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE works ADD COLUMN driver_schedule_id TEXT;
ALTER TABLE works ADD COLUMN auto_chain_interrupted INTEGER NOT NULL DEFAULT 0;
