mod callbacks;
mod env_state;
mod lifecycle;
mod provider_stub;

use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::{
    CoreChangesRequest, CoreCloseReport, CoreHostQuery, CoreHostQueryResponse, NativeCompatibility,
    NativeOpenOptions, ProviderCall, ProviderEventBatch, ProviderReply,
    WorldKbPatchEntityRequest,
};
use nexus_contracts::native_compatibility::NativeCompatibilityContractTreeSha256;
use nexus_core::Principal;
use nexus_provider_ports::ProviderPort;
use once_cell::sync::OnceCell;

use callbacks::install_js_provider;
use env_state::EnvState;

static ENV_STATE: OnceCell<Arc<EnvState>> = OnceCell::new();

fn state() -> Arc<EnvState> {
    ENV_STATE
        .get_or_init(|| Arc::new(EnvState::new()))
        .clone()
}

fn contract_tree_sha256() -> NativeCompatibilityContractTreeSha256 {
    // Stable placeholder until build-time schema hash wiring lands in T3 integration.
    NativeCompatibilityContractTreeSha256::try_from(
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .expect("valid placeholder sha256")
}

#[napi]
pub fn compatibility() -> Result<String> {
    let compat = NativeCompatibility {
        native_api_version: 1,
        writer_protocol: 1,
        target_triple: std::env::var("NEXUS_BUILD_TARGET").unwrap_or_else(|_| {
            format!(
                "{}-unknown-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        }),
        package_version: "0.1.0".to_string(),
        contract_tree_sha256: contract_tree_sha256(),
        db_schema_min: 0,
        db_schema_max: 999_999,
        napi_minimum: 8,
    };
    Ok(serde_json::to_string(&compat)?)
}

#[napi]
pub struct NativeCore {
    inner: Arc<EnvState>,
}

#[napi]
impl NativeCore {
    #[napi]
    pub async fn active_principal(&self) -> Result<String> {
        let core = self
            .inner
            .core
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let principal = core.active_principal().await.map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(EnvState::encode_principal(
            &principal,
            self.inner.generation.load(std::sync::atomic::Ordering::SeqCst),
        ))
    }

    #[napi]
    pub async fn world_kb_graph(
        &self,
        principal_handle: String,
        world_id: String,
        include_suggested: bool,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.world_kb_graph(&principal, world_id, include_suggested)
                .await
                .map_err(|e| Error::from_reason(format!("{e}")))
        })
        .await
    }

    #[napi]
    pub async fn patch_world_kb_entity(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorldKbPatchEntityRequest = serde_json::from_slice(request_json.as_ref())?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_world_kb_entity(&principal, world_id, request)
                .await
                .map_err(|e| Error::from_reason(format!("{e}")))
        })
        .await
    }

    #[napi]
    pub async fn world_kb_candidates(
        &self,
        principal_handle: String,
        world_id: String,
        limit: Option<i64>,
        cursor: Option<String>,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.world_kb_candidates(&principal, world_id, limit, cursor)
                .await
                .map_err(|e| Error::from_reason(format!("{e}")))
        })
        .await
    }

    #[napi]
    pub async fn changes(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CoreChangesRequest = serde_json::from_slice(request_json.as_ref())?;
        self.json_call(principal_handle, async move |core, principal| {
            core.changes(&principal, request)
                .await
                .map_err(|e| Error::from_reason(format!("{e}")))
        })
        .await
    }

    #[napi]
    pub async fn host_query(&self, request_json: Buffer) -> Result<Buffer> {
        let request: CoreHostQuery = serde_json::from_slice(request_json.as_ref())?;
        let core = self
            .inner
            .core
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let port = self
            .inner
            .provider_port
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let response = dispatch_host_query(&core, port.as_ref(), request)
            .await
            .map_err(|e| Error::from_reason(e))?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    #[napi]
    pub async fn provider_call(&self, request_json: Buffer) -> Result<Buffer> {
        let request: ProviderCall = serde_json::from_slice(request_json.as_ref())?;
        let port = self
            .inner
            .provider_port
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let reply = port
            .call(request)
            .await
            .map_err(|e| Error::from_reason(e.message.clone()))?;
        Ok(Buffer::from(serde_json::to_vec(&reply)?))
    }

    #[napi]
    pub async fn next_provider_events(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> Result<Buffer> {
        let port = self
            .inner
            .provider_port
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let batch = port
            .next(operation_id, max_events, max_bytes)
            .await
            .map_err(|e| Error::from_reason(e.message.clone()))?;
        Ok(Buffer::from(serde_json::to_vec(&batch)?))
    }

    #[napi]
    pub async fn close(&self) -> Result<Buffer> {
        let report: CoreCloseReport = lifecycle::close_core(&self.inner).await;
        Ok(Buffer::from(serde_json::to_vec(&report)?))
    }

    async fn json_call<T, F, Fut>(&self, principal_handle: String, f: F) -> Result<Buffer>
    where
        T: serde::Serialize,
        F: FnOnce(Arc<nexus_core::CoreService>, Principal) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        if self.inner.is_closing() {
            return Err(Error::from_reason("closing"));
        }
        let core = self
            .inner
            .core
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let principal = core.active_principal().await.map_err(|e| {
            Error::from_reason(format!("{e}"))
        })?;
        let encoded = EnvState::encode_principal(
            &principal,
            self.inner.generation.load(std::sync::atomic::Ordering::SeqCst),
        );
        if encoded != principal_handle {
            return Err(Error::from_reason("invalid principal handle"));
        }
        let value = f(core, principal).await?;
        Ok(Buffer::from(serde_json::to_vec(&value)?))
    }
}

async fn dispatch_host_query(
    core: &Arc<nexus_core::CoreService>,
    _port: &dyn ProviderPort,
    request: CoreHostQuery,
) -> std::result::Result<CoreHostQueryResponse, String> {
    use nexus_contracts::core_host_query::CoreHostQueryQuery;
    match request.query {
        CoreHostQueryQuery::Health => {
            let principal = core.active_principal().await.map_err(|e| e.to_string())?;
            let _ = principal;
            Ok(CoreHostQueryResponse {
                health: Some(nexus_contracts::core_host_query_response::CoreHostQueryResponseHealth {
                    running: true,
                    active_sessions: 0,
                    active_operations: 0,
                }),
                catalog: None,
                sessions: None,
                session: None,
                operation: None,
                scan: None,
            })
        }
        other => Err(format!("host query {:?} not wired in P2-T1", other)),
    }
}

#[napi]
pub fn register_provider_callbacks(env: Env, callbacks: Object<'_>) -> Result<()> {
    let st = state();
    let bridge = install_js_provider(&env, st.clone(), callbacks)?;
    st.pending_provider
        .lock()
        .map_err(|_| Error::from_reason("pending provider lock poisoned"))?
        .replace(bridge);
    Ok(())
}

#[napi]
pub async fn open(options_json: String) -> Result<NativeCore> {
    let options: NativeOpenOptions = serde_json::from_str(&options_json)?;
    let st = state();
    lifecycle::open_core(&st, options).await.map_err(Error::from_reason)?;
    Ok(NativeCore { inner: st })
}
