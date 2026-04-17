//! ACP session management endpoints.
//!
//! Provides daemon-side API endpoints for managing ACP sessions:
//! - `GET /v1/local/acp/sessions` — List active ACP sessions
//! - `DELETE /v1/local/acp/sessions/{id}` — Delete an ACP session
//!
//! Sessions are stored in SQLite via the `acp_sessions` table.
//! This enables the CLI to discover and manage sessions across invocations.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

/// GET /v1/local/acp/sessions
///
/// List all active ACP sessions stored in the daemon database.
#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct SessionInfo {
    pub session_id: String,
    pub agent_id: String,
    pub created_at: String,
    pub last_active: String,
    pub workspace_hint: String,
}

pub async fn list_sessions(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListSessionsResponse>, NexusApiError> {
    tracing::info!("Handling list ACP sessions request");

    let sessions = sqlx::query_as::<_, SessionInfo>(
        "SELECT session_id, agent_id, created_at, last_active, workspace_hint
         FROM acp_sessions
         ORDER BY last_active DESC",
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "SESSION_LIST_FAILED".into(),
        message: format!("failed to list sessions: {}", e),
    })?;

    let total = sessions.len();

    Ok(Json(ListSessionsResponse { sessions, total }))
}

/// DELETE /v1/local/acp/sessions/{id}
///
/// Delete a specific ACP session by its ID.
#[derive(Debug, Serialize)]
pub struct DeleteSessionResponse {
    pub deleted: bool,
    pub session_id: String,
}

pub async fn delete_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<DeleteSessionResponse>, NexusApiError> {
    tracing::info!(
        session_id = %session_id,
        "Handling delete ACP session request"
    );

    let result = sqlx::query("DELETE FROM acp_sessions WHERE session_id = ?1")
        .bind(&session_id)
        .execute(state.pool())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "SESSION_DELETE_FAILED".into(),
            message: format!("failed to delete session: {}", e),
        })?;

    Ok(Json(DeleteSessionResponse {
        deleted: result.rows_affected() > 0,
        session_id,
    }))
}

/// Cleanup expired sessions (older than 24 hours).
///
/// This can be called periodically by the daemon or on-demand.
pub async fn cleanup_expired_sessions(state: &WorkspaceState) -> Result<u64, NexusApiError> {
    let result = sqlx::query(
        "DELETE FROM acp_sessions
         WHERE datetime(last_active) < datetime('now', '-24 hours')",
    )
    .execute(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "SESSION_CLEANUP_FAILED".into(),
        message: format!("failed to cleanup expired sessions: {}", e),
    })?;

    Ok(result.rows_affected())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use crate::workspace::WorkspaceState;
    use axum::extract::State;

    async fn setup_sessions_db(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
             VALUES ('sess-001', 'claude-acp', '2026-04-08T10:00:00Z', '2026-04-08T15:00:00Z', '/tmp/workspace');
             INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
             VALUES ('sess-002', 'codex-acp', '2026-04-08T12:00:00Z', '2026-04-08T16:00:00Z', '/tmp/other');"
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_sessions_returns_all_sessions() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path.clone(), None).await;

        setup_sessions_db(state.pool()).await;

        let result = list_sessions(State(state)).await.unwrap();
        assert_eq!(result.sessions.len(), 2);
        assert_eq!(result.total, 2);
    }

    #[tokio::test]
    async fn list_sessions_empty_returns_empty_list() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = list_sessions(State(state)).await.unwrap();
        assert_eq!(result.sessions.len(), 0);
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn delete_session_removes_session() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path.clone(), None).await;

        setup_sessions_db(state.pool()).await;

        // Delete one session
        let result = delete_session(State(state.clone()), Path("sess-001".to_string()))
            .await
            .unwrap();
        assert!(result.deleted);
        assert_eq!(result.session_id, "sess-001");

        // Verify only one remains
        let list = list_sessions(State(state)).await.unwrap();
        assert_eq!(list.sessions.len(), 1);
        assert_eq!(list.sessions[0].session_id, "sess-002");
    }

    #[tokio::test]
    async fn delete_nonexistent_session_returns_not_deleted() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path.clone(), None).await;

        setup_sessions_db(state.pool()).await;

        let result = delete_session(State(state), Path("nonexistent".to_string()))
            .await
            .unwrap();

        assert!(!result.deleted);
    }

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;

        // Insert a session that's clearly expired
        sqlx::query(
            "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
             VALUES ('sess-old', 'claude-acp', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', '/tmp/old')"
        )
        .execute(&nexus_local_db::open_pool(&db_path).await.unwrap())
        .await
        .unwrap();

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let removed = cleanup_expired_sessions(&state).await.unwrap();
        assert!(removed > 0);
    }
}
