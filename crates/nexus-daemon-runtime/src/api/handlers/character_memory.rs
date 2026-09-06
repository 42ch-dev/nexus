//! Character SOUL/Memory Daemon API handlers (v1.184 P3 Task 3).
//!
//! Thin authorization + wire-shaping layer over the dedicated `character_*`
//! repositories and the shared bearer-parameterized memory pipeline. The
//! active Creator is the only trusted owner; request bodies never carry
//! `owner_creator_id`. Every mutating entrypoint is sealed behind
//! [`BearerPipelineCtx::character`] (format + owner + active lifecycle) so a
//! foreign/missing/inactive Character rejects before any DB row, file write,
//! or synthesis. Binding-local provenance requires the exact active binding of
//! the path Character in an owned active World.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::memory_pipeline::{
    process_bearer_review_batch, reflect_bearer_soul, BearerPipelineCtx, ReflectOutcome,
    REVIEW_BATCH_LIMIT,
};
use crate::api::handlers::soul_narrative_synthesizer::AcpSoulNarrativeSynthesizer;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::api::pagination::{decode_offset_cursor, offset_page_meta};
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_response::CaptureCharacterPendingReviewResponse;
use nexus_contracts::daemon_api::characters::memory::character_memory_fragment_info::CharacterMemoryFragmentInfo;
use nexus_contracts::daemon_api::characters::memory::character_pending_review_info::CharacterPendingReviewInfo;
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
use nexus_creator_memory::review::PendingReviewInput;
use serde::de::DeserializeOwned;
use serde::Serialize;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
const MAX_DIGEST_BYTES: usize = 64 * 1024;
const REVIEW_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn wire_err(err: impl std::fmt::Display) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_MEMORY_WIRE_INVALID".into(),
        message: err.to_string(),
    }
}

fn map_wire<T: DeserializeOwned>(value: impl Serialize) -> Result<T, NexusApiError> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

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

fn pagination_info(limit: u32, has_more: bool, next_cursor: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "limit": limit,
        "has_more": has_more,
        "next_cursor": next_cursor,
    })
}

/// Resolve the path Character owner, returning 404 for missing/foreign,
/// then 403 for inactive via the sealed `BearerPipelineCtx::character`.
async fn require_character_ctx<'a>(
    state: &WorkspaceState,
    owner_creator_id: &'a str,
    character_id: &'a str,
    scope_id: Option<&'a str>,
) -> Result<BearerPipelineCtx<'a>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let owned = nexus_local_db::get_character(pool, owner_creator_id, character_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
    if owned.status != "active" {
        return Err(NexusApiError::Forbidden {
            resource: "character_memory".into(),
            reason: format!(
                "character '{character_id}' is not active (status '{}'); only active Characters may access memory",
                owned.status
            ),
        });
    }
    BearerPipelineCtx::character(pool, owner_creator_id, character_id, scope_id).await
}

/// `POST /v1/daemon/characters/{character_id}/memory/pending-review`
pub async fn capture_pending_review(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CaptureCharacterPendingReviewResponse>, NexusApiError> {
    let req: CaptureCharacterPendingReviewRequest = parse_canonical_json(&body)?;
    let creator = require_creator(&state)?;
    let binding_id = optional_str(req.binding_id.as_ref());
    let _ctx = require_character_ctx(&state, &creator, &character_id, binding_id).await?;

    let pending_id = req.pending_id.as_str().to_string();
    let session_id = req.session_id.as_str().to_string();
    let task_kind = optional_str(req.task_kind.as_ref())
        .unwrap_or("unknown")
        .to_string();
    let raw_digest = req.raw_digest.as_str().to_string();
    let created_at = req
        .created_at
        .map_or_else(|| chrono::Utc::now().to_rfc3339(), |t| t.to_rfc3339());

    validate_capture_input(&req, &raw_digest)?;

    let record = nexus_local_db::CharacterPendingReviewRecord {
        pending_id: pending_id.clone(),
        session_id,
        character_id: character_id.clone(),
        actor_world_binding_id: binding_id.map(std::string::ToString::to_string),
        task_kind,
        raw_digest,
        created_at,
    };
    nexus_local_db::create_character_pending_review(state.pool_or_uninit()?, &creator, &record)
        .await?;

    Ok(Json(map_wire::<CaptureCharacterPendingReviewResponse>(
        serde_json::json!({
            "success": true,
            "pending_id": pending_id,
        }),
    )?))
}

fn validate_capture_input(
    req: &CaptureCharacterPendingReviewRequest,
    raw_digest: &str,
) -> Result<(), NexusApiError> {
    let pending_id = req.pending_id.as_str();
    if pending_id.is_empty() || pending_id.len() > 128 {
        return Err(NexusApiError::InvalidInput {
            field: "pending_id".into(),
            reason: "pending_id must be between 1 and 128 characters".into(),
        });
    }
    let session_id = req.session_id.as_str();
    if session_id.is_empty() || session_id.len() > 128 {
        return Err(NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: "session_id must be between 1 and 128 characters".into(),
        });
    }
    if raw_digest.is_empty() || raw_digest.len() > MAX_DIGEST_BYTES {
        return Err(NexusApiError::InvalidInput {
            field: "raw_digest".into(),
            reason: format!("raw_digest must be between 1 and {MAX_DIGEST_BYTES} bytes"),
        });
    }
    if let Some(kind) = optional_str(req.task_kind.as_ref()) {
        if kind.len() > 64 {
            return Err(NexusApiError::InvalidInput {
                field: "task_kind".into(),
                reason: "task_kind must be at most 64 characters".into(),
            });
        }
    }
    Ok(())
}

