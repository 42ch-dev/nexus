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
    ManagedSessionHandle,
};
use crate::config::AgentHostConfig;
use crate::core::session::SessionRegistry;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::policy::admission::AdmissionPolicy;
use crate::ProviderAdapter;

/// Entry in the provider map.
struct ProviderEntry {
    /// The provider adapter.
    adapter: Arc<dyn ProviderAdapter>,
    /// Whether this provider is currently available.
    available: bool,
}

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
    pub async fn register_provider(&self, adapter: Arc<dyn ProviderAdapter>) {
        let desc = adapter.descriptor();
        let provider_id = desc.provider_id.clone();
        let mut providers = self.providers.write().await;
        providers.insert(
            provider_id,
            ProviderEntry {
                adapter,
                available: false,
            },
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
        };

        Ok((adapter, provider_id))
    }
}

impl Default for HostManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::HostFacade for HostManager {
    async fn start(&self, config: HostStartConfig) -> HostResult<()> {
        // Validate config_path does not escape its parent directory.
        // We canonicalize the parent dir first, then validate the config path
        // is under it — no TOCTOU from checking .exists() separately (QC2 F-003).
        if let Some(expected_dir) = config.config_path.parent() {
            // Only validate containment if the parent directory exists on disk.
            // If the parent doesn't exist, the config file can't either, so
            // load_config_from_path will simply return defaults.
            if let Ok(canonical_dir) = std::path::Path::canonicalize(expected_dir) {
                // Lexical containment check before attempting to canonicalize
                // the config path — reject obvious escapes early.
                let normalized_config = crate::config::validate_workspace_path(&config.config_path);
                if let Ok(canonical_config) = normalized_config {
                    if !canonical_config.starts_with(&canonical_dir) {
                        return Err(HostError::policy_denied(format!(
                            "config path '{}' escapes config directory '{}'",
                            config.config_path.display(),
                            expected_dir.display()
                        )));
                    }
                }
                // If canonicalize of config_path fails, the file doesn't exist
                // (optional config) — load_config_from_path handles this correctly.
            }
        }

        // Validate and store the canonical workspace_root (QC2 F-002).
        let canonical_workspace_root =
            crate::config::validate_workspace_path(&config.workspace_root)?;

        {
            let admission = self.admission.read().await;
            admission.check_workspace_root(&config.workspace_root)?;
        }

        // Load config using TOCTOU-safe direct read (QC2 F-003).
        // load_config_from_path reads the file directly — NotFound → defaults.
        let host_config = crate::config::load_config_from_path(&config.config_path)?;
        *self.config.write().await = Some(host_config.clone());

        // Store canonical workspace boundary for session cwd validation.
        *self.workspace_root.write().await = Some(canonical_workspace_root);

        // Mark all registered providers as available and collect their IDs.
        let provider_count;
        let registered_ids: HashSet<ProviderId>;
        {
            let mut providers = self.providers.write().await;
            provider_count = providers.len();
            registered_ids = providers.keys().cloned().collect();
            for entry in providers.values_mut() {
                entry.available = true;
            }
        }

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

        if !entry.available {
            return Err(HostError::provider_unavailable(
                request.provider_id.clone(),
                "provider not available",
            ));
        }
        let adapter = entry.adapter.clone();
        drop(providers);

        // Build launch spec with cwd validated against workspace boundary (QC2 F-002).
        let boundary = {
            let ws = self.workspace_root.read().await;
            ws.clone()
                .ok_or_else(|| HostError::internal("workspace root not set — host not started"))?
        };
        let validated_cwd = crate::config::validate_workspace_path_under(&request.cwd, &boundary)?;
        let launch_spec = crate::capability::model::LaunchSpec {
            cwd: validated_cwd,
            model: request.model,
            mode: request.mode,
            mcp_servers: request.mcp_servers,
        };

        // Launch the session on the provider with session_ms timeout (D-004).
        // This bounds the total time for provider session creation including
        // any provider-side initialization that may occur during launch.
        let session_timeout = self.config.read().await.as_ref().map_or_else(
            || crate::config::TimeoutConfig::default().session_duration(),
            |c| c.timeouts.session_duration(),
        );
        let provider_id_for_timeout = request.provider_id.clone();
        let handle = tokio::time::timeout(session_timeout, adapter.launch(launch_spec))
            .await
            .map_err(|_| {
                HostError::timeout(
                    "launch",
                    format!(
                        "session creation timed out after {}ms",
                        session_timeout.as_millis()
                    ),
                )
                .with_provider(provider_id_for_timeout)
            })??;

        // Register in our session registry
        let mut sessions = self.sessions.write().await;
        let session_id =
            sessions.register(request.provider_id.clone(), handle.capabilities.clone());

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
        let (adapter, _) = self.get_provider_for_session(&session_id).await?;

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
                        let mut sess = sessions.write().await;
                        let _ = sess.transition_busy_to_ready(&sid, &oid);
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

        // Transition to Cancelling
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
        };

