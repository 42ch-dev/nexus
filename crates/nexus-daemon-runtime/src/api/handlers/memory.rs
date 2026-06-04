//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Memory pending review handlers — session-end capture for review pipeline.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
#[derive(Debug, Deserialize)]
pub struct CreatePendingReviewRequest {
    /// Unique identifier for this pending entry.
    pub pending_id: String,
    /// ACP session ID that triggered the capture.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Optional world ID for context.
    pub world_id: Option<String>,
    /// Task kind heuristic (brainstorm, outline, chapter, research, unknown).
    pub task_kind: Option<String>,
    /// Raw digest extracted from session.
    pub raw_digest: String,
    /// Creation timestamp (defaults to now if omitted).
    pub created_at: Option<String>,
}

/// Response body for creating a pending review entry.
#[derive(Debug, Serialize)]
pub struct CreatePendingReviewResponse {
    pub success: bool,
    pub pending_id: String,
}

/// Response body for listing pending reviews.
#[derive(Debug, Serialize)]
pub struct ListPendingReviewsResponse {
    pub items: Vec<PendingReviewInfo>,
    pub pagination: PaginationEnvelope,
}

/// Cursor-based pagination envelope.
#[derive(Debug, Serialize)]
pub struct PaginationEnvelope {
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Pending review info for API responses.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PendingReviewInfo {
    pub pending_id: String,
    pub session_id: String,
    pub creator_id: String,
    pub world_id: Option<String>,
    pub task_kind: String,
    pub raw_digest: String,
    pub created_at: String,
}

/// Response body for getting pending review count.
#[derive(Debug, Serialize)]
pub struct CountPendingReviewsResponse {
    pub count: usize,
}

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

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let creator_id_filter = active_creator; // R-V133P4-07: use active creator
    let all_reviews = sqlx::query_as!(
        PendingReviewInfo,
        r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at DESC"#,
        creator_id_filter
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to list pending reviews: {e}"),
    })?;

    let mut items = all_reviews;

    // Apply cursor-based pagination (cursor = pending_id)
    if let Some(ref cursor) = params.cursor {
        let pos = items.iter().position(|i| i.pending_id == *cursor);
        if let Some(idx) = pos {
            items = items.split_off(idx + 1);
        }
    }

    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|i| i.pending_id.clone())
    } else {
        None
    };

    debug!(count = items.len(), "Pending reviews retrieved");

    Ok(Json(ListPendingReviewsResponse {
        items,
        pagination: PaginationEnvelope { limit, next_cursor },
    }))
}

/// Query parameters for listing pending reviews.
#[derive(Debug, Deserialize)]
pub struct ListPendingReviewsQuery {
    pub creator_id: String,
    /// Maximum number of items to return (1–250, default 50).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Opaque cursor for pagination; pass `next_cursor` from the previous page.
    pub cursor: Option<String>,
}

const fn default_limit() -> usize {
    50
}

/// Maximum items per page.
const MAX_LIMIT: usize = 250;

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
        // SAFETY: SQLite COUNT(*) result fits in usize; unwrap_or(0) handles theoretical overflow
        count: usize::try_from(row).unwrap_or(0),
    }))
}

/// Query parameters for counting pending reviews.
#[derive(Debug, Deserialize)]
pub struct CountPendingReviewsQuery {
    pub creator_id: String,
}

/// Query parameters for deleting a pending review.
#[derive(Debug, Deserialize)]
pub struct DeletePendingReviewQuery {
    pub creator_id: String,
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
    let review = sqlx::query_as!(
        PendingReviewInfo,
        r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE pending_id = ?"#, // sqlx R3: use ? instead of ?1
        pid
    )
    .fetch_optional(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to lookup pending review: {e}"),
    })?;

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

/// Response body for deleting a pending review.
#[derive(Debug, Serialize)]
pub struct DeletePendingReviewResponse {
    pub success: bool,
    pub pending_id: String,
}

// ─── Review + Fragments handlers (V1.33 P4) ────────────────────────────────

/// Request body for `POST /v1/local/memory/review`.
///
/// Triggers the review pipeline for a creator's pending review queue.
/// The daemon classifies each pending entry (promote / fragment / drop)
/// and returns a summary of actions taken.
#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    /// Creator ID whose pending reviews should be processed.
    pub creator_id: String,
}

/// Response body for `POST /v1/local/memory/review`.
///
/// Summarizes how many pending entries were promoted to long-term memory,
/// fragmented, or dropped.
#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    /// Number of entries promoted to long-term memory.
    pub promoted: usize,
    /// Number of entries converted to keyword fragments.
    pub fragmented: usize,
    /// Number of entries dropped (below quality threshold).
    pub dropped: usize,
}

