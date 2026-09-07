//! `SqliteSessionStorage` — sqlx-backed [`graph_flow::SessionStorage`].
//!
//! ## Pool ownership
//!
//! Construction takes an `Arc<SqlitePool>` by value. The pool is owned by the
//! caller (daemon gets it from [`nexus_local_db::open_pool`]; tests construct
//! a fresh pool over a temp file). This crate **never** opens its own pool.
//!
//! ## Serialization convention
//!
//! The `orchestration_sessions` table stores:
//! - `session_id` ← `Session.id`
//! - `creator_id` / `preset_id` / `preset_version` — inferred from session
//!   context data (keys `_creator_id`, `_preset_id`, `_preset_version`).
//!   When these keys are absent the columns default to `"unknown"` / `"default"` / `0`.
//! - `parent_session_id` — from context key `_parent_session_id`.
//! - `current_task_id` ← `Session.current_task_id`
//! - `status` ← `"running"` always on save (engine manages lifecycle).
//! - `context_json` ← `serde_json::to_vec(&session.context)`
//!
//! ## Session recovery (WS2 R1)
//!
//! On daemon restart, `list_non_terminal_sessions()` queries persisted sessions
//! with status `running`, `paused`, or `waiting_for_input` so the in-memory
//! tracker can be repopulated.
//!
//! Design: `.mstar/specs/orchestration-engine.md` §4.3.

use async_trait::async_trait;
use graph_flow::{Session, SessionStorage};
use std::sync::Arc;

use super::inspect::{CheckpointRow, CheckpointSummary};
use crate::engine::{EngineError, SessionId, SessionStatus, SessionSummary};
use crate::run_state::{
    ChildCheckpoint, RunCheckpoint, RunDescriptorV1, RunRecord, RunStateV1, WorkflowStateStore,
};

/// SQLite-backed session storage sharing `nexus-local-db`'s pool.
pub struct SqliteSessionStorage {
    pool: Arc<sqlx::SqlitePool>,
}