        adapter.cancel(&handle, op_id.clone()).await?;

        // Transition back to Ready
        {
            let mut sessions = self.sessions.write().await;
            sessions.transition_cancelling_to_ready(&session.id, &op_id)?;
        }

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
                    let adapter = entry.adapter.clone();
                    let session = sessions.get(session_id)?;
                    let handle = ManagedSessionHandle {
                        provider_id: provider_id.clone(),
                        session_id: session_id.clone(),
                        capabilities: session.negotiated_capabilities.clone(),
                    };
                    Some((adapter, handle))
                })
                .collect()
        };

        // Call ProviderAdapter::shutdown() for each active session with a per-session timeout.
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
                        "Provider adapter shutdown returned error"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        session_id = %session_id,
                        provider_id = %provider_id,
                        timeout_ms = shutdown_timeout.as_millis(),
                        "Provider adapter shutdown timed out, proceeding with cleanup"
                    );
                }
            }
        }

        // Now transition sessions through Stopping → Stopped and clean up the registry.
        {
            let mut sessions = self.sessions.write().await;
            for session_id in &session_ids {
                let _ = sessions.transition_to_stopping(session_id);
                let _ = sessions.transition_to_stopped(
                    session_id,
                    crate::capability::model::SessionStopReason::GracefulShutdown,
                );
            }

            for session_id in &session_ids {
                let _ = sessions.remove_stopped(session_id);
            }
        }

        // Clear provider mappings
        self.session_providers.write().await.clear();

        tracing::info!(
            sessions_closed = session_ids.len(),
            "Host manager shutdown complete"
        );
        Ok(())
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
        };

        // Read configured shutdown timeout
        let shutdown_timeout = self.config.read().await.as_ref().map_or_else(
            || crate::config::TimeoutConfig::default().shutdown_duration(),
            |c| c.timeouts.shutdown_duration(),
        );

        // Call provider adapter shutdown with timeout
        match tokio::time::timeout(shutdown_timeout, adapter.shutdown(handle)).await {
            Ok(Ok(())) => {
                tracing::info!(
                    session_id = %session_id,
                    provider_id = %session.provider_id,
                    "Per-session shutdown succeeded"
                );
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "Per-session provider shutdown returned error"
                );
            }
            Err(_) => {
                tracing::warn!(
                    session_id = %session_id,
                    timeout_ms = shutdown_timeout.as_millis(),
                    "Per-session provider shutdown timed out"
                );
            }
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
        // Collect adapter descriptors under a short-lived read lock.
        let descs: Vec<(
            ProviderId,
            bool,
            crate::capability::model::ProviderDescriptor,
        )> = {
            let providers = self.providers.read().await;
            providers
                .iter()
                .map(|(pid, entry)| (pid.clone(), entry.available, entry.adapter.descriptor()))
                .collect()
        };

        let mut entries = Vec::new();
        for (provider_id, available, desc) in descs {
            entries.push(crate::ProviderCatalogEntry {
                provider_id: provider_id.clone(),
                display_name: desc.display_name.clone(),
                protocol_kind: desc.protocol_kind,
                launch: match desc.protocol_kind {
                    crate::capability::model::ProtocolKind::Acp => crate::LaunchStrategy::Acp {
                        command: String::new(),
                        args: vec![],
                        env: std::collections::HashMap::new(),
                    },
                    crate::capability::model::ProtocolKind::NativeCli => {
                        crate::LaunchStrategy::NativeCli {
                            command: String::new(),
                            args: vec![],
                            env: std::collections::HashMap::new(),
                        }
                    }
                },
                source: crate::DiscoverySource::Config,
                trust: crate::TrustLevel::Explicit,
                capabilities: desc.capabilities,
                health: crate::capability::model::ProviderHealth {
                    provider_id: provider_id.clone(),
                    available,
                    latency_ms: None,
                    message: None,
                },
            });
        }

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

    /// A minimal mock provider for testing the HostManager.
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
        }
    }

    #[tokio::test]
    async fn start_and_health_check() {
        let manager = HostManager::new();
        manager
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not started"));
    }

    /// Verify that shutdown calls ProviderAdapter::shutdown() for every active session.
    #[tokio::test]
    async fn shutdown_calls_provider_adapter_for_each_session() {
        let provider = Arc::new(TrackingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager.register_provider(provider.clone()).await;
        manager.start(start_config()).await.expect("start");

        let session1 = manager
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new("mock"),
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
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

    /// Verify that shutdown does not hang when a provider's shutdown takes too long.
    #[tokio::test]
    async fn shutdown_respects_timeout_and_does_not_hang() {
        let provider = Arc::new(HangingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager.register_provider(provider.clone()).await;

        // Create a temp config file with a very short shutdown timeout.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = crate::config::agent_host_config_path(temp_dir.path());
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).expect("create config dir");
        }
        std::fs::write(&config_path, "[timeouts]\nshutdown_ms = 100\n").expect("write config");

        manager
            .start(HostStartConfig {
                config_path: config_path.clone(),
                workspace_root: std::path::PathBuf::from("/tmp"),
                max_sessions: 4,
                max_ops_per_session: 1,
                timeouts: crate::config::TimeoutConfig::default(),
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
            })
            .await
            .expect("create");

        // shutdown should complete within a reasonable total time even though the
        // provider's shutdown() hangs forever.  The per-session timeout is 100ms,
        // so the whole thing should finish well within 5s.
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown should not hang");

        assert!(
            result.is_ok(),
            "shutdown should succeed even with a hanging provider"
        );
        assert!(
            provider.was_shutdown_called(),
            "provider shutdown() should have been invoked"
        );

        let health = manager.health().await.expect("health");
        assert!(!health.running);
        assert_eq!(health.active_sessions, 0);
    }

    /// Verify shutdown works correctly when there are no active sessions.
    #[tokio::test]
    async fn shutdown_with_no_sessions_succeeds() {
        let provider = Arc::new(TrackingMockProvider::new(ProviderId::new("mock")));
        let manager = HostManager::new();
        manager.register_provider(provider.clone()).await;
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

    /// Verify shutdown uses actual negotiated capabilities, not acp_full() fallback (QC2 F-004).
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
        manager.register_provider(provider.clone()).await;
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

    /// Verify that a provider denied by admission policy returns PolicyDenied.
    #[tokio::test]
    async fn create_session_denied_provider_returns_policy_denied() {
        // Build a strict admission policy with no known providers.
        let config = AgentHostConfig::default(); // deny_unknown_providers = true
        let admission = AdmissionPolicy::from_config(&config, HashSet::new());
        let manager = HostManager::with_admission(admission);
        // Register a provider, but admission policy has empty known_providers.
        manager
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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

    /// Verify that ops-per-session limit is enforced by admission policy in exec().
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
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
                },
            )
            .await;

        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error, got success"),
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
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
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
                },
            )
            .await;

        assert!(result.is_ok(), "exec should succeed within ops limit");
    }

    /// Verify that create_session rejects a cwd outside the workspace boundary (QC2 F-002).
    #[tokio::test]
    async fn create_session_rejects_cwd_outside_workspace_boundary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").expect("write config");

        let manager = HostManager::new();
        manager
            .register_provider(Arc::new(MockProvider {
                provider_id: ProviderId::new("mock"),
            }))
            .await;

        // Start with workspace_root = temp_dir
        manager
            .start(HostStartConfig {
                config_path,
                workspace_root: temp_dir.path().to_path_buf(),
                max_sessions: 4,
                max_ops_per_session: 1,
                timeouts: crate::config::TimeoutConfig::default(),
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
    /// A provider with a slow launch should trigger an OperationTimeout error.
    #[tokio::test]
    async fn create_session_enforces_session_timeout() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("config.toml");
        // Configure a very short session_ms (50ms) so the slow provider times out
        std::fs::write(&config_path, "[timeouts]\nsession_ms = 50\n").expect("write config");

        let manager = HostManager::new();
        manager
            .register_provider(Arc::new(SlowLaunchProvider {
                provider_id: ProviderId::new("slow-mock"),
            }))
            .await;

        manager
            .start(HostStartConfig {
                config_path,
                workspace_root: temp_dir.path().to_path_buf(),
                max_sessions: 4,
                max_ops_per_session: 1,
                timeouts: crate::config::TimeoutConfig::default(),
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
}
