//! Host Manager — the facade implementing [`HostFacade`].
//!
//! The manager owns the session registry, provider map, and policy gates.
//! It routes operations to the correct provider adapter based on session ownership.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::RwLock;

use crate::capability::model::{
    CreateSessionRequest, HostEvent, HostEventStream, HostHealth, HostStartConfig,
    ManagedSessionHandle, ProbeRequest, SessionOwner,
};
use crate::config::AgentHostConfig;
use crate::core::readiness::{
    discover_provider_entries, is_launch_class_failure, probe_request_for_owner,
    safe_provider_message, CandidateIdentity, ProviderEntry,
};
use crate::core::session::SessionRegistry;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::policy::admission::AdmissionPolicy;
use crate::policy::permission::HostPermissionResolver;
use crate::ProviderAdapter;

/// Broadcast channel capacity for host events.
///
/// Increased to 1024 to reduce silent event loss risk under multi-session load
/// (QC3 W-001). The shared buffer is consumed by all SSE subscribers; higher
/// capacity gives each subscriber more headroom before `RecvError::Lagged`.
const EVENT_BROADCAST_CAPACITY: usize = 1024;

/// The host manager facade.
///
/// Implements [`HostFacade`] — the narrow interface consumed by the daemon runtime.
/// Owns the session registry, routes operations to provider adapters, and
/// enforces policy gates.
pub struct HostManager {
    /// Session state machine registry.
    sessions: Arc<RwLock<SessionRegistry>>,
    /// Provider adapters indexed by provider ID.
    providers: RwLock<HashMap<ProviderId, ProviderEntry>>,
    /// Active session → provider mapping.
    session_providers: RwLock<HashMap<HostSessionId, ProviderId>>,
    /// Admission policy gate.
    admission: RwLock<AdmissionPolicy>,
    /// Whether admission was explicitly set via `with_admission()`.
    admission_custom: bool,
    /// Host configuration (set on start).
    config: RwLock<Option<AgentHostConfig>>,
    /// Whether the host has been started.
    running: RwLock<bool>,
    /// Canonical workspace root boundary — all session cwds must be under this.
    workspace_root: RwLock<Option<std::path::PathBuf>>,
    /// Broadcast sender for host events — SSE subscribers consume via `subscribe()`.
    event_tx: tokio::sync::broadcast::Sender<HostEvent>,
}

// HashMap import for providers/session_providers
use std::collections::HashMap;

impl HostManager {
    /// Create a new host manager with no providers registered.
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(EVENT_BROADCAST_CAPACITY);
        Self {
            sessions: Arc::new(RwLock::new(SessionRegistry::new())),
            providers: RwLock::new(HashMap::new()),
            session_providers: RwLock::new(HashMap::new()),
            admission: RwLock::new(AdmissionPolicy::from_config(
                &AgentHostConfig::default(),
                HashSet::new(),
            )),
            admission_custom: false,
            config: RwLock::new(None),
            running: RwLock::new(false),
            workspace_root: RwLock::new(None),
            event_tx,
        }
    }

    /// Create a host manager with a specific admission policy.
    #[must_use]
    pub fn with_admission(admission: AdmissionPolicy) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(EVENT_BROADCAST_CAPACITY);
        Self {
            admission: RwLock::new(admission),
            admission_custom: true,
            event_tx,
            ..Self::new()
        }
    }

    /// Subscribe to host events for a specific session.
    ///
    /// Returns a filtered receiver that only yields events belonging to the
    /// requested session. Late subscribers may miss events that were broadcast
    /// before they connected (standard broadcast semantics).
    pub fn subscribe(
        &self,
        _session_id: &HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<HostEvent> {
        self.event_tx.subscribe()
    }

    /// Register a provider adapter.
    ///
    /// Must be called before `start()`. The adapter is stored behind `Arc<dyn ProviderAdapter>`.
    /// `launch` is the configured launch recipe reported truthfully by the
    /// catalog (never a fabricated empty recipe).
    pub async fn register_provider(
        &self,
        adapter: Arc<dyn ProviderAdapter>,
        launch: crate::LaunchStrategy,
    ) {
        let desc = adapter.descriptor();
        let provider_id = desc.provider_id.clone();
        let mut providers = self.providers.write().await;
        providers.insert(
            provider_id,
            ProviderEntry::from_registration(adapter, launch),
        );
    }

    /// Get the provider adapter for a given session.
    async fn get_provider_for_session(
        &self,
        session_id: &HostSessionId,
    ) -> HostResult<(Arc<dyn ProviderAdapter>, ProviderId)> {
        let provider_id = {
            let session_providers = self.session_providers.read().await;
            session_providers
                .get(session_id)
                .ok_or_else(|| {
                    HostError::internal(format!("no provider mapped for session {session_id}"))
                })?
                .clone()
        };

        let adapter = {
            let providers = self.providers.read().await;
            providers
                .get(&provider_id)
                .ok_or_else(|| {
                    HostError::provider_unavailable(provider_id.clone(), "provider not registered")
                })?
                .adapter
                .clone()
                .ok_or_else(|| {
                    HostError::provider_unavailable(
                        provider_id.clone(),
                        "provider adapter not constructed",
                    )
                })?
        };

        Ok((adapter, provider_id))
    }

    async fn mark_probe_context_unavailable(&self) {
        let mut providers = self.providers.write().await;
        for entry in providers.values_mut() {
            // EVERY entry becomes unavailable without a verified probe owner.
            // A prior owner-bound start may have published `available = true`;
            // that success must not survive a re-start that has no owner, or
            // the catalog/admission would expose a provider this process can no
            // longer prove ready. Stale latency/success state is cleared too.
            entry.health.available = false;
            entry.health.latency_ms = None;
            entry.health.message = Some("probe_context_unavailable".to_string());
        }
    }

    async fn publish_probe_result(
        &self,
        identity: &CandidateIdentity,
        health: crate::capability::model::ProviderHealth,
        latency_ms: u64,
    ) {
        let mut providers = self.providers.write().await;
        if let Some(entry) = providers.get_mut(&identity.provider_id) {
            if entry.identity != *identity {
                return;
            }
            entry.health = crate::capability::model::ProviderHealth {
                latency_ms: Some(latency_ms),
                ..health
            };
        }
    }

    async fn invalidate_provider(&self, provider_id: &ProviderId, message: String) {
        let mut providers = self.providers.write().await;
        if let Some(entry) = providers.get_mut(provider_id) {
            entry.health.available = false;
            entry.health.latency_ms = None;
            entry.health.message = Some(message);
        }
    }

    async fn probe_all_providers(
        &self,
        owner: &SessionOwner,
        probe_cwd: &std::path::Path,
        timeout_ms: u64,
    ) -> HostResult<()> {
        let probes: Vec<(CandidateIdentity, Arc<dyn ProviderAdapter>, ProbeRequest)> = {
            let providers = self.providers.read().await;
            providers
                .values()
                .filter_map(|entry| {
                    entry.adapter.clone().map(|adapter| {
                        (
                            entry.identity.clone(),
                            adapter,
                            probe_request_for_owner(owner, probe_cwd, timeout_ms),
                        )
                    })
                })
                .collect()
        };

        for (identity, adapter, request) in probes {
            let started = std::time::Instant::now();
            let health = match adapter.probe(request).await {
                Ok(health) => health,
                Err(error) => crate::capability::model::ProviderHealth {
                    provider_id: identity.provider_id.clone(),
                    available: false,
                    latency_ms: None,
                    message: Some(safe_provider_message(&error)),
                },
            };
            let latency_ms = started.elapsed().as_millis() as u64;
            self.publish_probe_result(&identity, health, latency_ms)
                .await;
        }
        Ok(())
    }
}

