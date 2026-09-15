//! Retained CLI parameter/result translation over the core pack owner.
//! No conflict detection, remapping, SQL, provenance write or persistence lives here.
//! P6-T1 removes this module together with the CLI callsite migration.

use nexus_contracts::daemon_api::kb::{
    PackImportRequest, PackImportRequestConflict, PackImportResponseDetailsItemKind,
    PackImportResponseDetailsItemOutcome,
};
use nexus_spoke_adapter::pack::{build_pack, ParsedPack};
use sqlx::SqlitePool;
use thiserror::Error;

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

/// Per-atom outcome counters for import summary reporting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AtomCounts {
    pub created: u32,
    pub skipped: u32,
    pub rejected: u32,
    pub renamed: u32,
    pub overwritten: u32,
}

/// Whether an import detail row refers to an entry or a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAtomKind {
    Entry,
    Relation,
}

/// Outcome of importing one pack atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportOutcome {
    Created,
    Skipped,
    Rejected,
    Renamed,
    Overwritten,
}

/// One row in the structured import report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDetail {
    pub kind: ImportAtomKind,
    pub id: String,
    pub outcome: ImportOutcome,
    pub reason: Option<String>,
}

/// Structured import result returned to CLI and daemon callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub entries: AtomCounts,
    pub relations: AtomCounts,
    pub details: Vec<ImportDetail>,
}

/// Fatal errors from [`import_pack`] (storage / parse failures).
#[derive(Debug, Error)]
pub enum PackImportError {
    #[error("pack import storage error: {0}")]
    Storage(String),
}

/// Translate the retained CLI shape, delegating every effect to the core owner.
/// # Errors
/// Returns the legacy error carrier for core authorization, parse or storage errors.
// transitional: single deletion owner P6-T1; no new callers
pub async fn import_pack(
    pool: &SqlitePool,
    world_id: &str,
    creator_id: &str,
    pack: ParsedPack,
    conflict: ConflictPolicy,
    include_anchors: bool,
    dry_run: bool,
) -> Result<ImportSummary, PackImportError> {
    let value = build_pack(
        &pack.entries, &pack.relations, pack.source_anchors.as_deref(),
        &pack.pack_metadata.title, &pack.pack_metadata.version, &pack.pack_metadata.creator,
        pack.pack_metadata.description.as_deref(), Some(&pack.extra_modules),
    );
    let serde_json::Value::Object(pack) = value else {
        unreachable!("build_pack always produces a JSON object")
    };
    let request = PackImportRequest {
        pack,
        conflict: match conflict {
            ConflictPolicy::Skip => PackImportRequestConflict::Skip,
            ConflictPolicy::Rename => PackImportRequestConflict::Rename,
            ConflictPolicy::Overwrite => PackImportRequestConflict::Overwrite,
        },
        include_anchors,
    };
    let response = nexus_core::CoreService::import_legacy_world_pack(
        pool, creator_id, world_id, request, dry_run,
    ).await.map_err(|e| PackImportError::Storage(e.to_string()))?;
    // Core counts are derived from the same bounded u32 algorithm as this
    // retained result shape; conversion cannot lose a count.
    let count = |n| u32::try_from(n).expect("core pack count fits legacy u32");
    Ok(ImportSummary {
        entries: AtomCounts {
            created: count(response.entries.created), skipped: count(response.entries.skipped),
            rejected: count(response.entries.rejected), renamed: count(response.entries.renamed),
            overwritten: count(response.entries.overwritten),
        },
        relations: AtomCounts {
            created: count(response.relations.created), skipped: count(response.relations.skipped),
            rejected: count(response.relations.rejected), renamed: count(response.relations.renamed),
            overwritten: count(response.relations.overwritten),
        },
        details: response.details.into_iter().map(|detail| ImportDetail {
            kind: match detail.kind {
                PackImportResponseDetailsItemKind::Entry => ImportAtomKind::Entry,
                PackImportResponseDetailsItemKind::Relation => ImportAtomKind::Relation,
            },
            id: detail.id,
            outcome: match detail.outcome {
                PackImportResponseDetailsItemOutcome::Created => ImportOutcome::Created,
                PackImportResponseDetailsItemOutcome::Skipped => ImportOutcome::Skipped,
                PackImportResponseDetailsItemOutcome::Rejected => ImportOutcome::Rejected,
                PackImportResponseDetailsItemOutcome::Renamed => ImportOutcome::Renamed,
                PackImportResponseDetailsItemOutcome::Overwritten => ImportOutcome::Overwritten,
            },
            reason: detail.reason,
        }).collect(),
    })
}
