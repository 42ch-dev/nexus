//! World KB pack routes — Narrative Knowledge Pack export/import (V1.152 P0,
//! DF-77).
//!
//! - `POST /v1/daemon/worlds/:world_id/kb/pack/export` — export one World's
//!   lore as a handbook pack (read-only; mirrors CLI `pack export`).
//! - `POST /v1/daemon/worlds/:world_id/kb/pack/import` — import a handbook pack
//!   into a World under a conflict policy (mirrors CLI `pack import`).

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner};
use crate::pack_import::{
    import_pack, ConflictPolicy, ImportAtomKind, ImportOutcome, ImportSummary, PackImportError,
};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::generated::daemon_api::kb::{
    pack_export_request::PackExportRequest,
    pack_export_response::PackExportResponse,
    pack_import_request::{PackImportRequest, PackImportRequestConflict},
    pack_import_response::{
        PackImportResponse, PackImportResponseDetailsItem, PackImportResponseDetailsItemKind,
        PackImportResponseDetailsItemOutcome, PackImportResponseEntries,
        PackImportResponseRelations,
    },
};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_relationships::list_relationships_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::conversion::{kb_relationship_row_to_spoke, knowledge_record_to_spoke};
use nexus_spoke_adapter::pack::{build_pack, parse_pack};
use std::collections::HashSet;

/// Default `modules.pack.version` when the request omits `pack_version`.
const DEFAULT_PACK_VERSION: &str = "0.1.0";

// ─── Shared guards (imported from `world_kb_guards` — R-V1152P0-002) ───────

async fn resolve_world_title(
    pool: &sqlx::SqlitePool,
    world_id: &str,
) -> Result<String, NexusApiError> {
    // Runtime query: mirrors `world_kb.rs` pattern; table schema is stable.
    let title: Option<String> =
        sqlx::query_scalar("SELECT title FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(NexusApiError::from)?
            .flatten();

    title.ok_or_else(|| NexusApiError::NotFound(format!("world {world_id}")))
}

async fn resolve_creator_string(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
) -> Result<String, NexusApiError> {
    // Runtime query: mirrors `world_kb.rs` pattern; table schema is stable.
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
            .bind(creator_id)
            .fetch_optional(pool)
            .await
            .map_err(NexusApiError::from)?
            .flatten();

    Ok(display_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| creator_id.to_string()))
}

async fn load_pack_anchors(
    pool: &sqlx::SqlitePool,
    entry_ids: &HashSet<String>,
) -> Result<Vec<nexus_spoke_adapter::SourceAnchor>, NexusApiError> {
    let mut anchors = Vec::new();
    for entry_id in entry_ids {
        // Runtime query: mirrors `world_kb.rs` pattern; table schema is stable.
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT source_anchor_json FROM kb_source_anchors WHERE key_block_id = ? ORDER BY anchor_ordinal ASC",
        )
        .bind(entry_id)
        .fetch_all(pool)
        .await
        .map_err(NexusApiError::from)?;
        for json in rows {
            if let Ok(anchor) = serde_json::from_str::<nexus_spoke_adapter::SourceAnchor>(&json) {
                anchors.push(anchor);
            }
        }
    }
    Ok(anchors)
}

// ─── POST /v1/daemon/worlds/:world_id/kb/pack/export ────────────────────────

/// Export one World's lore as a Narrative Knowledge Pack (V1.152 P0).
///
/// Guard order: tier2 middleware → `require_creator` → `require_world_owner`
/// → business logic (mirrors CLI `pack.rs::export`).
// Handler boundary: error docs on internal axum handlers add noise without aiding
// callers (matches `world_kb.rs` pattern).
#[allow(clippy::missing_errors_doc)]
pub async fn pack_export(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackExportRequest>,
) -> Result<Json<PackExportResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    let world_title = resolve_world_title(pool, &world_id).await?;
    let creator = resolve_creator_string(pool, &creator_id).await?;

    let store = SqliteKbStore::new(pool.clone());
    let mut entries = if req.include_deprecated {
        store
            .list_by_world_including_deprecated(&world_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("World KB list failed for {world_id}: {e}"),
            })?
    } else {
        store
            .list_by_world(&world_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("World KB list failed for {world_id}: {e}"),
            })?
    };

    entries.sort_by(|a, b| a.canonical_name.cmp(&b.canonical_name));
    let entry_ids: HashSet<String> = entries.iter().map(|e| e.entry_id.clone()).collect();

    let relation_rows = list_relationships_for_world(pool, &world_id, false, i64::MAX)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("Failed to list relations for {world_id}: {e}"),
        })?;

    let mut relations: Vec<nexus_spoke_adapter::Relation> = relation_rows
        .iter()
        .filter(|r| {
            entry_ids.contains(&r.source_entity_id) && entry_ids.contains(&r.target_entity_id)
        })
        .map(kb_relationship_row_to_spoke)
        .collect();
    relations.sort_by(|a, b| a.relation_id.cmp(&b.relation_id));

    let spoke_entries: Vec<nexus_spoke_adapter::KnowledgeEntry> =
        entries.iter().map(knowledge_record_to_spoke).collect();

    let anchor_vec = if req.include_anchors {
        load_pack_anchors(pool, &entry_ids).await?
    } else {
        Vec::new()
    };
    let anchors: Option<&[nexus_spoke_adapter::SourceAnchor]> = if req.include_anchors {
        Some(anchor_vec.as_slice())
    } else {
        None
    };

    let title = req.title.unwrap_or(world_title);
    let pack_version = req
        .pack_version
        .unwrap_or_else(|| DEFAULT_PACK_VERSION.to_string());
    let description = req.description.as_deref();

    let pack_value = build_pack(
        &spoke_entries,
        &relations,
        anchors,
        &title,
        &pack_version,
        &creator,
        description,
        None,
    );

    let resp: PackExportResponse =
        serde_json::from_value(pack_value).map_err(|e| NexusApiError::Internal {
            code: "PACK_EXPORT_DECODE".to_string(),
            message: format!(
                "build_pack output did not match the PackExportResponse wire shape: {e}"
            ),
        })?;

    Ok(Json(resp))
}

