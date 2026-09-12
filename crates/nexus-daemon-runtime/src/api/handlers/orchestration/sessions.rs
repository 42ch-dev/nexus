//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Session handlers: list, get, signal, create.

use crate::api::errors::NexusApiError;
use crate::api::pagination::{decode_offset_cursor, encode_offset_cursor};
use crate::api::sort::{compare_by_terms, parse_sort_terms};
use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use nexus_contracts::local::orchestration::http::{
    CreateSessionRequest, CreateSessionResponse, GetSessionResponse, ListSessionsQuery,
    ListSessionsResponse, SessionSummary, SignalSessionRequest,
};
use nexus_contracts::PaginationInfo;
use nexus_orchestration::engine::{EngineSignal, SessionStatus};
use nexus_orchestration::run_state::WorkflowStateStore;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::Stream;

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
            // QC2 F-004: the live-ring capacity refusal keeps its typed,
            // retryable envelope instead of collapsing into a generic 500.
            if matches!(e, crate::preset_run::RunControlError::RunEventCapacity(_)) {
                return NexusApiError::from(e);
            }
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
            failure_reason: None,
            execution: None,
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

    let sid = nexus_orchestration::engine::SessionId(session_id.to_string());
    // Shared durable execution projection (A2/A7): present for terminal rows
    // with no live runner; a load error projects unreadable, never a
    // fabricated class.
    let execution = match state.pool() {
        Some(pool) => Some(
            crate::execution_projection::project_for_session(
                pool,
                Some(&session_id),
                "driven_v1",
                state.capability_registry().as_ref(),
            )
            .await,
        ),
        None => None,
    };
    let sessions = engine
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "ENGINE_ERROR".into(),
            message: e.to_string(),
        })?;

    let session = if let Some(active) = sessions.into_iter().find(|s| s.session_id == sid) {
        // The in-memory active summary is a fast path, but it is NOT the
        // authoritative projection (Finding 2): after an unconfirmed cancel
        // cleanup the durable row is `interrupted` while the in-memory
        // summary may still say `running` (the engine updates the summary
        // only after the durable write succeeds, and the first failed
        // cancel returns the cleanup error before any summary flip). When a
        // creator DB is present, cross-check the durable row and prefer it
        // whenever it is terminal — the durable row is the SSOT.
        let durable_terminal = match state.pool() {
            Some(pool) => {
                let storage = SqliteSessionStorage::new(Arc::new(pool.clone()));
                match storage.get_checkpoint_row(&session_id).await {
                    Ok(Some(row))
                        if SessionStatus::from_db_str(&row.status)
                            .is_some_and(|s| s.is_terminal()) =>
                    {
                        Some(row)
                    }
                    _ => None,
                }
            }
            None => None,
        };
        if let Some(row) = durable_terminal {
            let failure_reason = failure_reason_for(&row);
            SessionSummary {
                session_id: row.session_id,
                creator_id: row.creator_id,
                preset_id: row.preset_id,
                status: row.status,
                current_task_id: row.current_task_id,
                failure_reason,
                execution: execution.clone(),
            }
        } else {
            SessionSummary {
                session_id: active.session_id.0,
                creator_id: active.creator_id,
                preset_id: active.preset_id,
                status: session_status_to_str(&active.status),
                current_task_id: active.current_task_id,
                failure_reason: None,
                execution: execution.clone(),
            }
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
        let failure_reason = failure_reason_for(&row);
        SessionSummary {
            session_id: row.session_id,
            creator_id: row.creator_id,
            preset_id: row.preset_id,
            status: row.status,
            current_task_id: row.current_task_id,
            failure_reason,
            execution: execution.clone(),
        }
    };

    Ok(Json(GetSessionResponse { session }))
}

/// Actionable failure reason from the durable v1 run state (e.g. an
/// unconfirmed cancel cleanup that left the run `interrupted`).
fn failure_reason_for(row: &nexus_orchestration::storage::CheckpointRow) -> Option<String> {
    row.run_state_json
        .as_deref()
        .and_then(|blob| {
            serde_json::from_slice::<nexus_orchestration::run_state::RunStateV1>(blob).ok()
        })
        .and_then(|state| state.failure)
        .map(|f| f.message)
}

