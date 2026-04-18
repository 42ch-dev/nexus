//! Memory pending review handlers — session-end capture for review pipeline.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Request body for creating a pending review entry.
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
    pub pending_reviews: Vec<PendingReviewInfo>,
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
/// Uses `INSERT OR IGNORE` to handle retries gracefully. If a pending_id or
/// session_id already exists, the insert is silently skipped, returning success.
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
    let creator_id = req.creator_id.clone();
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
        message: format!("failed to create pending review: {}", e),
    })?;

    debug!(pending_id = %req.pending_id, "Pending review entry created (or ignored on duplicate)");

    Ok(Json(CreatePendingReviewResponse {
        success: true,
        pending_id: req.pending_id,
    }))
}

/// Validate input fields for create_pending_review.
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
    if !nexus_domain::is_valid_creator_id(&req.creator_id) {
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
/// Lists all pending reviews for a creator.
pub async fn list_pending_reviews(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListPendingReviewsQuery>,
) -> Result<Json<ListPendingReviewsResponse>, NexusApiError> {
    info!(creator_id = %params.creator_id, "Listing pending reviews");

    // Validate creator_id format
    if !nexus_domain::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    let creator_id_filter = params.creator_id.clone();
    let pending_reviews = sqlx::query_as!(
        PendingReviewInfo,
        r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at DESC"#,
        creator_id_filter
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to list pending reviews: {}", e),
    })?;

    debug!(count = pending_reviews.len(), "Pending reviews retrieved");

    Ok(Json(ListPendingReviewsResponse { pending_reviews }))
}

/// Query parameters for listing pending reviews.
#[derive(Debug, Deserialize)]
pub struct ListPendingReviewsQuery {
    pub creator_id: String,
}

/// GET /v1/local/memory/pending-review/count?creator_id=...
///
/// Returns the count of pending reviews for a creator.
pub async fn count_pending_reviews(
    State(state): State<WorkspaceState>,
    Query(params): Query<CountPendingReviewsQuery>,
) -> Result<Json<CountPendingReviewsResponse>, NexusApiError> {
    info!(creator_id = %params.creator_id, "Counting pending reviews");

    // Validate creator_id format
    if !nexus_domain::is_valid_creator_id(&params.creator_id) {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".into(),
            reason: "creator_id must start with 'ctr_' followed by alphanumeric characters".into(),
        });
    }

    let creator_id_filter = params.creator_id.clone();
    let row = sqlx::query_scalar!(
        "SELECT COUNT(*) as \"count!\" FROM memory_pending_review WHERE creator_id = ?",
        creator_id_filter
    )
    .fetch_one(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to count pending reviews: {}", e),
    })?;

    Ok(Json(CountPendingReviewsResponse {
        count: row as usize,
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

    // Validate creator_id format
    if !nexus_domain::is_valid_creator_id(&params.creator_id) {
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
         FROM memory_pending_review WHERE pending_id = ?1"#,
        pid
    )
    .fetch_optional(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("failed to lookup pending review: {}", e),
    })?;

    match review {
        None => {
            return Err(NexusApiError::NotFound(format!(
                "pending review '{}' not found",
                pending_id
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
        message: format!("failed to delete pending review: {}", e),
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
