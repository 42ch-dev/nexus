-- V1.148 P1: persisted spoke `Rule` storage (RuleQueryPort production).
-- Resolves spoke `rule_refs` against `rule_id`; `world_id` is an ownership
-- column for future CRUD (spoke `list_rules` has no world parameter, so the
-- lookup is by `rule_id` only).
--
-- Conventions:
--   * `schema_version` is the wire `NonZeroU64` stored as INTEGER (>= 1; 0 is
--     invalid on the wire).
--   * JSON-shaped columns are opaque TEXT: `target_entry_types_json` is a JSON
--     array of entry-type strings, `source_anchor_json` is an optional JSON
--     object, `extensions_json` is the product namespace bag (opaque,
--     round-tripped verbatim).
--   * `created_at` / `updated_at` are Unix epoch seconds (INTEGER); NULL when
--     unknown.

CREATE TABLE IF NOT EXISTS spoke_rules (
  rule_id               TEXT NOT NULL PRIMARY KEY,
  world_id              TEXT NOT NULL,
  schema_version        INTEGER NOT NULL DEFAULT 1,
  canonical_name        TEXT NOT NULL,
  kind                  TEXT NOT NULL,
  statement             TEXT,
  description           TEXT,
  target_entry_types_json TEXT NOT NULL DEFAULT '[]',
  severity_hint         TEXT,
  status                TEXT DEFAULT 'active',
  source_anchor_json    TEXT,
  extensions_json       TEXT NOT NULL DEFAULT '{}',
  created_at            INTEGER,
  updated_at            INTEGER
);

CREATE INDEX IF NOT EXISTS idx_spoke_rules_world ON spoke_rules(world_id);
