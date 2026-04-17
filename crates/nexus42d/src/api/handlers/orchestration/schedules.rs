//! Schedule HTTP handlers: 8 endpoints per WS7 §9.
//!
//! Endpoints:
//! - POST   /schedules — Add schedule
//! - GET    /schedules — List schedules (optional filters)
//! - GET    /schedules/{id} — Inspect schedule
//! - PATCH  /schedules/{id}/core-context — Apply EditOp
//! - GET    /schedules/{id}/core-context — Current content
//! - GET    /schedules/{id}/core-context-history — Version history
//! - POST   /schedules/{id}/signal — Pause/Resume/Cancel
//! - DELETE /schedules/{id} — Remove (terminal only)

use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use nexus_contracts::local::schedule::http::*;

/// `POST /v1/local/orchestration/schedules` — add a new schedule.
pub async fn add_schedule(
    _state: State<WorkspaceState>,
    Json(_body): Json<AddScheduleRequest>,
) -> Result<(StatusCode, Json<AddScheduleResponse>), (StatusCode, String)> {
    // TODO(T6): Delegate to ScheduleSupervisor + CoreContextManager
    // For now, return 501 Not Implemented as the supervisor wiring is in T9.
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `GET /v1/local/orchestration/schedules` — list schedules with optional filters.
pub async fn list_schedules(
    _state: State<WorkspaceState>,
    Query(_query): Query<ListSchedulesQuery>,
) -> Result<(StatusCode, Json<ListSchedulesResponse>), (StatusCode, String)> {
    // TODO(T6): Delegate to ScheduleSupervisor
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `GET /v1/local/orchestration/schedules/{schedule_id}` — inspect a schedule.
pub async fn inspect_schedule(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
) -> Result<Json<InspectScheduleResponse>, (StatusCode, String)> {
    // TODO(T6): Delegate to ScheduleSupervisor
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context` — apply EditOp.
pub async fn edit_core_context(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
    Json(_body): Json<EditCoreContextRequest>,
) -> Result<(StatusCode, Json<EditCoreContextResponse>), (StatusCode, String)> {
    // TODO(T6): Delegate to CoreContextManager
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `GET /v1/local/orchestration/schedules/{schedule_id}/core-context` — current content.
pub async fn get_core_context(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
) -> Result<Json<CoreContextResponse>, (StatusCode, String)> {
    // TODO(T6): Delegate to CoreContextManager
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `GET /v1/local/orchestration/schedules/{schedule_id}/core-context-history` — version history.
pub async fn get_core_context_history(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
) -> Result<Json<CoreContextHistoryResponse>, (StatusCode, String)> {
    // TODO(T6): Delegate to CoreContextManager
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `POST /v1/local/orchestration/schedules/{schedule_id}/signal` — pause/resume/cancel.
pub async fn signal_schedule(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
    Json(_body): Json<SignalScheduleRequest>,
) -> Result<(StatusCode, Json<SignalScheduleResponse>), (StatusCode, String)> {
    // TODO(T6): Delegate to ScheduleSupervisor
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

/// `DELETE /v1/local/orchestration/schedules/{schedule_id}` — remove terminal schedule.
pub async fn delete_schedule(
    _state: State<WorkspaceState>,
    Path(_schedule_id): Path<String>,
) -> Result<(StatusCode, Json<DeleteScheduleResponse>), (StatusCode, String)> {
    // TODO(T6): Delegate to ScheduleSupervisor
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "schedules endpoint requires supervisor wiring (T9)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_request_parse_valid_signals() {
        for signal in &["pause", "resume", "cancel", "start"] {
            let req = SignalScheduleRequest {
                signal: signal.to_string(),
            };
            let json = serde_json::to_string(&req).unwrap();
            let back: SignalScheduleRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(back.signal, *signal);
        }
    }
}
