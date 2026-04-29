//! Capabilities listing handler.

use crate::workspace::WorkspaceState;
use axum::{extract::State, http::StatusCode, Json};
use nexus_contracts::local::orchestration::http::{CapabilityInfo, ListCapabilitiesResponse};

/// `GET /v1/local/orchestration/capabilities`
pub async fn list_capabilities(
    State(state): State<WorkspaceState>,
) -> (StatusCode, Json<ListCapabilitiesResponse>) {
    let Some(registry) = state.capability_registry() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ListCapabilitiesResponse {
                capabilities: Vec::new(),
            }),
        );
    };

    let capabilities: Vec<CapabilityInfo> = registry
        .iter()
        .map(|cap| CapabilityInfo {
            name: cap.name().to_string(),
            input_schema: cap.input_schema().to_string(),
            output_schema: cap.output_schema().to_string(),
        })
        .collect();

    (
        StatusCode::OK,
        Json(ListCapabilitiesResponse { capabilities }),
    )
}
