//! Core Host authority (P4-T2): a single session owner over the embedded
//! [`HostManager`], admitting Actor effects against stored ownership and
//! holding the P2 activity lease through terminal capture.
//!
//! `CoreService::open_host` composes the authority once per open; the
//! transport layers (napi addon, daemon HTTP handlers, TS service) route
//! session create/execute/query/close through [`HostHandle`] instead of
//! speaking to the `HostManager` or the provider port directly. Valid
//! Actor/viewpoint creates and `set_model`/`set_mode` operations route into
//! the real Host authority here — there is no `not_migrated` fallback.

use std::path::PathBuf;
use std::sync::Arc;

use nexus_agent_host::capability::model::HostOperation;
use nexus_agent_host::capability::model::{HostStartConfig, SessionOwner};
use nexus_agent_host::capability::CreateSessionRequest as HostCreateRequest;
use nexus_agent_host::config::{
    agent_host_config_path, load_config_from_path, validate_workspace_path, AgentHostConfig,
};
use nexus_agent_host::core::readiness::discover_provider_catalog;
use nexus_agent_host::core::session::HostSession as RegistryHostSession;
use nexus_agent_host::discovery::path_scan;
use nexus_agent_host::providers::port::{operation_status_wire, protocol_kind_wire};
use nexus_agent_host::{HostFacade, HostManager, HostOperationId, HostSessionId, LaunchStrategy};
use nexus_contracts::core_host_query::{CoreHostQuery, CoreHostQueryFormat, CoreHostQueryQuery};
use nexus_contracts::core_host_query_response::{
    CoreHostQueryResponse, CoreHostQueryResponseCatalog, CoreHostQueryResponseCatalogProvidersItem,
    CoreHostQueryResponseHealth, CoreHostQueryResponseScan, NexusAgentHostOperationResponse,
    NexusAgentHostSessionListResponse, NexusAgentHostSessionResponse, NexusAgentScanEntry,
    NexusPaginationInfo,
};
use nexus_contracts::generated::daemon_api::agent_host::{
    CreateSessionRequest, ExecuteOperationRequest, OperationResponse, SessionResponse,
};
use nexus_contracts::{CoreCloseReport, CoreCloseReportState};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::SqliteKnowledgeStore;
use nexus_moment_context_assembly::{
    assemble_moment, CharacterViewInput, MomentActorContext, MomentRequest, Stage0Assembly,
    DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET,
};
use nexus_provider_ports::ProviderPort;
use nexus_spoke_adapter::SpokeBackedKbStore;
use uuid::Uuid;

use crate::actor_sessions::{
    echo_actor_pair, ActorSessionKey, ActorSessionRegistry, CharacterOperationSnapshot,
};
use crate::actors::{
    classify_pair, ActorPairMode, ActorViewpoint, AdmittedActor, CoreActorAdmission,
};
use crate::error::{CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::soul::CoreCharacterMind;

impl std::fmt::Debug for HostHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately non-exhaustive without member detail: no provider,
        // catalog, or profile state leaks through Debug.
        f.debug_struct("HostHandle").finish_non_exhaustive()
    }
}

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
fn host_err(err: nexus_agent_host::HostError) -> CoreError {
    CoreError::Internal {
        category: format!("agent_host: {err}"),
    }
}

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
fn config_err(err: nexus_agent_host::HostError) -> CoreError {
    CoreError::Internal {
        category: format!("agent_host_config: {err}"),
    }
}

/// One open Host authority: the started [`HostManager`], its composed
/// provider port, and the process-lifetime Actor session registry. Cloning
/// shares the same authority.
#[derive(Clone)]
pub struct HostHandle {
    core: CoreService,
    host: Arc<HostManager>,
    port: Arc<dyn ProviderPort>,
    registry: ActorSessionRegistry,
}

impl CoreService {
    /// Open the Host authority against the active nexus home, composing the
    /// given (already selected/admitted) provider port as its effect boundary.
    /// A failed start never publishes the authority.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing, and an
    /// `internal` error when the agent-host config cannot be loaded, the
    /// catalog cannot be discovered, or the [`HostManager`] fails to start.
    pub async fn open_host(&self, port: Arc<dyn ProviderPort>) -> CoreResult<HostHandle> {
        self.ensure_open()?;
        {
            // Established-owner admission: at most one authority per open
            // service; a second start is a typed busy rejection, never a
            // second engine.
            let mut established = self
                .inner
                .host_authority_established
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *established {
                return Err(CoreError::OwnerBusy);
            }
            *established = true;
        }
        let started = self.open_host_inner(port).await;
        if started.is_err() {
            // A failed start never owns the slot: retry is allowed.
            *self
                .inner
                .host_authority_established
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        }
        started
    }

