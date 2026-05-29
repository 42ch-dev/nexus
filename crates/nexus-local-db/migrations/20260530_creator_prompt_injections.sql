-- Prompt injection queue for creator.inject_prompt
-- Design: `.agents/plans/reports/2026-05-30-v1.31-creator-memory-capabilities/architect-c0-j0-injection-design.md`
-- Lifecycle: queued → claimed → consumed | expired

CREATE TABLE IF NOT EXISTS creator_prompt_injections (
  injection_id              TEXT PRIMARY KEY,
  creator_id                TEXT NOT NULL,
  session_id                TEXT NOT NULL,
  prompt                    TEXT NOT NULL,
  priority                  INTEGER NOT NULL DEFAULT 0,
  status                    TEXT NOT NULL,        -- queued | claimed | consumed | expired
  created_at                INTEGER NOT NULL,
  claimed_at                INTEGER,
  consumed_at               INTEGER,
  expires_at                INTEGER,
  source_schedule_id        TEXT,
  source_capability_call_id TEXT,
  metadata_json             BLOB
);

CREATE INDEX IF NOT EXISTS creator_prompt_injections_next
  ON creator_prompt_injections(creator_id, session_id, status, priority DESC, created_at ASC);

CREATE INDEX IF NOT EXISTS creator_prompt_injections_cleanup
  ON creator_prompt_injections(status, expires_at);
