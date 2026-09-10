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
//! - `status` — new rows insert as `"running"`; existing rows are never
//!   rewritten. The position/context-only save seam updates
//!   `current_task_id` / `context_json` / `updated_at` only when the row is
//!   in the re-stepable set `status IN ('running', 'paused')`; protected
//!   durable state (`waiting_for_input`, terminal, `cancelled`,
//!   `interrupted`) is fenced out and stays an expected no-op (A4).
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
    ChildCheckpoint, PromptAttempt, RunCheckpoint, RunDescriptorV1, RunRecord, RunStateV1,
    WorkflowStateStore,
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
               WHERE parent_session_id IS NULL
                 AND status IN ('running', 'paused', 'waiting_for_input')"#
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
                    status, context_json, execution_version, state_revision, run_state_json,
                    run_descriptor_json, created_at, updated_at
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
    /// Bounded to the 200 most recently updated recovery-relevant rows,
    /// including `interrupted` operator evidence. The list never grows
    /// unbounded, and `context_json` (which embeds chat history) is not
    /// loaded: legacy predicates and the exact current-task gate-park marker
    /// are projected in SQL. `json_each` is guarded by a
    /// `json_type($.data) = 'object'` check, so corrupt or non-object context
    /// cannot raise.
    ///
    /// The durable v1 state and descriptor blobs are projected raw and
    /// structurally validated in Rust by the CLI consumer. `json_valid`
    /// alone never labels corrupt evidence as readable; list, detail, and
    /// boot agree on malformed v1 metadata.
    ///
    /// Ordering is `updated_at DESC`; a secondary `session_id DESC` tie-break
    /// keeps order deterministic for identical timestamps (bulk seeds).
    ///
    /// # Errors
    /// Returns the verbatim `sqlx::Error` when the query fails.
    pub async fn list_checkpoint_rows(&self) -> Result<Vec<CheckpointSummary>, sqlx::Error> {
        sqlx::query_as::<_, CheckpointSummary>(
            "SELECT session_id, creator_id, preset_id, preset_version, current_task_id,
                    status, execution_version, state_revision, run_state_json,
                    run_descriptor_json, created_at, updated_at,
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
                    END AS live_join_keys,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data') = 'object'
                        THEN EXISTS(
                            SELECT 1
                            FROM json_each(json_extract(context_json, '$.data'))
                            WHERE key = '_gate_park_' || COALESCE(current_task_id, '')
                              AND value IS NOT NULL
                        )
                        ELSE 0
                    END AS gate_park_live
             FROM orchestration_sessions
             WHERE status IN ('running', 'paused', 'waiting_for_input', 'interrupted')
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
             WHERE status IN ('running', 'paused', 'waiting_for_input', 'interrupted')",
        )
        .fetch_one(&*self.pool)
        .await
    }
}

