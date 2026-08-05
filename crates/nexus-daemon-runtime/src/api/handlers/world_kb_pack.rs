//! World KB pack routes — Narrative Knowledge Pack export/import (V1.152 P0,
//! DF-77).
//!
//! - `POST /v1/daemon/worlds/:world_id/kb/pack/export` — export one World's
//!   lore as a handbook pack (read-only; mirrors CLI `pack export`).
//! - `POST /v1/daemon/worlds/:world_id/kb/pack/import` — placeholder for T4.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::generated::daemon_api::kb::{
    pack_export_request::PackExportRequest, pack_export_response::PackExportResponse,
};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_relationships::list_relationships_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::conversion::{kb_relationship_row_to_spoke, world_kb_to_spoke};
use nexus_spoke_adapter::pack::build_pack;
use std::collections::HashSet;

/// Default `modules.pack.version` when the request omits `pack_version`.
const DEFAULT_PACK_VERSION: &str = "0.1.0";


// ─── Shared guards (mirror `world_kb.rs`) ───────────────────────────────────

fn require_creator(state: &WorkspaceState) -> Result<String, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;
    Ok(creator_id)
}

async fn require_world_owner(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    creator_id: &str,
) -> Result<(), NexusApiError> {
    // SAFETY: SELECT against the known narrative_worlds table schema.
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(NexusApiError::from)?;
    match owner {
        None => Err(NexusApiError::NotFound(format!("world {world_id}"))),
        Some(Some(owner_id)) if owner_id == creator_id => Ok(()),
        Some(Some(_)) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason:
                "active creator does not own this world; cross-author World KB edits are forbidden"
                    .to_string(),
        }),
        Some(None) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "world has no owner_creator_id; cannot authorize World KB edit".to_string(),
        }),
    }
}

async fn resolve_world_title(pool: &sqlx::SqlitePool, world_id: &str) -> Result<String, NexusApiError> {
    // SAFETY: static SELECT against known narrative_worlds table schema.
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
    // SAFETY: static SELECT against known creators table schema.
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
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT source_anchor_json FROM kb_source_anchors WHERE key_block_id = ?              ORDER BY anchor_ordinal ASC",
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
        entries.iter().map(world_kb_to_spoke).collect();

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

/// Placeholder for pack import (T4).
#[allow(clippy::missing_errors_doc)]
pub async fn pack_import(
    State(_state): State<WorkspaceState>,
    Path(_world_id): Path<String>,
) -> Result<Json<serde_json::Value>, NexusApiError> {
    Err(NexusApiError::NotImplemented(
        "POST /v1/daemon/worlds/:world_id/kb/pack/import is not implemented yet (T4)".to_string(),
    ))
}
