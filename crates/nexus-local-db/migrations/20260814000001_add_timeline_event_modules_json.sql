-- V1.164 P1: Add modules_json column for TimelineEvent.modules (l5-mind observation).
-- Additive nullable TEXT; existing rows get NULL (no modules = unrecorded per spoke handbook).
-- Pattern: same as kb_key_blocks.modules_json (kb_store.rs:540-552 row struct, serialized at :458-462).
ALTER TABLE narrative_timeline_events ADD COLUMN modules_json TEXT;