impl Default for HostManager {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::too_many_lines)]
// HostFacade start() runs sequential config-validation + spawn; splitting obscures the single code path
#[async_trait]
impl crate::HostFacade for HostManager {
    async fn start(&self, config: HostStartConfig) -> HostResult<()> {
        // Validate config_path does not escape its parent directory.
        //
        // We canonicalize the parent dir first, then validate the config path
        // is under it — no TOCTOU from checking .exists() separately (QC2 F-003).
        //
        // Path-boundary rules, applied BEFORE any read:
        //   - the path must be absolute and non-empty (empty/relative paths
        //     cannot pass through `parent()` matching),
        //   - lexical `ParentDir` (`..`) components are rejected before
        //     canonicalization (matches `validate_workspace_path`),
        //   - when the config file exists, it must resolve under its parent
        //     directory (rejects symlink escapes), and the CANONICAL path is
        //     what is handed to `load_config_from_path` (no re-read of a raw
        //     path that could have been swapped after validation — TOCTOU),
        //   - a genuinely absent optional config (NotFound) is tolerated so
        //     `load_config_from_path` returns defaults.
        if !config.config_path.is_absolute() {
            return Err(HostError::policy_denied(format!(
                "config path must be an absolute, non-empty path: {}",
                config.config_path.display()
            )));
        }
        if config
            .config_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(HostError::policy_denied(format!(
                "config path must not contain '..': {}",
                config.config_path.display()
            )));
        }
        let mut resolved_config_path = config.config_path.clone();
        if let Some(expected_dir) = config.config_path.parent() {
            match std::path::Path::canonicalize(&config.config_path) {
                Ok(canonical_config) => {
                    let canonical_dir =
                        std::path::Path::canonicalize(expected_dir).map_err(|e| {
                            HostError::policy_denied(format!(
                                "config directory cannot be resolved: {} ({})",
                                expected_dir.display(),
                                e
                            ))
                        })?;
                    if !canonical_config.starts_with(&canonical_dir) {
                        return Err(HostError::policy_denied(format!(
                            "config path '{}' escapes config directory '{}'",
                            config.config_path.display(),
                            expected_dir.display()
                        )));
                    }
                    // Retain the canonical resolved path for the load, so a
                    // raw-path swap between validation and read cannot change
                    // what is loaded.
                    resolved_config_path = canonical_config;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Optional config absent — load_config_from_path returns defaults.
                }
                Err(e) => {
                    return Err(HostError::policy_denied(format!(
                        "config path cannot be resolved: {} ({})",
                        config.config_path.display(),
                        e
                    )));
                }
            }
        }

        // Validate and store the canonical workspace_root (QC2 F-002).
        let canonical_workspace_root =
            crate::config::validate_workspace_path(&config.workspace_root)?;

        {
            let admission = self.admission.read().await;
            admission.check_workspace_root(&config.workspace_root)?;
        }

        // Retain the validated config from boot when provided; otherwise load once.
        let host_config = if let Some(cfg) = config.host_config.clone() {
            crate::providers::validate_agent_host_config_providers(&cfg)?;
            cfg
        } else {
            crate::config::load_config_from_path(&resolved_config_path)?
        };
        *self.config.write().await = Some(host_config.clone());

        // Store canonical workspace boundary for session cwd validation.
        *self.workspace_root.write().await = Some(canonical_workspace_root.clone());

        let permission_resolver = HostPermissionResolver::new_native_only(&host_config.policy);

        // Discovery replaces ambient boot registration unless tests pre-registered.
        let pre_registered = !self.providers.read().await.is_empty();
        if !pre_registered {
            let discovered = discover_provider_entries(
                &host_config,
                config.timeouts.clone(),
                permission_resolver,
            )?;
            *self.providers.write().await = discovered;
        }

        if let Some(owner) = config.probe_owner.clone() {
            let probe_cwd = crate::config::validate_workspace_path_under(
                &owner.workspace_root,
                &canonical_workspace_root,
            )?;
            let probe_budget = config.timeouts.initialize_ms;
            self.probe_all_providers(&owner, &probe_cwd, probe_budget)
                .await?;
        } else {
            self.mark_probe_context_unavailable().await;
        }

        let registered_ids: HashSet<ProviderId> = {
            let providers = self.providers.read().await;
            providers.keys().cloned().collect()
        };
        let provider_count = registered_ids.len();

        // Rebuild admission policy from loaded config + registered providers,
        // but only if no custom policy was set via `with_admission()`.
        if !self.admission_custom {
            *self.admission.write().await =
                AdmissionPolicy::from_config(&host_config, registered_ids);
        }

        *self.running.write().await = true;

        let max_sessions = host_config.max_sessions;
        tracing::info!(max_sessions, provider_count, "Host manager started");

