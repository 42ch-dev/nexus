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
//! - GET    /v1/daemon/agent-host/operations/{operation_id}            — Character operation outcome
//! - POST   /v1/daemon/agent-host/operations/{operation_id}:cancel — Cancel in-flight operation
//! - GET    /v1/daemon/agent-host/sessions/{session_id}/events     — SSE event stream

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use crate::actor_admission::{
    ActorAdmissionService, ActorPairMode, ActorViewpoint, AdmittedActorContext,
};
use crate::actor_knowledge_view::AdmittedActor;
use crate::actor_run_capture::{drain_and_finalize_character_operation, initial_capture_outcome};
use crate::api::handlers::world_kb_guards::require_creator;
use crate::workspace::actor_sessions::{
    echo_actor_pair, ActorSessionRegistry, CharacterOperationSnapshot,
};
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::StreamExt;
use nexus_contracts::generated::daemon_api::agent_host::{
    AgentHostListSessionsQuery, AgentScanEntry, CancelOperationResponse, CharacterOperationResult,
    CreateSessionRequest, ExecuteOperationRequest, OperationResponse, ScanRequest, ScanResponse,
    SessionListResponse, SessionResponse, ShutdownSessionResponse,
};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::SqliteKnowledgeStore;
use nexus_moment_context_assembly::{
    assemble_moment, CharacterViewInput, MomentActorContext, MomentRequest, Stage0Assembly,
    DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET,
};
use nexus_spoke_adapter::SpokeBackedKbStore;
use serde::Serialize;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::api::errors::{actor_session_stale, NexusApiError};
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
        "policy_denied" | "owner_workspace_mismatch" => NexusApiError::Forbidden {
            resource: "agent_host".into(),
            reason: e.to_string(),
        },
        "cleanup_unconfirmed" => NexusApiError::ServiceUnavailable {
            message: e.to_string(),
        },
        _ => NexusApiError::Internal {
            code: "AGENT_HOST_ERROR".into(),
            message: e.to_string(),
        },
    }
}

/// Parse a session ID path parameter as UUID.
///
/// Returns 400 Bad Request for malformed IDs (agent-host spec boundary rule:
/// 400 invalid id, 404 unknown session).
fn parse_session_id(raw: &str) -> Result<Uuid, NexusApiError> {
    raw.parse::<Uuid>()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: format!("must be a valid UUID, got: {raw}"),
        })
}

/// Authorize a Character-indexed (or retired) session against the active
/// Creator **before** Host interaction (durable §11.3.4). A foreign id is
/// `404 not_found` with no Host effect. Never-indexed legacy/Creator-direct
/// sessions keep existing behavior (no gate). Owner safety/observation
/// operations (cancel/shutdown/events) are allowed for the owning Creator;
/// they are not authored writes.
fn authorize_actor_session(
    state: &WorkspaceState,
    session_id: &nexus_agent_host::HostSessionId,
) -> Result<(), NexusApiError> {
    let Some((owner_creator_id, _kind, _retired)) =
        state.actor_sessions().stored_session_owner(session_id)
    else {
        // Never-indexed legacy path: unchanged behavior.
        return Ok(());
    };
    let active_creator = require_creator(state)?;
    if owner_creator_id == active_creator {
        Ok(())
    } else {
        Err(NexusApiError::NotFound(format!("session {session_id}")))
    }
}

