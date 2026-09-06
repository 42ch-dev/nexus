-- v1.185 P1 Task 1: binding revision CAS counter (independent of Character revision).
ALTER TABLE actor_world_bindings
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 0
        CHECK (revision >= 0);
