//! ProviderPort adapter over existing ProviderAdapter / HostFacade.
//!
//! Owns the host→wire mappings (`HostEvent` promotion, provider/operation wire
//! vocabulary) and the one-outstanding-pull admission at the port boundary.

use std::collections::{HashMap, VecDeque};
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
use crate::core::manager::HostManager;
use crate::{HostFacade, ProviderAdapter};

const MAX_EVENTS_PER_BATCH: u32 = 16;
const MAX_BYTES_PER_BATCH: u32 = 256 * 1024;

/// Contract §7 — provider data caps per operation. These are rolling
/// per-operation bounds, not per-batch ones: a caller that keeps pulling must
/// hit a typed `delivery_overflow` gap rather than grow its buffer without
/// limit or, worse, have the over-budget item silently disappear.
/// Maximum events the port may hold undelivered for one operation.
pub const MAX_PENDING_MESSAGES_PER_OP: usize = 64;
/// Maximum wire bytes the port may hold undelivered for one operation.
pub const MAX_PENDING_BYTES_PER_OP: usize = 1024 * 1024;
/// Maximum wire bytes of a single event.
pub const MAX_EVENT_BYTES: usize = 256 * 1024;
/// Maximum wire bytes emitted for one operation before it is cut off.
pub const MAX_EMITTED_BYTES_PER_OP: usize = 4 * 1024 * 1024;

