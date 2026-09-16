//! Catalog-selected provider ports. Selection happens before effects; subsequent
//! calls use retained session/operation ownership, never caller routing hints.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};

use super::port::host_error_to_core_error;
use super::recipe_admission::reject_caller_recipe_payload;
use crate::capability::model::{HostOperation, LaunchSpec, ProbeRequest, ProtocolKind};
use crate::{HostFacade, HostManager, LaunchStrategy, ProviderId};
use crate::capability::model::ProviderHealth;

// Match the callback's retained payload window; completed routes are evicted
// before admitting further operations, not while an effect/pull is active.
const MAX_OPERATIONS_PER_SESSION: usize = 64;

struct SessionOwner {
    port: Arc<dyn ProviderPort>,
    gate: tokio::sync::Mutex<()>,
    closed: AtomicBool,
}

struct OperationOwner {
    session_id: String,
    session: Arc<SessionOwner>,
    terminal: AtomicBool,
}

#[derive(Default)]
struct Routes {
    sessions: HashMap<String, Arc<SessionOwner>>,
    operations: HashMap<String, Arc<OperationOwner>>,
    launching: usize,
    executing: HashSet<String>,
}

struct LaunchReservation<'a>(&'a Mutex<Routes>);

impl Drop for LaunchReservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut routes) = self.0.lock() {
            routes.launching -= 1;
        }
    }
}

struct OperationReservation<'a> {
    routes: &'a Mutex<Routes>,
    id: String,
}

impl Drop for OperationReservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut routes) = self.routes.lock() {
            routes.executing.remove(&self.id);
        }
    }
}

struct MultiplexProviderPort {
    host: Arc<HostManager>,
    native: Arc<dyn ProviderPort>,
    acp: Option<Arc<dyn ProviderPort>>,
    routes: Mutex<Routes>,
}

/// Compose maintained native adapters with an optional ACP callback port.
///
/// The callback receives only Rust-admitted recipes. Any tracking/journal
/// wrapper around it must consume that recipe, not re-admit caller payload.
/// No provider effect is used to discover the selected protocol.
#[must_use]
pub fn compose_provider_port(
    host: Arc<HostManager>,
    native: Arc<dyn ProviderPort>,
    acp: Option<Arc<dyn ProviderPort>>,
) -> Arc<dyn ProviderPort> {
    Arc::new(MultiplexProviderPort {
        host,
        native,
        acp,
        routes: Mutex::new(Routes::default()),
    })
}

fn error(code: CoreErrorCode, message: impl Into<String>, status: i64) -> CoreError {
    CoreError {
        code,
        message: message.into(),
        details: Default::default(),
        http_status: Some(status),
    }
}

