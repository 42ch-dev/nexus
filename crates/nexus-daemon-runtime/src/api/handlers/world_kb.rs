//! Canvas World KB Daemon API handlers (V1.73 P0 Track A → v1.190 P0-T1).
//!
//! World KB routes under `/v1/daemon/worlds/{world_id}/kb/*`, exposing the
//! World-scoped `KnowledgeEntryRecord` graph + promotion state machine
//! (entity-scope-model §5.5) to the canvas. All business — per-row OCC on
//! `kb_key_blocks.revision` / `kb_extract_jobs.version`, ownership, canonical
//! spoke promote/relate, durable `core_changes` — lives in
//! `nexus-core::world_kb`; these handlers keep only auth/DTO/status
//! translation (per-row OCC retained per the architect Phase 2b lock — no new
//! migration).
//!
//! # Endpoints
//!
//! - `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — edit an entity
//!   (`title/body/aliases/block_type`) with per-row OCC.
//! - `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` —
//!   adopt/reject/merge a pending candidate.
//! - `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` — add/update/
//!   remove a typed relationship.
//! - `GET  /v1/daemon/worlds/{world_id}/kb/graph` — entity graph projection.
//! - `GET  /v1/daemon/worlds/{world_id}/kb/candidates` — pending candidates.
//! - `GET  /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state` —
//!   computable entity `body.state` read.
//!
//! # Conflict model
//!
//! Conflict (409 `WorldKbConflictError`) fires per-entity on version
//! mismatch only. Domain-rule violations return 422
//! `WorldKbValidationError`. Stale versions short-circuit before any write.

#![allow(clippy::missing_errors_doc)]

use super::wire_cast;
use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    WorldKbCandidatesResponse, WorldKbGraphResponse, WorldKbKeyBlockStateResponse,
    WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbPatchRelationshipRequest,
    WorldKbPatchRelationshipResponse, WorldKbPromoteCandidateRequest,
    WorldKbPromoteCandidateResponse,
};
use nexus_knowledge::world_kb::knowledge_entry::reject_reserved_authoring_keys;
use serde::Deserialize;
use serde_json::Value;

// ─── patch-entity ───────────────────────────────────────────────────────────

/// Raw authoring scopes of the entity patch that can carry a reserved
/// governance key (durable §§3, 5, 7): the request root, its `patch` object,
/// and a raw extension document under either — the `extensions.nexus`
/// namespace included, whose legacy `creator_only` member the cutover removed
/// rather than passing through as an unknown key.
const RESERVED_KEY_SCOPES: [&str; 6] = [
    "",
    "/patch",
    "/extensions",
    "/extensions/nexus",
    "/patch/extensions",
    "/patch/extensions/nexus",
];

fn invalid_input(message: String) -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message,
    }
}

/// Parse the raw entity-patch body, refusing reserved authoring keys by
/// **presence** — `false` included — at every raw scope before
/// deserialization can discard them as unknown members (durable §§3, 5, 7).
/// The retired World-only `creator_only` key keeps its stable
/// `legacy_creator_only_unsupported` reason; a client-authored
/// `holder_entry_id`/`disclosure` is `reserved_governance_key`.
fn parse_patch_entity_request(body: &Bytes) -> Result<WorldKbPatchEntityRequest, NexusApiError> {
    let value: Value = serde_json::from_slice(body).map_err(|err| invalid_input(err.to_string()))?;
    for pointer in RESERVED_KEY_SCOPES {
        let scope = value.pointer(pointer).unwrap_or(&Value::Null);
        if let Err(rejected) = reject_reserved_authoring_keys(scope) {
            return Err(invalid_input(format!(
                "{}: {} is not accepted on a World KB entity patch; use `audience` with kind \
                 shared, author-only or character-private",
                rejected.reason(),
                rejected.key()
            )));
        }
    }
    serde_json::from_value(value).map_err(|err| invalid_input(err.to_string()))
}

/// `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — entity-level patch.
pub async fn patch_entity(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    body: Bytes,
) -> Result<Json<WorldKbPatchEntityResponse>, NexusApiError> {
    let req = parse_patch_entity_request(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .patch_world_kb_entity(&principal, world_id, req)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}

// ─── promote-candidate ──────────────────────────────────────────────────────

/// `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge.
pub async fn promote_candidate(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPromoteCandidateRequest>,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .promote_world_kb_candidate(&principal, world_id, req)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}

// ─── patch-relationship ─────────────────────────────────────────────────────

/// `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` — add/update/remove a
/// typed relationship between two World KB entities.
pub async fn patch_relationship(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPatchRelationshipRequest>,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .patch_world_kb_relationship(&principal, world_id, req)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}

// ─── read endpoints ─────────────────────────────────────────────────────────

/// `GET /v1/daemon/worlds/{world_id}/kb/graph` — entity graph projection.
///
/// V1.76: defaults to excluding `needs_review = 1` (extraction-suggested)
/// relationships from the graph. Pass `?include_suggested=true` to surface
/// them (rendered as dashed edges by the client). Existing data is unaffected
/// — all rows default to `needs_review = 0` (migration
/// `202606300001_kb_relationships_needs_review.sql`).
pub async fn get_graph(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<WorldKbGraphResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .world_kb_graph(
            &principal,
            world_id,
            query.include_suggested.unwrap_or(false),
        )
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}

/// `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state` —
/// computable `KnowledgeEntryRecord` state read.
///
/// V1.114 P2: dedicated read surface for `body.state` of computable `KnowledgeEntry` rows.
/// Returns `state` when `body.computable` is true; `state: null` and
/// `is_computable: false` otherwise. `version` mirrors the per-row OCC
/// revision so callers can use the same OCC pattern as the graph/patch flows.
pub async fn get_key_block_state(
    State(state): State<WorkspaceState>,
    Path((world_id, key_block_id)): Path<(String, String)>,
) -> Result<Json<WorldKbKeyBlockStateResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .world_kb_key_block_state(&principal, world_id, key_block_id)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}

/// Query params for the graph endpoint (V1.76).
#[derive(Debug, Deserialize)]
pub struct GraphQuery {
    /// When `true`, include `needs_review = 1` (extraction-suggested)
    /// relationships in the graph projection. Defaults to `false` so the
    /// confirmed graph is not flooded by co-occurrence suggestions.
    pub include_suggested: Option<bool>,
}

/// Query parameters for the candidates endpoint.
///
/// `limit` caps the page size; `cursor` is the opaque `kbp:`-prefixed keyset
/// cursor returned by a previous page.
#[derive(Debug, Deserialize)]
pub struct CandidatesQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// `GET /v1/daemon/worlds/{world_id}/kb/candidates` — pending candidates via
/// the core service, with keyset pagination over an opaque `kbp:` cursor.
pub async fn get_candidates(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(query): Query<CandidatesQuery>,
) -> Result<Json<WorldKbCandidatesResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .world_kb_candidates(&principal, world_id, query.limit, query.cursor.clone())
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(wire_cast(response)))
}
