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
use nexus_orchestration::applied_document::{AppliedDocument, AppliedDocumentSource};
use nexus_preset::LoadedPreset;
use nexus_session_storage::run_state::AppliedRunIdentity;

use crate::apply::ApplyLoadRefusal;
use crate::error::{CoreError, CoreResult};

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
/// is unreadable or names a mode outside `ir-only` — a mistyped key, or the
/// removed `yaml-only` arm, fails closed instead of silently selecting an arm.
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
/// Two identities answer here. A user bundle answers from its current apply
/// (the store route below). An EMBEDDED bundle answers from the document its
/// own compiled-in bytes construct, under the compass D10 identity
/// `apply_id = "embedded:<preset_id>"` / `content_hash =
/// nexus_workflow_ir::canonical::content_hash(document)` — the same hash a user
/// apply records, so a changed embedded byte flips it and recovery refuses
/// instead of falling back to the YAML graph. `_system.` and system-directory
/// bundles stay `None`: they execute from their manifest.
///
/// `nexus_home` is the workspace home the pins are re-compared against; the pin
/// re-comparison runs against `<nexus_home>/patterns`, so no caller can skip it.
///
/// # Errors
/// The store failure as an outer `Err`; the unverifiable applied bytes as the
/// inner `Err`, which each caller names in its own refusal type.
pub async fn resolve_applied_document(
    pool: &sqlx::SqlitePool,
    nexus_home: &std::path::Path,
    preset_id: &str,
    loaded: Option<&LoadedPreset>,
) -> CoreResult<Result<Option<AppliedDocument>, ApplyLoadRefusal>> {
    // An embedded bundle's applied identity is its own compiled-in bytes, not a
    // store row: the document is constructed here from the same `EMBEDDED_PRESETS`
    // tree the run reads, so admission and recovery reach one identity through
    // one route. A hash failure is a fault of this route (outer `Err`), never a
    // silent `None` that would rebuild the YAML graph.
    if let Some(loaded) = loaded {
        if let Some(nexus_preset::source_identity::PresetSourceIdentity::Embedded {
            preset_id,
            ..
        }) = &loaded.source_identity
        {
            let document = nexus_preset::embedded_workflow_document(loaded).map_err(|error| {
                CoreError::Internal {
                    category: format!("embedded applied document of `{preset_id}`: {error}"),
                }
            })?;
            let (apply_id, content_hash) =
                nexus_preset::embedded_applied_identity(preset_id, &document).map_err(|error| {
                    CoreError::Internal {
                        category: format!(
                            "embedded applied document of `{preset_id}` cannot be hashed: {error}"
                        ),
                    }
                })?;
            return Ok(Ok(Some(AppliedDocument {
                identity: AppliedRunIdentity {
                    apply_id,
                    content_hash,
                },
                document,
            })));
        }
    }
    // The loader's own classification, exactly as admission reads it: only a
    // user bundle with a current apply has an applied identity. `None` is the
    // applied re-entry (I-000443: the run's bundle may be gone), and it cannot
    // skip a refusal a `Some` would raise — `Some` of a NON-user bundle answers
    // `Ok(None)` (an accept, never a refusal), and every other `Some` runs this
    // same `current_apply` + `verified_apply_document` pair, so `None` only
    // ever ADDS verification. Nor can an identity resolved here under the
    // looser classification widen acceptance: `reconstruct_runner` rebuilds a
    // run only when this identity equals the one the descriptor froze.
    if loaded.is_some_and(|loaded| !crate::apply::is_user_preset(preset_id, loaded)) {
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
///
/// ONE home serves both routes: [`AppliedAdmissionPort::admit`] and
/// [`AppliedDocumentSource::applied_document`] both re-compare the frozen pins
/// against [`Self::nexus_home`] (I-000506). Admission once took the home from
/// its caller while recovery could only read this field — two values that only
/// wiring kept equal. A caller home that had moved its pinned pattern while
/// this one had not let recovery verify against stale bytes and ACCEPT an
/// applied run the admitting home would refuse as non-replayable: the same
/// accepting direction as the applied-identity defect, so the second value is
/// gone rather than policed.
pub struct CreatorAppliedDocuments {
    pool: Arc<sqlx::SqlitePool>,
    nexus_home: std::path::PathBuf,
}

impl CreatorAppliedDocuments {
    /// Bind the source to the Creator DB pool admission resolves applies on, and
    /// to the workspace home BOTH admission and recovery read — the engine's own
    /// `nexus_home`, so an admission resolves the pins against the same
    /// workspace a restart re-compares them against.
    #[must_use]
    pub const fn new(pool: Arc<sqlx::SqlitePool>, nexus_home: std::path::PathBuf) -> Self {
        Self { pool, nexus_home }
    }
}

#[async_trait]
impl AppliedAdmissionPort for CreatorAppliedDocuments {
    async fn admit(
        &self,
        preset_id: &str,
        loaded: &LoadedPreset,
    ) -> Result<AppliedAdmission, AppliedAdmissionError> {
        // The intake key governs user bundles only; a non-user bundle reads no
        // apply under it. The one remaining arm (`ir-only`) always resolves.
        let intake_mode = intake_mode_for_admission(&self.nexus_home, preset_id, loaded)
            .map_err(AppliedAdmissionError::InvalidIntakeConfig)?;
        let applied =
            resolve_applied_document(&self.pool, &self.nexus_home, preset_id, Some(loaded))
                .await
                .map_err(|error| AppliedAdmissionError::Store(error.to_string()))?
                .map_err(|refusal| AppliedAdmissionError::AppliedIdentity {
                    // The coded pair `ApplyLoadRefusal::into_error` builds: the
                    // stable code plus the sentence naming strategy and apply.
                    code: refusal.reason().to_string(),
                    message: refusal.to_string(),
                })?;
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
        loaded: Option<&LoadedPreset>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;

    /// A pool neither the embedded arm nor the system arm may query. `connect_lazy`
    /// builds it without opening a connection, so a stray query fails the test loudly
    /// instead of being answered by an empty database.
    fn unqueried_pool() -> sqlx::SqlitePool {
        sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .expect("a lazy pool")
    }

    fn unqueried_home() -> std::path::PathBuf {
        std::path::PathBuf::from("/nonexistent-nexus-home")
    }

    /// An embedded bundle resolves the document it executes, under the compass D10
    /// identity — computed from the same constructor admission and recovery use.
    #[tokio::test]
    async fn an_embedded_preset_resolves_the_document_it_runs() {
        let caps = BuiltinCapabilityCatalog;
        let loaded =
            nexus_preset::load_embedded_preset("memory-augmented", &caps).expect("preset loads");
        let resolved = resolve_applied_document(
            &unqueried_pool(),
            &unqueried_home(),
            "memory-augmented",
            Some(&loaded),
        )
        .await
        .expect("the embedded arm reads no store")
        .expect("an embedded bundle resolves its identity");
        let resolved = resolved.expect("an embedded bundle has an applied document");

        assert_eq!(resolved.identity.apply_id, "embedded:memory-augmented");
        let expected_hash = nexus_workflow_ir::canonical::content_hash(&resolved.document)
            .expect("the constructed document hashes");
        assert_eq!(
            resolved.identity.content_hash, expected_hash,
            "D10: the identity is the document's canonical content hash"
        );
        let constructed =
            nexus_preset::embedded_workflow_document(&loaded).expect("the bundle constructs");
        assert_eq!(
            nexus_preset::canonical_twin_json(&resolved.document).expect("serializes"),
            nexus_preset::canonical_twin_json(&constructed).expect("serializes"),
            "the run input is the document the embedded constructor builds"
        );
    }

    /// A `_system.` directory bundle keeps `Ok(None)`: it executes from its manifest,
    /// never through an applied identity, and the store is never read for it.
    #[tokio::test]
    async fn a_system_bundle_resolves_no_applied_document() {
        let home = tempfile::tempdir().expect("a temporary nexus home");
        nexus_preset::system_preset_dir::ensure_maintenance_preset(home.path())
            .expect("the system bundle is written");
        let bundle =
            nexus_preset::system_preset_dir::system_preset_bundle_dir(home.path(), "maintenance");
        let entry = nexus_preset::system_preset_dir::load_system_preset_from_dir(
            &bundle,
            "maintenance",
            &BuiltinCapabilityCatalog,
        )
        .expect("the system bundle loads");

        let resolved = resolve_applied_document(
            &unqueried_pool(),
            home.path(),
            &entry.qualified_id,
            Some(&entry.loaded),
        )
        .await
        .expect("a system bundle reads no store")
        .expect("a system bundle is not a refusal");
        assert!(
            resolved.is_none(),
            "a `_system.` bundle executes from its manifest, not an applied document"
        );
    }
}
