//! Wraps a JS [`ProviderPort`] and injects Rust-admitted recipes for probe/launch.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::providers::recipe_admission::reject_caller_recipe_payload;
use nexus_agent_host::HostManager;
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_local_db::LocalDbError;
use nexus_provider_ports::{ProviderPort, ProviderResult};

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
fn journal_failure(err: LocalDbError) -> CoreError {
    CoreError {
        code: CoreErrorCode::Internal,
        message: format!("durable js-provider journal write failed: {err}"),
        details: Default::default(),
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

pub struct AdmittingProviderPort {
    host: Arc<HostManager>,
    inner: Arc<dyn ProviderPort>,
    state: Arc<super::env_state::EnvState>,
}

impl AdmittingProviderPort {
    pub fn new(
        host: Arc<HostManager>,
        inner: Arc<dyn ProviderPort>,
        state: Arc<super::env_state::EnvState>,
    ) -> Self {
        Self { host, inner, state }
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
                    self.state
                        .record_js_session_operation(session_id, operation_id.clone());
                    // Durable write-through: an op still active at process exit
                    // is settled to `interrupted` by the next open (LIFE-3).
                    let provider_id = self
                        .state
                        .with_js_state(|s| s.session(session_id).map(|r| r.provider_id.clone()))
                        .flatten()
                        .unwrap_or_default();
                    self.state
                        .journal_operation(&operation_id, session_id, &provider_id, "running")
                        .await
                        .map_err(journal_failure)?;
                }
            }
            // A cooperative cancel acknowledgement settles the operation to the
            // `cancelled` terminal (observable via GET), mirrored durably. A
            // cancel call may carry only the operation id, so the owning session
            // is resolved from the operation when session_id is absent.
            ProviderCallMethod::Cancel if reply.ok => {
                let operation_id = request_operation_id.take();
                let session_id = session_id.or_else(|| {
                    operation_id.as_deref().and_then(|op_id| {
                        self.state
                            .with_js_state(|s| s.operation(op_id).map(|op| op.session_id))
                            .flatten()
                    })
                });
                if let Some(session_id) = session_id.as_deref() {
                    self.state.record_js_cancel_ack(session_id);
                }
                if let Some(operation_id) = operation_id {
                    self.state
                        .journal_operation_status(&operation_id, "cancelled")
                        .await
                        .map_err(journal_failure)?;
                }
            }
            ProviderCallMethod::Shutdown if reply.ok => {
                if let Some(session_id) = session_id.as_deref() {
                    self.state.forget_js_session(session_id);
                    self.state
                        .forget_journal_session(session_id)
                        .await
                        .map_err(journal_failure)?;
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
        let batch = self
            .inner
            .next(operation_id.clone(), max_events, max_bytes)
            .await?;
        // Terminal truth comes from the actual delivered events, never a label.
        let terminal = self
            .state
            .apply_js_batch_terminals(&operation_id, batch.events.as_slice());
        // Mirror the observed terminal durably so it survives a restart.
        if let Some(status) = terminal {
            self.state
                .journal_operation_status(&operation_id, status)
                .await
                .map_err(journal_failure)?;
        }
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_state::EnvState;
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
        payload.insert("provider_id".into(), serde_json::Value::String(provider_id.into()));
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
        let a = state.with_js_state(|s| s.session("s-a").cloned()).flatten().unwrap();
        let b = state.with_js_state(|s| s.session("s-b").cloned()).flatten().unwrap();
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
                events: vec![nexus_contracts::provider_event_batch::NexusProviderHostEvent::OpFailed {
                    error_category: "test".to_string(),
                    error_message: "terminal".to_string(),
                    op_id: operation_id.clone(),
                    session_id: "sess-j".to_string(),
                }],
                gap: None,
                has_more: false,
                operation_id,
            })
        }
    }

    /// A journal pool whose database has no journal table: the write fails for
    /// real (no mocks, no fault flags).
    async fn journalless_pool() -> sqlx::SqlitePool {
        sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool")
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
    /// durable mirror. The in-memory record still happened first.
    #[tokio::test]
    async fn execute_journal_failure_fails_the_reply() {
        let state = Arc::new(EnvState::new());
        state.set_journal_pool(journalless_pool().await);
        // Production records the session at Launch; the in-memory operation
        // record must land before the durable write-through is attempted.
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
        assert!(err.message.contains("journal"), "the error names the journal boundary");
        let recorded = state
            .with_js_state(|s| {
                s.session("sess-j")
                    .and_then(|r| r.active_operation_id.clone())
            })
            .flatten();
        assert_eq!(recorded, Some("op-j".to_string()));
    }

    /// An observed terminal is never reported as delivered when its durable
    /// mirror failed: `next` propagates the journal failure instead.
    #[tokio::test]
    async fn terminal_journal_failure_on_next_is_propagated() {
        let state = Arc::new(EnvState::new());
        state.record_js_session("sess-j".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-j", "op-j".to_string());
        state.set_journal_pool(journalless_pool().await);
        let port = AdmittingProviderPort::new(
            Arc::new(HostManager::new()),
            Arc::new(ExecuteOkPort),
            state,
        );
        let err = port
            .next("op-j".to_string(), 10, 64_000)
            .await
            .expect_err("a failed terminal mirror must fail the delivery");
        assert_eq!(err.code, CoreErrorCode::Internal);
        assert!(err.message.contains("journal"));
    }
}