/// `POST /v1/daemon/orchestration/sessions/{session_id}/signal`
#[allow(clippy::too_many_lines)] // handler covers many signal variants + per-variant error mapping; splitting adds indirection
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
            let wait_id =
                body.wait_id
                    .as_deref()
                    .ok_or_else(|| NexusApiError::BadRequestCodedDetails {
                        code: "invalid_input".into(),
                        message: "continue requires the exact durable wait token".into(),
                        details: serde_json::json!({
                            "field": "waitId",
                            "reason": "continue requires the exact durable wait token",
                        }),
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
        let coordinator = state
            .run_coordinator()
            .ok_or_else(|| NexusApiError::service_unavailable("run coordinator not configured"))?;
        let wait_id =
            body.wait_id
                .as_deref()
                .ok_or_else(|| NexusApiError::BadRequestCodedDetails {
                    code: "invalid_input".into(),
                    message: "continue requires the exact durable wait token".into(),
                    details: serde_json::json!({
                        "field": "waitId",
                        "reason": "continue requires the exact durable wait token",
                    }),
                })?;
        let result = coordinator
            .signal_run(
                &sid,
                crate::preset_run::RunSignal::Continue {
                    wait_id: wait_id.to_string(),
                },
            )
            .await
            .map_err(NexusApiError::from)?;
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "signal": "continue",
                "status": result.status,
                "current_wait_id": result.current_wait_id,
            })),
        ));
    }

    // QC2 F-002: Cancel has exactly ONE projection owner (the coordinator's
    // `cancel_run`). The durable outcome — confirmed `cancelled`, unconfirmed
    // `interrupted`, or the converged `driver_failed` — is a typed success;
    // the handler never string-matches an engine error and never turns an
    // actionable interrupted run into a 500.
    if body.signal == "cancel" {
        let coordinator = state
            .run_coordinator()
            .ok_or_else(|| NexusApiError::service_unavailable("run coordinator not configured"))?;
        let result = coordinator
            .cancel_run(&sid)
            .await
            .map_err(NexusApiError::from)?;
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "signal": "cancel",
                "status": result.status,
                "cancel_outcome": result.cancel_outcome.map(|o| o.as_str()),
            })),
        ));
    }

    let signal_result = engine.signal(&sid, signal).await;
    if let Err(e) = signal_result {
        return Err(match e {
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
                    message: format!(
                        "state conflict for {sid}: run is terminal or not in a signalable state"
                    ),
                }
            }
            // Finding 2: a CAS/revision loss is a linearized control race —
            // the durable winner is safe, but the losing public operation
            // must be an exact conflict response, never a 500. Reload the
            // authoritative row and project the exact envelope.
            nexus_orchestration::engine::EngineError::RevisionMismatch { session_id, .. } => {
                let conflict = match state.pool() {
                    Some(pool) => {
                        let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(
                            std::sync::Arc::new(pool.clone()),
                        );
                        match store
                            .load_run(&nexus_orchestration::engine::SessionId(session_id.clone()))
                            .await
                        {
                            Ok(Some(record)) => {
                                let state_blob = record.state.as_ref();
                                let cancel_requested =
                                    state_blob.is_some_and(|s| s.cancel_requested);
                                if record.status
                                    == nexus_orchestration::engine::SessionStatus::WaitingForInput
                                    && !cancel_requested
                                {
                                    let current_wait_id = state_blob
                                        .and_then(|s| s.wait.as_ref())
                                        .map(|w| w.wait_id.clone());
                                    NexusApiError::ConflictCoded {
                                        code: "workflow_wait_conflict".into(),
                                        message: format!(
                                            "wait conflict for {session_id}: current status: {}, current_wait_id: {}",
                                            record.status.as_db_str(),
                                            current_wait_id.unwrap_or_else(|| "<none>".to_string())
                                        ),
                                    }
                                } else {
                                    NexusApiError::ConflictCoded {
                                        code: "workflow_state_conflict".into(),
                                        message: format!(
                                            "state conflict for {session_id}: revision moved; current status is {}",
                                            record.status.as_db_str()
                                        ),
                                    }
                                }
                            }
                            _ => NexusApiError::ConflictCoded {
                                code: "workflow_state_conflict".into(),
                                message: format!("state conflict for {session_id}: revision moved"),
                            },
                        }
                    }
                    None => NexusApiError::ConflictCoded {
                        code: "workflow_state_conflict".into(),
                        message: format!("state conflict for {session_id}: revision moved"),
                    },
                };
                conflict
            }
            other => NexusApiError::Internal {
                code: "ENGINE_ERROR".into(),
                message: other.to_string(),
            },
        });
    }

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
        Arc::new(
            graph_flow::GraphBuilder::new(name)
                .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
                .build()
                .expect("manual-wait test graph"),
        )
    }

    /// AD-P0-2b (V1.120 P2 / F3): daemon auto-started `_system.*` boot sessions
    /// must not leak into the Sessions product surface. Seeds one `_system.*`
    /// session (mirroring boot.rs WS-D) plus one author-started session and
    /// asserts the list hides the former and keeps the latter.
    #[tokio::test]
    async fn list_sessions_excludes_system_preset_sessions() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

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
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

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

    /// Finding 2 (public projection): the durable row is the SSOT. When the
    /// durable row is terminal (`interrupted`) but the in-memory summary is
    /// stale non-terminal (`running`) — e.g. a durable write that succeeded
    /// outside the engine's summary-update path — `get_session` must project
    /// the authoritative durable status with the actionable cleanup reason,
    /// never the stale in-memory `running`.
    #[tokio::test]
    async fn get_session_projects_durable_interrupted_after_failed_cleanup() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let pool = state.pool().expect("test pool").clone();
        let sqlite = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn graph_flow::SessionStorage> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        state.set_engine(Arc::new(engine));

        // Start a v1 run (in-memory summary: `running`).
        let engine = state.engine().expect("engine set");
        let graph = manual_wait_graph("public-interrupted");
        let session_id = engine
            .start_session_with_graph("novel-writing", graph)
            .await
            .expect("start session");

        // The durable row becomes `interrupted` with the actionable cleanup
        // failure record — written directly through the store, as an
        // unconfirmed cancel cleanup would (the engine's summary-update path
        // is bypassed, leaving the in-memory summary stale `running`).
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let interrupted_state = nexus_orchestration::run_state::RunStateV1 {
            cancel_requested: true,
            in_flight: None,
            failure: Some(nexus_orchestration::run_state::RunFailure {
                code: "cancel_cleanup_unconfirmed".to_string(),
                message: "cancel cleanup unconfirmed for run 'x': shutdown timeout".to_string(),
            }),
            ..nexus_orchestration::run_state::RunStateV1::default()
        };
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                nexus_orchestration::engine::SessionStatus::Interrupted,
                &interrupted_state,
            )
            .await
            .expect("durable interrupted write");

        // The in-memory summary is stale `running` (the engine's signal path
        // updates it only after a durable success through that path).
        let in_memory = engine
            .list_active(nexus_orchestration::engine::SessionFilter::default())
            .await
            .expect("list active");
        assert!(
            in_memory.iter().any(|s| s.status == SessionStatus::Running),
            "in-memory summary is stale running (durable write bypassed the summary path)"
        );

        // The public projection must expose the durable `interrupted` status
        // with the actionable reason — never the stale in-memory `running`.
        let Json(body) = get_session(State(state), Path(session_id.0.clone()))
            .await
            .expect("get_session should succeed");
        assert_eq!(body.session.status, "interrupted");
        assert!(
            body.session
                .failure_reason
                .as_deref()
                .is_some_and(|r| r.contains("cancel cleanup unconfirmed")),
            "public projection must carry the actionable cleanup reason, got: {:?}",
            body.session.failure_reason
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 3 — Finding 4: public Interrupted projection is
    // exercised from the failed-cancel path
    // ------------------------------------------------------------------

    /// Scripted executor for the public-surface regression: the first
    /// `finalize_run` fails (cleanup unconfirmed), later calls succeed.
    struct ScriptedFailFirstExecutor {
        calls: std::sync::atomic::AtomicU64,
    }

    impl ScriptedFailFirstExecutor {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: std::sync::atomic::AtomicU64::new(0),
            })
        }
    }

    #[async_trait::async_trait]
    impl nexus_orchestration::capability::PromptExecutor for ScriptedFailFirstExecutor {
        async fn execute(
            &self,
            _request: nexus_orchestration::capability::PromptRequest,
        ) -> Result<
            nexus_orchestration::capability::PromptResult,
            nexus_orchestration::capability::CapabilityError,
        > {
            unreachable!("not used by the public cancel regression")
        }

        async fn finalize_run(
            &self,
            _run_id: &str,
        ) -> Result<(), nexus_orchestration::capability::CapabilityError> {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                Err(nexus_orchestration::capability::CapabilityError::Internal(
                    "shutdown timeout".to_string(),
                ))
            } else {
                Ok(())
            }
        }
    }

    /// Store wrapper that makes EVERY `Interrupted` commit fail its revision
    /// CAS (bounded-exhaustion): the engine's retry bound is exhausted and
    /// the public result must be a conflict/error — never an apparent
    /// successful cancellation.
    struct InterruptedWriteExhaustingStore {
        inner: Arc<dyn WorkflowStateStore>,
        storage: Arc<dyn graph_flow::SessionStorage>,
    }

    impl InterruptedWriteExhaustingStore {
        fn new(
            inner: Arc<dyn WorkflowStateStore>,
            storage: Arc<dyn graph_flow::SessionStorage>,
        ) -> Self {
            Self { inner, storage }
        }
    }

    #[async_trait::async_trait]
    impl WorkflowStateStore for InterruptedWriteExhaustingStore {
        async fn load_run(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
        ) -> Result<
            Option<nexus_orchestration::run_state::RunRecord>,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner.load_run(session_id).await
        }

        async fn start_run(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner
                .start_run(session_id, descriptor, checkpoint, next_state)
                .await
        }

        async fn admit_schedule_run(
            &self,
            schedule_id: &str,
            session_id: &nexus_orchestration::engine::SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            core_context_version: u32,
            expected_core_context_version: u32,
            admission_gate: Option<&nexus_orchestration::run_state::ScheduleAdmissionGate>,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner
                .admit_schedule_run(
                    schedule_id,
                    session_id,
                    descriptor,
                    checkpoint,
                    next_state,
                    core_context_version,
                    expected_core_context_version,
                    admission_gate,
                )
                .await
        }

        async fn commit_transition(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: nexus_orchestration::engine::SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            if next_status == nexus_orchestration::engine::SessionStatus::Interrupted {
                // The competing transition wins the CAS first, every time:
                // the engine's bounded retry exhausts and the public result
                // must be a conflict/error, never successful cancellation.
                let record = self
                    .inner
                    .load_run(session_id)
                    .await?
                    .expect("run exists at interrupted exhaustion gate");
                let root = self
                    .storage
                    .get(&session_id.0)
                    .await
                    .expect("root session at interrupted exhaustion gate")
                    .expect("root exists at interrupted exhaustion gate");
                let mut competing_state = record.state.unwrap_or_default();
                competing_state.cancel_requested = true;
                self.inner
                    .commit_transition(
                        session_id,
                        record.state_revision,
                        nexus_orchestration::run_state::RunCheckpoint {
                            root: &root,
                            children: &[],
                        },
                        record.status.clone(),
                        &competing_state,
                    )
                    .await
                    .expect("competing transition wins the CAS");
            }
            self.inner
                .commit_transition(
                    session_id,
                    expected_revision,
                    checkpoint,
                    next_status,
                    next_state,
                )
                .await
        }

        async fn commit_transition_with_graph_fence(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: nexus_orchestration::engine::SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            let _ = expected_graph_version;
            self.commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                next_status,
                next_state,
            )
            .await
        }

        async fn settle_run(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            terminal_target: nexus_orchestration::run_state::TerminalSettlementTarget,
        ) -> Result<
            nexus_orchestration::run_state::SettlementResult,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner
                .settle_run(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    next_state,
                    terminal_target,
                )
                .await
        }

        async fn restore_pre_step(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            pre_step: &graph_flow::Session,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner
                .restore_pre_step(session_id, expected_revision, pre_step)
                .await
        }

        async fn mark_step_in_flight(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            step_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<
            nexus_orchestration::run_state::RunRecord,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner
                .mark_step_in_flight(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    step_state,
                )
                .await
        }

        async fn persist_prompt_attempt(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            expected_attempt_id: Option<&str>,
            attempt: &nexus_orchestration::run_state::PromptAttempt,
        ) -> Result<(), nexus_orchestration::engine::EngineError> {
            self.inner
                .persist_prompt_attempt(
                    session_id,
                    expected_revision,
                    expected_step,
                    expected_attempt_id,
                    attempt,
                )
                .await
        }

        async fn clear_prompt_attempt(
            &self,
            session_id: &nexus_orchestration::engine::SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            attempt_id: &str,
        ) -> Result<(), nexus_orchestration::engine::EngineError> {
            self.inner
                .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
                .await
        }

        async fn load_children(
            &self,
            parent_session_id: &nexus_orchestration::engine::SessionId,
        ) -> Result<
            Vec<nexus_orchestration::run_state::RunRecord>,
            nexus_orchestration::engine::EngineError,
        > {
            self.inner.load_children(parent_session_id).await
        }
    }

    /// Production handler-level regression (QC2 F-002): a real v1 session,
    /// one injected cleanup failure, the first Cancel through the production
    /// `signal_session` handler — the coordinator projects the durable
    /// `interrupted` winner as a typed success — then an immediate
    /// `get_session`: the public projection must be `interrupted` with the
    /// actionable failure reason, never a stale in-memory `running` and
    /// never a 500.
    #[tokio::test]
    async fn get_session_projects_interrupted_after_unconfirmed_cancel() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let pool = state.pool().expect("test pool").clone();
        let sqlite = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn graph_flow::SessionStorage> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        // Inject one cleanup failure: the first finalize_run fails.
        let executor = ScriptedFailFirstExecutor::new();
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());
        // QC2 F-002: Cancel routes through the coordinator, so the test wires
        // the production coordinator over the SAME engine/store.
        let engine = Arc::new(engine);
        state.set_engine(engine.clone());
        state.set_run_coordinator(Arc::new(crate::preset_run::WorkflowRunCoordinator::new(
            engine,
            storage.clone(),
            Arc::new(pool.clone()),
            session_cancels,
        )));

        // Start a real v1 session.
        let engine = state.engine().expect("engine set");
        let graph = manual_wait_graph("public-failed-cancel");
        let session_id = engine
            .start_session_with_graph("novel-writing", graph)
            .await
            .expect("start session");

        // First Cancel through the production handler: the cleanup cannot be
        // confirmed, so the coordinator persists the durable `interrupted`
        // winner and projects it as a TYPED success — never a 500 derived
        // from an engine error string.
        let (code, body) = signal_session(
            State(state.clone()),
            Path(session_id.0.clone()),
            Json(SignalSessionRequest {
                signal: "cancel".to_string(),
                wait_id: None,
            }),
        )
        .await
        .expect("an unconfirmed cancel must project a typed success");
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            body.0["status"].as_str(),
            Some("interrupted"),
            "the unconfirmed cancel status is the durable interrupted winner"
        );
        assert_eq!(
            body.0["cancel_outcome"].as_str(),
            Some("unconfirmed"),
            "the typed projection must label the unconfirmed outcome"
        );

        // Immediately perform the public GET: the projection must be
        // `interrupted` with the actionable failure reason.
        let Json(body) = get_session(State(state), Path(session_id.0.clone()))
            .await
            .expect("get_session should succeed");
        assert_eq!(
            body.session.status, "interrupted",
            "the public projection must expose the durable interrupted status"
        );
        assert!(
            body.session
                .failure_reason
                .as_deref()
                .is_some_and(|r| r.contains("cancel cleanup unconfirmed")),
            "the public projection must carry the actionable cleanup reason, got: {:?}",
            body.session.failure_reason
        );
    }

    /// Bounded interruption-write exhaustion proof (Finding 4, round 3):
    /// when the `Interrupted` write can never win its revision CAS, the
    /// engine's bounded retry exhausts and the coordinator's typed cancel
    /// projection finds NO durable cancel outcome — the public result is the
    /// shared conflict envelope (409), never an apparent successful
    /// cancellation and never a cancel-specific 500.
    #[tokio::test]
    async fn interrupted_write_exhaustion_is_conflict_never_successful_cancel() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let pool = state.pool().expect("test pool").clone();
        let sqlite = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn graph_flow::SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = Arc::new(InterruptedWriteExhaustingStore::new(
            real_store.clone(),
            storage.clone(),
        ));

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        // The first finalize fails (cleanup unconfirmed) — the Interrupted
        // write then exhausts its bounded retry.
        let executor = ScriptedFailFirstExecutor::new();
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());
        // QC2 F-002: wire the production coordinator over the SAME faulting
        // store so the cancel projection reads exactly the durable truth.
        let engine = Arc::new(engine);
        state.set_engine(engine.clone());
        state.set_run_coordinator(Arc::new(
            crate::preset_run::WorkflowRunCoordinator::new(
                engine,
                storage.clone(),
                Arc::new(pool.clone()),
                session_cancels,
            )
            .with_workflow_store(store.clone()),
        ));

        // Start a real v1 session.
        let engine = state.engine().expect("engine set");
        let graph = manual_wait_graph("public-exhaustion");
        let session_id = engine
            .start_session_with_graph("novel-writing", graph)
            .await
            .expect("start session");

        // First Cancel: the Interrupted write exhausts its retry bound →
        // the handler maps the revision loss to a 409 conflict — never a
        // successful cancellation.
        let err = signal_session(
            State(state.clone()),
            Path(session_id.0.clone()),
            Json(SignalSessionRequest {
                signal: "cancel".to_string(),
                wait_id: None,
            }),
        )
        .await
        .expect_err("the exhausted interruption write must surface a conflict");
        assert!(
            matches!(err, NexusApiError::ConflictCoded { .. }),
            "the exhausted interruption write is a 409 conflict, got {err:?}"
        );

        // The durable row is NOT cancelled — the run cannot appear
        // successfully cancelled.
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_ne!(
            record.status,
            nexus_orchestration::engine::SessionStatus::Cancelled,
            "the run must never appear successfully cancelled"
        );
        assert!(
            record.state.as_ref().is_some_and(|s| s.cancel_requested),
            "the durable cancel intent is present (the run stays actionable)"
        );
    }
}

