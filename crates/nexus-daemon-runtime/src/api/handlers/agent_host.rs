#![allow(clippy::missing_errors_doc)]
//! Agent Host API handlers.
//!
//! Endpoints:
//! - GET    /v1/local/agent-host/health                           — Host health status
//! - GET    /v1/local/agent-host/providers                        — List available providers
//! - POST   /v1/local/agent-host/sessions                         — Create a managed session
//! - GET    /v1/local/agent-host/sessions                         — List active sessions (with pagination)
//! - GET    /v1/local/agent-host/sessions/{session_id}            — Get session detail
//! - DELETE /v1/local/agent-host/sessions/{session_id}            — Shutdown a single session
//! - POST   /v1/local/agent-host/sessions/{session_id}/operations — Execute a host operation
//! - POST   /v1/local/agent-host/operations/{operation_id}:cancel — Cancel in-flight operation
//! - GET    /v1/local/agent-host/sessions/{session_id}/events     — SSE event stream

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::StreamExt;
use nexus_contracts::PaginationInfo;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;
use uuid::Uuid;

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

// ---------------------------------------------------------------------------
// Response / Request types
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
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_op_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub items: Vec<SessionResponse>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub cursor: Option<String>,
}

const fn default_limit() -> usize {
    50
}

#[derive(Debug, Serialize)]
pub struct ShutdownSessionResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct OperationResponse {
    pub operation_id: String,
    pub session_id: String,
    pub status: String,
}

/// Request body for executing a host operation on a session.
/// Tagged by `kind`: `"prompt"`, `"set_model"`, or `"set_mode"`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecuteOperationRequest {
    Prompt { content: String },
    SetModel { model: String },
    SetMode { mode: String },
}

#[derive(Debug, Serialize)]
pub struct CancelOperationResponse {
    pub operation_id: String,
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

/// Map a `nexus_agent_host::HostError` to an API error with appropriate
/// HTTP status codes based on the error category.
fn map_host_error(e: &nexus_agent_host::HostError) -> NexusApiError {
    match e.category() {
        "provider_unavailable" => NexusApiError::NotFound(e.to_string()),
        "capability_unsupported" => NexusApiError::InvalidInput {
            field: "operation".into(),
            reason: e.to_string(),
        },
        "policy_denied" => NexusApiError::Forbidden {
            resource: "agent_host".into(),
            reason: e.to_string(),
        },
        _ => NexusApiError::Internal {
            code: "AGENT_HOST_ERROR".into(),
            message: e.to_string(),
        },
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

/// Parse an operation ID path parameter as UUID.
///
/// Returns 400 Bad Request for malformed IDs.
fn parse_operation_id(raw: &str) -> Result<Uuid, NexusApiError> {
    raw.parse::<Uuid>()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "operation_id".into(),
            reason: format!("operation_id must be a valid UUID, got: {raw}"),
        })
}

/// Map a session's active op ID to a display string.
fn active_op_display(session: &nexus_agent_host::core::session::HostSession) -> Option<String> {
    session
        .active_op_id
        .as_ref()
        .map(std::string::ToString::to_string)
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
/// Returns the real provider catalog from the agent host subsystem.
pub async fn list_providers(
    State(state): State<WorkspaceState>,
) -> Result<Json<ProviderListResponse>, NexusApiError> {
    let host = get_host(&state)?;
    let catalog = host
        .provider_catalog()
        .await
        .map_err(|e| map_host_error(&e))?;

    let providers = catalog
        .entries
        .into_iter()
        .map(|entry| ProviderEntryResponse {
            provider_id: entry.provider_id.to_string(),
            display_name: entry.display_name,
            protocol_kind: format!("{:?}", entry.protocol_kind),
            available: entry.health.available,
            latency_ms: entry.health.latency_ms,
            message: entry.health.message,
        })
        .collect();

    Ok(Json(ProviderListResponse { providers }))
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
        model: req.model.clone(),
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
        active_op_id: None,
        model: req.model,
    }))
}

