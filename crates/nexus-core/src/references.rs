//! Reference source authority over the guarded core workspace.
//!
//! Registry reads and the register producer for the V1.26 reference store
//! (`SQLite` registry + `body.md` on disk). Extracted from the legacy daemon
//! `references.rs` handlers (P1-T3); the cloud-backed refresh remains an
//! explicit caller concern and is deliberately not owned here.
//!
//! The response envelopes are the schema-owned projections
//! (`schemas/core/references/*.schema.json`), not local re-declarations, so the
//! `reference show` render fields (`workspace_id`/`updated_at`/`tags`/
//! `content_hash`/`refresh_policy`) reach the CLI without a second DTO.

use nexus_contracts::core::references::{
    ReferenceGetResponse, ReferenceListResponse, ReferenceSourceInfo,
};
use nexus_local_db::reference_source::ReferenceSourceRow;
use nexus_local_db::SourceMutability;

use crate::error::local_db_err;
use crate::{CoreError, CoreResult, CoreService, Principal};

/// Input for [`CoreService::register_reference`].
///
/// The creator, workspace binding and body-store home are derived from the
/// open service and the verified principal, so a caller cannot register into a
/// workspace it is not admitted to.
#[derive(Debug, Clone)]
pub struct RegisterReferenceParams {
    /// Contract enum string (`file`, `url`, `pdf`, `note`).
    pub source_type: String,
    /// Mutability policy (`static` or `refreshable`).
    pub source_mutability: SourceMutability,
    /// Logical locator URI.
    pub uri: String,
    /// Human-readable title.
    pub title: String,
    /// Serialized tag list (optional).
    pub tags: Option<String>,
    /// Canonical body text.
    pub body: String,
}

/// Project a registry row onto the schema-owned reference metadata DTO.
fn to_reference_info(row: &ReferenceSourceRow) -> ReferenceSourceInfo {
    ReferenceSourceInfo {
        reference_source_id: row.reference_source_id.clone(),
        source_type: row.source_type.clone(),
        source_mutability: row.source_mutability.clone(),
        uri: row.uri.clone(),
        title: row.title.clone(),
        content_path: row.content_path.clone(),
        scan_status: row.scan_status.clone(),
        created_at: row.created_at.clone(),
    }
}

/// Project a registry row onto the get envelope, including the show-render
/// fields the lean metadata projection omits.
fn to_get_response(row: &ReferenceSourceRow) -> ReferenceGetResponse {
    ReferenceGetResponse {
        reference: to_reference_info(row),
        workspace_id: row.workspace_id.clone(),
        updated_at: row.updated_at.clone(),
        tags: row.tags.clone(),
        content_hash: row.content_hash.clone(),
        refresh_policy: row.refresh_policy.clone(),
    }
}

/// Raw user home (`<user_home>/.nexus42` is the open-scoped nexus root); the
/// body-store layout below it is keyed by the raw home.
fn raw_user_home(nexus_home: &std::path::Path) -> CoreResult<&std::path::Path> {
    nexus_home.parent().ok_or_else(|| CoreError::Internal {
        category: "nexus_home_without_parent".to_string(),
    })
}

impl CoreService {
    /// Register a new reference source: the registry row lands in `SQLite`
    /// first, then the canonical `body.md` is written under the creator root.
    ///
    /// The workspace binding is the operational `wrk_<slug>` convention the
    /// registry rows already carry, derived from the verified principal.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the storage carrier (legacy `DATABASE_ERROR`) when the
    /// insert or the body write fails.
    pub async fn register_reference(
        &self,
        principal: &Principal,
        params: RegisterReferenceParams,
    ) -> CoreResult<ReferenceSourceInfo> {
        self.verify_principal(principal)?;
        let user_home = raw_user_home(&self.inner.nexus_home)?;
        let workspace_id = format!("wrk_{}", principal.workspace_slug());
        let row = nexus_local_db::register_reference(
            &self.inner.pool,
            nexus_local_db::RegisterParams {
                home: user_home,
                creator_id: principal.creator_id(),
                workspace_id: &workspace_id,
                source_type: &params.source_type,
                source_mutability: params.source_mutability,
                uri: &params.uri,
                title: &params.title,
                tags: params.tags.as_deref(),
                body: &params.body,
            },
        )
        .await
        .map_err(local_db_err)?;
        Ok(to_reference_info(&row))
    }

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
    ) -> CoreResult<ReferenceListResponse> {
        self.verify_principal(principal)?;
        let rows = nexus_local_db::list_references(&self.inner.pool, None, None, None)
            .await
            .map_err(local_db_err)?;
        let references = rows.iter().map(to_reference_info).collect();
        Ok(ReferenceListResponse { references })
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
    ) -> CoreResult<ReferenceGetResponse> {
        self.verify_principal(principal)?;
        let row = nexus_local_db::get_reference_by_id(&self.inner.pool, &reference_id)
            .await
            .map_err(local_db_err)?
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("reference_source: {reference_id}"),
            })?;
        Ok(to_get_response(&row))
    }
}
