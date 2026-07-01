//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Memory pending review handlers — session-end capture for review pipeline.
//!
//! V1.78 P0 (Batch 1): every request/response/query/item DTO is now the
//! generated `nexus_contracts` type — no hand-written DTOs (daemon-runtime
//! invariant). `PendingReviewInfo` is also the response item type; the
//! generated type cannot carry `sqlx::FromRow` (orphan rule — both
//! `sqlx::FromRow` and `nexus_contracts::PendingReviewInfo` are foreign to
//! this crate, and `nexus-contracts` intentionally does not depend on sqlx),
//! so the SQL projections use `query!` + explicit field mapping instead of
//! `query_as!`. See `fetch_pending_reviews_by_creator`. Wire behavior is
//! unchanged.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
pub use nexus_contracts::{
    CountPendingReviewsQuery, CountPendingReviewsResponse, CreatePendingReviewRequest,
    CreatePendingReviewResponse, DeletePendingReviewQuery, DeletePendingReviewResponse,
    ListMemoryFragmentsQuery, ListMemoryFragmentsResponse, ListPendingReviewsQuery,
    ListPendingReviewsResponse, MemoryFragmentInfo, PaginationInfo, PendingReviewInfo,
    ReviewRequest, ReviewResponse, SoulNarrativeRequest, SoulNarrativeResponse,
};
use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesizer as _;
use tracing::{debug, info};

/// POST /v1/local/memory/pending-review
///
/// Creates a new pending review entry from a session-end capture event.
/// This endpoint is called by the CLI when an ACP session ends.
///
/// ## Idempotency
///
/// Uses `INSERT OR IGNORE` to handle retries gracefully. If a `pending_id` or
/// `session_id` already exists, the insert is silently skipped, returning success.
/// This prevents 500 errors when the CLI retries on network failures.
pub async fn create_pending_review(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreatePendingReviewRequest>,
) -> Result<Json<CreatePendingReviewResponse>, NexusApiError> {
    info!(
        pending_id = %req.pending_id,
        session_id = %req.session_id,
        creator_id = %req.creator_id,
        "Creating pending review entry"
    );

    // R-V133P4-07: enforce active creator from auth context (consistency with
    // review/fragments handlers from R-V133P4-01 fix).
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    if req.creator_id != active_creator {
        return Err(NexusApiError::Forbidden {
            resource: "pending_review".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                req.creator_id, active_creator
            ),
        });
    }

    // Validate input fields (includes creator_id format validation)
    validate_pending_review_input(&req)?;

    // Use defaults for optional fields
    let task_kind = req
        .task_kind
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let created_at = req
        .created_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    // Use INSERT OR IGNORE for idempotent behavior on retries
    let pending_id = req.pending_id.clone();
    let session_id = req.session_id.clone();
    let creator_id = active_creator; // R-V133P4-07: use active creator, not body
    let world_id = &req.world_id;
    let raw_digest = req.raw_digest.clone();

    sqlx::query!(
        "INSERT OR IGNORE INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at
    )
    .execute(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to create pending review: {e}"),
    })?;

    debug!(pending_id = %req.pending_id, "Pending review entry created (or ignored on duplicate)");

    Ok(Json(CreatePendingReviewResponse {
        success: true,
        pending_id: req.pending_id,
    }))
}

/// Validate input fields for `create_pending_review`.
///
/// Returns 400 Bad Request with field-level detail on validation failure.
fn validate_pending_review_input(req: &CreatePendingReviewRequest) -> Result<(), NexusApiError> {
    // pending_id: non-empty, max 128 chars
    if req.pending_id.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "pending_id".into(),
            reason: "pending_id must not be empty".into(),
        });
    }
    if req.pending_id.len() > 128 {
        return Err(NexusApiError::InvalidInput {
            field: "pending_id".into(),
            reason: "pending_id must be at most 128 characters".into(),
        });
    }

    // session_id: non-empty, max 128 chars
    if req.session_id.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: "session_id must not be empty".into(),
        });
    }
    if req.session_id.len() > 128 {
        return Err(NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: "session_id must be at most 128 characters".into(),
        });
    }

    // creator_id: non-empty, must match ctr_<alphanumeric> pattern
    if !nexus_creator::local_identity::is_valid_creator_id(&req.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    // raw_digest: non-empty, max 64KB
    if req.raw_digest.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "raw_digest".into(),
            reason: "raw_digest must not be empty".into(),
        });
    }
    if req.raw_digest.len() > 64 * 1024 {
        return Err(NexusApiError::InvalidInput {
            field: "raw_digest".into(),
            reason: "raw_digest must be at most 64KB".into(),
        });
    }

    // task_kind: if provided, max 64 chars
    if let Some(task_kind) = &req.task_kind {
        if task_kind.len() > 64 {
            return Err(NexusApiError::InvalidInput {
                field: "task_kind".into(),
                reason: "task_kind must be at most 64 characters".into(),
            });
        }
    }

    // world_id: if provided, max 128 chars
    if let Some(world_id) = &req.world_id {
        if world_id.len() > 128 {
            return Err(NexusApiError::InvalidInput {
                field: "world_id".into(),
                reason: "world_id must be at most 128 characters".into(),
            });
        }
    }

    Ok(())
}