impl SqliteSessionStorage {
    /// Create a new storage backed by the given shared pool.
    ///
    /// The pool must already have migrations applied (including the
    /// `orchestration_sessions` table). Call
    /// [`nexus_local_db::run_migrations`] before constructing this.
    #[must_use]
    pub const fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
        Self { pool }
    }

    /// List all sessions with non-terminal status (WS2 R1).
    ///
    /// Queries persisted sessions where status is `running`, `paused`, or
    /// `waiting_for_input`. Used by the engine on daemon restart to repopulate
    /// the in-memory session tracker.
    ///
    /// Returns `SessionSummary` structs suitable for engine recovery.
    ///
    /// # Errors
    /// Returns a graph-flow error if the database query fails.
    pub async fn list_non_terminal_sessions(&self) -> graph_flow::Result<Vec<SessionSummary>> {
        #[derive(sqlx::FromRow)]
        struct SummaryRow {
            session_id: String,
            creator_id: String,
            preset_id: String,
            status: String,
            current_task_id: Option<String>,
        }

        let rows = sqlx::query_as!(
            SummaryRow,
            r#"SELECT session_id as "session_id!", creator_id as "creator_id!",
                      preset_id as "preset_id!", status as "status!", current_task_id
               FROM orchestration_sessions
               WHERE status IN ('running', 'paused', 'waiting_for_input')"#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            graph_flow::GraphError::StorageError(format!("list_non_terminal_sessions: {e}"))
        })?;

        let summaries: Vec<SessionSummary> = rows
            .into_iter()
            .map(|row| {
                let status = match row.status.as_str() {
                    "paused" => SessionStatus::Paused,
                    "waiting_for_input" => SessionStatus::WaitingForInput,
                    // Fallback for any other non-terminal values in DB
                    _ => SessionStatus::Running,
                };
                SessionSummary {
                    session_id: SessionId(row.session_id),
                    creator_id: row.creator_id,
                    preset_id: row.preset_id,
                    status,
                    current_task_id: row.current_task_id,
                }
            })
            .collect();

        Ok(summaries)
    }
    /// Read-only checkpoint row for the `nexus42 ops inspect` CLI surface
    /// (V1.182 P1 BL-04).
    ///
    /// Unlike [`SessionSummary`], this carries the persisted position
    /// timestamps (`created_at`/`updated_at`, unix epoch seconds written on
    /// every save) plus the raw `context_json` blob so the CLI can project
    /// the resume rules without constructing a `graph_flow::Context`.
    ///
    /// The queries are dynamic (`sqlx::query_as`, not macros) deliberately:
    /// this is a local read-only surface, so it adds no `.sqlx/` offline
    /// entries (CI `verify-sqlx-offline` stays untouched).
    ///
    /// # Errors
    /// Returns the verbatim `sqlx::Error` when the query fails.
    pub async fn get_checkpoint_row(
        &self,
        session_id: &str,
    ) -> Result<Option<CheckpointRow>, sqlx::Error> {
        sqlx::query_as::<_, CheckpointRow>(
            "SELECT session_id, creator_id, preset_id, preset_version, current_task_id,
                    status, context_json, created_at, updated_at
             FROM orchestration_sessions
             WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// Read-only, bounded checkpoint list for `nexus42 ops inspect` (V1.182
    /// P1 BL-04, QC wave item 2).
    ///
    /// Bounded to the 200 most recently updated non-terminal rows
    /// (recovery-filter status set) — the list never grows unbounded over
    /// the store's lifetime, and `context_json` (which embeds chat history)
    /// is **not** loaded: the resume-rule predicates are projected in SQL
    /// (JSON1: `json_valid` / `json_type` / `json_extract` / `json_each`)
    /// into [`CheckpointSummary`] verdict inputs. `json_each` is guarded by
    /// a `json_type($.data) = 'object'` CASE so a corrupt or non-object
    /// blob can never raise; corrupt rows surface as
    /// `context_valid_json = false`.
    ///
    /// Ordering is `updated_at DESC`; a secondary `session_id DESC` tie-break
    /// keeps order deterministic for identical timestamps (bulk seeds).
    ///
    /// # Errors
    /// Returns the verbatim `sqlx::Error` when the query fails.
    pub async fn list_checkpoint_rows(&self) -> Result<Vec<CheckpointSummary>, sqlx::Error> {
        sqlx::query_as::<_, CheckpointSummary>(
            "SELECT session_id, creator_id, preset_id, preset_version, current_task_id,
                    status, created_at, updated_at,
                    json_valid(context_json) AS context_valid_json,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data') = 'object' THEN 1
                        ELSE 0
                    END AS context_data_is_object,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data._run_status') = 'text'
                        THEN json_extract(context_json, '$.data._run_status')
                        ELSE NULL
                    END AS run_status,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data._run_error') = 'text'
                        THEN json_extract(context_json, '$.data._run_error')
                        ELSE NULL
                    END AS run_error,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data') = 'object'
                        THEN (SELECT group_concat(key)
                              FROM json_each(json_extract(context_json, '$.data'))
                              WHERE value IS NOT NULL
                                AND (key LIKE '\\_converge\\_arrivals\\_%' ESCAPE '\\'
                                     OR key LIKE '\\_merge\\_%' ESCAPE '\\'
                                     OR key LIKE '\\_join\\_wait\\_start\\_%' ESCAPE '\\'))
                        ELSE NULL
                    END AS live_join_keys
             FROM orchestration_sessions
             WHERE status IN ('running', 'paused', 'waiting_for_input')
             ORDER BY updated_at DESC, session_id DESC
             LIMIT 200",
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Honest full row count for the same status filter the list uses —
    /// surfaces the truncated total (`"200 of N+"`) so the LIMIT never
    /// silently hides rows. Read-only.
    ///
    /// # Errors
    /// Returns the verbatim `sqlx::Error` when the query fails.
    pub async fn count_checkpoint_rows(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM orchestration_sessions
             WHERE status IN ('running', 'paused', 'waiting_for_input')",
        )
        .fetch_one(&*self.pool)
        .await
    }
}

#[async_trait]
impl SessionStorage for SqliteSessionStorage {
    async fn save(&self, session: Session) -> graph_flow::Result<()> {
        let now = chrono::Utc::now().timestamp();

        // Extract metadata from context (uses async get which deserializes).
        let creator_id: String = session
            .context
            .get("_creator_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        let preset_id: String = session
            .context
            .get("_preset_id")
            .await
            .unwrap_or_else(|| "default".to_string());
        let preset_version: i64 = session.context.get("_preset_version").await.unwrap_or(0);
        let parent_session_id: Option<String> = session.context.get("_parent_session_id").await;

        // Serialize the entire context (includes chat history).
        let context_bytes = serde_json::to_vec(&session.context)
            .map_err(|e| graph_flow::GraphError::StorageError(format!("serialize context: {e}")))?;

        // Pre-own all bind params before the macro call (borrow lifetimes).
        let session_id = session.id;
        let current_task_id = session.current_task_id;

        sqlx::query!(
            r#"
            INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version,
                 parent_session_id, current_task_id, status,
                 context_json, chat_history_json, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                current_task_id = excluded.current_task_id,
                context_json     = excluded.context_json,
                updated_at       = excluded.updated_at
            WHERE orchestration_sessions.status = 'running'
            "#,
            session_id,
            creator_id,
            preset_id,
            preset_version,
            parent_session_id,
            current_task_id,
            context_bytes,
            now,
            now
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            graph_flow::GraphError::StorageError(format!("save session '{session_id}': {e}"))
        })?;

        Ok(())
    }

    async fn get(&self, id: &str) -> graph_flow::Result<Option<Session>> {
        let id_owned = id.to_owned();
        let row = sqlx::query_as!(
            SessionRow,
            "SELECT session_id as \"session_id!\", current_task_id, context_json
             FROM orchestration_sessions WHERE session_id = ?",
            id_owned
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| graph_flow::GraphError::StorageError(format!("get session '{id}': {e}")))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let context: graph_flow::Context =
            serde_json::from_slice(&row.context_json).map_err(|e| {
                graph_flow::GraphError::StorageError(format!(
                    "deserialize context for session '{id}': {e}"
                ))
            })?;

        Ok(Some(Session {
            id: row.session_id,
            graph_id: "default".to_string(),
            current_task_id: row.current_task_id.unwrap_or_default(),
            status_message: None,
            context,
        }))
    }

    async fn delete(&self, id: &str) -> graph_flow::Result<()> {
        let id_owned = id.to_owned();
        let result = sqlx::query!(
            "DELETE FROM orchestration_sessions WHERE session_id = ?",
            id_owned
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| graph_flow::GraphError::StorageError(format!("delete session '{id}': {e}")))?;

        if result.rows_affected() == 0 {
            return Err(graph_flow::GraphError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }
}

/// Internal row mapping for the durable run-state SELECT (A2).
#[derive(sqlx::FromRow)]
struct RunRow {
    session_id: String,
    status: String,
    execution_version: i64,
    state_revision: i64,
    run_state_json: Option<Vec<u8>>,
    run_descriptor_json: Option<Vec<u8>>,
}

/// Serialize a run-state value to a BLOB, mapping serde errors to
/// [`EngineError::GraphFlow`] (storage-layer serialization failure).
fn serialize_blob<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec(value).map_err(|e| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "serialize run-state blob: {e}"
        )))
    })
}

/// Deserialize a run-state BLOB, mapping serde errors to
/// [`EngineError::GraphFlow`].
fn deserialize_blob<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    what: &str,
) -> Result<T, EngineError> {
    serde_json::from_slice(bytes).map_err(|e| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "deserialize {what}: {e}"
        )))
    })
}