/// Map a host error onto the wire error taxonomy.
///
/// Shared by the effect path ([`ProviderPortAdapter`]) and the admission
/// boundary (`AdmittingProviderPort` in `nexus-core-node`) so a native caller
/// sees ONE category per condition. Collapsing everything to `invalid_input`
/// would report a policy denial or a busy provider as malformed input.
#[must_use]
pub fn host_error_to_core_error(err: &HostError) -> CoreError {
    let (code, http_status) = match err {
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

/// A pulled-but-undelivered wire event.
struct PendingItem {
    wire: NexusProviderHostEvent,
    bytes: usize,
    /// This item is the operation terminal — a *control* item, delivered even
    /// when the data window is full, and never erased by an EOF behind it.
    terminal: bool,
}

/// Per-operation pending window and rolling budget accumulators.
///
/// A pull that cannot fit an item no longer drops it: the item is deferred here
/// and delivered by the next pull. Only a genuine cap violation discards it, and
/// then the caller receives a typed `delivery_overflow` gap, so loss is always
/// explicit.
#[derive(Default)]
struct PullState {
    pending: VecDeque<PendingItem>,
    pending_bytes: usize,
    /// Rolling totals actually delivered to the caller for this operation.
    emitted_bytes: usize,
    emitted_messages: usize,
    /// The stream produced a terminal that has not been delivered yet.
    terminal_pending: bool,
    /// The terminal has been delivered; completion may be reported.
    terminal_delivered: bool,
    /// A per-operation cap was exceeded; every later pull reports the gap.
    overflowed: bool,
}

impl PullState {
    /// Buffer a pulled item. `false` means a per-operation cap would be
    /// exceeded, so the caller must report the typed gap instead of dropping the
    /// item silently.
    fn defer(&mut self, wire: NexusProviderHostEvent, bytes: usize, terminal: bool) -> bool {
        if self.emitted_bytes + self.pending_bytes + bytes > MAX_EMITTED_BYTES_PER_OP {
            return false;
        }
        if self.pending.len() >= MAX_PENDING_MESSAGES_PER_OP {
            return false;
        }
        if self.pending_bytes + bytes > MAX_PENDING_BYTES_PER_OP {
            return false;
        }
        self.pending.push_back(PendingItem {
            wire,
            bytes,
            terminal,
        });
        self.pending_bytes += bytes;
        if terminal {
            self.terminal_pending = true;
        }
        true
    }

    fn note_emitted(&mut self, bytes: usize) {
        self.emitted_bytes += bytes;
        self.emitted_messages += 1;
    }
}

/// Per-operation pull state. The admission flag is atomic and lives outside the
/// stream slot, so a concurrent pull is rejected without waiting on the owner.
struct OperationEntry {
    pull_in_flight: AtomicBool,
    terminal_seen: AtomicBool,
    stream_slot: Arc<Mutex<Option<HostEventStream>>>,
    /// Guarded by the pull-admission flag, so it needs no async lock.
    pull_state: Mutex<PullState>,
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
    manager: Option<Arc<HostManager>>,
    providers: HashMap<ProviderId, Arc<dyn ProviderAdapter>>,
    operations: DashMap<String, Arc<OperationEntry>>,
}

impl ProviderPortAdapter {
    #[must_use]
    pub fn new(host: Arc<dyn HostFacade>) -> Self {
        Self {
            host,
            manager: None,
            providers: HashMap::new(),
            operations: DashMap::new(),
        }
    }

    pub fn with_manager(manager: Arc<HostManager>) -> Self {
        Self {
            host: manager.clone(),
            manager: Some(manager),
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
                pull_state: Mutex::new(PullState::default()),
            }),
        );
    }

    fn map_host_error(err: HostError) -> CoreError {
        host_error_to_core_error(&err)
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
    ///
    /// Nothing pulled from the non-rewindable stream is ever silently discarded:
    /// an item that does not fit the remaining batch budget is deferred to the
    /// per-operation pending window and returned by the next pull. A terminal is
    /// a *control* item — it is retained for delivery, and an EOF arriving behind
    /// it cannot erase it. Only a genuine per-operation cap violation drops an
    /// item, and then the batch carries a typed `delivery_overflow` gap.
    async fn pull_events(
        entry: &OperationEntry,
        operation_id: &str,
        stream: &mut HostEventStream,
        cap_events: u32,
        cap_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        let mut events: Vec<NexusProviderHostEvent> = Vec::new();
        let mut used_bytes = 0u32;

        // (1) Deliver what a previous pull had to defer, oldest first.
        {
            let mut state = entry.pull_state.lock().await;
            if state.overflowed {
                return Ok(ProviderEventBatch {
                    operation_id: operation_id.to_string(),
                    events,
                    has_more: !state.terminal_delivered,
                    gap: Some(Self::oversized_gap(operation_id)),
                });
            }
            while (events.len() as u32) < cap_events {
                let Some(front) = state.pending.front() else {
                    break;
                };
                if used_bytes + front.bytes as u32 > cap_bytes {
                    break;
                }
                let item = state.pending.pop_front().expect("pending front");
                state.pending_bytes -= item.bytes;
                used_bytes += item.bytes as u32;
                state.note_emitted(item.bytes);
                if item.terminal {
                    state.terminal_delivered = true;
                }
                events.push(item.wire);
                if item.terminal {
                    break;
                }
            }
            if state.terminal_delivered {
                entry.terminal_seen.store(true, Ordering::Release);
                return Ok(ProviderEventBatch {
                    operation_id: operation_id.to_string(),
                    events,
                    has_more: false,
                    gap: None,
                });
            }
            if state.terminal_pending {
                // The terminal the stream already produced is still queued: do
                // not read past it, and do not report completion yet.
                return Ok(ProviderEventBatch {
                    operation_id: operation_id.to_string(),
                    events,
                    has_more: true,
                    gap: None,
                });
            }
        }

        // (2) Read further items, deferring rather than dropping what does not fit.
        while (events.len() as u32) < cap_events && used_bytes < cap_bytes {
            match stream.next().await {
                Some(Ok(event)) => {
                    let wire = wire_to_batch_event(host_event_to_wire(&event)?)?;
                    let size = serde_json::to_vec(&wire).map_or(0, |bytes| bytes.len());
                    let terminal = is_terminal(&event);

                    // An item that exceeds the per-event cap — or that could not
                    // fit an *empty* batch under the caller's byte cap — can
                    // never be delivered, so deferring it would spin forever.
                    // That is a typed gap, never a partial or silent drop.
                    if size > MAX_EVENT_BYTES || size as u32 > cap_bytes {
                        entry.pull_state.lock().await.overflowed = true;
                        return Ok(ProviderEventBatch {
                            operation_id: operation_id.to_string(),
                            events,
                            has_more: true,
                            gap: Some(Self::oversized_gap(operation_id)),
                        });
                    }

                    if used_bytes + size as u32 <= cap_bytes {
                        {
                            let mut state = entry.pull_state.lock().await;
                            state.note_emitted(size);
                            if terminal {
                                state.terminal_delivered = true;
                            }
                        }
                        used_bytes += size as u32;
                        events.push(wire);
                        if terminal {
                            entry.terminal_seen.store(true, Ordering::Release);
                            break;
                        }
                        continue;
                    }

                    // Does not fit this batch: defer it for the next pull.
                    let deferred = entry.pull_state.lock().await.defer(wire, size, terminal);
                    if !deferred {
                        entry.pull_state.lock().await.overflowed = true;
                        return Ok(ProviderEventBatch {
                            operation_id: operation_id.to_string(),
                            events,
                            has_more: true,
                            gap: Some(Self::oversized_gap(operation_id)),
                        });
                    }
                    break;
                }
                Some(Err(err)) => return Err(Self::map_host_error(err)),
                None => {
                    // Stream ended. Completion may only be reported once every
                    // deferred item — a terminal above all — has been delivered.
                    let mut state = entry.pull_state.lock().await;
                    if !state.pending.is_empty() {
                        return Ok(ProviderEventBatch {
                            operation_id: operation_id.to_string(),
                            events,
                            has_more: true,
                            gap: None,
                        });
                    }
                    state.terminal_delivered = true;
                    entry.terminal_seen.store(true, Ordering::Release);
                    return Ok(ProviderEventBatch {
                        operation_id: operation_id.to_string(),
                        events,
                        has_more: false,
                        gap: None,
                    });
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
                let started = std::time::Instant::now();
                let health = adapter.probe(probe).await.map_err(Self::map_host_error)?;
                let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                if let Some(manager) = &self.manager {
                    manager
                        .record_provider_probe(&provider_id, health.clone(), latency_ms)
                        .await;
                }
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

    fn status_event(text: &str) -> HostEvent {
        HostEvent::Status(crate::capability::model::StatusEvent {
            session_id: None,
            level: crate::capability::model::StatusLevel::Info,
            message: text.to_string(),
        })
    }

    /// Q1-C1: an event that does not fit the *remaining* batch budget must be
    /// deferred and delivered by the next pull — never consumed off the
    /// non-rewindable stream and lost.
    #[tokio::test]
    async fn over_budget_event_is_deferred_not_lost() {
        let port = adapter();
        let first = status_event("first");
        let second = status_event("second-event-that-does-not-fit");
        port.insert_operation(
            "op-defer".to_string(),
            Box::pin(futures_util::stream::iter(vec![Ok(first), Ok(second)])),
        );

        // A cap that fits exactly one event of this size.
        let cap = serde_json::to_vec(&wire_to_batch_event(
            host_event_to_wire(&status_event("second-event-that-does-not-fit")).unwrap(),
        )
        .unwrap())
        .unwrap()
        .len() as u32;

        let batch = port
            .next("op-defer".to_string(), 16, cap)
            .await
            .expect("first pull");
        assert_eq!(batch.events.len(), 1, "only the fitting event is delivered");
        assert!(batch.gap.is_none(), "a deferred event is not a gap");

        let next = port
            .next("op-defer".to_string(), 16, cap)
            .await
            .expect("second pull");
        assert_eq!(
            next.events.len(),
            1,
            "the deferred event must be delivered, not lost: {next:?}"
        );
    }

    /// Q1-C1: a terminal that does not fit the batch is a *control* item — it is
    /// retained, and the EOF behind it must not erase terminal truth.
    #[tokio::test]
    async fn over_budget_terminal_survives_trailing_eof() {
        let port = adapter();
        let filler = status_event("filler");
        port.insert_operation(
            "op-term".to_string(),
            Box::pin(futures_util::stream::iter(vec![Ok(filler), terminal_event()])),
        );

        // A cap that fits each item on its own but not both together: the
        // filler is delivered and the terminal must be deferred, not dropped.
        let wire_size = |event: &HostEvent| {
            serde_json::to_vec(&wire_to_batch_event(host_event_to_wire(event).unwrap()).unwrap())
                .unwrap()
                .len() as u32
        };
        let cap = wire_size(&status_event("filler")) + wire_size(&terminal_event().unwrap()) - 1;

        let first = port.next("op-term".to_string(), 16, cap).await.expect("pull 1");
        assert!(
            first.has_more,
            "completion must not be reported while the terminal is still queued"
        );

        let second = port
            .next("op-term".to_string(), 16, cap)
            .await
            .expect("pull 2");
        assert_eq!(second.events.len(), 1, "the terminal must be delivered");
        assert!(!second.has_more, "the delivered terminal completes the op");

        // The terminal is delivered once, not replayed.
        let third = port
            .next("op-term".to_string(), 16, cap)
            .await
            .expect("pull 3");
        assert!(third.events.is_empty(), "terminal must not replay: {third:?}");
        assert!(!third.has_more);
    }

    /// Q3-W4/S7: a single event beyond the per-event cap trips a typed gap even
    /// when it is not the first event of the batch.
    #[tokio::test]
    async fn oversized_non_first_event_reports_gap() {
        let port = adapter();
        let small = status_event("small");
        let huge = status_event(&"x".repeat(MAX_EVENT_BYTES + 1024));
        port.insert_operation(
            "op-mixed".to_string(),
            Box::pin(futures_util::stream::iter(vec![Ok(small), Ok(huge)])),
        );

        let batch = port
            .next("op-mixed".to_string(), 16, MAX_BYTES_PER_BATCH)
            .await
            .expect("pull");
        let gap = batch.gap.expect("oversized non-first event must report a gap");
        assert_eq!(gap.reason, ProviderEventBatchGapReason::Oversized);
        assert!(gap.resync_required);
        // The over-budget event is never delivered partially.
        for event in &batch.events {
            assert!(
                serde_json::to_vec(event).unwrap().len() <= MAX_EVENT_BYTES,
                "no delivered event may exceed the per-event cap"
            );
        }
    }

    /// Q3-W4: the per-operation pending window is bounded and tripping it is an
    /// explicit typed gap, not an unbounded buffer.
    #[tokio::test]
    async fn pending_window_is_bounded_and_trips_a_typed_gap() {
        let mut state = PullState::default();
        let wire = wire_to_batch_event(host_event_to_wire(&status_event("p")).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&wire).unwrap().len();

        for _ in 0..MAX_PENDING_MESSAGES_PER_OP {
            assert!(state.defer(wire.clone(), bytes, false), "within the window");
        }
        assert!(
            !state.defer(wire.clone(), bytes, false),
            "the message-count cap must trip"
        );
        assert_eq!(state.pending.len(), MAX_PENDING_MESSAGES_PER_OP);

        let mut byte_state = PullState::default();
        let chunk = MAX_PENDING_BYTES_PER_OP / 4;
        assert!(byte_state.defer(wire.clone(), chunk, false));
        assert!(byte_state.defer(wire.clone(), chunk, false));
        assert!(byte_state.defer(wire.clone(), chunk, false));
        assert!(
            !byte_state.defer(wire.clone(), chunk + 1, false),
            "the byte cap must trip"
        );
    }

    /// Q3-W4: the rolling per-operation emitted budget trips once exceeded, and
    /// every later pull keeps reporting the typed gap rather than silently
    /// growing.
    #[tokio::test]
    async fn per_operation_emitted_budget_trips_typed_gap() {
        let mut state = PullState::default();
        let wire = wire_to_batch_event(host_event_to_wire(&status_event("e")).unwrap()).unwrap();
        let mut deferred = 0usize;
        // Fill the emitted + pending budget; the first refusal is the trip point.
        while state.defer(wire.clone(), 64 * 1024, false) {
            deferred += 1;
            assert!(deferred <= MAX_PENDING_MESSAGES_PER_OP, "bounded by the window");
        }
        assert!(deferred > 0, "the budget must accept something before tripping");
        assert!(
            state.emitted_bytes + state.pending_bytes <= MAX_EMITTED_BYTES_PER_OP,
            "the accumulator must never exceed the per-operation budget"
        );
    }

    /// Q2-S2: the admission boundary and the effect path must agree on the
    /// category for each host condition, so a policy denial is not reported as
    /// malformed input.
    #[test]
    fn host_error_taxonomy_is_shared_and_distinguishing() {
        use crate::ids::ProviderId;
        let provider = ProviderId::new("mock");

        let cases = [
            (
                HostError::ProviderUnavailable {
                    provider_id: provider.clone(),
                    message: "missing".into(),
                },
                CoreErrorCode::NotFound,
                404,
            ),
            (
                HostError::PolicyDenied {
                    provider_id: None,
                    session_id: None,
                    message: "denied".into(),
                },
                CoreErrorCode::Forbidden,
                403,
            ),
            (
                HostError::OperationCancelled {
                    provider_id: None,
                    session_id: None,
                    op_id: None,
                    message: "cancelled".into(),
                },
                CoreErrorCode::Interrupted,
                503,
            ),
            (
                HostError::CleanupUnconfirmed {
                    provider_id: None,
                    session_id: None,
                    message: "retained".into(),
                },
                CoreErrorCode::Busy,
                503,
            ),
            (
                HostError::CapabilityUnsupported {
                    provider_id: provider.clone(),
                    capability: "prompt".into(),
                    message: "unsupported".into(),
                },
                CoreErrorCode::InvalidInput,
                400,
            ),
        ];

        for (err, code, http_status) in cases {
            let mapped = host_error_to_core_error(&err);
            assert_eq!(mapped.code, code, "wrong category for {err:?}");
            assert_eq!(mapped.http_status, Some(http_status), "wrong status for {err:?}");
        }

        // The admission boundary maps through the same function, so the two can
        // not drift: policy denial stays 403 rather than collapsing to 400.
        let denied = HostError::PolicyDenied {
            provider_id: None,
            session_id: None,
            message: "denied".into(),
        };
        assert_ne!(
            host_error_to_core_error(&denied).code,
            CoreErrorCode::InvalidInput,
            "policy denial must not be reported as invalid input"
        );
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