// ─── POST /v1/daemon/worlds/:world_id/kb/pack/import ──────────────────────

#[allow(clippy::missing_const_for_fn)]
fn conflict_policy_from_request(conflict: PackImportRequestConflict) -> ConflictPolicy {
    match conflict {
        PackImportRequestConflict::Skip => ConflictPolicy::Skip,
        PackImportRequestConflict::Rename => ConflictPolicy::Rename,
        PackImportRequestConflict::Overwrite => ConflictPolicy::Overwrite,
    }
}

fn atom_counts_to_entries(counts: crate::pack_import::AtomCounts) -> PackImportResponseEntries {
    PackImportResponseEntries {
        created: u64::from(counts.created),
        skipped: u64::from(counts.skipped),
        rejected: u64::from(counts.rejected),
        renamed: u64::from(counts.renamed),
        overwritten: u64::from(counts.overwritten),
    }
}

fn atom_counts_to_relations(counts: crate::pack_import::AtomCounts) -> PackImportResponseRelations {
    PackImportResponseRelations {
        created: u64::from(counts.created),
        skipped: u64::from(counts.skipped),
        rejected: u64::from(counts.rejected),
        renamed: u64::from(counts.renamed),
        overwritten: u64::from(counts.overwritten),
    }
}

fn import_summary_to_response(summary: ImportSummary) -> PackImportResponse {
    PackImportResponse {
        entries: atom_counts_to_entries(summary.entries),
        relations: atom_counts_to_relations(summary.relations),
        details: summary
            .details
            .into_iter()
            .map(|detail| PackImportResponseDetailsItem {
                kind: match detail.kind {
                    ImportAtomKind::Entry => PackImportResponseDetailsItemKind::Entry,
                    ImportAtomKind::Relation => PackImportResponseDetailsItemKind::Relation,
                },
                id: detail.id,
                outcome: match detail.outcome {
                    ImportOutcome::Created => PackImportResponseDetailsItemOutcome::Created,
                    ImportOutcome::Skipped => PackImportResponseDetailsItemOutcome::Skipped,
                    ImportOutcome::Rejected => PackImportResponseDetailsItemOutcome::Rejected,
                    ImportOutcome::Renamed => PackImportResponseDetailsItemOutcome::Renamed,
                    ImportOutcome::Overwritten => PackImportResponseDetailsItemOutcome::Overwritten,
                },
                reason: detail.reason,
            })
            .collect(),
    }
}

/// Import a handbook pack into a World under a conflict policy (V1.152 P0).
///
/// Guard order: tier2 middleware → `require_creator` → `require_world_owner`
/// → business logic (mirrors CLI `pack.rs::import`).
// Handler boundary: error docs on internal axum handlers add noise without aiding
// callers (matches `world_kb.rs` pattern).
#[allow(clippy::missing_errors_doc)]
pub async fn pack_import(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackImportRequest>,
) -> Result<Json<PackImportResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    let pack_value = serde_json::Value::Object(req.pack);
    let parsed = parse_pack(&pack_value).map_err(|e| NexusApiError::InvalidInput {
        field: "pack".to_string(),
        reason: e.to_string(),
    })?;

    let conflict = conflict_policy_from_request(req.conflict);

    // `include_anchors` is accepted on the wire but currently a no-op: Nexus has
    // no standalone SourceAnchor store yet (see `pack_import::import_pack`).
    let summary = import_pack(
        pool,
        &world_id,
        &creator_id,
        parsed,
        conflict,
        req.include_anchors,
        false,
    )
    .await
    .map_err(|e: PackImportError| NexusApiError::Internal {
        code: "PACK_IMPORT_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(Json(import_summary_to_response(summary)))
}
