//! Thin work-scope local file index HTTP adapters over the guarded core
//! knowledge service (V1.20 Batch 5, T39; scope clarified KCA-003 C2; V1.27
//! H3 scope honesty).
//!
//! The endpoints implement the **CLI local work KB index** — a per-creator,
//! per-workspace file-based index stored under
//! `~/.nexus42/creators/<id>/workspaces/<slug>/kb/`. Index layout rules,
//! scope validation, entry-ID minting, the O(1) entry-location index and the
//! crash-consistent write sequence live in the core (P1-T3).
//!
//! **This is NOT `nexus-kb` (World-scoped narrative KB graph) or
//! `nexus-knowledge` (User-scoped global knowledge).** Only `scope=work` is
//! implemented. See
//! [entity-scope-model.md §5.3](../../../../../.mstar/specs/entity-scope-model.md#53-cli-creator-kb--local-work-scope-file-index)
//! for the canonical scope definitions.
//!
//! # Endpoints
//!
//! - `GET /v1/daemon/kb/entries` — list/search work-scope entries
//! - `POST /v1/daemon/kb/entries` — add work-scope entry
//! - `GET /v1/daemon/kb/entries/{id}` — get single work-scope entry
//! - `DELETE /v1/daemon/kb/entries/{id}` — delete work-scope entry

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::kb::{
    AddKbEntryRequest as CoreAddKbEntryRequest, AddKbEntryResponse, DeleteKbEntryResponse,
    GetKbEntryResponse, ListKbEntriesQuery as CoreListKbEntriesQuery,
};
use nexus_contracts::PaginationInfo;
use serde::{Deserialize, Serialize};

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListKbEntriesQuery {
    pub creator_id: Option<String>,
    pub workspace_slug: Option<String>,
    /// KB scope — only `work` is supported. Non-`work` values return 400.
    /// See entity-scope-model §5.3 for scope definitions.
    #[serde(default)]
    pub scope: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KbEntrySummary {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListKbEntriesResponse {
    pub items: Vec<KbEntrySummary>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Deserialize)]
pub struct AddKbEntryRequest {
    pub creator_id: String,
    pub workspace_slug: Option<String>,
    /// KB scope — only `work` is supported. Non-`work` values return 400.
    /// See entity-scope-model §5.3 for scope definitions.
    #[serde(default)]
    pub scope: Option<String>,
    pub title: Option<String>,
    /// File content as UTF-8 string.
    pub content: Option<String>,
    /// Path to a local file to read content from.
    pub file_path: Option<String>,
}

// ─── Wire adapters ─────────────────────────────────────────────────────────

/// Map a core knowledge fault onto the legacy HTTP classification.
///
/// `InvalidInput` keeps the legacy 400 mapping; `NotFound` resources
/// round-trip verbatim; the foreign-creator entry access rides the
/// `kb_owner:` prefix carrier and re-emits the legacy
/// `Forbidden { resource, reason }` pair; the legacy internal codes
/// (`DIR_CREATE_ERROR`, `JSON_ERROR`, `FILE_WRITE_ERROR`, `FILE_RENAME_ERROR`,
/// `FILE_READ_ERROR`, `FILE_DELETE_ERROR`, `DIR_READ_ERROR`) ride verbatim as
/// `<CODE>: <message>` and are re-emitted with the original code; the core
/// `local_db_err` lowercase `database_error: …` carrier is re-classified as
/// the legacy `DATABASE_ERROR`; every other internal category keeps the
/// shared `CORE_ERROR` shape.
fn kb_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => {
            NexusApiError::InvalidInput { field, reason }
        }
        nexus_core::CoreError::NotFound { resource } => NexusApiError::NotFound(resource),
        nexus_core::CoreError::Forbidden { resource } => {
            let foreign = resource.strip_prefix("kb_owner:").map(str::to_owned);
            NexusApiError::Forbidden {
                resource: foreign.clone().unwrap_or(resource),
                reason: foreign.map_or_else(
                    || "forbidden".to_string(),
                    |_| "entry belongs to a different creator".to_string(),
                ),
            }
        }
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some((code, message))
                if matches!(
                    code,
                    "DIR_CREATE_ERROR"
                        | "JSON_ERROR"
                        | "FILE_WRITE_ERROR"
                        | "FILE_RENAME_ERROR"
                        | "FILE_READ_ERROR"
                        | "FILE_DELETE_ERROR"
                        | "DIR_READ_ERROR"
                ) =>
            {
                NexusApiError::Internal {
                    code: code.to_owned(),
                    message: message.to_owned(),
                }
            }
            Some(("database_error", message)) => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_owned(),
                message: message.to_owned(),
            },
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// Convert a wire list query into the core query shape.
fn to_core_query(query: ListKbEntriesQuery) -> CoreListKbEntriesQuery {
    CoreListKbEntriesQuery {
        creator_id: query.creator_id,
        workspace_slug: query.workspace_slug,
        scope: query.scope,
        q: query.q,
        limit: query
            .limit
            .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
        cursor: query.cursor,
    }
}

/// `GET /v1/daemon/kb/entries` — list/search work-scope entries (T39).
///
/// Returns entries from the local work file index for the given creator/workspace.
/// Only `scope=work` is supported; no World KB or User knowledge access.
pub async fn list_entries(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListKbEntriesQuery>,
) -> Result<Json<ListKbEntriesResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core
        .list_kb_entries(&principal, to_core_query(query))
        .await
        .map_err(kb_error)?;
    Ok(Json(ListKbEntriesResponse {
        items: super::wire_cast(result.items),
        pagination: super::wire_cast(result.pagination),
    }))
}

/// `POST /v1/daemon/kb/entries` — add work-scope entry (T39).
///
/// Adds an entry to the local work file index. Only `scope=work` is supported.
pub async fn add_entry(
    State(state): State<WorkspaceState>,
    Json(req): Json<AddKbEntryRequest>,
) -> Result<Json<AddKbEntryResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let request = CoreAddKbEntryRequest {
        content: req.content,
        creator_id: req.creator_id,
        file_path: req.file_path,
        scope: req.scope,
        title: req.title,
        workspace_slug: req.workspace_slug,
    };
    let response = core
        .add_kb_entry(&principal, request)
        .await
        .map_err(kb_error)?;
    Ok(Json(response))
}

/// `GET /v1/daemon/kb/entries/{id}` — get single work-scope entry (T39).
///
/// Uses the core KB entry index for O(1) lookup (QC3 W-005).
/// Only `scope=work` is supported.
pub async fn get_entry(
    State(state): State<WorkspaceState>,
    Path(entry_id): Path<String>,
) -> Result<Json<GetKbEntryResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .get_kb_entry(&principal, entry_id)
        .await
        .map_err(kb_error)?;
    Ok(Json(response))
}

/// `DELETE /v1/daemon/kb/entries/{id}` — delete work-scope entry (T39).
///
/// Uses the core KB entry index for O(1) lookup (QC3 W-005).
/// Only `scope=work` is supported.
pub async fn delete_entry(
    State(state): State<WorkspaceState>,
    Path(entry_id): Path<String>,
) -> Result<Json<DeleteKbEntryResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .delete_kb_entry(&principal, entry_id)
        .await
        .map_err(kb_error)?;
    Ok(Json(response))
}