/// GET /v1/local/memory/pending-review?creator_id=...
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

    // R-V178P0-QC3-002: push pagination into SQL (keyset on
    // `(created_at DESC, pending_id DESC)` with `LIMIT ? + 1`) so the daemon
    // never materializes the full creator set before applying the cursor/limit.
    // The prior fetch-all + in-Rust `split_off`/`truncate` is preserved at the
    // wire level — see `fetch_pending_reviews_page` for the behavior argument.
    let limit = resolve_query_limit(params.limit);
    let creator_id_filter = active_creator; // R-V133P4-07: use active creator
    let fetch_limit = i64::try_from(limit + 1).unwrap_or(i64::MAX);
    let mut items = fetch_pending_reviews_page(
        state.pool(),
        &creator_id_filter,
        params.cursor.as_deref(),
        fetch_limit,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to list pending reviews: {e}"),
    })?;

    // Determine the next cursor from the over-fetched (`limit + 1`) row. If we
    // fetched strictly more than `limit`, there is at least one more row on the
    // server; the next page's cursor is the last item of the truncated page.
    // This matches the prior `items.len() > limit` branch exactly.
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|i| i.pending_id.clone())
    } else {
        None
    };

    debug!(count = items.len(), "Pending reviews retrieved");

    Ok(Json(ListPendingReviewsResponse {
        items,
        pagination: PaginationInfo {
            limit: i64::try_from(limit).unwrap_or(i64::MAX),
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

/// Maximum items per page for the memory list endpoints.
const MAX_LIMIT: usize = 250;

/// Default page size when a list query omits `limit`.
const DEFAULT_QUERY_LIMIT: i64 = 50;

/// Resolve an optional wire `limit` (i64) into a clamped `usize`, applying the
/// memory default (`DEFAULT_QUERY_LIMIT` = 50) and the `1..=MAX_LIMIT` clamp
/// shared by the list and fragments endpoints. Wire behavior matches the prior
/// hand-written query structs (`#[serde(default = "default_limit")]` +
/// `.clamp(1, MAX_LIMIT)`): absent → 50, otherwise clamped to `1..=250`.
fn resolve_query_limit(raw: Option<i64>) -> usize {
    let clamped = raw
        .unwrap_or(DEFAULT_QUERY_LIMIT)
        .clamp(1, i64::try_from(MAX_LIMIT).unwrap_or(i64::MAX));
    usize::try_from(clamped).unwrap_or(MAX_LIMIT)
}

// (open item #7 bridging) The generated `PendingReviewInfo` cannot derive
// `sqlx::FromRow` — both `sqlx::FromRow` and `nexus_contracts::PendingReviewInfo`
// are foreign to this crate, so the orphan rule forbids `impl FromRow for
// PendingReviewInfo` here (and `nexus-contracts` intentionally does not depend
// on sqlx). The bounded helper `fetch_pending_reviews_page` below uses `query!`
// + explicit field mapping instead of `query_as!`; the list and review handlers
// share it.
//
// V1.80 REL-01: the unbounded `fetch_pending_reviews_by_creator` that used to
// live here was removed — the review handler now reuses the bounded
// `fetch_pending_reviews_page` (50 + 1 overfetch). The bounded helper carries
// the same column-set, ordering, and field-mapping convention.

/// Fetch one bounded page of a creator's pending reviews for the list endpoint.
///
/// **R-V178P0-QC3-002 (qc1 W-QC1-002 + qc3 W-QC3-002/W-QC3-003):** replaces the
/// unbounded `fetch_all` + in-Rust `split_off`/`truncate` pagination in
/// [`list_pending_reviews`]. The daemon now fetches at most `limit + 1` rows
/// from the database instead of materializing the full creator set.
///
/// # Keyset + behavior preservation
///
/// The wire cursor is a `pending_id`. Pagination is implemented as a keyset on
/// `(created_at DESC, pending_id DESC)`:
///
/// 1. If a cursor is supplied, its `created_at` is resolved with a point
///    lookup. The page query then returns rows strictly after the cursor's key:
///    `(created_at < cursor_ca) OR (created_at == cursor_ca AND pending_id <
///    cursor_pid)`, ordered `created_at DESC, pending_id DESC`, `LIMIT ?`.
/// 2. If no cursor is supplied (or the cursor row was deleted between pages),
///    the first page is returned with `LIMIT ?`.
///
/// This reproduces the prior `position(cursor)` → `split_off(idx + 1)` →
/// `truncate(limit)` semantics:
///
/// - **Distinct `created_at`** (the overwhelmingly common case): the observable
///   row order is identical to the prior `ORDER BY created_at DESC`, so every
///   page returns the same rows and the same `next_cursor`.
/// - **Equal `created_at` ties:** the prior query ordered by `created_at DESC`
///   only, leaving ties to the database's implementation-defined rowid order. Adding
///   `pending_id DESC` as a tiebreaker makes ties deterministic, which is
///   strictly more correct (the prior nondeterminism was a latent pagination
///   hazard at the tie boundary). No row that previously appeared on page *N*
///   can now appear on page *N-1* or *N+1* in a way that breaks cursor
///   continuity, because the tiebreaker is total and stable.
/// - **Deleted cursor:** the prior code returned the first page when
///   `position()` could not find the cursor (pos `None` → items unchanged).
///   This implementation returns the first page via the no-cursor query when the
///   cursor's `created_at` lookup misses, matching that fallback.
///
/// `fetch_limit` is `page_limit + 1` from the caller; the extra row drives the
/// `has_more` / `next_cursor` decision without a second round-trip (the caller
/// truncates back to `page_limit`).
async fn fetch_pending_reviews_page(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    cursor: Option<&str>,
    fetch_limit: i64,
) -> Result<Vec<PendingReviewInfo>, sqlx::Error> {
    // Resolve the cursor row's `created_at`. Returns `None` if the cursor was
    // deleted (or never existed); the caller then falls through to the
    // no-cursor first-page query, preserving the prior `position() == None`
    // behavior.
    let cursor_created_at: Option<String> = if let Some(cursor_pid) = cursor {
        sqlx::query_scalar!(
            "SELECT created_at FROM memory_pending_review
             WHERE creator_id = ? AND pending_id = ?",
            creator_id,
            cursor_pid
        )
        .fetch_optional(pool)
        .await?
    } else {
        None
    };

    let rows: Vec<PendingReviewInfo> = if let (Some(cursor_pid), Some(cursor_ca)) =
        (cursor, cursor_created_at)
    {
        // Keyset page: rows strictly after `(cursor_ca, cursor_pid)` in
        // `created_at DESC, pending_id DESC` order.
        sqlx::query!(
            r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
             FROM memory_pending_review
             WHERE creator_id = ?
               AND (created_at < ? OR (created_at = ? AND pending_id < ?))
             ORDER BY created_at DESC, pending_id DESC
             LIMIT ?"#,
            creator_id,
            cursor_ca,
            cursor_ca,
            cursor_pid,
            fetch_limit
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PendingReviewInfo {
            pending_id: row.pending_id,
            session_id: row.session_id,
            creator_id: row.creator_id,
            world_id: row.world_id,
            task_kind: row.task_kind,
            raw_digest: row.raw_digest,
            created_at: row.created_at,
        })
        .collect()
    } else {
        // First page (no cursor, or cursor deleted → restart from the top to
        // preserve the prior position()==None behavior).
        sqlx::query!(
            r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
             FROM memory_pending_review
             WHERE creator_id = ?
             ORDER BY created_at DESC, pending_id DESC
             LIMIT ?"#,
            creator_id,
            fetch_limit
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PendingReviewInfo {
            pending_id: row.pending_id,
            session_id: row.session_id,
            creator_id: row.creator_id,
            world_id: row.world_id,
            task_kind: row.task_kind,
            raw_digest: row.raw_digest,
            created_at: row.created_at,
        })
        .collect()
    };

    Ok(rows)
}

/// GET /v1/local/memory/pending-review/count?creator_id=...
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

    let creator_id_filter = active_creator; // R-V133P4-07: use active creator
    let row = sqlx::query_scalar!(
        "SELECT COUNT(*) as \"count!\" FROM memory_pending_review WHERE creator_id = ?",
        creator_id_filter
    )
    .fetch_one(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to count pending reviews: {e}"),
    })?;

    Ok(Json(CountPendingReviewsResponse {
        // `row` is the i64 COUNT(*) result; the generated `count` field is i64.
        count: row,
    }))
}

/// DELETE /v1/local/memory/pending-review/{id}?creator_id=...
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

    // Verify ownership before deletion
    let pid = pending_id.clone();
    let review = sqlx::query!(
        r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE pending_id = ?"#, // sqlx R3: use ? instead of ?1
        pid
    )
    .fetch_optional(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to lookup pending review: {e}"),
    })?
    .map(|row| PendingReviewInfo {
        pending_id: row.pending_id,
        session_id: row.session_id,
        creator_id: row.creator_id,
        world_id: row.world_id,
        task_kind: row.task_kind,
        raw_digest: row.raw_digest,
        created_at: row.created_at,
    });

    match review {
        None => {
            return Err(NexusApiError::NotFound(format!(
                "pending review '{pending_id}' not found"
            )));
        }
        Some(ref r) if r.creator_id != params.creator_id => {
            return Err(NexusApiError::Forbidden {
                resource: "pending_review".into(),
                reason: format!(
                    "pending review '{}' does not belong to creator '{}'",
                    pending_id, params.creator_id
                ),
            });
        }
        _ => {}
    }

    // Proceed with deletion
    let pid = pending_id.clone();
    let affected = sqlx::query!(
        "DELETE FROM memory_pending_review WHERE pending_id = ?",
        pid
    )
    .execute(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to delete pending review: {e}"),
    })?;

    debug_assert!(
        affected.rows_affected() > 0,
        "Expected 1 row deleted after ownership check"
    );

    Ok(Json(DeletePendingReviewResponse {
        success: true,
        pending_id,
    }))
}

