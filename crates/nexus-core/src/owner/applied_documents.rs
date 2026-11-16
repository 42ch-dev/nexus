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
use nexus_execution::admission::{AppliedAdmission, AppliedAdmissionError, AppliedAdmissionPort};
use nexus_orchestration::capability::{AppliedDocument, AppliedDocumentSource};
use nexus_orchestration::run_state::AppliedRunIdentity;
use nexus_preset::LoadedPreset;

use crate::apply::ApplyLoadRefusal;
use crate::error::CoreResult;

/// The intake mode one new admission runs under (P6 condition 6).
///
/// `Ok(None)` = the preset is not a user bundle (embedded, `_system.`, or a
/// directory the loader resolved as system): the intake key never governs it
/// and the descriptor records no mode. `Ok(Some(mode))` = a user bundle: the
/// mode `[strategy_intake]` selected, to freeze on the descriptor so recovery
/// follows the recorded arm, not the current config.
///
/// # Errors
/// The `intake_config_invalid`-prefixed reason when the home config document
/// is unreadable or names a mode outside `ir-only`/`yaml-only` — a mistyped
/// key fails closed instead of silently selecting an arm.
pub fn intake_mode_for_admission(
    nexus_home: &std::path::Path,
    preset_id: &str,
    loaded: &LoadedPreset,
) -> Result<Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>, String> {
    if !crate::apply::is_user_preset(preset_id, loaded) {
        return Ok(None);
    }
    nexus_home_layout::strategy_intake::strategy_intake_mode(nexus_home).map(Some)
}

/// Resolve the apply `preset_id` runs under: its current identity plus the
/// document the identity pins, or `None` when the strategy has no current apply.
///
/// This is the ONE route to an applied document. Schedule admission,
/// `WorkflowRunCoordinator::start_session` and recovery all call it, so a run
/// admitted under a resolved document is never rebuilt from another one. A run
/// admitted while the strategy had no current apply resolves `None` and
/// therefore freezes no identity — that case is recovery's, not this route's.
///
/// `nexus_home` is the workspace home the strategy's bundle was resolved from; the pin
/// re-comparison runs against `<nexus_home>/patterns`, so no caller can skip it.
///
/// # Errors
/// The store failure as an outer `Err`; the unverifiable applied bytes as the
/// inner `Err`, which each caller names in its own refusal type.
pub async fn resolve_applied_document(
    pool: &sqlx::SqlitePool,
    nexus_home: &std::path::Path,
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
    match crate::apply::verified_apply_document(pool, nexus_home, preset_id, &apply).await? {
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
    nexus_home: std::path::PathBuf,
}

impl CreatorAppliedDocuments {
    /// Bind the source to the Creator DB pool admission resolves applies on, and
    /// to the workspace home recovery re-enters through — the engine's own
    /// `nexus_home`, so a restart re-compares the reserved pin against the same
    /// workspace the run was admitted under.
    ///
    /// Admission does not read this field: [`AppliedAdmissionPort::admit`] takes
    /// the home from its caller, which is the home that resolved the preset.
    #[must_use]
    pub const fn new(pool: Arc<sqlx::SqlitePool>, nexus_home: std::path::PathBuf) -> Self {
        Self { pool, nexus_home }
    }
}

#[async_trait]
impl AppliedAdmissionPort for CreatorAppliedDocuments {
    async fn admit(
        &self,
        nexus_home: &std::path::Path,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<AppliedAdmission, AppliedAdmissionError> {
        // The intake key governs user bundles only; `yaml-only` runs the YAML
        // bundle without reading the apply at all.
        let intake_mode = intake_mode_for_admission(nexus_home, preset_id, loaded)
            .map_err(AppliedAdmissionError::InvalidIntakeConfig)?;
        let applied = match intake_mode {
            Some(nexus_home_layout::strategy_intake::StrategyIntakeMode::YamlOnly) => None,
            _ => resolve_applied_document(&self.pool, nexus_home, preset_id, loaded)
                .await
                .map_err(|error| AppliedAdmissionError::Store(error.to_string()))?
                .map_err(|refusal| AppliedAdmissionError::AppliedIdentity {
                    // The coded pair `ApplyLoadRefusal::into_error` builds: the
                    // stable code plus the sentence naming strategy and apply.
                    code: refusal.reason().to_string(),
                    message: refusal.to_string(),
                })?,
        };
        Ok(AppliedAdmission {
            intake_mode,
            applied,
        })
    }
}

#[async_trait]
impl AppliedDocumentSource for CreatorAppliedDocuments {
    async fn applied_document(
        &self,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<Option<AppliedDocument>, String> {
        resolve_applied_document(&self.pool, &self.nexus_home, preset_id, loaded)
            .await
            .map_err(|error| error.to_string())?
            // The stable code travels with the message: recovery wraps whatever
            // arrives here in `reconstruction_unavailable`, so a bare human
            // sentence would lose the `applied_pin_*` family a caller branches on.
            .map_err(|refusal| format!("{}: {refusal}", refusal.reason()))
    }
}
