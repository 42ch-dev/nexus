//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Agent Host API handlers.
//!
//! Endpoints:
//! - GET  /v1/local/agent-host/health          — Host health status
//! - GET  /v1/local/agent-host/providers        — List available providers
//! - POST /v1/local/agent-host/sessions         — Create a managed session
//! - GET  /v1/local/agent-host/sessions         — List active sessions
//! - DELETE /v1/local/agent-host/sessions/{id}  — Shutdown a session

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HostHealthResponse {
    pub running: bool,
    pub active_sessions: usize,
    pub active_operations: usize,
}

#[derive(Debug, Serialize)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderEntryResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProviderEntryResponse {
    pub provider_id: String,
    pub display_name: String,
    pub protocol_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub provider_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub provider_id: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionResponse>,
}

#[derive(Debug, Serialize)]
pub struct ShutdownSessionResponse {
    pub session_id: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the agent host facade from workspace state, or return an error.
fn get_host(
    state: &WorkspaceState,
) -> Result<Arc<dyn nexus_agent_host::HostFacade>, NexusApiError> {
    state.agent_host().ok_or_else(|| NexusApiError::Internal {
        code: "AGENT_HOST_NOT_CONFIGURED".into(),
        message: "agent host subsystem not initialized".into(),
    })
}

/// Map a `nexus_agent_host::HostError` to an API error.
fn map_host_error(e: &nexus_agent_host::HostError) -> NexusApiError {
    NexusApiError::Internal {
        code: "AGENT_HOST_ERROR".into(),
        message: e.to_string(),
    }
}

/// Parse a session ID path parameter as UUID.
///
/// Returns 400 Bad Request for malformed IDs.
fn parse_session_id(raw: &str) -> Result<Uuid, NexusApiError> {
    raw.parse::<Uuid>()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: format!("session_id must be a valid UUID, got: {raw}"),
        })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/local/agent-host/health
pub async fn health(
    State(state): State<WorkspaceState>,
) -> Result<Json<HostHealthResponse>, NexusApiError> {
    let host = get_host(&state)?;
    let health = host.health().await.map_err(|e| map_host_error(&e))?;

    Ok(Json(HostHealthResponse {
        running: health.running,
        active_sessions: health.active_sessions,
        active_operations: health.active_operations,
    }))
}

/// GET /v1/local/agent-host/providers
///
/// Returns a static placeholder for Wave 1 — actual provider discovery
/// integration will be added when the discovery service is wired.
pub async fn list_providers(
    State(state): State<WorkspaceState>,
) -> Result<Json<ProviderListResponse>, NexusApiError> {
    let _host = get_host(&state)?;

    // Wave 1: return empty list — provider catalog will be populated
    // from the discovery subsystem in a future iteration.
    Ok(Json(ProviderListResponse { providers: vec![] }))
}

/// POST /v1/local/agent-host/sessions
pub async fn create_session(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, NexusApiError> {
    let host = get_host(&state)?;

    let host_req = nexus_agent_host::capability::CreateSessionRequest {
        provider_id: nexus_agent_host::ProviderId::new(&req.provider_id),
        cwd: req.cwd.map_or_else(
            || std::path::PathBuf::from("/tmp"),
            std::path::PathBuf::from,
        ),
        model: req.model,
        mode: req.mode,
        mcp_servers: vec![],
        metadata: serde_json::Value::Null,
    };

    let session = host
        .create_session(host_req)
        .await
        .map_err(|e| map_host_error(&e))?;

    Ok(Json(SessionResponse {
        session_id: session.id.to_string(),
        provider_id: session.provider_id.to_string(),
        state: format!("{:?}", session.state),
    }))
}

/// GET /v1/local/agent-host/sessions
pub async fn list_sessions(
    State(state): State<WorkspaceState>,
) -> Result<Json<SessionListResponse>, NexusApiError> {
    let host = get_host(&state)?;
    let health = host.health().await.map_err(|e| map_host_error(&e))?;

    // Wave 1: return count-based info — full session listing
    // requires registry iteration which will be added later.
    Ok(Json(SessionListResponse {
        sessions: (0..health.active_sessions)
            .map(|_| SessionResponse {
                session_id: String::new(),
                provider_id: String::new(),
                state: "active".to_string(),
            })
            .collect(),
    }))
}

/// DELETE /v1/local/agent-host/sessions/{id}
pub async fn shutdown_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<ShutdownSessionResponse>, NexusApiError> {
    // Validate session ID format at handler boundary.
    let uuid = parse_session_id(&session_id)?;

    let host = get_host(&state)?;

    // Wave 1: shutdown the entire host — per-session shutdown
    // requires session-to-provider routing exposed through the facade.
    // Once per-session lookup is available, return 404 for valid-but-missing UUIDs.
    let _ = uuid; // Used for validation only until per-session routing exists.
    host.shutdown().await.map_err(|e| map_host_error(&e))?;

    Ok(Json(ShutdownSessionResponse {
        session_id,
        status: "shutdown".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use crate::workspace::WorkspaceState;
    use nexus_agent_host::core::manager::HostManager;

    async fn state_with_host() -> WorkspaceState {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let host: Arc<dyn nexus_agent_host::HostFacade> = Arc::new(HostManager::new());
        state.set_agent_host(host);
        state
    }

    #[tokio::test]
    async fn health_returns_ok_when_host_available() {
        let state = state_with_host().await;
        let result = health(State(state)).await;
        assert!(result.is_ok());
        let resp = result.expect("health should succeed");
        assert!(!resp.running); // HostManager starts as not-running
    }

    #[tokio::test]
    async fn health_returns_error_when_host_not_configured() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = health(State(state)).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code(), "INTERNAL");
    }

    #[tokio::test]
    async fn list_providers_returns_empty() {
        let state = state_with_host().await;
        let result = list_providers(State(state)).await;
        assert!(result.is_ok());
        let resp = result.expect("providers should succeed");
        assert!(resp.providers.is_empty());
    }

    #[tokio::test]
    async fn create_session_fails_for_unknown_provider() {
        let state = state_with_host().await;
        let req = CreateSessionRequest {
            provider_id: "nonexistent".to_string(),
            cwd: Some("/tmp".to_string()),
            model: None,
            mode: None,
        };
        let result = create_session(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn shutdown_session_rejects_invalid_uuid() {
        let state = state_with_host().await;
        let result = shutdown_session(State(state), Path("not-a-uuid".to_string())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn shutdown_session_rejects_empty_session_id() {
        let state = state_with_host().await;
        let result = shutdown_session(State(state), Path(String::new())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn shutdown_session_rejects_partial_uuid() {
        let state = state_with_host().await;
        let result = shutdown_session(State(state), Path("550e8400-e29b-41d4".to_string())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn parse_session_id_accepts_valid_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_session_id(uuid);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid);
    }

    #[tokio::test]
    async fn parse_session_id_rejects_invalid() {
        assert!(parse_session_id("garbage").is_err());
        assert!(parse_session_id("").is_err());
        assert!(parse_session_id("12345").is_err());
        assert!(parse_session_id("../../etc/passwd").is_err());
    }
}
