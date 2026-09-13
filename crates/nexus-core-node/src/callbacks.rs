//! JS provider callback bridge with bounded TSFN admission (architecture §7).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use async_trait::async_trait;
use napi::bindgen_prelude::{PromiseRaw, *};
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use tokio::sync::{oneshot, Semaphore};

use super::env_state::EnvState;

const MAX_ACTIVE_PROMISES: usize = 32;
const MAX_PENDING_BYTES: usize = 1024 * 1024;

type JsPromiseString<'a> = PromiseRaw<'a, String>;

pub struct JsProviderBridge {
    state: Arc<EnvState>,
    call_tsfn: ThreadsafeFunction<String, JsPromiseString<'static>, String, Status, true, false, 16>,
    next_tsfn: ThreadsafeFunction<String, JsPromiseString<'static>, String, Status, true, false, 16>,
    active: Arc<Semaphore>,
}

impl JsProviderBridge {
    pub fn from_callbacks(env: &Env, state: Arc<EnvState>, callbacks: Object) -> Result<Self> {
        let call_fn: Function<String, JsPromiseString<'_>> = callbacks.get_named_property("call")?;
        let next_fn: Function<String, JsPromiseString<'_>> = callbacks.get_named_property("next")?;
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
        if payload_len > MAX_PENDING_BYTES {
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
        tsfn: &ThreadsafeFunction<String, JsPromiseString<'static>, String, Status, true, false, 16>,
        payload: String,
    ) -> ProviderResult<String> {
        self.check_admission(payload.len())?;
        let permit = self
            .active
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CoreError {
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
        let (tx, rx) = oneshot::channel();
        tsfn.call_with_return_value(
            Ok(payload),
            ThreadsafeFunctionCallMode::NonBlocking,
            move |result: Result<JsPromiseString<'_>>, _env: Env| {
                match result {
                    Ok(promise) => {
                        let _ = promise.then(move |ctx| {
                            let _ = tx.send(Ok(ctx.value));
                            Ok(())
                        });
                    }
                    Err(status) => {
                        let _ = tx.send(Err(format!("callback rejected: {status}")));
                    }
                }
                Ok(())
            },
        );
        let reply = rx.await.map_err(|_| CoreError {
            code: CoreErrorCode::Internal,
            message: "callback channel closed".into(),
            details: Default::default(),
            http_status: Some(500),
        })?;
        drop(permit);
        if self.state.generation.load(Ordering::SeqCst) != gen_at_enqueue {
            return Err(CoreError {
                code: CoreErrorCode::Interrupted,
                message: "stale callback generation".into(),
                details: Default::default(),
                http_status: Some(503),
            });
        }
        reply.map_err(|message| CoreError {
            code: CoreErrorCode::Internal,
            message,
            details: Default::default(),
            http_status: Some(500),
        })
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
        let payload = serde_json::json!({
            "operation_id": operation_id,
            "max_events": max_events,
            "max_bytes": max_bytes,
        });
        let raw = self
            .invoke_tsfn(&self.next_tsfn, payload.to_string())
            .await?;
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
) -> Result<Arc<JsProviderBridge>> {
    Ok(Arc::new(JsProviderBridge::from_callbacks(env, state, callbacks)?))
}
