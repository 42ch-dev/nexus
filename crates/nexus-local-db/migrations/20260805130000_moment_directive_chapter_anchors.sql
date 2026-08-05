-- V1.150 residual close (R-V1150P2-004 / R-V1150P2-008) — per-(directive,
-- work) chapter anchors.
--
-- `moment_directives.last_chapter_no` (a single chapter marker on the
-- directive row) cannot express the fixed semantics:
--   - R-004: a `chapters` TTL burns the DELTA of chapter advances since the
--     last injecting assemble, not a flat 1 per assemble.
--   - R-008: a world-scoped directive is used by multiple Works sharing the
--     world — the last-seen chapter must be tracked PER WORK so assembling
--     Work A never burns the directive's TTL on behalf of Work B.
--
-- The table is new in this iteration (v12, unreleased — no deployed users),
-- so the column is dropped via table rebuild rather than left as a dead
-- column. Chapter anchors move to `moment_directive_chapter_anchors` keyed by
-- (directive_id, work_id); the scene-change anchor (`last_focused_event_id`)
-- stays on the directive row — it is directive-level, not work-level.

-- Step 1: recreate `moment_directives` without `last_chapter_no`
CREATE TABLE moment_directives_new (
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
  -- Scene-change proxy: last focused `MomentRequest.event_id` at an injecting
  -- assemble (directive-level, per the `clear_on_scene_change` flag).
  last_focused_event_id TEXT,
  created_at            INTEGER NOT NULL,  -- Unix epoch millis
  updated_at            INTEGER NOT NULL,  -- Unix epoch millis
  expires_at            INTEGER,           -- Unix epoch millis (TTL-0 / scene-clear / manual clear)
  replaced_by           TEXT               -- new directive_id when `--replace` superseded this row
);

-- Step 2: copy data (explicit column list — `last_chapter_no` is dropped)
INSERT INTO moment_directives_new
  (directive_id, creator_id, scope_kind, scope_id, body, insert_depth,
   ttl_kind, ttl_remaining, clear_on_scene_change, status,
   last_focused_event_id, created_at, updated_at, expires_at, replaced_by)
SELECT directive_id, creator_id, scope_kind, scope_id, body, insert_depth,
       ttl_kind, ttl_remaining, clear_on_scene_change, status,
       last_focused_event_id, created_at, updated_at, expires_at, replaced_by
FROM moment_directives;

-- Step 3: drop old table
DROP TABLE moment_directives;

-- Step 4: rename new table to original name
ALTER TABLE moment_directives_new RENAME TO moment_directives;

-- Step 5: recreate the partial unique index (same shape as the original
-- migration 20260805120000)
CREATE UNIQUE INDEX IF NOT EXISTS moment_directives_one_active_per_scope
  ON moment_directives(creator_id, scope_kind, scope_id)
  WHERE status = 'active';

-- Step 6: per-(directive, work) chapter anchors — the last-seen
-- `works.current_chapter` for each Work that injected a `chapters`-TTL
-- directive (work-scoped or world-scoped). A world-scoped directive has an
-- independent TTL burn per Work that uses it (R-008); the chapter delta
-- between this Work's injecting assembles is the burn amount (R-004).
CREATE TABLE IF NOT EXISTS moment_directive_chapter_anchors (
  directive_id    TEXT NOT NULL,
  work_id         TEXT NOT NULL,
  last_chapter_no INTEGER NOT NULL,  -- last observed `works.current_chapter`
  updated_at      INTEGER NOT NULL,  -- Unix epoch millis
  PRIMARY KEY (directive_id, work_id)
);
