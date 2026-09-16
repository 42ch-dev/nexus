//! Wraps a JS [`ProviderPort`] and injects Rust-admitted recipes for probe/launch.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nexus_agent_host::providers::recipe_admission::reject_caller_recipe_payload;
use nexus_agent_host::HostManager;
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};

use crate::env_state::{JsOperationStatus, JS_PROVIDER_MAX_ACTIVE_SESSIONS};

/// Reuse the effect-path taxonomy so a native caller sees one category per
/// condition at both the admission boundary and the effect path. Mapping every
/// host error to `invalid_input` reported policy denials and busy providers as
/// malformed input.
fn map_host_error(err: nexus_agent_host::error::HostError) -> CoreError {
    nexus_agent_host::providers::port::host_error_to_core_error(&err)
}

/// A durable-journal failure is a typed internal error at the provider
/// boundary: the effect's required write-through (LIFE-3) did not land, so
/// success is never reported without its durable mirror.
fn journal_failure(err: CoreError) -> CoreError {
    CoreError {
        code: CoreErrorCode::Internal,
        message: format!("durable js-provider journal write failed: {}", err.message),
        details: Default::default(),
        http_status: Some(500),
    }
}

/// An effect-committed failure: the provider side effect (operation installed
/// / cancelled / session reaped) already succeeded when the required durable
/// journal write failed. This is NOT a rollback and NOT retryable: the reply
/// is a typed 500 that names the affected operation/session so callers can
/// inspect and clean up deterministically, and EnvState is kept aligned with
/// the actual provider effect (see the per-method call sites).
fn effect_committed_failure(err: CoreError, operation_id: &str, session_id: &str) -> CoreError {
    let mut details = serde_json::Map::new();
    details.insert("effect_committed".into(), serde_json::Value::Bool(true));
    if !operation_id.is_empty() {
        details.insert(
            "operation_id".into(),
            serde_json::Value::String(operation_id.into()),
        );
    }
    if !session_id.is_empty() {
        details.insert(
            "session_id".into(),
            serde_json::Value::String(session_id.into()),
        );
    }
    CoreError {
        code: CoreErrorCode::Internal,
        message: format!(
            "durable js-provider journal write failed after the provider effect committed; the effect is not rolled back and the call is not retryable; inspect/cleanup operation_id={operation_id:?} session_id={session_id:?}: {}",
            err.message
        ),
        details,
        http_status: Some(500),
    }
}

fn provider_id_from_payload(
    payload: &serde_json::Map<String, serde_json::Value>,
) -> ProviderResult<String> {
    payload
        .get("provider_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| CoreError {
            code: CoreErrorCode::InvalidInput,
            message: "provider_id required".into(),
            details: Default::default(),
            http_status: Some(400),
        })
}

/// A provider batch whose terminal was consumed from the provider but whose
/// durable journal write failed. The real adapter dequeues terminal delivery
/// and acknowledges it before `next` returns, so the provider cannot replay
/// this batch: it is retained here and re-delivered exactly once after the
/// journal retry succeeds.
struct PendingTerminalBatch {
    batch: ProviderEventBatch,
    status: JsOperationStatus,
    session_id: String,
}

pub struct AdmittingProviderPort {
    host: Arc<HostManager>,
    inner: Arc<dyn ProviderPort>,
    state: Arc<super::env_state::EnvState>,
    /// Consumed-but-unjournaled terminal batches, keyed by operation id.
    /// Bounded by `JS_PROVIDER_MAX_ACTIVE_SESSIONS`: a pending batch exists
    /// only for an operation still active in memory, and every session holds
    /// at most one active operation.
    pending_terminal_batches: Mutex<HashMap<String, PendingTerminalBatch>>,
}

impl AdmittingProviderPort {
    pub fn new(
        host: Arc<HostManager>,
        inner: Arc<dyn ProviderPort>,
        state: Arc<super::env_state::EnvState>,
    ) -> Self {
        Self {
            host,
            inner,
            state,
            pending_terminal_batches: Mutex::new(HashMap::new()),
        }
    }

