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
    #[allow(dead_code)]
    admission: RwLock<AdmissionPolicy>,
    /// Host configuration (set on start).
    config: RwLock<Option<AgentHostConfig>>,
    /// Whether the host has been started.
    running: RwLock<bool>,
}

// HashMap import for providers/session_providers
use std::collections::HashMap;

impl HostManager {
    /// Create a new host manager with no providers registered.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(SessionRegistry::new())),
            providers: RwLock::new(HashMap::new()),
            session_providers: RwLock::new(HashMap::new()),
            admission: RwLock::new(AdmissionPolicy::from_config(
                &AgentHostConfig::default(),
                HashSet::new(),
            )),
            config: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// Create a host manager with a specific admission policy.
    #[must_use]
    pub fn with_admission(admission: AdmissionPolicy) -> Self {
        Self {
            admission: RwLock::new(admission),
            ..Self::new()
        }
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
        let session_providers = self.session_providers.read().await;
        let provider_id = session_providers
            .get(session_id)
            .ok_or_else(|| {
                HostError::internal(format!("no provider mapped for session {session_id}"))
            })?
            .clone();

        let providers = self.providers.read().await;
        let entry = providers.get(&provider_id).ok_or_else(|| {
            HostError::provider_unavailable(provider_id.clone(), "provider not registered")
        })?;

        Ok((entry.adapter.clone(), provider_id))
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
        // Load config
        let host_config = crate::config::load_config(&config.config_path)?;
        *self.config.write().await = Some(host_config.clone());

        // Mark all registered providers as available
        let provider_count;
        {
            let mut providers = self.providers.write().await;
            provider_count = providers.len();
            for entry in providers.values_mut() {
                entry.available = true;
            }
        }

        *self.running.write().await = true;

        let max_sessions = host_config.max_sessions;
        tracing::info!(
            max_sessions,
            provider_count,
            "Host manager started"
        );

        Ok(())
    }

    async fn create_session(&self, request: CreateSessionRequest) -> HostResult<crate::core::session::HostSession> {
        let running = self.running.read().await;
        if !*running {
            return Err(HostError::internal("host not started"));
        }
        drop(running);

        // Find the provider
        let providers = self.providers.read().await;
        let entry = providers.get(&request.provider_id).ok_or_else(|| {
            HostError::provider_unavailable(
                request.provider_id.clone(),
                "provider not registered",
            )
        })?;

        if !entry.available {
            return Err(HostError::provider_unavailable(
                request.provider_id.clone(),
                "provider not available",
            ));
        }
        let adapter = entry.adapter.clone();
        drop(providers);

        // Build launch spec
        let launch_spec = crate::capability::model::LaunchSpec {
            cwd: request.cwd,
            model: request.model,
            mode: request.mode,
            mcp_servers: request.mcp_servers,
        };

        // Launch the session on the provider
        let handle = adapter.launch(launch_spec).await?;

        // Register in our session registry
        let mut sessions = self.sessions.write().await;
        let session_id = sessions.register(
            request.provider_id.clone(),
            handle.capabilities.clone(),
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
        Ok(session)
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let (adapter, _) = self.get_provider_for_session(&session_id).await?;

        // Build the managed session handle
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or_else(|| {
            HostError::internal(format!("session {session_id} not found"))
        })?;

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

        // Execute on the provider
        let stream = adapter.execute(&handle, op).await?;

        // Wrap the stream to transition back to Ready on terminal event
        let sessions_arc = self.sessions.clone();
        let sid_for_wrap = session_id;
        let oid = op_id;
        let wrapped = stream
            .then(move |result| {
                let sessions = sessions_arc.clone();
                let sid = sid_for_wrap.clone();
                let oid = oid.clone();
                async move {
                    if let Ok(HostEvent::OpFinished(_)) | Ok(HostEvent::OpFailed(_)) = &result {
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
                .ok_or_else(|| {
                    HostError::internal(format!("no session found for op {op_id}"))
                })?;
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

        // Shutdown all sessions
        let mut sessions = self.sessions.write().await;
        let session_ids: Vec<HostSessionId> = sessions.iter().map(|s| s.id.clone()).collect();

        for session_id in &session_ids {
            let _ = sessions.transition_to_stopping(session_id);
            let _ = sessions.transition_to_stopped(
                session_id,
                crate::capability::model::SessionStopReason::GracefulShutdown,
            );
        }

        // Remove all stopped sessions from the registry
        for session_id in &session_ids {
            let _ = sessions.remove_stopped(session_id);
        }

        // Clear provider mappings
        self.session_providers.write().await.clear();

        tracing::info!("Host manager shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacade;
    use crate::core::session::SessionState;
    use crate::capability::model::{LaunchSpec, ProbeRequest};

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

        async fn probe(&self, _request: ProbeRequest) -> HostResult<crate::capability::model::ProviderHealth> {
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
                Ok(HostEvent::OpStarted(crate::capability::model::OperationStartedEvent {
                    op_id: HostOperationId::new(),
                    session_id: HostSessionId::new(),
                })),
                Ok(HostEvent::OpFinished(crate::capability::model::OperationFinishedEvent {
                    session_id: HostSessionId::new(),
                    op_id: HostOperationId::new(),
                    reason: crate::capability::model::FinishReason::EndTurn,
                })),
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

    fn start_config() -> HostStartConfig {
        use std::path::PathBuf;
        HostStartConfig {
            config_path: PathBuf::from("/tmp/nonexistent"),
            workspace_root: PathBuf::from("/tmp/workspace"),
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

        manager.start(start_config()).await.expect("start should succeed");

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
        assert!(result.unwrap_err().to_string().contains("not registered"));
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
}