// ─── Review + Fragments handlers (V1.33 P4) ────────────────────────────────

/// `POST /v1/local/memory/review`
///
/// Triggers the review pipeline for a creator's pending review queue.
/// Classifies each entry using rule-based heuristics:
/// - **`PromoteToLongTerm`**: high-signal creative content → long-term memory file
/// - **`FragmentOnly`**: informational content → keyword fragment record
/// - **`Drop`**: below threshold → deleted
///
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

    // V1.80 REL-01: bounded fetch + per-creator serialization + per-call timeout.
    //
    // All of fetch, classify, side effects, and best-effort deletes happen
    // inside the per-creator guard scope so two overlapping requests for the
    // same creator cannot fetch/delete the same pending rows (the side effects
    // mint fresh IDs and are not idempotent at the DB). The map mutex is only
    // held briefly inside `memory_review_lock`; this `.await` waits on the
    // creator-scoped lock. Overlapping calls serialize instead of erroring.
    let outcome = {
        let creator_lock = state.memory_review_lock(&active_creator);
        let _guard = creator_lock.lock().await;

        // Bounded fetch: REVIEW_BATCH_LIMIT + 1 overfetch drives `has_more`.
        let fetch_limit = REVIEW_BATCH_LIMIT + 1;
        let mut rows = fetch_pending_reviews_page(state.pool(), &active_creator, None, fetch_limit)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("failed to fetch pending reviews for review: {e}"),
            })?;

        // The over-fetched (51st) row, if present, means more rows remain in the
        // DB beyond this batch. Truncate the processing slice back to the limit.
        let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
        let more_in_db = rows.len() > batch_limit;
        if more_in_db {
            rows.truncate(batch_limit);
        }
        let processing_slice = rows.len();

        let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
        let nexus_home = state.nexus_home().to_owned();
        let pool = state.pool().clone();
        let mut batch =
            process_review_batch(&rows, &nexus_home, &active_creator, &pool, deadline).await;

        // `has_more` is the drain-completion contract: `true` means the queue
        // may not be fully drained and the client should re-request
        // (`apps/web` `useReviewMemory` drains while `has_more === true`). It is
        // `true` when ANY of:
        //   - `more_in_db`: the overfetched (51st) row proves rows exist beyond
        //     this batch;
        //   - `deadline_stopped`: the per-call budget expired before the loop
        //     inspected every fetched row (`processed < processing_slice`);
        //   - `any_row_remained_pending`: a fetched row was inspected but NOT
        //     completed — its action failed or it timed out mid-row, so it was
        //     left pending. This is the key invariant added for W-QC3-001
        //     (R-V180P0-QC3-001): if any row fetched in this batch still awaits
        //     completion, `has_more` MUST be true, otherwise the client could
        //     terminate with a false "Review complete" while a pending row
        //     remains. `processed` alone was insufficient because it advances
        //     for inspected-but-uncompleted rows too.
        let deadline_stopped = batch.processed < processing_slice;
        let has_more = more_in_db || deadline_stopped || batch.any_row_remained_pending;
        batch.has_more = has_more;
        batch.more_in_db = more_in_db;
        batch.processing_slice = processing_slice;
        batch
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

    Ok(Json(ReviewResponse {
        promoted: outcome.promoted,
        fragmented: outcome.fragmented,
        dropped: outcome.dropped,
        has_more: Some(outcome.has_more),
        processed: Some(i64::try_from(outcome.processed).unwrap_or(i64::MAX)),
    }))
}