/// Build the frozen child [`RunDescriptorV1`] for a child checkpoint (A2/A4).
///
/// A child run inherits the trusted root identity (creator, preset, source,
/// workspace, bindings) and names its parent session and inner graph. This is
/// the reconstructible child identity — never `"unknown"`/`"default"`.
fn child_descriptor(
    root: &RunDescriptorV1,
    root_session_id: &str,
    child: &ChildCheckpoint,
) -> RunDescriptorV1 {
    RunDescriptorV1 {
        creator_id: root.creator_id.clone(),
        work_id: root.work_id.clone(),
        workspace_root: root.workspace_root.clone(),
        preset_id: root.preset_id.clone(),
        preset_version: root.preset_version,
        source: root.source.clone(),
        input: root.input.clone(),
        agent_bindings: root.agent_bindings.clone(),
        parent_session_id: Some(SessionId(root_session_id.to_string())),
        graph_name: child.graph_name.clone(),
    }
}

/// Determine the outcome of a failed child CAS (0 rows affected).
///
/// Returns `None` when the child is already terminal at the expected
/// revision **and** the DB terminal status matches the parent's submitted
/// checkpoint status — the parent's checkpoint confirms the child's final
/// state and no overwrite is needed (Important 1: a legitimately-completed
/// child must not fail the parent's transition). When the DB terminal
/// status differs from the submitted child status (e.g. the parent thinks
/// the child is Running/completed but the DB is Failed/Cancelled at that
/// revision), the confirmation is rejected and a [`EngineError::GraphFlow`]
/// mismatch is returned — a stale Running checkpoint must never let a parent
/// commit Completed over a child that is Failed/Cancelled at that revision
/// (Important 6). Returns `Some(EngineError)` when the child CAS genuinely
/// failed: a stale revision (child moved on) or a terminal child at a stale
/// revision.
async fn child_cas_outcome(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    child_id: &str,
    expected_revision: u64,
    submitted_status: SessionStatus,
) -> Option<EngineError> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT state_revision, status FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(child_id)
    .fetch_optional(&mut **tx)
    .await
    .ok()
    .flatten();

    match row {
        Some((found_rev, status)) => {
            let terminal = matches!(
                status.as_str(),
                "completed" | "failed" | "cancelled" | "interrupted"
            );
            if terminal && found_rev == expected_revision as i64 {
                // Child already terminal at the expected revision. The
                // parent's checkpoint confirms it **only** when the DB
                // terminal status matches the submitted child status. A
                // stale Running/completed checkpoint submitted against a
                // Failed/Cancelled child must not confirm the parent's
                // transition (Important 6).
                if status == submitted_status.as_db_str() {
                    None
                } else {
                    Some(EngineError::GraphFlow(graph_flow::GraphError::StorageError(
                        format!(
                            "child '{child_id}' is terminal '{status}' at revision {expected_revision} \
                             but the parent submitted '{}' (ChildCheckpoint status/state \
                             mismatch, non-replayable confirmation)",
                            submitted_status.as_db_str()
                        ),
                    )))
                }
            } else if terminal {
                Some(EngineError::TerminalState(child_id.to_string()))
            } else if found_rev != expected_revision as i64 {
                Some(EngineError::RevisionMismatch {
                    session_id: child_id.to_string(),
                    expected: expected_revision,
                    found: u64::try_from(found_rev).unwrap_or(0),
                })
            } else {
                Some(EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "child CAS failed for '{child_id}' (revision {expected_revision}, status {status})"
                ))))
            }
        }
        None => Some(EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "child CAS failed for '{child_id}': row absent"
        )))),
    }
}

/// Read the persisted root descriptor for a session within a transaction.
///
/// Returns `None` when the row has no descriptor (v0 legacy row).
async fn read_root_descriptor(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
) -> Result<Option<RunDescriptorV1>, EngineError> {
    let bytes: Option<Option<Vec<u8>>> = sqlx::query_scalar(
        "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "read root descriptor '{session_id}': {e}"
        )))
    })?;
    match bytes {
        Some(Some(b)) if !b.is_empty() => {
            deserialize_blob::<RunDescriptorV1>(&b, "run_descriptor_json").map(Some)
        }
        _ => Ok(None),
    }
}

