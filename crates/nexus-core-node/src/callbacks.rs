//! JS provider callback bridge with bounded TSFN admission (architecture §7).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use napi::bindgen_prelude::{Promise, *};
use napi::threadsafe_function::ThreadsafeFunction;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use tokio::sync::Semaphore;

use super::env_state::EnvState;

const MAX_ACTIVE_PROMISES: usize = 32;

type JsPromiseString = Promise<String>;

fn map_callback_rejection(err: napi::Error) -> CoreError {
    let reason = if err.reason.is_empty() {
        err.to_string()
    } else {
        err.reason.clone()
    };
    if reason.contains("operation_not_found:") || reason.contains("ProviderNextError") {
        return CoreError {
            code: CoreErrorCode::NotFound,
            message: reason,
            details: Default::default(),
            http_status: Some(404),
        };
    }
    CoreError {
        code: CoreErrorCode::Internal,
        message: format!("callback rejected: {reason}"),
        details: Default::default(),
        http_status: Some(500),
    }
}

pub struct JsProviderBridge {
    state: Arc<EnvState>,
    call_tsfn: ThreadsafeFunction<String, JsPromiseString, String, Status, true, false, 16>,
    next_tsfn: ThreadsafeFunction<String, JsPromiseString, String, Status, true, false, 16>,
    active: Arc<Semaphore>,
    pull_in_flight: DashMap<String, Arc<AtomicBool>>,
}

impl JsProviderBridge {
    pub fn from_callbacks(_env: &Env, state: Arc<EnvState>, callbacks: Object) -> Result<Self> {
        let call_fn: Function<String, JsPromiseString> = callbacks.get_named_property("call")?;
        let next_fn: Function<String, JsPromiseString> = callbacks.get_named_property("next")?;
        let call_tsfn = call_fn
            .build_threadsafe_function()
            .callee_handled::<true>()
            .max_queue_size::<16>()
            .build()?;
        let next_tsfn = next_fn
            .build_threadsafe_function()
            .callee_handled::<true>()
            .max_queue_size::<16>()
            .build()?;
        Ok(Self {
            state,
            call_tsfn,
            next_tsfn,
            active: Arc::new(Semaphore::new(MAX_ACTIVE_PROMISES)),
            pull_in_flight: DashMap::new(),
        })
    }

    fn check_admission(&self, payload_len: usize) -> ProviderResult<()> {
        if self.state.is_closing() {
            return Err(CoreError {
                code: CoreErrorCode::Closing,
                message: "closing".into(),
                details: Default::default(),
                http_status: Some(503),
            });
        }
        if payload_len > super::env_state::MAX_PENDING_BYTES_TOTAL {
            return Err(CoreError {
                code: CoreErrorCode::InvalidInput,
                message: "input_too_large".into(),
                details: Default::default(),
                http_status: Some(413),
            });
        }
        Ok(())
    }

    async fn invoke_tsfn(
        &self,
        tsfn: &ThreadsafeFunction<String, JsPromiseString, String, Status, true, false, 16>,
        payload: String,
    ) -> ProviderResult<String> {
        self.check_admission(payload.len())?;
        let _budget = self
            .state
            .pending_budget
            .try_charge(payload.len())
            .map_err(|e| e)?;
        let permit = self.active.clone().try_acquire_owned().map_err(|_| CoreError {
            code: CoreErrorCode::Busy,
            message: "callback admission busy".into(),
            details: Default::default(),
            http_status: Some(503),
        })?;
        let gen_at_enqueue = self.state.generation.load(Ordering::SeqCst);
        if self.state.is_closing() {
            return Err(CoreError {
                code: CoreErrorCode::Closing,
                message: "closing".into(),
                details: Default::default(),
                http_status: Some(503),
            });
        }

        let promise = tsfn
            .call_async_catch(Ok(payload))
            .await
            .map_err(|e| CoreError {
                code: CoreErrorCode::Internal,
                message: format!("callback enqueue failed: {e}"),
                details: Default::default(),
                http_status: Some(503),
            })?;

        let close_fut = self.state.close_notify.notified();
        let reply = tokio::select! {
            () = close_fut => {
                return Err(CoreError {
                    code: CoreErrorCode::Closing,
                    message: "closing".into(),
                    details: Default::default(),
                    http_status: Some(503),
                });
            }
            result = promise => result.map_err(map_callback_rejection)?,
        };

        drop(permit);
        if self.state.generation.load(Ordering::SeqCst) != gen_at_enqueue {
            return Err(CoreError {
                code: CoreErrorCode::Interrupted,
                message: "stale callback generation".into(),
                details: Default::default(),
                http_status: Some(503),
            });
        }
        Ok(reply)
    }
}

#[async_trait]
impl ProviderPort for JsProviderBridge {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        let payload = serde_json::to_string(&request).map_err(|e| CoreError {
            code: CoreErrorCode::Internal,
            message: format!("provider_call_serialize: {e}"),
            details: Default::default(),
            http_status: Some(500),
        })?;
        let raw = self.invoke_tsfn(&self.call_tsfn, payload).await?;
        serde_json::from_str(&raw).map_err(|e| CoreError {
            code: CoreErrorCode::SchemaMismatch,
            message: format!("provider_call_reply: {e}"),
            details: Default::default(),
            http_status: Some(409),
        })
    }

    async fn next(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        let pull_flag = self
            .pull_in_flight
            .entry(operation_id.clone())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        if pull_flag
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
        let payload = serde_json::json!({
            "operation_id": operation_id,
            "max_events": max_events,
            "max_bytes": max_bytes,
        });
        let raw = self
            .invoke_tsfn(&self.next_tsfn, payload.to_string())
            .await;
        pull_flag.store(false, Ordering::Release);
        let raw = raw?;
        serde_json::from_str(&raw).map_err(|e| CoreError {
            code: CoreErrorCode::SchemaMismatch,
            message: format!("provider_next_reply: {e}"),
            details: Default::default(),
            http_status: Some(409),
        })
    }
}

pub fn install_js_provider(
    env: &Env,
    state: Arc<EnvState>,
    callbacks: Object,
) -> Result<Arc<dyn ProviderPort>> {
    Ok(Arc::new(JsProviderBridge::from_callbacks(env, state, callbacks)?))
}
