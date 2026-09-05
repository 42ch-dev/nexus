#![allow(clippy::missing_errors_doc)]
//! Agent Host API handlers.
//!
//! Endpoints:
//! - GET    /v1/daemon/agent-host/health                           — Host health status
//! - GET    /v1/daemon/agent-host/providers                        — List available providers
//! - POST   /v1/daemon/agent-host/sessions                         — Create a managed session
//! - GET    /v1/daemon/agent-host/sessions                         — List active sessions (with pagination)
//! - GET    /v1/daemon/agent-host/sessions/{session_id}            — Get session detail
//! - DELETE /v1/daemon/agent-host/sessions/{session_id}            — Shutdown a single session
//! - POST   /v1/daemon/agent-host/sessions/{session_id}/operations — Execute a host operation
//! - POST   /v1/daemon/agent-host/operations/{operation_id}:cancel — Cancel in-flight operation
//! - GET    /v1/daemon/agent-host/sessions/{session_id}/events     — SSE event stream

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::StreamExt;
use crate::actor_admission::{ActorAdmissionService, ActorPairMode, ActorViewpoint};
use crate::actor_knowledge_view::AdmittedActor;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::workspace::actor_sessions::{echo_actor_pair, ActorSessionRegistry};
use nexus_contracts::generated::daemon_api::agent_host::{
    AgentHostListSessionsQuery, AgentScanEntry, CancelOperationResponse, CreateSessionRequest,
    ExecuteOperationRequest, OperationResponse, ScanRequest, ScanResponse, SessionListResponse,
    SessionResponse, ShutdownSessionResponse,
};
use serde::Serialize;
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

/// GET /v1/daemon/agent-host/health
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

/// GET /v1/daemon/agent-host/providers
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

/// POST /v1/daemon/agent-host/sessions
pub async fn create_session(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, NexusApiError> {
    let pair = ActorAdmissionService::classify_pair(req.actor_ref.is_some(), req.viewpoint.is_some())?;
    if pair == ActorPairMode::Actor {
        let creator_id = require_creator(&state)?;
        let admission = ActorAdmissionService::new(state.pool_or_uninit()?.clone());
        let (Some(actor_ref), Some(viewpoint)) = (req.actor_ref.as_ref(), req.viewpoint.as_ref())
        else {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "actor_ref and viewpoint must both be present or both absent".into(),
            });
        };
        let actor = match actor_ref {
            nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CreatorActorRef { creator_id, .. } => {
                AdmittedActor::Creator { creator_id: creator_id.to_string() }
            }
            nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CharacterActorRef { character_id, .. } => {
                AdmittedActor::Character { character_id: character_id.to_string() }
            }
        };
        let ctx = admission
            .admit(
                &creator_id,
                actor,
                ActorViewpoint {
                    world_id: viewpoint.world_id.to_string(),
                    binding_id: viewpoint.binding_id.as_ref().map(|id| (**id).clone()),
                    branch_id: viewpoint.branch_id.as_ref().map(|id| (**id).clone()),
                    event_id: viewpoint.event_id.as_ref().map(|id| (**id).clone()),
                },
            )
            .await?;
        let host = get_host(&state)?;
        let cwd = session_cwd_path(&req);
        let key = ActorSessionRegistry::key_for(
            &req.provider_id,
            &cwd,
            req.model.clone(),
            req.mode.clone(),
            &ctx,
        )?;
        let host_req = host_create_request(&req);
        let host_for_create = Arc::clone(&host);
        let session = state
            .actor_sessions()
            .resolve_or_create(key, ctx.clone(), host.as_ref(), move || {
                let host_for_create = host_for_create;
                let host_req = host_req;
                async move {
                    host_for_create
                        .create_session(host_req)
                        .await
                        .map_err(|e| map_host_error(&e))
                }
            })
            .await?;
        let (actor_ref, viewpoint) = echo_actor_pair(&ctx)?;
        return Ok(Json(session_wire(
            session.id.to_string(),
            session.provider_id.to_string(),
            format!("{:?}", session.state),
            None,
            req.model.clone(),
            actor_ref,
            viewpoint,
        )));
    }

    let host = get_host(&state)?;
    let host_req = host_create_request(&req);
    let model = req.model.clone();

    let session = host
        .create_session(host_req)
        .await
        .map_err(|e| map_host_error(&e))?;

    Ok(Json(session_wire(
        session.id.to_string(),
        session.provider_id.to_string(),
        format!("{:?}", session.state),
        None,
        model,
        None,
        None,
    )))
}

fn session_cwd_path(req: &CreateSessionRequest) -> std::path::PathBuf {
    req.cwd.as_ref().map_or_else(
        || std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from,
    )
}

/// Legacy host request: empty MCP list and `metadata: null`.
fn host_create_request(
    req: &CreateSessionRequest,
) -> nexus_agent_host::capability::CreateSessionRequest {
    nexus_agent_host::capability::CreateSessionRequest {
        provider_id: nexus_agent_host::ProviderId::new(&req.provider_id),
        cwd: session_cwd_path(req),
        model: req.model.clone(),
        mode: req.mode.clone(),
        mcp_servers: vec![],
        metadata: serde_json::Value::Null,
    }
}

