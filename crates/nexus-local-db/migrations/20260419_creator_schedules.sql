-- Creator Schedules + Dependencies + Core Context Versions
-- Design: `.agents/plans/knowledge/creator-schedule-and-core-context-v1.md` §10
-- WS7 T1: additive-only migration; no existing tables touched.

CREATE TABLE IF NOT EXISTS creator_schedules (
  schedule_id              TEXT PRIMARY KEY,                     -- ULID
  creator_id               TEXT NOT NULL,
  preset_id                TEXT NOT NULL,
  preset_version           INTEGER NOT NULL,
  status                   TEXT NOT NULL,                        -- pending|running|paused|completed|cancelled|failed
  concurrency_kind         TEXT NOT NULL,                        -- serial|parallel_with|parallel_any
  concurrency_whitelist    TEXT,                                 -- JSON array of schedule_ids (for parallel_with)
  current_core_context_version INTEGER NOT NULL DEFAULT 0,
  current_session_id       TEXT,                                 -- FK to orchestration_sessions(session_id), nullable
  scheduled_at             INTEGER,                              -- nullable; V1.4 unused; V1.5 clock trigger
  label                    TEXT,
  created_at               INTEGER NOT NULL,
  updated_at               INTEGER NOT NULL,
  terminated_at            INTEGER,
  FOREIGN KEY (current_session_id) REFERENCES orchestration_sessions(session_id)
);

CREATE INDEX IF NOT EXISTS creator_schedules_by_creator   ON creator_schedules(creator_id, status);
CREATE INDEX IF NOT EXISTS creator_schedules_by_status    ON creator_schedules(status);
CREATE INDEX IF NOT EXISTS creator_schedules_by_scheduled ON creator_schedules(scheduled_at) WHERE scheduled_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS schedule_dependencies (
  schedule_id              TEXT NOT NULL,
  depends_on               TEXT NOT NULL,
  PRIMARY KEY (schedule_id, depends_on),
  FOREIGN KEY (schedule_id) REFERENCES creator_schedules(schedule_id) ON DELETE CASCADE,
  FOREIGN KEY (depends_on)  REFERENCES creator_schedules(schedule_id)
);

CREATE TABLE IF NOT EXISTS core_context_versions (
  schedule_id              TEXT NOT NULL,
  version                  INTEGER NOT NULL,
  payload_kind             TEXT NOT NULL,                        -- text|struct
  content                  BLOB NOT NULL,                        -- serialized CoreContextPayload
  derivation_kind          TEXT NOT NULL,                        -- seed|user_edit|preset_hook|preset_seed_expansion|llm_summarize
  derivation_detail        BLOB,                                 -- serialized DerivationStep (op details, template hash, …)
  created_at               INTEGER NOT NULL,
  created_by_kind          TEXT NOT NULL,                        -- user|system
  created_by_user_id       TEXT,                                 -- nullable; set when created_by_kind='user'
  PRIMARY KEY (schedule_id, version),
  FOREIGN KEY (schedule_id) REFERENCES creator_schedules(schedule_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS core_context_versions_by_schedule ON core_context_versions(schedule_id, version DESC);