#[async_trait]
impl SessionStorage for SqliteSessionStorage {
    async fn save(&self, session: Session) -> graph_flow::Result<()> {
        let now = chrono::Utc::now().timestamp();

        // graph-flow 0.8 OCC: the incoming `Session.version` maps exclusively
        // to the durable `graph_version` column (brief §3.1 — `state_revision`
        // stays the workflow control CAS; `execution_version` stays the format
        // marker). Checked conversions: an incoming version outside SQLite's
        // signed range or an exhausted counter is a hard storage error with
        // NO write — never wrapped, saturated, or clamped.
        let incoming_version = i64::try_from(session.version).map_err(|_| {
            graph_flow::GraphError::StorageError(format!(
                "save session '{}': incoming graph version {} exceeds SQLite's signed range",
                session.id, session.version
            ))
        })?;
        let next_version = incoming_version.checked_add(1).ok_or_else(|| {
            graph_flow::GraphError::StorageError(format!(
                "save session '{}': graph version counter exhausted",
                session.id
            ))
        })?;

        // Extract metadata from context (synchronous get deserializes).
        let creator_id: String = session
            .context.get("_creator_id").unwrap_or_else(|| "unknown".to_string());
        let preset_id: String = session
            .context.get("_preset_id").unwrap_or_else(|| "default".to_string());
        let preset_version: i64 = session.context.get("_preset_version").unwrap_or(0);
        let parent_session_id: Option<String> = session.context.get("_parent_session_id");

        // Serialize the entire context (includes chat history).
        let context_bytes = serde_json::to_vec(&session.context)
            .map_err(|e| graph_flow::GraphError::StorageError(format!("serialize context: {e}")))?;

        // Pre-own all bind params before the macro call (borrow lifetimes).
        let session_id = session.id;
        let current_task_id = session.current_task_id;

        // Atomic position/context-only upsert + zero-row classification under
        // one `BEGIN IMMEDIATE` write lock (the same convention as
        // `commit_transition` / `restore_pre_step`). The write and the
        // classification read are one atomic observation — no check-then-write
        // race that could re-open overwrite risk.
        //
        // graph-flow 0.8 OCC contract (brief §3.2): an absent row inserts with
        // the incremented graph_version; an existing row updates ONLY when it
        // is re-stepable (`status IN ('running','paused')`) AND its stored
        // graph_version still equals the incoming session version. A stale
        // version or a protected durable status is a `SessionConflict`, not a
        // successful no-op. Only cursor/context/timestamp/graph_version move
        // here — never status, workflow revision, descriptor, ownership,
        // execution format, or wait token.
        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                graph_flow::GraphError::StorageError(format!(
                    "save session '{session_id}': begin tx: {e}"
                ))
            })?;

        let result = sqlx::query!(
            r#"
            INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version,
                 parent_session_id, current_task_id, status,
                 context_json, chat_history_json, created_at, updated_at,
                 graph_version)
            VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                current_task_id = excluded.current_task_id,
                context_json     = excluded.context_json,
                updated_at       = excluded.updated_at,
                graph_version    = excluded.graph_version
            WHERE orchestration_sessions.status IN ('running', 'paused')
              AND orchestration_sessions.graph_version = ?
            "#,
            session_id,
            creator_id,
            preset_id,
            preset_version,
            parent_session_id,
            current_task_id,
            context_bytes,
            now,
            now,
            next_version,
            incoming_version
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            graph_flow::GraphError::StorageError(format!("save session '{session_id}': {e}"))
        })?;
        // A missing row inserts (rows_affected = 1). Zero rows means the row
        // already exists outside the admitted set: a stale graph version, a
        // protected durable status, or a corrupt/unknown row. Classify the
        // outcome under the same write transaction instead of silently
        // reporting success: in 0.8 a successful save promises a versioned
        // write, so a refused save reports `SessionConflict` (stale version /
        // protected known status) or an explicit storage error
        // (unknown/corrupt) — never a fake success, never a mutation on
        // refusal.
        if result.rows_affected() == 0 {
            #[derive(sqlx::FromRow)]
            struct FenceRow {
                status: String,
                graph_version: i64,
            }
            let current: Option<FenceRow> = sqlx::query_as!(
                FenceRow,
                "SELECT status, graph_version FROM orchestration_sessions WHERE session_id = ?",
                session_id
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                graph_flow::GraphError::StorageError(format!(
                    "save session '{session_id}': read status/version: {e}"
                ))
            })?;
            let Some(row) = current else {
                return Err(graph_flow::GraphError::StorageError(format!(
                    "save session '{session_id}': row absent after upsert (non-replayable)"
                )));
            };
            // A negative stored graph version is corrupt: surface it, never
            // coerce, never write.
            if row.graph_version < 0 {
                return Err(graph_flow::GraphError::StorageError(format!(
                    "save session '{session_id}': negative graph_version {} (non-replayable; \
                     save not applied)",
                    row.graph_version
                )));
            }
            match SessionStatus::from_db_str(&row.status) {
                // Stepable status: the status fence passed, so the zero-row
                // cause is the graph_version OCC — a concurrent writer won.
                // (The write lock makes status-passing + version-matching
                // unreachable here; both shapes are the same concurrency
                // outcome: conflict, save not applied.)
                Some(SessionStatus::Running | SessionStatus::Paused) => {
                    return Err(graph_flow::GraphError::SessionConflict(format!(
                        "save session '{}': stored graph version {} no longer equals the \
                         incoming version {} (concurrent save won; save not applied)",
                        session_id, row.graph_version, incoming_version
                    )));
                }
                // Unknown/corrupt status: non-replayable (A7) — never a
                // silent success, never a conflict alias.
                None => {
                    return Err(graph_flow::GraphError::StorageError(format!(
                        "save session '{session_id}': unknown/corrupt status {:?} \
                         (non-replayable; save not applied)",
                        row.status
                    )));
                }
                // Protected durable state (waiting_for_input / terminal /
                // interrupted / cancelled): the save is REFUSED. 0.8 honesty:
                // report SessionConflict rather than the old fake-success
                // no-op; the durable row/blob/token/revisions/timestamp stay
                // byte-identical (A4).
                Some(_) => {
                    return Err(graph_flow::GraphError::SessionConflict(format!(
                        "save session '{}': durable status {:?} is protected from late \
                         graph saves (save not applied)",
                        session_id, row.status
                    )));
                }
            }
        }

        tx.commit().await.map_err(|e| {
            graph_flow::GraphError::StorageError(format!("save session '{session_id}': {e}"))
        })?;

        Ok(())
    }

    async fn get(&self, id: &str) -> graph_flow::Result<Option<Session>> {
        let id_owned = id.to_owned();
        let row = sqlx::query_as!(
            SessionRow,
            "SELECT session_id as \"session_id!\", current_task_id, context_json, graph_version
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

        // Checked conversion: a negative/corrupt stored graph version is a
        // hard storage error — never coerced to zero, never silently
        // deserialized away (brief §3.1).
        let version = u64::try_from(row.graph_version).map_err(|_| {
            graph_flow::GraphError::StorageError(format!(
                "get session '{id}': negative/corrupt graph_version {} (non-replayable)",
                row.graph_version
            ))
        })?;

        Ok(Some(Session {
            id: row.session_id,
            graph_id: "default".to_string(),
            current_task_id: row.current_task_id.unwrap_or_default(),
            status_message: None,
            context,
            version,
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

/// Checked conversion + increment of a graph-flow `Session.version` bound for
/// the durable `graph_version` column (brief §3.1): an incoming version outside
/// SQLite's signed range or an exhausted counter is a hard storage error with
/// no write — never wrapped, saturated, or clamped.
fn checked_next_graph_version(version: u64) -> Result<i64, EngineError> {
    let as_i64 = i64::try_from(version).map_err(|_| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "graph version {version} exceeds SQLite's signed range"
        )))
    })?;
    as_i64.checked_add(1).ok_or_else(|| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(
            "graph version counter exhausted".to_string(),
        ))
    })
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
/// (Important 6). Returns `Ok(Some(EngineError))` when the child CAS genuinely
/// failed: a stale revision (child moved on) or a terminal child at a stale
/// revision. SQL read failures are propagated so callers never mistake an
/// unreadable row for an absent row.
#[allow(clippy::cast_possible_wrap)] // SQLite column is i64; u64 revision fits signed range for all realistic runs
async fn child_cas_outcome(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    child_id: &str,
    expected_revision: u64,
    submitted_status: SessionStatus,
) -> Result<Option<EngineError>, EngineError> {
    let row = sqlx::query!(
        "SELECT state_revision, status FROM orchestration_sessions WHERE session_id = ?",
        child_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "read child CAS outcome for '{child_id}': {e}"
        )))
    })?;

    Ok(match row {
        Some(row) => {
            let found_rev = row.state_revision;
            let status = row.status;
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
        None => Some(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "child CAS failed for '{child_id}': row absent"
            )),
        )),
    })
}