/// Resolve a session through the same durable owner lookup as inspect, then
/// verify the active Creator owns it. Foreign/missing sessions are 404.
async fn authorize_orchestration_session_read(
    state: &WorkspaceState,
    session_id: &str,
) -> Result<(), NexusApiError> {
    use crate::api::handlers::works::read_active_creator_id;
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let engine = state
        .engine()
        .ok_or_else(|| NexusApiError::service_unavailable("engine not available"))?;
    let sid = nexus_orchestration::engine::SessionId(session_id.to_string());
    let sessions = engine
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "ENGINE_ERROR".into(),
            message: e.to_string(),
        })?;
    // The DURABLE row is the ownership SSOT, exactly as `get_session` treats
    // it for status. The in-memory active summary is only a fast path for a
    // row the durable store does not know about: after an ownership change
    // (or any writer that moved `orchestration_sessions.creator_id`), a stale
    // in-memory summary would otherwise authorize a FOREIGN creator to read
    // another creator's live event stream.
    let durable_owner = match state.pool() {
        Some(pool) => {
            let storage = SqliteSessionStorage::new(Arc::new(pool.clone()));
            storage
                .get_checkpoint_row(session_id)
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "STORAGE_ERROR".into(),
                    message: e.to_string(),
                })?
                .map(|row| row.creator_id)
        }
        None => None,
    };
    let owner = match durable_owner {
        Some(owner) => owner,
        None => {
            let active = sessions
                .iter()
                .find(|s| s.session_id == sid)
                .ok_or_else(|| NexusApiError::NotFound(format!("session {session_id}")))?;
            active.creator_id.clone()
        }
    };
    if owner != active_creator {
        return Err(NexusApiError::NotFound(format!("session {session_id}")));
    }
    Ok(())
}