    /// Admit the recipe and return the request-local provider id, so concurrent
    /// probe/launch calls can never cross-assign provider ownership.
    async fn admit_probe_or_launch(&self, request: &mut ProviderCall) -> ProviderResult<String> {
        reject_caller_recipe_payload(&request.payload).map_err(map_host_error)?;
        let provider_id_raw = provider_id_from_payload(&request.payload)?;
        let provider_id = nexus_agent_host::ids::ProviderId::new(provider_id_raw.clone());
        let admitted = self
            .host
            .admit_validated_provider_recipe(&provider_id)
            .await
            .map_err(map_host_error)?;
        let recipe_value = serde_json::to_value(&admitted).map_err(|e| CoreError {
            code: CoreErrorCode::Internal,
            message: format!("recipe_admission_serialize: {e}"),
            details: Default::default(),
            http_status: Some(500),
        })?;
        request.payload.insert("recipe".to_string(), recipe_value);
        Ok(provider_id_raw)
    }

    /// Retain a consumed provider terminal batch for exactly-one re-delivery
    /// after the journal retry succeeds. The bound mirrors the active
    /// operation cap: at most one pending batch per active operation, at most
    /// one active operation per session, at most
    /// `JS_PROVIDER_MAX_ACTIVE_SESSIONS` sessions. If the bound were somehow
    /// exceeded the batch is dropped rather than growing unbounded — the
    /// typed journal failure still reports the terminal loss.
    fn retain_pending_terminal_batch(&self, operation_id: &str, pending: PendingTerminalBatch) {
        if let Ok(mut pending_batches) = self.pending_terminal_batches.lock() {
            if pending_batches.len() < JS_PROVIDER_MAX_ACTIVE_SESSIONS
                || pending_batches.contains_key(operation_id)
            {
                pending_batches.insert(operation_id.to_string(), pending);
            }
        }
    }
}

