//! Retained CLI parameter/result translation over the core pack owner.
//!
//! No conflict detection, remapping, SQL, provenance write or persistence lives here.
//! P6-T1 removes this module together with the CLI callsite migration.

use nexus_contracts::daemon_api::kb::{PackImportRequest, PackImportRequestConflict};
use nexus_core::{HolderMapping, ImportQuarantineReview};
use nexus_spoke_adapter::pack::ParsedPack;
use sqlx::SqlitePool;
use thiserror::Error;

// The complete report types stay core-owned: this module translates the
// retained CLI shape onto the core owner and re-exports what the CLI prints, so
// there is no second copy of the outcome/quarantine vocabulary to drift.
pub use nexus_core::{
    AtomCounts, ImportAtomKind, ImportDetail, ImportOutcome, ImportSummary, QuarantineReason,
    QuarantinedAtomReport, REVIEW_IMPORT_MAX_ATOMS,
};

/// Provenance stamp on imported Knowledge entries and relations.
pub const IMPORT_PROVENANCE: &str = "pack_import";

/// Retained CLI argument adapter; core owns conflict-policy execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Skip colliding entries/relations.
    Skip,
    /// Disambiguate colliding canonical names and mint fresh ids.
    Rename,
    /// Replace colliding entry/relation bodies via orchestrator CAS upsert.
    Overwrite,
}

/// Fatal errors from [`import_pack`] (storage / parse failures).
#[derive(Debug, Error)]
pub enum PackImportError {
    #[error("pack import storage error: {0}")]
    Storage(String),
}

/// Translate the retained CLI shape, delegating every effect to the core owner.
///
/// `holder_map` carries the CLI's explicit `--holder-map` adoptions; the core
/// owns their admission and their effect (durable §6).
///
/// # Errors
///
/// Returns the legacy error carrier for core authorization, parse, mapping or
/// storage errors.
///
/// # Panics
///
/// Panics only if the caller supplies a `world_id`/`creator_id` pair the core
/// admission rejects as malformed (the CLI always passes resolved ids).
// transitional: single deletion owner P6-T1; no new callers
#[allow(clippy::too_many_arguments)] // one admission seam forwarding every import dependency
pub async fn import_pack(
    pool: &SqlitePool,
    world_id: &str,
    creator_id: &str,
    pack: ParsedPack,
    conflict: ConflictPolicy,
    include_anchors: bool,
    holder_map: Vec<HolderMapping>,
    dry_run: bool,
) -> Result<ImportSummary, PackImportError> {
    // The parsed pack document travels through **verbatim**
    // (`ParsedPack::source`) instead of being rebuilt from the typed atoms: a
    // rebuild re-serializes every atom, which is exactly the drift the import
    // boundary must not introduce (a quarantined atom keeps the document's own
    // JSON).
    let serde_json::Value::Object(pack) = pack.source.clone() else {
        unreachable!("parse_pack only accepts a JSON object")
    };
    // The CLI carries its adoptions as parsed `HolderMapping`s (the bridge's
    // out-of-band argument), so the retained wire literal stays on the import
    // arm: no `review_import` selector, no duplicate wire mappings.
    let request = PackImportRequest {
        pack,
        conflict: match conflict {
            ConflictPolicy::Skip => PackImportRequestConflict::Skip,
            ConflictPolicy::Rename => PackImportRequestConflict::Rename,
            ConflictPolicy::Overwrite => PackImportRequestConflict::Overwrite,
        },
        include_anchors,
        holder_map: Vec::new(),
        review_import: None,
    };
    nexus_core::CoreService::import_legacy_world_pack(
        pool, creator_id, world_id, request, holder_map, dry_run,
    )
    .await
    .map_err(|e| PackImportError::Storage(e.to_string()))
}

/// Translate the retained CLI's read-only quarantine review arm (durable §6).
///
/// # Errors
///
/// Returns the legacy error carrier for core authorization or storage errors.
// transitional: single deletion owner P6-T1; no new callers
pub async fn review_import(
    pool: &SqlitePool,
    world_id: &str,
    creator_id: &str,
    import_batch_id: &str,
) -> Result<ImportQuarantineReview, PackImportError> {
    nexus_core::CoreService::review_legacy_world_pack_import(
        pool,
        creator_id,
        world_id,
        import_batch_id,
    )
    .await
    .map_err(|e| PackImportError::Storage(e.to_string()))
}
