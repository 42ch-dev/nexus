//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Session handlers: list, get, signal, create.

use crate::api::errors::NexusApiError;
use crate::api::pagination::{decode_offset_cursor, encode_offset_cursor};
use crate::api::sort::{compare_by_terms, parse_sort_terms};
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

/// `POST /v1/daemon/orchestration/sessions` — create a new session from a preset.
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

/// `GET /v1/daemon/orchestration/sessions`
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
        // AD-P0-2b (V1.120 P2 / F3): Sessions is an active-work monitor — hide
        // daemon-internal `_system.*` preset sessions auto-started at boot
        // (boot.rs WS-D) so an idle daemon yields an empty list. Filter runs
        // before map/sort/paginate; `list_active` non-terminal semantics are
        // unchanged (additive exclusion only).
        .filter(|s| !s.preset_id.starts_with("_system."))
        .map(|s| SessionSummary {
            session_id: s.session_id.0,
            creator_id: s.creator_id,
            preset_id: s.preset_id,
            status: session_status_to_str(&s.status),
            current_task_id: s.current_task_id,
        })
        .collect();

    // F-F1: apply server-side sort (in-memory; active-session lists are small).
    mapped.sort_by(|a, b| {
        compare_by_terms(a, b, &sort_terms, |key, a, b| match key {
            "session_id" => Some(a.session_id.cmp(&b.session_id)),
            "creator_id" => Some(a.creator_id.cmp(&b.creator_id)),
            "preset_id" => Some(a.preset_id.cmp(&b.preset_id)),
            "status" => Some(a.status.cmp(&b.status)),
            _ => None,
        })
    });

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

/// `GET /v1/daemon/orchestration/sessions/{session_id}`
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

/// `POST /v1/daemon/orchestration/sessions/{session_id}/signal`
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use std::sync::Arc;

    /// Build a minimal graph with a single manual-wait task (sessions stay
    /// non-terminal without executing any step).
    fn manual_wait_graph(name: &str) -> Arc<graph_flow::Graph> {
        let graph = Arc::new(graph_flow::Graph::new(name));
        graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
        graph
    }

    /// AD-P0-2b (V1.120 P2 / F3): daemon auto-started `_system.*` boot sessions
    /// must not leak into the Sessions product surface. Seeds one `_system.*`
    /// session (mirroring boot.rs WS-D) plus one author-started session and
    /// asserts the list hides the former and keeps the latter.
    #[tokio::test]
    async fn list_sessions_excludes_system_preset_sessions() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
        let caps = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
        let engine = nexus_orchestration::GraphFlowEngine::new_with_storage(storage, caps);

        // Mirror boot.rs WS-D: `_system.*` preset session auto-started at boot.
        engine
            .start_session("_system.maintenance", manual_wait_graph("system-boot"))
            .await
            .expect("system session start");
        // Author-started orchestration session.
        engine
            .start_session("novel-writing", manual_wait_graph("author-work"))
            .await
            .expect("author session start");

        state.set_engine(Arc::new(engine));

        let Json(body) = list_sessions(State(state), Query(ListSessionsQuery::default()))
            .await
            .expect("list_sessions should succeed");

        let preset_ids: Vec<&str> = body.items.iter().map(|s| s.preset_id.as_str()).collect();
        assert!(
            preset_ids.iter().all(|p| !p.starts_with("_system.")),
            "system preset sessions must be hidden, got: {preset_ids:?}"
        );
        assert!(
            preset_ids.contains(&"novel-writing"),
            "author non-terminal session must remain, got: {preset_ids:?}"
        );
    }

    /// AC-P2-1: idle daemon with only `_system.*` boot sessions active ⇒ the
    /// product list is empty (Sessions is an active-work monitor).
    #[tokio::test]
    async fn list_sessions_idle_daemon_yields_empty_list() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
        let caps = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
        let engine = nexus_orchestration::GraphFlowEngine::new_with_storage(storage, caps);

        engine
            .start_session("_system.maintenance", manual_wait_graph("system-boot-a"))
            .await
            .expect("system session start");
        engine
            .start_session("_system.health", manual_wait_graph("system-boot-b"))
            .await
            .expect("system session start");

        state.set_engine(Arc::new(engine));

        let Json(body) = list_sessions(State(state), Query(ListSessionsQuery::default()))
            .await
            .expect("list_sessions should succeed");

        assert!(
            body.items.is_empty(),
            "idle daemon (only _system.* sessions) must yield zero rows, got: {:?}",
            body.items.iter().map(|s| &s.preset_id).collect::<Vec<_>>()
        );
    }
}