/// `GET /v1/daemon/characters/{character_id}/memory/pending-review`
pub async fn list_pending_reviews(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterPendingReviewsQuery>,
) -> Result<Json<ListCharacterPendingReviewsResponse>, NexusApiError> {
    let creator = require_creator(&state)?;
    let binding_id = optional_str(query.binding_id.as_ref());
    require_character_ctx(&state, &creator, &character_id, binding_id).await?;

    let limit = resolve_limit(query.limit)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let fetch_limit = i64::from(limit) + 1;
    let rows = nexus_local_db::list_character_pending_reviews(
        state.pool_or_uninit()?,
        &creator,
        &character_id,
        binding_id,
        fetch_limit,
        i64::from(offset),
    )
    .await?;
    let (next_cursor, has_more) = offset_page_meta(rows.len(), limit, offset);
    let items: Vec<CharacterPendingReviewInfo> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            map_wire::<CharacterPendingReviewInfo>(serde_json::json!({
                "pending_id": r.pending_id,
                "session_id": r.session_id,
                "character_id": r.character_id,
                "binding_id": r.actor_world_binding_id,
                "task_kind": r.task_kind,
                "raw_digest": r.raw_digest,
                "created_at": r.created_at,
            }))
        })
        .collect::<Result<_, _>>()?;
    Ok(Json(map_wire::<ListCharacterPendingReviewsResponse>(
        serde_json::json!({
            "items": items,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }),
    )?))
}

/// `GET /v1/daemon/characters/{character_id}/memory/pending-review/count`
pub async fn count_pending_reviews(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<CountCharacterPendingReviewsQuery>,
) -> Result<Json<CountCharacterPendingReviewsResponse>, NexusApiError> {
    let creator = require_creator(&state)?;
    let binding_id = optional_str(query.binding_id.as_ref());
    require_character_ctx(&state, &creator, &character_id, binding_id).await?;
    let count = nexus_local_db::count_character_pending_reviews(
        state.pool_or_uninit()?,
        &creator,
        &character_id,
        binding_id,
    )
    .await?;
    Ok(Json(map_wire::<CountCharacterPendingReviewsResponse>(
        serde_json::json!({
            "count": i64::try_from(count).unwrap_or(i64::MAX),
        }),
    )?))
}

/// `DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}`
pub async fn delete_pending_review(
    State(state): State<WorkspaceState>,
    Path((character_id, pending_id)): Path<(String, String)>,
) -> Result<Json<DeleteCharacterPendingReviewResponse>, NexusApiError> {
    let creator = require_creator(&state)?;
    require_character_ctx(&state, &creator, &character_id, None).await?;
    let deleted = nexus_local_db::delete_character_pending_review(
        state.pool_or_uninit()?,
        &creator,
        &character_id,
        &pending_id,
    )
    .await?;
    if !deleted {
        return Err(NexusApiError::NotFound(format!(
            "pending review '{pending_id}'"
        )));
    }
    Ok(Json(map_wire::<DeleteCharacterPendingReviewResponse>(
        serde_json::json!({
            "success": true,
            "pending_id": pending_id,
        }),
    )?))
}