/// Read the persisted root descriptor for a session within a transaction.
///
/// Returns `None` when the row has no descriptor (v0 legacy row).
async fn read_root_descriptor(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
) -> Result<Option<RunDescriptorV1>, EngineError> {
    let bytes: Option<Option<Vec<u8>>> = sqlx::query_scalar!(
        "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
        session_id
    )
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

#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::items_after_statements
)] // SQLite column types are i64; every cast is bounds-checked above the cast site
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
                "load_run '{}': {e}",
                session_id.0
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
            let descriptor =
                deserialize_blob::<RunDescriptorV1>(descriptor_bytes, "run_descriptor_json")?;
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
        let parent_session_id = descriptor.parent_session_id.as_ref().map(|s| s.0.clone());
        let current_task_id = checkpoint.root.current_task_id.clone();
        let context_bytes = serde_json::to_vec(&checkpoint.root.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "serialize context: {e}"
            )))
        })?;
        let state_bytes = serialize_blob(next_state)?;
        let descriptor_bytes = serialize_blob(descriptor)?;
        // graph-flow 0.8: seed the new row's durable graph_version from the
        // checked incoming checkpoint version + 1 (normally 1) — brief §3.3.
        let root_graph_version = checked_next_graph_version(checkpoint.root.version)?;

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
        let existing_version: Option<i64> = sqlx::query_scalar!(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
            id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "start_run '{}': {e}",
                session_id.0
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
                 execution_version, state_revision, run_state_json, run_descriptor_json,
                 graph_version)
            VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, ?, ?, 1, 1, ?, ?, ?)
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
            descriptor_bytes,
            root_graph_version
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "start_run '{}': {e}",
                session_id.0
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
            // graph-flow 0.8: a NEW child row seeds graph_version from the
            // checked incoming checkpoint version + 1; an existing row's
            // authoritative write advances the stored graph_version in the
            // same transaction (authority comes from the workflow CAS above,
            // not the stale pre-step session version — brief §3.3). The range
            // guard makes counter exhaustion/corruption fail closed (zero
            // rows -> typed error), never wrap into a REAL.
            let child_graph_version = checked_next_graph_version(child.session.version)?;
            let child_result = sqlx::query!(
                r#"
                INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version,
                     parent_session_id, current_task_id, status,
                     context_json, chat_history_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json,
                     graph_version)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 1, ?, ?, ?, ?)
                ON CONFLICT(session_id) DO UPDATE SET
                    status = excluded.status,
                    current_task_id = excluded.current_task_id,
                    context_json = excluded.context_json,
                    updated_at = excluded.updated_at,
                    execution_version = 1,
                    state_revision = state_revision + 1,
                    run_state_json = excluded.run_state_json,
                    run_descriptor_json = excluded.run_descriptor_json,
                    graph_version = orchestration_sessions.graph_version + 1
                WHERE orchestration_sessions.state_revision = ?
                  AND orchestration_sessions.status NOT IN
                      ('completed', 'failed', 'cancelled', 'interrupted')
                  AND orchestration_sessions.graph_version >= 0
                  AND orchestration_sessions.graph_version < 9223372036854775807
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
                child_graph_version,
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
                if let Some(err) = child_cas_outcome(
                    &mut tx,
                    &child_id,
                    child.state_revision,
                    child.status.clone(),
                )
                .await?
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

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &RunDescriptorV1,
        checkpoint: RunCheckpoint<'_>,
        next_state: &RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&crate::run_state::ScheduleAdmissionGate>,
    ) -> Result<RunRecord, EngineError> {
        let now = chrono::Utc::now().timestamp();
        let id = session_id.0.clone();
        let creator_id = descriptor.creator_id.clone();
        let preset_id = descriptor.preset_id.clone();
        let preset_version = i64::from(descriptor.preset_version);
        let parent_session_id = descriptor.parent_session_id.as_ref().map(|s| s.0.clone());
        let current_task_id = checkpoint.root.current_task_id.clone();
        let context_bytes = serde_json::to_vec(&checkpoint.root.context).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "serialize context: {e}"
            )))
        })?;
        let state_bytes = serialize_blob(next_state)?;
        let descriptor_bytes = serialize_blob(descriptor)?;
        let root_graph_version = checked_next_graph_version(checkpoint.root.version)?;

        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run begin tx: {e}"
                )))
            })?;

        // 1. Load + verify the schedule row inside the transaction. The
        //    claim (status/current_session_id) and the v1 run row commit
        //    together — a concurrent admission loses the claim and returns
        //    the winner's owned run (C-1).
        #[derive(sqlx::FromRow)]
        struct ScheduleClaimRow {
            execution_policy: String,
            status: String,
            current_session_id: Option<String>,
            preset_id: String,
            current_core_context_version: i64,
        }
        let row = sqlx::query_as!(
            ScheduleClaimRow,
            "SELECT execution_policy, status, current_session_id, preset_id,
                    current_core_context_version
             FROM creator_schedules WHERE schedule_id = ?",
            schedule_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run '{}': {e}",
                session_id.0
            )))
        })?
        .ok_or_else(|| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run: schedule {schedule_id} not found"
            )))
        })?;

        // N-3: the admission claim is fenced on the EXACT frozen
        // core-context version the caller read. A concurrent context edit
        // that advanced the row after the caller's read fails the fence —
        // the claim never overwrites a newer pointer with a stale payload.
        if u32::try_from(row.current_core_context_version).unwrap_or(0)
            != expected_core_context_version
        {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: schedule {schedule_id} core-context version \
                     moved (expected {expected_core_context_version}, found {})",
                    row.current_core_context_version
                )),
            ));
        }

        // N-1: re-verify the admission matrix INSIDE the claim transaction.
        // The preflight outside the transaction is advisory; the running set
        // is only authoritative under the `BEGIN IMMEDIATE` write lock, so
        // two distinct serial schedules for one creator can never both claim.
        if let Some(gate) = admission_gate {
            let now_ts = chrono::Utc::now().timestamp();
            if gate.scheduled_at.is_some_and(|at| at > now_ts) {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "admit_schedule_run: schedule {schedule_id} is not due \
                         (scheduled_at {})",
                        gate.scheduled_at.unwrap_or_default()
                    )),
                ));
            }
            for dep in &gate.depends_on {
                let dep_status: Option<String> = sqlx::query_scalar!(
                    "SELECT status FROM creator_schedules WHERE schedule_id = ?",
                    dep
                )
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "admit_schedule_run '{}': {e}",
                        session_id.0
                    )))
                })?;
                if !matches!(dep_status.as_deref(), Some("completed" | "cancelled")) {
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::StorageError(format!(
                            "admit_schedule_run: schedule {schedule_id} dependency \
                             '{dep}' is not completed/cancelled"
                        )),
                    ));
                }
            }
            // Per-creator running set: only driven_v1 rows count toward
            // capacity; the candidate's OWN row is excluded (a concurrent
            // admission that already claimed it must not look
            // serial-blocked — the loser re-reads the winner's run).
            let running: Vec<String> = sqlx::query_scalar!(
                r#"SELECT schedule_id as "schedule_id!" FROM creator_schedules
                 WHERE creator_id = ? AND status = 'running'
                   AND execution_policy = 'driven_v1'
                   AND schedule_id != ?"#,
                gate.creator_id,
                schedule_id
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run '{}': {e}",
                    session_id.0
                )))
            })?;
            let concurrency_ok = match gate.concurrency_kind.as_str() {
                "parallel_any" => true,
                "parallel_with" => {
                    let whitelist: std::collections::HashSet<String> = gate
                        .concurrency_whitelist
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok())
                        .unwrap_or_default();
                    running.iter().all(|id| whitelist.contains(id))
                }
                // "serial" (and any unknown kind — conservative fail-closed).
                _ => running.is_empty(),
            };
            if !concurrency_ok {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "admit_schedule_run: schedule {schedule_id} fails the \
                         per-creator concurrency gate ({} running)",
                        running.len()
                    )),
                ));
            }
        }

        // A8 fail-closed: a `_system.*` preset is never driven, regardless
        // of the stored policy (C-4).
        if row.preset_id.starts_with("_system.") {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: system preset '{0}' cannot be admitted as a driven run",
                    row.preset_id
                )),
            ));
        }
        let policy_ok =
            row.execution_policy == "driven_v1" || row.execution_policy == "legacy_inert";
        if !policy_ok {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: schedule {schedule_id} execution_policy is '{}'",
                    row.execution_policy
                )),
            ));
        }
        let status_ok = row.status == "pending"
            || row.status == "paused"
            || (row.status == "running" && row.current_session_id.is_none());
        if !status_ok {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: schedule {schedule_id} status is '{}'",
                    row.status
                )),
            ));
        }
        if let Some(existing) = &row.current_session_id {
            // Re-entry: the schedule already owns a run. Verify the run row
            // exists; a stale claim (crash between claim and run creation)
            // is NOT cleared here — boot recovery classifies it (C-1).
            let existing_version: Option<i64> = sqlx::query_scalar!(
                "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
                existing
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run '{}': {e}",
                    session_id.0
                )))
            })?;
            if existing_version.is_some() {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "admit_schedule_run: schedule {schedule_id} already owns run {existing}"
                    )),
                ));
            }
            // Stale claim: refuse fresh admission — the caller must route
            // through recovery, never mint a second session.
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: schedule {schedule_id} has a stale claim on {existing}; \
                     recovery must classify it before re-admission"
                )),
            ));
        }

        // 2. Insert the v1 session row (initial checkpoint + descriptor +
        //    seeded context) — the run is durable before the schedule
        //    pointer is published (C-2).
        let existing_version: Option<i64> = sqlx::query_scalar!(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
            id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run '{}': {e}",
                session_id.0
            )))
        })?;
        if let Some(version) = existing_version {
            return Err(EngineError::RunAlreadyExists {
                session_id: session_id.0.clone(),
                execution_version: u32::try_from(version).unwrap_or(0),
            });
        }

        sqlx::query!(
            r#"
            INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version,
                 parent_session_id, current_task_id, status,
                 context_json, chat_history_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json,
                 graph_version)
            VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, ?, ?, 1, 1, ?, ?, ?)
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
            descriptor_bytes,
            root_graph_version
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run '{}': {e}",
                session_id.0
            )))
        })?;

        // 3. Claim the schedule row atomically with the run row (C-1).
        let core_context_version_i64 = i64::from(core_context_version);
        let claim = sqlx::query!(
            "UPDATE creator_schedules
             SET status = 'running', current_session_id = ?,
                 current_core_context_version = ?, updated_at = ?,
                 execution_policy = 'driven_v1'
             WHERE schedule_id = ? AND status IN ('pending', 'paused', 'running')
               AND current_session_id IS NULL",
            session_id.0,
            core_context_version_i64,
            now,
            schedule_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run claim '{}': {e}",
                session_id.0
            )))
        })?;
        if claim.rows_affected() == 0 {
            // A concurrent admission won the claim between our read and
            // update. Roll back the run row (the transaction aborts) and
            // report the conflict — the caller re-reads the winner.
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run: schedule {schedule_id} was claimed concurrently"
                )),
            ));
        }

        tx.commit().await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "admit_schedule_run commit: {e}"
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

        // Revision CAS + terminal/cancelled + execution-version fence: only
        // authoritative v1 rows may advance. Legacy v0 rows are inspectable,
        // but cannot be mutated into partially reconstructed v1 runs.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r"
            UPDATE orchestration_sessions
            SET status = ?, current_task_id = ?, context_json = ?,
                updated_at = ?, state_revision = state_revision + 1,
                run_state_json = ?,
                graph_version = graph_version + 1
            WHERE session_id = ? AND state_revision = ?
              AND execution_version = 1
              AND status NOT IN ('completed', 'failed', 'cancelled', 'interrupted')
              AND graph_version >= 0 AND graph_version < 9223372036854775807
            ",
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
                "commit_transition '{}': {e}",
                session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // Distinguish an unsupported legacy row, a revision mismatch, and
            // a terminal/cancelled fence without mutating any of them.
            let current = sqlx::query!(
                "SELECT state_revision, execution_version \
                 FROM orchestration_sessions WHERE session_id = ?",
                session_id.0
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "commit_transition read revision/version: {e}"
                )))
            })?;

            return match current {
                Some(row) if row.execution_version != 1 => Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "commit_transition '{}': execution_version {} is non-replayable; \
                             legacy runs cannot transition",
                        session_id.0, row.execution_version
                    )),
                )),
                Some(row) if row.state_revision != expected_revision as i64 => {
                    Err(EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    })
                }
                _ => Err(EngineError::TerminalState(session_id.0.clone())),
            };
        }

        // Read the root descriptor for the authoritative v1 record.
        let root_descriptor = read_root_descriptor(&mut tx, &session_id.0).await?;

        // The update fence admits only execution_version=1. Re-read it inside
        // the transaction so a corrupt storage invariant rolls back rather
        // than returning a misleading reconstructible record.
        let persisted_version: i64 = sqlx::query_scalar!(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
            session_id.0
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "commit_transition read execution_version: {e}"
            )))
        })?;
        if persisted_version != 1 {
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
            // graph-flow 0.8: a NEW child row seeds graph_version from the
            // checked incoming checkpoint version + 1; an existing row's
            // authoritative write advances the stored graph_version in the
            // same transaction (authority comes from the workflow CAS above,
            // not the stale pre-step session version — brief §3.3). The range
            // guard makes counter exhaustion/corruption fail closed (zero
            // rows -> typed error), never wrap into a REAL.
            let child_graph_version = checked_next_graph_version(child.session.version)?;
            let child_result = sqlx::query!(
                r#"
                INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version,
                     parent_session_id, current_task_id, status,
                     context_json, chat_history_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json,
                     graph_version)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 1, ?, ?, ?, ?)
                ON CONFLICT(session_id) DO UPDATE SET
                    status = excluded.status,
                    current_task_id = excluded.current_task_id,
                    context_json = excluded.context_json,
                    updated_at = excluded.updated_at,
                    execution_version = 1,
                    state_revision = state_revision + 1,
                    run_state_json = excluded.run_state_json,
                    run_descriptor_json = excluded.run_descriptor_json,
                    graph_version = orchestration_sessions.graph_version + 1
                WHERE orchestration_sessions.state_revision = ?
                  AND orchestration_sessions.status NOT IN
                      ('completed', 'failed', 'cancelled', 'interrupted')
                  AND orchestration_sessions.graph_version >= 0
                  AND orchestration_sessions.graph_version < 9223372036854775807
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
                child_graph_version,
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
                if let Some(err) = child_cas_outcome(
                    &mut tx,
                    &child_id,
                    child.state_revision,
                    child.status.clone(),
                )
                .await?
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

    async fn settle_cancelled(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: RunCheckpoint<'_>,
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

        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "settle_cancelled begin tx: {e}"
                )))
            })?;

        // A5 settlement fence: revision CAS + execution-version fence, and the
        // current status must be `interrupted` (the cleanup-retry shape) or
        // non-terminal — a run already settled `cancelled`/`completed`/
        // `failed` is never overwritten by a retry.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r"
            UPDATE orchestration_sessions
            SET status = 'cancelled', current_task_id = ?, context_json = ?,
                updated_at = ?, state_revision = state_revision + 1,
                run_state_json = ?,
                graph_version = graph_version + 1
            WHERE session_id = ? AND state_revision = ?
              AND execution_version = 1
              AND status IN ('running', 'paused', 'waiting_for_input', 'interrupted')
              AND graph_version >= 0 AND graph_version < 9223372036854775807
            ",
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
                "settle_cancelled '{}': {e}",
                session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            let current = sqlx::query!(
                "SELECT state_revision, execution_version \
                 FROM orchestration_sessions WHERE session_id = ?",
                session_id.0
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "settle_cancelled read revision/version: {e}"
                )))
            })?;

            return match current {
                Some(row) if row.execution_version != 1 => Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "settle_cancelled '{}': execution_version {} is non-replayable; \
                             legacy runs cannot transition",
                        session_id.0, row.execution_version
                    )),
                )),
                Some(row) if row.state_revision != expected_revision as i64 => {
                    Err(EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    })
                }
                _ => Err(EngineError::TerminalState(session_id.0.clone())),
            };
        }

        let root_descriptor = read_root_descriptor(&mut tx, &session_id.0).await?;
        let persisted_version: i64 = sqlx::query_scalar!(
            "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
            session_id.0
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "settle_cancelled read execution_version: {e}"
            )))
        })?;
        if persisted_version != 1 {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "settle_cancelled '{}': unsupported execution_version {persisted_version} \
                     (non-replayable)",
                    session_id.0
                )),
            ));
        }
        let execution_version = persisted_version as u32;

        tx.commit().await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "settle_cancelled commit: {e}"
            )))
        })?;

        Ok(RunRecord {
            session_id: SessionId(session_id.0.clone()),
            status: SessionStatus::Cancelled,
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
                "load_children '{}': {e}",
                parent_session_id.0
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
            SET current_task_id = ?, context_json = ?, updated_at = ?, run_state_json = ?,
                graph_version = graph_version + 1
            WHERE session_id = ? AND state_revision = ?
              AND status IN ('running', 'paused')
              AND graph_version >= 0 AND graph_version < 9223372036854775807
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
                "restore_pre_step '{}': {e}",
                session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // Distinguish a revision mismatch from a non-running row.
            let current: Option<i64> = sqlx::query_scalar!(
                "SELECT state_revision FROM orchestration_sessions WHERE session_id = ?",
                session_id.0
            )
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

        // Fence to the statuses that may actually be stepped (`running` or
        // `paused`) and `state_revision = expected`. `waiting_for_input` and
        // unknown statuses are excluded so a stale step cannot overwrite a
        // durable wait. Advancing the revision in the same write makes the
        // in-flight marker a CAS boundary: a control signal loaded before this
        // marker cannot subsequently erase it.
        let expected_revision_i64 = expected_revision as i64;
        let result = sqlx::query!(
            r"
            UPDATE orchestration_sessions
            SET current_task_id = ?, context_json = ?, updated_at = ?, run_state_json = ?,
                state_revision = state_revision + 1,
                graph_version = graph_version + 1
            WHERE session_id = ? AND status IN ('running', 'paused')
              AND state_revision = ?
              AND graph_version >= 0 AND graph_version < 9223372036854775807
            ",
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
                "mark_step_in_flight '{}': {e}",
                session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // The pre-step row is no longer running or its revision moved —
            // the external effect must not run on an unmarked row.
            let current = sqlx::query!(
                "SELECT status, state_revision FROM orchestration_sessions WHERE session_id = ?",
                session_id.0
            )
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "mark_step_in_flight read row: {e}"
                )))
            })?;
            return Err(match current {
                Some(row) if row.state_revision != expected_revision as i64 => {
                    EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    }
                }
                Some(_) => EngineError::TerminalState(session_id.0.clone()),
                None => EngineError::SessionNotFound(session_id.0.clone()),
            });
        }
        Ok(())
    }

    /// Persist the durable `PromptAttempt` dispatch intent (A2/A5) BEFORE
    /// the external Host effect, and update it to `Active` once the Host
    /// session/operation IDs are known.
    ///
    /// Revision-linearized write: merges `in_flight` into the current
    /// `run_state_json` fenced on `state_revision = expected_revision` AND
    /// (when `expected_step` is `Some`) the current step marker
    /// `step_in_flight = expected_step`, plus step-able status
    /// (`running`/`paused`). The revision is NOT advanced so the engine's
    /// step transition CAS anchor is preserved. A stale prompt invocation
    /// whose loaded revision/step were superseded by a control transition
    /// fails the CAS and is rejected — a post-transition prompt can never
    /// replace newer state. A crash after the effect but before the result
    /// checkpoint leaves the attempt persisted and the run recovers as
    /// `Interrupted` (never auto-replayed). The engine's terminal/wait
    /// `commit_transition` clears `in_flight` on success.
    ///
    /// **Durable attempt ownership**: when `expected_attempt_id` is `Some`,
    /// the row must either have no `in_flight` attempt yet (Dispatching
    /// claims the empty slot) or already carry exactly this operation's
    /// attempt id (Active updates the SAME operation). Same-anchor
    /// concurrent prompts can never overwrite or replace the durable
    /// in-flight attempt with another operation's ids.
    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &PromptAttempt,
    ) -> Result<(), EngineError> {
        let id = session_id.0.clone();
        let attempt_json = serde_json::to_string(attempt).map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "persist_prompt_attempt '{}': serialize attempt: {e}",
                session_id.0
            )))
        })?;

        // Merge the attempt into the current run_state_json without
        // advancing the revision. The fence keeps the write off terminal
        // rows, off rows whose revision moved since the caller loaded it,
        // and (with `expected_step = Some(task)`) off rows whose step
        // marker moved — the same linearization boundary the engine's
        // control transitions anchor to. When `expected_attempt_id` names
        // this request's own admission, the row must either have NO
        // `in_flight` attempt yet (Dispatching claims an empty slot) or
        // already carry exactly this operation's attempt id (Active updates
        // the SAME operation) — a concurrent same-anchor prompt cannot
        // overwrite another operation's durable attempt.
        let expected_revision_i64 = expected_revision as i64;
        // Owner CAS clause: when this request names its own admission, the
        // row must either have NO `in_flight` attempt yet (Dispatching
        // claims an empty slot) or already carry exactly this operation's
        // attempt id (Active updates the SAME operation).
        let owner_cas_sql = match expected_attempt_id {
            Some(_) => " AND (json_extract(COALESCE(run_state_json, '{}'), '$.in_flight') IS NULL \
                   OR json_extract(COALESCE(run_state_json, '{}'), '$.in_flight.attempt_id') = ?)"
                .to_string(),
            None => String::new(),
        };
        let sql = match expected_step {
            Some(_) => format!(
                r"
                UPDATE orchestration_sessions
                SET run_state_json = json_set(
                        COALESCE(run_state_json, '{{}}'),
                        '$.in_flight',
                        json(?)
                    ),
                    updated_at = ?
                WHERE session_id = ? AND status IN ('running', 'paused')
                  AND state_revision = ?
                  AND json_extract(COALESCE(run_state_json, '{{}}'), '$.step_in_flight') = ?
                {owner_cas_sql}
                "
            ),
            None => format!(
                r"
                UPDATE orchestration_sessions
                SET run_state_json = json_set(
                        COALESCE(run_state_json, '{{}}'),
                        '$.in_flight',
                        json(?)
                    ),
                    updated_at = ?
                WHERE session_id = ? AND status IN ('running', 'paused')
                  AND state_revision = ?
                {owner_cas_sql}
                "
            ),
        };
        // SAFETY: dynamic SQL — the owner-CAS/step clauses are spliced at
        // runtime; compile-time macro not applicable.
        let mut q = sqlx::query(&sql)
            .bind(&attempt_json)
            .bind(chrono::Utc::now().timestamp())
            .bind(&id)
            .bind(expected_revision_i64);
        if let Some(step) = expected_step {
            q = q.bind(step);
        }
        if let Some(attempt_id) = expected_attempt_id {
            q = q.bind(attempt_id);
        }
        let result = q.execute(&*self.pool).await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "persist_prompt_attempt '{}': {e}",
                session_id.0
            )))
        })?;

        if result.rows_affected() == 0 {
            // Distinguish a revision/step mismatch from a non-running row
            // or an unrelated in-flight owner.
            #[derive(sqlx::FromRow)]
            struct FenceRow {
                state_revision: i64,
                step_marker: Option<String>,
                in_flight_attempt_id: Option<String>,
            }
            let current: Option<FenceRow> = sqlx::query_as!(
                FenceRow,
                r#"SELECT state_revision,
                        json_extract(COALESCE(run_state_json, '{}'), '$.step_in_flight')
                        AS "step_marker?: String",
                        json_extract(COALESCE(run_state_json, '{}'), '$.in_flight.attempt_id')
                        AS "in_flight_attempt_id?: String"
                 FROM orchestration_sessions WHERE session_id = ?"#,
                session_id.0
            )
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "persist_prompt_attempt read row: {e}"
                )))
            })?;
            return Err(match current {
                Some(row) if row.state_revision != expected_revision as i64 => {
                    EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    }
                }
                // The revision still matches but the step marker moved (or is
                // absent while `expected_step` named one): a newer step
                // superseded this prompt's admission — same linearization
                // boundary, same typed mismatch.
                Some(row)
                    if expected_step.is_some() && row.step_marker.as_deref() != expected_step =>
                {
                    EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    }
                }
                // The revision and step still match but a DIFFERENT attempt
                // owns `in_flight`: a concurrent same-anchor prompt claimed
                // the slot (or updated its own op) before this request — the
                // durable marker must never be replaced (attempt ownership).
                Some(row)
                    if expected_attempt_id.is_some()
                        && row.in_flight_attempt_id.as_deref() != expected_attempt_id =>
                {
                    EngineError::RevisionMismatch {
                        session_id: session_id.0.clone(),
                        expected: expected_revision,
                        found: u64::try_from(row.state_revision).unwrap_or(0),
                    }
                }
                Some(_) => EngineError::TerminalState(session_id.0.clone()),
                None => EngineError::SessionNotFound(session_id.0.clone()),
            });
        }
        Ok(())
    }

    /// Clear the durable `in_flight` prompt-attempt marker (see trait docs).
    /// Fenced on the admission anchor + owning attempt id; zero-row fence
    /// misses are benign no-ops (idempotent) — the engine's
    /// `commit_transition` may already have cleared the marker.
    async fn clear_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) -> Result<(), EngineError> {
        let id = session_id.0.clone();
        let expected_revision_i64 = expected_revision as i64;
        let sql = match expected_step {
            Some(_) => r"
                UPDATE orchestration_sessions
                SET run_state_json = json_set(
                        COALESCE(run_state_json, '{}'),
                        '$.in_flight',
                        json('null')
                    ),
                    updated_at = ?
                WHERE session_id = ? AND status IN ('running', 'paused')
                  AND state_revision = ?
                  AND json_extract(COALESCE(run_state_json, '{}'), '$.step_in_flight') = ?
                  AND json_extract(COALESCE(run_state_json, '{}'), '$.in_flight.attempt_id') = ?
                "
            .to_string(),
            None => r"
                UPDATE orchestration_sessions
                SET run_state_json = json_set(
                        COALESCE(run_state_json, '{}'),
                        '$.in_flight',
                        json('null')
                    ),
                    updated_at = ?
                WHERE session_id = ? AND status IN ('running', 'paused')
                  AND state_revision = ?
                  AND json_extract(COALESCE(run_state_json, '{}'), '$.in_flight.attempt_id') = ?
                "
            .to_string(),
        };
        // SAFETY: dynamic SQL — the step-marker clause is spliced at
        // runtime; compile-time macro not applicable.
        let mut q = sqlx::query(&sql)
            .bind(chrono::Utc::now().timestamp())
            .bind(&id)
            .bind(expected_revision_i64);
        if let Some(step) = expected_step {
            q = q.bind(step);
        }
        q = q.bind(attempt_id);
        q.execute(&*self.pool).await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "clear_prompt_attempt '{}': {e}",
                session_id.0
            )))
        })?;
        Ok(())
    }
}