/// Maximum pending rows inspected per `POST /memory/review` call (V1.80 REL-01).
///
/// Aligns the review-drain batch with the memory list default
/// (`DEFAULT_QUERY_LIMIT = 50`): a 50-row synchronous batch is the smallest
/// policy that preserves the local-only / small-queue threat model while
/// bounding the request duration. Not user-configurable in this slice.
const REVIEW_BATCH_LIMIT: i64 = 50;

/// Per-call server budget for `POST /memory/review` (V1.80 REL-01).
///
/// Implemented as a deadline checked before each row; on expiry the handler
/// returns the partial progress accumulated so far (`has_more = true`) instead
/// of failing the request.
const REVIEW_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Outcome of a bounded review batch, accumulated incrementally so the deadline
/// path can return partial progress.
struct ReviewBatchOutcome {
    promoted: i64,
    fragmented: i64,
    dropped: i64,
    /// Rows inspected (classified + action attempted) so far. This is the
    /// "rows attempted" progress signal — it advances even when a row's action
    /// fails or times out and the row is left pending. Drain *completion* is
    /// decided from `any_row_remained_pending` (see below), NOT from this count
    /// alone, so a non-advancing row can never produce a false "Review
    /// complete" (W-QC3-001 / R-V180P0-QC3-001).
    processed: usize,
    has_more: bool,
    /// True if any fetched row was inspected but NOT completed — it timed out
    /// mid-row, or its side effect failed, leaving the row pending (not
    /// deleted). When true, the caller MUST report `has_more = true` so the
    /// client re-requests and the queue keeps draining. This is the
    /// queue-advancement signal that `processed` is not: a row counts as
    /// `processed` whether or not it was actually removed from the pending
    /// queue. Key invariant: if any row fetched in this batch still awaits
    /// completion, the drain is not done.
    any_row_remained_pending: bool,
    // Diagnostics for logging; not serialized.
    more_in_db: bool,
    processing_slice: usize,
}

impl ReviewBatchOutcome {
    const fn new() -> Self {
        Self {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            any_row_remained_pending: false,
            more_in_db: false,
            processing_slice: 0,
        }
    }
}

/// Process a bounded slice of the review queue for a creator's pending entries.
///
/// This is the deadline-aware evolution of the V1.33 `process_review_queue`:
/// before each row the deadline is checked, and the per-row async side effect
/// is bounded by `timeout_at(deadline, …)` so a single slow promote cannot
/// overrun the whole request. Classification (promote/fragment/drop) and the
/// post-success pending-row delete are unchanged. On budget expiry the loop
/// stops and the caller reports `has_more = true` with the counters accumulated
/// so far — completed side effects/deletes are NOT rolled back.
async fn process_review_batch(
    rows: &[PendingReviewInfo],
    nexus_home: &std::path::Path,
    creator_id: &str,
    pool: &sqlx::SqlitePool,
    deadline: tokio::time::Instant,
) -> ReviewBatchOutcome {
    let mut outcome = ReviewBatchOutcome::new();

    for row in rows {
        // Check the deadline before each row. If the budget is exhausted, stop
        // and let the caller report partial progress.
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        let input = nexus_creator_memory::review::PendingReviewInput {
            pending_id: row.pending_id.clone(),
            session_id: row.session_id.clone(),
            creator_id: row.creator_id.clone(),
            world_id: row.world_id.clone(),
            task_kind: row.task_kind.clone(),
            raw_digest: row.raw_digest.clone(),
            created_at: row.created_at.clone(),
        };

        let decision = nexus_creator_memory::review::classify_pending_review(&input);

        // Wrap the per-row work so a slow side effect cannot overrun the budget.
        // On timeout the row is left in place (not deleted) and the loop stops;
        // the caller reports `has_more = true` and the row is retried next call.
        let row_result = tokio::time::timeout_at(
            deadline,
            process_single_review_row(&decision, &input, nexus_home, creator_id, pool),
        )
        .await;

        // `processed` counts rows *inspected* (attempted): it advances for both
        // successful and timed-out/failed rows, so the client's progress signal
        // reflects work the server actually performed. It is NOT a drain-
        // completion signal — queue advancement (rows actually completed and
        // deleted) is tracked separately via `any_row_remained_pending`, which
        // the caller folds into `has_more` so a non-advancing row never yields a
        // false "Review complete" (W-QC3-001 / R-V180P0-QC3-001).
        outcome.processed += 1;

        match row_result {
            Ok(action_counts) => {
                outcome.promoted += action_counts.promoted;
                outcome.fragmented += action_counts.fragmented;
                outcome.dropped += action_counts.dropped;
                // A row whose action produced zero counts was NOT completed: its
                // side effect failed (promote/fragment error) or it hit an
                // unimplemented action, so it was not deleted and remains
                // pending. Any such row must keep `has_more` true so the client
                // re-requests instead of terminating with a false "complete".
                if action_counts.promoted + action_counts.fragmented + action_counts.dropped == 0 {
                    outcome.any_row_remained_pending = true;
                }
            }
            Err(_elapsed) => {
                // Deadline expired mid-row. The row was inspected but its side
                // effect was abandoned, so it remains pending (not deleted).
                // Stop processing further rows; the caller reports `has_more`
                // and the row is retried next call.
                outcome.any_row_remained_pending = true;
                tracing::info!(
                    creator_id = %creator_id,
                    pending_id = %row.pending_id,
                    processed = outcome.processed,
                    "Review deadline reached mid-batch; returning partial progress"
                );
                break;
            }
        }
    }

    outcome
}

