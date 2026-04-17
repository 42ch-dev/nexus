//! Context assembly handlers

use axum::{extract::State, Json};
use chrono::Utc;
use nexus_contracts::generated::ContextAssembleRequestV1;
use nexus_domain::{AssembleMetadata, AssembleResponse};
use tracing::info;

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

/// POST /v1/local/context/assemble (V1.2 two-stage assembly)
///
/// V1.2: Returns mock response or forwards to platform.
/// For `local_only` mode, daemon returns 403 (blocked).
/// For `local_first`/`cloud_enhanced`, returns simulated response.
///
/// # Request body
///
/// Uses `AssembleRequest` internal shape (not `ContextAssembleRequestV1`):
/// - `creator_id`: Creator identifier
/// - `workspace_slug`: Workspace slug
/// - `runtime_mode`: Runtime mode string (`local_only` / `local_first` / `cloud_enhanced`)
/// - `prompt_hint`: Optional prompt hint for context assembly
pub async fn assemble(
    State(state): State<WorkspaceState>,
    Json(req): Json<AssembleRequest>,
) -> Result<Json<AssembleResponse>, NexusApiError> {
    info!(
        "Handling context assemble request (runtime_mode={})",
        req.runtime_mode
    );

    // Check runtime mode — local_only prohibits platform assemble
    if req.runtime_mode == "local_only" {
        return Err(NexusApiError::Forbidden {
            resource: "context/assemble".into(),
            reason: "Platform assemble prohibited in local_only mode".into(),
        });
    }

    // Validate runtime_mode matches daemon configuration (optional consistency check)
    let daemon_mode = state.runtime_mode_as_str();
    if req.runtime_mode != daemon_mode {
        tracing::warn!(
            "Request runtime_mode ({}) differs from daemon config ({})",
            req.runtime_mode,
            daemon_mode
        );
        // For V1.2, we allow mismatched modes (request overrides daemon config for testing)
        // V1.3 may enforce stricter validation
    }

    // V1.2: Mock response (no real platform integration yet)
    // Real platform HTTP comes in V1.3
    let mock_response = AssembleResponse {
        memory_items: vec![],
        kb: vec![],
        timeline: vec![],
        metadata: AssembleMetadata {
            assembled_at: Utc::now().to_rfc3339(),
            token_count_estimate: Some(0),
        },
    };

    Ok(Json(mock_response))
}

/// Internal request shape for V1.2 two-stage assembly.
///
/// Different from `ContextAssembleRequestV1` (platform wire contract).
/// This is the daemon-to-daemon internal request for Stage-1 platform proxy.
#[derive(Debug, serde::Deserialize)]
pub struct AssembleRequest {
    /// Creator identifier
    pub creator_id: String,
    /// Workspace slug
    pub workspace_slug: String,
    /// Runtime mode string (`local_only` / `local_first` / `cloud_enhanced`)
    pub runtime_mode: String,
    /// Optional prompt hint for context assembly
    #[serde(default)]
    pub prompt_hint: Option<String>,
}

/// Legacy POST /v1/local/context/assemble (ContextAssembleRequestV1 wire contract)
///
/// Returns 501 Not Implemented — legacy platform wire contract not used in V1.2.
/// V1.2 uses the internal `AssembleRequest` shape above.
pub async fn assemble_legacy(
    Json(_req): Json<ContextAssembleRequestV1>,
) -> Result<Json<()>, NexusApiError> {
    info!("Handling legacy context assemble request");
    Err(NexusApiError::NotImplemented(
        "Context assembly not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use axum::http::StatusCode;

    /// Test: assemble returns 403 Forbidden for local_only mode
    #[tokio::test]
    async fn assemble_blocked_for_local_only() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = AssembleRequest {
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            runtime_mode: "local_only".to_string(),
            prompt_hint: None,
        };

        let result = assemble(State(state), Json(req)).await;

        match result {
            Err(err) => {
                assert_eq!(
                    err.status_code(),
                    StatusCode::FORBIDDEN,
                    "Expected 403 Forbidden"
                );
                assert_eq!(err.error_code(), "FORBIDDEN");
            }
            Ok(_) => panic!("Expected error for local_only mode, got Ok"),
        }
    }

    /// Test: assemble returns mock response for local_first mode
    #[tokio::test]
    async fn assemble_returns_mock_for_local_first() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = AssembleRequest {
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            runtime_mode: "local_first".to_string(),
            prompt_hint: Some("Test prompt".to_string()),
        };

        let result = assemble(State(state), Json(req)).await;

        match result {
            Ok(Json(response)) => {
                assert!(response.memory_items.is_empty());
                assert!(response.kb.is_empty());
                assert!(response.timeline.is_empty());
                assert!(response.metadata.assembled_at.contains("202"));
                assert_eq!(response.metadata.token_count_estimate, Some(0));
            }
            Err(err) => panic!("Expected Ok for local_first mode, got Err: {}", err),
        }
    }

    /// Test: assemble returns mock response for cloud_enhanced mode
    #[tokio::test]
    async fn assemble_returns_mock_for_cloud_enhanced() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = AssembleRequest {
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            runtime_mode: "cloud_enhanced".to_string(),
            prompt_hint: None,
        };

        let result = assemble(State(state), Json(req)).await;

        match result {
            Ok(Json(response)) => {
                assert!(response.memory_items.is_empty());
                assert!(response.kb.is_empty());
                assert!(response.timeline.is_empty());
                assert!(response.metadata.assembled_at.contains("202"));
                assert_eq!(response.metadata.token_count_estimate, Some(0));
            }
            Err(err) => panic!("Expected Ok for cloud_enhanced mode, got Err: {}", err),
        }
    }

    /// Test: assemble accepts mismatched runtime_mode (V1.2 behavior)
    #[tokio::test]
    async fn assemble_accepts_mismatched_runtime_mode() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        // WorkspaceState defaults to LocalOnly runtime mode
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Request with cloud_enhanced (different from daemon's default LocalOnly)
        let req = AssembleRequest {
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            runtime_mode: "cloud_enhanced".to_string(),
            prompt_hint: None,
        };

        let result = assemble(State(state), Json(req)).await;

        // V1.2: Should succeed even with mismatched mode (for testing flexibility)
        match result {
            Ok(Json(response)) => {
                assert!(response.memory_items.is_empty());
            }
            Err(err) => panic!(
                "V1.2 should allow mismatched modes for testing, got Err: {}",
                err
            ),
        }
    }
}
