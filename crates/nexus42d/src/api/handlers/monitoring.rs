//! Monitoring and observability handlers
//!
//! Provides endpoints for pool status, health checks, and runtime metrics.

use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

/// Pool status response
#[derive(Debug, Serialize)]
pub struct PoolStatusResponse {
    /// Maximum pool size
    pub max_size: usize,
    /// Current pool size (total connections)
    pub size: usize,
}

/// Get database pool status (QC-W3)
///
/// Returns current pool metrics for monitoring and observability.
/// Useful for debugging connection pool exhaustion and tuning pool configuration.
pub async fn pool_status(State(state): State<WorkspaceState>) -> Json<PoolStatusResponse> {
    let status = state.db_pool().status();

    // Log pool status for observability
    tracing::debug!(
        max_size = status.max_size,
        size = status.size,
        "Pool status query"
    );

    Json(PoolStatusResponse {
        max_size: status.max_size,
        size: status.size,
    })
}
