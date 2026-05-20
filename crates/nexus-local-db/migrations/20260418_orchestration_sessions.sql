-- Orchestration Sessions Table
-- Design: `.agents/knowledge/specs/orchestration-engine.md` §4.3

CREATE TABLE IF NOT EXISTS orchestration_sessions (
  session_id    TEXT PRIMARY KEY,
  creator_id    TEXT NOT NULL,
  preset_id     TEXT NOT NULL,
  preset_version INTEGER NOT NULL,
  parent_session_id TEXT,             -- set for inner-graph child sessions
  current_task_id TEXT,
  status        TEXT NOT NULL,        -- running | paused | waiting_for_input | completed | failed
  context_json  BLOB NOT NULL,        -- serialized graph_flow::Context
  chat_history_json BLOB,             -- optional; separate column for readability
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  FOREIGN KEY (parent_session_id) REFERENCES orchestration_sessions(session_id)
);

CREATE INDEX IF NOT EXISTS orchestration_sessions_by_creator ON orchestration_sessions(creator_id);
CREATE INDEX IF NOT EXISTS orchestration_sessions_by_status  ON orchestration_sessions(status);