/// Counts produced by a single row's classify+action. Each field is 0 or 1.
struct RowActionCounts {
    promoted: i64,
    fragmented: i64,
    dropped: i64,
}

/// Classify one pending row, perform the action (promote/fragment/drop), and
/// delete the pending row on success. Behavior matches the V1.33
/// `process_review_queue` body; extracted so the deadline loop can bound it.
async fn process_single_review_row(
    decision: &nexus_creator_memory::review::ReviewDecision,
    input: &nexus_creator_memory::review::PendingReviewInput,
    nexus_home: &std::path::Path,
    creator_id: &str,
    pool: &sqlx::SqlitePool,
) -> RowActionCounts {
    let mut counts = RowActionCounts {
        promoted: 0,
        fragmented: 0,
        dropped: 0,
    };

    match decision.action {
        nexus_creator_memory::review::ReviewAction::PromoteToLongTerm => {
            let summarizer = PassthroughSummarizer {
                creator_id: creator_id.to_string(),
            };
            match nexus_creator_memory::review::promote_to_long_term(
                nexus_home,
                creator_id,
                input,
                &summarizer,
            )
            .await
            {
                Ok(_) => {
                    counts.promoted = 1;
                    delete_pending_by_id(pool, &input.pending_id).await;
                }
                Err(e) => {
                    tracing::warn!(
                        pending_id = %input.pending_id,
                        error = %e,
                        "Failed to promote pending review; skipping"
                    );
                }
            }
        }
        nexus_creator_memory::review::ReviewAction::FragmentOnly => {
            let fragment = nexus_creator_memory::review::create_fragment_from_review(input);
            let record = nexus_local_db::memory_fragment::MemoryFragmentRecord {
                fragment_id: fragment.fragment_id,
                session_id: fragment.session_id,
                creator_id: fragment.creator_id,
                keywords: serde_json::to_string(&fragment.keywords).unwrap_or_default(),
                summary: fragment.summary,
                created_at: fragment.created_at,
                ttl: fragment.ttl,
                world_id: input.world_id.clone(),
            };

            match nexus_local_db::memory_fragment::create_fragment(pool, &record).await {
                Ok(()) => {
                    counts.fragmented = 1;
                    delete_pending_by_id(pool, &input.pending_id).await;
                }
                Err(e) => {
                    tracing::warn!(
                        pending_id = %input.pending_id,
                        error = %e,
                        "Failed to create fragment; skipping"
                    );
                }
            }
        }
        nexus_creator_memory::review::ReviewAction::Drop => {
            delete_pending_by_id(pool, &input.pending_id).await;
            counts.dropped = 1;
        }
        // MergeIntoExisting and TriggerSoulExperienceOnly are Phase 2 features
        _ => {
            tracing::debug!(
                pending_id = %input.pending_id,
                action = ?decision.action,
                "Skipping unimplemented review action"
            );
        }
    }

    counts
}

/// Delete a pending review entry by ID (best-effort, logs on failure).
async fn delete_pending_by_id(pool: &sqlx::SqlitePool, pending_id: &str) {
    let pid = pending_id.to_string();
    if let Err(e) = sqlx::query!(
        "DELETE FROM memory_pending_review WHERE pending_id = ?",
        pid
    )
    .execute(pool)
    .await
    {
        tracing::warn!(pending_id = %pending_id, error = %e, "Failed to delete pending review after processing");
    }
}

/// Passthrough summarizer that returns the raw digest with an UNTRUSTED header.
///
/// In a future iteration, this will be replaced by an ACP-based summarizer
/// that produces a more structured memory entry. For V1.33, the raw digest
/// is used directly to close the loop without requiring LLM calls.
///
/// **Security (R-V133P4-03):** Prepends a provenance header so downstream
/// consumers can identify untrusted content from session capture digests.
///
/// **Safety (R-V133P4-06):** Caps the raw digest at `MAX_DIGEST_BYTES` (256 KiB).
/// Larger digests are truncated with a warning log.
struct PassthroughSummarizer {
    /// Active creator ID — injected at construction time so the UNTRUSTED
    /// header is self-contained for downstream consumers (R-V133P4-03).
    creator_id: String,
}

/// Maximum allowed digest size in bytes (256 KiB). R-V133P4-06.
const MAX_DIGEST_BYTES: usize = 256 * 1024;

impl nexus_creator_memory::review::SessionDigestSummarizer for PassthroughSummarizer {
    async fn summarize(
        &self,
        session_id: &str,
        task_kind: &str,
        raw_digest: &str,
        world_id: Option<&str>,
    ) -> Result<String, nexus_creator_memory::errors::MemoryError> {
        // R-V133P4-06: Size guard — truncate if digest exceeds 256 KiB.
        let digest = if raw_digest.len() > MAX_DIGEST_BYTES {
            tracing::warn!(
                original_len = raw_digest.len(),
                max_bytes = MAX_DIGEST_BYTES,
                "PassthroughSummarizer: raw_digest exceeds 256 KiB cap, truncating"
            );
            &raw_digest[..MAX_DIGEST_BYTES]
        } else {
            raw_digest
        };

        // R-V133P4-03: Prepend UNTRUSTED provenance header so downstream
        // consumers (context assembly, moment prompts) can apply extra validation.
        // Header must be self-contained: creator_id binds the LTM to the active
        // creator (not body-supplied), captured_at provides RFC 3339 provenance.
        let captured_at = chrono::Utc::now().to_rfc3339();
        let header = format!(
            "# UNTRUSTED: sourced from session_capture digest\n# creator_id: {}\n# session_id: {session_id}\n# task_kind: {task_kind}\n# world_id: {}\n# captured_at: {captured_at}\n\n",
            self.creator_id,
            world_id.unwrap_or("(none)")
        );
        Ok(format!("{header}{digest}"))
    }
}