#[async_trait]
impl WorkflowStateStore for SqliteSessionStorage {
    async fn load_run(&self, session_id: &SessionId) -> Result<Option<RunRecord>, EngineError> {
        let id = session_id.0.clone();
        let row = sqlx::query_as!(
            RunRow,
            r#"SELECT session_id as "session_id!", status as "status!",
                      execution_version as "execution_version!",
                      state_revision as "state_revision!", run_state_json, run_descriptor_json
               FROM orchestration_sessions WHERE session_id = ?"#,
            id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "load_run '{}': {e}", session_id.0
            )))
        })?;

        let Some(row) = row else {
            return Ok(None);
        };

        // A corrupt/unsupported execution_version is non-replayable under A7:
        // negative values and any forward version (>=2) with no written
        // forward-version policy must be surfaced as errors, never coerced to
        // 0 (legacy) or 1 (v1). The row/blobs are preserved verbatim — the
        // caller does not rewrite them (Important 4).
        let execution_version_raw = row.execution_version;
        if execution_version_raw < 0 {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': negative execution_version {execution_version_raw} \
                     (non-replayable)",
                    session_id.0
                )),
            ));
        }
        if execution_version_raw > 1 {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': unsupported execution_version {execution_version_raw} \
                     (non-replayable, no forward-version policy)",
                    session_id.0
                )),
            ));
        }
        let execution_version = execution_version_raw as u32;

        // A negative/out-of-range integer revision on a v0 OR v1 row is
        // corrupt and non-replayable (A2/A7): it must be surfaced as an
        // error, never silently coerced to zero. The original row bytes are
        // preserved verbatim (the caller does not rewrite them).
        if row.state_revision < 0 {
            let rev = row.state_revision;
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': negative state_revision {rev} (non-replayable)",
                    session_id.0
                )),
            ));
        }
        let state_revision = u64::try_from(row.state_revision).unwrap_or(0);

        // v0 rows are legacy/unverified: no descriptor/state, status is
        // diagnostic only. v1 rows carry the authoritative descriptor/state.
        if execution_version >= 1 {
            // A v1 row's status is authoritative (A2). An unknown/unsupported
            // status value is corrupt/ambiguous and must be surfaced as
            // non-replayable (A7), never silently reinterpreted as Running.
            let status = SessionStatus::from_db_str(&row.status).ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': unknown/unsupported v1 status {:?} (non-replayable)",
                    session_id.0, row.status
                )))
            })?;
            // A v1 row must carry both descriptor and state; a missing blob
            // is corrupt/unsupported and non-replayable (A7).
            let descriptor_bytes = row.run_descriptor_json.as_deref().ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': v1 row missing run_descriptor_json (non-replayable)",
                    session_id.0
                )))
            })?;
            let state_bytes = row.run_state_json.as_deref().ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': v1 row missing run_state_json (non-replayable)",
                    session_id.0
                )))
            })?;
            let descriptor = deserialize_blob::<RunDescriptorV1>(descriptor_bytes, "run_descriptor_json")?;
            let state = deserialize_blob::<RunStateV1>(state_bytes, "run_state_json")?;
            Ok(Some(RunRecord {
                session_id: SessionId(row.session_id),
                status,
                state_revision,
                execution_version,
                descriptor: Some(descriptor),
                state: Some(state),
            }))
        } else {
            // v0 legacy row: status is diagnostic only. A corrupt/ambiguous
            // status must remain distinct and non-replayable (A7) — never
            // silently reinterpreted as `running`. A negative or out-of-range
            // integer revision is also corrupt and must not be coerced to
            // zero (A2/A7): preserve the row but surface it as unknown.
            let status = SessionStatus::from_db_str(&row.status).ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "load_run '{}': unknown/unsupported v0 status {:?} (non-replayable)",
                    session_id.0, row.status
                )))
            })?;
            Ok(Some(RunRecord {
                session_id: SessionId(row.session_id),
                status,
                state_revision,
                execution_version,
                descriptor: None,
                state: None,
            }))
        }
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &RunDescriptorV1,
        checkpoint: RunCheckpoint<'_>,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError> {
        let now = chrono::Utc::now().timestamp();
        let id = session_id.0.clone();
        let creator_id = descriptor.creator_id.clone();
        let preset_id = descriptor.preset_id.clone();
        let preset_version = i64::from(descriptor.preset_version);
        let parent_session_id = descriptor
            .parent_session_id
            .as_ref()
            .map(|s| s.0.clone());
        let current_task_id = checkpoint.root.current_task_id.clone();
        let context_bytes = serde_json::to_vec(&checkpoint.root.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "serialize context: {e}"
            )))
        })?;
        let state_bytes = serialize_blob(next_state)?;
        let descriptor_bytes = serialize_blob(descriptor)?;

        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "start_run begin tx: {e}"
                )))
            })?;

        // A2: a real explicit new start creates a NEW v1 run. It never
        // launders an existing uncertain legacy row in place — an existing
        // row (any execution_version, including a v0 legacy row) must be
        // left untouched and the caller must mint a new session id.
        let existing_version: Option<i64> = sqlx::query_scalar(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "start_run '{}': {e}", session_id.0
            )))
        })?;

        if let Some(version) = existing_version {
            return Err(EngineError::RunAlreadyExists {
                session_id: session_id.0.clone(),
                execution_version: u32::try_from(version).unwrap_or(0),
            });
        }

        // Fresh row: insert the authoritative v1 record.
        sqlx::query!(
            r#"
            INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version,
                 parent_session_id, current_task_id, status,
                 context_json, chat_history_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
            VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, ?, ?, 1, 1, ?, ?)
            "#,
            id,
            creator_id,
            preset_id,
            preset_version,
            parent_session_id,
            current_task_id,
            context_bytes,
            now,
            now,
            state_bytes,
            descriptor_bytes
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "start_run '{}': {e}", session_id.0
            )))
        })?;

        // Persist child checkpoints atomically under the root start. Each
        // child inherits the trusted root identity (A2/A4) and is
        // revision-fenced + terminal-fenced (Important 2).
        for child in checkpoint.children {
            let child_ctx = serde_json::to_vec(&child.session.context).map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "serialize child context: {e}"
                )))
            })?;
            let child_state = serialize_blob(&child.state)?;
            let child_descriptor = child_descriptor(descriptor, &session_id.0, child);
            let child_descriptor_bytes = serialize_blob(&child_descriptor)?;
            let child_status = child.status.as_db_str();
            let child_id = child.session.id.clone();
            let child_task = child.session.current_task_id.clone();
            let child_creator = child_descriptor.creator_id.clone();
            let child_preset = child_descriptor.preset_id.clone();
            let child_preset_version = i64::from(child_descriptor.preset_version);
            let child_parent = child_descriptor
                .parent_session_id
                .as_ref()
                .map(|s| s.0.clone());
            let child_revision = child.state_revision as i64;
            let child_result = sqlx::query!(
                r#"
                INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version,
                     parent_session_id, current_task_id, status,
                     context_json, chat_history_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 1, ?, ?, ?)
                ON CONFLICT(session_id) DO UPDATE SET
                    status = excluded.status,
                    current_task_id = excluded.current_task_id,
                    context_json = excluded.context_json,
                    updated_at = excluded.updated_at,
                    execution_version = 1,
                    state_revision = state_revision + 1,
                    run_state_json = excluded.run_state_json,
                    run_descriptor_json = excluded.run_descriptor_json
                WHERE orchestration_sessions.state_revision = ?
                  AND orchestration_sessions.status NOT IN
                      ('completed', 'failed', 'cancelled', 'interrupted')
                "#,
                child_id,
                child_creator,
                child_preset,
                child_preset_version,
                child_parent,
                child_task,
                child_status,
                child_ctx,
                now,
                now,
                child_revision,
                child_state,
                child_descriptor_bytes,
                child_revision
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "start_run child '{child_id}': {e}"
                )))
            })?;

            // A child CAS that matches zero rows means the child's persisted
            // revision no longer equals the expected value, or the child is
            // already terminal. The root start must roll back rather than
            // silently drop the child checkpoint (Important 1). A child that
            // is already terminal at the expected revision is confirmed (no
            // overwrite needed) and does not fail the root start.
            if child_result.rows_affected() == 0 {
                if let Some(err) =
                    child_cas_outcome(&mut tx, &child_id, child.state_revision, child.status.clone()).await
                {
                    return Err(err);
                }
            }
        }

        tx.commit().await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "start_run commit: {e}"
            )))
        })?;

        Ok(RunRecord {
            session_id: SessionId(session_id.0.clone()),
            status: SessionStatus::Running,
            state_revision: 1,
            execution_version: 1,
            descriptor: Some(descriptor.clone()),
            state: Some(next_state.clone()),
        })
    }

    async fn commit_transition(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError> {
        let now = chrono::Utc::now().timestamp();
        let id = session_id.0.clone();
        let current_task_id = checkpoint.root.current_task_id.clone();
        let context_bytes = serde_json::to_vec(&checkpoint.root.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "serialize context: {e}"
            )))
        })?;
        let state_bytes = serialize_blob(next_state)?;
        let status_str = next_status.as_db_str();

        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "commit_transition begin tx: {e}"
                )))
            })?;

        // Revision CAS + terminal/cancelled fence: only advance when the
        // persisted revision still matches AND the row is not already
        // terminal/cancelled. A late graph save cannot overwrite newer
        // checkpoint state.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r#"
            UPDATE orchestration_sessions
            SET status = ?, current_task_id = ?, context_json = ?,
                updated_at = ?, state_revision = state_revision + 1,
                run_state_json = ?,
                execution_version = CASE
                    WHEN execution_version = 0 AND run_descriptor_json IS NOT NULL THEN 1
                    ELSE execution_version END
            WHERE session_id = ? AND state_revision = ?
              AND status NOT IN ('completed', 'failed', 'cancelled', 'interrupted')
            "#,
            status_str,
            current_task_id,
            context_bytes,
            now,
            state_bytes,
            id,
            expected_revision_i64
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "commit_transition '{}': {e}", session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // Distinguish a revision mismatch from a terminal/cancelled fence.
            let current = sqlx::query_scalar::<_, i64>(
                "SELECT state_revision FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(&session_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "commit_transition read revision: {e}"
                )))
            })?;

            return match current {
                Some(found) if found != expected_revision as i64 => {
                    Err(EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(found).unwrap_or(0),
                    })
                }
                _ => Err(EngineError::TerminalState(session_id.0.clone())),
            };
        }

        // Read the root descriptor (promoted to v1 if it was a v0 row) so
        // the returned record is authoritative and reconstructible (Important 4).
        let root_descriptor = read_root_descriptor(&mut tx, &session_id.0).await?;

        // Read the actual persisted execution_version (a descriptorless v0
        // row stays legacy; a v0 row with a descriptor is promoted to v1).
        let persisted_version: i64 = sqlx::query_scalar(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&session_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "commit_transition read execution_version: {e}"
            )))
        })?;
        // A corrupt/unsupported execution_version is non-replayable (A7) —
        // never coerce negative values to 0 or forward versions to 1. The
        // root update already committed inside this transaction; returning a
        // GraphFlow error rolls it back, preserving the row verbatim
        // (Important 4).
        if persisted_version < 0 || persisted_version > 1 {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "commit_transition '{}': unsupported execution_version {persisted_version} \
                     (non-replayable)",
                    session_id.0
                )),
            ));
        }
        let execution_version = persisted_version as u32;

        // Persist child checkpoints atomically under the root transition.
        // Each child inherits the trusted root identity (A2/A4) and is
        // revision-fenced + terminal-fenced (Important 2).
        for child in checkpoint.children {
            let child_ctx = serde_json::to_vec(&child.session.context).map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "serialize child context: {e}"
                )))
            })?;
            let child_state = serialize_blob(&child.state)?;
            let child_descriptor = match &root_descriptor {
                Some(root) => child_descriptor(root, &session_id.0, child),
                None => {
                    // No root descriptor (v0 root): the child cannot inherit
                    // a trusted identity. Refuse rather than write
                    // "unknown"/"default" (A2/A4).
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::StorageError(format!(
                            "commit_transition '{}': root has no descriptor; \
                             cannot persist child identity",
                            session_id.0
                        )),
                    ));
                }
            };
            let child_descriptor_bytes = serialize_blob(&child_descriptor)?;
            let child_status = child.status.as_db_str();
            let child_id = child.session.id.clone();
            let child_task = child.session.current_task_id.clone();
            let child_creator = child_descriptor.creator_id.clone();
            let child_preset = child_descriptor.preset_id.clone();
            let child_preset_version = i64::from(child_descriptor.preset_version);
            let child_parent = child_descriptor
                .parent_session_id
                .as_ref()
                .map(|s| s.0.clone());
            let child_revision = child.state_revision as i64;
            let child_result = sqlx::query!(
                r#"
                INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version,
                     parent_session_id, current_task_id, status,
                     context_json, chat_history_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 1, ?, ?, ?)
                ON CONFLICT(session_id) DO UPDATE SET
                    status = excluded.status,
                    current_task_id = excluded.current_task_id,
                    context_json = excluded.context_json,
                    updated_at = excluded.updated_at,
                    execution_version = 1,
                    state_revision = state_revision + 1,
                    run_state_json = excluded.run_state_json,
                    run_descriptor_json = excluded.run_descriptor_json
                WHERE orchestration_sessions.state_revision = ?
                  AND orchestration_sessions.status NOT IN
                      ('completed', 'failed', 'cancelled', 'interrupted')
                "#,
                child_id,
                child_creator,
                child_preset,
                child_preset_version,
                child_parent,
                child_task,
                child_status,
                child_ctx,
                now,
                now,
                child_revision,
                child_state,
                child_descriptor_bytes,
                child_revision
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "commit_transition child '{child_id}': {e}"
                )))
            })?;

            // A child CAS that matches zero rows means the child's persisted
            // revision no longer equals the expected value, or the child is
            // already terminal. The root transition must roll back rather
            // than silently drop the child checkpoint (Important 1). A child
            // that is already terminal at the expected revision is confirmed
            // (no overwrite needed) and does not fail the root transition.
            if child_result.rows_affected() == 0 {
                if let Some(err) =
                    child_cas_outcome(&mut tx, &child_id, child.state_revision, child.status.clone()).await
                {
                    return Err(err);
                }
            }
        }

        tx.commit().await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "commit_transition commit: {e}"
            )))
        })?;

        Ok(RunRecord {
            session_id: SessionId(session_id.0.clone()),
            status: next_status,
            state_revision: expected_revision + 1,
            execution_version,
            descriptor: root_descriptor,
            state: Some(next_state.clone()),
        })
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<RunRecord>, EngineError> {
        let parent = parent_session_id.0.clone();
        let rows = sqlx::query_as!(
            RunRow,
            r#"SELECT session_id as "session_id!", status as "status!",
                      execution_version as "execution_version!",
                      state_revision as "state_revision!", run_state_json, run_descriptor_json
               FROM orchestration_sessions WHERE parent_session_id = ?"#,
            parent
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "load_children '{}': {e}", parent_session_id.0
            )))
        })?;

        let mut children = Vec::with_capacity(rows.len());
        for row in rows {
            // A corrupt/unsupported execution_version is non-replayable (A7):
            // never coerce negative values to 0 or forward versions (>=2) to
            // v1. Preserve the row verbatim (Important 4).
            if row.execution_version < 0 {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': child '{}' has negative execution_version {} \
                         (non-replayable)",
                        parent_session_id.0, row.session_id, row.execution_version
                    )),
                ));
            }
            if row.execution_version > 1 {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': child '{}' has unsupported execution_version {} \
                         (non-replayable)",
                        parent_session_id.0, row.session_id, row.execution_version
                    )),
                ));
            }
            let execution_version = row.execution_version as u32;
            // A negative child `state_revision` is corrupt/non-replayable
            // (A7) — never coerce it to zero. Preserve the row verbatim
            // (Important 1), exactly as `load_run` does for the root row.
            let state_revision_i64 = row.state_revision;
            if state_revision_i64 < 0 {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': child '{}' has negative state_revision \
                         {state_revision_i64} (non-replayable)",
                        parent_session_id.0, row.session_id
                    )),
                ));
            }
            let state_revision = u64::try_from(state_revision_i64).unwrap_or(0);
            if execution_version >= 1 {
                let status = SessionStatus::from_db_str(&row.status).ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': unknown/unsupported v1 status {:?} (non-replayable)",
                        row.session_id, row.status
                    )))
                })?;
                let descriptor_bytes = row.run_descriptor_json.as_deref().ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': v1 child missing run_descriptor_json (non-replayable)",
                        row.session_id
                    )))
                })?;
                let state_bytes = row.run_state_json.as_deref().ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': v1 child missing run_state_json (non-replayable)",
                        row.session_id
                    )))
                })?;
                let descriptor =
                    deserialize_blob::<RunDescriptorV1>(descriptor_bytes, "run_descriptor_json")?;
                let state = deserialize_blob::<RunStateV1>(state_bytes, "run_state_json")?;
                children.push(RunRecord {
                    session_id: SessionId(row.session_id),
                    status,
                    state_revision,
                    execution_version,
                    descriptor: Some(descriptor),
                    state: Some(state),
                });
            } else {
                // A v0 child row is legacy/unverified; surface it as a
                // non-replayable error rather than silently dropping it.
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "load_children '{}': v0 legacy child row (non-replayable)",
                        row.session_id
                    )),
                ));
            }
        }
        Ok(children)
    }

    /// Atomically restore a pre-step root snapshot via ONE revision-fenced
    /// storage operation (Important 3).
    ///
    /// This replaces the round-4 check-then-save race: the restore writes the
    /// pre-step position/context inside a single `state_revision = expected`
    /// AND non-terminal-fenced UPDATE. A concurrent transition that advanced
    /// the revision (or moved the row terminal) between a check and a separate
    /// save is never overwritten — the row is left untouched and a
    /// `RevisionMismatch`/`TerminalState` is returned.
    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<(), EngineError> {
        let now = chrono::Utc::now().timestamp();
        let id = session_id.0.clone();
        let context_bytes = serde_json::to_vec(&pre_step.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "restore_pre_step '{}': serialize context: {e}",
                session_id.0
            )))
        })?;
        let current_task_id = pre_step.current_task_id.clone();

        // The restored position is a safe pre-step boundary: clear any
        // in-flight markers the aborted step's `mark_step_in_flight` wrote so
        // the restored row is not left pretending a step is dispatching
        // (A2/A7 Important 3 + 5).
        let cleared_state = RunStateV1 {
            step_in_flight: None,
            in_flight: None,
            ..RunStateV1::default()
        };
        let state_bytes = serialize_blob(&cleared_state)?;

        // ONE atomic operation: restore only when the persisted revision still
        // matches AND the row is a status that may actually be stepped
        // (explicit running/paused intent — a paused row is re-stepped by the
        // drive loop and its position must restore too). `waiting_for_input`
        // (and unknown statuses) are excluded so a stale step invocation can
        // never clear or overwrite a durable human-wait row, including its
        // WaitRecord token (A4). A concurrent transition that won between a
        // check and a separate save is left untouched.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r#"
            UPDATE orchestration_sessions
            SET current_task_id = ?, context_json = ?, updated_at = ?, run_state_json = ?
            WHERE session_id = ? AND state_revision = ?
              AND status IN ('running', 'paused')
            "#,
            current_task_id,
            context_bytes,
            now,
            state_bytes,
            id,
            expected_revision_i64
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "restore_pre_step '{}': {e}", session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // Distinguish a revision mismatch from a non-running row.
            let current = sqlx::query_scalar::<_, i64>(
                "SELECT state_revision FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(&session_id.0)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "restore_pre_step read revision: {e}"
                )))
            })?;
            return match current {
                Some(found) if found != expected_revision as i64 => {
                    Err(EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(found).unwrap_or(0),
                    })
                }
                _ => Err(EngineError::TerminalState(session_id.0.clone())),
            };
        }
        Ok(())
    }

    /// Persist the in-flight (current-position safety) intent BEFORE an
    /// external effect dispatch (A2/A7 Important 5), so a crash after an
    /// effect but before the result checkpoint leaves an interrupted (never
    /// replayable) run.
    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: RunCheckpoint<'_>,
        step_state: &RunStateV1,
    ) -> Result<(), EngineError> {
        let now = chrono::Utc::now().timestamp();
        let id = session_id.0.clone();
        let current_task_id = checkpoint.root.current_task_id.clone();
        let context_bytes = serde_json::to_vec(&checkpoint.root.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "mark_step_in_flight '{}': serialize context: {e}",
                session_id.0
            )))
        })?;
        let state_bytes = serialize_blob(step_state)?;

        // Fence to the statuses that may actually be stepped (`status`
        // running or paused — a paused session is re-stepped by the drive
        // loop) AND `state_revision = expected`. `waiting_for_input` (and
        // unknown statuses) are excluded so a stale step invocation can never
        // overwrite a durable human-wait row, including its WaitRecord token
        // (A4). Deliberately does NOT advance the revision (a subsequent
        // `commit_transition` still CAS-anchors to the same pre-step
        // revision); it only marks the durable in-flight intent.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r#"
            UPDATE orchestration_sessions
            SET current_task_id = ?, context_json = ?, updated_at = ?, run_state_json = ?
            WHERE session_id = ? AND status IN ('running', 'paused')
              AND state_revision = ?
            "#,
            current_task_id,
            context_bytes,
            now,
            state_bytes,
            id,
            expected_revision_i64
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "mark_step_in_flight '{}': {e}", session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // The pre-step row is no longer running or its revision moved —
            // the external effect must not run on an unmarked row.
            let current: Option<(String, i64)> = sqlx::query_as(
                "SELECT status, state_revision FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(&session_id.0)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "mark_step_in_flight read row: {e}"
                )))
            })?;
            return Err(match current {
                Some((_status, rev)) if rev != expected_revision as i64 => {
                    EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(rev).unwrap_or(0),
                    }
                }
                Some(_) => EngineError::TerminalState(session_id.0.clone()),
                None => EngineError::SessionNotFound(session_id.0.clone()),
            });
        }
        Ok(())
    }
}

