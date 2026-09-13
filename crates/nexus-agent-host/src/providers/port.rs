//! ProviderPort adapter over existing ProviderAdapter / HostFacade.
//!
//! Owns the host→wire mappings (`HostEvent` promotion, provider/operation wire
//! vocabulary) and the one-outstanding-pull admission at the port boundary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use futures_util::StreamExt;
use nexus_contracts::{
    CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderEventBatchGap,
    ProviderEventBatchGapReason, ProviderHostEvent, ProviderReply,
    generated::core::provider_event_batch::NexusProviderHostEvent,
    provider_call::ProviderCallMethod, provider_reply::ProviderReplyHealth,
};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::capability::model::{
    CreateSessionRequest, HostEvent, HostEventStream, HostOperation, LaunchSpec, ProbeRequest,
    ProtocolKind, ProviderHealth,
};
use crate::core::session::SessionState;
use crate::error::HostError;
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::{HostFacade, ProviderAdapter};

const MAX_EVENTS_PER_BATCH: u32 = 16;
const MAX_BYTES_PER_BATCH: u32 = 256 * 1024;

/// Wire spelling of [`ProtocolKind`] — the contract's snake_case value.
#[must_use]
pub fn protocol_kind_wire(kind: ProtocolKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Wire status of `op_id`, derived from its owning session's state.
#[must_use]
pub fn operation_status_wire(state: &SessionState, op_id: &HostOperationId) -> &'static str {
    match state {
        SessionState::Busy(current) if current == op_id => "running",
        SessionState::Cancelling(current) if current == op_id => "cancelling",
        SessionState::ErrorTerminal | SessionState::ErrorRecoverable => "failed",
        SessionState::Stopped | SessionState::Ready => "completed",
        SessionState::Created | SessionState::Starting | SessionState::Stopping => "pending",
        SessionState::Busy(_) | SessionState::Cancelling(_) => "unknown",
    }
}

/// Per-operation pull state. The admission flag is atomic and lives outside the
/// stream slot, so a concurrent pull is rejected without waiting on the owner.
struct OperationEntry {
    pull_in_flight: AtomicBool,
    terminal_seen: AtomicBool,
    stream_slot: Arc<Mutex<Option<HostEventStream>>>,
}

/// Releases the pull admission flag on every exit path, including `?` and drop.
struct PullGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for PullGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

/// Owns the stream for one pull and returns it to the slot on every exit path,
/// including cancellation of the awaiting future.
struct StreamRestore {
    slot: Arc<Mutex<Option<HostEventStream>>>,
    stream: Option<HostEventStream>,
}

impl StreamRestore {
    fn new(slot: Arc<Mutex<Option<HostEventStream>>>) -> Self {
        Self { slot, stream: None }
    }

    /// Take the stream out of the slot; the slot lock is never held across a pull.
    async fn acquire(&mut self) {
        let mut guard = self.slot.lock().await;
        self.stream = guard.take();
    }
}

impl Drop for StreamRestore {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            if let Ok(mut guard) = self.slot.try_lock() {
                if guard.is_none() {
                    *guard = Some(stream);
                }
            }
        }
    }
}

/// Pull-based provider port backed by the existing host facade and adapters.
pub struct ProviderPortAdapter {
    host: Arc<dyn HostFacade>,
    providers: HashMap<ProviderId, Arc<dyn ProviderAdapter>>,
    operations: DashMap<String, Arc<OperationEntry>>,
}

impl ProviderPortAdapter {
    #[must_use]
    pub fn new(host: Arc<dyn HostFacade>) -> Self {
        Self {
            host,
            providers: HashMap::new(),
            operations: DashMap::new(),
        }
    }

    pub fn register_provider(&mut self, provider_id: ProviderId, adapter: Arc<dyn ProviderAdapter>) {
        self.providers.insert(provider_id, adapter);
    }

    fn insert_operation(&self, operation_id: String, stream: HostEventStream) {
        self.operations.insert(
            operation_id,
            Arc::new(OperationEntry {
                pull_in_flight: AtomicBool::new(false),
                terminal_seen: AtomicBool::new(false),
                stream_slot: Arc::new(Mutex::new(Some(stream))),
            }),
        );
    }