/// Query parameters for `GET /v1/local/memory/fragments`.
#[derive(Debug, Deserialize)]
pub struct ListFragmentsQuery {
    /// Creator ID to filter fragments by (required).
    pub creator_id: String,
    /// Optional keyword filter (case-insensitive LIKE match).
    pub keyword: Option<String>,
    /// Maximum number of fragments to return (1–250, default 50).
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// A single fragment row in the list fragments response.
#[derive(Debug, Serialize)]
pub struct FragmentInfo {
    pub fragment_id: String,
    pub summary: String,
}

/// Response body for `GET /v1/local/memory/fragments`.
#[derive(Debug, Serialize)]
pub struct ListFragmentsResponse {
    pub fragments: Vec<FragmentInfo>,
}

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

    let creator_id_filter = active_creator.clone();
    let rows = sqlx::query_as!(
        PendingReviewInfo,
        r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at DESC"#,
        creator_id_filter
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to fetch pending reviews for review: {e}"),
    })?;

    let nexus_home = state.nexus_home().to_owned();
    let pool = state.pool().clone();

    let result = process_review_queue(&rows, &nexus_home, &active_creator, &pool).await;

    info!(
        creator_id = %active_creator,
        promoted = result.promoted,
        fragmented = result.fragmented,
        dropped = result.dropped,
        "Review completed"
    );

    Ok(Json(result))
}

/// Process the review queue for a creator's pending entries.
///
/// Classifies each entry, performs the appropriate action (promote, fragment,
/// or drop), and returns a summary of actions taken.
async fn process_review_queue(
    rows: &[PendingReviewInfo],
    nexus_home: &std::path::Path,
    creator_id: &str,
    pool: &sqlx::SqlitePool,
) -> ReviewResponse {
    let mut promoted: usize = 0;
    let mut fragmented: usize = 0;
    let mut dropped: usize = 0;

    for row in rows {
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

        match decision.action {
            nexus_creator_memory::review::ReviewAction::PromoteToLongTerm => {
                let summarizer = PassthroughSummarizer {
                    creator_id: creator_id.to_string(),
                };
                match nexus_creator_memory::review::promote_to_long_term(
                    nexus_home,
                    creator_id,
                    &input,
                    &summarizer,
                )
                .await
                {
                    Ok(_) => {
                        promoted += 1;
                        delete_pending_by_id(pool, &row.pending_id).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            pending_id = %row.pending_id,
                            error = %e,
                            "Failed to promote pending review; skipping"
                        );
                    }
                }
            }
            nexus_creator_memory::review::ReviewAction::FragmentOnly => {
                let fragment = nexus_creator_memory::review::create_fragment_from_review(&input);
                let record = nexus_local_db::memory_fragment::MemoryFragmentRecord {
                    fragment_id: fragment.fragment_id,
                    session_id: fragment.session_id,
                    creator_id: fragment.creator_id,
                    keywords: serde_json::to_string(&fragment.keywords).unwrap_or_default(),
                    summary: fragment.summary,
                    created_at: fragment.created_at,
                    ttl: fragment.ttl,
                };

                match nexus_local_db::memory_fragment::create_fragment(pool, &record).await {
                    Ok(()) => {
                        fragmented += 1;
                        delete_pending_by_id(pool, &row.pending_id).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            pending_id = %row.pending_id,
                            error = %e,
                            "Failed to create fragment; skipping"
                        );
                    }
                }
            }
            nexus_creator_memory::review::ReviewAction::Drop => {
                delete_pending_by_id(pool, &row.pending_id).await;
                dropped += 1;
            }
            // MergeIntoExisting and TriggerSoulExperienceOnly are Phase 2 features
            _ => {
                tracing::debug!(
                    pending_id = %row.pending_id,
                    action = ?decision.action,
                    "Skipping unimplemented review action"
                );
            }
        }
    }

    ReviewResponse {
        promoted,
        fragmented,
        dropped,
    }
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
    Query(params): Query<ListFragmentsQuery>,
) -> Result<Json<ListFragmentsResponse>, NexusApiError> {
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
        "Listing memory fragments"
    );

    let limit = params.limit.clamp(1, MAX_LIMIT);

    let records = if params.keyword.is_some() {
        // Use filtered query for keyword search
        let limit_u32 = u32::try_from(limit).unwrap_or(u32::MAX);
        nexus_local_db::memory_fragment::list_fragments_filtered(
            state.pool(),
            &active_creator,
            params.keyword.as_deref(),
            limit_u32,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("failed to list memory fragments: {e}"),
        })?
    } else {
        // Use compile-time checked query (more reliable) when no keyword filter
        let all = nexus_local_db::memory_fragment::list_fragments(state.pool(), &active_creator)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("failed to list memory fragments: {e}"),
            })?;
        // Apply limit manually
        let mut truncated = all;
        truncated.truncate(limit);
        truncated
    };

    let fragments_list: Vec<FragmentInfo> = records
        .into_iter()
        .map(|r| FragmentInfo {
            fragment_id: r.fragment_id,
            summary: r.summary,
        })
        .collect();

    debug!(count = fragments_list.len(), "Fragments retrieved");

    Ok(Json(ListFragmentsResponse {
        fragments: fragments_list,
    }))
}

// ─── Helpers ────────────────────────────────────────────────────────────────

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
}