/// `GET /v1/local/memory/fragments?creator_id=...&keyword=...&limit=...`
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

    let records = if params.keyword.is_some() {
        // Use filtered query for keyword search
        let limit_u32 = u32::try_from(limit).unwrap_or(u32::MAX);
        nexus_local_db::memory_fragment::list_fragments_filtered(
            state.pool(),
            &active_creator,
            params.keyword.as_deref(),
            params.world_id.as_deref(),
            limit_u32,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to list memory fragments: {e}"),
        })?
    } else {
        // R-V178P0-QC3-002 (W-QC3-002): no-keyword path now uses the bounded
        // DAO (`LIMIT ?` in SQL) instead of `list_fragments` (fetch-all) +
        // in-Rust `truncate(limit)`. For total ≤ limit the returned set is
        // identical; the cap is simply enforced server-side now.
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        nexus_local_db::memory_fragment::list_fragments_limited(
            state.pool(),
            &active_creator,
            params.world_id.as_deref(),
            limit_i64,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to list memory fragments: {e}"),
        })?
    };

    let fragments_list: Vec<MemoryFragmentInfo> = records
        .into_iter()
        .map(|r| MemoryFragmentInfo {
            fragment_id: r.fragment_id,
            summary: r.summary,
            world_id: r.world_id,
            keywords: Some(decode_fragment_keywords(&r.keywords)),
            created_at: Some(r.created_at),
        })
        .collect();

    debug!(count = fragments_list.len(), "Fragments retrieved");

    Ok(Json(ListMemoryFragmentsResponse {
        fragments: fragments_list,
    }))
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Minimum fragment count before narrative synthesis is attempted.
const MIN_SOUL_NARRATIVE_FRAGMENTS: i64 = 10;

/// Minimum distinct keyword count before narrative synthesis is attempted.
const MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS: i64 = 20;

/// `POST /v1/local/memory/soul/reflect`
///
/// Reads or regenerates the cached whole-Creator SOUL narrative.
/// The endpoint:
/// 1. Enforces active creator (same pattern as `fragments`).
/// 2. Computes fragment stats for the insufficient-data gate.
/// 3. Checks the cache for a current/stale/ungenerated state.
/// 4. If `force_regenerate` or stale or ungenerated, synthesizes via
///    `SoulNarrativeSynthesizer` (ACP-backed), persists the result, and returns it.
/// 5. The insufficient-data gate (`fragment_count < 10` OR
///    `distinct_keyword_count < 20`) is evaluated BEFORE any ACP call, so
///    new creators never pay LLM latency for a guaranteed-thin result.
///
/// Auth: requires active creator from config.toml.
/// Request body `creator_id` must match the active creator, otherwise 403.
#[allow(clippy::too_many_lines, clippy::similar_names)]
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
        force_regenerate = req.force_regenerate.unwrap_or(false),
        "Reflecting on Creator SOUL narrative"
    );

    // 1. Compute fragment stats for the insufficient-data gate + stale detection.
    let fragment_stats =
        nexus_local_db::soul_narrative_fragment_stats(state.pool(), &active_creator)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("failed to compute fragment stats: {e}"),
            })?;

    let force = req.force_regenerate.unwrap_or(false);

    // 2. Insufficient-data gate (before any ACP call).
    let min_distinct = usize::try_from(MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS).unwrap_or(usize::MAX);
    let insufficient = fragment_stats.fragment_count < MIN_SOUL_NARRATIVE_FRAGMENTS
        || fragment_stats.distinct_keyword_count < min_distinct;

    if insufficient {
        return Ok(Json(SoulNarrativeResponse {
            creator_id: active_creator,
            state: "insufficient_data".to_string(),
            narrative: None,
            generated_at: None,
            stale: false,
            fragment_count_at_generation: None,
            max_fragment_created_at_at_generation: None,
            current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
            current_distinct_keyword_count: u64::try_from(fragment_stats.distinct_keyword_count)
                .unwrap_or(0),
            min_fragment_count: MIN_SOUL_NARRATIVE_FRAGMENTS,
            min_distinct_keyword_count: MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
        }));
    }

    // 3. Check the cache.
    let cached = nexus_local_db::get_soul_narrative(state.pool(), &active_creator)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to read soul narrative cache: {e}"),
        })?;

    // 4. Stale detection.
    let stale = if let Some(ref c) = cached {
        c.fragment_count_at_generation != fragment_stats.fragment_count
            || c.max_fragment_created_at_at_generation.as_deref()
                != fragment_stats.max_created_at.as_deref()
    } else {
        false
    };

    // 5. If cached and not force and not stale → return current.
    if !force && !stale {
        if let Some(ref c) = cached {
            return Ok(Json(SoulNarrativeResponse {
                creator_id: active_creator,
                state: "current".to_string(),
                narrative: Some(c.narrative.clone()),
                generated_at: Some(c.generated_at.clone()),
                stale: false,
                fragment_count_at_generation: Some(
                    u64::try_from(c.fragment_count_at_generation).unwrap_or(0),
                ),
                max_fragment_created_at_at_generation: c
                    .max_fragment_created_at_at_generation
                    .clone(),
                current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
                current_distinct_keyword_count: u64::try_from(
                    fragment_stats.distinct_keyword_count,
                )
                .unwrap_or(0),
                min_fragment_count: MIN_SOUL_NARRATIVE_FRAGMENTS,
                min_distinct_keyword_count: MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
            }));
        }
    }

    // 6. If cached and not force and stale → return stale (don't regenerate).
    if !force && stale {
        if let Some(ref c) = cached {
            return Ok(Json(SoulNarrativeResponse {
                creator_id: active_creator,
                state: "stale".to_string(),
                narrative: Some(c.narrative.clone()),
                generated_at: Some(c.generated_at.clone()),
                stale: true,
                fragment_count_at_generation: Some(
                    u64::try_from(c.fragment_count_at_generation).unwrap_or(0),
                ),
                max_fragment_created_at_at_generation: c
                    .max_fragment_created_at_at_generation
                    .clone(),
                current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
                current_distinct_keyword_count: u64::try_from(
                    fragment_stats.distinct_keyword_count,
                )
                .unwrap_or(0),
                min_fragment_count: MIN_SOUL_NARRATIVE_FRAGMENTS,
                min_distinct_keyword_count: MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
            }));
        }
    }

    // 7. Need to synthesize (ungenerated, stale + force, or force).
    let registry =
        state
            .capability_registry()
            .ok_or_else(|| NexusApiError::ServiceUnavailable {
                message: "capability registry not available".to_string(),
            })?;

    let synthesizer =
        crate::api::handlers::soul_narrative_synthesizer::AcpSoulNarrativeSynthesizer::new(
            registry,
        );

    // Build capped input signal.
    let input =
        build_soul_narrative_synthesis_input(state.pool(), &active_creator, &fragment_stats)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("failed to build synthesis input: {e}"),
            })?;

    let draft = synthesizer
        .synthesize(&active_creator, input)
        .await
        .map_err(|e| {
            // Map MemoryError to appropriate NexusApiError.
            let msg = e.to_string();
            if msg.contains("not available") || msg.contains("unavailable") {
                NexusApiError::ServiceUnavailable { message: msg }
            } else {
                NexusApiError::Internal {
                    code: "NARRATIVE_SYNTHESIS_ERROR".into(),
                    message: msg,
                }
            }
        })?;

    // 8. Persist the result.
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::SoulNarrativeRecord {
        creator_id: active_creator.clone(),
        narrative: draft.narrative.clone(),
        generated_at: now.clone(),
        fragment_count_at_generation: fragment_stats.fragment_count,
        max_fragment_created_at_at_generation: fragment_stats.max_created_at.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    nexus_local_db::upsert_soul_narrative(state.pool(), &record)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to persist soul narrative: {e}"),
        })?;

    Ok(Json(SoulNarrativeResponse {
        creator_id: active_creator,
        state: "current".to_string(),
        narrative: Some(draft.narrative),
        generated_at: Some(record.generated_at),
        stale: false,
        fragment_count_at_generation: Some(
            u64::try_from(record.fragment_count_at_generation).unwrap_or(0),
        ),
        max_fragment_created_at_at_generation: record.max_fragment_created_at_at_generation,
        current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(fragment_stats.distinct_keyword_count)
            .unwrap_or(0),
        min_fragment_count: MIN_SOUL_NARRATIVE_FRAGMENTS,
        min_distinct_keyword_count: MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
    }))
}

