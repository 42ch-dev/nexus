-- v1.185 P0 Task 1: Character lifecycle maintenance counters.
ALTER TABLE characters
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 0
        CHECK (revision >= 0);
ALTER TABLE characters
    ADD COLUMN lifecycle_epoch INTEGER NOT NULL DEFAULT 0
        CHECK (lifecycle_epoch >= 0);