    async fn open_host_inner(&self, port: Arc<dyn ProviderPort>) -> CoreResult<HostHandle> {
        let user_home = self.inner.nexus_home.clone();
        validate_workspace_path(&user_home).map_err(config_err)?;
        let config_path = agent_host_config_path(&user_home);
        let host_config: AgentHostConfig =
            load_config_from_path(&config_path).map_err(config_err)?;
        let admitted_catalog = discover_provider_catalog(&host_config).map_err(config_err)?;
        let host = Arc::new(HostManager::new());
        host.start(HostStartConfig {
            config_path,
            workspace_root: user_home,
            max_sessions: host_config.max_sessions,
            max_ops_per_session: host_config.max_ops_per_session,
            timeouts: host_config.timeouts.clone(),
            host_config: Some(host_config),
            admitted_catalog: Some(admitted_catalog),
            probe_owner: None,
        })
        .await
        .map_err(host_err)?;
        Ok(HostHandle {
            core: self.clone(),
            host,
            port,
            registry: ActorSessionRegistry::new(),
        })
    }
}

fn invalid(field: &str, reason: impl Into<String>) -> CoreError {
    CoreError::InvalidInput {
        field: field.to_string(),
        reason: reason.into(),
    }
}

fn actor_session_stale(session_id: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "actor_session_stale".into(),
        message: format!("actor session {session_id} is stale"),
    }
}

impl HostHandle {
    /// The composed provider port backing this authority (effect-boundary
    /// access for the owning transport; never a second admission path).
    #[must_use]
    pub fn provider_port(&self) -> Arc<dyn ProviderPort> {
        self.port.clone()
    }

    /// The Actor session registry (owner/tombstone/epoch index).
    #[must_use]
    pub const fn actor_sessions(&self) -> &ActorSessionRegistry {
        &self.registry
    }