/// Build a capped `SoulNarrativeSynthesisInput` from the creator's fragments.
///
/// Caps: ≤30 keywords, ≤24 summaries ≤280 chars, ≤8 temporal buckets.
async fn build_soul_narrative_synthesis_input(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> Result<nexus_creator_memory::soul_narrative::SoulNarrativeSynthesisInput, NexusApiError> {
    use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesisInput;

    // Fetch recent fragments for summaries + keyword counting.
    let fragments = nexus_local_db::list_fragments_limited(pool, creator_id, None, 100)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to fetch fragments for synthesis: {e}"),
        })?;

    // Count keywords across all fragments.
    let mut keyword_counts: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    let mut summaries: Vec<String> = Vec::new();

    for frag in &fragments {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&frag.keywords) {
            for kw in keywords {
                *keyword_counts.entry(kw).or_default() += 1;
            }
        }
        // Cap summaries: ≤24, each ≤280 chars.
        if summaries.len() < 24 {
            let summary = if frag.summary.len() > 280 {
                format!("{}…", &frag.summary[..279])
            } else {
                frag.summary.clone()
            };
            summaries.push(summary);
        }
    }

    // Sort keywords by count desc, cap at 30.
    let mut top_keywords: Vec<(String, u64)> = keyword_counts.into_iter().collect();
    top_keywords.sort_by_key(|(_k, count)| std::cmp::Reverse(*count));
    top_keywords.truncate(30);

    // Build temporal buckets (up to 8) by grouping fragments into time windows.
    let temporal_buckets = build_temporal_buckets(&fragments);

    Ok(SoulNarrativeSynthesisInput {
        top_keywords,
        recent_summaries: summaries,
        temporal_buckets,
        total_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
        oldest_created_at: fragments.last().map(|f| f.created_at.clone()),
        newest_created_at: fragments.first().map(|f| f.created_at.clone()),
    })
}

/// Build up to 8 temporal buckets from fragments (ordered by `created_at`).
fn build_temporal_buckets(
    fragments: &[nexus_local_db::MemoryFragmentRecord],
) -> Vec<nexus_creator_memory::soul_narrative::TemporalBucket> {
    use nexus_creator_memory::soul_narrative::TemporalBucket;

    if fragments.is_empty() {
        return Vec::new();
    }

    let max_buckets = 8;
    let n = fragments.len();
    let bucket_size = n.div_ceil(max_buckets); // ceiling division
    let bucket_size = bucket_size.max(1);

    let mut buckets: Vec<TemporalBucket> = Vec::new();

    for (bi, chunk) in fragments.chunks(bucket_size).enumerate() {
        if buckets.len() >= max_buckets {
            break;
        }

        // Collect top 5 keywords in this bucket.
        let mut kw_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for frag in chunk {
            if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&frag.keywords) {
                for kw in keywords {
                    *kw_counts.entry(kw).or_default() += 1;
                }
            }
        }
        let mut top: Vec<(String, u64)> = kw_counts.into_iter().collect();
        top.sort_by_key(|(_k, count)| std::cmp::Reverse(*count));
        top.truncate(5);
        let top_keywords: Vec<String> = top.into_iter().map(|(k, _)| k).collect();

        // Label: use the first fragment's created_at date portion.
        let label = chunk.first().map_or_else(
            || format!("bucket_{bi}"),
            |f| {
                // Extract date portion (first 10 chars = YYYY-MM-DD).
                if f.created_at.len() >= 10 {
                    f.created_at[..10].to_string()
                } else {
                    f.created_at.clone()
                }
            },
        );

        buckets.push(TemporalBucket {
            label,
            top_keywords,
            fragment_count: u64::try_from(chunk.len()).unwrap_or(0),
        });
    }

    buckets
}

