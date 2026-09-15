//! Character SOUL/Memory Daemon API handlers (v1.184 P3 Task 3).
//!
//! Thin translation over the core bearer-isolated memory family (v1.190
//! P2-T2): pending-review capture/list/count/delete, fragment listing and
//! revision-checked promotion, and the SOUL narrative reflect all live in
//! [`nexus_core`]; the handlers keep only auth resolution, wire parsing and
//! status/envelope translation. The active Creator is the only trusted owner;
//! request bodies never carry `owner_creator_id`. Every mutating route holds
//! the core per-Character activity lease inside the core call (a foreign or
//! missing Character rejects before any DB row, file write, or synthesis;
//! retained reads permit an archived Character's rows while every mutation
//! admits the fence — an archived Character is `409 character_inactive`).

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::soul_narrative_synthesizer::AcpSoulNarrativeSynthesizer;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::api::pagination::decode_offset_cursor;
use crate::workspace::WorkspaceState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_response::CaptureCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::count_character_pending_reviews_query::CountCharacterPendingReviewsQuery;
use nexus_contracts::daemon_api::characters::memory::count_character_pending_reviews_response::CountCharacterPendingReviewsResponse;
use nexus_contracts::daemon_api::characters::memory::delete_character_pending_review_response::DeleteCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::list_character_memory_fragments_query::ListCharacterMemoryFragmentsQuery;
use nexus_contracts::daemon_api::characters::memory::list_character_memory_fragments_response::ListCharacterMemoryFragmentsResponse;
use nexus_contracts::daemon_api::characters::memory::list_character_pending_reviews_query::ListCharacterPendingReviewsQuery;
use nexus_contracts::daemon_api::characters::memory::list_character_pending_reviews_response::ListCharacterPendingReviewsResponse;
use nexus_contracts::daemon_api::characters::memory::promote_character_fragment_request::PromoteCharacterFragmentRequest;
use nexus_contracts::daemon_api::characters::memory::promote_character_fragment_response::PromoteCharacterFragmentResponse;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_response::ReviewCharacterMemoryResponse;
use nexus_contracts::daemon_api::characters::soul::character_soul_narrative_request::CharacterSoulNarrativeRequest;
use nexus_contracts::daemon_api::characters::soul::character_soul_narrative_response::CharacterSoulNarrativeResponse;
use serde::de::DeserializeOwned;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

fn parse_canonical_json<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, NexusApiError> {
    serde_json::from_slice(bytes).map_err(|err| NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    })
}

fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => {
            u32::try_from(n).map_err(|_| NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "limit is out of range".into(),
            })
        }
        Some(_) => Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: format!("limit must be between 1 and {MAX_LIMIT}"),
        }),
    }
}

fn optional_str(value: Option<&impl std::ops::Deref<Target = String>>) -> Option<&str> {
    value.map(|s| s.as_str())
}

/// `POST /v1/daemon/characters/{character_id}/memory/pending-review`
pub async fn capture_pending_review(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CaptureCharacterPendingReviewResponse>, NexusApiError> {
    let req: CaptureCharacterPendingReviewRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .capture_character_pending_review(&principal, character_id, req)
        .await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}/memory/pending-review`
pub async fn list_pending_reviews(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterPendingReviewsQuery>,
) -> Result<Json<ListCharacterPendingReviewsResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let binding_id = optional_str(query.binding_id.as_ref()).map(str::to_string);
    let limit = resolve_limit(query.limit)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let response = core
        .list_character_pending_reviews(&principal, character_id, binding_id, limit, offset)
        .await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}/memory/pending-review/count`
pub async fn count_pending_reviews(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<CountCharacterPendingReviewsQuery>,
) -> Result<Json<CountCharacterPendingReviewsResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let binding_id = optional_str(query.binding_id.as_ref()).map(str::to_string);
    let response = core
        .count_character_pending_reviews(&principal, character_id, binding_id)
        .await?;
    Ok(Json(response))
}

/// `DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}`
pub async fn delete_pending_review(
    State(state): State<WorkspaceState>,
    Path((character_id, pending_id)): Path<(String, String)>,
) -> Result<Json<DeleteCharacterPendingReviewResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .delete_character_pending_review(&principal, character_id, pending_id)
        .await?;
    Ok(Json(response))
}

/// `POST /v1/daemon/characters/{character_id}/memory/review`
pub async fn review(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<ReviewCharacterMemoryResponse>, NexusApiError> {
    let req: ReviewCharacterMemoryRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .review_character_memory(&principal, character_id, req)
        .await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}/memory/fragments`
pub async fn list_fragments(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterMemoryFragmentsQuery>,
) -> Result<Json<ListCharacterMemoryFragmentsResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let binding_id = optional_str(query.binding_id.as_ref()).map(str::to_string);
    let limit = resolve_limit(query.limit)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let response = core
        .list_character_memory_fragments(&principal, character_id, binding_id, limit, offset)
        .await?;
    Ok(Json(response))
}

/// `POST /v1/daemon/characters/{character_id}/memory/fragments/{fragment_id}:promote`
pub async fn promote_fragment(
    State(state): State<WorkspaceState>,
    Path((character_id, segment)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<PromoteCharacterFragmentResponse>, NexusApiError> {
    // matchit 0.7 rejects `:fragment_id:promote`; strip the `:promote` suffix
    // (mirrors `cancel_operation`).
    let fragment_id = segment
        .strip_suffix(":promote")
        .ok_or_else(|| {
            NexusApiError::NotFound(format!(
                "Character memory fragment route '{segment}' not found"
            ))
        })?
        .to_string();
    let req: PromoteCharacterFragmentRequest = parse_canonical_json(&body)?;
    let expected_revision =
        i64::try_from(req.expected_revision).map_err(|_| NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "expected_revision is out of range".into(),
        })?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .promote_character_fragment(&principal, character_id, fragment_id, expected_revision)
        .await?;
    Ok(Json(response))
}

/// `POST /v1/daemon/characters/{character_id}/soul/reflect`
pub async fn reflect_soul(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CharacterSoulNarrativeResponse>, NexusApiError> {
    let req: CharacterSoulNarrativeRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    // The provider effect is resolved only as an argument to the already
    // core-authorized call: ownership/admission errors (404/403) always win
    // over a missing registry, and the core's own retained 503
    // ("capability registry not available") fires only after authorization
    // succeeds. No provider state is touched before auth.
    let synthesizer = state.capability_registry().map(AcpSoulNarrativeSynthesizer::new);
    let response = core
        .reflect_character_soul(&principal, character_id, req, synthesizer.as_ref())
        .await?;
    Ok(Json(response))
}
