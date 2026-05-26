//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Reference source handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use tracing::{debug, info};

/// Registry metadata for a reference source (API response DTO).
#[derive(Debug, Serialize)]
pub struct ReferenceInfo {
    pub reference_source_id: String,
    pub source_type: String,
    pub source_mutability: String,
    pub uri: String,
    pub title: String,
    pub content_path: Option<String>,
    pub scan_status: String,
    pub created_at: String,
}

impl From<nexus_local_db::ReferenceSourceRow> for ReferenceInfo {
    fn from(row: nexus_local_db::ReferenceSourceRow) -> Self {
        Self {
            reference_source_id: row.reference_source_id,
            source_type: row.source_type,
            source_mutability: row.source_mutability,
            uri: row.uri,
            title: row.title,
            content_path: row.content_path,
            scan_status: row.scan_status,
            created_at: row.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct ListReferencesResponse {
    pub references: Vec<ReferenceInfo>,
}

#[derive(Serialize)]
pub struct GetReferenceResponse {
    pub reference: ReferenceInfo,
}

/// GET /v1/local/references
pub async fn list(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListReferencesResponse>, NexusApiError> {
    info!("Handling list references request");

    let rows = nexus_local_db::list_references(state.pool(), None, None)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    let references: Vec<ReferenceInfo> = rows.into_iter().map(ReferenceInfo::from).collect();
    debug!(count = references.len(), "References retrieved");
    info!("List references completed");
    Ok(Json(ListReferencesResponse { references }))
}

/// `GET /v1/local/references/{reference_id}`
pub async fn get(
    State(state): State<WorkspaceState>,
    Path(reference_id): Path<String>,
) -> Result<Json<GetReferenceResponse>, NexusApiError> {
    info!(%reference_id, "Handling get reference request");

    let row = nexus_local_db::get_reference_by_id(state.pool(), &reference_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    let row =
        row.ok_or_else(|| NexusApiError::NotFound(format!("reference_source: {reference_id}")))?;

    Ok(Json(GetReferenceResponse {
        reference: ReferenceInfo::from(row),
    }))
}