/// Parse an operation ID path parameter as UUID.
///
/// Returns 400 Bad Request for malformed IDs (agent-host spec boundary rule).
fn parse_operation_id(raw: &str) -> Result<Uuid, NexusApiError> {
    raw.parse::<Uuid>()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "operation_id".into(),
            reason: format!("must be a valid UUID, got: {raw}"),
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
    let pair =
        ActorAdmissionService::classify_pair(req.actor_ref.is_some(), req.viewpoint.is_some())?;
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
        // For a Character create, admit one per-Character activity fence and
        // hold it through Host creation AND registry insertion, so an archive
        // cannot race an in-flight create into a reusable old-epoch session
        // (durable §11.3.4). The guard is dropped after the registry insert.
        let creation_guard = match &actor {
            AdmittedActor::Character { character_id } => Some(
                state
                    .actor_sessions()
                    .admit_character_activity(state.pool_or_uninit()?, &creator_id, character_id)
                    .await?,
            ),
            AdmittedActor::Creator { .. } => None,
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
        let cwd = session_cwd_path(&req, &state);
        let key = ActorSessionRegistry::key_for(
            &req.provider_id,
            &cwd,
            req.model.clone(),
            req.mode.clone(),
            &ctx,
        )?;
        let host_req = host_create_request(&req, &state, &ctx.owner_creator_id);
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
        drop(creation_guard);
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
    let owner_creator_id = require_creator(&state)?;
    let host_req = host_create_request(&req, &state, &owner_creator_id);
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

fn session_cwd_path(req: &CreateSessionRequest, state: &WorkspaceState) -> std::path::PathBuf {
    req.cwd
        .as_ref()
        .map_or_else(|| verified_workspace_root(state), std::path::PathBuf::from)
}

/// Resolve the canonical Creator workspace root from verified state.
///
/// The active workspace path is the trusted boundary for Host sessions; the
/// request body never supplies the workspace root.
fn verified_workspace_root(state: &WorkspaceState) -> std::path::PathBuf {
    state
        .workspace_path()
        .map_or_else(|| state.nexus_home().clone(), std::path::PathBuf::from)
}

/// Build the Host create request with verified owner metadata.
///
/// `owner_creator_id` comes from existing verified admission (the active
/// Creator at request time, or the admitted Character's owner) — never from
/// request-body assertions. The canonical workspace root is the active
/// workspace path.
fn host_create_request(
    req: &CreateSessionRequest,
    state: &WorkspaceState,
    owner_creator_id: &str,
) -> nexus_agent_host::capability::CreateSessionRequest {
    nexus_agent_host::capability::CreateSessionRequest {
        provider_id: nexus_agent_host::ProviderId::new(&req.provider_id),
        cwd: session_cwd_path(req, state),
        model: req.model.clone(),
        mode: req.mode.clone(),
        mcp_servers: vec![],
        metadata: serde_json::Value::Null,
        owner: nexus_agent_host::capability::model::SessionOwner {
            creator_id: owner_creator_id.to_string(),
            workspace_root: verified_workspace_root(state),
            orchestration_run_id: None,
        },
    }
}

const fn session_wire(
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
>{
    state
        .actor_sessions()
        .echo_actor_pair_for_session(session_id)
}

fn session_for_operation<'a>(
    sessions: &'a [nexus_agent_host::HostSession],
    op_id: &nexus_agent_host::HostOperationId,
) -> Option<&'a nexus_agent_host::HostSession> {
    sessions
        .iter()
        .find(|s| s.active_op_id.as_ref() == Some(op_id) || s.state.active_op_id() == Some(op_id))
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

    let limit = params.limit.unwrap_or(50).clamp(1, 250);
    let limit_us = usize::try_from(limit).unwrap_or(250);

    // Filter foreign Character/indexed sessions BEFORE pagination: the active
    // Creator only observes its own Actor sessions; a never-indexed legacy /
    // Creator-direct session is unchanged (durable §11.3.4).
    let active_creator = require_creator(&state).ok();
    // Cursor-based pagination: cursor is a session ID (UUID string).
    // If cursor is provided, skip entries until we find the cursor,
    // then return up to `limit` entries after it.
    let items: Vec<SessionResponse> = sessions
        .into_iter()
        .filter(
            |s| match state.actor_sessions().stored_session_owner(&s.id) {
                Some((owner, _kind, _retired)) => active_creator.as_deref() == Some(owner.as_str()),
                None => true, // never-indexed legacy path
            },
        )
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

    // Foreign/never-owned Character session id is a 404 before Host access.
    let sid = nexus_agent_host::HostSessionId(uuid);
    authorize_actor_session(&state, &sid)?;

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
    // Foreign id is 404 before any Host shutdown effect; owner may shut down
    // (resource cleanup is not an authored Character write).
    authorize_actor_session(&state, &sid)?;
    state
        .actor_sessions()
        .shutdown_session(sid, host.as_ref())
        .await?;

    Ok(Json(ShutdownSessionResponse {
        session_id,
        status: "shutdown".to_string(),
    }))
}

fn actor_shutting_down() -> NexusApiError {
    NexusApiError::ServiceUnavailable {
        message: "daemon is shutting down".into(),
    }
}

fn actor_model_mode_rejected() -> NexusApiError {
    NexusApiError::ConflictCoded {
        code: "actor_session_immutable".into(),
        message: "SetModel and SetMode are rejected on Actor-indexed sessions".into(),
    }
}

/// Rebuild prompt text for an Actor session: re-admit, then one `assemble_moment`.
///
/// For a Character session this also admits one per-Character activity fence
/// and verifies the indexed session epoch against the current stored epoch
/// after admission (mismatch → `actor_session_stale`); the guard is returned
/// so the caller moves it into the server-owned drain and holds it through
/// terminal finalization. Legacy/Creator sessions (never indexed) return no
/// guard.
///
/// A retired Actor id (tombstoned by a material transition) is never treated
/// as legacy: it fails with `actor_session_stale` unless the daemon is
/// shutting down, in which case it is `service_unavailable`.
async fn prepare_prompt(
    state: &WorkspaceState,
    session_id: &nexus_agent_host::HostSessionId,
    content: String,
) -> Result<
    (
        String,
        Option<crate::workspace::actor_sessions::CharacterActivityGuard>,
    ),
    NexusApiError,
> {
    if state.shutdown_requested() {
        return Err(actor_shutting_down());
    }
    let registry = state.actor_sessions();
    let Some(stored) = registry.context_for(session_id) else {
        if registry.is_actor_session(session_id) {
            // Retired (or closed) Actor id: never legacy fallback, never a
            // reused history. A closed registry reports shutdown.
            if registry.is_closed() {
                return Err(actor_shutting_down());
            }
            return Err(actor_session_stale(&session_id.to_string()));
        }
        // Never-indexed legacy/Creator path: unchanged raw prompt bytes.
        return Ok((content, None));
    };

    let creator_id = require_creator(state)?;
    if stored.owner_creator_id != creator_id {
        // Foreign Actor session id: 404 before any Host/MCA/provider access.
        return Err(NexusApiError::NotFound(format!("session {session_id}")));
    }

    let admission = ActorAdmissionService::new(state.pool_or_uninit()?.clone());
    // For a Character session, admit activity BEFORE MCA/Host so an archive
    // cannot race the prompt into a stale session (§11.3.1, §11.3.4).
    let activity_guard = match &stored.actor {
        AdmittedActor::Character { character_id } => {
            let guard = registry
                .admit_character_activity(state.pool_or_uninit()?, &creator_id, character_id)
                .await?;
            if stored.character_epoch != Some(guard.epoch()) {
                return Err(actor_session_stale(&session_id.to_string()));
            }
            Some(guard)
        }
        AdmittedActor::Creator { .. } => None,
    };

    let ctx = admission
        .admit(
            &creator_id,
            stored.actor.clone(),
            ActorViewpoint {
                world_id: stored.world_id.clone(),
                binding_id: stored.binding_id.clone(),
                branch_id: stored.branch_id.clone(),
                event_id: stored.event_id.clone(),
            },
        )
        .await?;
    let content = assemble_admitted_prompt(state, &ctx, content).await?;
    Ok((content, activity_guard))
}

async fn assemble_admitted_prompt(
    state: &WorkspaceState,
    ctx: &AdmittedActorContext,
    user_prompt: String,
) -> Result<String, NexusApiError> {
    let pool = state.pool().cloned().ok_or(NexusApiError::Uninitialized)?;
    // The active Creator is the only trusted owner of the admitted Character.
    let creator_id = require_creator(state)?;
    let view = CharacterViewInput::from_entries(ctx.view.items.clone());
    let actor = match &ctx.actor {
        AdmittedActor::Character { character_id } => {
            // v1.184 P3: project only the admitted Character's SOUL/Memory
            // (shared scope + the selected binding scope) into the reserved
            // mind slots. Honest-empty when optional data is absent.
            let binding_id =
                ctx.binding_id
                    .as_deref()
                    .ok_or_else(|| NexusApiError::InvalidInput {
                        field: "binding_id".into(),
                        reason: "Character host launch requires an active binding".into(),
                    })?;
            let mind =
                crate::api::handlers::memory_pipeline::load_character_mind_projection_with_tom(
                    &pool,
                    state.nexus_home(),
                    &creator_id,
                    character_id,
                    &ctx.world_id,
                    binding_id,
                )
                .await?;
            MomentActorContext::character_with_mind(view, mind)
        }
        AdmittedActor::Creator { .. } => MomentActorContext::creator_with_view(view),
    };
    let mut request = MomentRequest::new(Stage0Assembly {
        user_prompt,
        ..Stage0Assembly::default()
    })
    .with_world(ctx.world_id.clone())
    .with_actor(actor)
    .with_max_tokens(DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET);
    if let Some(branch) = ctx.branch_id.clone() {
        request = request.with_branch(branch);
    }
    if let Some(event) = ctx.event_id.clone() {
        request = request.with_event(event);
    }
    if matches!(ctx.actor, AdmittedActor::Creator { .. }) {
        request = request.with_user(creator_id);
    }
    let narrative = SqliteNarrativeGateway::new(pool.clone());
    let kb = SpokeBackedKbStore::new(pool.clone());
    let knowledge = SqliteKnowledgeStore::new(pool);
    let assembled = assemble_moment(&request, &narrative, &kb, &knowledge).await;
    Ok(assembled.to_full_context())
}

/// POST /v1/daemon/agent-host/sessions/{session_id}/operations
///
/// Execute a normalized host operation (prompt, `set_model`, `set_mode`).
/// Returns the operation ID for tracking.
#[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
pub async fn execute_operation(
    State(state): State<WorkspaceState>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteOperationRequest>,
) -> Result<Json<OperationResponse>, NexusApiError> {
    let uuid = parse_session_id(&session_id)?;
    if state.shutdown_requested() {
        return Err(actor_shutting_down());
    }
    let host = get_host(&state)?;

    let sid = nexus_agent_host::HostSessionId(uuid);
    if state.actor_sessions().is_actor_session(&sid)
        && matches!(
            req,
            ExecuteOperationRequest::SetModel { .. } | ExecuteOperationRequest::SetMode { .. }
        )
    {
        return Err(actor_model_mode_rejected());
    }
    let op_id = nexus_agent_host::HostOperationId::new();

    // Activity guard is admitted (for a Character Prompt) before MCA/Host; it
    // is MOVED into the server-owned drain and held through terminal
    // finalization, NOT dropped at HTTP return or Host Ready (durable
    // §11.3.1/§11.6). No SSE subscriber is required for the guard's lifetime.
    let host_op = match req {
        ExecuteOperationRequest::Prompt { content, remember } => {
            let remember = remember.unwrap_or(false);
            let raw_prompt = content.clone();
            let is_character = match state.actor_sessions().context_for(&sid) {
                // Retired tombstones still identify Character sessions: let
                // prepare_prompt report 409 actor_session_stale instead of
                // misclassifying them as legacy/Creator sessions.
                Some(ctx) => matches!(ctx.actor, AdmittedActor::Character { .. }),
                None => state.actor_sessions().is_actor_session(&sid),
            };
            if remember && !is_character {
                return Err(NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "remember requires an admitted stored Character session with an active binding".into(),
                });
            }
            let (content, guard) = prepare_prompt(&state, &sid, content).await?;
            let capture = if is_character {
                Some(initial_capture_outcome(remember))
            } else {
                None
            };
            let snapshot = if is_character {
                let ctx = state
                    .actor_sessions()
                    .context_for(&sid)
                    .expect("character session context");
                let creator_id = require_creator(&state)?;
                let snapshot = CharacterOperationSnapshot {
                    owner_creator_id: creator_id,
                    ctx,
                    session_id: sid.clone(),
                    operation_id: op_id.clone(),
                    remember,
                    raw_prompt: raw_prompt.clone(),
                };
                state
                    .actor_sessions()
                    .reserve_character_operation(snapshot.clone())?;
                state
                    .actor_sessions()
                    .register_indexed_operation(op_id.clone(), sid.clone());
                Some(snapshot)
            } else {
                None
            };
            let host_op = nexus_agent_host::capability::model::HostOperation::Prompt {
                op_id: op_id.clone(),
                content: vec![
                    nexus_agent_host::capability::model::HostContentBlock::Text { text: content },
                ],
                permission_scope: None,
            };
            let stream = host.exec(sid.clone(), host_op).await.map_err(|e| {
                if snapshot.is_some() {
                    state.actor_sessions().remove_operation_reservation(&op_id);
                    state.actor_sessions().clear_indexed_operation(&op_id);
                }
                map_host_error(&e)
            })?;
            if let Some(snapshot) = snapshot {
                let pool = state.pool_or_uninit()?.clone();
                let registry = state.actor_sessions().clone();
                tokio::spawn(async move {
                    drain_and_finalize_character_operation(pool, registry, stream, guard, snapshot)
                        .await;
                });
            } else {
                tokio::spawn(drive_operation_stream(stream, guard));
            }
            return Ok(Json(OperationResponse {
                operation_id: op_id.to_string(),
                session_id: sid.to_string(),
                status: "started".to_string(),
                capture,
            }));
        }
        ExecuteOperationRequest::SetModel { model } => {
            nexus_agent_host::capability::model::HostOperation::SetModel { model }
        }
        ExecuteOperationRequest::SetMode { mode } => {
            nexus_agent_host::capability::model::HostOperation::SetMode { mode }
        }
    };

    // Non-Prompt operations (SetModel/SetMode) are not Character side-effecting
    // captures; execute fire-and-forget as before, no activity guard needed.
    let stream = host
        .exec(sid.clone(), host_op)
        .await
        .map_err(|e| map_host_error(&e))?;

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
        capture: None,
    }))
}