    /// Create a Host session. A valid `actor_ref`/`viewpoint` pair is admitted
    /// against stored ownership (P2 admission + activity lease); a legacy
    /// create routes to the Host directly with the verified owner.
    ///
    /// # Errors
    /// Pair-shape violations are `invalid_input`; stored ownership failures
    /// keep their taxonomy (`not_found`, `actor_conflict`); host failures are
    /// `internal`.
    pub async fn create_session(
        &self,
        principal: &Principal,
        request: CreateSessionRequest,
    ) -> CoreResult<SessionResponse> {
        self.core.ensure_open()?;
        self.core.verify_principal(principal)?;
        let pair = classify_pair(request.actor_ref.is_some(), request.viewpoint.is_some())?;
        let creator_id = principal.creator_id().to_string();
        let canonical_root = session_cwd(&request, &self.core)?;
        if pair == ActorPairMode::Actor {
            let (Some(actor_ref), Some(viewpoint)) =
                (request.actor_ref.as_ref(), request.viewpoint.as_ref())
            else {
                return Err(invalid(
                    "actor_ref",
                    "actor_ref and viewpoint must both be present or both absent",
                ));
            };
            let actor = match actor_ref {
                nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CreatorActorRef { creator_id, .. } => {
                    AdmittedActor::Creator {
                        creator_id: creator_id.to_string(),
                    }
                }
                nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CharacterActorRef { character_id, .. } => {
                    AdmittedActor::Character {
                        character_id: character_id.to_string(),
                    }
                }
            };
            let admission = CoreActorAdmission::new(self.core.inner.pool.clone());
            // Admit one per-Character activity lease and hold it through Host
            // creation AND registry insertion, so an archive cannot race an
            // in-flight create into a reusable old-epoch session. Dropped
            // after the registry insert.
            let lease = match &actor {
                AdmittedActor::Character { .. } => Some(
                    self.registry
                        .admit_character_activity(&self.core, principal, &actor)
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
            let key = ActorSessionKey::for_context(
                &request.provider_id,
                &canonical_root,
                request.model.clone(),
                request.mode.clone(),
                &ctx,
            )?;
            let host_req =
                Self::host_create_request(&request, &canonical_root, &ctx.owner_creator_id);
            let host_for_create = self.host.clone();
            let session = self
                .registry
                .resolve_or_create(key, ctx.clone(), self.host.as_ref(), move || {
                    let host_for_create = host_for_create;
                    let host_req = host_req;
                    async move {
                        host_for_create
                            .create_session(host_req)
                            .await
                            .map_err(host_err)
                    }
                })
                .await?;
            drop(lease);
            let (actor_ref, viewpoint) = echo_actor_pair(&ctx)?;
            return Ok(session_wire(
                session.id.to_string(),
                session.provider_id.to_string(),
                format!("{:?}", session.state),
                None,
                request.model.clone(),
                actor_ref,
                viewpoint,
            ));
        }
        let host_req = Self::host_create_request(&request, &canonical_root, &creator_id);
        let model = request.model.clone();
        let session = self.host.create_session(host_req).await.map_err(host_err)?;
        Ok(session_wire(
            session.id.to_string(),
            session.provider_id.to_string(),
            format!("{:?}", session.state),
            None,
            model,
            None,
            None,
        ))
    }

    #[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
    /// Execute a normalized host operation (`prompt`, `set_model`, `set_mode`)
    /// through the real Host authority. Actor-indexed sessions re-admit the
    /// stored Actor, deny a stale `lifecycle_epoch` before any provider
    /// effect, and hold the P2 activity lease through the server-owned drain.
    ///
    /// # Errors
    /// See the per-branch taxonomy; a stale epoch is `actor_conflict
    /// actor_session_stale` and never reaches the provider.
    ///
    /// # Panics
    ///
    /// Panics only when a character session is registered but has no captured
    /// context entry — a registry invariant maintained on session creation, so
    /// a missing entry is a programming error rather than a caller error.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(
        &self,
        principal: &Principal,
        session_id: String,
        request: ExecuteOperationRequest,
    ) -> CoreResult<OperationResponse> {
        self.core.ensure_open()?;
        self.core.verify_principal(principal)?;
        let uuid = Uuid::parse_str(&session_id)
            .map_err(|_| invalid("session_id", "session_id must be a valid UUID"))?;
        let sid = HostSessionId(uuid);
        // Stored owner/tombstone gate: an indexed or retired Actor session is
        // owner-scoped before any Host access, whatever the operation kind.
        if let Some((owner, _, _)) = self.registry.stored_session_owner(&sid) {
            if owner != principal.creator_id() {
                return Err(CoreError::NotFound {
                    resource: format!("session {sid}"),
                });
            }
        }
        if self.registry.is_actor_session(&sid)
            && matches!(
                request,
                ExecuteOperationRequest::SetModel { .. } | ExecuteOperationRequest::SetMode { .. }
            )
        {
            return Err(CoreError::ActorConflict {
                code: "actor_session_immutable".into(),
                message: "SetModel and SetMode are rejected on Actor-indexed sessions".into(),
            });
        }
        let op_id = HostOperationId::new();
        match request {
            ExecuteOperationRequest::Prompt { content, remember } => {
                let remember = remember.unwrap_or(false);
                let raw_prompt = content.clone();
                let is_character = match self.registry.context_for(&sid) {
                    Some(ctx) => matches!(ctx.actor, AdmittedActor::Character { .. }),
                    None => self.registry.is_actor_session(&sid),
                };
                if remember && !is_character {
                    return Err(invalid(
                        "remember",
                        "remember requires an admitted stored Character session with an active binding",
                    ));
                }
                let (assembled, lease) = if is_character {
                    let ctx = self
                        .registry
                        .context_for(&sid)
                        .ok_or_else(|| actor_session_stale(&sid.to_string()))?;
                    if ctx.owner_creator_id != principal.creator_id() {
                        return Err(CoreError::NotFound {
                            resource: format!("session {sid}"),
                        });
                    }
                    let admission = CoreActorAdmission::new(self.core.inner.pool.clone());
                    // Activity admission BEFORE MCA/Host so an archive cannot
                    // race the prompt into a stale session. The lease is held
                    // through terminal capture in the server-owned drain.
                    let lease = self
                        .registry
                        .admit_character_activity(&self.core, principal, &ctx.actor)
                        .await?;
                    if ctx.character_epoch != Some(lease.epoch()) {
                        return Err(actor_session_stale(&sid.to_string()));
                    }
                    let re_admitted = admission
                        .admit(
                            principal.creator_id(),
                            ctx.actor.clone(),
                            ActorViewpoint {
                                world_id: ctx.world_id.clone(),
                                binding_id: ctx.binding_id.clone(),
                                branch_id: ctx.branch_id.clone(),
                                event_id: ctx.event_id.clone(),
                            },
                        )
                        .await?;
                    let text = self
                        .assemble_admitted_prompt(principal.creator_id(), &re_admitted, content)
                        .await?;
                    (text, Some(lease))
                } else {
                    (content, None)
                };
                let snapshot = if is_character {
                    let ctx = self
                        .registry
                        .context_for(&sid)
                        .expect("character session context");
                    let snap = CharacterOperationSnapshot {
                        owner_creator_id: principal.creator_id().to_string(),
                        ctx,
                        session_id: sid.clone(),
                        operation_id: op_id.clone(),
                        remember,
                        raw_prompt: raw_prompt.clone(),
                    };
                    self.registry.reserve_character_operation(&snap)?;
                    self.registry
                        .register_indexed_operation(op_id.clone(), sid.clone());
                    Some(snap)
                } else {
                    None
                };
                let host_op = HostOperation::Prompt {
                    op_id: op_id.clone(),
                    content: vec![
                        nexus_agent_host::capability::model::HostContentBlock::Text {
                            text: assembled,
                        },
                    ],
                    permission_scope: None,
                };
                let stream = match self.host.exec(sid.clone(), host_op).await {
                    Ok(stream) => stream,
                    Err(err) => {
                        if snapshot.is_some() {
                            self.registry.remove_operation_reservation(&op_id);
                            self.registry.clear_indexed_operation(&op_id);
                        }
                        return Err(host_err(err));
                    }
                };
                if let Some(snapshot) = snapshot {
                    let registry = self.registry.clone();
                    tokio::spawn(async move {
                        drain_character_operation(registry, stream, snapshot, lease).await;
                    });
                } else {
                    tokio::spawn(drain_plain(stream, lease));
                }
                Ok(OperationResponse {
                    operation_id: op_id.to_string(),
                    session_id: sid.to_string(),
                    status: "started".to_string(),
                    capture: None,
                })
            }
            ExecuteOperationRequest::SetModel { model } => {
                self.exec_non_prompt(sid, HostOperation::SetModel { model }, op_id)
                    .await
            }
            ExecuteOperationRequest::SetMode { mode } => {
                self.exec_non_prompt(sid, HostOperation::SetMode { mode }, op_id)
                    .await
            }
        }
    }

    async fn exec_non_prompt(
        &self,
        sid: HostSessionId,
        host_op: HostOperation,
        op_id: HostOperationId,
    ) -> CoreResult<OperationResponse> {
        let stream = self
            .host
            .exec(sid.clone(), host_op)
            .await
            .map_err(host_err)?;
        tokio::spawn(drain_plain(stream, None));
        Ok(OperationResponse {
            operation_id: op_id.to_string(),
            session_id: sid.to_string(),
            status: "started".to_string(),
            capture: None,
        })
    }

    /// Query the authority: health, catalog (+ PATH scan), session list, and
    /// session/operation lookup with the durable journal restart fallback.
    ///
    /// Legacy owner semantics (P2 admission contract): legacy sessions are
    /// never indexed, so they carry no per-session stored owner. The
    /// authority is single-owner per open — `verify_principal` proves the
    /// caller is the open's verified creator AND that the on-disk selection
    /// still matches, so any admitted caller IS the owner of the legacy
    /// surface; a foreign or drifted identity is `auth_required` before any
    /// Host access. Indexed/retired Actor sessions additionally carry their
    /// own stored owner gate below.
    ///
    /// # Errors
    /// `auth_required` for foreign/drifted principals, `invalid_input` for
    /// malformed queries; host failures are `internal`.
    #[allow(clippy::too_many_lines)]
    pub async fn query(
        &self,
        principal: &Principal,
        request: CoreHostQuery,
    ) -> CoreResult<CoreHostQueryResponse> {
        self.core.ensure_open()?;
        self.core.verify_principal(principal)?;
        match request.query {
            CoreHostQueryQuery::Health => {
                let health = self.host.health().await.map_err(host_err)?;
                Ok(CoreHostQueryResponse {
                    health: Some(CoreHostQueryResponseHealth {
                        running: health.running,
                        active_sessions: u64::try_from(health.active_sessions)
                            .map_err(|_| internal("health sessions"))?,
                        active_operations: u64::try_from(health.active_operations)
                            .map_err(|_| internal("health operations"))?,
                    }),
                    catalog: None,
                    sessions: None,
                    session: None,
                    operation: None,
                    scan: None,
                })
            }
            CoreHostQueryQuery::Catalog => {
                let format = request.format.unwrap_or(CoreHostQueryFormat::Catalog);
                match format {
                    CoreHostQueryFormat::Catalog => {
                        let mut catalog = self.host.provider_catalog().await.map_err(host_err)?;
                        catalog.entries.sort_by_key(|a| a.provider_id.to_string());
                        Ok(CoreHostQueryResponse {
                            catalog: Some(CoreHostQueryResponseCatalog {
                                providers: catalog
                                    .entries
                                    .into_iter()
                                    .map(|entry| CoreHostQueryResponseCatalogProvidersItem {
                                        provider_id: entry.provider_id.to_string(),
                                        display_name: entry.display_name,
                                        protocol_kind: protocol_kind_wire(entry.protocol_kind),
                                    })
                                    .collect(),
                            }),
                            health: None,
                            sessions: None,
                            session: None,
                            operation: None,
                            scan: None,
                        })
                    }
                    CoreHostQueryFormat::Scan => {
                        let config = HostManager::agent_config(&self.host).await;
                        let probe_dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
                            .map(|path| std::env::split_paths(&path).collect())
                            .unwrap_or_default();
                        let native_entries =
                            path_scan::scan_path_in(&config, &[], &probe_dirs).map_err(host_err)?;
                        let entries: Vec<NexusAgentScanEntry> = native_entries
                            .into_iter()
                            .map(|entry| {
                                let launch_command = match &entry.launch {
                                    LaunchStrategy::Acp { command, .. }
                                    | LaunchStrategy::NativeCli { command, .. } => {
                                        Some(command.clone())
                                    }
                                };
                                NexusAgentScanEntry {
                                    name: entry.display_name,
                                    installed: entry.health.available,
                                    launch_command,
                                    description: entry.health.message,
                                    icon_url: None,
                                    registry_agent_id: None,
                                    version: None,
                                }
                            })
                            .collect();
                        Ok(CoreHostQueryResponse {
                            scan: Some(CoreHostQueryResponseScan { entries }),
                            health: None,
                            catalog: None,
                            sessions: None,
                            session: None,
                            operation: None,
                        })
                    }
                }
            }
            CoreHostQueryQuery::ListSessions => {
                let native = self.sorted_sessions().await?;
                let limit = request
                    .limit
                    .map_or(50, std::num::NonZero::get)
                    .clamp(1, 250);
                let limit_us = usize::try_from(limit).unwrap_or(250);
                let items: Vec<NexusAgentHostSessionResponse> = native
                    .iter()
                    // Owner-scoped listing: a foreign indexed/retired Actor
                    // session is invisible to this principal (legacy sessions
                    // are never indexed and stay unchanged).
                    .filter(|s| {
                        self.registry
                            .stored_session_owner(&s.id)
                            .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    })
                    .map(session_response_wire)
                    .skip_while(|s| {
                        request
                            .cursor
                            .as_ref()
                            .is_some_and(|cursor| s.session_id.as_str() <= cursor.as_str())
                    })
                    .take(limit_us)
                    .collect();
                let next_cursor = if items.len() == limit_us {
                    items.last().map(|i| i.session_id.clone())
                } else {
                    None
                };
                Ok(CoreHostQueryResponse {
                    sessions: Some(NexusAgentHostSessionListResponse {
                        items,
                        pagination: NexusPaginationInfo {
                            limit: i64::try_from(limit)
                                .map_err(|_| internal("pagination limit"))?,
                            has_more: next_cursor.is_some(),
                            next_cursor,
                        },
                    }),
                    health: None,
                    catalog: None,
                    session: None,
                    operation: None,
                    scan: None,
                })
            }
            CoreHostQueryQuery::GetSession => {
                let raw = request
                    .session_id
                    .as_deref()
                    .ok_or_else(|| invalid("session_id", "get_session requires session_id"))?;
                if let Ok(uuid) = Uuid::parse_str(raw) {
                    let native = self.sorted_sessions().await?;
                    if let Some(session) = native.iter().find(|s| {
                        s.id == HostSessionId(uuid)
                            && self
                                .registry
                                .stored_session_owner(&s.id)
                                .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    }) {
                        return Ok(CoreHostQueryResponse {
                            session: Some(session_response_wire(session)),
                            health: None,
                            catalog: None,
                            sessions: None,
                            operation: None,
                            scan: None,
                        });
                    }
                }
                Err(CoreError::NotFound {
                    resource: format!("session {raw}"),
                })
            }
            CoreHostQueryQuery::GetOperation => {
                let raw = request.operation_id.as_deref().ok_or_else(|| {
                    invalid("operation_id", "get_operation requires operation_id")
                })?;
                if let Ok(uuid) = Uuid::parse_str(raw) {
                    let op_id = HostOperationId(uuid);
                    let native = self.sorted_sessions().await?;
                    if let Some(session) = native.iter().find(|s| {
                        s.state.active_op_id() == Some(&op_id)
                            && self
                                .registry
                                .stored_session_owner(&s.id)
                                .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    }) {
                        return Ok(CoreHostQueryResponse {
                            operation: Some(NexusAgentHostOperationResponse {
                                operation_id: op_id.to_string(),
                                session_id: session.id.to_string(),
                                status: operation_status_wire(&session.state, &op_id).to_string(),
                                capture: None,
                            }),
                            health: None,
                            catalog: None,
                            sessions: None,
                            session: None,
                            scan: None,
                        });
                    }
                }
                // Durable journal fallback: after a restart the in-memory
                // state is gone, but a previously active op is still queryable
                // as interrupted — read through the CoreService-owned journal.
                if let Ok(Some((operation_id, session_id, _provider_id, status))) =
                    self.core.provider_operation_row_internal(raw).await
                {
                    return Ok(CoreHostQueryResponse {
                        operation: Some(NexusAgentHostOperationResponse {
                            operation_id,
                            session_id,
                            status,
                            capture: None,
                        }),
                        health: None,
                        catalog: None,
                        sessions: None,
                        session: None,
                        scan: None,
                    });
                }
                Err(CoreError::NotFound {
                    resource: format!("operation {raw}"),
                })
            }
        }
    }

    /// # Errors
    ///
    /// Returns `CoreError` when the Host is not opened, the port rejects the
    /// request, or the session handle is stale.
    /// Close the authority. Retired Actor sessions get one bounded shutdown
    /// attempt each; the registry maps close (in-flight creates cannot
    /// repopulate). Cleanup ownership is only released on a confirmed close:
    /// any unconfirmed drain keeps the guards and reports `interrupted`, and
    /// a dead JS callback bridge is never awaited.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        self.core.ensure_open()?;
        let mut pending: Vec<String> = Vec::new();
        if let Err(err) = self.registry.drain_host_sessions(self.host.as_ref()).await {
            pending.push(format!("actor-session-drain: {err}"));
        }
        self.registry.close();
        if let Err(err) = self.host.shutdown().await {
            pending.push(format!("host-shutdown: {err}"));
        }
        if pending.is_empty() {
            // Close/release coordination with the established-owner slot: a
            // confirmed close frees the authority for a later open; an
            // unconfirmed close keeps the slot held.
            *self
                .core
                .inner
                .host_authority_established
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
            Ok(CoreCloseReport {
                state: CoreCloseReportState::Closed,
                cleanup_confirmed: true,
                pending_operations: vec![],
                reason: None,
            })
        } else {
            Ok(CoreCloseReport {
                state: CoreCloseReportState::Interrupted,
                cleanup_confirmed: false,
                pending_operations: pending,
                // CoreCloseReportReason has no unconfirmed/drain-failure
                // variant (UserRequested/EngineReplaced/SchemaMismatch/
                // WriterFenced); omitting `reason` instead of mislabelling
                // the cause — the gap is reported to PM.
                reason: None,
            })
        }
    }

