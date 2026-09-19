//! Node-API adapter for Nexus core + provider port.
//!
//! # Safety boundary
//! N-API/Javascript interop is confined to dependency-provided shims and
//! `#[napi]`-generated glue. All crate-local logic is safe Rust.

mod actors;
mod admitting_provider_port;
mod callbacks;
mod cleanup_registry;
mod core_error;
mod domain;
mod env_state;
mod execution;
mod host_query;
mod lifecycle;
mod runtime;
pub mod wire_fixture;

use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::native_compatibility::NativeCompatibilityContractTreeSha256;
use nexus_contracts::{
    CoreChangesRequest, CoreCloseReport, CoreHostQuery, CoreHostQueryResponse, NativeCompatibility,
    NativeOpenOptions, ProviderCall, WorldKbPatchEntityRequest,
};
use nexus_contracts::{CoreError, CoreErrorCode};
use nexus_core::Principal;
use serde_json::Value;

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
    fn deny_service_only(&self) -> Result<()> {
        if self.inner.is_service_only_uninitialized() {
            return Err(core_error::napi_error_from_wire(
                core_error::service_only_uninitialized_wire(),
            ));
        }
        Ok(())
    }

    #[napi]
    pub async fn active_principal(&self) -> Result<String> {
        self.deny_service_only()?;
        let core = self
            .inner
            .core
            .lock()
            .map_err(|_| Error::from_reason("core mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let principal = core
            .active_principal()
            .await
            .map_err(core_error::napi_error_from_domain)?;
        Ok(EnvState::encode_principal(
            &principal,
            self.inner
                .generation
                .load(std::sync::atomic::Ordering::SeqCst),
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
        self.deny_service_only()?;
        let request: WorldKbPatchEntityRequest = serde_json::from_slice(request_json.as_ref())?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_world_kb_entity(&principal, world_id, request)
                .await
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
        self.deny_service_only()?;
        let limit_i64 = limit
            .map(|n| {
                if !n.is_finite() || n < 0.0 || n > 9_007_199_254_740_991.0 || n.fract() != 0.0 {
                    return Err(Error::from_reason("invalid limit"));
                }
                Ok(n as i64)
            })
            .transpose()?;
        self.json_call(principal_handle, async move |core, principal| {
            core.world_kb_candidates(&principal, world_id, limit_i64, cursor)
                .await
        })
        .await
    }

    #[napi]
    pub async fn changes(&self, principal_handle: String, request_json: Buffer) -> Result<Buffer> {
        self.deny_service_only()?;
        let request: CoreChangesRequest = serde_json::from_slice(request_json.as_ref())?;
        self.json_call(principal_handle, async move |core, principal| {
            core.changes(&principal, request).await
        })
        .await
    }

    #[napi]
    pub async fn host_query(&self, request_json: Buffer) -> Result<Buffer> {
        self.deny_service_only()?;
        let request: CoreHostQuery = serde_json::from_slice(request_json.as_ref())?;
        let response: CoreHostQueryResponse = host_query::dispatch_host_query(&self.inner, request)
            .await
            .map_err(core_error::napi_error_from_open_reason)?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    #[napi]
    pub async fn provider_call(&self, request_json: Buffer) -> Result<Buffer> {
        self.deny_service_only()?;
        let request: ProviderCall = serde_json::from_slice(request_json.as_ref())?;
        let port = self
            .inner
            .provider_port
            .lock()
            .map_err(|_| Error::from_reason("port mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let reply = port
            .call(request)
            .await
            .map_err(core_error::napi_error_from_wire)?;
        Ok(Buffer::from(serde_json::to_vec(&reply)?))
    }

    #[napi]
    pub async fn next_provider_events(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> Result<Buffer> {
        self.deny_service_only()?;
        let port = self
            .inner
            .provider_port
            .lock()
            .map_err(|_| Error::from_reason("port mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let batch = port
            .next(operation_id, max_events, max_bytes)
            .await
            .map_err(core_error::napi_error_from_wire)?;
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
        Fut: std::future::Future<Output = std::result::Result<T, nexus_core::CoreError>>,
    {
        self.deny_service_only()?;
        if self.inner.is_closing() {
            return Err(Error::from_reason("closing"));
        }
        let core = self
            .inner
            .core
            .lock()
            .map_err(|_| Error::from_reason("core mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let principal = core
            .active_principal()
            .await
            .map_err(core_error::napi_error_from_domain)?;
        let encoded = EnvState::encode_principal(
            &principal,
            self.inner
                .generation
                .load(std::sync::atomic::Ordering::SeqCst),
        );
        if encoded != principal_handle {
            return Err(Error::from_reason("invalid principal handle"));
        }
        let value = f(core, principal)
            .await
            .map_err(core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&value)?))
    }
}

/// Test-only: force cleanup attempts to report unconfirmed.
#[napi]
pub fn force_unconfirmed_cleanup(enable: bool) {
    lifecycle::set_force_unconfirmed(enable);
}

#[napi]
pub fn open(env: Env, options_json: String, callbacks: Option<Object<'_>>) -> Result<NativeCore> {
    let options: NativeOpenOptions = serde_json::from_str(&options_json)?;
    let state = if let Ok(existing) = EnvInstance::get(&env) {
        existing
    } else {
        let fresh = Arc::new(EnvState::new());
        EnvInstance::install(&env, fresh.clone())?;
        fresh
    };
    let js_port = if let Some(callbacks) = callbacks {
        Some(callbacks::install_js_provider(
            &env,
            state.clone(),
            callbacks,
        )?)
    } else {
        None
    };
    runtime::runtime()
        .block_on(lifecycle::open_core(state.clone(), options, js_port))
        .map_err(core_error::napi_error_from_open_reason)?;
    Ok(NativeCore { inner: state })
}

/// Reset the product's local state (`state.db` + its WAL/SHM siblings) under
/// the trusted `home`, resolving with the number of reset stores.
///
/// The deletion runs in Rust behind the writer protocol's exclusive migration
/// fences (v1.192 P0 row 19, compass D18/D20); this binding only forwards the
/// trusted home and hands back the count or a refusal. The filesystem work is
/// offloaded so a reset cannot stall the async runtime.
///
/// # Errors
///
/// Rejects with the wire `CoreError` envelope: `owner_busy` when a live writer
/// holds a target's fence, `forbidden` when a target is a symlink or not the
/// declared file, `invalid_input` when `home` is not an absolute path, and
/// `internal` for filesystem failures or a failed worker.
#[napi]
pub async fn reset_local_state(home: String) -> Result<u32> {
    let reset = tokio::task::spawn_blocking(move || {
        nexus_local_db::reset_local_state(std::path::Path::new(&home))
    })
    .await;
    let count = match reset {
        Ok(result) => result.map_err(napi_error_from_reset)?,
        Err(_) => return Err(internal_reset_error("reset worker failed")),
    };
    u32::try_from(count).map_err(|_| internal_reset_error("reset count out of range"))
}

/// Map a local-state reset failure to the wire `CoreError` envelope.
fn wire_core_error_from_reset(err: &nexus_local_db::LocalDbError) -> CoreError {
    match err {
        nexus_local_db::LocalDbError::OwnerBusy { resource } => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message: "local state reset refused: a live writer holds the store".into(),
            details: serde_json::Map::from_iter([(
                "resource".into(),
                Value::String(resource.clone()),
            )]),
            http_status: Some(409),
        },
        nexus_local_db::LocalDbError::PathEscape { path, prefix } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: "local state reset refused: the target is not the product's own state file"
                .into(),
            details: serde_json::Map::from_iter([
                ("path".into(), Value::String(path.clone())),
                ("prefix".into(), Value::String(prefix.clone())),
            ]),
            http_status: Some(403),
        },
        nexus_local_db::LocalDbError::ValidationError(reason) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: reason.clone(),
            details: serde_json::Map::new(),
            http_status: Some(400),
        },
        nexus_local_db::LocalDbError::IoWithPath { path, .. } => CoreError {
            code: CoreErrorCode::Internal,
            message: "local state reset failed: filesystem error".into(),
            details: serde_json::Map::from_iter([("path".into(), Value::String(path.clone()))]),
            http_status: Some(500),
        },
        // The reset cannot produce another variant; refuse honestly rather than
        // leaking an unrelated diagnostic.
        _ => CoreError {
            code: CoreErrorCode::Internal,
            message: "local state reset failed".into(),
            details: serde_json::Map::new(),
            http_status: Some(500),
        },
    }
}

/// Wire rejection for a reset failure, with the native reason attached.
fn napi_error_from_reset(err: nexus_local_db::LocalDbError) -> Error {
    core_error::napi_error_from_wire(wire_core_error_from_reset(&err))
}

/// Wire rejection for a reset infrastructure failure.
fn internal_reset_error(message: &str) -> Error {
    core_error::napi_error_from_wire(CoreError {
        code: CoreErrorCode::Internal,
        message: message.to_string(),
        details: serde_json::Map::new(),
        http_status: Some(500),
    })
}
