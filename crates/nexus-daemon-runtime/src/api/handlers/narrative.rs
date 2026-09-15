//! Narrative read/write surface handlers (V1.25 → v1.190 P0-T1 core cutover).
//!
//! Thin daemon routes over [`nexus_core::CoreService`]: the World lifecycle
//! business (list/get/create/delete) lives in `nexus-core::worlds`; these
//! handlers keep only auth/DTO/status translation, so HTTP status and
//! response envelopes are unchanged.
//!
//! # Endpoints
//!
//! - `GET /v1/daemon/narrative/worlds` — list all worlds
//! - `GET /v1/daemon/narrative/worlds/{world_id}` — get a single world state
//! - `POST /v1/daemon/worlds` — create a new World (V1.130 P2)
//! - `DELETE /v1/daemon/worlds/{world_id}` — hard-delete a World (V1.129 P2)
//!
//! These are **narrative state** routes, distinct from the work-scope
//! `/v1/daemon/kb/*` file-index routes.

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::daemon_api::{CreateWorldRequest, CreateWorldResponse};
use nexus_narrative::WorldState;
use serde::Serialize;

// ─── Response types ────────────────────────────────────────────────────────

/// `GET /v1/daemon/narrative/worlds` response.
#[derive(Debug, Serialize)]
pub struct ListWorldsResponse {
    pub worlds: Vec<WorldState>,
}

/// `GET /v1/daemon/narrative/worlds/{world_id}` response.
#[derive(Debug, Serialize)]
pub struct GetWorldResponse {
    pub world: WorldState,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/daemon/narrative/worlds` — list all worlds.
///
/// Returns worlds from the core workspace read. Empty list when no worlds
/// have been seeded into the database.
pub async fn list_worlds(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListWorldsResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let worlds = core
        .list_worlds(&principal)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(ListWorldsResponse { worlds }))
}

/// `GET /v1/daemon/narrative/worlds/{world_id}` — get a single world state.
///
/// Returns 404 for an unknown world ID. Returns the projected world
/// state for a known world from the core workspace read.
pub async fn get_world(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<GetWorldResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let world = core
        .get_world(&principal, world_id)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(GetWorldResponse { world }))
}

/// `POST /v1/daemon/worlds` — create a new World (V1.130 P2).
///
/// The daemon resolves the caller through the stored principal; clients never
/// send ownership. The title is validated (1-200 chars after trim), an ASCII
/// kebab slug is derived, and the world is persisted via the shared
/// `nexus_local_db::narrative_write` repository from the core service.
///
/// # Errors
///
/// - `400 BadRequest` (`invalid_title`) if title is empty or exceeds 200
///   chars after trim.
/// - `401 AuthRequired` if no active creator is configured.
/// - `500 Internal` on database error.
pub async fn create_world(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateWorldRequest>,
) -> Result<(StatusCode, Json<CreateWorldResponse>), NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .create_world(&principal, req)
        .await
        // Status translation: the retained wire body for an invalid title is
        // the `invalid_title` 400 (the core rule raises the neutral
        // `InvalidInput` carrier).
        .map_err(|e| match e {
            nexus_core::CoreError::InvalidInput { reason, .. } => NexusApiError::BadRequest {
                code: "invalid_title".to_string(),
                message: reason,
            },
            other => NexusApiError::from(other),
        })?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `DELETE /v1/daemon/worlds/{world_id}` — hard-delete a World (V1.129 P2).
///
/// Per architect lock (Seat 2, 2026-07-21 — R-V1126P0-T2-001): **hard delete**,
/// not soft. Confirm dialog in the web UI is the safety net. The core service
/// owns the binding guard, the manual cascade (`kb_extract_jobs`,
/// `works.world_id = NULL` for the owner's Works) and the World `DELETE` in
/// one transaction; FKs cascade timelines, KB blocks (+ anchors) and
/// relationships.
///
/// # Errors
///
/// - `401 AuthRequired` if no active creator is configured.
/// - `404 NotFound` if the world id is unknown or not owned by the caller.
/// - `409 Conflict` (`world_has_actor_bindings`) if any Character binding remains.
/// - `500 Internal` on database error.
pub async fn delete_world(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<StatusCode, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    core.delete_world(&principal, world_id)
        .await
        // Status translation: the retained wire body for a delete blocked by
        // Character bindings is the `409 world_has_actor_bindings` coded
        // conflict (the core raises the marker documented on
        // `nexus_core::DELETE_WORLD_BLOCKED_BY_BINDINGS`).
        .map_err(|e| match e {
            nexus_core::CoreError::Forbidden { resource }
                if resource == nexus_core::DELETE_WORLD_BLOCKED_BY_BINDINGS =>
            {
                let code = nexus_local_db::ActorContractConflict::WorldHasActorBindings;
                NexusApiError::ConflictCoded {
                    code: code.as_str().to_string(),
                    message: code.message().to_string(),
                }
            }
            other => NexusApiError::from(other),
        })?;
    Ok(StatusCode::NO_CONTENT)
}