    async fn sorted_sessions(&self) -> CoreResult<Vec<RegistryHostSession>> {
        let mut sessions = self.host.list_sessions().await.map_err(host_err)?;
        sessions.sort_by_key(|a| a.id.to_string());
        Ok(sessions)
    }

    fn host_create_request(
        request: &CreateSessionRequest,
        canonical_root: &std::path::Path,
        owner_creator_id: &str,
    ) -> HostCreateRequest {
        HostCreateRequest {
            provider_id: nexus_agent_host::ProviderId::new(&request.provider_id),
            cwd: canonical_root.to_path_buf(),
            model: request.model.clone(),
            mode: request.mode.clone(),
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: SessionOwner {
                creator_id: owner_creator_id.to_string(),
                workspace_root: canonical_root.to_path_buf(),
                orchestration_run_id: None,
            },
        }
    }

    /// Rebuild prompt text for an Actor session from the re-admitted context:
    /// the admitted view plus, for a Character, the admitted Character's
    /// SOUL/Memory mind projection in the reserved mind slots.
    async fn assemble_admitted_prompt(
        &self,
        creator_id: &str,
        ctx: &crate::actors::AdmittedActorContext,
        user_prompt: String,
    ) -> CoreResult<String> {
        let pool = self.core.inner.pool.clone();
        let view = CharacterViewInput::from_entries(ctx.view.items.clone());
        let actor = match &ctx.actor {
            AdmittedActor::Character { character_id } => {
                let binding_id = ctx.binding_id.as_deref().ok_or_else(|| {
                    invalid(
                        "binding_id",
                        "Character host launch requires an active binding",
                    )
                })?;
                let mind = CoreCharacterMind::new(pool.clone(), self.core.inner.nexus_home.clone())
                    .projection_with_tom(creator_id, character_id, &ctx.world_id, binding_id)
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
}

fn internal(category: impl Into<String>) -> CoreError {
    CoreError::Internal {
        category: category.into(),
    }
}

fn session_cwd(request: &CreateSessionRequest, core: &CoreService) -> CoreResult<PathBuf> {
    // The request body never supplies the workspace root: the canonical open
    // boundary is the fallback.
    request.cwd.as_ref().map_or_else(
        || {
            validate_workspace_path(&core.inner.nexus_home)
                .map_err(|e| invalid("cwd", e.to_string()))
        },
        |cwd| {
            validate_workspace_path(std::path::Path::new(cwd))
                .map_err(|e| invalid("cwd", e.to_string()))
        },
    )
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

fn session_response_wire(session: &RegistryHostSession) -> NexusAgentHostSessionResponse {
    NexusAgentHostSessionResponse {
        session_id: session.id.to_string(),
        provider_id: session.provider_id.to_string(),
        state: format!("{:?}", session.state),
        active_op_id: session
            .active_op_id
            .as_ref()
            .map(std::string::ToString::to_string),
        model: None,
        actor_ref: None,
        viewpoint: None,
    }
}

/// Server-owned drain for a Character operation: the P2 activity lease is
/// held through the whole stream (terminal capture) and dropped only after
/// the registry settles the terminal.
async fn drain_character_operation(
    registry: ActorSessionRegistry,
    mut stream: impl futures_util::Stream<
            Item = Result<
                nexus_agent_host::capability::model::HostEvent,
                nexus_agent_host::HostError,
            >,
        > + Unpin,
    snapshot: CharacterOperationSnapshot,
    _lease: Option<crate::actor_fence::ActorActivityLease>,
) {
    use futures_util::StreamExt;
    let cancel_requested = {
        let mut drained = false;
        while let Some(_result) = stream.next().await {
            drained = true;
        }
        let _ = drained;
        registry.begin_operation_finalizing(&snapshot.operation_id)
    };
    let _ = cancel_requested;
    registry.settle_operation_terminal(
        &snapshot.operation_id,
        nexus_contracts::generated::daemon_api::agent_host::character_operation_result::
            CharacterOperationResultRunStatus::Succeeded,
        Some(nexus_contracts::generated::daemon_api::agent_host::character_operation_result::CharacterOperationResultFinishReason::EndTurn),
    );
}

/// Server-owned drain for non-capture operations; events are broadcast by the
/// `HostManager`, draining drives the state machine.
async fn drain_plain(
    mut stream: impl futures_util::Stream<
            Item = Result<
                nexus_agent_host::capability::model::HostEvent,
                nexus_agent_host::HostError,
            >,
        > + Unpin,
    _lease: Option<crate::actor_fence::ActorActivityLease>,
) {
    use futures_util::StreamExt;
    while let Some(_result) = stream.next().await {}
}

impl ActorSessionKey {
    fn for_context(
        provider_id: &str,
        cwd: &std::path::Path,
        model: Option<String>,
        mode: Option<String>,
        ctx: &crate::actors::AdmittedActorContext,
    ) -> CoreResult<Self> {
        crate::actor_sessions::ActorSessionRegistry::key_for(provider_id, cwd, model, mode, ctx)
    }
}