impl MultiplexProviderPort {
    fn routes(&self) -> ProviderResult<std::sync::MutexGuard<'_, Routes>> {
        self.routes.lock().map_err(|_| {
            error(CoreErrorCode::Internal, "provider ownership lock poisoned", 500)
        })
    }

    async fn select(&self, request: &mut ProviderCall) -> ProviderResult<Arc<dyn ProviderPort>> {
        let provider_id = request.payload.get("provider_id").and_then(|id| id.as_str())
            .ok_or_else(|| error(CoreErrorCode::InvalidInput, "provider_id required", 400))?;
        let provider_id = ProviderId::new(provider_id);
        let catalog = self.host.provider_catalog().await.map_err(|e| host_error_to_core_error(&e))?;
        let entry = catalog.find(&provider_id).ok_or_else(|| {
            error(CoreErrorCode::NotFound, "provider not in admitted catalog", 404)
        })?;
        if !matches!(
            (&entry.protocol_kind, &entry.launch),
            (ProtocolKind::Acp, LaunchStrategy::Acp { .. })
                | (ProtocolKind::NativeCli, LaunchStrategy::NativeCli { .. })
        ) {
            return Err(error(CoreErrorCode::InvalidInput, "catalog protocol/launch mismatch", 400));
        }
        if entry.protocol_kind != ProtocolKind::Acp || self.acp.is_none() {
            return Ok(self.native.clone());
        }
        if request.method == ProviderCallMethod::Launch && !entry.health.available {
            return Err(error(CoreErrorCode::NotFound, "provider is not ready", 404));
        }
        let mut recipe = self.host.admit_validated_provider_recipe(&provider_id).await
            .map_err(|e| host_error_to_core_error(&e))?;
        // Both the host boundary and the verified Creator owner constrain cwd.
        let payload = serde_json::Value::Object(request.payload.clone());
        let (cwd, owner) = if request.method == ProviderCallMethod::Launch {
            let spec: LaunchSpec = serde_json::from_value(payload).map_err(|e| {
                error(CoreErrorCode::InvalidInput, format!("launch payload: {e}"), 400)
            })?;
            (spec.cwd, spec.owner)
        } else {
            let probe: ProbeRequest = serde_json::from_value(payload).map_err(|e| {
                error(CoreErrorCode::InvalidInput, format!("probe payload: {e}"), 400)
            })?;
            (probe.cwd, probe.owner)
        };
        let boundary = std::path::Path::new(&recipe.cwd);
        let cwd = crate::config::validate_workspace_path_under(&cwd, boundary)
            .map_err(|e| host_error_to_core_error(&e))?;
        let owner_root = crate::config::validate_workspace_path_under(&owner.workspace_root, boundary)
            .map_err(|e| host_error_to_core_error(&e))?;
        if !cwd.starts_with(owner_root) {
            return Err(error(CoreErrorCode::Forbidden, "owner workspace mismatch", 403));
        }
        recipe.cwd = cwd.to_string_lossy().into_owned();
        request.payload.insert("recipe".into(), serde_json::to_value(recipe).map_err(|e| {
            error(CoreErrorCode::Internal, format!("recipe serialization: {e}"), 500)
        })?);
        self.acp.clone().ok_or_else(|| error(CoreErrorCode::Internal, "ACP port missing", 500))
    }

    async fn probe(&self, mut request: ProviderCall) -> ProviderResult<ProviderReply> {
        let port = self.select(&mut request).await?;
        let provider_id = request.payload.get("provider_id").and_then(|id| id.as_str())
            .ok_or_else(|| error(CoreErrorCode::InvalidInput, "provider_id required", 400))?.to_string();
        let reply = port.call(request).await?;
        if reply.ok {
            if let Some(health) = &reply.health {
                if health.provider_id != provider_id {
                    return Err(error(CoreErrorCode::Internal, "probe provider identity mismatch", 500));
                }
                let id = ProviderId::new(provider_id);
                self.host.record_provider_probe(&id, ProviderHealth {
                    provider_id: id.clone(), available: health.available,
                    latency_ms: health.latency_ms, message: health.message.clone(),
                }, health.latency_ms.unwrap_or(0)).await;
            }
        }
        Ok(reply)
    }

    fn session(&self, id: Option<&str>) -> ProviderResult<(String, Arc<SessionOwner>)> {
        let id = id.ok_or_else(|| error(CoreErrorCode::InvalidInput, "session_id required", 400))?;
        let owner = self.routes()?.sessions.get(id).cloned().ok_or_else(|| {
            error(CoreErrorCode::NotFound, "session owner not found", 404)
        })?;
        Ok((id.to_string(), owner))
    }

    fn operation(&self, id: &str) -> ProviderResult<Arc<OperationOwner>> {
        self.routes()?.operations.get(id).cloned().ok_or_else(|| {
            error(CoreErrorCode::NotFound, "operation owner not found", 404)
        })
    }

    async fn launch(&self, mut request: ProviderCall) -> ProviderResult<ProviderReply> {
        // Session identity is provider-owned, never a caller-selected existing ID.
        if request.session_id.is_some() || request.operation_id.is_some() {
            return Err(error(CoreErrorCode::InvalidInput, "launch cannot supply session/operation ID", 400));
        }
        let port = self.select(&mut request).await?;
        let max_sessions = self.host.agent_config().await.max_sessions;
        {
            let mut routes = self.routes()?;
            if routes.sessions.len() + routes.launching >= max_sessions {
                return Err(error(CoreErrorCode::Busy, "provider session limit reached", 503));
            }
            routes.launching += 1;
        }
        let _reservation = LaunchReservation(&self.routes);
        let reply = port.call(request).await?;
        if !reply.ok { return Ok(reply); }
        let session_id = reply.session_id.as_ref().ok_or_else(|| {
            error(CoreErrorCode::Internal, "launch succeeded without session_id", 500)
        })?;
        let mut routes = self.routes()?;
        if routes.sessions.contains_key(session_id) {
            // Do not replace the incumbent route or try a second adapter. The
            // selected adapter/tracking wrapper retains cleanup ownership.
            return Err(error(CoreErrorCode::Internal, "provider returned an owned session_id", 500));
        }
        routes.sessions.insert(session_id.clone(), Arc::new(SessionOwner {
            port,
            gate: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(false),
        }));
        Ok(reply)
    }

    async fn execute(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        let (session_id, session) = self.session(request.session_id.as_deref())?;
        let _gate = session.gate.try_lock().map_err(|_| error(CoreErrorCode::Busy, "session effect in flight", 503))?;
        if session.closed.load(Ordering::Acquire) {
            return Err(error(CoreErrorCode::NotFound, "session closed", 404));
        }
        let operation: HostOperation = serde_json::from_value(serde_json::Value::Object(request.payload.clone()))
            .map_err(|e| error(CoreErrorCode::InvalidInput, format!("operation payload: {e}"), 400))?;
        let reserved_id = match operation {
            HostOperation::Prompt { op_id, .. } => Some(op_id.to_string()),
            HostOperation::SetModel { .. } | HostOperation::SetMode { .. } => None,
        };
        let _reservation = {
            let mut routes = self.routes()?;
            if let Some(op_id) = &reserved_id {
                if routes.operations.contains_key(op_id) || routes.executing.contains(op_id) {
                    return Err(error(CoreErrorCode::InvalidInput, "operation_id already owned", 400));
                }
            }
            if routes.operations.values().any(|op| op.session_id == session_id && !op.terminal.load(Ordering::Acquire)) {
                return Err(error(CoreErrorCode::Busy, "session operation not yet drained", 503));
            }
            let count = routes.operations.values().filter(|op| op.session_id == session_id).count();
            if count >= MAX_OPERATIONS_PER_SESSION {
                routes.operations.retain(|_, op| op.session_id != session_id || !op.terminal.load(Ordering::Acquire));
                if routes.operations.values().filter(|op| op.session_id == session_id).count() >= MAX_OPERATIONS_PER_SESSION {
                    return Err(error(CoreErrorCode::Busy, "operation ownership window full", 503));
                }
            }
            reserved_id.map(|id| {
                routes.executing.insert(id.clone());
                OperationReservation { routes: &self.routes, id }
            })
        };
        let reply = session.port.call(request).await?;
        if !reply.ok { return Ok(reply); }
        let operation_id = reply.operation_id.as_ref().ok_or_else(|| {
            error(CoreErrorCode::Internal, "execute succeeded without operation_id", 500)
        })?;
        let mut routes = self.routes()?;
        if routes.operations.contains_key(operation_id) {
            return Err(error(CoreErrorCode::Internal, "provider returned an owned operation_id", 500));
        }
        routes.operations.insert(operation_id.clone(), Arc::new(OperationOwner {
            session_id,
            session: session.clone(),
            terminal: AtomicBool::new(false),
        }));
        Ok(reply)
    }
}