#[async_trait]
impl ProviderPort for AdmittingProviderPort {
    async fn call(&self, mut request: ProviderCall) -> ProviderResult<ProviderReply> {
        let method = request.method;
        let session_id = request.session_id.clone();
        // A cancel call may carry only the operation id; capture it before the
        // request is consumed so the settle path can resolve the owning session.
        let mut request_operation_id = request.operation_id.clone();
        // Reserve a bounded tracking slot *before* any admission or dispatch for
        // a launch. If the cap is reached, refuse with a typed Busy before
        // touching the adapter, so a launched child is never dropped untracked.
        let reserved = if matches!(method, ProviderCallMethod::Launch) {
            if !self.state.try_reserve_js_session() {
                return Err(CoreError {
                    code: CoreErrorCode::Busy,
                    message: "too many active JS provider sessions".into(),
                    details: Default::default(),
                    http_status: Some(503),
                });
            }
            true
        } else {
            false
        };
        // The admitted provider id is request-local: never a shared last-value
        // slot, so concurrent launches cannot cross-assign ownership.
        let admitted_provider_id = if matches!(
            method,
            ProviderCallMethod::Probe | ProviderCallMethod::Launch
        ) {
            match self.admit_probe_or_launch(&mut request).await {
                Ok(id) => id,
                Err(err) => {
                    if reserved {
                        self.state.release_js_session_reservation();
                    }
                    return Err(err);
                }
            }
        } else {
            String::new()
        };
        let reply = match self.inner.call(request).await {
            Ok(reply) => reply,
            Err(err) => {
                if reserved {
                    self.state.release_js_session_reservation();
                }
                return Err(err);
            }
        };
        // The JS adapter owns its ACP children; native close must be able to
        // cancel and release them, so track live JS sessions at the one place
        // that observes every launch/execute/shutdown reply. The same state backs
        // `hostQuery`, so it records the request-local admitted provider id.
        match method {
            ProviderCallMethod::Launch => {
                if reply.ok {
                    if let Some(session_id) = reply.session_id.clone().or(session_id) {
                        self.state
                            .record_js_session(session_id, admitted_provider_id);
                    } else if reserved {
                        self.state.release_js_session_reservation();
                    }
                } else if reserved {
                    self.state.release_js_session_reservation();
                }
            }
            ProviderCallMethod::Execute if reply.ok => {
                if let (Some(session_id), Some(operation_id)) =
                    (session_id.as_deref(), reply.operation_id.clone())
                {
                    // Durable write-through FIRST (LIFE-3) on the happy path:
                    // an op still active at process exit is settled to
                    // `interrupted` by the next open, so success is never
                    // reported without its durable mirror.
                    let provider_id = self
                        .state
                        .with_js_state(|s| s.session(session_id).map(|r| r.provider_id.clone()))
                        .flatten()
                        .unwrap_or_default();
                    match self
                        .state
                        .journal_operation(&operation_id, session_id, &provider_id, "running")
                        .await
                    {
                        Ok(()) => {
                            self.state
                                .record_js_session_operation(session_id, operation_id);
                        }
                        Err(err) => {
                            // Effect-committed failure: the adapter already
                            // installed and started this operation before the
                            // journal write ran, so the effect is committed —
                            // never rolled back, never retryable. EnvState
                            // stays aligned with the real provider state (the
                            // operation is live), so inspect and native close
                            // can still discover and reap it deterministically.
                            self.state
                                .record_js_session_operation(session_id, operation_id.clone());
                            return Err(effect_committed_failure(err, &operation_id, session_id));
                        }
                    }
                }
            }
            // A cooperative cancel acknowledgement settles exactly the
            // requested operation to the `cancelled` terminal (observable via
            // GET), mirrored durably — never whichever operation happens to
            // be active for the session. A cancel call may carry only the
            // operation id, so the owning session is resolved from the
            // operation when session_id is absent.
            ProviderCallMethod::Cancel if reply.ok => {
                let operation_id = request_operation_id.take();
                let session_id = session_id.or_else(|| {
                    operation_id.as_deref().and_then(|op_id| {
                        self.state
                            .with_js_state(|s| s.operation(op_id).map(|op| op.session_id))
                            .flatten()
                    })
                });
                if let Some(operation_id) = operation_id {
                    // Journal the `cancelled` terminal BEFORE the in-memory
                    // acknowledgement on the happy path (LIFE-3).
                    match self
                        .state
                        .journal_operation_status(&operation_id, "cancelled")
                        .await
                    {
                        Ok(()) => {
                            // Settle exactly the requested operation id —
                            // never whichever operation happens to be active
                            // for the session; the session's active op is
                            // cleared only when it matches the targeted op.
                            if let Some(session_id) = session_id.as_deref() {
                                self.state.record_js_operation_terminal(
                                    session_id,
                                    &operation_id,
                                    JsOperationStatus::Cancelled,
                                );
                            }
                        }
                        Err(err) => {
                            // Effect-committed failure: the adapter already
                            // cancelled the requested operation, reaped the
                            // owned child and released the session before this
                            // journal write ran. The effect is committed —
                            // never rolled back, never retryable — so EnvState
                            // settles the same `cancelled` terminal the
                            // provider actually produced, exactly for the
                            // requested operation id, and the typed 500 names
                            // the operation/session for deterministic
                            // inspect/cleanup. The missing durable row is
                            // settled by the next open (orphan settlement).
                            let session = session_id.clone().unwrap_or_default();
                            if !session.is_empty() {
                                self.state.record_js_operation_terminal(
                                    &session,
                                    &operation_id,
                                    JsOperationStatus::Cancelled,
                                );
                            }
                            return Err(effect_committed_failure(err, &operation_id, &session));
                        }
                    }
                }
            }
            ProviderCallMethod::Shutdown if reply.ok => {
                if let Some(session_id) = session_id.as_deref() {
                    // Delete the durable mirror BEFORE forgetting the session
                    // on the happy path (LIFE-3).
                    match self.state.forget_journal_session(session_id).await {
                        Ok(()) => {
                            self.state.forget_js_session(session_id);
                        }
                        Err(err) => {
                            // Effect-committed failure: the adapter already
                            // reaped the owned child and deleted the session,
                            // so there is nothing left to reap — EnvState
                            // stays aligned with that reality (the session is
                            // gone from the live set) and the typed 500 names
                            // the session. The stale journal rows are settled
                            // by the next open (LIFE-3 orphan settlement).
                            self.state.forget_js_session(session_id);
                            return Err(effect_committed_failure(err, "", session_id));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(reply)
    }

    async fn next(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        // A pending terminal batch means an earlier delivery consumed this
        // terminal from the provider (terminal delivery is dequeued and
        // acknowledged before `inner.next` returns, so a re-pull can never
        // replay it) but its durable mirror failed. Journal first, then apply
        // the memory terminal, then return the exact retained batch exactly
        // once (LIFE-3).
        if let Some(pending) = self
            .pending_terminal_batches
            .lock()
            .ok()
            .and_then(|mut pending_batches| pending_batches.remove(&operation_id))
        {
            match self
                .state
                .journal_operation_status(&operation_id, pending.status.wire())
                .await
            {
                Ok(()) => {
                    self.state.apply_js_batch_terminal(
                        &pending.session_id,
                        &operation_id,
                        pending.status,
                    );
                    return Ok(pending.batch);
                }
                Err(err) => {
                    self.retain_pending_terminal_batch(&operation_id, pending);
                    return Err(journal_failure(err));
                }
            }
        }
        let batch = self
            .inner
            .next(operation_id.clone(), max_events, max_bytes)
            .await?;
        // Terminal truth comes from the actual delivered events, never a
        // label. Detect without mutating, mirror the derived terminal durably
        // (it must survive a restart), and only then apply it in memory
        // (LIFE-3). A failed journal write cannot be repaired by a re-pull —
        // the provider already consumed the batch — so the exact batch is
        // retained and re-delivered after the journal retry succeeds.
        if let Some((status, session_id)) = self
            .state
            .detect_js_batch_terminal(&operation_id, batch.events.as_slice())
        {
            match self
                .state
                .journal_operation_status(&operation_id, status.wire())
                .await
            {
                Ok(()) => {
                    self.state
                        .apply_js_batch_terminal(&session_id, &operation_id, status);
                }
                Err(err) => {
                    self.retain_pending_terminal_batch(
                        &operation_id,
                        PendingTerminalBatch {
                            batch: batch.clone(),
                            status,
                            session_id,
                        },
                    );
                    return Err(journal_failure(err));
                }
            }
        }
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_state::{EnvState, JsOperationStatus};
    use async_trait::async_trait;
    use nexus_contracts::provider_call::ProviderCallMethod;
    use nexus_contracts::{CoreError, CoreErrorCode, ProviderReply};
    use nexus_provider_ports::{ProviderPort, ProviderResult};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A probe/launch stub that echoes a deterministic session id and records
    /// how many calls actually reached it (so a cap must deny *before* inner).
    struct EchoPort {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ProviderPort for EchoPort {
        async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderReply {
                request_id: request.request_id.clone(),
                ok: true,
                session_id: Some(format!("sess-{}", self.calls.load(Ordering::SeqCst))),
                operation_id: None,
                health: None,
                error: None,
            })
        }

        async fn next(
            &self,
            _operation_id: String,
            _max_events: u32,
            _max_bytes: u32,
        ) -> ProviderResult<ProviderEventBatch> {
            Err(CoreError {
                code: CoreErrorCode::Internal,
                message: "not used".into(),
                details: Default::default(),
                http_status: Some(500),
            })
        }
    }

    use nexus_agent_host::HostManager;

    fn launch_request(provider_id: &str, request_id: &str) -> ProviderCall {
        let mut payload = serde_json::Map::new();
        payload.insert(
            "provider_id".into(),
            serde_json::Value::String(provider_id.into()),
        );
        ProviderCall {
            method: ProviderCallMethod::Launch,
            request_id: request_id.into(),
            session_id: None,
            operation_id: None,
            deadline_ms: 5_000,
            payload,
        }
    }

    /// The cap must deny a launch *before* dispatching to inner, so a refused
    /// launch never spawns an untracked owned child.
    #[tokio::test]
    async fn js_session_cap_denies_launch_before_inner() {
        let state = Arc::new(EnvState::new());
        let inner = Arc::new(EchoPort {
            calls: AtomicUsize::new(0),
        });
        let host = Arc::new(HostManager::new());
        let port = AdmittingProviderPort::new(host, inner.clone(), state.clone());
        // Fill the transport session cap with direct reservations.
        for _ in 0..crate::env_state::JS_PROVIDER_MAX_ACTIVE_SESSIONS {
            assert!(state.try_reserve_js_session());
        }
        let err = port
            .call(launch_request("mock", "r1"))
            .await
            .expect_err("cap must deny the launch");
        assert_eq!(err.code, CoreErrorCode::Busy);
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            0,
            "a capped launch must not reach the adapter and spawn a child"
        );
    }

    /// Concurrent launches must keep request-local ownership: each recorded
    /// session carries its own provider id, never a shared last-value.
    #[tokio::test]
    async fn concurrent_launches_keep_request_local_provider_ownership() {
        let state = Arc::new(EnvState::new());
        let inner = Arc::new(EchoPort {
            calls: AtomicUsize::new(0),
        });
        let host = Arc::new(HostManager::new());
        let port = Arc::new(AdmittingProviderPort::new(host, inner, state.clone()));
        // Seed two launch records with distinct provider ids directly, proving
        // the state maps session -> its own provider id (no cross-assignment).
        state.record_js_session("s-a".to_string(), "provider-a".to_string());
        state.record_js_session("s-b".to_string(), "provider-b".to_string());
        let a = state
            .with_js_state(|s| s.session("s-a").cloned())
            .flatten()
            .unwrap();
        let b = state
            .with_js_state(|s| s.session("s-b").cloned())
            .flatten()
            .unwrap();
        assert_eq!(a.provider_id, "provider-a");
        assert_eq!(b.provider_id, "provider-b");
        let _ = port;
    }

    /// An Execute-ok stub carrying both session and operation ids, so the
    /// durable write-through path runs after the inner effect succeeds.
    struct ExecuteOkPort;

    #[async_trait]
    impl ProviderPort for ExecuteOkPort {
        async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
            Ok(ProviderReply {
                request_id: request.request_id.clone(),
                ok: true,
                session_id: Some("sess-j".to_string()),
                operation_id: Some("op-j".to_string()),
                health: None,
                error: None,
            })
        }

        async fn next(
            &self,
            operation_id: String,
            _max_events: u32,
            _max_bytes: u32,
        ) -> ProviderResult<ProviderEventBatch> {
            Ok(ProviderEventBatch {
                events: vec![
                    nexus_contracts::provider_event_batch::NexusProviderHostEvent::OpFailed {
                        error_category: "test".to_string(),
                        error_message: "terminal".to_string(),
                        op_id: operation_id.clone(),
                        session_id: "sess-j".to_string(),
                    },
                ],
                gap: None,
                has_more: false,
                operation_id,
            })
        }
    }

    /// Build a real [`nexus_core::CoreService`] and install it on the env as
    /// the journal owner (P4-T2 seam). A read-only core refuses journal
    /// writes for real; an engine-owner core accepts them.
    async fn install_journal_core(
        state: &EnvState,
        access: nexus_core::CoreAccess,
    ) -> (tempfile::TempDir, Arc<nexus_core::CoreService>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let user_home = tmp.path().to_path_buf();
        let nexus_home = nexus_home_layout::nexus_root_from_home(&user_home);
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home,
            "creator-journal",
            "default",
        ))
        .expect("workspace dir");
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"creator-journal\"\n\
             [active_workspace_slug_by_creator]\n\
             \"creator-journal\" = \"default\"",
        )
        .expect("config");
        let core = nexus_core::CoreService::open(nexus_core::CoreOpenOptions { user_home, access })
            .await
            .expect("core open");
        let core = Arc::new(core);
        *state.core.lock().expect("core mutex") = Some(core.clone());
        (tmp, core)
    }

