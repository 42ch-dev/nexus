-- V1.150 P1 (DF-75) — Moment Directive (Author's-Note analogue)
-- Product-local prompt control (spec `fl-l-w5-prompt-control-plane.md` §3):
-- persistent, per-scope, TTL'd author instruction injected by MCA into the
-- reserved `moment.directive` slot. NEVER on the spoke wire (AC-I3): not a
-- `modules.*` object, not a KnowledgeEntry, not in AssemblePacket
-- `placement[]` / `activation_trace[]`.
--
-- Lifecycle: `active` → (ttl_remaining hits 0 | scene clear | manual clear |
-- `--replace`) → `expired` (soft-delete; rows retained for DF-76 inspection).
-- At most ONE active directive per (creator_id, scope_kind, scope_id) —
-- enforced by a unique partial index.

CREATE TABLE IF NOT EXISTS moment_directives (
  directive_id          TEXT PRIMARY KEY,
  creator_id            TEXT NOT NULL,
  scope_kind            TEXT NOT NULL CHECK (scope_kind IN ('work','world')),
  scope_id              TEXT NOT NULL,
  body                  TEXT NOT NULL,
  insert_depth          TEXT NOT NULL CHECK (insert_depth IN ('head','mid','tail')),
  ttl_kind              TEXT NOT NULL CHECK (ttl_kind IN ('generations','chapters')),
  ttl_remaining         INTEGER NOT NULL CHECK (ttl_remaining >= 0),
  clear_on_scene_change INTEGER NOT NULL DEFAULT 0 CHECK (clear_on_scene_change IN (0,1)),
  status                TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','expired')),
  -- Lifecycle bookkeeping: cross-assemble state for the TTL / scene-change
  -- signals (spec §3.3 — `MomentRequest.event_id` change = scene change;
  -- `works.current_chapter` advance = chapter advance for novel Works).
  last_focused_event_id TEXT,
  last_chapter_no       INTEGER,
  created_at            INTEGER NOT NULL,  -- Unix epoch millis
  updated_at            INTEGER NOT NULL,  -- Unix epoch millis
  expires_at            INTEGER,           -- Unix epoch millis (TTL-0 / scene-clear / manual clear)
  replaced_by           TEXT               -- new directive_id when `--replace` superseded this row
);

-- At most one ACTIVE directive per scope (spec §3.1).
CREATE UNIQUE INDEX IF NOT EXISTS moment_directives_one_active_per_scope
  ON moment_directives(creator_id, scope_kind, scope_id)
  WHERE status = 'active';