/// `GET /v1/daemon/orchestration/sessions/{session_id}/events` — live SSE replay.
pub async fn session_events(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, NexusApiError> {
    authorize_orchestration_session_read(&state, &session_id).await?;
    let inspect_url = format!("/v1/daemon/orchestration/sessions/{session_id}");
    let last_event_id = headers.get("last-event-id").and_then(|v| v.to_str().ok());
    let registry = state.run_event_registry();
    let sub = match registry.subscribe_live(&session_id, last_event_id, inspect_url) {
        Ok(rx) => rx,
        Err(crate::run_events::SubscribeError::MalformedCursor)
        | Err(crate::run_events::SubscribeError::FutureCursor) => {
            return Err(NexusApiError::BadRequest {
                code: "invalid_cursor".into(),
                message: "malformed or future Last-Event-ID".into(),
            });
        }
        Err(crate::run_events::SubscribeError::TooManySubscribers) => {
            return Err(NexusApiError::ConflictCoded {
                code: "sse_subscriber_limit".into(),
                message: "too many concurrent SSE subscribers for this run".into(),
            });
        }
        Err(crate::run_events::SubscribeError::HistoryUnavailable(body)) => {
            return Err(NexusApiError::BadRequestCodedDetails {
                code: "history_unavailable".into(),
                message: "run event history is not available for replay".into(),
                details: serde_json::json!({
                    "run_id": body.run_id,
                    "inspect_url": body.inspect_url,
                }),
            });
        }
    };
    let stream = futures_util::stream::unfold(sub, |mut sub| async move {
        match sub.recv().await {
            Some(frame) => {
                let event = Event::default()
                    .id(frame.id)
                    .event(frame.event)
                    .data(frame.data);
                Some((Ok(event), sub))
            }
            None => None,
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
