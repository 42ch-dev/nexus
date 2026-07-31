-- V1.146 P4 T1 — additive migration for modules.* durability.
-- Spec: plan 2026-07-30-v1.146-p4-lore-activation-and-assemble-inspectors.md.
-- Triad ADR: modules.* ≠ extensions.*; separate column per P4 Architecture Lock.
--
-- Adds a single nullable TEXT column that carries the full serialized
-- `modules` namespace, so per-entry functional dialects (e.g.
-- modules.activation, modules.pack) survive the SQLite read-modify-write
-- cycle and the spoke conversion seam. Unknown namespaces are preserved
-- verbatim. Legacy rows default to NULL (backfilled on next write cycle).

ALTER TABLE kb_key_blocks ADD COLUMN modules_json TEXT;