fn session_wire(
    session_id: String,
    provider_id: String,
    state: String,
    active_op_id: Option<String>,
    model: Option<String>,
    actor_ref: Option<
        nexus_contracts::generated::daemon_api::agent_host::session_response::NexusActorRef,
    >,
    viewpoint: Option<
        nexus_contracts::generated::daemon_api::agent_host::session_response::NexusSessionViewpoint,
    >,
) -> SessionResponse {
    SessionResponse {
        session_id,
        provider_id,
        state,
        active_op_id,
        model,
        actor_ref,
        viewpoint,
    }
}

fn overlay_actor_pair(
    state: &WorkspaceState,
    session_id: &nexus_agent_host::HostSessionId,
) -> Result<
    (
        Option<
            nexus_contracts::generated::daemon_api::agent_host::session_response::NexusActorRef,
        >,
        Option<
            nexus_contracts::generated::daemon_api::agent_host::session_response::NexusSessionViewpoint,
        >,
    ),
    NexusApiError,
> {
    match state.actor_sessions().context_for(session_id) {
        Some(ctx) => echo_actor_pair(&ctx),
        None => Ok((None, None)),
    }
}

/// GET /v1/daemon/agent-host/sessions
///
/// Returns real session registry from agent host with pagination.
pub async fn list_sessions(
    State(state): State<WorkspaceState>,
    Query(params): Query<AgentHostListSessionsQuery>,
) -> Result<Json<SessionListResponse>, NexusApiError> {
    let host = get_host(&state)?;
    let sessions = host.list_sessions().await.map_err(|e| map_host_error(&e))?;

    let limit = params
        .limit
        .unwrap_or(50)
        .clamp(1, 250);
    let limit_us = usize::try_from(limit).unwrap_or(250);

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
        .take(limit_us)
        .map(|s| {
            let (actor_ref, viewpoint) = overlay_actor_pair(&state, &s.id)?;
            Ok(session_wire(
                s.id.to_string(),
                s.provider_id.to_string(),
                format!("{:?}", s.state),
                active_op_display(&s),
                None,
                actor_ref,
                viewpoint,
            ))
        })
        .collect::<Result<Vec<_>, NexusApiError>>()?;

    let next_cursor = if items.len() == limit_us {
        items.last().map(|i| i.session_id.clone())
    } else {
        None
    };

    Ok(Json(SessionListResponse {
        items: super::wire_cast(items),
        pagination: super::wire_cast(nexus_contracts::PaginationInfo {
            limit,
            has_more: next_cursor.is_some(),
            next_cursor,
        }),
    }))
}

/// GET /v1/daemon/agent-host/sessions/{session_id}
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

    let (actor_ref, viewpoint) = overlay_actor_pair(&state, &session.id)?;
    Ok(Json(session_wire(
        session.id.to_string(),
        session.provider_id.to_string(),
        format!("{:?}", session.state),
        active_op_display(&session),
        None,
        actor_ref,
        viewpoint,
    )))
}

/// DELETE /v1/daemon/agent-host/sessions/{session_id}
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
    host.shutdown_session(sid.clone())
        .await
        .map_err(|e| map_host_error(&e))?;
    state.actor_sessions().remove_session(&sid);

    Ok(Json(ShutdownSessionResponse {
        session_id,
        status: "shutdown".to_string(),
    }))
}

/// POST /v1/daemon/agent-host/sessions/{session_id}/operations
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

/// `POST /v1/daemon/agent-host/operations/{operation_id}:cancel` — cancel in-flight operation.
///
/// Routed as `POST /v1/daemon/agent-host/operations/:operation_id` because
/// matchit 0.7 cannot register `:operation_id:cancel` as a separate pattern
/// (`:a:b` is rejected). The path segment must end with `:cancel`; otherwise
/// this returns 404 (plain POST without the verb is not a cancel). Mirrors
/// `logout_creator`. Residual: R-HOTFIX-404-PARAM-SYNTAX.
pub async fn cancel_operation(
    State(state): State<WorkspaceState>,
    Path(segment): Path<String>,
) -> Result<Json<CancelOperationResponse>, NexusApiError> {
    // matchit 0.7 rejects consecutive captures like `:operation_id:cancel`.
    // Workaround: capture the full segment as `:operation_id` and strip
    // `:cancel` in the handler (mirrors `logout_creator`).
    let operation_id = segment
        .strip_suffix(":cancel")
        .ok_or_else(|| NexusApiError::NotFound(format!("Operation route '{segment}' not found")))?
        .to_string();

    let uuid = parse_operation_id(&operation_id)?;
    let host = get_host(&state)?;

    let op_id = nexus_agent_host::HostOperationId(uuid);
    host.cancel(op_id).await.map_err(|e| map_host_error(&e))?;

    Ok(Json(CancelOperationResponse {
        operation_id,
        status: "cancelled".to_string(),
    }))
}

/// GET /v1/daemon/agent-host/sessions/{session_id}/events
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
// Agent scan
// ---------------------------------------------------------------------------