    fn map_host_error(err: HostError) -> CoreError {
        let (code, http_status) = match &err {
            HostError::ProviderUnavailable { .. } => (CoreErrorCode::NotFound, 404),
            HostError::PolicyDenied { .. } | HostError::OwnerWorkspaceMismatch { .. } => {
                (CoreErrorCode::Forbidden, 403)
            }
            HostError::OperationCancelled { .. } => (CoreErrorCode::Interrupted, 503),
            HostError::OperationTimeout { .. } | HostError::CleanupUnconfirmed { .. } => {
                (CoreErrorCode::Busy, 503)
            }
            HostError::CapabilityUnsupported { .. } => (CoreErrorCode::InvalidInput, 400),
            HostError::LaunchFailed { .. }
            | HostError::ProviderProtocolError { .. }
            | HostError::InternalHostError { .. } => (CoreErrorCode::Internal, 500),
        };
        CoreError {
            code,
            message: err.to_string(),
            details: Default::default(),
            http_status: Some(http_status),
        }
    }

    fn invalid_input(message: impl Into<String>) -> CoreError {
        CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.into(),
            details: Default::default(),
            http_status: Some(400),
        }
    }

    fn internal_error(message: impl Into<String>) -> CoreError {
        CoreError {
            code: CoreErrorCode::Internal,
            message: message.into(),
            details: Default::default(),
            http_status: Some(500),
        }
    }

    fn parse_session_id(raw: &str) -> Result<HostSessionId, CoreError> {
        let uuid =
            Uuid::parse_str(raw).map_err(|e| Self::invalid_input(format!("session_id: {e}")))?;
        Ok(HostSessionId(uuid))
    }

    fn parse_operation_id(raw: &str) -> Result<HostOperationId, CoreError> {
        let uuid =
            Uuid::parse_str(raw).map_err(|e| Self::invalid_input(format!("operation_id: {e}")))?;
        Ok(HostOperationId(uuid))
    }

    fn health_to_wire(health: ProviderHealth) -> ProviderReplyHealth {
        ProviderReplyHealth {
            provider_id: health.provider_id.to_string(),
            available: health.available,
            latency_ms: health.latency_ms,
            message: health.message,
        }
    }

    fn oversized_gap(operation_id: &str) -> ProviderEventBatchGap {
        ProviderEventBatchGap {
            reason: ProviderEventBatchGapReason::Oversized,
            operation_id: Some(operation_id.to_string()),
            resync_required: true,
            inspect_url: format!("native://provider/events/{operation_id}"),
        }
    }

    /// Drain one bounded batch from an owned stream. The caller guarantees
    /// single-owner access through the per-operation admission flag.
    async fn pull_events(
        entry: &OperationEntry,
        operation_id: &str,
        stream: &mut HostEventStream,
        cap_events: u32,
        cap_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        let mut events = Vec::new();
        let mut used_bytes = 0u32;
        while (events.len() as u32) < cap_events && used_bytes < cap_bytes {
            match stream.next().await {
                Some(Ok(event)) => {
                    let wire = wire_to_batch_event(host_event_to_wire(&event)?)?;
                    let size = serde_json::to_vec(&wire).map_or(0, |bytes| bytes.len());
                    if events.is_empty() && size as u32 > cap_bytes {
                        return Ok(ProviderEventBatch {
                            operation_id: operation_id.to_string(),
                            events: vec![],
                            has_more: true,
                            gap: Some(Self::oversized_gap(operation_id)),
                        });
                    }
                    if used_bytes + size as u32 > cap_bytes && !events.is_empty() {
                        break;
                    }
                    used_bytes += size as u32;
                    events.push(wire);
                    if is_terminal(&event) {
                        entry.terminal_seen.store(true, Ordering::Release);
                        break;
                    }
                }
                Some(Err(err)) => return Err(Self::map_host_error(err)),
                None => {
                    entry.terminal_seen.store(true, Ordering::Release);
                    break;
                }
            }
        }

        let has_more = !entry.terminal_seen.load(Ordering::Acquire);
        Ok(ProviderEventBatch {
            operation_id: operation_id.to_string(),
            events,
            has_more,
            gap: None,
        })
    }
}

fn wire_to_batch_event(event: ProviderHostEvent) -> ProviderResult<NexusProviderHostEvent> {
    let value = serde_json::to_value(&event).map_err(|e| CoreError {
        code: CoreErrorCode::Internal,
        message: format!("host_event_batch_serialize: {e}"),
        details: Default::default(),
        http_status: Some(500),
    })?;
    serde_json::from_value(value).map_err(|e| CoreError {
        code: CoreErrorCode::SchemaMismatch,
        message: format!("host_event_batch_wire: {e}"),
        details: Default::default(),
        http_status: Some(409),
    })
}

