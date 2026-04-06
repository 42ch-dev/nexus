//! Context assembly handlers

use axum::Json;
use serde::Deserialize;

use crate::api::errors::NexusApiError;

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ContextAssembleRequest {
    pub request_id: String,
    pub workspace_id: String,
    pub creator_id: String,
    pub world_id: String,
}

/// POST /v1/local/context/assemble
///
/// Returns 501 Not Implemented — context assembly is not yet implemented on the daemon side.
pub async fn assemble(Json(_req): Json<ContextAssembleRequest>) -> Result<Json<()>, NexusApiError> {
    Err(NexusApiError::NotImplemented(
        "Context assembly not yet implemented".into(),
    ))
}