    fn execute_request() -> ProviderCall {
        ProviderCall {
            method: ProviderCallMethod::Execute,
            request_id: "r-execute".into(),
            session_id: Some("sess-j".into()),
            operation_id: None,
            deadline_ms: 5_000,
            payload: serde_json::Map::new(),
        }
    }

    /// LIFE-3 write-through is part of the Execute contract: a failed journal
    /// upsert is a typed internal error, never a success reply without its
    /// durable mirror. The provider effect (the installed, running operation)
    /// is already committed when the journal fails, so EnvState stays aligned
    /// with it — the session keeps its active operation so inspect and native
    /// close can discover/reap it — and the typed 500 names the
    /// operation/session identity. The effect is not rolled back and not
    /// retryable.
    #[tokio::test]
    async fn execute_journal_failure_is_effect_committed_and_tracked() {
        let state = Arc::new(EnvState::new());
        // A read-only core refuses journal writes for real (P4-T2 seam).
        let (_dir, _core) = install_journal_core(&state, nexus_core::CoreAccess::ReadOnly).await;
        // Production records the session at Launch; the journal write-through
        // resolves the provider id from that session record.
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(ExecuteOkPort),
            state.clone(),
        );
        let err = port
            .call(execute_request())
            .await
            .expect_err("a failed journal write-through must fail the call");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert_eq!(err.http_status, Some(500));
        assert!(
            err.message.contains("effect committed"),
            "the error names the committed-effect boundary: {}",
            err.message
        );
        assert!(
            err.message.contains("op-j") && err.message.contains("sess-j"),
            "the error carries operation/session identity: {}",
            err.message
        );
        assert_eq!(
            err.details.get("operation_id").and_then(|v| v.as_str()),
            Some("op-j"),
            "structured identity supports deterministic inspect/cleanup"
        );
        assert_eq!(
            state
                .with_js_state(|s| {
                    s.session("sess-j")
                        .and_then(|r| r.active_operation_id.clone())
                })
                .flatten(),
            Some("op-j".to_string()),
            "the committed provider effect stays discoverable for inspect/close"
        );
    }

    /// An observed terminal is never reported as delivered when its durable
    /// mirror failed: `next` propagates the journal failure, detection ran
    /// without mutating, so the operation is still active in memory — and the
    /// consumed batch is retained in the wrapper for re-delivery.
    #[tokio::test]
    async fn terminal_journal_failure_on_next_is_propagated() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-j".to_string());
        // A read-only core refuses journal writes for real (P4-T2 seam).
        let (_dir, _core) = install_journal_core(&state, nexus_core::CoreAccess::ReadOnly).await;
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(ExecuteOkPort),
            state.clone(),
        );
        let err = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect_err("a failed terminal mirror must fail the delivery");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert!(err.message.contains("journal"));
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-j").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Running),
            "a failed terminal mirror must leave the operation running in memory"
        );
        assert_eq!(
            state
                .with_js_state(|s| s
                    .session("sess-j")
                    .and_then(|r| r.active_operation_id.clone()))
                .flatten(),
            Some("op-j".to_string()),
            "the session keeps its active op while the terminal awaits its durable mirror"
        );
        assert!(
            port.pending_terminal_batches
                .lock()
                .expect("cache lock")
                .contains_key("op-j"),
            "the consumed terminal batch must be retained for re-delivery"
        );
    }

    /// A consuming provider stub: the terminal batch is dequeued exactly
    /// once — every later pull returns an empty batch, mirroring the real
    /// ACP adapter where terminal delivery is removed/acknowledged before
    /// `next` returns.
    struct ConsumingTerminalPort {
        pulls: AtomicUsize,
    }

    #[async_trait]
    impl ProviderPort for ConsumingTerminalPort {
        async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
            Ok(ProviderReply {
                request_id: request.request_id.clone(),
                ok: true,
                session_id: Some("sess-j".to_string()),
                operation_id: Some("op-j".to_string()),
                health: None,
                error: None,
            })
        }

        async fn next(
            &self,
            operation_id: String,
            _max_events: u32,
            _max_bytes: u32,
        ) -> ProviderResult<ProviderEventBatch> {
            let pull = self.pulls.fetch_add(1, Ordering::SeqCst);
            let events = if pull == 0 {
                vec![
                    nexus_contracts::provider_event_batch::NexusProviderHostEvent::OpFailed {
                        error_category: "test".to_string(),
                        error_message: "terminal".to_string(),
                        op_id: operation_id.clone(),
                        session_id: "sess-j".to_string(),
                    },
                ]
            } else {
                Vec::new()
            };
            Ok(ProviderEventBatch {
                events,
                gap: None,
                has_more: false,
                operation_id,
            })
        }
    }

    /// The provider terminal is consumed on delivery (a re-pull can never
    /// replay it), so a failed terminal journal must not lose it: the exact
    /// batch is retained, a retry journals FIRST and re-delivers the same
    /// batch exactly once, and only after the durable mirror succeeds does
    /// memory settle the terminal.
    #[tokio::test]
    async fn consumed_terminal_batch_is_re_delivered_once_after_journal_retry() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-j".to_string());
        // A read-only core refuses journal writes for real (P4-T2 seam).
        let (_dir, _core) = install_journal_core(&state, nexus_core::CoreAccess::ReadOnly).await;
        let inner = Arc::new(ConsumingTerminalPort {
            pulls: AtomicUsize::new(0),
        });
        let port =
            AdmittingProviderPort::new(Arc::new(HostManager::new()), inner.clone(), state.clone());

        // First delivery: the provider terminal is consumed, the journal
        // write fails, and the exact batch is retained.
        let err = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect_err("a failed terminal mirror must fail the delivery");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert!(err.message.contains("journal"));
        assert_eq!(inner.pulls.load(Ordering::SeqCst), 1);
        assert!(
            port.pending_terminal_batches
                .lock()
                .expect("cache lock")
                .contains_key("op-j"),
            "the consumed batch must be retained for re-delivery"
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-j").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Running),
            "memory stays running until the durable mirror succeeds"
        );

        // Retry with the journal still failing: no re-pull — the retained
        // batch is kept for a later attempt.
        let err = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect_err("the retained batch must still fail while the journal fails");
        assert!(err.message.contains("journal"));
        assert_eq!(
            inner.pulls.load(Ordering::SeqCst),
            1,
            "a retry must not re-pull a consumed terminal"
        );

        // The journal recovers: the retry journals first, settles memory,
        // re-delivers the exact batch once, and clears the cache.
        // The journal recovers: an engine-owner core accepts the write.
        let (_dir_ok, core_ok) =
            install_journal_core(&state, nexus_core::CoreAccess::EngineOwner).await;
        let batch = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect("the retained batch is re-delivered after the journal succeeds");
        assert_eq!(
            inner.pulls.load(Ordering::SeqCst),
            1,
            "re-delivery comes from the retained cache, not the provider"
        );
        assert_eq!(
            batch.events.len(),
            1,
            "the exact terminal batch is re-delivered"
        );
        assert!(
            !port
                .pending_terminal_batches
                .lock()
                .expect("cache lock")
                .contains_key("op-j"),
            "the retained batch is consumed exactly once"
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-j").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Failed),
            "memory settles the terminal only after the durable mirror succeeded"
        );
        assert_eq!(
            state
                .with_js_state(|s| s
                    .session("sess-j")
                    .and_then(|r| r.active_operation_id.clone()))
                .flatten(),
            None,
            "the terminal releases the session's active operation"
        );
        let journaled = core_ok
            .provider_operation_row_internal("op-j")
            .await
            .expect("journal read")
            .expect("the durable mirror row exists");
        assert_eq!(journaled.2, "failed");
        assert_eq!(journaled.1, "sess-j");

        // A later pull reaches the provider again and carries no terminal:
        // the terminal batch was delivered exactly once.
        let later = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect("later pulls resume from the provider");
        assert_eq!(inner.pulls.load(Ordering::SeqCst), 2);
        assert!(
            later.events.is_empty(),
            "the terminal batch is never delivered twice"
        );
    }

    /// A cancel whose durable `cancelled` mirror fails is a typed error. The
    /// provider effect (cancel + child reap + session release) already
    /// committed, so EnvState settles the same `cancelled` terminal the
    /// provider actually produced — never a stale running state — and the
    /// typed 500 names the operation/session identity for deterministic
    /// inspect/cleanup. The effect is not rolled back and not retryable.
    #[tokio::test]
    async fn cancel_journal_failure_is_effect_committed() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-j".to_string());
        // A read-only core refuses journal writes for real (P4-T2 seam).
        let (_dir, _core) = install_journal_core(&state, nexus_core::CoreAccess::ReadOnly).await;
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(EchoPort {
                calls: AtomicUsize::new(0),
            }),
            state.clone(),
        );
        let request = ProviderCall {
            method: ProviderCallMethod::Cancel,
            request_id: "r-cancel".into(),
            session_id: Some("sess-j".into()),
            operation_id: Some("op-j".into()),
            deadline_ms: 5_000,
            payload: serde_json::Map::new(),
        };
        let err = port
            .call(request)
            .await
            .expect_err("a failed cancel mirror must fail the call");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert_eq!(err.http_status, Some(500));
        assert!(
            err.message.contains("effect committed") && err.message.contains("op-j"),
            "the error names the committed effect and the operation: {}",
            err.message
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-j").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Cancelled),
            "memory settles the terminal the provider actually produced"
        );
        assert_eq!(
            state
                .with_js_state(|s| s
                    .session("sess-j")
                    .and_then(|r| r.active_operation_id.clone()))
                .flatten(),
            None,
            "the committed cancel releases the session's active operation"
        );
    }

    /// A successful cancel settles exactly the requested operation id — never
    /// whichever operation happens to be active for the session (QC3-C2).
    /// With `op-y` active, cancelling `op-x` must reach the `cancelled`
    /// terminal for `op-x`, leave `op-y` running and still the session's
    /// active op, and mirror only `op-x` durably.
    #[tokio::test]
    async fn cancel_settles_the_requested_operation_not_the_session_active_op() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-y".to_string());
        let (_dir, core) = install_journal_core(&state, nexus_core::CoreAccess::EngineOwner).await;
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(EchoPort {
                calls: AtomicUsize::new(0),
            }),
            state.clone(),
        );
        let request = ProviderCall {
            method: ProviderCallMethod::Cancel,
            request_id: "r-cancel-x".into(),
            session_id: Some("sess-j".into()),
            operation_id: Some("op-x".into()),
            deadline_ms: 5_000,
            payload: serde_json::Map::new(),
        };
        port.call(request)
            .await
            .expect("a successful cancel with a durable mirror must succeed");
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-x").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Cancelled),
            "the requested operation reaches the cancelled terminal"
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-y").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Running),
            "cancel(op-x) must not mark the session's active op op-y cancelled"
        );
        assert_eq!(
            state
                .with_js_state(|s| s
                    .session("sess-j")
                    .and_then(|r| r.active_operation_id.clone()))
                .flatten(),
            Some("op-y".to_string()),
            "the session keeps its active op when the cancel targets a different operation"
        );
        let journaled = core
            .provider_operation_row_internal("op-x")
            .await
            .expect("journal read")
            .expect("the durable mirror row exists for the requested op");
        assert_eq!(journaled.2, "cancelled");
        assert!(
            core.provider_operation_row_internal("op-y")
                .await
                .expect("journal read")
                .is_none(),
            "the cancel must not journal the untouched active op"
        );
    }

    /// A cancel whose durable mirror fails settles the same `cancelled`
    /// terminal in memory, exactly for the requested operation id (QC3-C2):
    /// with `op-y` active, a failed cancel of `op-x` must settle `op-x` and
    /// leave `op-y` running and still the session's active op.
    #[tokio::test]
    async fn cancel_journal_failure_settles_the_requested_operation_not_the_active_op() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-y".to_string());
        // A read-only core refuses journal writes for real (P4-T2 seam).
        let (_dir, _core) = install_journal_core(&state, nexus_core::CoreAccess::ReadOnly).await;
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(EchoPort {
                calls: AtomicUsize::new(0),
            }),
            state.clone(),
        );
        let request = ProviderCall {
            method: ProviderCallMethod::Cancel,
            request_id: "r-cancel-x".into(),
            session_id: Some("sess-j".into()),
            operation_id: Some("op-x".into()),
            deadline_ms: 5_000,
            payload: serde_json::Map::new(),
        };
        let err = port
            .call(request)
            .await
            .expect_err("a failed cancel mirror must fail the call");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert!(
            err.message.contains("op-x"),
            "the error names the requested operation: {}",
            err.message
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-x").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Cancelled),
            "the failure-recovery path settles the requested operation id"
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-y").map(|op| op.status))
                .flatten(),
            Some(JsOperationStatus::Running),
            "cancel(op-x) must not mark the session's active op op-y cancelled"
        );
        assert_eq!(
            state
                .with_js_state(|s| s
                    .session("sess-j")
                    .and_then(|r| r.active_operation_id.clone()))
                .flatten(),
            Some("op-y".to_string()),
            "the session keeps its active op when the cancel targets a different operation"
        );
    }
}
