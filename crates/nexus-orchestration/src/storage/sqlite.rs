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

use crate::engine::{SessionId, SessionStatus, SessionSummary};

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
    pub async fn list_checkpoint_rows(&self) -> Result<Vec<CheckpointRow>, sqlx::Error> {
        sqlx::query_as::<_, CheckpointRow>(
            "SELECT session_id, creator_id, preset_id, preset_version, current_task_id,
                    status, context_json, created_at, updated_at
             FROM orchestration_sessions
             WHERE status IN ('running', 'paused', 'waiting_for_input')
             ORDER BY session_id",
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Read-only by-id checkpoint lookup for `nexus42 ops inspect <id>`
    /// (V1.182 P1 BL-04). No status filter — detail mode reports `db_status`
    /// honestly (the column is not authoritative: every save writes
    /// `'running'` and ON CONFLICT never updates it).
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
}

/// Raw checkpoint row for the daemon-free `nexus42 ops inspect` surface
/// (V1.182 P1 BL-04).
///
/// Read-only projection of `orchestration_sessions`: `status` is the raw DB
/// column (NOT authoritative — see module docs), `context_json` is the raw
/// blob the CLI parses itself (`{"data": {...}}` shape) to project the
/// resume rules without constructing a `graph_flow::Context`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CheckpointRow {
    /// Run id (PRIMARY KEY).
    pub session_id: String,
    /// Owning creator id.
    pub creator_id: String,
    /// Strategy / preset id.
    pub preset_id: String,
    /// Strategy / preset version.
    pub preset_version: i64,
    /// Persisted position (`None` = no position recorded yet).
    pub current_task_id: Option<String>,
    /// Raw DB status column — NOT authoritative (every save writes
    /// `'running'`; ON CONFLICT never updates it).
    pub status: String,
    /// Raw serialized session context blob.
    pub context_json: Vec<u8>,
    /// First-save timestamp (unix epoch seconds).
    pub created_at: i64,
    /// Last-save timestamp (unix epoch seconds).
    pub updated_at: i64,
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
    async fn list_checkpoint_rows_filters_non_terminal_and_carries_timestamps() {
        let (pool, _db) = fresh_pool().await;
        seed_row(&pool, "sess-run", "running", Some("task_1"), b"{}").await;
        seed_row(&pool, "sess-pause", "paused", None, b"{}").await;
        seed_row(&pool, "sess-wait", "waiting_for_input", None, b"{}").await;
        seed_row(&pool, "sess-done", "completed", Some("task_9"), b"{}").await;
        seed_row(&pool, "sess-fail", "failed", None, b"{}").await;

        let storage = SqliteSessionStorage::new(pool);
        let rows = storage.list_checkpoint_rows().await.expect("list rows");
        let ids: Vec<&str> = rows.iter().map(|r| r.session_id.as_str()).collect();
        assert_eq!(ids, ["sess-pause", "sess-run", "sess-wait"]);

        let row = &rows[1];
        assert_eq!(row.creator_id, "ctr_t");
        assert_eq!(row.preset_id, "preset_t");
        assert_eq!(row.preset_version, 7);
        assert_eq!(row.current_task_id.as_deref(), Some("task_1"));
        assert_eq!(row.status, "running");
        assert_eq!(row.context_json, b"{}");
        assert_eq!(row.created_at, 1_756_990_000);
        assert_eq!(row.updated_at, 1_756_990_300);
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
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = 'sess-run'",
        )
        .fetch_one(&*pool)
        .await
        .expect("count");
        assert_eq!(count, 1, "read-only accessors must not write");
    }
}
