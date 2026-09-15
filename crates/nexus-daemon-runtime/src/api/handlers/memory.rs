//! Memory pending review handlers — session-end capture for review pipeline
//! (thin translation over the core Creator memory family, v1.190 P2-T2).
//!
//! All DTOs are the generated `nexus_contracts` types; the pending-review
//! queries, keyset pagination, fragment listing, bounded review drain and the
//! SOUL narrative reflect live in [`nexus_core`]. The handlers keep only
//! active-creator resolution, the retained 403/format envelopes and the
//! in-process per-creator review lock (V1.80 REL-01 host composition).

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::soul_narrative_synthesizer::AcpSoulNarrativeSynthesizer;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::config::read_active_creator_id;
use crate::workspace::WorkspaceState;
use axum::Json;
use axum::extract::{Path, Query, State};
use nexus_contracts::daemon_api::memory::{
    CountPendingReviewsQuery, CountPendingReviewsResponse, DeletePendingReviewQuery,
    DeletePendingReviewResponse, ListMemoryFragmentsQuery, ListMemoryFragmentsResponse,
    ListPendingReviewsQuery, ListPendingReviewsResponse, ReviewRequest, ReviewResponse,
    SoulNarrativeRequest, SoulNarrativeResponse,
};
use tracing::{debug, info};

/// GET /v1/daemon/memory/pending-review?creator_id=...
///
/// Lists all pending reviews for a creator with cursor-based pagination.
pub async fn list_pending_reviews(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListPendingReviewsQuery>,
) -> Result<Json<ListPendingReviewsResponse>, NexusApiError> {
    info!(creator_id = %params.creator_id, "Listing pending reviews");

    // R-V133P4-07: enforce active creator from auth context.
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if params.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "pending_review".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                params.creator_id, active_creator
            ),
        });
    }

    // Validate creator_id format
    if !nexus_creator::local_identity::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    let (core, principal) = resolve_core_principal(&state).await?;
    let limit = resolve_query_limit(params.limit);
    let response = core
        .list_pending_reviews(&principal, params.cursor, limit)
        .await?;
    debug!(count = response.items.len(), "Pending reviews retrieved");
    Ok(Json(response))
}

/// Maximum items per page for the memory list endpoints.
const MAX_LIMIT: usize = 250;

/// Default page size when a list query omits `limit`.
const DEFAULT_QUERY_LIMIT: i64 = 50;

/// Resolve an optional wire `limit` (i64) into a clamped `usize`, applying the
/// memory default (`DEFAULT_QUERY_LIMIT` = 50) and the `1..=MAX_LIMIT` clamp
/// shared by the list and fragments endpoints.
fn resolve_query_limit(raw: Option<i64>) -> usize {
    let clamped = raw
        .unwrap_or(DEFAULT_QUERY_LIMIT)
        .clamp(1, i64::try_from(MAX_LIMIT).unwrap_or(i64::MAX));
    usize::try_from(clamped).unwrap_or(MAX_LIMIT)
}

/// GET /v1/daemon/memory/pending-review/count?creator_id=...
///
/// Returns the count of pending reviews for a creator.
pub async fn count_pending_reviews(
    State(state): State<WorkspaceState>,
    Query(params): Query<CountPendingReviewsQuery>,
) -> Result<Json<CountPendingReviewsResponse>, NexusApiError> {
    info!(creator_id = %params.creator_id, "Counting pending reviews");

    // R-V133P4-07: enforce active creator from auth context.
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if params.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "pending_review".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                params.creator_id, active_creator
            ),
        });
    }

    // Validate creator_id format
    if !nexus_creator::local_identity::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.count_pending_reviews(&principal).await?;
    Ok(Json(response))
}

/// DELETE /v1/daemon/memory/pending-review/{id}?creator_id=...
///
/// Deletes a pending review by its ID, but only if it belongs to the specified creator.
pub async fn delete_pending_review(
    State(state): State<WorkspaceState>,
    Path(pending_id): Path<String>,
    Query(params): Query<DeletePendingReviewQuery>,
) -> Result<Json<DeletePendingReviewResponse>, NexusApiError> {
    info!(
        pending_id = %pending_id,
        creator_id = %params.creator_id,
        "Deleting pending review"
    );

    // R-V133P4-07: enforce active creator from auth context.
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if params.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "pending_review".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                params.creator_id, active_creator
            ),
        });
    }

    // Validate creator_id format
    if !nexus_creator::local_identity::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.delete_pending_review(&principal, pending_id).await?;
    Ok(Json(response))
}

