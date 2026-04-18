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

    let phase_row: Option<String> =
        sqlx::query_scalar!("SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'")
            .fetch_optional(state.pool())
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: e.to_string(),
            })?;

    let manifest_row: Option<String> =
        sqlx::query_scalar!("SELECT value FROM workspace_meta WHERE key = 'active_manifest_id'")
            .fetch_optional(state.pool())
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: e.to_string(),
            })?;

    debug!(phase = ?phase_row, active_manifest_id = ?manifest_row, "Manuscript status retrieved");
    info!("Manuscript status completed");
    Ok(Json(ManuscriptStatusResponse {
        phase: phase_row,
        active_manifest_id: manifest_row,
    }))
}
