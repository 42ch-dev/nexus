//! Creator handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct CreatorInfo {
    pub creator_id: String,
    pub display_name: String,
    pub status: String,
    pub cached_at: Option<String>,
}

#[derive(Serialize)]
pub struct ListCreatorsResponse {
    pub creators: Vec<CreatorInfo>,
}

/// GET /v1/local/creators
pub async fn list(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListCreatorsResponse>, NexusApiError> {
    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_UNAVAILABLE".into(),
        message: format!("Database connection error: {}", e),
    })?;

    let creators = conn
        .query_map(
            "SELECT creator_id, display_name, status, cached_at FROM creators ORDER BY cached_at DESC",
            [],
            |row| {
                Ok(CreatorInfo {
                    creator_id: row.get(0)?,
                    display_name: row.get(1)?,
                    status: row.get(2)?,
                    cached_at: row.get(3)?,
                })
            },
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    Ok(Json(ListCreatorsResponse { creators }))
}
