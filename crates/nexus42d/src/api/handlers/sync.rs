//! Sync handler — sync status endpoint

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub struct SyncStatusResponse {
    /// Number of pending bundles in outbox
    pub pending_count: u64,
    /// Number of failed outbox entries
    pub failed_count: u64,
    /// Timestamp of the last successful sync (RFC 3339)
    pub last_sync_at: Option<String>,
    /// Number of unresolved conflicts
    pub conflict_count: u64,
}

/// GET /v1/local/sync/status
pub async fn status(
    State(state): State<WorkspaceState>,
) -> Result<Json<SyncStatusResponse>, NexusApiError> {
    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_UNAVAILABLE".into(),
        message: format!("Database connection error: {}", e),
    })?;

    // Count pending outbox entries
    let pending_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?
        .unwrap_or(0);

    // Count failed outbox entries
    let failed_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?
        .unwrap_or(0);

    // Get last successful sync timestamp from workspace_meta
    let last_sync_at: Option<String> = conn
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'last_sync_at'",
            [],
            |row| row.get(0),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    // Count unresolved conflicts (currently no conflict table; always 0)
    let conflict_count: u64 = 0;

    Ok(Json(SyncStatusResponse {
        pending_count,
        failed_count,
        last_sync_at,
        conflict_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_response_serialization() {
        let resp = SyncStatusResponse {
            pending_count: 3,
            failed_count: 1,
            last_sync_at: Some("2026-04-07T00:00:00Z".to_string()),
            conflict_count: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"pending_count\":3"));
        assert!(json.contains("\"failed_count\":1"));
        assert!(json.contains("\"conflict_count\":0"));
        assert!(json.contains("last_sync_at"));
    }

    #[test]
    fn test_sync_status_response_no_last_sync() {
        let resp = SyncStatusResponse {
            pending_count: 0,
            failed_count: 0,
            last_sync_at: None,
            conflict_count: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"last_sync_at\":null"));
    }

    #[tokio::test]
    async fn test_sync_status_empty_outbox() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.unwrap();
        assert_eq!(body.pending_count, 0);
        assert_eq!(body.failed_count, 0);
        assert!(body.last_sync_at.is_none());
        assert_eq!(body.conflict_count, 0);
    }

    #[tokio::test]
    async fn test_sync_status_with_pending_bundles() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace();

        // Insert pending outbox entries
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO outbox (command_type, payload, status, created_at) VALUES ('sync', '{}', 'pending', '2026-04-07T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO outbox (command_type, payload, status, created_at) VALUES ('sync', '{}', 'pending', '2026-04-07T00:00:01Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO outbox (command_type, payload, status, created_at, sent_at, error) VALUES ('sync', '{}', 'failed', '2026-04-07T00:00:02Z', '2026-04-07T00:00:03Z', 'timeout')",
            [],
        ).unwrap();

        // Insert last sync timestamp
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('last_sync_at', '2026-04-06T12:00:00Z')",
            [],
        ).unwrap();

        drop(conn);

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.unwrap();
        assert_eq!(body.pending_count, 2);
        assert_eq!(body.failed_count, 1);
        assert_eq!(body.last_sync_at, Some("2026-04-06T12:00:00Z".to_string()));
        assert_eq!(body.conflict_count, 0);
    }
}