/// Internal row mapping for SELECT queries.
#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id: String,
    current_task_id: Option<String>,
    context_json: Vec<u8>,
    graph_version: i64,
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

        // Update with a different task id (reload OCC version after first write).
        session.current_task_id = "task-b".to_string();
        session.version = storage
            .get("sess-upsert")
            .await
            .unwrap()
            .unwrap()
            .version;
        storage.save(session).await.unwrap();

        let loaded = storage.get("sess-upsert").await.unwrap().unwrap();
        assert_eq!(loaded.current_task_id, "task-b");
    }

    #[tokio::test]
    async fn save_persists_task_context_on_paused_row() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // A paused row is the A2 scheduler-park shape that the reopen
        // resume re-drive re-steps. The graph-flow post-step `save` seam
        // must persist the task's in-memory context mutations (the
        // `_gate_park_*` marker) for a paused row too — otherwise the
        // engine's post-step re-fetch reads a marker-less stale context and
        // misclassifies the re-park as a human wait.
        let session = Session::new_from_task("sess-paused-save".into(), "join");
        seed_row(
            &pool,
            "sess-paused-save",
            "paused",
            Some("join"),
            &serde_json::to_vec(&session.context).expect("serialize seed context"),
        )
        .await;
        storage
            .save(session.clone())
            .await
            .expect("insert paused row");
        let mut session = storage
            .get("sess-paused-save")
            .await
            .expect("load paused row")
            .expect("row exists");
        session.context.set("_gate_park_join", true);
        session.current_task_id = "join".to_string();
        storage
            .save(session)
            .await
            .expect("save mutated paused row");

        let persisted = storage
            .get("sess-paused-save")
            .await
            .expect("reload after save")
            .expect("row exists");
        assert_eq!(
            persisted.current_task_id, "join",
            "position update must persist on a paused row"
        );
        assert_eq!(
            persisted.context.get::<bool>("_gate_park_join"),
            Some(true),
            "task context mutations must persist on a paused row (the \
             reopen-resume post-step save seam)"
        );
    }

    #[tokio::test]
    async fn save_fence_still_excludes_waiting_for_input() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // A4: a durable human-wait row (waiting_for_input + token) must
        // never be overwritten by the graph-flow post-step save seam — a
        // stale step invocation cannot clear or replace the WaitRecord.
        let session = Session::new_from_task("sess-wait-fence".into(), "wait_state");
        seed_row(
            &pool,
            "sess-wait-fence",
            "waiting_for_input",
            Some("wait_state"),
            &serde_json::to_vec(&session.context).expect("serialize seed context"),
        )
        .await;
        // The seeded row IS the durable wait row; any graph save against it
        // must conflict (0.8 honest refusal, no fake-success no-op).
        let mut session = storage
            .get("sess-wait-fence")
            .await
            .expect("load wait row")
            .expect("row exists");
        session.context.set("_gate_park_wait_state", true);
        session.current_task_id = "other_state".to_string();
        let err = storage
            .save(session)
            .await
            .expect_err("protected wait row must reject graph save");
        assert!(
            matches!(err, graph_flow::GraphError::SessionConflict(_)),
            "expected SessionConflict, got {err:?}"
        );

        let persisted = storage
            .get("sess-wait-fence")
            .await
            .expect("reload after save")
            .expect("row exists");
        assert_eq!(
            persisted.current_task_id, "wait_state",
            "the save seam must not move a waiting_for_input row (A4)"
        );
        assert!(
            persisted
                .context.get::<bool>("_gate_park_wait_state").is_none(),
            "the save seam must not write context onto a waiting_for_input row"
        );
    }

    #[tokio::test]
    async fn save_reports_unknown_status_zero_row_as_error() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // A corrupt/unknown status is non-replayable (A7): a zero-row save
        // (WHERE fence skipped) against it must be an explicit error, never
        // a silent `Ok(())` that masks the broken row.
        let session = Session::new_from_task("sess-unknown-status".into(), "task_a");
        seed_row(
            &pool,
            "sess-unknown-status",
            "corrupted_status",
            Some("task_a"),
            &serde_json::to_vec(&session.context).expect("serialize seed context"),
        )
        .await;
        let err = storage
            .save(session)
            .await
            .expect_err("unknown status zero-row must not silently succeed");
        match &err {
            graph_flow::GraphError::StorageError(msg) => {
                assert!(
                    msg.contains("unknown/corrupt status"),
                    "unexpected error: {msg}"
                );
                assert!(
                    msg.contains("sess-unknown-status"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected StorageError, got {other:?}"),
        }

        // The row must be untouched (the save never applied).
        let persisted = storage
            .get("sess-unknown-status")
            .await
            .expect("reload after rejected save")
            .expect("row exists");
        assert_eq!(persisted.current_task_id, "task_a", "row must be unchanged");
    }

    #[tokio::test]
    async fn save_reports_unexpected_zero_row_race_on_running() {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // A zero-row save against a still-running row means the write was
        // skipped by something other than the status fence (the BEGIN
        // IMMEDIATE write lock makes the check-then-write race unreachable
        // through this path). A regression trigger reproduces the skip; the
        // save must surface a revision/status conflict, never success.
        let session = Session::new_from_task("sess-zero-row-race".into(), "task_a");
        seed_row(
            &pool,
            "sess-zero-row-race",
            "running",
            Some("task_a"),
            &serde_json::to_vec(&session.context).expect("serialize seed context"),
        )
        .await;
        sqlx::query(
            "CREATE TRIGGER sess_zero_row_race_skip
             BEFORE UPDATE ON orchestration_sessions
             WHEN NEW.session_id = 'sess-zero-row-race'
             BEGIN SELECT RAISE(IGNORE); END",
        )
        .execute(&*pool)
        .await
        .expect("install regression trigger");

        let err = storage
            .save(session)
            .await
            .expect_err("unexpected zero-row on a running row must not succeed");
        match &err {
            graph_flow::GraphError::SessionConflict(msg) => {
                assert!(
                    msg.contains("sess-zero-row-race"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected SessionConflict, got {other:?}"),
        }

        // The row must be untouched.
        let persisted = storage
            .get("sess-zero-row-race")
            .await
            .expect("reload after rejected save")
            .expect("row exists");
        assert_eq!(persisted.current_task_id, "task_a", "row must be unchanged");

        sqlx::query("DROP TRIGGER sess_zero_row_race_skip")
            .execute(&*pool)
            .await
            .expect("drop regression trigger");
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

    /// Seed a v1 run row with a frozen descriptor and a step marker
    /// (`step_in_flight = "task_a"`), at revision 2 (1 after `start_run` +
    /// 1 after the engine's `mark_step_in_flight`).
    async fn seed_v1_stepped_row(pool: &sqlx::SqlitePool, session_id: &str) {
        let descriptor = RunDescriptorV1 {
            creator_id: "ctr_t".to_string(),
            work_id: None,
            workspace_root: std::path::PathBuf::from("/ws"),
            preset_id: "preset_t".to_string(),
            preset_version: 7,
            source: crate::run_state::PresetSourceIdentity::Embedded {
                preset_id: "preset_t".to_string(),
                content_hash: [7u8; 32],
            },
            input: serde_json::Map::new(),
            agent_bindings: std::collections::HashMap::from([(
                "default".to_string(),
                crate::run_state::AgentBinding {
                    provider_id: "mock-acp".to_string(),
                    model: None,
                },
            )]),
            parent_session_id: None,
            graph_name: None,
        };
        let state = serde_json::json!({
            "wait": null,
            "step_in_flight": "task_a",
            "in_flight": null,
            "failure": null,
            "cancel_requested": false,
        });
        let state_bytes = serde_json::to_vec(&state).expect("serialize seed state");
        let descriptor_bytes = serde_json::to_vec(&descriptor).expect("serialize seed descriptor");
        sqlx::query!(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
             VALUES (?, 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                     '{}', 1756990000, 1756990300, 1, 2, ?, ?)",
            session_id,
            state_bytes,
            descriptor_bytes
        )
        .execute(pool)
        .await
        .expect("seed v1 stepped row");
    }

    fn test_attempt(task_id: &str, phase: crate::run_state::PromptPhase) -> PromptAttempt {
        PromptAttempt {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            phase,
            host_session_id: None,
            operation_id: None,
            process_identity: None,
        }
    }

    #[tokio::test]
    async fn persist_prompt_attempt_cas_writes_and_preserves_revision() {
        let (pool, _db) = fresh_pool().await;
        seed_v1_stepped_row(&pool, "sess-cas-ok").await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // Matching fence: revision 2 + step marker "task_a" → the intent is
        // written and the revision is NOT advanced (the engine's step
        // transition CAS anchor is preserved). The ownership CAS accepts
        // the claim onto an empty `in_flight` slot.
        let first = test_attempt("t1", crate::run_state::PromptPhase::Dispatching);
        storage
            .persist_prompt_attempt(
                &SessionId("sess-cas-ok".into()),
                2,
                Some("task_a"),
                Some(&first.attempt_id),
                &first,
            )
            .await
            .expect("matching fence must write");
        let record = storage
            .load_run(&SessionId("sess-cas-ok".into()))
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.state_revision, 2,
            "intent write must not advance the revision"
        );
        let state = record.state.expect("v1 state");
        assert_eq!(state.in_flight.as_ref().expect("in_flight").task_id, "t1");

        // The same operation may update its own Active attempt (same
        // attempt id — the ownership CAS matches the occupant).
        let active = test_attempt("t1", crate::run_state::PromptPhase::Active);
        let mut active = active;
        active.attempt_id = first.attempt_id.clone();
        storage
            .persist_prompt_attempt(
                &SessionId("sess-cas-ok".into()),
                2,
                Some("task_a"),
                Some(&first.attempt_id),
                &active,
            )
            .await
            .expect("same-owner Active update must write");
        let record = storage
            .load_run(&SessionId("sess-cas-ok".into()))
            .await
            .expect("load")
            .expect("row");
        let state = record.state.expect("v1 state");
        assert_eq!(
            state.in_flight.as_ref().expect("in_flight").attempt_id,
            first.attempt_id,
            "Active update must target the SAME durable attempt"
        );
        assert_eq!(
            state.in_flight.as_ref().expect("in_flight").phase,
            crate::run_state::PromptPhase::Active
        );
    }

    #[tokio::test]
    async fn concurrent_same_anchor_prompt_cannot_replace_in_flight_owner() {
        let (pool, _db) = fresh_pool().await;
        seed_v1_stepped_row(&pool, "sess-owner").await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // Request A claims the empty slot with its own attempt id.
        let first = test_attempt("t1", crate::run_state::PromptPhase::Dispatching);
        storage
            .persist_prompt_attempt(
                &SessionId("sess-owner".into()),
                2,
                Some("task_a"),
                Some(&first.attempt_id),
                &first,
            )
            .await
            .expect("request A claims the empty slot");

        // Request B — same revision, same step marker, same unchanged anchor —
        // tries to claim the slot with a DIFFERENT attempt id: the durable
        // `in_flight` owner must never be replaced.
        let second = test_attempt("t2", crate::run_state::PromptPhase::Dispatching);
        let err = storage
            .persist_prompt_attempt(
                &SessionId("sess-owner".into()),
                2,
                Some("task_a"),
                Some(&second.attempt_id),
                &second,
            )
            .await
            .expect_err("a different attempt id must not overwrite the in_flight owner");
        assert!(
            matches!(err, EngineError::RevisionMismatch { .. }),
            "expected RevisionMismatch for foreign-owner overwrite, got: {err:?}"
        );

        // The durable marker still names request A's operation — request B
        // could not replace nor displace it.
        let record = storage
            .load_run(&SessionId("sess-owner".into()))
            .await
            .expect("load")
            .expect("row");
        let state = record.state.expect("v1 state");
        let in_flight = state.in_flight.expect("in_flight must remain");
        assert_eq!(
            in_flight.attempt_id, first.attempt_id,
            "the durable in_flight marker must remain request A's operation"
        );

        // Request B's own Active update must also be refused: it does not
        // own the slot, so it can never write its Host ids into the marker.
        let second_active = test_attempt("t2", crate::run_state::PromptPhase::Active);
        let err = storage
            .persist_prompt_attempt(
                &SessionId("sess-owner".into()),
                2,
                Some("task_a"),
                Some(&second_active.attempt_id),
                &second_active,
            )
            .await
            .expect_err("foreign Active update must fail the ownership CAS");
        assert!(
            matches!(err, EngineError::RevisionMismatch { .. }),
            "expected RevisionMismatch for foreign Active update, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn stale_prompt_attempt_after_revision_transition_is_rejected() {
        let (pool, _db) = fresh_pool().await;
        seed_v1_stepped_row(&pool, "sess-stale").await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // A control transition (e.g. Cancel) advances the revision while the
        // prompt invocation holds the old anchor.
        sqlx::query!(
            "UPDATE orchestration_sessions
             SET state_revision = state_revision + 1, run_state_json = json_set(
                     COALESCE(run_state_json, '{}'), '$.cancel_requested', json('true'))
             WHERE session_id = 'sess-stale'"
        )
        .execute(&*pool)
        .await
        .expect("advance revision");

        // The stale prompt attempt must be rejected — the post-transition
        // state (revision 3, cancel_requested) is never overwritten.
        let stale = test_attempt("stale-t1", crate::run_state::PromptPhase::Dispatching);
        let err = storage
            .persist_prompt_attempt(
                &SessionId("sess-stale".into()),
                2, // stale anchor
                Some("task_a"),
                Some(&stale.attempt_id),
                &stale,
            )
            .await
            .expect_err("stale revision must fail the CAS");
        match err {
            EngineError::RevisionMismatch {
                session_id,
                expected,
                found,
            } => {
                assert_eq!(session_id, "sess-stale");
                assert_eq!(expected, 2);
                assert_eq!(found, 3);
            }
            other => panic!("expected RevisionMismatch, got: {other:?}"),
        }

        let record = storage
            .load_run(&SessionId("sess-stale".into()))
            .await
            .expect("load")
            .expect("row");
        assert_eq!(record.state_revision, 3, "newer state must be untouched");
        let run_state = record.state.as_ref().expect("state");
        assert!(
            run_state.cancel_requested,
            "post-transition cancel_requested must survive the stale prompt"
        );
        assert!(
            run_state.in_flight.is_none(),
            "stale prompt must never install an in_flight attempt"
        );
    }

    #[tokio::test]
    async fn stale_prompt_attempt_with_moved_step_marker_is_rejected() {
        let (pool, _db) = fresh_pool().await;
        seed_v1_stepped_row(&pool, "sess-step-moved").await;
        let storage = SqliteSessionStorage::new(pool.clone());

        // The step moved on (a newer task is now in flight) while the prompt
        // invocation still holds the old task marker.
        sqlx::query!(
            "UPDATE orchestration_sessions
             SET run_state_json = json_set(
                     COALESCE(run_state_json, '{}'), '$.step_in_flight', json('\"task_b\"'))"
        )
        .execute(&*pool)
        .await
        .expect("move step marker");

        let stale = test_attempt("stale-t2", crate::run_state::PromptPhase::Dispatching);
        let err = storage
            .persist_prompt_attempt(
                &SessionId("sess-step-moved".into()),
                2, // revision still matches
                Some("task_a"),
                Some(&stale.attempt_id),
                &stale,
            )
            .await
            .expect_err("moved step marker must fail the CAS");
        assert!(
            matches!(err, EngineError::RevisionMismatch { .. }),
            "expected RevisionMismatch for moved marker, got: {err:?}"
        );

        let record = storage
            .load_run(&SessionId("sess-step-moved".into()))
            .await
            .expect("load")
            .expect("row");
        assert!(
            record.state.expect("state").in_flight.is_none(),
            "stale step prompt must never install an in_flight attempt"
        );
    }
}
