-- V1.34: Add FL-E stage tracking columns to works table
-- (per creator-workflow.md §3.1 and §3.2)

ALTER TABLE works ADD COLUMN current_stage TEXT NOT NULL DEFAULT 'intake'
    CHECK (current_stage IN ('intake','research','produce','review','persist'));

ALTER TABLE works ADD COLUMN stage_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (stage_status IN ('pending','active','complete','skipped','failed'));