/// Drain an operation event stream to completion, releasing the optional
/// per-Character activity guard only after the stream terminates (terminal
/// finalization) — never at HTTP return or when the Host transitions Ready.
async fn drive_operation_stream(
    mut stream: impl futures_util::Stream<
            Item = Result<
                nexus_agent_host::capability::model::HostEvent,
                nexus_agent_host::HostError,
            >,
        > + Unpin,
    _activity_guard: Option<crate::workspace::actor_sessions::CharacterActivityGuard>,
) {
    while let Some(_result) = stream.next().await {
        // Events are broadcast by HostManager; draining drives the state machine.
    }
    // `_activity_guard` drops here, after terminal finalization.
}

/// GET /v1/daemon/agent-host/operations/{operation_id}
///
/// Returns the authoritative Character operation outcome for the active owner.
pub async fn get_operation_result(
    State(state): State<WorkspaceState>,
    Path(operation_id): Path<String>,
) -> Result<Json<CharacterOperationResult>, NexusApiError> {
    if operation_id.ends_with(":cancel") {
        return Err(NexusApiError::NotFound(format!(
            "Operation route '{operation_id}' not found"
        )));
    }
    let uuid = parse_operation_id(&operation_id)?;
    let creator_id = require_creator(&state)?;
    let op_id = nexus_agent_host::HostOperationId(uuid);
    let result = state
        .actor_sessions()
        .character_operation_result(&creator_id, &op_id)?;
    Ok(Json(result))
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

    let op_id = nexus_agent_host::HostOperationId(uuid);
    let creator_id = require_creator(&state)?;
    let host = get_host(&state)?;

    if state.actor_sessions().operation_owner(&op_id).is_some() {
        state
            .actor_sessions()
            .request_operation_cancel(&creator_id, &op_id)?;
        let session_id = state
            .actor_sessions()
            .operation_session_id(&op_id)
            .or_else(|| {
                state
                    .actor_sessions()
                    .resolve_indexed_operation_session(&op_id)
            })
            .ok_or_else(|| NexusApiError::NotFound(format!("operation {operation_id}")))?;
        authorize_actor_session(&state, &session_id)?;
    } else {
        let sessions = host.list_sessions().await.map_err(|e| map_host_error(&e))?;
        let indexed_session = session_for_operation(&sessions, &op_id)
            .map(|s| s.id.clone())
            .or_else(|| {
                state
                    .actor_sessions()
                    .resolve_indexed_operation_session(&op_id)
            });
        if let Some(session_id) = indexed_session {
            authorize_actor_session(&state, &session_id)?;
            state.actor_sessions().clear_indexed_operation(&op_id);
        }
    }
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
    authorize_actor_session(&state, &sid)?;
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
        .filter(|entry| native_entry_installed(entry))
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

    agents.extend(native_entries.into_iter().map(|entry| {
        let installed = native_entry_installed(&entry);
        map_native_catalog_entry(entry, &by_binary, installed)
    }));

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

/// Whether a discovered native CLI candidate is INSTALLED on this machine.
///
/// The wire contract defines `installed` as "the binary referenced by
/// `launch_command` is found on the system PATH via a which-equivalent lookup" —
/// PRESENCE, not probe-proven readiness. `ProviderHealth.available` deliberately
/// means the stronger "a bounded probe proved the provider READY" (v1.188 P2), so
/// it cannot answer this question: an unprobed candidate is still installed.
///
/// A candidate whose command does not resolve stays false, so an invalid
/// `DSH_RUNTIME_BIN` override keeps reporting the honest not-installed.
fn native_entry_installed(entry: &nexus_agent_host::ProviderCatalogEntry) -> bool {
    match &entry.launch {
        nexus_agent_host::LaunchStrategy::NativeCli { command, .. } => {
            which::which(command).is_ok()
        }
        // ACP candidates are judged by the registry scan, not here.
        nexus_agent_host::LaunchStrategy::Acp { .. } => false,
    }
}

/// Map a native CLI catalog entry to an [`AgentScanEntry`].
///
/// `installed` is decided by the caller ([`native_entry_installed`]) because the
/// catalog entry's `health` reports probe readiness, which is a different — and
/// strictly stronger — claim than "the binary is present on this machine".
fn map_native_catalog_entry(
    entry: nexus_agent_host::ProviderCatalogEntry,
    by_binary: &HashMap<String, nexus_acp_host::registry::LocalInstallation>,
    installed: bool,
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
        installed,
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
            remember: None,
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
        // agent-host spec boundary rule: malformed id -> 400 InvalidInput.
        for raw in ["garbage", "", "12345", "../../etc/passwd"] {
            match parse_session_id(raw) {
                Err(NexusApiError::InvalidInput { .. }) => {}
                other => panic!("session_id {raw:?}: expected 400 InvalidInput, got {other:?}"),
            }
        }
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
        // agent-host spec boundary rule: malformed id -> 400 InvalidInput.
        for raw in ["garbage", ""] {
            match parse_operation_id(raw) {
                Err(NexusApiError::InvalidInput { .. }) => {}
                other => panic!("operation_id {raw:?}: expected 400 InvalidInput, got {other:?}"),
            }
        }
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
        let server = TestServer::new(app);

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
        let server = TestServer::new(app);

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
        let server = TestServer::new(app);

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

        let scan_entry = map_native_catalog_entry(entry, &by_binary, true);
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

    #[tokio::test]
    async fn legacy_create_request_uses_verified_owner_without_actor_pair() {
        let req: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "cwd": "/tmp"
        }))
        .expect("legacy body");
        assert!(req.actor_ref.is_none());
        assert!(req.viewpoint.is_none());

        let state = state_with_host().await;
        let host_req = host_create_request(&req, &state, "ctr_verified");
        assert!(host_req.metadata.is_null());
        assert!(host_req.mcp_servers.is_empty());
        assert_eq!(host_req.owner.creator_id, "ctr_verified");
        assert_eq!(
            host_req.owner.workspace_root,
            verified_workspace_root(&state)
        );
    }

    #[test]
    fn actor_pair_partial_json_is_still_deserializable() {
        let only_actor: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "actor_ref": {"actor_kind":"creator","creator_id":"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
        }))
        .expect("partial actor_ref still parses");
        assert!(ActorAdmissionService::classify_pair(
            only_actor.actor_ref.is_some(),
            only_actor.viewpoint.is_some()
        )
        .is_err());
        let only_view: CreateSessionRequest = serde_json::from_value(serde_json::json!({
            "provider_id": "claude-native",
            "viewpoint": {"world_id":"wld_worldA"}
        }))
        .expect("partial viewpoint still parses");
        assert!(ActorAdmissionService::classify_pair(
            only_view.actor_ref.is_some(),
            only_view.viewpoint.is_some()
        )
        .is_err());
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
            self.execs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.exec(session_id, op).await
        }

        async fn cancel(
            &self,
            op_id: nexus_agent_host::HostOperationId,
        ) -> nexus_agent_host::HostResult<()> {
            self.inner.cancel(op_id).await
        }

        async fn health(
            &self,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostHealth> {
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

        async fn list_sessions(
            &self,
        ) -> nexus_agent_host::HostResult<Vec<nexus_agent_host::HostSession>> {
            self.inner.list_sessions().await
        }

        async fn provider_catalog(
            &self,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::ProviderCatalog> {
            self.inner.provider_catalog().await
        }

        fn subscribe_events(
            &self,
            session_id: nexus_agent_host::HostSessionId,
        ) -> tokio::sync::broadcast::Receiver<nexus_agent_host::capability::model::HostEvent>
        {
            self.inner.subscribe_events(session_id)
        }
    }

    async fn state_with_counting_host() -> (
        crate::test_utils::TestTempRoot,
        WorkspaceState,
        Arc<CountingHost>,
    ) {
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
        assert_eq!(
            host.create_sessions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
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
        assert_eq!(
            host.create_sessions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
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

    fn character_session_req(
        character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> CreateSessionRequest {
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

        sqlx::query(
            "UPDATE narrative_worlds SET status = 'archived' WHERE world_id = 'wld_worldA'",
        )
        .execute(pool)
        .await
        .unwrap();
        let err = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
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
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
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
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
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

        assert_eq!(
            host.create_sessions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    fn rebind_host_event(
        event: nexus_agent_host::capability::model::HostEvent,
        session_id: nexus_agent_host::HostSessionId,
        op_id: nexus_agent_host::HostOperationId,
    ) -> nexus_agent_host::capability::model::HostEvent {
        use nexus_agent_host::capability::model::{
            HostEvent, OperationFailedEvent, OperationFinishedEvent, PlanUpdateEvent,
            TextDeltaEvent, ToolCallEvent, ToolCallUpdateEvent,
        };
        match event {
            HostEvent::MessageDelta(TextDeltaEvent { text, .. }) => {
                HostEvent::MessageDelta(TextDeltaEvent {
                    session_id,
                    op_id,
                    text,
                })
            }
            HostEvent::ThoughtDelta(TextDeltaEvent { text, .. }) => {
                HostEvent::ThoughtDelta(TextDeltaEvent {
                    session_id,
                    op_id,
                    text,
                })
            }
            HostEvent::OpFinished(OperationFinishedEvent { reason, .. }) => {
                HostEvent::OpFinished(OperationFinishedEvent {
                    session_id,
                    op_id,
                    reason,
                })
            }
            HostEvent::OpFailed(OperationFailedEvent {
                error_category,
                error_message,
                ..
            }) => HostEvent::OpFailed(OperationFailedEvent {
                session_id,
                op_id,
                error_category,
                error_message,
            }),
            HostEvent::ToolCall(ToolCallEvent {
                tool_call_id,
                tool_name,
                ..
            }) => HostEvent::ToolCall(ToolCallEvent {
                session_id,
                op_id,
                tool_call_id,
                tool_name,
            }),
            HostEvent::ToolCallUpdate(ToolCallUpdateEvent {
                tool_call_id,
                content,
                ..
            }) => HostEvent::ToolCallUpdate(ToolCallUpdateEvent {
                session_id,
                op_id,
                tool_call_id,
                content,
            }),
            HostEvent::PlanUpdate(PlanUpdateEvent { content, .. }) => {
                HostEvent::PlanUpdate(PlanUpdateEvent {
                    session_id,
                    op_id,
                    content,
                })
            }
            other => other,
        }
    }

    struct PromptHost {
        sessions: std::sync::Mutex<
            std::collections::HashMap<
                nexus_agent_host::HostSessionId,
                nexus_agent_host::HostSession,
            >,
        >,
        ops: std::sync::Mutex<Vec<nexus_agent_host::capability::model::HostOperation>>,
        stream_scripts: std::sync::Mutex<
            std::collections::VecDeque<Vec<nexus_agent_host::capability::model::HostEvent>>,
        >,
        // When true, queued stream events get rebound to the exec session/op.
        execs: std::sync::atomic::AtomicU64,
        cancels: std::sync::atomic::AtomicU64,
        shutdowns: std::sync::atomic::AtomicU64,
        events: tokio::sync::broadcast::Sender<nexus_agent_host::capability::model::HostEvent>,
    }

    impl PromptHost {
        fn new() -> Arc<Self> {
            let (events, _) = tokio::sync::broadcast::channel(16);
            Arc::new(Self {
                sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
                ops: std::sync::Mutex::new(Vec::new()),
                stream_scripts: std::sync::Mutex::new(std::collections::VecDeque::new()),
                execs: std::sync::atomic::AtomicU64::new(0),
                cancels: std::sync::atomic::AtomicU64::new(0),
                shutdowns: std::sync::atomic::AtomicU64::new(0),
                events,
            })
        }

        fn last_prompt_text(&self) -> String {
            match self.ops.lock().expect("ops").last() {
                Some(nexus_agent_host::capability::model::HostOperation::Prompt {
                    content,
                    ..
                }) => match content.as_slice() {
                    [nexus_agent_host::capability::model::HostContentBlock::Text { text }] => {
                        text.clone()
                    }
                    _ => panic!("expected one text block"),
                },
                other => panic!("expected prompt, got {other:?}"),
            }
        }

        fn queue_stream(&self, events: Vec<nexus_agent_host::capability::model::HostEvent>) {
            self.stream_scripts
                .lock()
                .expect("stream_scripts")
                .push_back(events);
        }
    }

    #[async_trait::async_trait]
    impl nexus_agent_host::HostFacade for PromptHost {
        async fn start(
            &self,
            _config: nexus_agent_host::capability::model::HostStartConfig,
        ) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn create_session(
            &self,
            request: nexus_agent_host::capability::CreateSessionRequest,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::HostSession> {
            let session = nexus_agent_host::HostSession {
                id: nexus_agent_host::HostSessionId::new(),
                provider_id: request.provider_id,
                state: nexus_agent_host::SessionState::Ready,
                created_at: chrono::Utc::now(),
                active_op_id: None,
                negotiated_capabilities:
                    nexus_agent_host::capability::model::CapabilityDescriptor::native_cli_limited(),
                owner: request.owner,
                process_identity: None,
            };
            self.sessions
                .lock()
                .expect("sessions")
                .insert(session.id.clone(), session.clone());
            Ok(session)
        }

        async fn exec(
            &self,
            session_id: nexus_agent_host::HostSessionId,
            op: nexus_agent_host::capability::model::HostOperation,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostEventStream>
        {
            self.execs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.ops.lock().expect("ops").push(op.clone());
            let script = self
                .stream_scripts
                .lock()
                .expect("stream_scripts")
                .pop_front()
                .unwrap_or_default();
            let rebound = match &op {
                nexus_agent_host::capability::model::HostOperation::Prompt { op_id, .. } => script
                    .into_iter()
                    .map(|event| rebind_host_event(event, session_id.clone(), op_id.clone()))
                    .collect::<Vec<_>>(),
                _ => script,
            };
            Ok(Box::pin(futures_util::stream::iter(
                rebound.into_iter().map(Ok),
            )))
        }

        async fn cancel(
            &self,
            _op_id: nexus_agent_host::HostOperationId,
        ) -> nexus_agent_host::HostResult<()> {
            self.cancels
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn health(
            &self,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostHealth> {
            Ok(nexus_agent_host::capability::model::HostHealth {
                running: true,
                active_sessions: self.sessions.lock().expect("sessions").len(),
                active_operations: 0,
            })
        }

        async fn shutdown(&self) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn shutdown_session(
            &self,
            session_id: nexus_agent_host::HostSessionId,
        ) -> nexus_agent_host::HostResult<()> {
            self.shutdowns
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.sessions
                .lock()
                .expect("sessions")
                .remove(&session_id)
                .ok_or_else(|| nexus_agent_host::HostError::internal("session"))?;
            Ok(())
        }

        async fn list_sessions(
            &self,
        ) -> nexus_agent_host::HostResult<Vec<nexus_agent_host::HostSession>> {
            Ok(self
                .sessions
                .lock()
                .expect("sessions")
                .values()
                .cloned()
                .collect())
        }

        async fn provider_catalog(
            &self,
        ) -> nexus_agent_host::HostResult<nexus_agent_host::ProviderCatalog> {
            Ok(nexus_agent_host::ProviderCatalog::new())
        }

        fn subscribe_events(
            &self,
            _session_id: nexus_agent_host::HostSessionId,
        ) -> tokio::sync::broadcast::Receiver<nexus_agent_host::capability::model::HostEvent>
        {
            self.events.subscribe()
        }
    }

    async fn state_with_prompt_host() -> (
        crate::test_utils::TestTempRoot,
        WorkspaceState,
        Arc<PromptHost>,
    ) {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n[active_workspace_slug_by_creator]\n\"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" = \"default\"\n",
        )
        .unwrap();
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let host = PromptHost::new();
        let facade: Arc<dyn nexus_agent_host::HostFacade> = host.clone();
        state.set_agent_host(facade);
        (tmp, state, host)
    }

    #[test]
    fn legacy_prompt_json_bytes_are_kind_and_content_only() {
        let req: ExecuteOperationRequest =
            serde_json::from_str(r#"{"kind":"prompt","content":"hello"}"#).expect("legacy prompt");
        let json = serde_json::to_string(&req).expect("json");
        assert_eq!(json, r#"{"kind":"prompt","content":"hello"}"#);
    }

    #[tokio::test]
    async fn legacy_prompt_submits_raw_bytes_unchanged() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp"
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("legacy create");
        let result = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "hello".to_string(),
                remember: None,
            }),
        )
        .await
        .expect("legacy prompt");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(host.last_prompt_text(), "hello");
        assert_eq!(host.ops.lock().expect("ops").len(), 1);
        assert_eq!(result.session_id, created.session_id.clone());
    }

    #[tokio::test]
    async fn character_prompt_re_admits_empty_headings_and_one_prompt() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp",
                    "actor_ref": {"actor_kind":"character","character_id": character_id},
                    "viewpoint": {"world_id":"wld_worldA","binding_id": binding_id}
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("create actor session");
        let _ = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "Act now.".to_string(),
                remember: None,
            }),
        )
        .await
        .expect("prompt");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 1);
        let text = host.last_prompt_text();
        assert!(text.contains("## Character SOUL"));
        assert!(text.contains("## Character Memory"));
        assert!(text.contains("## Character ToM — L1"));
        assert!(text.contains("## Character ToM — L2"));
        assert!(text.contains("Act now."));
        assert!(!text.contains("## Personality"));
        let ops = host.ops.lock().expect("ops");
        match ops.as_slice() {
            [nexus_agent_host::capability::model::HostOperation::Prompt { content, .. }] => {
                assert_eq!(content.len(), 1);
            }
            other => panic!("expected one prompt, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn removed_binding_blocks_prompt_before_host_exec() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp",
                    "actor_ref": {"actor_kind":"character","character_id": character_id},
                    "viewpoint": {"world_id":"wld_worldA","binding_id": binding_id}
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("create");
        sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = ?")
            .bind(&binding_id)
            .execute(state.pool().unwrap())
            .await
            .unwrap();
        let err = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "Act now.".to_string(),
                remember: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "not_found");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn shutdown_rejects_actor_prompt_before_host_exec() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp",
                    "actor_ref": {"actor_kind":"character","character_id": character_id},
                    "viewpoint": {"world_id":"wld_worldA","binding_id": binding_id}
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("create");
        state.request_shutdown();
        let err = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "Act now.".to_string(),
                remember: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "service_unavailable");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn set_model_and_set_mode_reject_on_actor_session() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp",
                    "actor_ref": {"actor_kind":"character","character_id": character_id},
                    "viewpoint": {"world_id":"wld_worldA","binding_id": binding_id}
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("create");
        let err = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::SetModel {
                model: "opus".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "actor_session_immutable");
        let err = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::SetMode {
                mode: "plan".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "actor_session_immutable");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    // ── v1.185 P0 Task 2: session authorization + retired execute stale ──

    /// Create a Character-indexed Actor session through the public create
    /// handler (requires an owned active Character + active binding + World).
    async fn create_actor_session(state: &WorkspaceState) -> (String, String, String) {
        let (character_id, binding_id) = seed_owned_character(state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create actor session");
        (created.session_id.clone(), character_id, binding_id)
    }

    /// A foreign (different active creator) Character session id is a 404 for
    /// get/shutdown/cancel with NO Host effect; the owning creator can still
    /// observe it.
    #[tokio::test]
    async fn foreign_character_session_read_and_shutdown_404_without_host_effect() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (session_id, character_id, _binding_id) = create_actor_session(&state).await;

        // Point the active creator at a DIFFERENT creator (foreign owner).
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n\n[active_workspace_slug_by_creator]\n\"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\" = \"default\"\n",
        )
        .unwrap();

        // Foreign read → 404, no list/host effect.
        let err = get_session(State(state.clone()), Path(session_id.clone()))
            .await
            .expect_err("foreign get");
        assert_eq!(err.error_code(), "not_found");

        // Foreign shutdown → 404, host session untouched.
        let err = shutdown_session(State(state.clone()), Path(session_id.clone()))
            .await
            .expect_err("foreign shutdown");
        assert_eq!(err.error_code(), "not_found");
        assert_eq!(
            host.shutdowns.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "foreign shutdown must not reach Host"
        );

        // Switch back to the owning creator: owned observation retained.
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n[active_workspace_slug_by_creator]\n\"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" = \"default\"\n",
        )
        .unwrap();
        let ok = get_session(State(state.clone()), Path(session_id))
            .await
            .expect("owned get");
        assert_eq!(ok.state, "Ready");
        assert_eq!(character_id.len(), 36); // sanity: seeded character id shape
    }

    /// A retired-session execute (post-lifecycle-transition id) is a `409
    /// actor_session_stale` and never falls back to legacy/Creator bytes.
    #[tokio::test]
    async fn retired_actor_session_execute_is_stale_not_legacy() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (session_id, character_id, _binding_id) = create_actor_session(&state).await;
        let _ = state
            .actor_sessions()
            .retire_character_sessions(&character_id);

        let err = execute_operation(
            State(state.clone()),
            Path(session_id),
            Json(ExecuteOperationRequest::Prompt {
                content: "do a thing".into(),
                remember: None,
            }),
        )
        .await
        .expect_err("retired execute must be stale");
        assert_eq!(err.error_code(), "actor_session_stale");
        assert_eq!(
            host.execs.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "retired prompt must be rejected before Host exec"
        );
    }

    /// A foreign cancel of an operation mapped to an indexed Character
    /// session is a 404 with no Host cancel effect.
    #[tokio::test]
    async fn foreign_cancel_resolves_operation_to_session_and_404s() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (session_id, _character_id, _binding_id) = create_actor_session(&state).await;

        // Simulate an in-flight operation bound to that session.
        let op_uuid = nexus_agent_host::HostOperationId::new();
        if let Some(session) =
            host.sessions
                .lock()
                .expect("sessions")
                .get_mut(&nexus_agent_host::HostSessionId(
                    session_id.parse().expect("uuid"),
                ))
        {
            session.active_op_id = Some(op_uuid.clone());
        }

        // Foreign creator cancels → 404, host cancel not called.
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n\n[active_workspace_slug_by_creator]\n\"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\" = \"default\"\n",
        )
        .unwrap();
        let err = cancel_operation(State(state.clone()), Path(format!("{op_uuid}:cancel")))
            .await
            .expect_err("foreign cancel");
        assert_eq!(err.error_code(), "not_found");
        assert_eq!(
            host.cancels.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "foreign cancel must not reach Host"
        );
    }

    /// The session list filters foreign Character-indexed rows before
    /// pagination: a foreign Character session never appears in another
    /// Cancel of an indexed Character operation is fail-closed when Host has
    /// not yet surfaced `active_op_id`: foreign caller → 404, zero Host cancels.
    #[tokio::test]
    async fn foreign_cancel_without_host_active_op_id_404s_with_zero_host_cancels() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (session_id, _character_id, _binding_id) = create_actor_session(&state).await;
        let started = execute_operation(
            State(state.clone()),
            Path(session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "do a thing".into(),
                remember: None,
            }),
        )
        .await
        .expect("start prompt");
        let op_uuid =
            nexus_agent_host::HostOperationId(started.operation_id.parse().expect("uuid"));
        assert!(
            host.sessions
                .lock()
                .expect("sessions")
                .get(&nexus_agent_host::HostSessionId(
                    session_id.parse().expect("uuid")
                ))
                .expect("session")
                .active_op_id
                .is_none(),
            "test models Host before active_op_id is indexed"
        );

        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"