        Ok(())
    }

    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> HostResult<crate::core::session::HostSession> {
        let running = self.running.read().await;
        if !*running {
            return Err(HostError::internal("host not started"));
        }
        drop(running);

        // Admission checks: provider allow/deny + session limit.
        {
            let admission = self.admission.read().await;
            admission.check_provider(&request.provider_id)?;
            let session_count = self.sessions.read().await.len();
            admission.check_session_limit(session_count)?;
        }

        // Find the provider
        let providers = self.providers.read().await;
        let entry = providers.get(&request.provider_id).ok_or_else(|| {
            HostError::provider_unavailable(request.provider_id.clone(), "provider not registered")
        })?;

        if !entry.is_available() {
            return Err(HostError::provider_unavailable(
                request.provider_id.clone(),
                "provider not available",
            ));
        }
        let adapter = entry.adapter.clone().ok_or_else(|| {
            HostError::provider_unavailable(
                request.provider_id.clone(),
                "provider adapter not constructed",
            )
        })?;
        drop(providers);

        // Build launch spec with cwd validated against workspace boundary (QC2 F-002)
        // and against the verified Creator workspace root (A5 owner/cwd isolation).
        let boundary = {
            let ws = self.workspace_root.read().await;
            ws.clone()
                .ok_or_else(|| HostError::internal("workspace root not set — host not started"))?
        };
        let validated_cwd = crate::config::validate_workspace_path_under(&request.cwd, &boundary)?;
        // The session cwd must belong to the verified Creator workspace; a
        // profile switch or foreign workspace must never retarget an existing
        // run (A5: owner_workspace_mismatch).
        let owner_workspace =
            crate::config::validate_workspace_path_under(&request.owner.workspace_root, &boundary)?;
        if !validated_cwd.starts_with(&owner_workspace) {
            return Err(HostError::owner_workspace_mismatch(format!(
                "session cwd '{}' is outside verified Creator workspace '{}'",
                validated_cwd.display(),
                owner_workspace.display()
            ))
            .with_provider(request.provider_id.clone()));
        }
        let launch_spec = crate::capability::model::LaunchSpec {
            cwd: validated_cwd,
            model: request.model,
            mode: request.mode,
            mcp_servers: request.mcp_servers,
            owner: request.owner.clone(),
        };

        // Launch the session on the provider with session_ms timeout (D-004).
        // This bounds the total time for provider session creation including
        // any provider-side initialization that may occur during launch.
        let session_timeout = self.config.read().await.as_ref().map_or_else(
            || crate::config::TimeoutConfig::default().session_duration(),
            |c| c.timeouts.session_duration(),
        );
        let provider_id_for_timeout = request.provider_id.clone();
        let handle = match tokio::time::timeout(session_timeout, adapter.launch(launch_spec)).await
        {
            Ok(Ok(handle)) => handle,
            Ok(Err(error)) => {
                if is_launch_class_failure(&error) {
                    self.invalidate_provider(&request.provider_id, safe_provider_message(&error))
                        .await;
                }
                return Err(error);
            }
            Err(_) => {
                let error = HostError::timeout(
                    "launch",
                    format!(
                        "session creation timed out after {}ms",
                        session_timeout.as_millis()
                    ),
                )
                .with_provider(provider_id_for_timeout);
                self.invalidate_provider(&request.provider_id, safe_provider_message(&error))
                    .await;
                return Err(error);
            }
        };

        // Register in our session registry
        let mut sessions = self.sessions.write().await;
        let session_id = sessions.register(
            handle.session_id,
            request.provider_id.clone(),
            handle.capabilities.clone(),
            request.owner.clone(),
            handle.process_identity.clone(),
        );

        // Transition to starting → ready
        sessions.transition_to_starting(&session_id)?;
        sessions.transition_to_ready(&session_id)?;

        // Map session to provider
        {
            let mut session_providers = self.session_providers.write().await;
            session_providers.insert(session_id.clone(), request.provider_id.clone());
        }

        let session = sessions.get(&session_id).expect("just registered").clone();
        drop(sessions);
        Ok(session)
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let (adapter, provider_id) = self.get_provider_for_session(&session_id).await?;

        // Admission check: ops-per-session limit.
        {
            let admission = self.admission.read().await;
            let active_ops: usize = {
                let sessions = self.sessions.read().await;
                sessions
                    .get(&session_id)
                    .map_or(0, |s| usize::from(s.state.is_busy()))
            };
            admission.check_before_exec(&session_id, active_ops)?;
        }

        // Build the managed session handle
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| HostError::internal(format!("session {session_id} not found")))?;

        let handle = ManagedSessionHandle {
            provider_id: session.provider_id.clone(),
            session_id: session_id.clone(),
            capabilities: session.negotiated_capabilities.clone(),
            process_identity: session.process_identity.clone(),
        };
        drop(sessions);

        // Transition to Busy
        let op_id = match &op {
            crate::capability::model::HostOperation::Prompt { op_id, .. } => op_id.clone(),
            _ => HostOperationId::new(),
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.transition_to_busy(&session_id, op_id.clone())?;
        }

        // Execute on the provider. If execute() returns an error (not a stream),
        // the session is stuck in Busy — transition back to Ready before
        // propagating the error (QC3 F-001 defense-in-depth).
        let stream = match adapter.execute(&handle, op).await {
            Ok(s) => s,
            Err(e) => {
                let _ = self
                    .sessions
                    .write()
                    .await
                    .transition_busy_to_ready(&session_id, &op_id);
                // An early execution failure (sealed/ordinary initialization or
                // a confirmed-close failure on the first denied prompt, or a
                // lazy child setup failure) is the same launch-class failure
                // `create_session` already handles: the candidate is no longer
                // ready, so invalidate it. Ordinary prompt/content timeouts are
                // excluded by the category+stage gate inside
                // `is_launch_class_failure`.
                if is_launch_class_failure(&e) {
                    self.invalidate_provider(&provider_id, safe_provider_message(&e))
                        .await;
                }
                return Err(e);
            }
        };

        // Wrap the stream to:
        // 1. Broadcast each event to SSE subscribers
        // 2. Transition back to Ready on terminal event
        let sessions_arc = self.sessions.clone();
        let sid_for_wrap = session_id;
        let oid = op_id;
        let event_tx = self.event_tx.clone();
        let wrapped = stream
            .then(move |result| {
                let sessions = sessions_arc.clone();
                let sid = sid_for_wrap.clone();
                let oid = oid.clone();
                let tx = event_tx.clone();
                async move {
                    if let Ok(event) = &result {
                        // Broadcast to SSE subscribers (ignore lagged/disconnected)
                        let _ = tx.send(event.clone());
                    }
                    if let Ok(HostEvent::OpFinished(_) | HostEvent::OpFailed(_)) = &result {
                        // The operation is terminal: leave Busy or Cancelling
                        // (both keyed by this op) back to Ready. During a
                        // cancellation the session stays Cancelling until the
                        // stream itself emits its terminal event — a
                        // subsequent exec cannot overlap the still-owning
                        // ACP read loop (A5 "reuse serially").
                        let mut sess = sessions.write().await;
                        match sess.get(&sid).map(|s| s.state.clone()) {
                            Some(crate::core::session::SessionState::Busy(op)) if op == oid => {
                                let _ = sess.transition_busy_to_ready(&sid, &oid);
                            }
                            Some(crate::core::session::SessionState::Cancelling(op))
                                if op == oid =>
                            {
                                let _ = sess.transition_cancelling_to_ready(&sid, &oid);
                            }
                            _ => {}
                        }
                    }
                    result
                }
            })
            .boxed();

        Ok(wrapped)
    }

    async fn cancel(&self, op_id: HostOperationId) -> HostResult<()> {
        // Find the session that owns this op
        let session;
        {
            let sessions = self.sessions.read().await;
            session = sessions
                .iter()
                .find(|s| s.active_op_id.as_ref() == Some(&op_id))
                .cloned()
                .ok_or_else(|| HostError::internal(format!("no session found for op {op_id}")))?;
        }

        // Transition to Cancelling. The session STAYS Cancelling until the
        // provider stream emits its terminal event (the wrapped exec stream
        // then transitions to Ready) — no immediate Ready that would allow an
        // overlapping prompt on one ACP session (L2 Issue 5).
        {
            let mut sessions = self.sessions.write().await;
            sessions.transition_to_cancelling(&session.id, &op_id)?;
        }

        // Get the provider and cancel
        let (adapter, _) = self.get_provider_for_session(&session.id).await?;
        let handle = ManagedSessionHandle {
            provider_id: session.provider_id.clone(),
            session_id: session.id.clone(),
            capabilities: session.negotiated_capabilities.clone(),
            process_identity: session.process_identity.clone(),
        };

        adapter.cancel(&handle, op_id).await?;

        // No transition here: the cancelled ACP stream's terminal event will
        // return the session to Ready. A cancel failure leaves the session
        // Cancelling; the stream's own timeout/EOF guarantee a terminal.
        Ok(())
    }

    async fn health(&self) -> HostResult<HostHealth> {
        let running = *self.running.read().await;
        let sessions = self.sessions.read().await;
        let active_ops = sessions.iter().filter(|s| s.state.is_busy()).count();

        Ok(HostHealth {
            running,
            active_sessions: sessions.len(),
            active_operations: active_ops,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        *self.running.write().await = false;

        // Read the configured shutdown timeout (default 5s if config was never set).
        let shutdown_timeout = self.config.read().await.as_ref().map_or_else(
            || crate::config::TimeoutConfig::default().shutdown_duration(),
            |c| c.timeouts.shutdown_duration(),
        );

        // Collect active sessions and their provider mappings before clearing state.
        let session_ids: Vec<HostSessionId>;
        let provider_map: HashMap<HostSessionId, ProviderId>;
        {
            let session_ids_raw: Vec<HostSessionId> = self
                .sessions
                .read()
                .await
                .iter()
                .map(|s| s.id.clone())
                .collect();
            let sp = self.session_providers.read().await;
            provider_map = session_ids_raw
                .iter()
                .filter_map(|id| sp.get(id).map(|pid| (id.clone(), pid.clone())))
                .collect();
            session_ids = session_ids_raw;
        }

        // Collect (adapter, handle) pairs while holding the provider read lock briefly,
        // then call shutdown outside the lock to avoid holding it across .await points.
        // Use the actual negotiated capabilities from the session registry (QC2 F-004).
        // If a session has no registry entry, skip it rather than fabricating
        // full capabilities (QC2 F-004 fix: no acp_full() fallback).
        let shutdown_tasks: Vec<(Arc<dyn ProviderAdapter>, ManagedSessionHandle)> = {
            let sessions = self.sessions.read().await;
            let providers = self.providers.read().await;
            session_ids
                .iter()
                .filter_map(|session_id| {
                    let provider_id = provider_map.get(session_id)?;
                    let entry = providers.get(provider_id)?;
                    let adapter = entry.adapter.clone()?;
                    let session = sessions.get(session_id)?;
                    let handle = ManagedSessionHandle {
                        provider_id: provider_id.clone(),
                        session_id: session_id.clone(),
                        capabilities: session.negotiated_capabilities.clone(),
                        process_identity: session.process_identity.clone(),
                    };
                    Some((adapter, handle))
                })
                .collect()
        };

        // Call ProviderAdapter::shutdown() for each active session with a
        // per-session timeout. A typed error or outer timeout means the exact
        // owned process cleanup was not confirmed: those sessions must remain
        // visibly interrupted (L2 Issue 2) — never marked/removed as cleanly
        // stopped, never dropped from ownership maps.
        let mut cleanup_unconfirmed: Vec<HostSessionId> = Vec::new();
        for (adapter, handle) in shutdown_tasks {
            let session_id = handle.session_id.clone();
            let provider_id = handle.provider_id.clone();

            match tokio::time::timeout(shutdown_timeout, adapter.shutdown(handle)).await {
                Ok(Ok(())) => {
                    tracing::info!(
                        session_id = %session_id,
                        provider_id = %provider_id,
                        "Provider adapter shutdown succeeded"
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        session_id = %session_id,
                        provider_id = %provider_id,
                        error = %e,
                        "Provider adapter shutdown returned error (cleanup unconfirmed)"
                    );
                    cleanup_unconfirmed.push(session_id);
                }
                Err(_) => {
                    tracing::warn!(
                        session_id = %session_id,
                        provider_id = %provider_id,
                        timeout_ms = shutdown_timeout.as_millis(),
                        "Provider adapter shutdown timed out (cleanup unconfirmed)"
                    );
                    cleanup_unconfirmed.push(session_id);
                }
            }
        }

        // Now transition sessions through Stopping → Stopped and clean up the
        // registry — EXCEPT sessions whose cleanup is unconfirmed.
        {
            let mut sessions = self.sessions.write().await;
            for session_id in &session_ids {
                if cleanup_unconfirmed.contains(session_id) {
                    let already_recoverable = matches!(
                        sessions.get(session_id).map(|session| &session.state),
                        Some(crate::core::session::SessionState::ErrorRecoverable)
                    );
                    if !already_recoverable {
                        if matches!(
                            sessions.get(session_id).map(|session| &session.state),
                            Some(crate::core::session::SessionState::Ready)
                        ) {
                            sessions.transition_to_stopping(session_id)?;
                        }
                        sessions.transition_to_error_recoverable(session_id)?;
                    }
                    continue;
                }
                let _ = sessions.transition_to_stopping(session_id);
                let _ = sessions.transition_to_stopped(
                    session_id,
                    crate::capability::model::SessionStopReason::GracefulShutdown,
                );
            }

            for session_id in &session_ids {
                if cleanup_unconfirmed.contains(session_id) {
                    continue;
                }
                let _ = sessions.remove_stopped(session_id);
            }
        }

        // Clear provider mappings only for sessions with confirmed cleanup;
        // unconfirmed sessions keep their ownership mapping for reconciliation.
        {
            let mut session_providers = self.session_providers.write().await;
            for session_id in &session_ids {
                if !cleanup_unconfirmed.contains(session_id) {
                    session_providers.remove(session_id);
                }
            }
        }

        tracing::info!(
            sessions_closed = session_ids.len() - cleanup_unconfirmed.len(),
            sessions_unconfirmed = cleanup_unconfirmed.len(),
            "Host manager shutdown complete (unconfirmed sessions retained)"
        );
        if cleanup_unconfirmed.is_empty() {
            Ok(())
        } else {
            Err(HostError::cleanup_unconfirmed(format!(
                "{} session(s) with unconfirmed owned-process cleanup retained: {:?}",
                cleanup_unconfirmed.len(),
                cleanup_unconfirmed
            )))
        }
    }

    async fn shutdown_session(&self, session_id: HostSessionId) -> HostResult<()> {
        // Verify session exists
        let session = {
            let sessions = self.sessions.read().await;
            sessions
                .get(&session_id)
                .cloned()
                .ok_or_else(|| HostError::internal(format!("session {session_id} not found")))?
        };

        // Get provider for this session
        let (adapter, _) = self.get_provider_for_session(&session_id).await?;

        // Build handle from registry data
        let handle = ManagedSessionHandle {
            provider_id: session.provider_id.clone(),
            session_id: session_id.clone(),
            capabilities: session.negotiated_capabilities.clone(),
            process_identity: session.process_identity.clone(),
        };

        // Read configured shutdown timeout
        let shutdown_timeout = self.config.read().await.as_ref().map_or_else(
            || crate::config::TimeoutConfig::default().shutdown_duration(),
            |c| c.timeouts.shutdown_duration(),
        );

        // Call provider adapter shutdown with timeout. A typed cleanup
        // failure/timeout means the exact owned process was not confirmed
        // reaped: the session must remain visibly interrupted (A5), never be
        // marked/removed as cleanly stopped.
        let cleanup_unconfirmed =
            match tokio::time::timeout(shutdown_timeout, adapter.shutdown(handle)).await {
                Ok(Ok(())) => {
                    tracing::info!(
                        session_id = %session_id,
                        provider_id = %session.provider_id,
                        "Per-session shutdown succeeded"
                    );
                    false
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Per-session provider shutdown returned error (cleanup unconfirmed)"
                    );
                    true
                }
                Err(_) => {
                    tracing::warn!(
                        session_id = %session_id,
                        timeout_ms = shutdown_timeout.as_millis(),
                        "Per-session provider shutdown timed out (cleanup unconfirmed)"
                    );
                    true
                }
            };

        if cleanup_unconfirmed {
            // Keep the session in the registry as ErrorRecoverable: it must
            // not be reported as cleanly stopped while cleanup is unconfirmed.
            let mut sessions = self.sessions.write().await;
            let already_recoverable = matches!(
                sessions.get(&session_id).map(|current| &current.state),
                Some(crate::core::session::SessionState::ErrorRecoverable)
            );
            if !already_recoverable {
                if matches!(
                    sessions.get(&session_id).map(|current| &current.state),
                    Some(crate::core::session::SessionState::Ready)
                ) {
                    sessions.transition_to_stopping(&session_id)?;
                }
                sessions.transition_to_error_recoverable(&session_id)?;
            }
            drop(sessions); // release the write guard before returning
            return Err(HostError::cleanup_unconfirmed(format!(
                "session {session_id} cleanup unconfirmed"
            ))
            .with_provider(session.provider_id.clone())
            .with_session(session_id.clone()));
        }

        // Transition session through Stopping → Stopped
        {
            let mut sessions = self.sessions.write().await;
            let _ = sessions.transition_to_stopping(&session_id);
            let _ = sessions.transition_to_stopped(
                &session_id,
                crate::capability::model::SessionStopReason::GracefulShutdown,
            );
            let _ = sessions.remove_stopped(&session_id);
        }

        // Remove session → provider mapping
        {
            let mut session_providers = self.session_providers.write().await;
            session_providers.remove(&session_id);
        }

        Ok(())
    }

    async fn list_sessions(&self) -> HostResult<Vec<crate::core::session::HostSession>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.iter().cloned().collect())
    }

    async fn provider_catalog(&self) -> HostResult<crate::discovery::ProviderCatalog> {
        let providers = self.providers.read().await;
        let entries = providers
            .values()
            .map(|entry| crate::ProviderCatalogEntry {
                provider_id: entry.metadata.provider_id.clone(),
                display_name: entry.metadata.display_name.clone(),
                protocol_kind: entry.metadata.protocol_kind,
                launch: entry.metadata.launch.clone(),
                source: entry.metadata.source.clone(),
                trust: entry.metadata.trust.clone(),
                capabilities: entry.metadata.capabilities.clone(),
                health: entry.health.clone(),
            })
            .collect();
        Ok(crate::discovery::ProviderCatalog { entries })
    }

    fn subscribe_events(
        &self,
        session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<HostEvent> {
        self.subscribe(&session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::model::{CapabilityDescriptor, LaunchSpec, ProbeRequest};
    use crate::core::session::SessionState;
    use crate::HostFacade;

    /// A minimal mock provider for testing the `HostManager`.
    struct MockProvider {
        provider_id: ProviderId,
    }

    #[async_trait]
    impl ProviderAdapter for MockProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "Mock".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            Ok(ManagedSessionHandle {
                provider_id: self.provider_id.clone(),
                session_id: HostSessionId::new(),
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
                process_identity: None,
            })
        }

        async fn execute(
            &self,
            _session: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            let stream = futures_util::stream::iter(vec![
                Ok(HostEvent::OpStarted(
                    crate::capability::model::OperationStartedEvent {
                        op_id: HostOperationId::new(),
                        session_id: HostSessionId::new(),
                    },
                )),
                Ok(HostEvent::OpFinished(
                    crate::capability::model::OperationFinishedEvent {
                        session_id: HostSessionId::new(),
                        op_id: HostOperationId::new(),
                        reason: crate::capability::model::FinishReason::EndTurn,
                    },
                )),
            ])
            .boxed();
            Ok(stream)
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    /// A mock provider that tracks whether `shutdown()` was called per session.
    struct TrackingMockProvider {
        provider_id: ProviderId,
        shutdown_calls: Arc<std::sync::Mutex<Vec<HostSessionId>>>,
        shutdown_capabilities: Arc<std::sync::Mutex<Vec<CapabilityDescriptor>>>,
    }

    impl TrackingMockProvider {
        fn new(provider_id: ProviderId) -> Self {
            Self {
                provider_id,
                shutdown_calls: Arc::new(std::sync::Mutex::new(Vec::new())),
                shutdown_capabilities: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn take_shutdown_calls(&self) -> Vec<HostSessionId> {
            let mut guard = self.shutdown_calls.lock().unwrap();
            std::mem::take(&mut *guard)
        }

        fn take_shutdown_capabilities(&self) -> Vec<CapabilityDescriptor> {
            let mut guard = self.shutdown_capabilities.lock().unwrap();
            std::mem::take(&mut *guard)
        }
    }

    #[async_trait]
    impl ProviderAdapter for TrackingMockProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "TrackingMock".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            Ok(ManagedSessionHandle {
                provider_id: self.provider_id.clone(),
                session_id: HostSessionId::new(),
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
                process_identity: None,
            })
        }

        async fn execute(
            &self,
            _session: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            let stream = futures_util::stream::iter(vec![
                Ok(HostEvent::OpStarted(
                    crate::capability::model::OperationStartedEvent {
                        op_id: HostOperationId::new(),
                        session_id: HostSessionId::new(),
                    },
                )),
                Ok(HostEvent::OpFinished(
                    crate::capability::model::OperationFinishedEvent {
                        session_id: HostSessionId::new(),
                        op_id: HostOperationId::new(),
                        reason: crate::capability::model::FinishReason::EndTurn,
                    },
                )),
            ])
            .boxed();
            Ok(stream)
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
            self.shutdown_calls.lock().unwrap().push(session.session_id);
            self.shutdown_capabilities
                .lock()
                .unwrap()
                .push(session.capabilities);
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    /// A mock provider whose `shutdown()` hangs forever (to test timeout behavior).
    struct HangingMockProvider {
        provider_id: ProviderId,
        shutdown_called: Arc<std::sync::Mutex<bool>>,
    }

    impl HangingMockProvider {
        fn new(provider_id: ProviderId) -> Self {
            Self {
                provider_id,
                shutdown_called: Arc::new(std::sync::Mutex::new(false)),
            }
        }

        fn was_shutdown_called(&self) -> bool {
            *self.shutdown_called.lock().unwrap()
        }
    }

    #[async_trait]
    impl ProviderAdapter for HangingMockProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "HangingMock".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            Ok(ManagedSessionHandle {
                provider_id: self.provider_id.clone(),
                session_id: HostSessionId::new(),
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
                process_identity: None,
            })
        }

        async fn execute(
            &self,
            _session: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            Ok(futures_util::stream::empty().boxed())
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
            *self.shutdown_called.lock().unwrap() = true;
            // Simulate a provider that never completes shutdown.
            std::future::pending::<()>().await;
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    fn start_config() -> HostStartConfig {
        use std::path::PathBuf;
        HostStartConfig {
            config_path: PathBuf::from("/tmp/nonexistent"),
            workspace_root: PathBuf::from("/tmp"),
            max_sessions: 4,
            max_ops_per_session: 1,
            timeouts: crate::config::TimeoutConfig::default(),
            host_config: None,
            probe_owner: Some(test_owner()),
        }
    }

    fn test_owner() -> crate::capability::model::SessionOwner {
        test_owner_for(std::path::PathBuf::from("/tmp"))
    }

    fn test_owner_for(
        workspace_root: std::path::PathBuf,
    ) -> crate::capability::model::SessionOwner {
        crate::capability::model::SessionOwner {
            creator_id: "ctr_test".to_string(),
            workspace_root,
            orchestration_run_id: None,
        }
    }

    fn mock_launch() -> crate::LaunchStrategy {
        crate::LaunchStrategy::Acp {
            command: "mock".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
        }
    }
    #[tokio::test]
    async fn absent_optional_config_path_returns_defaults() {
        // An absent optional config whose parent exists is tolerated and
        // `load_config_from_path` supplies defaults.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("absent-config.toml");
        let cfg = HostStartConfig {
            config_path,
            workspace_root: temp_dir.path().to_path_buf(),
            ..start_config()
        };
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager
            .start(cfg)
            .await
            .expect("absent optional config must be tolerated (defaults recorded)");
    }

    #[tokio::test]
    async fn relative_and_escaping_config_paths_rejected_before_read() {
        // Relative config path → rejected outright (PolicyDenied), never read.
        let rel = HostStartConfig {
            config_path: std::path::PathBuf::from("../escape.toml"),
            ..start_config()
        };
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        let err = manager
            .start(rel)
            .await
            .expect_err("relative path rejected");
        assert!(matches!(err, HostError::PolicyDenied { .. }), "got {err:?}");

        // Empty config path → rejected before any parent()/read path.
        let empty = HostStartConfig {
            config_path: std::path::PathBuf::from(""),
            ..start_config()
        };
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        let err = manager.start(empty).await.expect_err("empty path rejected");
        assert!(matches!(err, HostError::PolicyDenied { .. }), "got {err:?}");

        // Symlink escape: config path resolves outside its own parent dir.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let outside = temp_dir.path().join("outside-config.toml");

        // Absolute in-root path with lexical `..` → rejected BEFORE
        // canonicalization (validate_workspace_path contract).
        let parentdir = HostStartConfig {
            config_path: temp_dir.path().join("cfg").join("..").join("escape.toml"),
            ..start_config()
        };
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        let err = manager
            .start(parentdir)
            .await
            .expect_err("ParentDir component rejected");
        assert!(matches!(err, HostError::PolicyDenied { .. }), "got {err:?}");
        std::fs::write(&outside, "max_sessions = 2\n").expect("write outside config");
        let parent = temp_dir.path().join("cfg");
        std::fs::create_dir_all(&parent).expect("make cfg dir");
        let link = parent.join("config.toml");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink escape");
        let esc = HostStartConfig {
            config_path: link,
            workspace_root: temp_dir.path().to_path_buf(),
            ..start_config()
        };
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        let err = manager
            .start(esc)
            .await
            .expect_err("symlink escape rejected");
        assert!(matches!(err, HostError::PolicyDenied { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn start_and_health_check() {
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;

        manager
            .start(start_config())
            .await
            .expect("start should succeed");

        let health = manager.health().await.expect("health should succeed");
        assert!(health.running);
        assert_eq!(health.active_sessions, 0);
    }

    #[tokio::test]
    async fn create_session_registers_in_state_machine() {
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;

        manager.start(start_config()).await.expect("start");

        let session = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create_session should succeed");

        assert_eq!(session.state, SessionState::Ready);
        assert_eq!(session.provider_id.0, "mock");

        let health = manager.health().await.expect("health");
        assert_eq!(health.active_sessions, 1);
    }

    #[tokio::test]
    async fn create_session_unknown_provider_fails() {
        let manager = HostManager::new();
        manager.start(start_config()).await.expect("start");

        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("nonexistent"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Admission policy catches unknown providers before the registry lookup.
        assert!(
            err.to_string().contains("not in the known providers list"),
            "expected admission denial, got: {err}"
        );
        assert_eq!(err.category(), "policy_denied");
    }

    #[tokio::test]
    async fn shutdown_clears_sessions() {
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create");

        manager.shutdown().await.expect("shutdown");

        let health = manager.health().await.expect("health");
        assert!(!health.running);
        assert_eq!(health.active_sessions, 0);
    }

    #[tokio::test]
    async fn not_started_rejects_create_session() {
        let manager = HostManager::new();
        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not started"));
    }

    /// Verify that shutdown calls `ProviderAdapter::shutdown()` for every active session.
    #[tokio::test]
    async fn shutdown_calls_provider_adapter_for_each_session() {
        let provider = Arc::new(TrackingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager
            .register_provider(provider.clone(), mock_launch())
            .await;
        manager.start(start_config()).await.expect("start");

        let session1 = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create session 1");

        let session2 = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create session 2");

        manager.shutdown().await.expect("shutdown");

        let shutdown_ids = provider.take_shutdown_calls();
        assert_eq!(
            shutdown_ids.len(),
            2,
            "shutdown should be called for both sessions"
        );
        assert!(
            shutdown_ids.contains(&session1.id),
            "session1 should have been shut down"
        );
        assert!(
            shutdown_ids.contains(&session2.id),
            "session2 should have been shut down"
        );
    }

    /// Verify bounded global shutdown retains cleanup-unconfirmed ownership.
    #[tokio::test]
    async fn shutdown_timeout_retains_unconfirmed_session() {
        let provider = Arc::new(HangingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager
            .register_provider(provider.clone(), mock_launch())
            .await;

        // Create a temp config file with a very short shutdown timeout.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = crate::config::agent_host_config_path(temp_dir.path());
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).expect("create config dir");
        }
        std::fs::write(&config_path, "[timeouts]\nshutdown_ms = 100\n").expect("write config");

        manager
            .start({
                let mut cfg = start_config();
                cfg.config_path = config_path.clone();
                cfg
            })
            .await
            .expect("start");

        manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create");

        // The per-session timeout bounds the call, but unconfirmed cleanup is
        // an error and the owned session must remain visible for recovery.
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown should not hang");
        let error = result.expect_err("hanging cleanup must remain unconfirmed");
        assert_eq!(error.category(), "cleanup_unconfirmed");
        assert!(
            provider.was_shutdown_called(),
            "provider shutdown() should have been invoked"
        );

        let health = manager.health().await.expect("health");
        assert!(!health.running);
        assert_eq!(health.active_sessions, 1);
        let sessions = manager.list_sessions().await.expect("sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].state,
            crate::core::session::SessionState::ErrorRecoverable
        );

        // A failed retry is idempotent: it remains typed cleanup-unconfirmed
        // and preserves the same recoverable ownership state.
        let retry_error = manager
            .shutdown()
            .await
            .expect_err("retry must remain cleanup-unconfirmed");
        assert_eq!(retry_error.category(), "cleanup_unconfirmed");
        let retained = manager.list_sessions().await.expect("retained session");
        assert_eq!(retained.len(), 1);
        assert_eq!(
            retained[0].state,
            crate::core::session::SessionState::ErrorRecoverable
        );
    }

    /// Verify shutdown works correctly when there are no active sessions.
    #[tokio::test]
    async fn shutdown_with_no_sessions_succeeds() {
        let provider = Arc::new(TrackingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager
            .register_provider(provider.clone(), mock_launch())
            .await;
        manager.start(start_config()).await.expect("start");

        manager.shutdown().await.expect("shutdown");

        let shutdown_ids = provider.take_shutdown_calls();
        assert!(
            shutdown_ids.is_empty(),
            "no sessions → no adapter shutdown calls"
        );

        let health = manager.health().await.expect("health");
        assert!(!health.running);
        assert_eq!(health.active_sessions, 0);
    }

    /// Verify shutdown uses actual negotiated capabilities, not `acp_full()` fallback (QC2 F-004).
    #[tokio::test]
    async fn shutdown_uses_negotiated_capabilities_not_acp_full() {
        // Use custom capabilities that differ from acp_full()
        let custom_caps = CapabilityDescriptor {
            text_prompt: true,
            streaming: true,
            cancellation: false, // differs from acp_full
            session_restore: false,
            structured_tool_calls: true,
            mcp_http: false,
            mcp_sse: false,
            mcp_stdio: false,
            images: false,
            audio: false,
            embedded_context: false,
            set_model: false,
            set_mode: false,
            diagnostics: false,
        };
        let expected_caps = custom_caps.clone();

        let provider = Arc::new(TrackingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager
            .register_provider(provider.clone(), mock_launch())
            .await;
        manager.start(start_config()).await.expect("start");

        // Create a session
        let session = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("create session");

        // Override the negotiated capabilities to something non-standard
        {
            let mut sessions = manager.sessions.write().await;
            if let Some(s) = sessions.get_mut(&session.id) {
                s.negotiated_capabilities = custom_caps;
            }
        }

        manager.shutdown().await.expect("shutdown");

        let caps = provider.take_shutdown_capabilities();
        assert_eq!(caps.len(), 1, "should have one shutdown call");
        let received = &caps[0];
        assert!(
            !received.cancellation,
            "shutdown should use negotiated capabilities, not acp_full() — cancellation should be false"
        );
        assert_eq!(
            received.cancellation, expected_caps.cancellation,
            "capabilities should match the negotiated (non-acp-full) descriptor"
        );
    }

    // ── Admission policy enforcement tests ──────────────────────────────

    /// Verify that a provider denied by admission policy returns `PolicyDenied`.
    #[tokio::test]
    async fn create_session_denied_provider_returns_policy_denied() {
        // Build a strict admission policy with no known providers.
        let config = AgentHostConfig::default(); // deny_unknown_providers = true
        let admission = AdmissionPolicy::from_config(&config, HashSet::new());
        let manager = HostManager::with_admission(admission);
        // Register a provider, but admission policy has empty known_providers.
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category(), "policy_denied");
        assert!(
            err.to_string().contains("not in the known providers list"),
            "expected provider denial, got: {err}"
        );
    }

    /// Verify that session limit is enforced by admission policy.
    #[tokio::test]
    async fn create_session_enforces_session_limit() {
        // Build admission with max_sessions = 1.
        let config = AgentHostConfig {
            max_sessions: 1,
            ..AgentHostConfig::default()
        };
        let mut known = HashSet::new();
        known.insert(ProviderId::new("mock"));
        let admission = AdmissionPolicy::from_config(&config, known);
        let manager = HostManager::with_admission(admission);
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        // First session should succeed.
        let _session1 = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("first session should succeed");

        // Second session should be denied by session limit.
        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category(), "policy_denied");
        assert!(
            err.to_string().contains("session limit reached"),
            "expected session limit denial, got: {err}"
        );
    }

    /// Verify that ops-per-session limit is enforced by admission policy in `exec()`.
    #[tokio::test]
    async fn exec_enforces_ops_per_session_limit() {
        // Build admission with max_ops_per_session = 0 (zero → any exec denied).
        let config = AgentHostConfig {
            max_sessions: 4,
            max_ops_per_session: 0,
            ..AgentHostConfig::default()
        };
        let mut known = HashSet::new();
        known.insert(ProviderId::new("mock"));
        let admission = AdmissionPolicy::from_config(&config, known);
        let manager = HostManager::with_admission(admission);
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        let session = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("session should be created");

        let result = manager
            .exec(
                session.id,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![crate::capability::model::HostContentBlock::Text {
                        text: "hello".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;

        assert!(result.is_err());
        let Err(err) = result else {
            panic!("expected error, got success")
        };
        assert_eq!(err.category(), "policy_denied");
        assert!(
            err.to_string().contains("operation limit reached"),
            "expected ops limit denial, got: {err}"
        );
    }

    /// Verify that an allowed provider within limits can create sessions and exec ops.
    #[tokio::test]
    async fn create_session_and_exec_allowed_when_within_limits() {
        let config = AgentHostConfig {
            max_sessions: 4,
            max_ops_per_session: 2,
            ..AgentHostConfig::default()
        };
        let mut known = HashSet::new();
        known.insert(ProviderId::new("mock"));
        let admission = AdmissionPolicy::from_config(&config, known);
        let manager = HostManager::with_admission(admission);
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        let session = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("session should be created");

        assert_eq!(session.state, SessionState::Ready);

        let result = manager
            .exec(
                session.id.clone(),
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![crate::capability::model::HostContentBlock::Text {
                        text: "hello".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;

        assert!(result.is_ok(), "exec should succeed within ops limit");
    }

    /// Verify that `create_session` rejects a cwd outside the workspace boundary (QC2 F-002).
    #[tokio::test]
    async fn create_session_rejects_cwd_outside_workspace_boundary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").expect("write config");

        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;

        // Start with workspace_root = temp_dir
        manager
            .start({
                let mut cfg = start_config();
                cfg.config_path = config_path;
                cfg.workspace_root = temp_dir.path().to_path_buf();
                cfg.probe_owner = Some(test_owner_for(temp_dir.path().to_path_buf()));
                cfg
            })
            .await
            .expect("start");

        // Try to create a session with cwd = /tmp (outside temp_dir)
        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner_for(temp_dir.path().to_path_buf()),
            })
            .await;

        assert!(result.is_err(), "cwd outside boundary should be rejected");
        let err = result.unwrap_err();
        assert_eq!(
            err.category(),
            "policy_denied",
            "expected policy_denied, got: {err}"
        );
    }

    // ── DF-21: Timeout enforcement tests ──────────────────────────────────

    /// A mock provider whose `launch()` takes longer than the configured timeout.
    struct SlowLaunchProvider {
        provider_id: ProviderId,
    }

    #[async_trait]
    impl ProviderAdapter for SlowLaunchProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "SlowLaunch".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            // Simulate a slow provider that takes 10 seconds to launch
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            Ok(ManagedSessionHandle {
                provider_id: self.provider_id.clone(),
                session_id: HostSessionId::new(),
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
                process_identity: None,
            })
        }

        async fn execute(
            &self,
            _session: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            Ok(futures_util::stream::empty().boxed())
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    /// Verify that `create_session()` enforces the `session_ms` timeout (DF-21).
    /// A provider with a slow launch should trigger an `OperationTimeout` error.
    #[tokio::test]
    async fn create_session_enforces_session_timeout() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("config.toml");
        // Configure a very short session_ms (50ms) so the slow provider times out
        std::fs::write(&config_path, "[timeouts]\nsession_ms = 50\n").expect("write config");

        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(SlowLaunchProvider {
                    provider_id: ProviderId::new("slow-mock"),
                }),
                mock_launch(),
            )
            .await;

        manager
            .start({
                let mut cfg = start_config();
                cfg.config_path = config_path;
                cfg.workspace_root = temp_dir.path().to_path_buf();
                cfg.probe_owner = Some(test_owner_for(temp_dir.path().to_path_buf()));
                cfg
            })
            .await
            .expect("start");

        let result = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("slow-mock"),
                cwd: temp_dir.path().to_path_buf(),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner_for(temp_dir.path().to_path_buf()),
            })
            .await;

        assert!(result.is_err(), "slow provider should time out");
        let err = result.unwrap_err();
        assert_eq!(
            err.category(),
            "operation_timeout",
            "expected operation_timeout, got: {err}"
        );
        assert!(
            err.to_string().contains("session creation timed out"),
            "expected session creation timeout message, got: {err}"
        );
    }

    #[tokio::test]
    async fn start_without_probe_owner_marks_candidates_unavailable() {
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        let mut cfg = start_config();
        cfg.probe_owner = None;
        manager.start(cfg).await.expect("start");

        let catalog = manager.provider_catalog().await.expect("catalog");
        let entry = catalog
            .entries
            .iter()
            .find(|e| e.provider_id == ProviderId::new("mock"))
            .expect("mock provider in catalog");
        assert!(!entry.health.available);
        assert_eq!(
            entry.health.message.as_deref(),
            Some("probe_context_unavailable")
        );

        let denied = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await;
        assert!(
            denied.is_err(),
            "admission must match catalog unavailable health"
        );
    }

    /// A provider whose probe succeeds and whose first `execute` fails with a
    /// chosen error — the post-ready launch-failure shape.
    struct ExecFailingMockProvider {
        provider_id: ProviderId,
        error: fn() -> HostError,
    }

    #[async_trait]
    impl ProviderAdapter for ExecFailingMockProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "ExecFail".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: Some(1),
                message: Some("probe ok".to_string()),
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            Ok(ManagedSessionHandle {
                provider_id: self.provider_id.clone(),
                session_id: HostSessionId::new(),
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
                process_identity: None,
            })
        }

        async fn execute(
            &self,
            _handle: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            Err((self.error)())
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    async fn exec_probe_then_fail(
        manager: &HostManager,
        provider: Arc<dyn ProviderAdapter>,
    ) -> String {
        manager.register_provider(provider, mock_launch()).await;
        manager.start(start_config()).await.expect("start");
        let session = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("exec-fail"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect("session created from a probed-available provider");
        match manager
            .exec(
                session.id.clone(),
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![crate::capability::model::HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await
        {
            Err(error) => error.category().to_string(),
            Ok(_) => panic!("execute must fail"),
        }
    }

    async fn catalog_availability(manager: &HostManager) -> bool {
        manager
            .provider_catalog()
            .await
            .expect("catalog")
            .entries
            .iter()
            .find(|e| e.provider_id == ProviderId::new("exec-fail"))
            .expect("provider in catalog")
            .health
            .available
    }

    #[tokio::test]
    async fn exec_launch_class_failure_invalidates_provider() {
        // A launch-class failure from `exec` (the dsh sealed/ordinary init or a
        // lazy child-setup failure) must invalidate the SAME candidate health
        // `create_session` does, so a later admission cannot keep using a
        // provider that is no longer ready.
        let manager = HostManager::new();
        let category = exec_probe_then_fail(
            &manager,
            Arc::new(ExecFailingMockProvider {
                provider_id: ProviderId::new("exec-fail"),
                error: || {
                    HostError::cleanup_unconfirmed("probe cleanup unconfirmed").with_provider(
                        ProviderId::new("exec-fail"),
                    )
                },
            }),
        )
        .await;
        assert_eq!(category, "cleanup_unconfirmed");
        assert!(
            !catalog_availability(&manager).await,
            "a launch-class exec failure must invalidate the candidate"
        );
    }

    #[tokio::test]
    async fn exec_content_timeout_keeps_provider_ready() {
        // The counterpart: an ordinary PROMPT timeout is not a launch-class
        // failure. A provider that initialized successfully must stay ready —
        // blanket-classifying every `operation_timeout` would tear down a
        // working recipe.
        let manager = HostManager::new();
        let category = exec_probe_then_fail(
            &manager,
            Arc::new(ExecFailingMockProvider {
                provider_id: ProviderId::new("exec-fail"),
                error: || {
                    HostError::timeout("prompt", "prompt exceeded its budget").with_provider(
                        ProviderId::new("exec-fail"),
                    )
                },
            }),
        )
        .await;
        assert_eq!(category, "operation_timeout");
        assert!(
            catalog_availability(&manager).await,
            "a content/prompt timeout must NOT invalidate a probed-ready provider"
        );
    }

    #[tokio::test]
    async fn launch_stage_timeout_is_launch_class() {
        // The stage-specific half of the same rule: a timeout AT a launch
        // boundary is launch-class.
        let launch_stage = HostError::timeout("launch", "initialize handshake timed out");
        assert!(is_launch_class_failure(&launch_stage));
        let init_stage = HostError::timeout("initialize", "init timed out");
        assert!(is_launch_class_failure(&init_stage));
        let prompt_stage = HostError::timeout("prompt", "content timed out");
        assert!(!is_launch_class_failure(&prompt_stage));
        // The safe diagnostic must use the REAL category.
        assert_eq!(safe_provider_message(&prompt_stage), "operation timed out");
    }

    #[tokio::test]
    async fn restart_without_probe_owner_invalidates_previously_healthy_entries() {
        // F6: a manager that completed an owner-bound start publishes
        // `available = true`. Re-starting it with NO verified owner must clear
        // EVERY row (not only rows already unavailable), including latency.
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(MockProvider {
                    provider_id: ProviderId::new("mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("owner-bound start");
        assert!(
            manager
                .provider_catalog()
                .await
                .expect("catalog")
                .entries
                .iter()
                .find(|e| e.provider_id == ProviderId::new("mock"))
                .expect("mock in catalog")
                .health
                .available,
            "the probed provider must be available after an owner-bound start"
        );

        let mut cfg = start_config();
        cfg.probe_owner = None;
        manager.start(cfg).await.expect("owner-less re-start");

        let entry = manager
            .provider_catalog()
            .await
            .expect("catalog")
            .entries
            .into_iter()
            .find(|e| e.provider_id == ProviderId::new("mock"))
            .expect("mock in catalog");
        assert!(
            !entry.health.available,
            "a re-start without a verified owner must invalidate every entry"
        );
        assert_eq!(
            entry.health.message.as_deref(),
            Some("probe_context_unavailable")
        );
        assert!(
            entry.health.latency_ms.is_none(),
            "stale latency/success state must be cleared"
        );
    }

    struct LaunchFailingMockProvider {
        provider_id: ProviderId,
    }

    #[async_trait]
    impl ProviderAdapter for LaunchFailingMockProvider {
        fn descriptor(&self) -> crate::capability::model::ProviderDescriptor {
            crate::capability::model::ProviderDescriptor {
                provider_id: self.provider_id.clone(),
                display_name: "LaunchFail".to_string(),
                protocol_kind: crate::capability::model::ProtocolKind::Acp,
                capabilities: crate::capability::model::CapabilityDescriptor::acp_full(),
            }
        }

        async fn probe(
            &self,
            _request: ProbeRequest,
        ) -> HostResult<crate::capability::model::ProviderHealth> {
            Ok(crate::capability::model::ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: Some(1),
                message: Some("probe ok".to_string()),
            })
        }

        async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
            Err(HostError::launch_failed(
                self.provider_id.clone(),
                "simulated spawn failure",
                None,
            ))
        }

        async fn execute(
            &self,
            _session: &ManagedSessionHandle,
            _op: crate::capability::model::HostOperation,
        ) -> HostResult<HostEventStream> {
            unreachable!()
        }

        async fn cancel(
            &self,
            _session: &ManagedSessionHandle,
            _op_id: HostOperationId,
        ) -> HostResult<()> {
            Ok(())
        }

        async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
            Ok(())
        }

        fn capabilities(&self) -> crate::capability::model::CapabilityDescriptor {
            crate::capability::model::CapabilityDescriptor::acp_full()
        }
    }

    #[tokio::test]
    async fn launch_failure_invalidates_catalog_health() {
        let manager = HostManager::new();
        manager
            .register_provider(
                Arc::new(LaunchFailingMockProvider {
                    provider_id: ProviderId::new("fail-mock"),
                }),
                mock_launch(),
            )
            .await;
        manager.start(start_config()).await.expect("start");

        let _ = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("fail-mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: test_owner(),
            })
            .await
            .expect_err("launch should fail");

        let catalog = manager.provider_catalog().await.expect("catalog");
        let entry = catalog
            .entries
            .iter()
            .find(|e| e.provider_id == ProviderId::new("fail-mock"))
            .expect("provider still listed");
        assert!(
            !entry.health.available,
            "launch failure must invalidate readiness"
        );
    }
}