/// `POST /v1/daemon/memory/review`
///
/// Triggers the review pipeline for a creator's pending review queue.
/// Auth: requires active creator from config.toml (R-V133P4-01).
/// Request body `creator_id` must match the active creator, otherwise 403.
pub async fn review(
    State(state): State<WorkspaceState>,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<ReviewResponse>, NexusApiError> {
    // R-V133P4-01: Enforce active creator from config (matches works.rs pattern).
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if req.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "memory_review".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                req.creator_id, active_creator
            ),
        });
    }

    // Validate creator_id format
    if !nexus_creator::local_identity::is_valid_creator_id(&req.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    info!(creator_id = %active_creator, "Reviewing pending memories");

    // V1.80 REL-01: bounded fetch + per-creator serialization + per-call
    // timeout. The per-creator lock is host composition: two overlapping
    // requests for the same creator serialize here so they cannot race the
    // core's bounded fetch; the core's queue-advance transactions remain
    // exactly-one-row so cross-process callers can never duplicate fragments.
    let outcome = {
        let creator_lock = state.memory_review_lock(&active_creator);
        let _guard = creator_lock.lock().await;
        let (core, principal) = resolve_core_principal(&state).await?;
        core.review_memory(&principal, req).await?
    }; // per-creator guard drops here (before the response is returned)

    info!(
        creator_id = %active_creator,
        promoted = outcome.promoted,
        fragmented = outcome.fragmented,
        dropped = outcome.dropped,
        processed = outcome.processed,
        has_more = outcome.has_more,
        "Review completed"
    );

    Ok(Json(outcome))
}

/// `GET /v1/daemon/memory/fragments?creator_id=...&keyword=...&limit=...`
///
/// Lists memory fragments for a creator with optional keyword filter.
/// Returns fragment IDs and summaries for the CLI `creator memory fragments` command.
///
/// Auth: requires active creator from config.toml (R-V133P4-01).
/// Query `creator_id` must match the active creator, otherwise 403.
pub async fn fragments(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListMemoryFragmentsQuery>,
) -> Result<Json<ListMemoryFragmentsResponse>, NexusApiError> {
    // R-V133P4-01: Enforce active creator from config (matches works.rs pattern).
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if params.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "memory_fragments".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                params.creator_id, active_creator
            ),
        });
    }

    // Validate creator_id format
    if !nexus_creator::local_identity::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    info!(
        creator_id = %active_creator,
        keyword = ?params.keyword,
        world_id = ?params.world_id,
        "Listing memory fragments"
    );

    let limit = resolve_query_limit(params.limit);
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .list_memory_fragments(&principal, params.keyword, params.world_id, limit)
        .await?;
    debug!(count = response.fragments.len(), "Fragments retrieved");
    Ok(Json(response))
}

// ─── SOUL narrative reflect (v1.184 P3; core-owned since v1.190 P2-T2) ────

/// `POST /v1/daemon/memory/soul/reflect`
///
/// Reads or regenerates the cached whole-Creator SOUL narrative. The public
/// Creator handler validates the active creator, then delegates to the core
/// reflect family; the wire response shape and state strings are preserved
/// byte-for-byte.
///
/// Auth: requires active creator from config.toml.
pub async fn reflect_soul(
    State(state): State<WorkspaceState>,
    Json(req): Json<SoulNarrativeRequest>,
) -> Result<Json<SoulNarrativeResponse>, NexusApiError> {
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if req.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "soul_narrative".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                req.creator_id, active_creator
            ),
        });
    }

    if !nexus_creator::local_identity::is_valid_creator_id(&req.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    info!(
        creator_id = %active_creator,
        world_id = ?req.world_id,
        force_regenerate = req.force_regenerate,
        "Reflecting on SOUL narrative"
    );

    // The world-ownership gate (retained `soul_narrative` 403) lives in the
    // core reflect command; the provider effect is a lazy factory evaluated
    // by the core only after authorization (and only when synthesis is
    // actually demanded), so a missing registry yields the core's retained
    // 503 after authorization, never before it (no provider state is touched
    // pre-auth).
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .reflect_creator_soul(&principal, req, || {
            state.capability_registry().map(AcpSoulNarrativeSynthesizer::new)
        })
        .await?;
    Ok(Json(response))
}
