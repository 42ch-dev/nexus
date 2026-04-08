//! Context assembly handlers

use axum::Json;
use nexus_contracts::generated::ContextAssembleRequestV1;
use tracing::info;

use crate::api::errors::NexusApiError;

/// POST /v1/local/context/assemble
///
/// Returns 501 Not Implemented — context assembly is not yet implemented on the daemon side.
pub async fn assemble(
    Json(_req): Json<ContextAssembleRequestV1>,
) -> Result<Json<()>, NexusApiError> {
    info!("Handling context assemble request");
    Err(NexusApiError::NotImplemented(
        "Context assembly not yet implemented".into(),
    ))
}
