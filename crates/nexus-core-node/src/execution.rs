//! Execution / Host control native family surface (P5-T3).
//!
//! Two lanes, both thin napi adapters over the single core authority:
//!
//! 1. **Preset authoring + Strategy edits** — the P3 `CoreService` authority
//!    (`presets.rs`). Owned JSON payloads in, generated wire DTOs out; no
//!    SQL, no second engine, no policy. Compute is deliberately NOT routed:
//!    the WASM edge is a daemon-cohort capability (architecture §Cohorts,
//!    `compute` feature never enabled on the default/domain/Connect-host
//!    cohort), so the compute run family stays an explicit 501
//!    route_not_migrated on this surface instead of a degraded fake.
//! 2. **Execution owner establishment + schedule mutations** — the P3
//!    `ExecutionHandle` (`start_execution` / `add_schedule` /
//!    `signal_schedule`). The standalone service builds `RunnerDeps` from
//!    core-side defaults (bare builtin capability registry, no daemon tool
//!    dispatch, no WASM compute runtime). The provider port is the env's
//!    injected JS-provider port: a handle can never exist without a provider
//!    contract.
//!
//! Schedule listing/inspection and core-context history have no core
//! authority yet (they remain daemon-internal); this surface deliberately
//! does not route them rather than start a second scheduler.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::generated::daemon_api::compute::run_accept_request::RunAcceptRequest;
use nexus_contracts::generated::daemon_api::compute::run_request::RunRequest;
use nexus_contracts::{
    CoreStrategyPatchResponse, GetPresetResponse, ScaffoldPresetRequest, ScaffoldPresetResponse,
    StrategyPatchPromptTemplateRequest, StrategyPatchStateRequest, StrategyPatchTransitionRequest,
    UpdatePresetRequest, UpdatePresetResponse, ValidatePresetRequest, ValidatePresetResponse,
};
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AddScheduleResponse, SignalScheduleRequest, SignalScheduleResponse,
};
use nexus_core::execution::RunnerDeps;

use crate::NativeCore;

fn decode<T: serde::de::DeserializeOwned>(payload: Buffer, label: &str) -> Result<T> {
    serde_json::from_slice(payload.as_ref())
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))
}

#[napi]
impl NativeCore {

    /// Establish the single execution owner for this engine-owner core,
    /// building `RunnerDeps` from core-side defaults and wiring the env's
    /// JS-provider port. Refuses when the core is not the execution owner
    /// (`NotEngineOwner`) or an owner already exists (`AlreadyOwned`) — the
    /// same single-owner fence the daemon boot obeys.
    #[napi]
    pub async fn start_execution_owner(&self) -> Result<Buffer> {
        self.deny_service_only()?;
        let core = self
            .inner
            .core
            .lock()
            .map_err(|_| Error::from_reason("core mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let providers = self
            .inner
            .provider_port
            .lock()
            .map_err(|_| Error::from_reason("port mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("provider port unavailable"))?;
        let handle = core
            .start_execution(providers, RunnerDeps::default())
            .await
            .map_err(|error| Error::from_reason(format!("execution owner: {error}")))?;
        Ok(Buffer::from(serde_json::to_vec(&serde_json::json!({
            "engine_epoch": handle.engine_epoch(),
        }))?))
    }

    // ── Durable schedule mutations (P3 ExecutionHandle authority) ───────────

    /// The established execution owner, or a truthful rejection naming the
    /// missing establishment (never a silently degraded write).
    fn execution_handle(&self) -> Result<std::sync::Arc<nexus_core::ExecutionHandle>> {
        self.deny_service_only()?;
        let core = self
            .inner
            .core
            .lock()
            .map_err(|_| Error::from_reason("core mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        core.execution()
            .ok_or_else(|| Error::from_reason("execution owner not started"))
    }

    /// `POST /v1/daemon/orchestration/schedules` (201 at the adapter).
    #[napi]
    pub async fn add_schedule(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: AddScheduleRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: AddScheduleResponse = handle.add_schedule(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/orchestration/schedules/{schedule_id}/signal`.
    #[napi]
    pub async fn signal_schedule(
        &self,
        principal_handle: String,
        schedule_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: SignalScheduleRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: SignalScheduleResponse =
                handle.signal_schedule(&principal, schedule_id, request).await?;
            Ok(response)
        })
        .await
    }

    // ── Preset authoring (P3 authority) ─────────────────────────────────────

    /// `GET /v1/daemon/presets`.
    #[napi]
    pub async fn list_presets(&self, principal_handle: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.list_presets(&principal).await
        })
        .await
    }

    /// `GET /v1/daemon/presets/{id}`.
    #[napi]
    pub async fn get_preset(&self, principal_handle: String, preset_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let response: GetPresetResponse = core.get_preset(&principal, preset_id).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/presets` — scaffold (201 at the adapter).
    #[napi]
    pub async fn scaffold_preset(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ScaffoldPresetRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.scaffold_preset(&principal, request).await
        })
        .await
    }

    /// `POST /v1/daemon/presets:validate`.
    #[napi]
    pub async fn validate_preset(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ValidatePresetRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: ValidatePresetResponse =
                core.validate_preset(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `PATCH /v1/daemon/presets/{id}`.
    #[napi]
    pub async fn update_preset(
        &self,
        principal_handle: String,
        preset_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: UpdatePresetRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: UpdatePresetResponse =
                core.update_preset(&principal, preset_id, request).await?;
            Ok(response)
        })
        .await
    }

    /// `DELETE /v1/daemon/presets/{id}` — 204 at the adapter.
    #[napi]
    pub async fn delete_preset(&self, principal_handle: String, preset_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_preset(&principal, preset_id).await?;
            Ok(serde_json::Value::Null)
        })
        .await
    }

    /// `GET /v1/daemon/orchestration/presets`.
    #[napi]
    pub async fn list_orchestration_presets(&self, principal_handle: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.list_orchestration_presets(&principal).await
        })
        .await
    }

    /// `GET /v1/daemon/orchestration/presets/{id}/profile`.
    #[napi]
    pub async fn get_preset_profile(
        &self,
        principal_handle: String,
        preset_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.get_preset_profile(&principal, preset_id).await
        })
        .await
    }

    // ── Strategy edits (P3 authority) ───────────────────────────────────────

    /// `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch`.
    #[napi]
    pub async fn patch_strategy_state(
        &self,
        principal_handle: String,
        strategy_id: String,
        state_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: StrategyPatchStateRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: CoreStrategyPatchResponse = core
                .patch_strategy_state(&principal, strategy_id, state_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/strategies/{strategy_id}/transitions/patch`.
    #[napi]
    pub async fn patch_strategy_transition(
        &self,
        principal_handle: String,
        strategy_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: StrategyPatchTransitionRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: CoreStrategyPatchResponse = core
                .patch_strategy_transition(&principal, strategy_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch`.
    #[napi]
    pub async fn patch_strategy_prompt_template(
        &self,
        principal_handle: String,
        strategy_id: String,
        state_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: StrategyPatchPromptTemplateRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: CoreStrategyPatchResponse = core
                .patch_strategy_prompt_template(&principal, strategy_id, state_id, request)
                .await?;
            Ok(response)
        })
        .await
    }



}
