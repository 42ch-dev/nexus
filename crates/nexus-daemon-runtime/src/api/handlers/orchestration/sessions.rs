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
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use std::sync::Arc;

/// `POST /v1/daemon/orchestration/sessions` — create a new session from a preset.
pub async fn create_session(
    State(state): State<WorkspaceState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), NexusApiError> {
    // I-4: a session POST requires a complete Creator-DB runtime bundle
    // (engine + coordinator). Tier-0 in-memory user runs are never created —
    // the daemon must have a creator DB (boot or lazy attach) before a
    // public session can be driven.
    //
    // I-5/N-4: `CreateSessionRequest.agentBindings` (camelCase) carries the
    // role → provider binding map; the coordinator freezes it into the run
    // descriptor and refuses unknown role/provider references before
    // enqueue. Absent bindings behave as an empty map (the `default` role
    // must then be bound by the preset's own admission path).
    let coordinator = state
        .run_coordinator()
        .ok_or_else(|| NexusApiError::service_unavailable("run coordinator not configured"))?;
    let _engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;
    let caps = state
        .capability_registry_holder()
        .ok_or_else(|| NexusApiError::service_unavailable("capability registry not available"))?;

    // I-4: resolve via the shared public resolver (embedded, user, and
    // system directory presets) — not `load_embedded_preset` only. The
    // coordinator's `start_session` freezes the supplied seed/input and
    // agent bindings into the v1 admission path and drives the run —
    // mandatory, never optional. Unknown role/provider references are
    // refused before enqueue (N-4).
    let agent_bindings = body
        .agent_bindings
        .as_ref()
        .map(|bindings| {
            bindings
                .iter()
                .map(|(role, dto)| {
                    (
                        role.clone(),
                        nexus_orchestration::run_state::AgentBinding {
                            provider_id: dto.provider_id.clone(),
                            model: dto.model.clone(),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let session_id = coordinator
        .start_session(
            &body.preset_id,
            &body.creator_id,
            body.seed.as_deref(),
            agent_bindings,
            state.nexus_home(),
            &caps,
            state.daemon_tool_dispatch(),
            state.prompt_executor(),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not eligible")
                || msg.contains("system preset")
                || msg.contains("unknown role")
                || msg.contains("unknown provider")
                || msg.contains("invalid agent binding")
                || msg.contains("missing agent binding")
            {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: msg,
                }
            } else {
                NexusApiError::Internal {
                    code: "RUN_CONTROL_ERROR".into(),
                    message: msg,
                }
            }
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

    let session = if let Some(session) = sessions.into_iter().find(|s| s.session_id == sid) {
        SessionSummary {
            session_id: session.session_id.0,
            creator_id: session.creator_id,
            preset_id: session.preset_id,
            status: session_status_to_str(&session.status),
            current_task_id: session.current_task_id,
        }
    } else {
        let pool = state.pool_or_uninit()?;
        let storage = SqliteSessionStorage::new(Arc::new(pool.clone()));
        let row = storage
            .get_checkpoint_row(&session_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "STORAGE_ERROR".into(),
                message: e.to_string(),
            })?
            .ok_or_else(|| NexusApiError::NotFound(format!("session {session_id}")))?;
        SessionSummary {
            session_id: row.session_id,
            creator_id: row.creator_id,
            preset_id: row.preset_id,
            status: row.status,
            current_task_id: row.current_task_id,
        }
    };

    Ok(Json(GetSessionResponse { session }))
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
        "continue" => {
            let wait_id = body.wait_id.as_deref().ok_or_else(|| {
                NexusApiError::BadRequestCodedDetails {
                    code: "invalid_input".into(),
                    message: "continue requires the exact durable wait token".into(),
                    details: serde_json::json!({
                        "field": "waitId",
                        "reason": "continue requires the exact durable wait token",
                    }),
                }
            })?;
            EngineSignal::Continue {
                wait_id: wait_id.to_string(),
            }
        }
        other => {
            return Err(NexusApiError::BadRequest {
                code: "invalid_signal".into(),
                message: format!(
                    "invalid signal: '{other}' — expected pause|resume|cancel|advance|continue"
                ),
            });
        }
    };

    let sid = nexus_orchestration::engine::SessionId(session_id);

    // `continue` must route through the coordinator so the run is re-driven
    // after the wait CAS (single-flight). Other signals go through the
    // engine's revision-fenced transition path directly.
    if body.signal == "continue" {
        let coordinator = state.run_coordinator().ok_or_else(|| {
            NexusApiError::service_unavailable("run coordinator not configured")
        })?;
        let wait_id = body.wait_id.as_deref().ok_or_else(|| {
            NexusApiError::BadRequestCodedDetails {
                code: "invalid_input".into(),
                message: "continue requires the exact durable wait token".into(),
                details: serde_json::json!({
                    "field": "waitId",
                    "reason": "continue requires the exact durable wait token",
                }),
            }
        })?;
        let result = coordinator
            .signal_run(
                &sid,
                crate::preset_run::RunSignal::Continue {
                    wait_id: wait_id.to_string(),
                },
            )
            .await
            .map_err(|e| match e {
                crate::preset_run::RunControlError::WaitConflict {
                    session_id,
                    status,
                    current_wait_id,
                } => NexusApiError::ConflictCodedDetails {
                    code: "workflow_wait_conflict".into(),
                    message: format!(
                        "wait conflict for {session_id}: current status {status}, current_wait_id {current_wait_id:?}"
                    ),
                    details: serde_json::json!({
                        "session_id": session_id,
                        "status": status,
                        "current_wait_id": current_wait_id,
                    }),
                },
                crate::preset_run::RunControlError::StateConflict(sid, msg) => {
                    NexusApiError::ConflictCoded {
                        code: "workflow_state_conflict".into(),
                        message: format!("state conflict for {sid}: {msg}"),
                    }
                }
                crate::preset_run::RunControlError::ScheduleNotFound(sid) => {
                    NexusApiError::NotFound(format!("session {sid} not found"))
                }
                other => NexusApiError::Internal {
                    code: "RUN_CONTROL_ERROR".into(),
                    message: other.to_string(),
                },
            })?;
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "signal": "continue",
                "status": result.status,
                "current_wait_id": result.current_wait_id,
            })),
        ));
    }

    engine.signal(&sid, signal).await.map_err(
        |e: nexus_orchestration::engine::EngineError| match e {
            nexus_orchestration::engine::EngineError::SessionNotFound(_) => {
                NexusApiError::NotFound("session not found".into())
            }
            nexus_orchestration::engine::EngineError::WaitConflict {
                session_id,
                status,
                current_wait_id,
            } => NexusApiError::ConflictCoded {
                code: "workflow_wait_conflict".into(),
                message: format!(
                    "wait conflict for {session_id}: current status: {}, current_wait_id: {}",
                    status.as_db_str(),
                    current_wait_id.unwrap_or_else(|| "<none>".to_string())
                ),
            },
            nexus_orchestration::engine::EngineError::TerminalState(sid) => {
                NexusApiError::ConflictCoded {
                    code: "workflow_state_conflict".into(),
                    message: format!("state conflict for {sid}: run is terminal or not in a signalable state"),
                }
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
        SessionStatus::Cancelled => "cancelled".to_string(),
        SessionStatus::Interrupted => "interrupted".to_string(),
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
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
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
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
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