pub fn host_event_to_wire(event: &HostEvent) -> ProviderResult<ProviderHostEvent> {
    let value = serde_json::to_value(event).map_err(|e| CoreError {
        code: CoreErrorCode::Internal,
        message: format!("host_event_serialize: {e}"),
        details: Default::default(),
        http_status: Some(500),
    })?;
    serde_json::from_value(value).map_err(|e| CoreError {
        code: CoreErrorCode::SchemaMismatch,
        message: format!("host_event_wire: {e}"),
        details: Default::default(),
        http_status: Some(409),
    })
}

fn is_terminal(event: &HostEvent) -> bool {
    matches!(
        event,
        HostEvent::OpFinished(_) | HostEvent::OpFailed(_) | HostEvent::SessionStopped(_)
    )
}

#[async_trait]
impl ProviderPort for ProviderPortAdapter {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        let payload_value = serde_json::Value::Object(request.payload.clone());
        match request.method {
            ProviderCallMethod::Probe => {
                let provider_id = ProviderId::new(
                    payload_value
                        .get("provider_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Self::invalid_input("probe requires provider_id"))?,
                );
                let adapter = self.providers.get(&provider_id).ok_or_else(|| CoreError {
                    code: CoreErrorCode::NotFound,
                    message: format!("provider {provider_id} not registered"),
                    details: Default::default(),
                    http_status: Some(404),
                })?;
                let probe: ProbeRequest = serde_json::from_value(payload_value)
                    .map_err(|e| Self::invalid_input(format!("probe payload: {e}")))?;
                let health = adapter.probe(probe).await.map_err(Self::map_host_error)?;
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: None,
                    session_id: None,
                    health: Some(Self::health_to_wire(health)),
                    error: None,
                })
            }
            ProviderCallMethod::Launch => {
                let provider_id = ProviderId::new(
                    payload_value
                        .get("provider_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Self::invalid_input("launch requires provider_id"))?,
                );
                let _adapter = self.providers.get(&provider_id).ok_or_else(|| CoreError {
                    code: CoreErrorCode::NotFound,
                    message: format!("provider {provider_id} not registered"),
                    details: Default::default(),
                    http_status: Some(404),
                })?;
                let spec: LaunchSpec = serde_json::from_value(payload_value)
                    .map_err(|e| Self::invalid_input(format!("launch payload: {e}")))?;
                let session = self
                    .host
                    .create_session(CreateSessionRequest {
                        provider_id,
                        cwd: spec.cwd,
                        model: spec.model,
                        mode: spec.mode,
                        mcp_servers: spec.mcp_servers,
                        metadata: serde_json::Value::Object(Default::default()),
                        owner: spec.owner,
                    })
                    .await
                    .map_err(Self::map_host_error)?;
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: None,
                    session_id: Some(session.id.to_string()),
                    health: None,
                    error: None,
                })
            }
            ProviderCallMethod::Execute => {
                let session_raw = request
                    .session_id
                    .as_deref()
                    .ok_or_else(|| Self::invalid_input("execute requires session_id"))?;
                let session_id = Self::parse_session_id(session_raw)?;
                let op: HostOperation = serde_json::from_value(payload_value)
                    .map_err(|e| Self::invalid_input(format!("execute payload: {e}")))?;
                let op_id = match &op {
                    HostOperation::Prompt { op_id, .. } => op_id.clone(),
                    _ => HostOperationId::new(),
                };
                let stream = self
                    .host
                    .exec(session_id.clone(), op)
                    .await
                    .map_err(Self::map_host_error)?;
                self.insert_operation(op_id.to_string(), stream);
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: Some(op_id.to_string()),
                    session_id: Some(session_id.to_string()),
                    health: None,
                    error: None,
                })
            }
            ProviderCallMethod::Cancel => {
                let op_raw = request
                    .operation_id
                    .as_deref()
                    .ok_or_else(|| Self::invalid_input("cancel requires operation_id"))?;
                let op_id = Self::parse_operation_id(op_raw)?;
                let op_id_str = op_id.to_string();
                self.host.cancel(op_id).await.map_err(Self::map_host_error)?;
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: Some(op_id_str),
                    session_id: request.session_id.clone(),
                    health: None,
                    error: None,
                })
            }
            ProviderCallMethod::Shutdown => {
                let session_raw = request
                    .session_id
                    .as_deref()
                    .ok_or_else(|| Self::invalid_input("shutdown requires session_id"))?;
                let session_id = Self::parse_session_id(session_raw)?;
                self.host
                    .shutdown_session(session_id.clone())
                    .await
                    .map_err(Self::map_host_error)?;
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: None,
                    session_id: Some(session_id.to_string()),
                    health: None,
                    error: None,
                })
            }
        }
    }

    async fn next(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        let cap_events = max_events.min(MAX_EVENTS_PER_BATCH);
        let cap_bytes = max_bytes.min(MAX_BYTES_PER_BATCH);

        // Clone the entry handle and release the map guard before any await.
        let entry = self
            .operations
            .get(&operation_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| CoreError {
                code: CoreErrorCode::NotFound,
                message: format!("operation {operation_id} not found"),
                details: Default::default(),
                http_status: Some(404),
            })?;

        if entry.terminal_seen.load(Ordering::Acquire) {
            return Ok(ProviderEventBatch {
                operation_id,
                events: vec![],
                has_more: false,
                gap: None,
            });
        }

        // Atomic, non-blocking admission: a concurrent pull is Busy immediately.
        if entry
            .pull_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(CoreError {
                code: CoreErrorCode::Busy,
                message: format!("pull already in flight for operation {operation_id}"),
                details: Default::default(),
                http_status: Some(503),
            });
        }
        let _pull = PullGuard {
            flag: &entry.pull_in_flight,
        };

        let mut restore = StreamRestore::new(Arc::clone(&entry.stream_slot));
        restore.acquire().await;

        if let Some(stream) = restore.stream.as_mut() {
            Self::pull_events(&entry, &operation_id, stream, cap_events, cap_bytes).await
        } else {
            Err(Self::internal_error(format!(
                "operation {operation_id} stream unavailable"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::model::{
        CreateSessionRequest, FinishReason, HostHealth, HostStartConfig, OperationFinishedEvent,
        SessionStopReason,
    };
    use crate::core::session::HostSession;
    use crate::{HostResult, ProviderCatalog};

    struct NullHost;

    #[async_trait]
    impl HostFacade for NullHost {
        async fn start(&self, _config: HostStartConfig) -> crate::HostResult<()> {
            Ok(())
        }

        async fn create_session(
            &self,
            _request: CreateSessionRequest,
        ) -> crate::HostResult<HostSession> {
            Err(HostError::internal("not used"))
        }

        async fn exec(
            &self,
            _session_id: HostSessionId,
            _op: HostOperation,
        ) -> crate::HostResult<HostEventStream> {
            Err(HostError::internal("not used"))
        }

        async fn cancel(&self, _op_id: HostOperationId) -> crate::HostResult<()> {
            Err(HostError::internal("not used"))
        }

        async fn health(&self) -> crate::HostResult<HostHealth> {
            Err(HostError::internal("not used"))
        }

        async fn shutdown(&self) -> crate::HostResult<()> {
            Ok(())
        }

        async fn shutdown_session(&self, _session_id: HostSessionId) -> crate::HostResult<()> {
            Err(HostError::internal("not used"))
        }

        async fn list_sessions(&self) -> crate::HostResult<Vec<HostSession>> {
            Ok(Vec::new())
        }

        async fn provider_catalog(&self) -> crate::HostResult<ProviderCatalog> {
            Ok(ProviderCatalog { entries: Vec::new() })
        }

        fn subscribe_events(
            &self,
            _session_id: HostSessionId,
        ) -> tokio::sync::broadcast::Receiver<HostEvent> {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx.subscribe()
        }
    }

    fn adapter() -> ProviderPortAdapter {
        ProviderPortAdapter::new(Arc::new(NullHost))
    }

    fn terminal_event() -> HostResult<HostEvent> {
        Ok(HostEvent::OpFinished(OperationFinishedEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            reason: FinishReason::EndTurn,
        }))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_same_operation_pull_is_busy_and_flag_resets() {
        let port = adapter();
        port.insert_operation(
            "op-1".to_string(),
            Box::pin(futures_util::stream::pending::<HostResult<HostEvent>>()),
        );

        let mut first = Box::pin(port.next("op-1".to_string(), 16, 4096));
        assert!(
            futures_util::future::poll_immediate(&mut first).await.is_none(),
            "first pull must park on the pending host stream"
        );

        let second = port.next("op-1".to_string(), 16, 4096).await;
        let err = second.expect_err("second concurrent pull must be rejected");
        assert_eq!(err.code, CoreErrorCode::Busy);

        // Dropping a parked pull must release admission (guard reset path).
        drop(first);
        port.insert_operation(
            "op-2".to_string(),
            Box::pin(futures_util::stream::iter(vec![terminal_event()])),
        );
        let batch = port
            .next("op-2".to_string(), 16, 4096)
            .await
            .expect("pull after admission reset");
        assert_eq!(batch.events.len(), 1);
        assert!(!batch.has_more, "terminal batch must report no more events");

        // Terminal truth stays inspectable rather than vanishing into NotFound.
        let after_terminal = port
            .next("op-2".to_string(), 16, 4096)
            .await
            .expect("terminal operation stays inspectable");
        assert!(after_terminal.events.is_empty());
        assert!(!after_terminal.has_more);
    }

    #[tokio::test]
    async fn oversized_first_event_reports_gap_without_exceeding_cap() {
        let port = adapter();
        let big = HostEvent::Status(crate::capability::model::StatusEvent {
            session_id: None,
            level: crate::capability::model::StatusLevel::Info,
            message: "x".repeat(1024),
        });
        port.insert_operation(
            "op-big".to_string(),
            Box::pin(futures_util::stream::iter(vec![Ok(big)])),
        );

        let batch = port
            .next("op-big".to_string(), 16, 64)
            .await
            .expect("oversized pull returns a gap batch");
        assert!(batch.events.is_empty(), "oversized event must not be pushed");
        assert!(batch.has_more);
        let gap = batch.gap.expect("gap present");
        assert_eq!(gap.reason, ProviderEventBatchGapReason::Oversized);
    }

    #[tokio::test]
    async fn unknown_operation_is_not_found() {
        let port = adapter();
        let err = port
            .next("missing".to_string(), 16, 4096)
            .await
            .expect_err("unknown operation must be NotFound");
        assert_eq!(err.code, CoreErrorCode::NotFound);
    }

    #[test]
    fn protocol_kind_wire_uses_contract_snake_case() {
        assert_eq!(protocol_kind_wire(ProtocolKind::Acp), "acp");
        assert_eq!(protocol_kind_wire(ProtocolKind::NativeCli), "native_cli");
    }

    #[test]
    fn operation_status_wire_follows_session_state() {
        let op = HostOperationId::new();
        let other = HostOperationId::new();
        assert_eq!(
            operation_status_wire(&SessionState::Busy(op.clone()), &op),
            "running"
        );
        assert_eq!(
            operation_status_wire(&SessionState::Cancelling(op.clone()), &op),
            "cancelling"
        );
        assert_eq!(operation_status_wire(&SessionState::Ready, &op), "completed");
        assert_eq!(
            operation_status_wire(&SessionState::Stopped, &op),
            "completed"
        );
        assert_eq!(
            operation_status_wire(&SessionState::ErrorTerminal, &op),
            "failed"
        );
        assert_eq!(
            operation_status_wire(&SessionState::Starting, &op),
            "pending"
        );
        assert_eq!(
            operation_status_wire(&SessionState::Busy(other), &op),
            "unknown"
        );
    }

    #[test]
    fn oversized_gap_is_marked_resync_required() {
        let gap = ProviderPortAdapter::oversized_gap("op-1");
        assert!(gap.resync_required);
        assert_eq!(gap.operation_id.as_deref(), Some("op-1"));
    }

    #[tokio::test]
    async fn session_stopped_is_terminal() {
        let port = adapter();
        let stopped = HostEvent::SessionStopped(crate::capability::model::SessionStoppedEvent {
            session_id: HostSessionId::new(),
            reason: SessionStopReason::GracefulShutdown,
        });
        port.insert_operation(
            "op-3".to_string(),
            Box::pin(futures_util::stream::iter(vec![Ok(stopped)])),
        );
        let batch = port
            .next("op-3".to_string(), 16, 4096)
            .await
            .expect("terminal session pull");
        assert_eq!(batch.events.len(), 1);
        assert!(!batch.has_more);
    }
}
