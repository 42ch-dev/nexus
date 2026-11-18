//! Applied-document contracts: the current-apply resolution seam shared by
//! admission and recovery.
//!
//! Design: `docs/specs/orchestration/strategy-composition.md`.

use async_trait::async_trait;

/// Resolves the current applied identity a schedule run executes (P4).
///
/// Admission builds an applied run from the strategy's applied document instead
/// of its YAML descriptor; recovery therefore asks THIS source for the same
/// resolution, so a restart mid-run re-enters the applied identity rather than
/// rebuilding the descriptor's graph. The source owns the whole route — the
/// user-preset classification, the current-apply read and the content-hash
/// verification — so there is exactly one reader of the approved bytes.
///
/// `preset_id` is the run's strategy id and `loaded` is the bundle the frozen
/// descriptor named (the classification reads the source identity the loader
/// recorded, never a fresh preset lookup). `None` is a re-entry that holds no
/// bundle: a USER applied run's recovery, whose frozen applied identity already
/// encodes the classification admission made. An embedded applied run does pass
/// its reloaded bundle — the embedded arm is keyed off that source identity, so
/// the compiled-in bytes are re-resolved and re-compared on every restart.
/// `Ok(None)` is "this strategy has no current apply", the common case for a
/// user bundle; an embedded bundle always resolves its document.
#[async_trait]
pub trait AppliedDocumentSource: Send + Sync {
    /// The current apply of `preset_id` — its frozen identity plus the verified
    /// document that identity pins — or `None` when the strategy has no current
    /// apply.
    ///
    /// The identity travels with the document because recovery must compare it
    /// against the identity frozen on the run's descriptor before building: the
    /// current apply may have moved past the one the run was admitted under.
    /// An embedded bundle's identity is `embedded:<preset_id>` over the document
    /// its own compiled-in bytes construct, so a changed embedded byte is the
    /// same non-replayable comparison.
    ///
    /// # Errors
    /// The refusal or store failure that makes the applied identity
    /// unreplayable, as the message recovery surfaces.
    async fn applied_document(
        &self,
        preset_id: &str,
        loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> Result<Option<AppliedDocument>, String>;
}

/// A strategy's current apply: the identity it installed and the document it pinned.
///
/// Built only after the document's frozen bytes re-verified against
/// `identity.content_hash`, so holding one means the pair is consistent.
#[derive(Debug, Clone)]
pub struct AppliedDocument {
    /// The apply row that is current for the strategy.
    pub identity: nexus_session_storage::run_state::AppliedRunIdentity,
    /// The verified document those bytes decode to.
    pub document:
        nexus_contracts::generated::daemon_api::orchestration::workflow_document::WorkflowDocument,
}