/// `POST /v1/daemon/characters/{character_id}/memory/review`
pub async fn review(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<ReviewCharacterMemoryResponse>, NexusApiError> {
    let req: ReviewCharacterMemoryRequest = parse_canonical_json(&body)?;
    let creator = require_creator(&state)?;
    let binding_id = optional_str(req.binding_id.as_ref());
    require_character_ctx(&state, &creator, &character_id, binding_id).await?;

    let pool = state.pool_or_uninit()?.clone();
    let nexus_home = state.nexus_home().to_owned();
    // Fetch batch_limit + 1 so the extra row proves more rows exist; truncate
    // the processing slice back to the documented batch bound (mirrors the
    // Creator memory review handler — no off-by-one on has_more).
    let fetch_limit = REVIEW_BATCH_LIMIT + 1;
    let mut rows = nexus_local_db::list_character_pending_reviews(
        &pool,
        &creator,
        &character_id,
        binding_id,
        fetch_limit,
        0,
    )
    .await?;
    let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
    let more_in_db = rows.len() > batch_limit;
    if more_in_db {
        rows.truncate(batch_limit);
    }
    let processing_slice = rows.len();
    let inputs: Vec<PendingReviewInput> = rows
        .into_iter()
        .map(|r| PendingReviewInput {
            pending_id: r.pending_id,
            session_id: r.session_id,
            bearer_id: r.character_id,
            scope_id: r.actor_world_binding_id,
            task_kind: r.task_kind,
            raw_digest: r.raw_digest,
            created_at: r.created_at,
        })
        .collect();
    let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
    let ctx = BearerPipelineCtx::character(&pool, &creator, &character_id, binding_id).await?;
    let mut outcome =
        process_bearer_review_batch(&inputs, &nexus_home, &ctx, &pool, deadline).await;
    let deadline_stopped = outcome.processed < processing_slice;
    outcome.has_more = more_in_db || deadline_stopped || outcome.any_row_remained_pending;
    outcome.more_in_db = more_in_db;
    outcome.processing_slice = processing_slice;

    Ok(Json(map_wire::<ReviewCharacterMemoryResponse>(
        serde_json::json!({
            "promoted": outcome.promoted,
            "fragmented": outcome.fragmented,
            "dropped": outcome.dropped,
            "has_more": outcome.has_more,
            "processed": i64::try_from(outcome.processed).unwrap_or(i64::MAX),
        }),
    )?))
}

/// `GET /v1/daemon/characters/{character_id}/memory/fragments`
pub async fn list_fragments(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterMemoryFragmentsQuery>,
) -> Result<Json<ListCharacterMemoryFragmentsResponse>, NexusApiError> {
    let creator = require_creator(&state)?;
    let binding_id = optional_str(query.binding_id.as_ref());
    require_character_ctx(&state, &creator, &character_id, binding_id).await?;

    let limit = resolve_limit(query.limit)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let fetch_limit = i64::from(limit) + 1;
    let rows = nexus_local_db::list_character_fragments(
        state.pool_or_uninit()?,
        &creator,
        &character_id,
        binding_id,
        fetch_limit,
        i64::from(offset),
    )
    .await?;
    let (next_cursor, has_more) = offset_page_meta(rows.len(), limit, offset);
    let fragments: Vec<CharacterMemoryFragmentInfo> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            map_wire::<CharacterMemoryFragmentInfo>(serde_json::json!({
                "fragment_id": r.fragment_id,
                "session_id": r.session_id,
                "character_id": r.character_id,
                "binding_id": r.actor_world_binding_id,
                "summary": r.summary,
                "keywords": decode_fragment_keywords(&r.keywords),
                "created_at": r.created_at,
                "ttl": r.ttl,
                "revision": r.revision,
            }))
        })
        .collect::<Result<_, _>>()?;
    Ok(Json(map_wire::<ListCharacterMemoryFragmentsResponse>(
        serde_json::json!({
            "fragments": fragments,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }),
    )?))
}

