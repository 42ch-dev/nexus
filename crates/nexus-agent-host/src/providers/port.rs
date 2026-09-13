//! ProviderPort adapter over existing ProviderAdapter / HostFacade.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use nexus_contracts::{
    CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderHostEvent, ProviderReply,
    generated::core::provider_event_batch::NexusProviderHostEvent,
    provider_call::ProviderCallMethod, provider_reply::ProviderReplyHealth,
};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::capability::model::{
    HostEvent, HostEventStream, HostOperation, LaunchSpec, ProbeRequest, ProviderHealth,
};
use crate::error::HostError;
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::{HostFacade, ProviderAdapter};

const MAX_EVENTS_PER_BATCH: u32 = 16;
const MAX_BYTES_PER_BATCH: u32 = 256 * 1024;

struct ActiveOperation {
    stream: HostEventStream,
    terminal_seen: bool,
}

/// Pull-based provider port backed by the existing host facade and adapters.
pub struct ProviderPortAdapter {
    host: Arc<dyn HostFacade>,
    providers: HashMap<ProviderId, Arc<dyn ProviderAdapter>>,
    operations: Mutex<HashMap<String, ActiveOperation>>,
}

impl ProviderPortAdapter {
    #[must_use]
    pub fn new(host: Arc<dyn HostFacade>) -> Self {
        Self {
            host,
            providers: HashMap::new(),
            operations: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_provider(&mut self, provider_id: ProviderId, adapter: Arc<dyn ProviderAdapter>) {
        self.providers.insert(provider_id, adapter);
    }

    fn map_host_error(err: HostError) -> CoreError {
        CoreError {
            code: CoreErrorCode::Internal,
            message: err.to_string(),
            details: Default::default(),
            http_status: Some(500),
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

    fn parse_session_id(raw: &str) -> Result<HostSessionId, CoreError> {
        let uuid = Uuid::parse_str(raw).map_err(|e| Self::invalid_input(format!("session_id: {e}")))?;
        Ok(HostSessionId(uuid))
    }

    fn parse_operation_id(raw: &str) -> Result<HostOperationId, CoreError> {
        let uuid = Uuid::parse_str(raw).map_err(|e| Self::invalid_input(format!("operation_id: {e}")))?;
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
                let probe: ProbeRequest = serde_json::from_value(payload_value).map_err(|e| {
                    Self::invalid_input(format!("probe payload: {e}"))
                })?;
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
                let adapter = self.providers.get(&provider_id).ok_or_else(|| CoreError {
                    code: CoreErrorCode::NotFound,
                    message: format!("provider {provider_id} not registered"),
                    details: Default::default(),
                    http_status: Some(404),
                })?;
                let spec: LaunchSpec = serde_json::from_value(payload_value).map_err(|e| {
                    Self::invalid_input(format!("launch payload: {e}"))
                })?;
                let handle = adapter.launch(spec).await.map_err(Self::map_host_error)?;
                Ok(ProviderReply {
                    request_id: request.request_id,
                    ok: true,
                    operation_id: None,
                    session_id: Some(handle.session_id.to_string()),
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
                let op: HostOperation = serde_json::from_value(payload_value).map_err(|e| {
                    Self::invalid_input(format!("execute payload: {e}"))
                })?;
                let op_id = match &op {
                    HostOperation::Prompt { op_id, .. } => op_id.clone(),
                    _ => HostOperationId::new(),
                };
                let stream = self
                    .host
                    .exec(session_id.clone(), op)
                    .await
                    .map_err(Self::map_host_error)?;
                self.operations.lock().await.insert(
                    op_id.to_string(),
                    ActiveOperation {
                        stream,
                        terminal_seen: false,
                    },
                );
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
        let mut ops = self.operations.lock().await;
        let active = ops.get_mut(&operation_id).ok_or_else(|| CoreError {
            code: CoreErrorCode::NotFound,
            message: format!("operation {operation_id} not found"),
            details: Default::default(),
            http_status: Some(404),
        })?;
        if active.terminal_seen {
            return Ok(ProviderEventBatch {
                operation_id,
                events: vec![],
                has_more: false,
                gap: None,
            });
        }

        let mut events = Vec::new();
        let mut used_bytes = 0u32;
        while (events.len() as u32) < cap_events && used_bytes < cap_bytes {
            match active.stream.next().await {
                Some(Ok(event)) => {
                    let wire = wire_to_batch_event(host_event_to_wire(&event)?)?;
                    let size = serde_json::to_vec(&wire).map(|v| v.len()).unwrap_or(0);
                    if used_bytes + size as u32 > cap_bytes && !events.is_empty() {
                        break;
                    }
                    used_bytes += size as u32;
                    events.push(wire);
                    if is_terminal(&event) {
                        active.terminal_seen = true;
                        break;
                    }
                }
                Some(Err(err)) => return Err(Self::map_host_error(err)),
                None => {
                    active.terminal_seen = true;
                    break;
                }
            }
        }

        let has_more = !active.terminal_seen;
        if active.terminal_seen {
            ops.remove(&operation_id);
        }

        Ok(ProviderEventBatch {
            operation_id,
            events,
            has_more,
            gap: None,
        })
    }
}