[active_workspace_slug_by_creator]
\"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\" = \"default\"
",
        )
        .unwrap();
        let err = cancel_operation(State(state.clone()), Path(format!("{op_uuid}:cancel")))
            .await
            .expect_err("foreign cancel");
        assert_eq!(err.error_code(), "not_found");
        assert_eq!(
            host.cancels.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "foreign cancel must not reach Host"
        );
    }

    /// Retired Character sessions still overlay `actor_ref/viewpoint` on list/get.
    #[tokio::test]
    async fn retired_session_overlay_keeps_actor_ref_on_list_and_get() {
        let (_tmp, state, _host) = state_with_prompt_host().await;
        let (session_id, character_id, _binding_id) = create_actor_session(&state).await;
        let _ = state
            .actor_sessions()
            .retire_character_sessions(&character_id);

        let listed = list_sessions(
            State(state.clone()),
            Query(AgentHostListSessionsQuery {
                limit: None,
                cursor: None,
            }),
        )
        .await
        .expect("list");
        let row = listed
            .items
            .iter()
            .find(|s| s.session_id == session_id)
            .expect("retired host row still listed");
        assert!(
            row.actor_ref.is_some(),
            "retired row keeps actor_ref overlay"
        );
        assert!(
            row.viewpoint.is_some(),
            "retired row keeps viewpoint overlay"
        );

        let got = get_session(State(state.clone()), Path(session_id.clone()))
            .await
            .expect("get retired session");
        assert!(got.actor_ref.is_some());
        assert!(got.viewpoint.is_some());
    }

    /// The session list filters foreign Character-indexed rows before
    /// pagination: a foreign Character session never appears in another
    /// creator's list, while the owning creator's own indexed session does.
    #[tokio::test]
    async fn session_list_filters_foreign_character_rows_before_pagination() {
        let (_tmp, state, _host) = state_with_prompt_host().await;
        let (_own_session, _own_char, _own_bind) = create_actor_session(&state).await;

        // Seed an OTHER-owned World + Character so a foreign Character session
        // can be indexed under a different owner.
        let pool = state.pool().unwrap();
        nexus_local_db::ensure_creator_row(pool, "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "Other")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_worldB', 'ws', 'ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'b', 'b', 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .execute(pool)
        .await
        .unwrap();
        let other_created = nexus_local_db::create_character_with_initial_binding(
            pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                display_name: "OtherChar",
                image_uri: None,
                persona_json: "{}",
                world_id: "wld_worldB",
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();

        // Index the OTHER-owned session via the OTHER active creator.
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n\n[active_workspace_slug_by_creator]\n\"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\" = \"default\"\n",
        )
        .unwrap();
        let _ = create_session(
            State(state.clone()),
            Json(character_session_req(
                &other_created.character.character_id,
                "wld_worldB",
                &other_created.binding.binding_id,
            )),
        )
        .await
        .expect("other creator session");

        // Back to the first creator: the OTHER-owned Character session must
        // be filtered out of the list (only the own indexed session remains).
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n[active_workspace_slug_by_creator]\n\"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" = \"default\"\n",
        )
        .unwrap();
        let listed = list_sessions(
            State(state.clone()),
            Query(AgentHostListSessionsQuery {
                limit: Some(50),
                cursor: None,
            }),
        )
        .await
        .expect("list ok");
        assert_eq!(listed.items.len(), 1, "foreign Character session filtered");
    }

    async fn wait_for_terminal_operation(state: &WorkspaceState, operation_id: &str) {
        use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::CharacterOperationResultRunStatus;
        let op = nexus_agent_host::HostOperationId(
            uuid::Uuid::parse_str(operation_id).expect("op uuid"),
        );
        for _ in 0..100 {
            if let Ok(result) = state
                .actor_sessions()
                .character_operation_result("ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &op)
            {
                if !matches!(
                    result.run_status,
                    CharacterOperationResultRunStatus::Running
                ) {
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("operation {operation_id} did not reach terminal state");
    }

    fn capture_script(
        session_id: &nexus_agent_host::HostSessionId,
        op_id: &nexus_agent_host::HostOperationId,
        text: &str,
        reason: nexus_agent_host::capability::model::FinishReason,
    ) -> Vec<nexus_agent_host::capability::model::HostEvent> {
        use nexus_agent_host::capability::model::{
            HostEvent, OperationFinishedEvent, TextDeltaEvent,
        };
        vec![
            HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text: text.into(),
            }),
            HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                reason,
            }),
        ]
    }

    /// remember:true on a RETIRED Character session is not `invalid_input`:
    /// it reaches `prepare_prompt` and reports 409 `actor_session_stale`.
    #[tokio::test]
    async fn remember_on_retired_character_session_reports_stale() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        // Retire the session (simulates a material archive).
        let _ = state
            .actor_sessions()
            .retire_character_sessions(&character_id);
        let err = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "hello".into(),
                remember: Some(true),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(err.error_code(), "actor_session_stale");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// remember:true on legacy/Creator sessions is rejected before exec.
    #[tokio::test]
    async fn remember_rejects_legacy_prompt_before_exec() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let created = create_session(
            State(state.clone()),
            Json(
                serde_json::from_value(serde_json::json!({
                    "provider_id": "prov",
                    "cwd": "/tmp"
                }))
                .unwrap(),
            ),
        )
        .await
        .expect("legacy create");
        let err = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "hello".into(),
                remember: Some(true),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(err.error_code(), "invalid_input");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// Character prompt with remember returns pending capture immediately.
    #[tokio::test]
    async fn character_remember_returns_pending_capture() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        host.queue_stream(vec![]);
        let resp = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "Act now.".into(),
                remember: Some(true),
            }),
        )
        .await
        .expect("prompt");
        assert_eq!(resp.0.status, "started");
        let capture = resp.0.capture.expect("capture");
        assert_eq!(capture.status.to_string(), "pending");
    }

    /// Drain-finalized `end_turn` with remember persists capture candidate.
    #[tokio::test]
    async fn character_end_turn_capture_persists_pending_row() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        let sid = nexus_agent_host::HostSessionId::new();
        let op_id = nexus_agent_host::HostOperationId::new();
        host.queue_stream(capture_script(
            &sid,
            &op_id,
            "visible answer",
            nexus_agent_host::capability::model::FinishReason::EndTurn,
        ));
        let resp = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "user prompt".into(),
                remember: Some(true),
            }),
        )
        .await
        .expect("prompt");
        // Wait for server-owned drain to finalize.
        wait_for_terminal_operation(&state, &resp.0.operation_id).await;
        let Json(outcome) =
            get_operation_result(State(state.clone()), Path(resp.0.operation_id.clone()))
                .await
                .expect("get outcome");
        assert_eq!(outcome.run_status.to_string(), "succeeded");
        assert_eq!(outcome.capture.status.to_string(), "captured");
        assert!(outcome.capture.pending_id.is_some());
        let operation_id = resp.0.operation_id.clone();
        let pending_id = outcome.capture.pending_id.unwrap().to_string();
        let row: (String, Option<String>) = sqlx::query_as(
            "SELECT pending_id, source_operation_id FROM character_memory_pending_review WHERE pending_id = ?",
        )
        .bind(&pending_id)
        .fetch_one(state.pool().unwrap())
        .await
        .expect("pending row");
        assert_eq!(row.1.as_deref(), Some(operation_id.as_str()));
    }

    /// Cancel after terminal finalization returns `actor_operation_finished`.
    #[tokio::test]
    async fn cancel_after_terminal_returns_finished_conflict() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        host.queue_stream(vec![]);
        let resp = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "q".into(),
                remember: Some(false),
            }),
        )
        .await
        .expect("prompt");
        wait_for_terminal_operation(&state, &resp.0.operation_id).await;
        let err = cancel_operation(
            State(state),
            Path(format!("{}:cancel", resp.0.operation_id)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "actor_operation_finished");
        assert_eq!(host.cancels.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// Foreign owner cannot read operation outcome.
    #[tokio::test]
    async fn get_operation_outcome_hides_foreign_owner() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        host.queue_stream(vec![]);
        let resp = execute_operation(
            State(state.clone()),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "q".into(),
                remember: Some(false),
            }),
        )
        .await
        .expect("prompt");
        wait_for_terminal_operation(&state, &resp.0.operation_id).await;
        std::fs::write(
            state.nexus_home().join("config.toml"),
            "active_creator_id = \"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n\n[active_workspace_slug_by_creator]\n\"ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\" = \"default\"\n",
        )
        .unwrap();
        let err = get_operation_result(State(state), Path(resp.0.operation_id.clone()))
            .await
            .unwrap_err();
        assert_eq!(err.error_code(), "not_found");
    }

    /// Nonterminal operation capacity is bounded at 128 before provider exec.
    #[tokio::test]
    async fn character_operation_capacity_refuses_before_exec() {
        let (_tmp, state, host) = state_with_prompt_host().await;
        let (character_id, binding_id) = seed_owned_character(&state).await;
        let created = create_session(
            State(state.clone()),
            Json(character_session_req(
                &character_id,
                "wld_worldA",
                &binding_id,
            )),
        )
        .await
        .expect("create");
        let ctx = state
            .actor_sessions()
            .context_for(&nexus_agent_host::HostSessionId(
                uuid::Uuid::parse_str(&created.session_id).unwrap(),
            ))
            .expect("ctx");
        for _ in 0..128 {
            let snapshot = CharacterOperationSnapshot {
                owner_creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                ctx: ctx.clone(),
                session_id: nexus_agent_host::HostSessionId(
                    uuid::Uuid::parse_str(&created.session_id).unwrap(),
                ),
                operation_id: nexus_agent_host::HostOperationId::new(),
                remember: false,
                raw_prompt: "x".into(),
            };
            state
                .actor_sessions()
                .reserve_character_operation(snapshot)
                .expect("reserve");
        }
        let err = execute_operation(
            State(state),
            Path(created.session_id.clone()),
            Json(ExecuteOperationRequest::Prompt {
                content: "blocked".into(),
                remember: Some(false),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.error_code(), "actor_operation_capacity");
        assert_eq!(host.execs.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