/// Decode the `memory_fragments.keywords` JSON-array string into `Vec<String>`.
///
/// V1.79 SOUL visualization reads this off the list-fragments wire shape. The
/// stored column is a JSON array (`["alpha","beta"]`); legacy or corrupt rows
/// may hold malformed JSON. A decode failure degrades to an **empty** list
/// rather than failing the whole fragments response — the read-only viz then
/// simply shows no keywords for that fragment. Mirrors the same
/// `serde_json::from_str::<Vec<String>>(...).unwrap_or_default()` contract used
/// by `nexus_local_db::memory_fragment::get_all_keywords`.
fn decode_fragment_keywords(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Read active `creator_id` from CLI config (matches works.rs pattern).
///
/// Returns `None` if no active creator is configured in `config.toml`.
fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_creator_memory::review::SessionDigestSummarizer as _;

    #[tokio::test]
    async fn passthrough_summarizer_includes_untrusted_header() {
        let summarizer = PassthroughSummarizer {
            creator_id: "ctr_test_creator".to_string(),
        };
        let result = summarizer
            .summarize(
                "sess_123",
                "brainstorm",
                "My brainstorm content",
                Some("world_1"),
            )
            .await
            .unwrap();

        assert!(
            result.starts_with("# UNTRUSTED:"),
            "LTM body should start with UNTRUSTED header, got: {}",
            &result[..result.len().min(50)]
        );
        assert!(
            result.contains("# creator_id: ctr_test_creator"),
            "Header should include creator_id (active creator)"
        );
        assert!(
            result.contains("# session_id: sess_123"),
            "Header should include session_id"
        );
        assert!(
            result.contains("# task_kind: brainstorm"),
            "Header should include task_kind"
        );
        assert!(
            result.contains("# world_id: world_1"),
            "Header should include world_id"
        );
        assert!(
            result.contains("# captured_at: "),
            "Header should include captured_at (RFC 3339)"
        );
        assert!(
            result.contains("My brainstorm content"),
            "Body should contain the raw digest after the header"
        );
    }

    #[tokio::test]
    async fn passthrough_summarizer_truncates_large_digest() {
        let summarizer = PassthroughSummarizer {
            creator_id: "ctr_big".to_string(),
        };
        // Create a digest larger than 256 KiB.
        let large_digest = "x".repeat(MAX_DIGEST_BYTES + 1000);
        let result = summarizer
            .summarize("sess_big", "test", &large_digest, None)
            .await
            .unwrap();

        // The result should be capped at MAX_DIGEST_BYTES + header.
        let body_after_header = result
            .split_once("\n\n")
            .map(|(_, body)| body)
            .unwrap_or("");
        assert_eq!(
            body_after_header.len(),
            MAX_DIGEST_BYTES,
            "Digest should be truncated to MAX_DIGEST_BYTES"
        );
    }

    #[tokio::test]
    async fn passthrough_summarizer_small_digest_unchanged() {
        let summarizer = PassthroughSummarizer {
            creator_id: "ctr_small".to_string(),
        };
        let small = "Hello world";
        let result = summarizer
            .summarize("sess_small", "test", small, None)
            .await
            .unwrap();

        assert!(
            result.contains(small),
            "Small digest should be included verbatim"
        );
    }

    // ── V1.79: keyword JSON decode (SOUL visualization projection) ──────────

    #[test]
    fn decode_fragment_keywords_parses_valid_json_array() {
        let kw = decode_fragment_keywords(r#"["historical fiction","moral ambiguity"]"#);
        assert_eq!(kw, vec!["historical fiction", "moral ambiguity"]);
    }

    #[test]
    fn decode_fragment_keywords_empty_array() {
        assert!(decode_fragment_keywords("[]").is_empty());
    }

    #[test]
    fn decode_fragment_keywords_malformed_json_degrades_to_empty() {
        // Legacy/corrupt rows must never fail the fragments response.
        assert!(decode_fragment_keywords("not valid json").is_empty());
        assert!(decode_fragment_keywords("").is_empty());
        // A JSON object (not an array) is also rejected gracefully.
        assert!(decode_fragment_keywords(r#"{"key":"value"}"#).is_empty());
    }

    #[test]
    fn decode_fragment_keywords_non_string_items_rejected() {
        // Mixed-type arrays are not `Vec<String>` → graceful empty.
        assert!(decode_fragment_keywords(r#"["ok", 42]"#).is_empty());
    }

    // ─── V1.80 REL-01: deadline-aware partial progress ───────────────────

    /// With a deadline already in the past, `process_review_batch` inspects
    /// zero rows and returns all-zero counters. The caller (review handler)
    /// then reports `has_more = true` because `processed < processing_slice`.
    /// This proves the deadline-check-before-each-row logic stops the loop
    /// immediately on budget exhaustion without touching side effects.
    #[tokio::test]
    async fn process_review_batch_past_deadline_processes_zero_rows() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("test pool");

        // Seed 3 rows so the processing slice is non-empty; the deadline alone
        // must prevent them from being inspected.
        for i in 0..3 {
            // SAFETY: test-only seed using runtime query.
            sqlx::query(
                "INSERT OR IGNORE INTO memory_pending_review \
                 (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at) \
                 VALUES (?, ?, 'ctr_testuser', NULL, 'research', \
                 'Research summary long enough to classify as fragment.', '2026-01-01T00:00:0X0Z')",
            )
            .bind(format!("pending_deadline_{i}"))
            .bind(format!("sess_deadline_{i}"))
            .execute(&pool)
            .await
            .expect("seed");
        }

        let rows = fetch_pending_reviews_page(&pool, "ctr_testuser", None, 51)
            .await
            .expect("fetch");
        assert_eq!(rows.len(), 3, "precondition: 3 rows seeded");

        // Deadline already in the past.
        let deadline = tokio::time::Instant::now();
        let outcome =
            process_review_batch(&rows, &nexus_home, "ctr_testuser", &pool, deadline).await;

        assert_eq!(outcome.processed, 0, "past deadline must inspect zero rows");
        assert_eq!(outcome.promoted, 0);
        assert_eq!(outcome.fragmented, 0);
        assert_eq!(outcome.dropped, 0);

        // The 3 seeded rows are untouched (no side effects ran).
        let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_pending_review")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(remaining.0, 3, "no rows deleted under a past deadline");

        drop(tmp);
    }
}
