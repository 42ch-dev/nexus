//! Thin reference-source HTTP adapters over the guarded core references
//! service (P1-T3).
//!
//! Registry reads live in the core; the cloud-backed refresh remains the
//! CLI's explicit daemon host-call (`nexus.reference.refresh`) and is not
//! owned here.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

/// Registry metadata for a reference source (API response DTO; wire copy of
/// the core `ReferenceInfo` projection).
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

impl From<nexus_core::ReferenceInfo> for ReferenceInfo {
    fn from(row: nexus_core::ReferenceInfo) -> Self {
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

/// Map a core references fault onto the legacy HTTP classification.
///
/// `NotFound` resources round-trip verbatim; the core `local_db_err` lowercase
/// `database_error: …` carrier is re-classified as the legacy
/// `DATABASE_ERROR`; every other internal category keeps the shared
/// `CORE_ERROR` shape.
fn references_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::NotFound { resource } => NexusApiError::NotFound(resource),
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some(("database_error", message)) => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_owned(),
                message: message.to_owned(),
            },
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// GET /v1/daemon/references
pub async fn list(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListReferencesResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core
        .list_references(&principal)
        .await
        .map_err(references_error)?;
    Ok(Json(ListReferencesResponse {
        references: result
            .references
            .into_iter()
            .map(ReferenceInfo::from)
            .collect(),
    }))
}

/// `GET /v1/daemon/references/{reference_id}`
pub async fn get(
    State(state): State<WorkspaceState>,
    Path(reference_id): Path<String>,
) -> Result<Json<GetReferenceResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core
        .get_reference(&principal, reference_id)
        .await
        .map_err(references_error)?;
    Ok(Json(GetReferenceResponse {
        reference: ReferenceInfo::from(result.reference),
    }))
}
