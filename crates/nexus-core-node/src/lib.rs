//! Node-API adapter for Nexus core + provider port.
//!
//! # Safety boundary
//! N-API/Javascript interop is confined to dependency-provided shims and
//! `#[napi]`-generated glue. All crate-local logic is safe Rust.

mod admitting_provider_port;
mod callbacks;
mod env_state;
mod host_query;
mod lifecycle;
mod runtime;

use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::{
    CoreChangesRequest, CoreCloseReport, CoreHostQuery, CoreHostQueryResponse, NativeCompatibility,
    NativeOpenOptions, ProviderCall,
    WorldKbPatchEntityRequest,
};
use nexus_contracts::native_compatibility::NativeCompatibilityContractTreeSha256;
use nexus_core::Principal;

use env_state::{EnvInstance, EnvState};

fn contract_tree_sha256() -> NativeCompatibilityContractTreeSha256 {
    NativeCompatibilityContractTreeSha256::try_from(env!("NEXUS_CONTRACT_TREE_SHA256"))
        .expect("valid build-time contract_tree_sha256")
}

fn db_schema_min() -> u64 {
    env!("NEXUS_DB_SCHEMA_MIN").parse().expect("db_schema_min")
}

fn db_schema_max() -> u64 {
    env!("NEXUS_DB_SCHEMA_MAX").parse().expect("db_schema_max")
}

#[napi]
pub fn compatibility() -> Result<String> {
    let compat = NativeCompatibility {
        native_api_version: 1,
        writer_protocol: 1,
        // Build-time target from `build.rs` (`TARGET`); never a runtime-env read.
        target_triple: env!("NEXUS_BUILD_TARGET").to_string(),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        contract_tree_sha256: contract_tree_sha256(),
        db_schema_min: db_schema_min(),
        db_schema_max: db_schema_max(),
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
        let principal = core
            .active_principal()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
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
        limit: Option<f64>,
        cursor: Option<String>,
    ) -> Result<Buffer> {
        let limit_i64 = limit.map(|n| {
            if !n.is_finite() || n < 0.0 || n > 9_007_199_254_740_991.0 || n.fract() != 0.0 {
                return Err(Error::from_reason("invalid limit"));
            }
            Ok(n as i64)
        }).transpose()?;
        self.json_call(principal_handle, async move |core, principal| {
            core.world_kb_candidates(&principal, world_id, limit_i64, cursor)
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
        let response: CoreHostQueryResponse =
            host_query::dispatch_host_query(&self.inner, request)
                .await
                .map_err(Error::from_reason)?;
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
        let report: CoreCloseReport = lifecycle::close_core(self.inner.clone()).await;
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
        let principal = core
            .active_principal()
            .await
            .map_err(|e| Error::from_reason(format!("{e}")))?;
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

/// Test-only: force cleanup attempts to report unconfirmed. Present only in
/// debug builds (`debug_assertions`); release artifacts export no such hook.
#[cfg(debug_assertions)]
#[napi]
pub fn force_unconfirmed_cleanup(enable: bool) {
    lifecycle::set_force_unconfirmed(enable);
}

#[napi]
pub fn open(
    env: Env,
    options_json: String,
    callbacks: Option<Object<'_>>,
) -> Result<NativeCore> {
    let options: NativeOpenOptions = serde_json::from_str(&options_json)?;
    let state = match EnvInstance::get(&env) {
        Ok(existing) => existing,
        Err(_) => {
            let fresh = Arc::new(EnvState::new());
            EnvInstance::install(&env, fresh.clone())?;
            fresh
        }
    };
    let js_port = if let Some(callbacks) = callbacks {
        Some(callbacks::install_js_provider(&env, state.clone(), callbacks)?)
    } else {
        None
    };
    runtime::runtime()
        .block_on(lifecycle::open_core(state.clone(), options, js_port))
        .map_err(Error::from_reason)?;
    Ok(NativeCore { inner: state })
}
