//! Process-lifetime Actor session registry over `HostFacade`.
//!
//! Indexes only Actor-mode sessions. Legacy creates never enter these maps.
//! Concurrent creates for one exact key serialize on a per-key lock.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nexus_agent_host::{HostFacade, HostSession, HostSessionId, SessionState};
use nexus_contracts::generated::daemon_api::agent_host::session_response::{
    NexusActorRef, NexusSessionViewpoint,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::actor_admission::AdmittedActorContext;
use crate::actor_knowledge_view::AdmittedActor;
use crate::api::errors::NexusApiError;

/// Discriminant participating in exact Actor session equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorSessionKind {
    Creator,
    Character,
}

/// Canonical exact-match key for an Actor-mode host session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActorSessionKey {
    pub provider_id: String,
    pub canonical_cwd: PathBuf,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub actor_kind: ActorSessionKind,
    pub actor_id: String,
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
}

struct IndexedActorSession {
    key: ActorSessionKey,
    ctx: AdmittedActorContext,
}

struct RegistryMaps {
    by_key: HashMap<ActorSessionKey, HostSessionId>,
    by_session: HashMap<HostSessionId, IndexedActorSession>,
    key_locks: HashMap<ActorSessionKey, Arc<AsyncMutex<()>>>,
    closed: bool,
}

/// Process-lifetime maps: `key -> HostSessionId` and `HostSessionId -> context`.
#[derive(Clone)]
pub struct ActorSessionRegistry {
    maps: Arc<Mutex<RegistryMaps>>,
}

impl Default for ActorSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn shutting_down() -> NexusApiError {
    NexusApiError::ServiceUnavailable {
        message: "daemon is shutting down".into(),
    }
}

