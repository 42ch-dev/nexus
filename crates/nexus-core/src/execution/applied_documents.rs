//! Recovery-side applied identity (P4, `.mstar/plans/p4a-applied-identity.md`
//! Task 2).
//!
//! Admission resolves a schedule's current apply before it builds the run's
//! graph; recovery must re-enter through the same resolution, otherwise a
//! daemon restart during an in-flight applied run rebuilds the YAML
//! descriptor's graph instead of the applied one.
//!
//! The whole route lives in [`resolve_applied_document`], over the same
//! functions admission calls: [`crate::apply::is_user_preset`],
//! [`crate::apply::current_apply`] and
//! [`crate::apply::verified_apply_document`]. No SQL, no cache and no second
//! reader of `strategy_approvals.document_json` — an applied document whose
//! frozen bytes no longer hash to the identity the apply recorded is the same
//! typed refusal here as at admission.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_orchestration::capability::{AppliedDocument, AppliedDocumentSource};
use nexus_orchestration::run_state::AppliedRunIdentity;
use nexus_preset::LoadedPreset;

use crate::apply::ApplyLoadRefusal;
use crate::error::CoreResult;

/// Resolve the apply `preset_id` runs under: its current identity plus the
/// document the identity pins, or `None` when the strategy has no current apply.
///
/// This is the ONE route to an applied document. Schedule admission,
/// `WorkflowRunCoordinator::start_session` and recovery all call it, so a run
/// can never be admitted under one document and rebuilt from another.
///
/// `nexus_home` is the workspace home the strategy's bundle was resolved from;
/// the pin re-comparison that reads it (`p6-pin-freeze-and-bundle-import`) runs
/// here so no caller can skip it.
///
/// # Errors
/// The store failure as an outer `Err`; the unverifiable applied bytes as the
/// inner `Err`, which each caller names in its own refusal type.
pub async fn resolve_applied_document(
    pool: &sqlx::SqlitePool,
    _nexus_home: &std::path::Path,
    preset_id: &str,
    loaded: &LoadedPreset,
) -> CoreResult<Result<Option<AppliedDocument>, ApplyLoadRefusal>> {
    // The loader's own classification, exactly as admission reads it: only a
    // user bundle with a current apply has an applied identity.
    if !crate::apply::is_user_preset(preset_id, loaded) {
        return Ok(Ok(None));
    }
    let Some(apply) = crate::apply::current_apply(pool, preset_id).await? else {
        return Ok(Ok(None));
    };
    match crate::apply::verified_apply_document(pool, preset_id, &apply).await? {
        Ok(document) => Ok(Ok(Some(AppliedDocument {
            identity: AppliedRunIdentity {
                apply_id: apply.apply_id,
                content_hash: apply.content_hash,
            },
            document,
        }))),
        Err(refusal) => Ok(Err(refusal)),
    }
}

/// Reads the current apply of a strategy out of the Creator DB.
pub struct CreatorAppliedDocuments {
    pool: Arc<sqlx::SqlitePool>,
}

impl CreatorAppliedDocuments {
    /// Bind the source to the Creator DB pool admission resolves applies on.
    #[must_use]
    pub const fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AppliedDocumentSource for CreatorAppliedDocuments {
    async fn applied_document(
        &self,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<Option<AppliedDocument>, String> {
        resolve_applied_document(
            &self.pool,
            // shortcut: recovery carries no workspace home, so the pin
            // re-comparison reads an empty one; pass the real home when this
            // source is constructed with it.
            std::path::Path::new(""),
            preset_id,
            loaded,
        )
        .await
        .map_err(|error| error.to_string())?
        .map_err(|refusal| refusal.to_string())
    }
}
