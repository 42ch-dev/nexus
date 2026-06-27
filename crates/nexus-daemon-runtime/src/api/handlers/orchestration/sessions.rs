//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Session handlers: list, get, signal, create.

use crate::api::errors::NexusApiError;
use crate::api::pagination::{decode_offset_cursor, encode_offset_cursor};
use crate::api::sort::parse_sort_terms;
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
use nexus_contracts::PaginationInfo;
use nexus_orchestration::engine::{EngineSignal, SessionStatus};

/// `POST /v1/local/orchestration/sessions` — create a new session from a preset.
pub async fn create_session(
    State(state): State<WorkspaceState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), NexusApiError> {
    let engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;

    // Load the preset by ID.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset(&body.preset_id, &caps)
        .map_err(|e| NexusApiError::BadRequest {
            code: "preset_load_failed".into(),
            message: format!("failed to load preset '{}': {}", body.preset_id, e),
        })?;

    // Start session with the loaded preset.
    let session_id = engine
        .start_session_with_preset_for_creator(&loaded, &body.creator_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "ENGINE_ERROR".into(),
            message: e.to_string(),
        })?;

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
) -> Result<Json<ListSessionsResponse>, NexusApiError> {
    let engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;

    let sort_terms = parse_sort_terms(
        query.sort.as_deref(),
        &["session_id", "creator_id", "preset_id", "status"],
        "session",
    )?;

    let filter = nexus_orchestration::engine::SessionFilter {
        creator_id: query.creator_id,
        preset_id: None,
    };

    let sessions = engine
        .list_active(filter)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "ENGINE_ERROR".into(),
            message: e.to_string(),
        })?;

    let mut mapped: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|s| SessionSummary {
            session_id: s.session_id.0,
            creator_id: s.creator_id,
            preset_id: s.preset_id,
            status: session_status_to_str(&s.status),
            current_task_id: s.current_task_id,
        })
        .collect();

    // F-F1: apply server-side sort (in-memory; active-session lists are small).
    mapped.sort_by(|a, b| compare_session_summary(a, b, &sort_terms));

    // F-P1/F-P3: cursor pagination.
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit: u32 = query.limit.unwrap_or(100).min(500);
    let total = mapped.len();
    let start = usize::try_from(offset).unwrap_or(0).min(total);
    let end = start
        .saturating_add(usize::try_from(limit).unwrap_or(total))
        .min(total);
    let page_items: Vec<SessionSummary> = mapped.drain(start..end).collect();
    let has_more = end < total;
    let next_cursor = if has_more {
        Some(encode_offset_cursor(offset.saturating_add(limit)))
    } else {
        None
    };

    Ok(Json(ListSessionsResponse {
        items: page_items,
        pagination: PaginationInfo {
            limit: i64::from(limit),
            next_cursor,
            has_more,
        },
    }))
}

fn compare_session_summary(
    a: &SessionSummary,
    b: &SessionSummary,
    terms: &[(String, bool)],
) -> std::cmp::Ordering {
    for (key, ascending) in terms {
        let ord = match key.as_str() {
            "session_id" => a.session_id.cmp(&b.session_id),
            "creator_id" => a.creator_id.cmp(&b.creator_id),
            "preset_id" => a.preset_id.cmp(&b.preset_id),
            "status" => a.status.cmp(&b.status),
            _ => std::cmp::Ordering::Equal,
        };
        let ord = if *ascending { ord } else { ord.reverse() };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

/// `GET /v1/local/orchestration/sessions/{session_id}`
pub async fn get_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
) -> Result<Json<GetSessionResponse>, NexusApiError> {
    let engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;

    let sid = nexus_orchestration::engine::SessionId(session_id.clone());
    let sessions = engine
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "ENGINE_ERROR".into(),
            message: e.to_string(),
        })?;

    let session = sessions
        .into_iter()
        .find(|s| s.session_id == sid)
        .ok_or_else(|| NexusApiError::NotFound(format!("session {session_id}")))?;

    Ok(Json(GetSessionResponse {
        session: SessionSummary {
            session_id: session.session_id.0,
            creator_id: session.creator_id,
            preset_id: session.preset_id,
            status: session_status_to_str(&session.status),
            current_task_id: session.current_task_id,
        },
    }))
}

/// `POST /v1/local/orchestration/sessions/{session_id}/signal`
pub async fn signal_session(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
    Json(body): Json<SignalSessionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), NexusApiError> {
    let engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;

    let signal = match body.signal.as_str() {
        "pause" => EngineSignal::Pause,
        "resume" => EngineSignal::Resume,
        "cancel" => EngineSignal::Cancel,
        "advance" => EngineSignal::Advance,
        other => {
            return Err(NexusApiError::BadRequest {
                code: "invalid_signal".into(),
                message: format!(
                    "invalid signal: '{other}' — expected pause|resume|cancel|advance"
                ),
            });
        }
    };

    let sid = nexus_orchestration::engine::SessionId(session_id);
    engine.signal(&sid, signal).await.map_err(
        |e: nexus_orchestration::engine::EngineError| match e {
            nexus_orchestration::engine::EngineError::SessionNotFound(_) => {
                NexusApiError::NotFound("session not found".into())
            }
            other => NexusApiError::Internal {
                code: "ENGINE_ERROR".into(),
                message: other.to_string(),
            },
        },
    )?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"signal": body.signal, "status": "accepted"})),
    ))
}

/// Convert [`SessionStatus`] to the `snake_case` string expected by the API contract.
///
/// `Debug` formatting produces `WaitingForInput` → `waitingforinput` (no separator).
/// This function maps each variant explicitly to the correct `snake_case` form.
fn session_status_to_str(status: &SessionStatus) -> String {
    match status {
        SessionStatus::Running => "running".to_string(),
        SessionStatus::Paused => "paused".to_string(),
        SessionStatus::WaitingForInput => "waiting_for_input".to_string(),
        SessionStatus::Completed => "completed".to_string(),
        SessionStatus::Failed => "failed".to_string(),
    }
}
