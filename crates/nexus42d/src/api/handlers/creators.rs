//! Creator handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use tracing::{debug, info};

#[derive(Debug, Serialize, sqlx::FromRow)]
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
    info!("Handling list creators request");

    let creators = sqlx::query_as!(
        CreatorInfo,
        r#"SELECT creator_id as "creator_id!", display_name, status, cached_at FROM creators ORDER BY cached_at DESC"#
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    debug!(count = creators.len(), "Creators retrieved");
    info!("List creators completed");
    Ok(Json(ListCreatorsResponse { creators }))
}
