//! Recovery-side applied identity (P4, `.mstar/plans/p4a-applied-identity.md`
//! Task 2).
//!
//! Admission resolves a schedule's current apply before it builds the run's
//! graph; recovery must re-enter through the same resolution, otherwise a
//! daemon restart during an in-flight applied run rebuilds the YAML
//! descriptor's graph instead of the applied one.
//!
//! The whole route lives here, over the same functions admission calls:
//! [`crate::apply::is_user_preset`], [`crate::apply::current_apply`] and
//! [`crate::apply::verified_apply_document`]. No SQL, no cache and no second
//! reader of `strategy_approvals.document_json` — an applied document whose
//! frozen bytes no longer hash to the identity the apply recorded is the same
//! typed refusal here as at admission.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_orchestration::capability::{AppliedDocument, AppliedDocumentSource};
use nexus_preset::LoadedPreset;

/// Reads the current apply of a strategy out of the Creator DB.
pub struct CreatorAppliedDocuments {
    pool: Arc<sqlx::SqlitePool>,
}

impl CreatorAppliedDocuments {
    /// Bind the source to the Creator DB pool admission resolves applies on.
    #[must_use]
    pub fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
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
        // The loader's own classification, exactly as admission reads it: only
        // a user bundle with a current apply has an applied identity.
        if !crate::apply::is_user_preset(preset_id, loaded) {
            return Ok(None);
        }
        let Some(apply) = crate::apply::current_apply(&self.pool, preset_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        match crate::apply::verified_apply_document(&self.pool, preset_id, &apply)
            .await
            .map_err(|error| error.to_string())?
        {
            Ok(document) => Ok(Some(AppliedDocument {
                identity: nexus_orchestration::run_state::AppliedRunIdentity {
                    apply_id: apply.apply_id,
                    content_hash: apply.content_hash,
                },
                document,
            })),
            Err(refusal) => Err(refusal.to_string()),
        }
    }
}
