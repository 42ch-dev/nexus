//! Reference source handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct ReferenceInfo {
    pub reference_source_id: String,
    pub source_type: String,
    pub title: String,
    pub scan_status: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ListReferencesResponse {
    pub references: Vec<ReferenceInfo>,
}

/// GET /v1/local/references
pub async fn list(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListReferencesResponse>, NexusApiError> {
    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_UNAVAILABLE".into(),
        message: format!("Database connection error: {}", e),
    })?;

    let references = conn
        .query_map(
            "SELECT reference_source_id, source_type, title, scan_status, created_at
             FROM reference_sources ORDER BY created_at DESC",
            [],
            |row| {
                Ok(ReferenceInfo {
                    reference_source_id: row.get(0)?,
                    source_type: row.get(1)?,
                    title: row.get(2)?,
                    scan_status: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    Ok(Json(ListReferencesResponse { references }))
}
