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
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §4.3.

use async_trait::async_trait;
use graph_flow::{Session, SessionStorage};
use std::sync::Arc;

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
    pub fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
        Self { pool }
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
            graph_flow::GraphError::StorageError(format!("save session '{}': {e}", session_id))
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
        .map_err(|e| {
            graph_flow::GraphError::StorageError(format!("delete session '{id}': {e}"))
        })?;

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

    /// Helper: open a fresh on-disk temp SQLite pool with migrations applied.
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
}