#[async_trait]
impl ProviderPort for MultiplexProviderPort {
    async fn call(&self, mut request: ProviderCall) -> ProviderResult<ProviderReply> {
        reject_caller_recipe_payload(&request.payload).map_err(|e| host_error_to_core_error(&e))?;
        match request.method {
            ProviderCallMethod::Probe => self.probe(request).await,
            ProviderCallMethod::Launch => self.launch(request).await,
            ProviderCallMethod::Execute => self.execute(request).await,
            ProviderCallMethod::Cancel => {
                let id = request.operation_id.as_deref().ok_or_else(|| {
                    error(CoreErrorCode::InvalidInput, "operation_id required", 400)
                })?;
                let op = self.operation(id)?;
                if request.session_id.as_ref().is_some_and(|id| id != &op.session_id) {
                    return Err(error(CoreErrorCode::InvalidInput, "operation/session mismatch", 400));
                }
                request.session_id = Some(op.session_id.clone());
                op.session.port.call(request).await
            }
            ProviderCallMethod::Shutdown => {
                let (id, session) = self.session(request.session_id.as_deref())?;
                let _gate = session.gate.try_lock().map_err(|_| error(CoreErrorCode::Busy, "session effect in flight", 503))?;
                let reply = session.port.call(request).await?;
                if reply.ok {
                    session.closed.store(true, Ordering::Release);
                    let mut routes = self.routes()?;
                    routes.sessions.remove(&id);
                    routes.operations.retain(|_, op| op.session_id != id);
                }
                Ok(reply)
            }
        }
    }

    async fn next(&self, operation_id: String, max_events: u32, max_bytes: u32) -> ProviderResult<ProviderEventBatch> {
        let op = self.operation(&operation_id)?;
        let batch = op.session.port.next(operation_id, max_events, max_bytes).await?;
        if !batch.has_more && batch.gap.is_none() {
            op.terminal.store(true, Ordering::Release);
        }
        Ok(batch)
    }
}
