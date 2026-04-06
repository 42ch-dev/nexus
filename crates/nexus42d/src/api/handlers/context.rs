//! Context assembly handlers

use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ContextAssembleRequest {
    pub request_id: String,
    pub workspace_id: String,
    pub creator_id: String,
    pub world_id: String,
}

#[derive(Serialize)]
pub struct ContextAssembleResponse {
    pub status: String,
    pub message: String,
}

/// POST /v1/local/context/assemble
///
/// Placeholder handler — context assembly is not yet implemented on the daemon side.
/// Returns a valid JSON response so the CLI client does not receive a 404.
pub async fn assemble(Json(_req): Json<ContextAssembleRequest>) -> Json<ContextAssembleResponse> {
    Json(ContextAssembleResponse {
        status: "ok".to_string(),
        message: "context assembly not yet implemented on daemon side".to_string(),
    })
}