fn decode_fragment_keywords(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
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
    let creator = require_creator(&state)?;
    require_character_ctx(&state, &creator, &character_id, None).await?;
    let expected_revision =
        i64::try_from(req.expected_revision).map_err(|_| NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "expected_revision is out of range".into(),
        })?;
    // The repository commits and returns the authoritative promoted record
    // (same fragment id, cleared binding provenance, bumped revision). Map it
    // directly — no post-commit re-query or panic path.
    let promoted = nexus_local_db::promote_character_fragment_to_shared(
        state.pool_or_uninit()?,
        &creator,
        &character_id,
        &fragment_id,
        expected_revision,
    )
    .await
    .map_err(map_local_db_promote_error)?;
    Ok(Json(map_wire::<PromoteCharacterFragmentResponse>(
        serde_json::json!({
            "fragment": character_fragment_info(&promoted)?,
        }),
    )?))
}

fn map_local_db_promote_error(e: nexus_local_db::LocalDbError) -> NexusApiError {
    match e {
        nexus_local_db::LocalDbError::VersionMismatch { .. } => NexusApiError::ConflictCoded {
            code: "version_mismatch".into(),
            message: e.to_string(),
        },
        other => NexusApiError::from(other),
    }
}

fn character_fragment_info(
    r: &nexus_local_db::CharacterMemoryFragmentRecord,
) -> Result<CharacterMemoryFragmentInfo, NexusApiError> {
    map_wire::<CharacterMemoryFragmentInfo>(serde_json::json!({
        "fragment_id": r.fragment_id,
        "session_id": r.session_id,
        "character_id": r.character_id,
        "binding_id": r.actor_world_binding_id,
        "summary": r.summary,
        "keywords": decode_fragment_keywords(&r.keywords),
        "created_at": r.created_at,
        "ttl": r.ttl,
        "revision": r.revision,
    }))
}

/// `POST /v1/daemon/characters/{character_id}/soul/reflect`
pub async fn reflect_soul(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CharacterSoulNarrativeResponse>, NexusApiError> {
    let req: CharacterSoulNarrativeRequest = parse_canonical_json(&body)?;
    let creator = require_creator(&state)?;
    let binding_id = optional_str(req.binding_id.as_ref());
    require_character_ctx(&state, &creator, &character_id, binding_id).await?;

    let synthesizer = if req.force_regenerate {
        let registry =
            state
                .capability_registry()
                .ok_or_else(|| NexusApiError::ServiceUnavailable {
                    message: "capability registry not available".to_string(),
                })?;
        Some(AcpSoulNarrativeSynthesizer::new(registry))
    } else {
        None
    };

    let pool = state.pool_or_uninit()?;
    let ctx = BearerPipelineCtx::character(pool, &creator, &character_id, binding_id).await?;
    let outcome =
        reflect_bearer_soul(pool, &ctx, req.force_regenerate, synthesizer.as_ref()).await?;
    Ok(Json(map_character_reflect_outcome(&character_id, &outcome)))
}

fn map_character_reflect_outcome(
    character_id: &str,
    o: &ReflectOutcome,
) -> CharacterSoulNarrativeResponse {
    let state_str = match o.state {
        crate::api::handlers::memory_pipeline::ReflectState::InsufficientData => {
            "insufficient_data"
        }
        crate::api::handlers::memory_pipeline::ReflectState::Ungenerated => "ungenerated",
        crate::api::handlers::memory_pipeline::ReflectState::Current => "current",
        crate::api::handlers::memory_pipeline::ReflectState::Stale => "stale",
    };
    map_wire::<CharacterSoulNarrativeResponse>(serde_json::json!({
        "character_id": character_id,
        "state": state_str,
        "narrative": o.narrative,
        "generated_at": o.generated_at,
        "stale": o.stale,
        "fragment_count_at_generation": o.fragment_count_at_generation,
        "max_fragment_created_at_at_generation": o.max_fragment_created_at_at_generation,
        "current_fragment_count": o.current_fragment_count,
        "current_distinct_keyword_count": o.current_distinct_keyword_count,
        "min_fragment_count": crate::api::handlers::memory_pipeline::MIN_SOUL_NARRATIVE_FRAGMENTS,
        "min_distinct_keyword_count": crate::api::handlers::memory_pipeline::MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
    }))
    .expect("canonical character soul narrative wire")
}
