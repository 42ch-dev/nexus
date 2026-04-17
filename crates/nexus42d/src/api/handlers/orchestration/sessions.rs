//! Session handlers: list, get, signal, create.

use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use nexus_contracts::local::orchestration::http::{
    CreateSessionRequest, CreateSessionResponse, GetSessionResponse, ListSessionsQuery,
    ListSessionsResponse, SessionSummary, SignalSessionRequest,
};
use nexus_orchestration::engine::{EngineSignal, SessionStatus};

/// `POST /v1/local/orchestration/sessions` — create a new session from a preset.
pub async fn create_session(
    State(state): State<WorkspaceState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), (StatusCode, String)> {
    let engine = state.engine().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "engine not available".into(),
        )
    })?;

    // Load the preset by ID.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset(&body.preset_id, &caps)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("failed to load preset '{}': {}", body.preset_id, e),
            )
        })?;

    // Start session with the loaded preset.
    let session_id = engine
        .start_session_with_preset(&loaded)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            session_id: session_id.0,
        }),
    ))
}

/// `GET /v1/local/orchestration/sessions`
pub async fn list_sessions(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListSessionsQuery>,
) -> (StatusCode, Json<ListSessionsResponse>) {
    let engine = match state.engine() {
        Some(e) => e,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ListSessionsResponse {
                    sessions: Vec::new(),
                }),
            );
        }
    };

    let filter = nexus_orchestration::engine::SessionFilter {
        creator_id: query.creator_id,
        preset_id: None,
    };

    let sessions = match engine.list_active(filter).await {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ListSessionsResponse {
                    sessions: Vec::new(),
                }),
            );
        }
    };

    let mapped: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|s| SessionSummary {
            session_id: s.session_id.0,
            creator_id: s.creator_id,
            preset_id: s.preset_id,
            status: session_status_to_str(s.status),
            current_task_id: s.current_task_id,
        })
        .collect();

    (
        StatusCode::OK,
        Json(ListSessionsResponse { sessions: mapped }),
    )
}

/// `GET /v1/local/orchestration/sessions/{session_id}`
pub async fn get_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<GetSessionResponse>, (StatusCode, String)> {
    let engine = state.engine().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "engine not available".into(),
        )
    })?;

    let sid = nexus_orchestration::engine::SessionId(session_id);
    let sessions = engine
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let session = sessions
        .into_iter()
        .find(|s| s.session_id == sid)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "session not found".into()))?;

    Ok(Json(GetSessionResponse {
        session: SessionSummary {
            session_id: session.session_id.0,
            creator_id: session.creator_id,
            preset_id: session.preset_id,
            status: session_status_to_str(session.status),
            current_task_id: session.current_task_id,
        },
    }))
}

/// `POST /v1/local/orchestration/sessions/{session_id}/signal`
pub async fn signal_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
    Json(body): Json<SignalSessionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let engine = state.engine().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "engine not available".into(),
        )
    })?;

    let signal = match body.signal.as_str() {
        "pause" => EngineSignal::Pause,
        "resume" => EngineSignal::Resume,
        "cancel" => EngineSignal::Cancel,
        "advance" => EngineSignal::Advance,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("invalid signal: '{other}' — expected pause|resume|cancel|advance"),
            ));
        }
    };

    let sid = nexus_orchestration::engine::SessionId(session_id);
    engine.signal(&sid, signal).await.map_err(
        |e: nexus_orchestration::engine::EngineError| match e {
            nexus_orchestration::engine::EngineError::SessionNotFound(_) => {
                (StatusCode::NOT_FOUND, "session not found".into())
            }
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        },
    )?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"signal": body.signal, "status": "accepted"})),
    ))
}

/// Convert [`SessionStatus`] to the snake_case string expected by the API contract.
///
/// `Debug` formatting produces `WaitingForInput` → `waitingforinput` (no separator).
/// This function maps each variant explicitly to the correct `snake_case` form.
fn session_status_to_str(status: SessionStatus) -> String {
    match status {
        SessionStatus::Running => "running".to_string(),
        SessionStatus::Paused => "paused".to_string(),
        SessionStatus::WaitingForInput => "waiting_for_input".to_string(),
        SessionStatus::Completed => "completed".to_string(),
        SessionStatus::Failed => "failed".to_string(),
    }
}
