//! Reference source handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use tracing::{debug, info};

#[derive(Debug, Serialize, sqlx::FromRow)]
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
    info!("Handling list references request");

    let references = sqlx::query_as!(
        ReferenceInfo,
        r#"SELECT reference_source_id as "reference_source_id!", source_type, title, scan_status, created_at
         FROM reference_sources ORDER BY created_at DESC"#,
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    debug!(count = references.len(), "References retrieved");
    info!("List references completed");
    Ok(Json(ListReferencesResponse { references }))
}