/// GET /v1/local/agent-host/sessions
///
/// Returns real session registry from agent host with pagination.
pub async fn list_sessions(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, NexusApiError> {
    let host = get_host(&state)?;
    let sessions = host.list_sessions().await.map_err(|e| map_host_error(&e))?;

    let limit = params.limit.clamp(1, 250);

    // Cursor-based pagination: cursor is a session ID (UUID string).
    // If cursor is provided, skip entries until we find the cursor,
    // then return up to `limit` entries after it.
    let items: Vec<SessionResponse> = sessions
        .into_iter()
        .skip_while(|s| {
            params
                .cursor
                .as_ref()
                .is_some_and(|cursor| s.id.to_string() <= *cursor)
        })
        .take(limit)
        .map(|s| SessionResponse {
            session_id: s.id.to_string(),
            provider_id: s.provider_id.to_string(),
            state: format!("{:?}", s.state),
            active_op_id: active_op_display(&s),
            model: None,
        })
        .collect();

    let next_cursor = if items.len() == limit {
        items.last().map(|i| i.session_id.clone())
    } else {
        None
    };

    Ok(Json(SessionListResponse {
        items,
        pagination: PaginationInfo {
            limit: i64::try_from(limit).unwrap_or(i64::MAX),
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

/// GET /v1/local/agent-host/sessions/{session_id}
pub async fn get_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, NexusApiError> {
    let uuid = parse_session_id(&session_id)?;
    let host = get_host(&state)?;

    let sessions = host.list_sessions().await.map_err(|e| map_host_error(&e))?;
    let session = sessions
        .into_iter()
        .find(|s| s.id.0 == uuid)
        .ok_or_else(|| NexusApiError::NotFound(format!("session {session_id}")))?;

    Ok(Json(SessionResponse {
        session_id: session.id.to_string(),
        provider_id: session.provider_id.to_string(),
        state: format!("{:?}", session.state),
        active_op_id: active_op_display(&session),
        model: None,
    }))
}

/// DELETE /v1/local/agent-host/sessions/{session_id}
///
/// Shuts down a single session. The host remains running.
/// Returns 404 if the session does not exist.
pub async fn shutdown_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<ShutdownSessionResponse>, NexusApiError> {
    let uuid = parse_session_id(&session_id)?;
    let host = get_host(&state)?;

    let sid = nexus_agent_host::HostSessionId(uuid);
    host.shutdown_session(sid)
        .await
        .map_err(|e| map_host_error(&e))?;

    Ok(Json(ShutdownSessionResponse {
        session_id,
        status: "shutdown".to_string(),
    }))
}

/// POST /v1/local/agent-host/sessions/{session_id}/operations
///
/// Execute a normalized host operation (prompt, `set_model`, `set_mode`).
/// Returns the operation ID for tracking.
pub async fn execute_operation(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteOperationRequest>,
) -> Result<Json<OperationResponse>, NexusApiError> {
    let uuid = parse_session_id(&session_id)?;
    let host = get_host(&state)?;

    let sid = nexus_agent_host::HostSessionId(uuid);
    let op_id = nexus_agent_host::HostOperationId::new();

    let host_op = match req {
        ExecuteOperationRequest::Prompt { content } => {
            nexus_agent_host::capability::model::HostOperation::Prompt {
                op_id: op_id.clone(),
                content: vec![
                    nexus_agent_host::capability::model::HostContentBlock::Text { text: content },
                ],
            }
        }
        ExecuteOperationRequest::SetModel { model } => {
            nexus_agent_host::capability::model::HostOperation::SetModel { model }
        }
        ExecuteOperationRequest::SetMode { mode } => {
            nexus_agent_host::capability::model::HostOperation::SetMode { mode }
        }
    };

    // Execute the operation and return immediately (QC3 W-003: fire-and-forget).
    // The wrapped stream in HostManager handles Busy→Ready state transitions.
    // SSE subscribers receive events via the broadcast channel.
    let stream = host
        .exec(sid.clone(), host_op)
        .await
        .map_err(|e| map_host_error(&e))?;

    // Spawn background task to drain the event stream and drive the state machine.
    // This prevents blocking the HTTP handler for the duration of long-running operations.
    tokio::spawn(async move {
        let mut s = stream;
        while let Some(_result) = s.next().await {
            // Events are broadcast by HostManager; draining drives the state machine.
        }
    });

    Ok(Json(OperationResponse {
        operation_id: op_id.to_string(),
        session_id: sid.to_string(),
        status: "started".to_string(),
    }))
}

/// POST /v1/local/agent-host/operations/{operation_id}:cancel
///
/// Cancel an in-flight operation.
pub async fn cancel_operation(
    State(state): State<WorkspaceState>,
    Path(operation_id): Path<String>,
) -> Result<Json<CancelOperationResponse>, NexusApiError> {
    let uuid = parse_operation_id(&operation_id)?;
    let host = get_host(&state)?;

    let op_id = nexus_agent_host::HostOperationId(uuid);
    host.cancel(op_id).await.map_err(|e| map_host_error(&e))?;

    Ok(Json(CancelOperationResponse {
        operation_id,
        status: "cancelled".to_string(),
    }))
}

/// GET /v1/local/agent-host/sessions/{session_id}/events
///
/// SSE endpoint that delivers `HostEvent` variants for a session.
/// Compatible with the browser `EventSource` API.
///
/// Subscribes to the broadcast channel in `HostManager` and filters events
/// by the requested session ID. Events are serialized as JSON in `data:` lines.
pub async fn session_events(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, NexusApiError> {
    let uuid = parse_session_id(&session_id)?;
    let host = get_host(&state)?;

    let sid = nexus_agent_host::HostSessionId(uuid);
    let rx = host.subscribe_events(sid.clone());

    // Convert the broadcast receiver into a filtered SSE stream using unfold.
    // We manually recv() from the broadcast receiver and yield matching events.
    // The unfold state is (receiver, done_flag) — once `done` is true the stream
    // terminates on the next poll (QC3 W-002: prevent zombie SSE connections).
    let stream = futures_util::stream::unfold((rx, false), move |(mut rx, done)| {
        let sid = sid.clone();
        async move {
            if done {
                return None;
            }
            // Keep receiving until we get a session-matching event or the channel closes.
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event_matches_session(&event, &sid) {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            let is_terminal = matches!(
                                &event,
                                nexus_agent_host::capability::model::HostEvent::SessionStopped(e)
                                if e.session_id == sid
                            );
                            return Some((
                                Ok::<Event, Infallible>(Event::default().data(json)),
                                (rx, is_terminal),
                            ));
                        }
                        // Not our session — skip and continue
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "SSE broadcast lagged — client may have missed events"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Channel closed — end the stream
                        return None;
                    }
                }
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Check if a host event belongs to the given session.
fn event_matches_session(
    event: &nexus_agent_host::capability::model::HostEvent,
    sid: &nexus_agent_host::HostSessionId,
) -> bool {
    use nexus_agent_host::capability::model::HostEvent;
    match event {
        HostEvent::OpStarted(e) => &e.session_id == sid,
        HostEvent::OpFinished(e) => &e.session_id == sid,
        HostEvent::OpFailed(e) => &e.session_id == sid,
        HostEvent::ThoughtDelta(e) | HostEvent::MessageDelta(e) => &e.session_id == sid,
        HostEvent::ToolCall(e) => &e.session_id == sid,
        HostEvent::ToolCallUpdate(e) => &e.session_id == sid,
        HostEvent::PlanUpdate(e) => &e.session_id == sid,
        HostEvent::SessionCreated(e) => &e.session_id == sid,
        HostEvent::SessionStopped(e) => &e.session_id == sid,
        HostEvent::Status(_) => true, // global events pass through
    }
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
        assert_eq!(err.error_code(), "internal");
    }

    #[tokio::test]
    async fn list_providers_returns_empty_when_no_providers() {
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
    async fn list_sessions_returns_empty_when_no_sessions() {
        let state = state_with_host().await;
        let result = list_sessions(
            State(state),
            Query(ListSessionsQuery {
                limit: 50,
                cursor: None,
            }),
        )
        .await;
        assert!(result.is_ok());
        let resp = result.expect("sessions should succeed");
        assert!(resp.items.is_empty());
        assert!(resp.pagination.next_cursor.is_none());
    }

    #[tokio::test]
    async fn list_sessions_respects_limit() {
        let state = state_with_host().await;
        let result = list_sessions(
            State(state),
            Query(ListSessionsQuery {
                limit: 1,
                cursor: None,
            }),
        )
        .await;
        assert!(result.is_ok());
        let resp = result.expect("sessions should succeed");
        assert!(resp.items.len() <= 1);
    }

    #[tokio::test]
    async fn shutdown_session_rejects_invalid_uuid() {
        let state = state_with_host().await;
        let result = shutdown_session(State(state), Path("not-a-uuid".to_string())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "invalid_input");
    }

    #[tokio::test]
    async fn shutdown_session_rejects_empty_session_id() {
        let state = state_with_host().await;
        let result = shutdown_session(State(state), Path(String::new())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "invalid_input");
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
    async fn get_session_rejects_invalid_uuid() {
        let state = state_with_host().await;
        let result = get_session(State(state), Path("garbage".to_string())).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn get_session_returns_404_for_unknown() {
        let state = state_with_host().await;
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = get_session(State(state), Path(uuid.to_string())).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn execute_operation_rejects_invalid_session_uuid() {
        let state = state_with_host().await;
        let req = ExecuteOperationRequest::Prompt {
            content: "hello".to_string(),
        };
        let result = execute_operation(State(state), Path("bad-uuid".to_string()), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn cancel_operation_rejects_invalid_op_uuid() {
        let state = state_with_host().await;
        let result = cancel_operation(State(state), Path("bad-uuid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn session_events_rejects_invalid_session_uuid() {
        let state = state_with_host().await;
        let result = session_events(State(state), Path("bad-uuid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );
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

    #[tokio::test]
    async fn parse_operation_id_accepts_valid_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_operation_id(uuid);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid);
    }

    #[tokio::test]
    async fn parse_operation_id_rejects_invalid() {
        assert!(parse_operation_id("garbage").is_err());
        assert!(parse_operation_id("").is_err());
    }
}