/// Internal row mapping for SELECT queries.
#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id: String,
    current_task_id: Option<String>,
    context_json: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: open a fresh on-disk temp `SQLite` pool with migrations applied.
    async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::NamedTempFile) {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        (Arc::new(pool), db)
    }

    #[tokio::test]
    async fn session_roundtrip() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool);
        let storage: Arc<dyn SessionStorage> = Arc::new(storage);

        let session = Session::new_from_task("sess-001".into(), "dummy-task");
        storage.save(session).await.unwrap();
        let loaded = storage
            .get("sess-001")
            .await
            .unwrap()
            .expect("session present");
        assert_eq!(loaded.id, "sess-001");
        storage.delete("sess-001").await.unwrap();
        assert!(storage.get("sess-001").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn restart_resume_smoke() {
        let db = tempfile::NamedTempFile::new().unwrap();
        {
            let pool = nexus_local_db::open_pool(db.path())
                .await
                .expect("open pool (first)");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("run migrations (first)");
            let storage = SqliteSessionStorage::new(std::sync::Arc::new(pool));
            let session = Session::new_from_task("sess-restart".into(), "dummy-task");
            storage.save(session).await.unwrap();
        } // pool drops — simulates daemon shutdown
        {
            let pool = nexus_local_db::open_pool(db.path())
                .await
                .expect("open pool (second)");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("run migrations (second) — idempotent");
            let storage = SqliteSessionStorage::new(std::sync::Arc::new(pool));
            assert!(storage.get("sess-restart").await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn save_upserts_existing_session() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool);

        let mut session = Session::new_from_task("sess-upsert".into(), "task-a");
        storage.save(session.clone()).await.unwrap();

        // Update with a different task id.
        session.current_task_id = "task-b".to_string();
        storage.save(session).await.unwrap();

        let loaded = storage.get("sess-upsert").await.unwrap().unwrap();
        assert_eq!(loaded.current_task_id, "task-b");
    }

    /// Seed a raw `orchestration_sessions` row (test-only DML).
    async fn seed_row(
        pool: &sqlx::SqlitePool,
        session_id: &str,
        status: &str,
        current_task_id: Option<&str>,
        context: &[u8],
    ) {
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at)
             VALUES (?, 'ctr_t', 'preset_t', 7, ?, ?, ?, 1756990000, 1756990300)",
        )
        .bind(session_id)
        .bind(status)
        .bind(current_task_id)
        .bind(context)
        .execute(pool)
        .await
        .expect("seed row");
    }

    #[tokio::test]
    async fn list_checkpoint_rows_filters_non_terminal_and_projects_verdict_inputs() {
        let (pool, _db) = fresh_pool().await;
        // Chain-class context: live join keys, no failure record.
        seed_row(
            &pool,
            "sess-run",
            "running",
            Some("task_1"),
            br#"{"data": {"_converge_arrivals_j1": ["a"], "_join_wait_start_j1": 1}}"#,
        )
        .await;
        // Typed-failure context.
        seed_row(
            &pool,
            "sess-pause",
            "paused",
            None,
            br#"{"data": {"_run_error": "boom"}}"#,
        )
        .await;
        // Non-class context.
        seed_row(
            &pool,
            "sess-wait",
            "waiting_for_input",
            None,
            br#"{"data": {"_creator_id": "ctr_t"}}"#,
        )
        .await;
        // Terminal rows must be filtered out (recovery-filter status set).
        seed_row(&pool, "sess-done", "completed", Some("task_9"), b"{}").await;
        seed_row(&pool, "sess-cancelled", "cancelled", None, b"{}").await;

        let storage = SqliteSessionStorage::new(pool);
        let rows = storage.list_checkpoint_rows().await.expect("list rows");
        let ids: Vec<&str> = rows.iter().map(|r| r.session_id.as_str()).collect();
        // All seeds share the same updated_at → session_id DESC tie-break.
        assert_eq!(ids, ["sess-wait", "sess-run", "sess-pause"]);

        let by_id = |id: &str| rows.iter().find(|r| r.session_id == id).unwrap();
        let run = by_id("sess-run");
        assert_eq!(run.creator_id, "ctr_t");
        assert_eq!(run.preset_id, "preset_t");
        assert_eq!(run.preset_version, 7);
        assert_eq!(run.current_task_id.as_deref(), Some("task_1"));
        assert_eq!(run.status, "running");
        assert_eq!(run.created_at, 1_756_990_000);
        assert_eq!(run.updated_at, 1_756_990_300);
        assert!(run.context_valid_json, "chain context is valid json");
        assert!(run.context_data_is_object);
        assert_eq!(run.run_status, None);
        assert_eq!(run.run_error, None);
        assert_eq!(
            run.live_join_keys.as_deref(),
            Some("_converge_arrivals_j1,_join_wait_start_j1")
        );

        let pause = by_id("sess-pause");
        assert_eq!(pause.run_error.as_deref(), Some("boom"));
        assert_eq!(pause.run_status, None);
        assert_eq!(pause.live_join_keys, None);

        let wait = by_id("sess-wait");
        assert_eq!(wait.run_status, None);
        assert_eq!(wait.run_error, None);
        assert_eq!(wait.live_join_keys, None);
    }

    #[tokio::test]
    async fn list_checkpoint_rows_surfaces_corrupt_context_without_failing() {
        let (pool, _db) = fresh_pool().await;
        seed_row(&pool, "sess-bad", "running", None, b"not-json-at-all").await;

        let storage = SqliteSessionStorage::new(pool);
        let rows = storage.list_checkpoint_rows().await.expect("list rows");
        assert_eq!(rows.len(), 1);
        assert!(
            !rows[0].context_valid_json,
            "corrupt blob reported honestly"
        );
        assert!(!rows[0].context_data_is_object);
        assert_eq!(rows[0].run_status, None);
        assert_eq!(rows[0].run_error, None);
        assert_eq!(rows[0].live_join_keys, None);
    }

    #[tokio::test]
    async fn list_checkpoint_rows_is_bounded_and_count_matches_total() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());
        for i in 0..205 {
            seed_row(
                &pool,
                &format!("sess-{i:03}"),
                "running",
                None,
                br#"{"data": {}}"#,
            )
            .await;
        }

        let rows = storage.list_checkpoint_rows().await.expect("list rows");
        assert_eq!(rows.len(), 200, "list bounded to 200 rows");
        // updated_at DESC (all 1756990300) then session_id DESC tie-break:
        // the most recently seeded (highest id) row comes first.
        assert_eq!(rows[0].session_id, "sess-204");
        assert_eq!(rows[199].session_id, "sess-005");
        assert_eq!(
            storage.count_checkpoint_rows().await.expect("count"),
            205,
            "honest total must not be truncated"
        );
    }

    #[tokio::test]
    async fn get_checkpoint_row_is_by_id_without_status_filter() {
        let (pool, _db) = fresh_pool().await;
        seed_row(&pool, "sess-done", "completed", None, b"{\"data\":{}}").await;

        let storage = SqliteSessionStorage::new(pool);
        let row = storage
            .get_checkpoint_row("sess-done")
            .await
            .expect("get row")
            .expect("terminal rows are visible in detail mode");
        assert_eq!(row.status, "completed");
        assert_eq!(row.context_json, b"{\"data\":{}}");

        let missing = storage
            .get_checkpoint_row("sess-nope")
            .await
            .expect("get missing");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn checkpoint_accessors_leave_rows_untouched() {
        let (pool, _db) = fresh_pool().await;
        seed_row(&pool, "sess-run", "running", Some("task_1"), b"{}").await;

        let storage = SqliteSessionStorage::new(pool.clone());
        storage.list_checkpoint_rows().await.expect("list");
        storage.get_checkpoint_row("sess-run").await.expect("get");
        storage.count_checkpoint_rows().await.expect("count");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = 'sess-run'",
        )
        .fetch_one(&*pool)
        .await
        .expect("count");
        assert_eq!(count, 1, "read-only accessors must not write");
    }
}