/// POST /v1/daemon/agent-host/scan
///
/// Returns the ACP registry agent list annotated with local PATH-install
/// availability. Combines the cached registry with [`scan_local_installations`].
/// The request can refresh the registry cache and/or filter to installed agents.
pub async fn scan(
    State(state): State<WorkspaceState>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<ScanResponse>, NexusApiError> {
    let cache_dir = state.nexus_home().join("registry");
    let registry_client = nexus_acp_host::registry::RegistryClient::with_cache_dir(cache_dir)
        .map_err(|e| NexusApiError::Internal {
            code: "REGISTRY_CLIENT_ERROR".into(),
            message: format!("failed to create registry client: {e}"),
        })?;

    let registry = if req.registry_refresh {
        registry_client.refresh().await
    } else {
        registry_client.get_registry().await
    }
    .map_err(|e| NexusApiError::Internal {
        code: "REGISTRY_FETCH_ERROR".into(),
        message: format!("failed to fetch ACP registry: {e}"),
    })?;

    // Probe against process PATH + login-shell-equivalent dirs without
    // mutating the process environment (safe on a live Tokio runtime).
    let probe_dirs = crate::path_enrichment::probe_path_dirs();
    let installations =
        nexus_acp_host::registry::scan_local_installations_with_path(&registry, &probe_dirs).await;
    let by_binary: HashMap<String, nexus_acp_host::registry::LocalInstallation> = installations
        .into_iter()
        .map(|li| (li.binary.clone(), li))
        .collect();

    let mut agents: Vec<AgentScanEntry> = registry
        .agents
        .into_iter()
        .map(|agent| build_scan_entry(agent, &by_binary))
        .collect();

    // Discover native CLI providers and merge them in. ACP entries come first;
    // native entries are appended. If a native equivalent is installed, suppress
    // the corresponding ACP registry entry so the UI shows the honest path.
    let host_config = state.agent_host_config();
    let native_probe_dirs = probe_dirs.clone();
    let native_entries = tokio::task::spawn_blocking(move || {
        nexus_agent_host::discovery::path_scan::scan_path_in(&host_config, &[], &native_probe_dirs)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "NATIVE_SCAN_ERROR".into(),
        message: format!("native PATH scan task panicked: {e}"),
    })?
    .map_err(|e| NexusApiError::Internal {
        code: "NATIVE_SCAN_ERROR".into(),
        message: format!("failed to scan native CLI providers: {e}"),
    })?;

    let suppress_ids: std::collections::HashSet<&str> = native_entries
        .iter()
        .filter(|entry| entry.health.available)
        .filter_map(|entry| {
            NATIVE_PREFERRED_FAMILIES
                .iter()
                .find(|(_, native_id)| entry.provider_id.0 == *native_id)
                .map(|(registry_id, _)| *registry_id)
        })
        .collect();

    agents.retain(|entry| {
        entry
            .registry_agent_id
            .as_deref()
            .is_none_or(|id| !suppress_ids.contains(id))
    });

    agents.extend(
        native_entries
            .into_iter()
            .map(|entry| map_native_catalog_entry(entry, &by_binary)),
    );

    if req.filter == nexus_contracts::ScanRequestFilter::Installed {
        agents.retain(|a| a.installed);
    }

    Ok(Json(ScanResponse {
        agents: super::wire_cast(agents),
    }))
}

/// Build an [`AgentScanEntry`] from a registry agent plus PATH probe results.
fn build_scan_entry(
    agent: nexus_acp_host::registry::AgentEntry,
    by_binary: &HashMap<String, nexus_acp_host::registry::LocalInstallation>,
) -> AgentScanEntry {
    let platform_cmds = agent
        .distribution
        .binary
        .as_ref()
        .map_or_else(Vec::new, platform_binary_commands);

    // Pick the first installed command, otherwise the first known command.
    let launch_command = platform_cmds
        .iter()
        .find(|cmd| by_binary.contains_key(*cmd))
        .or_else(|| platform_cmds.first())
        .cloned();

    let installed = platform_cmds.iter().any(|cmd| by_binary.contains_key(cmd));

    let version = launch_command
        .as_ref()
        .and_then(|cmd| by_binary.get(cmd))
        .and_then(|li| li.version.clone());

    AgentScanEntry {
        name: agent.name,
        registry_agent_id: Some(agent.id),
        launch_command,
        installed,
        version,
        description: agent.description,
        icon_url: agent.icon,
    }
}

/// Native CLI provider families that take precedence over their ACP registry
/// counterparts. Tuple order is `(registry_agent_id, native_provider_id)`.
const NATIVE_PREFERRED_FAMILIES: &[(&str, &str)] = &[
    ("claude-acp", "claude-native"),
    ("codex-acp", "codex-native"),
];

/// Map a native CLI catalog entry to an [`AgentScanEntry`].
fn map_native_catalog_entry(
    entry: nexus_agent_host::ProviderCatalogEntry,
    by_binary: &HashMap<String, nexus_acp_host::registry::LocalInstallation>,
) -> AgentScanEntry {
    let launch_command = match entry.launch {
        nexus_agent_host::LaunchStrategy::NativeCli { command, .. } => Some(command),
        nexus_agent_host::LaunchStrategy::Acp { .. } => None,
    };

    let version = launch_command
        .as_ref()
        .map(|cmd| nexus_acp_host::registry::bare_command_name(cmd))
        .and_then(|bare| by_binary.get(&bare))
        .and_then(|li| li.version.clone());

    AgentScanEntry {
        name: entry.display_name,
        registry_agent_id: None,
        launch_command,
        installed: entry.health.available,
        version,
        description: None,
        icon_url: None,
    }
}

/// Return the ordered list of binary commands for an agent's binary distribution.
/// Commands are normalized to bare names so PATH probes match registry keys
/// consistently (see `nexus_acp_host::registry::bare_command_name`).
fn platform_binary_commands(binary: &nexus_acp_host::registry::BinaryDistribution) -> Vec<String> {
    let mut cmds = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for pb in [
        &binary.darwin_aarch64,
        &binary.darwin_x86_64,
        &binary.linux_aarch64,
        &binary.linux_x86_64,
        &binary.windows_aarch64,
        &binary.windows_x86_64,
    ]
    .into_iter()
    .flatten()
    {
        let bare = nexus_acp_host::registry::bare_command_name(&pb.cmd);
        if seen.insert(bare.clone()) {
            cmds.push(bare);
        }
    }
    cmds
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // SCAN_PATH_LOCK is deliberately held across awaits: it serializes
    // PATH-mutating scan tests so concurrent probes don't see each other's
    // shim directories (see the lock's doc comment below).
    #![allow(clippy::await_holding_lock)]

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
            actor_ref: None,
            cwd: Some("/tmp".to_string()),
            mode: None,
            model: None,
            provider_id: "nonexistent".to_string(),
            viewpoint: None,
        };
        let result = create_session(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_sessions_returns_empty_when_no_sessions() {
        let state = state_with_host().await;
        let result = list_sessions(
            State(state),
            Query(AgentHostListSessionsQuery {
                limit: Some(50),
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
            Query(AgentHostListSessionsQuery {
                limit: Some(1),
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
        // Route shares `:operation_id` with the `:cancel` verb (mirrors
        // `logout_creator`); include the suffix so the stripped value reaches
        // UUID parsing and is rejected as 400.
        let result = cancel_operation(State(state), Path("bad-uuid:cancel".to_string())).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn cancel_operation_rejects_missing_cancel_verb() {
        let state = state_with_host().await;
        // A plain operation id without the trailing `:cancel` verb is not a
        // cancel request and must 404 (mirrors `logout_creator`).
        let result = cancel_operation(
            State(state),
            Path("550e8400-e29b-41d4-a716-446655440000".to_string()),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().status_code(),
            axum::http::StatusCode::NOT_FOUND
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

    // ── Agent scan integration tests ────────────────────────────────────────

    use crate::api::auth_middleware::DaemonApiConfig;
    use axum_test::TestServer;

    /// Serialize agent-scan integration tests that mutate `PATH` so concurrent
    /// probes do not see each other's shim directories.
    static SCAN_PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Temporarily prepend a directory to `PATH`, restoring the previous value
    /// on drop.
    struct PathGuard {
        previous: Option<String>,
    }

    impl PathGuard {
        /// Replace `PATH` with a single directory, restoring the previous value on
        /// drop. Useful for deterministic tests that must not see the host PATH.
        fn isolate(dir: &std::path::Path) -> Self {
            let previous = std::env::var("PATH").ok();
            let new_path = std::env::join_paths([dir.to_path_buf()]).expect("valid PATH");
            std::env::set_var("PATH", new_path);
            Self { previous }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match self.previous {
                Some(ref p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    fn write_shim(dir: &std::path::Path, name: &str, script: &str) -> std::path::PathBuf {
        let shim = dir.join(name);
        std::fs::create_dir_all(dir).expect("create bin dir");
        std::fs::write(&shim, script).expect("write shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&shim, perms).expect("chmod shim");
        }
        shim
    }

    async fn create_scan_test_app_with_installed() -> (TestServer, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let bin_dir = tmp.path().join("bin");
        write_shim(
            &bin_dir,
            "nexus-scan-installed",
            "#!/bin/sh\necho \"nexus-scan-installed 1.2.3\"\n",
        );

        let nexus_home = tmp.path().join(".nexus42");
        write_registry_cache(&nexus_home);
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"test-creator\"\n",
        )
        .expect("config.toml");

        let db_path = tmp.path().join("state.db");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let app = crate::api::create_router(state, DaemonApiConfig::keyless());
        let server = TestServer::new(app).expect("TestServer should initialize");

        (server, tmp)
    }

    fn write_registry_cache(home: &std::path::Path) {
        let registry_dir = home.join("registry");
        std::fs::create_dir_all(&registry_dir).expect("create registry dir");

        let registry_json = r#"{
            "version": "1.0.0",
            "agents": [
                {
                    "id": "installed-agent",
                    "name": "Installed Agent",
                    "version": "1.0.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": { "archive": "https://example.com/i.tar.gz", "cmd": "nexus-scan-installed" }
                        }
                    }
                },
                {
                    "id": "missing-agent",
                    "name": "Missing Agent",
                    "version": "2.0.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": { "archive": "https://example.com/m.tar.gz", "cmd": "nexus-scan-missing-42ch" }
                        }
                    }
                }
            ],
            "extensions": []
        }"#;
        std::fs::write(registry_dir.join("cache.json"), registry_json).expect("write cache");

        let meta_json = r#"{"fetched_at":"2026-07-06T00:00:00Z","registry_version":"1.0.0"}"#;
        std::fs::write(registry_dir.join("cache_meta.json"), meta_json).expect("write meta");
    }

    async fn create_scan_test_app() -> (TestServer, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        write_registry_cache(&nexus_home);
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"test-creator\"\n",
        )
        .expect("config.toml");

        let db_path = tmp.path().join("state.db");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let app = crate::api::create_router(state, DaemonApiConfig::keyless());
        let server = TestServer::new(app).expect("TestServer should initialize");

        (server, tmp)
    }

    fn write_registry_cache_with_native_acp(home: &std::path::Path) {
        let registry_dir = home.join("registry");
        std::fs::create_dir_all(&registry_dir).expect("create registry dir");

        let registry_json = r#"{
            "version": "1.0.0",
            "agents": [
                {
                    "id": "codex-acp",
                    "name": "Codex ACP",
                    "version": "1.0.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": { "archive": "https://example.com/codex.tar.gz", "cmd": "./codex" }
                        }
                    }
                },
                {
                    "id": "claude-acp",
                    "name": "Claude ACP",
                    "version": "1.0.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": { "archive": "https://example.com/claude.tar.gz", "cmd": "./claude" }
                        }
                    }
                },
                {
                    "id": "other-agent",
                    "name": "Other Agent",
                    "version": "1.0.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": { "archive": "https://example.com/other.tar.gz", "cmd": "other-cmd" }
                        }
                    }
                }
            ],
            "extensions": []
        }"#;
        std::fs::write(registry_dir.join("cache.json"), registry_json).expect("write cache");

        let meta_json = r#"{"fetched_at":"2026-07-06T00:00:00Z","registry_version":"1.0.0"}"#;
        std::fs::write(registry_dir.join("cache_meta.json"), meta_json).expect("write meta");
    }

    async fn create_scan_test_app_with_native_acp_registry() -> (TestServer, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        write_registry_cache_with_native_acp(&nexus_home);
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"test-creator\"\n",
        )
        .expect("config.toml");

        let db_path = tmp.path().join("state.db");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let app = crate::api::create_router(state, DaemonApiConfig::keyless());
        let server = TestServer::new(app).expect("TestServer should initialize");

        (server, tmp)
    }

    #[tokio::test]
    async fn scan_endpoint_returns_200_with_frozen_shape() {
        let _lock = SCAN_PATH_LOCK.lock().expect("lock scan tests");
        let (server, _tmp) = create_scan_test_app().await;
        let response = server
            .post("/v1/daemon/agent-host/scan")
            .json(&serde_json::json!({}))
            .await;
        assert_eq!(response.status_code(), axum::http::StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(
            body.agents.len() >= 2,
            "scan should include at least the two registry agents"
        );

        let installed = body
            .agents
            .iter()
            .find(|a| a.registry_agent_id.as_deref() == Some("installed-agent"))
            .expect("installed agent");
        assert_eq!(installed.name, "Installed Agent");
        assert_eq!(
            installed.registry_agent_id.as_deref(),
            Some("installed-agent")
        );
        assert_eq!(
            installed.launch_command.as_deref(),
            Some("nexus-scan-installed")
        );

        let missing = body
            .agents
            .iter()
            .find(|a| a.registry_agent_id.as_deref() == Some("missing-agent"))
            .expect("missing agent");
        assert_eq!(missing.name, "Missing Agent");
        assert_eq!(
            missing.launch_command.as_deref(),
            Some("nexus-scan-missing-42ch")
        );
    }

    #[tokio::test]
    async fn scan_endpoint_filter_installed_keeps_only_installed() {
        let _lock = SCAN_PATH_LOCK.lock().expect("lock scan tests");
        let (server, tmp) = create_scan_test_app_with_installed().await;
        let _path_guard = PathGuard::isolate(tmp.path().join("bin").as_path());

        let response = server
            .post("/v1/daemon/agent-host/scan")
            .json(&serde_json::json!({ "filter": "installed" }))
            .await;
        assert_eq!(response.status_code(), axum::http::StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(
            body.agents.iter().all(|a| a.installed),
            "filter=installed should only return installed agents"
        );
        let installed = body
            .agents
            .iter()
            .find(|a| a.registry_agent_id.as_deref() == Some("installed-agent"))
            .expect("installed agent");
        assert!(installed.version.is_some());
    }

    #[tokio::test]
    async fn scan_endpoint_includes_native_cli_entries_when_on_path() {
        let _lock = SCAN_PATH_LOCK.lock().expect("lock scan tests");
        // Registry must list codex/claude binaries so PATH probes populate
        // `by_binary` for native entry version lookup (V1.125).
        let (server, tmp) = create_scan_test_app_with_native_acp_registry().await;
        let bin_dir = tmp.path().join("bin");
        write_shim(&bin_dir, "claude", "#!/bin/sh\necho \"claude 1.2.3\"\n");
        write_shim(&bin_dir, "codex", "#!/bin/sh\necho \"codex 1.2.3\"\n");
        let _path_guard = PathGuard::isolate(bin_dir.as_path());

        let response = server
            .post("/v1/daemon/agent-host/scan")
            .json(&serde_json::json!({}))
            .await;
        assert_eq!(response.status_code(), axum::http::StatusCode::OK);

        let body: ScanResponse = response.json();
        let claude = body
            .agents
            .iter()
            .find(|a| a.name == "claude (native CLI)")
            .expect("claude native entry");
        assert!(claude.installed);
        assert!(claude.launch_command.is_some());
        assert_eq!(claude.registry_agent_id, None);

        let codex = body
            .agents
            .iter()
            .find(|a| a.name == "codex (native CLI)")
            .expect("codex native entry");
        assert!(codex.installed);
        assert!(codex.launch_command.is_some());
        assert_eq!(codex.registry_agent_id, None);
        assert!(
            codex.version.is_some(),
            "native codex entry should include probed --version output"
        );
    }

    #[tokio::test]
    async fn scan_endpoint_suppresses_acp_entry_when_native_preferred_is_installed() {
        let _lock = SCAN_PATH_LOCK.lock().expect("lock scan tests");
        let (server, tmp) = create_scan_test_app_with_native_acp_registry().await;
        let bin_dir = tmp.path().join("bin");
        write_shim(&bin_dir, "claude", "#!/bin/sh\necho \"claude 1.2.3\"\n");
        write_shim(&bin_dir, "codex", "#!/bin/sh\necho \"codex 1.2.3\"\n");
        let _path_guard = PathGuard::isolate(bin_dir.as_path());

        let response = server
            .post("/v1/daemon/agent-host/scan")
            .json(&serde_json::json!({}))
            .await;
        assert_eq!(response.status_code(), axum::http::StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(
            body.agents
                .iter()
                .all(|a| a.registry_agent_id.as_deref() != Some("codex-acp")),
            "codex-acp should be suppressed when codex-native is installed"
        );
        assert!(
            body.agents
                .iter()
                .all(|a| a.registry_agent_id.as_deref() != Some("claude-acp")),
            "claude-acp should be suppressed when claude-native is installed"
        );
        assert!(body
            .agents
            .iter()
            .any(|a| a.name == "codex (native CLI)" && a.installed));
        assert!(body
            .agents
            .iter()
            .any(|a| a.name == "claude (native CLI)" && a.installed));
    }

    // ── Agent scan unit tests ───────────────────────────────────────────────

    #[test]
    fn build_scan_entry_marks_installed_when_binary_on_path() {
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
                    darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                        archive: "https://example.com/a.tar.gz".to_string(),
                        cmd: "test-cmd".to_string(),
                        args: None,
                    }),
                    darwin_x86_64: None,
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };

        let mut by_binary = HashMap::new();
        by_binary.insert(
            "test-cmd".to_string(),
            nexus_acp_host::registry::LocalInstallation {
                binary: "test-cmd".to_string(),
                version: Some("test-cmd 1.2.3".to_string()),
            },
        );

        let entry = build_scan_entry(agent, &by_binary);
        assert!(entry.installed);
        assert_eq!(entry.launch_command.as_deref(), Some("test-cmd"));
        assert_eq!(entry.version.as_deref(), Some("test-cmd 1.2.3"));
    }

    #[test]
    fn build_scan_entry_marks_missing_when_binary_not_on_path() {
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: Some("desc".to_string()),
            repository: None,
            authors: None,
            license: None,
            icon: Some("https://example.com/icon.svg".to_string()),
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
                    darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                        archive: "https://example.com/a.tar.gz".to_string(),
                        cmd: "missing-cmd".to_string(),
                        args: None,
                    }),
                    darwin_x86_64: None,
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };

        let by_binary = HashMap::new();
        let entry = build_scan_entry(agent, &by_binary);
        assert!(!entry.installed);
        assert_eq!(entry.launch_command.as_deref(), Some("missing-cmd"));
        assert!(entry.version.is_none());
    }

    #[test]
    fn build_scan_entry_prefers_installed_command() {
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
                    darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                        archive: "https://example.com/a.tar.gz".to_string(),
                        cmd: "first".to_string(),
                        args: None,
                    }),
                    darwin_x86_64: Some(nexus_acp_host::registry::PlatformBinary {
                        archive: "https://example.com/b.tar.gz".to_string(),
                        cmd: "second".to_string(),
                        args: None,
                    }),
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };

        let mut by_binary = HashMap::new();
        by_binary.insert(
            "second".to_string(),
            nexus_acp_host::registry::LocalInstallation {
                binary: "second".to_string(),
                version: Some("second 2.0.0".to_string()),
            },
        );

        let entry = build_scan_entry(agent, &by_binary);
        assert!(entry.installed);
        assert_eq!(entry.launch_command.as_deref(), Some("second"));
        assert_eq!(entry.version.as_deref(), Some("second 2.0.0"));
    }

    #[test]
    fn build_scan_entry_normalizes_relative_binary_commands() {
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "cursor".to_string(),
            name: "Cursor".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
                    darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                        archive: "https://example.com/a.tar.gz".to_string(),
                        cmd: "./dist-package/cursor-agent".to_string(),
                        args: None,
                    }),
                    darwin_x86_64: None,
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };

        let mut by_binary = HashMap::new();
        by_binary.insert(
            "cursor-agent".to_string(),
            nexus_acp_host::registry::LocalInstallation {
                binary: "cursor-agent".to_string(),
                version: Some("cursor-agent 1.2.3".to_string()),
            },
        );

        let entry = build_scan_entry(agent, &by_binary);
        assert!(entry.installed);
        assert_eq!(entry.launch_command.as_deref(), Some("cursor-agent"));
        assert_eq!(entry.version.as_deref(), Some("cursor-agent 1.2.3"));
    }

    #[test]
    fn map_native_catalog_entry_populates_version_from_by_binary() {
        use nexus_agent_host::capability::{CapabilityDescriptor, ProtocolKind, ProviderHealth};
        use nexus_agent_host::{
            DiscoverySource, LaunchStrategy, ProviderCatalogEntry, ProviderId, TrustLevel,
        };

        let entry = ProviderCatalogEntry {
            provider_id: ProviderId::new("codex-native"),
            display_name: "codex (native CLI)".to_string(),
            protocol_kind: ProtocolKind::NativeCli,
            launch: LaunchStrategy::NativeCli {
                command: "/tmp/bin/codex".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
            source: DiscoverySource::PathScan,
            trust: TrustLevel::LocalPath,
            capabilities: CapabilityDescriptor::native_cli_limited(),
            health: ProviderHealth {
                provider_id: ProviderId::new("codex-native"),
                available: true,
                latency_ms: None,
                message: None,
            },
        };

        let mut by_binary = HashMap::new();
        by_binary.insert(
            "codex".to_string(),
            nexus_acp_host::registry::LocalInstallation {
                binary: "codex".to_string(),
                version: Some("codex 1.2.3".to_string()),
            },
        );

        let scan_entry = map_native_catalog_entry(entry, &by_binary);
        assert!(scan_entry.installed);
        assert_eq!(scan_entry.launch_command.as_deref(), Some("/tmp/bin/codex"));
        assert_eq!(scan_entry.version.as_deref(), Some("codex 1.2.3"));
    }

    #[test]
    fn platform_binary_commands_dedupes_and_orders() {
        let binary = nexus_acp_host::registry::BinaryDistribution {
            darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                archive: "https://example.com/a.tar.gz".to_string(),
                cmd: "cmd".to_string(),
                args: None,
            }),
            darwin_x86_64: Some(nexus_acp_host::registry::PlatformBinary {
                archive: "https://example.com/b.tar.gz".to_string(),
                cmd: "cmd".to_string(),
                args: None,
            }),
            linux_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                archive: "https://example.com/c.tar.gz".to_string(),
                cmd: "linux-cmd".to_string(),
                args: None,
            }),
            linux_x86_64: None,
            windows_aarch64: None,
            windows_x86_64: None,
        };

        let cmds = platform_binary_commands(&binary);
        assert_eq!(cmds, vec!["cmd".to_string(), "linux-cmd".to_string()]);
    }

    #[test]
    fn legacy_session_json_omits_actor_pair() {
        let resp = session_wire(
            "sid".into(),
            "prov".into(),
            "Ready".into(),
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            serde_json::to_string(&resp).expect("json"),
            r#"{"session_id":"sid","provider_id":"prov","state":"Ready"}"#
        );
        let with_optionals = session_wire(
            "sid".into(),
            "prov".into(),
            "Busy".into(),
            Some("op".into()),
            Some("m".into()),
            None,
            None,
        );
        assert_eq!(
            serde_json::to_string(&with_optionals).expect("json"),
            r#"{"session_id":"sid","provider_id":"prov","state":"Busy","active_op_id":"op","model":"m"}"#
        );
    }

    #[test]
    fn legacy_create_request_bytes_omit_actor_pair() {
        let req: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "cwd": "/tmp"
        }))
        .expect("legacy body");
        assert!(req.actor_ref.is_none());
        assert!(req.viewpoint.is_none());
        let host_req = host_create_request(&req);
        assert!(host_req.metadata.is_null());
        assert!(host_req.mcp_servers.is_empty());
        let host_json = serde_json::to_string(&host_req).expect("host json");
        assert_eq!(
            host_json,
            r#"{"provider_id":"claude-native","cwd":"/tmp","model":null,"mode":null,"mcp_servers":[],"metadata":null}"#
        );
    }

    #[test]
    fn actor_pair_partial_json_is_still_deserializable() {
        let only_actor: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "actor_ref": {"actor_kind":"creator","creator_id":"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
        }))
        .expect("partial actor_ref still parses");
        assert!(ActorAdmissionService::classify_pair(only_actor.actor_ref.is_some(), only_actor.viewpoint.is_some()).is_err());
        let only_view: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "viewpoint": {"world_id":"wld_worldA"}
        }))
        .expect("partial viewpoint still parses");
        assert!(ActorAdmissionService::classify_pair(only_view.actor_ref.is_some(), only_view.viewpoint.is_some()).is_err());
    }

    struct CountingHost {
        inner: Arc<dyn nexus_agent_host::HostFacade>,
        create_sessions: std::sync::atomic::AtomicU64,
        execs: std::sync::atomic::AtomicU64,
    }

    #[async_trait::async_trait]
    impl nexus_agent_host::HostFacade for CountingHost {
        async fn start(
            &self,
            config: nexus_agent_host::capability::model::HostStartConfig,
        ) -> nexus_agent_host::HostResult<()> {
            self.inner.start(config).await
        }

        async fn create_session(
            &self,
            request: nexus_agent_host::capability::CreateSessionRequest,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::HostSession> {
            self.create_sessions
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.create_session(request).await
        }

        async fn exec(
            &self,
            session_id: nexus_agent_host::HostSessionId,
            op: nexus_agent_host::capability::model::HostOperation,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostEventStream>
        {
            self.execs
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.exec(session_id, op).await
        }

        async fn cancel(
            &self,
            op_id: nexus_agent_host::HostOperationId,
        ) -> nexus_agent_host::HostResult<()> {
            self.inner.cancel(op_id).await
        }

        async fn health(&self) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostHealth> {
            self.inner.health().await
        }

        async fn shutdown(&self) -> nexus_agent_host::HostResult<()> {
            self.inner.shutdown().await
        }

        async fn shutdown_session(
            &self,
            session_id: nexus_agent_host::HostSessionId,
        ) -> nexus_agent_host::HostResult<()> {
            self.inner.shutdown_session(session_id).await
        }

        async fn list_sessions(&self) -> nexus_agent_host::HostResult<Vec<nexus_agent_host::HostSession>> {
            self.inner.list_sessions().await
        }

        async fn provider_catalog(&self) -> nexus_agent_host::HostResult<nexus_agent_host::ProviderCatalog> {
            self.inner.provider_catalog().await
        }

        fn subscribe_events(
            &self,
            session_id: nexus_agent_host::HostSessionId,
        ) -> tokio::sync::broadcast::Receiver<nexus_agent_host::capability::model::HostEvent> {
            self.inner.subscribe_events(session_id)
        }
    }

    async fn state_with_counting_host() -> (crate::test_utils::TestTempRoot, WorkspaceState, Arc<CountingHost>) {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n[active_workspace_slug_by_creator]\n\"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" = \"default\"\n",
        )
        .unwrap();
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let inner: Arc<dyn nexus_agent_host::HostFacade> = Arc::new(HostManager::new());
        let counting = Arc::new(CountingHost {
            inner,
            create_sessions: std::sync::atomic::AtomicU64::new(0),
            execs: std::sync::atomic::AtomicU64::new(0),
        });
        let host: Arc<dyn nexus_agent_host::HostFacade> = counting.clone();
        state.set_agent_host(host);
        (tmp, state, counting)
    }

    #[tokio::test]
    async fn partial_actor_pair_rejects_before_host_create() {
        let (_tmp, state, host) = state_with_counting_host().await;
        let req: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "nonexistent",
            "actor_ref": {"actor_kind":"creator","creator_id":"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
        }))
        .unwrap();
        let result = create_session(State(state), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "invalid_input");
        assert_eq!(host.create_sessions.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_character_admission_rejects_before_host_create() {
        let (_tmp, state, host) = state_with_counting_host().await;
        let req: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "nonexistent",
            "actor_ref": {
                "actor_kind":"character",
                "character_id":"chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            "viewpoint": {
                "world_id":"wld_worldA",
                "binding_id":"awb_cccccccccccccccccccccccccccccccc"
            }
        }))
        .unwrap();
        let result = create_session(State(state), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "not_found");
        assert_eq!(host.create_sessions.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }


    async fn seed_owned_character(state: &WorkspaceState) -> (String, String) {
        let pool = state.pool().unwrap();
        nexus_local_db::ensure_creator_row(pool, "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "Owner")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_worldA', 'ws', 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'w', 'w', 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .execute(pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: "wld_worldA",
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        (created.character.character_id, created.binding.binding_id)
    }

    fn character_session_req(character_id: &str, world_id: &str, binding_id: &str) -> CreateSessionRequest {
        serde_json::from_value(serde_json::json!({
            "provider_id": "nonexistent",
            "actor_ref": {"actor_kind":"character","character_id": character_id},
            "viewpoint": {"world_id": world_id, "binding_id": binding_id}
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn ownership_status_denies_before_host_side_effects() {
        let (_tmp, state, host) = state_with_counting_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let pool = state.pool().unwrap();

        let missing_world = serde_json::from_value(serde_json::json!({
            "provider_id": "nonexistent",
            "actor_ref": {"actor_kind":"creator","creator_id":"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "viewpoint": {"world_id":"wld_missing"}
        }))
        .unwrap();
        let err = create_session(State(state.clone()), Json(missing_world))
            .await
            .unwrap_err();
        assert_eq!(err.error_code(), "not_found");
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);

        sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = 'wld_worldA'")
            .execute(pool)
            .await
            .unwrap();
        let err = create_session(
            State(state.clone()),
            Json(character_session_req(&character_id, "wld_worldA", &binding_id)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "world_inactive");
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        sqlx::query("UPDATE narrative_worlds SET status = 'active' WHERE world_id = 'wld_worldA'")
            .execute(pool)
            .await
            .unwrap();

        sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
            .bind(&character_id)
            .execute(pool)
            .await
            .unwrap();
        let err = create_session(
            State(state.clone()),
            Json(character_session_req(&character_id, "wld_worldA", &binding_id)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "character_inactive");
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        sqlx::query("UPDATE characters SET status = 'active' WHERE character_id = ?")
            .bind(&character_id)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = ?")
            .bind(&binding_id)
            .execute(pool)
            .await
            .unwrap();
        let err = create_session(
            State(state.clone()),
            Json(character_session_req(&character_id, "wld_worldA", &binding_id)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "not_found");
        sqlx::query("UPDATE actor_world_bindings SET status = 'active' WHERE binding_id = ?")
            .bind(&binding_id)
            .execute(pool)
            .await
            .unwrap();

        let err = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                "awb_dddddddddddddddddddddddddddddddd",
            )),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "not_found");

        assert_eq!(host.create_sessions.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

}
