//! Reference source registry reads over the guarded core workspace.
//! Extracted from the legacy daemon `references.rs` handlers (P1-T3); the
//! cloud-backed refresh remains the CLI's explicit daemon host-call
//! (`nexus.reference.refresh`) and is deliberately not owned here.

use nexus_local_db::reference_source::ReferenceSourceRow;
use serde::Serialize;

use crate::error::local_db_err;
use crate::{CoreResult, CoreService, Principal};

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

impl From<ReferenceSourceRow> for ReferenceInfo {
    fn from(row: ReferenceSourceRow) -> Self {
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

/// Response envelope for the reference list.
#[derive(Debug, Serialize)]
pub struct ListReferencesResponse {
    pub references: Vec<ReferenceInfo>,
}

/// Response envelope for a single reference lookup.
#[derive(Debug, Serialize)]
pub struct GetReferenceResponse {
    pub reference: ReferenceInfo,
}

impl CoreService {
    /// List registered reference sources.
    ///
    /// The registry read is global across creators, matching the local-first
    /// single-creator model the legacy surface documented (one active creator
    /// per workspace); principal verification keeps auth parity.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the storage carrier (legacy `DATABASE_ERROR`) on
    /// database failure.
    pub async fn list_references(
        &self,
        principal: &Principal,
    ) -> CoreResult<ListReferencesResponse> {
        self.verify_principal(principal)?;
        let rows = nexus_local_db::list_references(&self.inner.pool, None, None, None)
            .await
            .map_err(local_db_err)?;
        let references = rows.into_iter().map(ReferenceInfo::from).collect();
        self.verify_principal(principal)?;
        Ok(ListReferencesResponse { references })
    }

    /// Fetch one reference source by ID.
    ///
    /// # Errors
    /// As [`CoreService::list_references`]; additionally
    /// [`CoreError::NotFound`] with the legacy
    /// `reference_source: <id>` resource string.
    pub async fn get_reference(
        &self,
        principal: &Principal,
        reference_id: String,
    ) -> CoreResult<GetReferenceResponse> {
        self.verify_principal(principal)?;
        let row = nexus_local_db::get_reference_by_id(&self.inner.pool, &reference_id)
            .await
            .map_err(local_db_err)?
            .ok_or_else(|| crate::CoreError::NotFound {
                resource: format!("reference_source: {reference_id}"),
            })?;
        self.verify_principal(principal)?;
        Ok(GetReferenceResponse { reference: ReferenceInfo::from(row) })
    }
}
