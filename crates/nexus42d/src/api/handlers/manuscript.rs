//! Manuscript handler

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use tracing::{debug, info};

#[derive(Serialize)]
pub struct ManuscriptStatusResponse {
    pub phase: Option<String>,
    pub active_manifest_id: Option<String>,
}

/// GET /v1/local/manuscript
pub async fn status(
    State(state): State<WorkspaceState>,
) -> Result<Json<ManuscriptStatusResponse>, NexusApiError> {
    info!("Handling manuscript status request");

    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_UNAVAILABLE".into(),
        message: format!("Database connection error: {}", e),
    })?;

    let phase: Option<String> = conn
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
            [],
            |row| row.get(0),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    let active_manifest_id: Option<String> = conn
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'active_manifest_id'",
            [],
            |row| row.get(0),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    debug!(phase = ?phase, active_manifest_id = ?active_manifest_id, "Manuscript status retrieved");
    info!("Manuscript status completed");
    Ok(Json(ManuscriptStatusResponse {
        phase,
        active_manifest_id,
    }))
}
