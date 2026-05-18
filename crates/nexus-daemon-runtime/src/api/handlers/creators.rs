//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Creator handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CreatorInfo {
    pub creator_id: String,
    pub display_name: String,
    pub status: String,
    pub cached_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListCreatorsQuery {
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

#[derive(Serialize)]
pub struct ListCreatorsResponse {
    pub items: Vec<CreatorInfo>,
    pub pagination: PaginationEnvelope,
}

/// Cursor-based pagination envelope.
#[derive(Debug, Serialize)]
pub struct PaginationEnvelope {
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// GET /v1/local/creators
pub async fn list(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListCreatorsQuery>,
) -> Result<Json<ListCreatorsResponse>, NexusApiError> {
    info!("Handling list creators request");

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let all_creators = sqlx::query_as!(
        CreatorInfo,
        r#"SELECT creator_id as "creator_id!", display_name, status, cached_at FROM creators ORDER BY cached_at DESC"#
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    let mut items = all_creators;

    // Apply cursor-based pagination (cursor = creator_id)
    if let Some(ref cursor) = params.cursor {
        let pos = items.iter().position(|i| i.creator_id == *cursor);
        if let Some(idx) = pos {
            items = items.split_off(idx + 1);
        }
    }

    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|i| i.creator_id.clone())
    } else {
        None
    };

    debug!(count = items.len(), "Creators retrieved");
    info!("List creators completed");
    Ok(Json(ListCreatorsResponse {
        items,
        pagination: PaginationEnvelope { limit, next_cursor },
    }))
}