impl ActorSessionRegistry {
    /// Empty process-lifetime registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            maps: Arc::new(Mutex::new(RegistryMaps {
                by_key: HashMap::new(),
                by_session: HashMap::new(),
                key_locks: HashMap::new(),
                closed: false,
            })),
        }
    }

    fn maps(&self) -> std::sync::MutexGuard<'_, RegistryMaps> {
        self.maps.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("actor_sessions mutex poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Canonicalize cwd with the Agent Host workspace-root helper.
    ///
    /// # Errors
    ///
    /// Returns a policy-mapped API error when the path is relative, traverses,
    /// or cannot be resolved.
    pub fn canonicalize_cwd(cwd: &Path) -> Result<PathBuf, NexusApiError> {
        nexus_agent_host::config::validate_workspace_path(cwd).map_err(map_policy)
    }

    /// Build the exact tuple key from an admitted Actor context.
    ///
    /// # Errors
    ///
    /// Returns a policy-mapped API error when `cwd` cannot be canonicalized.
    pub fn key_for(
        provider_id: &str,
        cwd: &Path,
        model: Option<String>,
        mode: Option<String>,
        ctx: &AdmittedActorContext,
    ) -> Result<ActorSessionKey, NexusApiError> {
        let (actor_kind, actor_id) = match &ctx.actor {
            AdmittedActor::Creator { creator_id } => {
                (ActorSessionKind::Creator, creator_id.clone())
            }
            AdmittedActor::Character { character_id } => {
                (ActorSessionKind::Character, character_id.clone())
            }
        };
        Ok(ActorSessionKey {
            provider_id: provider_id.to_string(),
            canonical_cwd: Self::canonicalize_cwd(cwd)?,
            model,
            mode,
            actor_kind,
            actor_id,
            world_id: ctx.world_id.clone(),
            binding_id: ctx.binding_id.clone(),
            branch_id: ctx.branch_id.clone(),
            event_id: ctx.event_id.clone(),
        })
    }

    /// Admitted context for an indexed Actor session, if any.
    #[must_use]
    pub fn context_for(&self, session_id: &HostSessionId) -> Option<AdmittedActorContext> {
        self.maps()
            .by_session
            .get(session_id)
            .map(|row| row.ctx.clone())
    }

    /// Close the process-lifetime maps (daemon shutdown). In-flight creates cannot repopulate.
    pub fn close(&self) {
        let mut maps = self.maps();
        maps.closed = true;
        maps.by_key.clear();
        maps.by_session.clear();
        maps.key_locks
            .retain(|_, lock| Arc::strong_count(lock) > 1);
    }

    /// Drop the process-lifetime maps (daemon shutdown).
    pub fn clear(&self) {
        self.close();
    }

    /// Count indexed Actor sessions (tests / diagnostics).
    #[must_use]
    pub fn len(&self) -> usize {
        self.maps().by_key.len()
    }

    /// True when no Actor sessions are indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.maps().by_key.is_empty()
    }

    /// Count retained per-key locks (tests).
    #[must_use]
    pub fn lock_entry_count(&self) -> usize {
        self.maps().key_locks.len()
    }

    fn lock_for_key(&self, key: &ActorSessionKey) -> Arc<AsyncMutex<()>> {
        let mut maps = self.maps();
        if maps.closed {
            return maps
                .key_locks
                .get(key)
                .cloned()
                .unwrap_or_else(|| Arc::new(AsyncMutex::new(())));
        }
        Arc::clone(
            maps.key_locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn evict_locked(maps: &mut RegistryMaps, key: &ActorSessionKey, session_id: &HostSessionId) {
        maps.by_key.remove(key);
        maps.by_session.remove(session_id);
    }

    fn reclaim(&self, key: &ActorSessionKey, held: &Arc<AsyncMutex<()>>) {
        let mut maps = self.maps();
        if maps.by_key.contains_key(key) {
            return;
        }
        if let Some(stored) = maps.key_locks.get(key) {
            if Arc::ptr_eq(stored, held) && Arc::strong_count(stored) == 2 {
                maps.key_locks.remove(key);
            }
        }
    }

    fn reject_if_closed(maps: &RegistryMaps) -> Result<(), NexusApiError> {
        if maps.closed {
            Err(shutting_down())
        } else {
            Ok(())
        }
    }

    /// Shut down a host session under the Actor key lock when indexed.
    ///
    /// # Errors
    ///
    /// Host shutdown errors are mapped to API errors.
    pub async fn shutdown_session(
        &self,
        session_id: HostSessionId,
        host: &dyn HostFacade,
    ) -> Result<(), NexusApiError> {
        let key = self
            .maps()
            .by_session
            .get(&session_id)
            .map(|row| row.key.clone());
        if let Some(key) = key {
            let lock = self.lock_for_key(&key);
            let _guard = lock.lock().await;
            let still_indexed = self
                .maps()
                .by_session
                .get(&session_id)
                .is_some_and(|row| row.key == key);
            host.shutdown_session(session_id.clone())
                .await
                .map_err(map_host)?;
            if still_indexed {
                let mut maps = self.maps();
                Self::evict_locked(&mut maps, &key, &session_id);
            }
            drop(_guard);
            self.reclaim(&key, &lock);
            Ok(())
        } else {
            host.shutdown_session(session_id).await.map_err(map_host)
        }
    }

    /// Reuse a Ready exact match, reject Busy, or mint a replacement after stale eviction.
    ///
    /// # Errors
    ///
    /// `actor_session_busy` when the HostFacade session is Busy; host list/create errors otherwise.
    pub async fn resolve_or_create<F, Fut>(
        &self,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
        host: &dyn HostFacade,
        create: F,
    ) -> Result<HostSession, NexusApiError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<HostSession, NexusApiError>> + Send,
    {
        {
            let maps = self.maps();
            Self::reject_if_closed(&maps)?;
        }
        let lock = self.lock_for_key(&key);
        let _guard = lock.lock().await;
        {
            let maps = self.maps();
            if let Err(err) = Self::reject_if_closed(&maps) {
                drop(maps);
                drop(_guard);
                self.reclaim(&key, &lock);
                return Err(err);
            }
        }

        let existing = {
            let maps = self.maps();
            maps.by_key.get(&key).cloned()
        };
        if let Some(existing) = existing {
            let listed = host.list_sessions().await.map_err(map_host)?;
            {
                let maps = self.maps();
                if let Err(err) = Self::reject_if_closed(&maps) {
                    drop(maps);
                    drop(_guard);
                    self.reclaim(&key, &lock);
                    return Err(err);
                }
            }
            match listed.into_iter().find(|session| session.id == existing) {
                Some(session) if matches!(session.state, SessionState::Ready) => {
                    drop(_guard);
                    self.reclaim(&key, &lock);
                    return Ok(session);
                }
                Some(session) if session.state.is_busy() => {
                    return Err(NexusApiError::ConflictCoded {
                        code: "actor_session_busy".into(),
                        message: "actor session is busy".into(),
                    });
                }
                Some(session) => {
                    let mut maps = self.maps();
                    Self::evict_locked(&mut maps, &key, &session.id);
                }
                None => {
                    let mut maps = self.maps();
                    Self::evict_locked(&mut maps, &key, &existing);
                }
            }
        }

        let session = create().await?;
        let inserted = {
            let mut maps = self.maps();
            if maps.closed {
                false
            } else {
                maps.by_key.insert(key.clone(), session.id.clone());
                maps.by_session.insert(
                    session.id.clone(),
                    IndexedActorSession {
                        key: key.clone(),
                        ctx,
                    },
                );
                true
            }
        };
        if !inserted {
            let _ = host.shutdown_session(session.id.clone()).await;
            drop(_guard);
            self.reclaim(&key, &lock);
            return Err(shutting_down());
        }
        drop(_guard);
        self.reclaim(&key, &lock);
        Ok(session)
    }
}

/// Map admitted context onto generated session response optionals.
///
/// # Errors
///
/// Returns `internal` if stored ids fail generated pattern checks (should not happen).
pub fn echo_actor_pair(
    ctx: &AdmittedActorContext,
) -> Result<(Option<NexusActorRef>, Option<NexusSessionViewpoint>), NexusApiError> {
    let actor_ref = match &ctx.actor {
        AdmittedActor::Creator { creator_id } => NexusActorRef::CreatorActorRef {
            actor_kind: "creator"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            creator_id: creator_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
        AdmittedActor::Character { character_id } => NexusActorRef::CharacterActorRef {
            actor_kind: "character"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            character_id: character_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
    };
    let viewpoint = NexusSessionViewpoint {
        world_id: ctx.world_id.parse().map_err(
            |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                NexusApiError::Internal {
                    code: "VIEWPOINT_ECHO".into(),
                    message: e.to_string(),
                }
            },
        )?,
        binding_id: match &ctx.binding_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        branch_id: match &ctx.branch_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        event_id: match &ctx.event_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
    };
    Ok((Some(actor_ref), Some(viewpoint)))
}

fn map_policy(err: nexus_agent_host::HostError) -> NexusApiError {
    NexusApiError::Forbidden {
        resource: "agent_host".into(),
        reason: err.to_string(),
    }
}

fn map_host(err: nexus_agent_host::HostError) -> NexusApiError {
    match err.category() {
        "provider_unavailable" => NexusApiError::NotFound(err.to_string()),
        "capability_unsupported" => NexusApiError::InvalidInput {
            field: "operation".into(),
            reason: err.to_string(),
        },
        "policy_denied" => NexusApiError::Forbidden {
            resource: "agent_host".into(),
            reason: err.to_string(),
        },
        _ => NexusApiError::Internal {
            code: "AGENT_HOST_ERROR".into(),
            message: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_knowledge_view::ActorKnowledgePage;
    use async_trait::async_trait;
    use nexus_agent_host::capability::model::{
        CapabilityDescriptor, CreateSessionRequest, HostEvent, HostEventStream, HostHealth,
        HostOperation, HostStartConfig,
    };
    use nexus_agent_host::{HostOperationId, ProviderCatalog};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn empty_view() -> ActorKnowledgePage {
        ActorKnowledgePage {
            items: Vec::new(),
            limit: 50,
            has_more: false,
            next_cursor: None,
        }
    }

    fn sample_ctx(actor: AdmittedActor, world: &str, binding: Option<&str>) -> AdmittedActorContext {
        AdmittedActorContext {
            actor,
            world_id: world.to_string(),
            binding_id: binding.map(str::to_string),
            branch_id: None,
            event_id: None,
            view: empty_view(),
        }
    }

    fn base_character_ctx() -> AdmittedActorContext {
        sample_ctx(
            AdmittedActor::Character {
                character_id: "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            },
            "wld_worldA",
            Some("awb_cccccccccccccccccccccccccccccccc"),
        )
    }

    fn key_with(
        ctx: &AdmittedActorContext,
        provider: &str,
        cwd: &Path,
        model: Option<&str>,
        mode: Option<&str>,
    ) -> ActorSessionKey {
        ActorSessionRegistry::key_for(
            provider,
            cwd,
            model.map(str::to_string),
            mode.map(str::to_string),
            ctx,
        )
        .expect("canonical cwd")
    }

    fn host_req() -> CreateSessionRequest {
        CreateSessionRequest {
            provider_id: nexus_agent_host::ProviderId::new("prov"),
            cwd: PathBuf::from("/tmp"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
        }
    }

    struct ScriptedHost {
        sessions: Mutex<HashMap<HostSessionId, HostSession>>,
        creates: AtomicU64,
        create_delay: Mutex<Duration>,
        list_delay: Mutex<Duration>,
        shutdown_delay: Mutex<Duration>,
        events: broadcast::Sender<HostEvent>,
    }

    impl ScriptedHost {
        fn new() -> Arc<Self> {
            let (events, _) = broadcast::channel(16);
            Arc::new(Self {
                sessions: Mutex::new(HashMap::new()),
                creates: AtomicU64::new(0),
                create_delay: Mutex::new(Duration::from_millis(0)),
                list_delay: Mutex::new(Duration::from_millis(0)),
                shutdown_delay: Mutex::new(Duration::from_millis(0)),
                events,
            })
        }

        fn set_delay(&self, delay: Duration) {
            *self.create_delay.lock().expect("delay") = delay;
        }

        fn set_list_delay(&self, delay: Duration) {
            *self.list_delay.lock().expect("list delay") = delay;
        }

        fn set_shutdown_delay(&self, delay: Duration) {
            *self.shutdown_delay.lock().expect("shutdown delay") = delay;
        }

        fn set_state(&self, id: HostSessionId, state: SessionState) {
            let mut sessions = self.sessions.lock().expect("sessions");
            if let Some(session) = sessions.get_mut(&id) {
                session.state = state.clone();
                session.active_op_id = state.active_op_id().cloned();
            }
        }
    }

    #[async_trait]
    impl HostFacade for ScriptedHost {
        async fn start(&self, _config: HostStartConfig) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn create_session(
            &self,
            request: CreateSessionRequest,
        ) -> nexus_agent_host::HostResult<HostSession> {
            let delay = *self.create_delay.lock().expect("delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.creates.fetch_add(1, Ordering::SeqCst);
            let session = HostSession {
                id: HostSessionId::new(),
                provider_id: request.provider_id,
                state: SessionState::Ready,
                created_at: chrono::Utc::now(),
                active_op_id: None,
                negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            };
            self.sessions
                .lock()
                .expect("sessions")
                .insert(session.id.clone(), session.clone());
            Ok(session)
        }

        async fn exec(
            &self,
            _session_id: HostSessionId,
            _op: HostOperation,
        ) -> nexus_agent_host::HostResult<HostEventStream> {
            Err(nexus_agent_host::HostError::internal("unused"))
        }

        async fn cancel(&self, _op_id: HostOperationId) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn health(&self) -> nexus_agent_host::HostResult<HostHealth> {
            Ok(HostHealth {
                running: true,
                active_sessions: self.sessions.lock().expect("sessions").len(),
                active_operations: 0,
            })
        }

        async fn shutdown(&self) -> nexus_agent_host::HostResult<()> {
            self.sessions.lock().expect("sessions").clear();
            Ok(())
        }

        async fn shutdown_session(
            &self,
            session_id: HostSessionId,
        ) -> nexus_agent_host::HostResult<()> {
            let delay = *self.shutdown_delay.lock().expect("shutdown delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.sessions
                .lock()
                .expect("sessions")
                .remove(&session_id)
                .ok_or_else(|| nexus_agent_host::HostError::internal("session"))?;
            Ok(())
        }

        async fn list_sessions(&self) -> nexus_agent_host::HostResult<Vec<HostSession>> {
            let delay = *self.list_delay.lock().expect("list delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok(self
                .sessions
                .lock()
                .expect("sessions")
                .values()
                .cloned()
                .collect())
        }

        async fn provider_catalog(&self) -> nexus_agent_host::HostResult<ProviderCatalog> {
            Ok(ProviderCatalog::new())
        }

        fn subscribe_events(
            &self,
            _session_id: HostSessionId,
        ) -> broadcast::Receiver<HostEvent> {
            self.events.subscribe()
        }
    }

    #[test]
    fn every_key_dimension_participates_in_equality() {
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let base = key_with(&ctx, "prov-a", cwd.path(), Some("m1"), Some("code"));

        let other_cwd = tempfile::tempdir().expect("cwd2");
        assert_ne!(
            base,
            key_with(&ctx, "prov-b", cwd.path(), Some("m1"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", other_cwd.path(), Some("m1"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", cwd.path(), Some("m2"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", cwd.path(), Some("m1"), Some("ask"))
        );

        let mut actor = ctx.clone();
        actor.actor = AdmittedActor::Character {
            character_id: "chr_dddddddddddddddddddddddddddddddd".into(),
        };
        assert_ne!(
            base,
            key_with(&actor, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut world = ctx.clone();
        world.world_id = "wld_worldB".into();
        assert_ne!(
            base,
            key_with(&world, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut binding = ctx.clone();
        binding.binding_id = Some("awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
        assert_ne!(
            base,
            key_with(&binding, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut branch = ctx.clone();
        branch.branch_id = Some("fbk_branch1".into());
        assert_ne!(
            base,
            key_with(&branch, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut event = ctx.clone();
        event.event_id = Some("evt_anchor1".into());
        assert_ne!(
            base,
            key_with(&event, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let creator = sample_ctx(
            AdmittedActor::Creator {
                creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
            "wld_worldA",
            None,
        );
        assert_ne!(
            base,
            key_with(&creator, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );
    }

    #[test]
    fn canonical_cwd_collapses_symlink_aliases() {
        let real = tempfile::tempdir().expect("real");
        let alias_root = tempfile::tempdir().expect("alias root");
        let link = alias_root.path().join("link");
        std::os::unix::fs::symlink(real.path(), &link).expect("symlink");
        let ctx = base_character_ctx();
        let a = key_with(&ctx, "prov", real.path(), None, None);
        let b = key_with(&ctx, "prov", &link, None, None);
        assert_eq!(a, b);
    }

    #[test]
    fn relative_cwd_is_rejected_by_host_helper() {
        let ctx = base_character_ctx();
        let err = ActorSessionRegistry::key_for("prov", Path::new("relative"), None, None, &ctx)
            .expect_err("relative");
        assert_eq!(err.error_code(), "forbidden");
    }

    #[tokio::test]
    async fn same_key_converges_and_concurrent_creates_mint_one_host_session() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let host_a = host.clone();
        let host_b = host.clone();
        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let key_a = key.clone();
        let key_b = key;
        let ctx_a = ctx.clone();
        let ctx_b = ctx;

        let (left, right) = tokio::join!(
            async move {
                let host_ref: &dyn HostFacade = host_a.as_ref();
                reg_a
                    .resolve_or_create(key_a, ctx_a, host_ref, || async {
                        host_ref.create_session(host_req()).await.map_err(map_host)
                    })
                    .await
            },
            async move {
                let host_ref: &dyn HostFacade = host_b.as_ref();
                reg_b
                    .resolve_or_create(key_b, ctx_b, host_ref, || async {
                        host_ref.create_session(host_req()).await.map_err(map_host)
                    })
                    .await
            }
        );

        let left = left.expect("left");
        let right = right.expect("right");
        assert_eq!(left.id, right.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn busy_exact_match_is_conflict_coded() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let created = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("create");
        host.set_state(created.id.clone(), SessionState::Busy(HostOperationId::new()));

        let err = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must not create while busy");
            })
            .await
            .expect_err("busy");
        assert_eq!(err.error_code(), "actor_session_busy");
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_or_terminal_match_is_evicted_and_replaced() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("first");

        host.set_state(first.id.clone(), SessionState::Stopped);
        let replaced = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("replace terminal");
        assert_ne!(replaced.id, first.id);
        assert!(registry.context_for(&first.id).is_none());

        host.shutdown_session(replaced.id.clone()).await.expect("drop host row");
        let after_missing = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("replace missing");
        assert_ne!(after_missing.id, replaced.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn session_and_daemon_shutdown_drop_indexes() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let session = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("create");
        assert_eq!(registry.len(), 1);
        registry
            .shutdown_session(session.id.clone(), host.as_ref())
            .await
            .expect("session shutdown");
        assert!(registry.is_empty());
        assert!(registry.context_for(&session.id).is_none());
        assert_eq!(registry.lock_entry_count(), 0);

        let ctx = base_character_ctx();
        let cwd = tempfile::tempdir().expect("cwd");
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let _ = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("second");
        registry.clear();
        assert!(registry.is_empty());
    }

    async fn mint(
        registry: &ActorSessionRegistry,
        host: &Arc<ScriptedHost>,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
    ) -> HostSession {
        registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req()).await.map_err(map_host)
            })
            .await
            .expect("mint")
    }

    #[tokio::test]
    async fn shutdown_holds_key_lock_across_host_teardown() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_shutdown_delay(Duration::from_millis(40));

        let reg_shut = registry.clone();
        let host_shut = host.clone();
        let first_id = first.id.clone();
        let shutdown = tokio::spawn(async move {
            reg_shut
                .shutdown_session(first_id, host_shut.as_ref())
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let replaced = mint(&registry, &host, key, ctx).await;
        shutdown.await.expect("join").expect("shutdown");
        assert_ne!(replaced.id, first.id);
        assert!(registry.context_for(&first.id).is_none());
        assert!(registry.context_for(&replaced.id).is_some());
        assert_eq!(host.creates.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn reuse_completes_before_overlapping_shutdown() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_list_delay(Duration::from_millis(40));

        let reg_a = registry.clone();
        let host_a = host.clone();
        let key_a = key;
        let ctx_a = ctx;
        let reuse = tokio::spawn(async move { mint(&reg_a, &host_a, key_a, ctx_a).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry
            .shutdown_session(first.id.clone(), host.as_ref())
            .await
            .expect("shutdown after reuse started");
        let reused = reuse.await.expect("join");
        assert_eq!(reused.id, first.id);
        assert!(registry.is_empty());
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
        assert_eq!(registry.lock_entry_count(), 0);
    }

    #[tokio::test]
    async fn close_aborts_in_flight_create_without_repopulation() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let reg_a = registry.clone();
        let host_a = host.clone();
        let create = tokio::spawn(async move {
            reg_a
                .resolve_or_create(key, ctx, host_a.as_ref(), || async {
                    host_a.create_session(host_req()).await.map_err(map_host)
                })
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = create.await.expect("join").expect_err("closed");
        assert_eq!(err.error_code(), "service_unavailable");
        assert!(registry.is_empty());
        assert!(host.sessions.lock().expect("sessions").is_empty());
        let cwd = tempfile::tempdir().expect("cwd2");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let err = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must not create after close");
            })
            .await
            .expect_err("still closed");
        assert_eq!(err.error_code(), "service_unavailable");
    }

    #[tokio::test]
    async fn lock_is_reclaimed_without_splitting_live_key() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        assert_eq!(registry.lock_entry_count(), 1);
        registry
            .shutdown_session(first.id.clone(), host.as_ref())
            .await
            .expect("shutdown");
        assert_eq!(registry.lock_entry_count(), 0);

        host.set_delay(Duration::from_millis(30));
        let host_a = host.clone();
        let host_b = host.clone();
        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let key_a = key.clone();
        let key_b = key;
        let ctx_a = ctx.clone();
        let ctx_b = ctx;
        let (left, right) = tokio::join!(
            async move { mint(&reg_a, &host_a, key_a, ctx_a).await },
            async move { mint(&reg_b, &host_b, key_b, ctx_b).await }
        );
        assert_eq!(left.id, right.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 2);
        assert_eq!(registry.lock_entry_count(), 1);
    }

    #[test]
    fn legacy_sessions_are_never_indexed() {
        let registry = ActorSessionRegistry::new();
        assert!(registry.context_for(&HostSessionId::new()).is_none());
        assert!(registry.is_empty());
    }
}
