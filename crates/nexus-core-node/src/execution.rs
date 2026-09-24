//! Execution / Host control native family surface (P5-T3).
//!
//! Four lanes, all thin napi adapters over the single core authority:
//!
//! 1. **Preset authoring + Strategy edits** — the P3 `CoreService` authority
//!    (`presets.rs`). Owned JSON payloads in, generated wire DTOs out; no
//!    SQL, no second engine, no policy.
//! 2. **Execution owner establishment + schedule mutations** — the P3
//!    `ExecutionHandle` (`start_execution` / `add_schedule` /
//!    `signal_schedule`). The hosted factory composes the owner's runtime
//!    edges (workspace ports, Host prompt executor, catalog, run-event
//!    registry, scheduler) including, under the `compute` cohort feature, the
//!    ONE shared WASM engine/cache/serializer bundle. The provider port is the
//!    env's injected JS-provider port: a handle can never exist without a
//!    provider contract.
//!
//! The control reads and the core-context edit (v1.195 P0-T6) are the SAME
//! `ExecutionHandle` authority as the mutations: schedule list/inspect, the
//! durable run list/detail and the core-context append all read or advance
//! rows this owner already owns. Core-context HISTORY browsing stays
//! unrouted — no producer exists for it on this owner, and this surface does
//! not invent one.
//!
//! The same-run observation family (v1.195 P1-T3) is the third lane: one
//! authorized subscription to a durable run's bounded ring plus its bounded
//! pull and its release. All three delegate to the `ExecutionHandle`
//! subscription authority — the token, the ring attachment and every cap stay
//! in Rust; nothing durable, no SQL pool and no Host handle crosses napi, and
//! no second event dialect is minted here. The frames travel exactly as the
//! ring encoded them (`id`/`event`/`data`), so the service transport may write
//! them verbatim and never renumbers.
//!
//! The Compute family (v1.195 P2-T3) is the fourth: the C1–C8 public
//! operations of current-host contracts §5 over the SAME owner. Discovery and
//! module detail read the compiled-in registry, run/detail/history/discard/
//! clear delegate to the existing compute authority on this handle, and the
//! WASM engine/cache/serializer are the ONE bundle the hosted factory
//! installed at boot — nothing here compiles a module, mints a run id or
//! touches a run row itself.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_agent_host::config::AgentHostConfig;
use nexus_agent_host::discovery::ProviderCatalog;
use nexus_agent_host::{HostFacade, HostManager};
use nexus_contracts::generated::core::{
    CoreWorkflowEventBatch, CoreWorkflowSubscribeRequest, CoreWorkflowSubscription,
};
use nexus_contracts::generated::daemon_api::compute::clear_runs_query::ClearRunsQuery;
use nexus_contracts::generated::daemon_api::compute::clear_runs_response::ClearRunsResponse;
use nexus_contracts::generated::daemon_api::compute::discard_run_response::DiscardRunResponse;
use nexus_contracts::generated::daemon_api::compute::list_modules_response::ListModulesResponse;
use nexus_contracts::generated::daemon_api::compute::list_runs_query::ListRunsQuery;
use nexus_contracts::generated::daemon_api::compute::module_detail::ModuleDetail;
use nexus_contracts::generated::daemon_api::compute::run_accept_request::RunAcceptRequest;
use nexus_contracts::generated::daemon_api::compute::run_accept_response::RunAcceptResponse;
use nexus_contracts::generated::daemon_api::compute::run_detail::RunDetail;
use nexus_contracts::generated::daemon_api::compute::run_list_response::RunListResponse;
use nexus_contracts::generated::daemon_api::compute::run_request::RunRequest;
use nexus_contracts::generated::daemon_api::compute::run_response::RunResponse;
use nexus_contracts::generated::daemon_api::orchestration::sessions::list_sessions_query::ListSessionsQuery;
use nexus_contracts::generated::daemon_api::orchestration::sessions::list_sessions_response::ListSessionsResponse;
use nexus_contracts::generated::daemon_api::orchestration::sessions::session_detail_response::SessionDetailResponse;
use nexus_contracts::generated::daemon_api::schedule::edit_core_context_request::EditCoreContextRequest;
use nexus_contracts::generated::daemon_api::schedule::edit_core_context_response::EditCoreContextResponse;
use nexus_contracts::generated::daemon_api::schedule::inspect_schedule_response::InspectScheduleResponse;
use nexus_contracts::generated::daemon_api::schedule::list_schedules_query::ListSchedulesQuery;
use nexus_contracts::generated::daemon_api::schedule::list_schedules_response::ListSchedulesResponse;
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AddScheduleResponse, SignalScheduleRequest, SignalScheduleResponse,
};
use nexus_contracts::{
    CoreStrategyPatchResponse, GetPresetResponse, ScaffoldPresetRequest,
    StrategyPatchPromptTemplateRequest, StrategyPatchStateRequest, StrategyPatchTransitionRequest,
    UpdatePresetRequest, UpdatePresetResponse, ValidatePresetRequest, ValidatePresetResponse,
};
use nexus_core::execution::ExecutionOpenError;
use nexus_core::CoreError;

use crate::core_error;
use crate::NativeCore;

fn decode<T: serde::de::DeserializeOwned>(payload: Buffer, label: &str) -> Result<T> {
    serde_json::from_slice(payload.as_ref())
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))
}

/// Whether every provider this host configuration SELECTS is available.
///
/// The selection is the enabled provider set of the Host's own configuration —
/// the same document admission reads — in configuration order, so readiness is
/// never decided by a discovery/catalog row order and never by a provider the
/// operator did not select. An EMPTY selection is not ready: a catalog
/// candidate nobody selected can be a candidate, never a readiness claim.
///
/// `available` is the bounded owner-bound probe result the Host published at
/// open (`probe_all_providers`); a provider that was never probed — the
/// owner-less start that marks every row `probe_context_unavailable` — reports
/// unavailable and therefore not ready.
async fn selected_providers_ready(host: &HostManager) -> std::result::Result<bool, String> {
    let config = host.agent_config().await;
    let catalog = host
        .provider_catalog()
        .await
        .map_err(|e| format!("provider catalog: {e}"))?;
    Ok(selected_providers_available(&config, &catalog))
}

fn selected_providers_available(config: &AgentHostConfig, catalog: &ProviderCatalog) -> bool {
    let selected: Vec<_> = config.providers.iter().filter(|pc| pc.enabled).collect();
    if selected.is_empty() {
        return false;
    }
    selected.iter().all(|pc| {
        catalog
            .find(&pc.provider_id())
            .is_some_and(|entry| entry.health.available)
    })
}

#[napi]
impl NativeCore {
    /// Establish the single hosted execution owner for this engine-owner core.
    ///
    /// This is the production boot of current-host contracts §3.1: the ONE
    /// core-owned `start_hosted_execution` factory composes the complete
    /// hosted owner (selected-root workspace bundle and its settled startup
    /// recovery, the prompt executor over the ONE already-owned Host, the
    /// provider catalog, the run-event registry, the shared cancellation map
    /// and the hosted scheduler) and reports the facts the caller must publish
    /// rather than defaults:
    ///
    /// - `engine_epoch` is the ACTUAL `ExecutionHandle::engine_epoch()` of the
    ///   established owner, or `null` when this profile cannot host one (the
    ///   selected workspace registers no creative root, so the factory's
    ///   workspace composition refuses with `uninitialized`). A refusal that
    ///   carries a workspace `CoreError` — a selected root that moved after this
    ///   admission was pinned, a rival commit authority, a storage fault —
    ///   keeps that typed class on the wire. The other three owner refusals —
    ///   not the engine owner, an owner already exists, closing — carry NO typed
    ///   code: they cross as an unstructured native reason, which the service
    ///   reports as its generic `internal` failure. Either way a duplicate or
    ///   stale boot is a hard failure, never a quiet success.
    /// - `provider_ready` is the native-owned readiness of the providers this
    ///   host configuration SELECTS, read from the SAME Host that ran the
    ///   bounded owner-bound probes at open. Catalog presence is a candidate,
    ///   never readiness: a provider without a successful probe (or with no
    ///   selection at all) is not ready.
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
        let host = self
            .inner
            .host
            .lock()
            .map_err(|_| Error::from_reason("host mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("host not started"))?;
        // The readiness answer is read BEFORE establishment so a workspace
        // refusal still reports the truth about the provider lane.
        let provider_ready = selected_providers_ready(&host)
            .await
            .map_err(|error| Error::from_reason(format!("provider readiness: {error}")))?;
        let timeouts = host.agent_config().await.timeouts;
        let engine_epoch = match core.start_hosted_execution(host, providers, timeouts).await {
            Ok(handle) => Some(handle.engine_epoch()),
            // A workspace that cannot host an owner is the ONE shape this
            // profile reports as "no engine epoch": the admission pinned no
            // usable creative root, so the caller publishes the null sentinel.
            Err(ExecutionOpenError::Workspace(CoreError::Uninitialized)) => None,
            // Every other workspace refusal — a selected root that moved since
            // the admission was pinned, a rival commit authority, a storage
            // fault — keeps its neutral class on the wire instead of decaying
            // into an unstructured reason the caller can only read as
            // "internal".
            Err(ExecutionOpenError::Workspace(err)) => {
                return Err(core_error::napi_error_from_domain(err));
            }
            Err(error) => {
                return Err(Error::from_reason(format!("execution owner: {error}")));
            }
        };
        Ok(Buffer::from(serde_json::to_vec(&serde_json::json!({
            "engine_epoch": engine_epoch,
            "provider_ready": provider_ready,
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
            let response: SignalScheduleResponse = handle
                .signal_schedule(&principal, schedule_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    // ── Durable control reads + core-context edits (P0-T6) ──────────────────

    /// `GET /v1/daemon/orchestration/schedules`.
    ///
    /// The query crosses as the generated public DTO (`deny_unknown_fields`:
    /// a key outside the schema is a typed client refusal), and the response is
    /// the generated snake_case page. Scope and pagination stay the core
    /// owner's: this adapter adds no filter and no default.
    #[napi]
    pub async fn list_schedules(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListSchedulesQuery = decode(query_json, "query")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: ListSchedulesResponse = handle.list_schedules(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/orchestration/schedules/{schedule_id}`.
    #[napi]
    pub async fn inspect_schedule(
        &self,
        principal_handle: String,
        schedule_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: InspectScheduleResponse =
                handle.inspect_schedule(&principal, schedule_id).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/orchestration/sessions`.
    #[napi]
    pub async fn list_workflow_sessions(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListSessionsQuery = decode(query_json, "query")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: ListSessionsResponse =
                handle.list_workflow_sessions(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/orchestration/sessions/{run_id}`.
    #[napi]
    pub async fn get_workflow_session(
        &self,
        principal_handle: String,
        session_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: SessionDetailResponse =
                handle.get_workflow_session(&principal, session_id).await?;
            Ok(response)
        })
        .await
    }

    /// `PATCH /v1/daemon/orchestration/schedules/{schedule_id}/core-context`.
    #[napi]
    pub async fn edit_core_context(
        &self,
        principal_handle: String,
        schedule_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: EditCoreContextRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: EditCoreContextResponse = handle
                .edit_core_context(&principal, schedule_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    // ── Compute module/run/transaction surface (v1.195 P2-T3) ───────────────

    /// `GET /v1/daemon/compute/modules` — the installed module registry (C1).
    ///
    /// Machine capability rather than creator state, so there is no
    /// per-creator filter — but the principal is still verified, and the
    /// module DETAIL this lists carries the invocation schema Run Studio
    /// renders straight from the shipped manifest (TS never manufactures one).
    #[napi]
    pub async fn list_compute_modules(&self, principal_handle: String) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: ListModulesResponse = handle.list_compute_modules(&principal)?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/compute/modules/{module_id}` — one module's manifest (C2).
    #[napi]
    pub async fn get_compute_module(
        &self,
        principal_handle: String,
        module_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: ModuleDetail = handle.get_compute_module(&principal, module_id)?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/compute/run` — invoke a module against an owned World (C3).
    ///
    /// The run reaches the WASM authority the hosted factory installed, applies
    /// the existing input/fuel/memory/wall-clock limits, and persists proposals
    /// (or the honest failure) WITHOUT mutating the World — the caller reviews
    /// and then accepts or discards.
    #[napi]
    pub async fn compute_run(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: RunRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: RunResponse = handle.compute_run(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/compute/runs/{run_id}` — one run's proposals or error (C4).
    #[napi]
    pub async fn get_compute_run(
        &self,
        principal_handle: String,
        run_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: RunDetail = handle.get_compute_run(&principal, run_id).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/compute/runs` — the creator's run history page (C5).
    ///
    /// The query crosses as the generated DTO (`deny_unknown_fields` plus the
    /// closed status enum: a key or value outside the schema is a typed client
    /// refusal), and scope stays the core owner's — this adapter adds no filter
    /// and no default.
    #[napi]
    pub async fn list_compute_runs(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListRunsQuery = decode(query_json, "query")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: RunListResponse = handle.list_compute_runs(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/compute/runs/{run_id}/accept` — commit once (C6).
    ///
    /// The generated request carries the optional `evt_<index>` selection; the
    /// ONE domain transaction, its CAS and its rollback stay in the core.
    #[napi]
    pub async fn accept_compute_run(
        &self,
        principal_handle: String,
        run_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: RunAcceptRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: RunAcceptResponse = handle
                .accept_compute_run(&principal, run_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/compute/runs/{run_id}/discard` — drop the proposals (C7).
    #[napi]
    pub async fn discard_compute_run(
        &self,
        principal_handle: String,
        run_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: DiscardRunResponse =
                handle.discard_compute_run(&principal, run_id).await?;
            Ok(response)
        })
        .await
    }

    /// `DELETE /v1/daemon/compute/runs?world_id=…` — World-scoped clear (C8).
    ///
    /// The generated query makes `world_id` required and admits only terminal
    /// status values, so a missing scope or an out-of-vocabulary filter is
    /// refused at the transport; the terminal-only, direct-lane predicate stays
    /// the storage authority, and a foreign World refuses before any row is
    /// touched.
    #[napi]
    pub async fn clear_compute_runs(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ClearRunsQuery = decode(query_json, "query")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: ClearRunsResponse = handle.clear_compute_runs(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    // ── Same-run event subscriptions (P1-T3) ────────────────────────────────

    /// `GET /v1/daemon/orchestration/sessions/{run_id}/events` — open ONE
    /// authorized subscription to a durable root run's bounded event stream.
    ///
    /// The generated request DTO crosses as an owned buffer (`deny_unknown_fields`:
    /// a key outside the schema is a typed client refusal) and carries the
    /// `Last-Event-ID` cursor verbatim — the transport forwards the header, it
    /// never parses or renumbers it. The core resolves the run's STORED
    /// ownership before any ring, epoch or cursor is consulted, so an absent,
    /// foreign or child run closes with `not_found` (the adapter's 404 before
    /// any SSE header) and a malformed/future cursor with `invalid_input`.
    ///
    /// An unresumable history (prior epoch, evicted ring, restart) is NOT an
    /// error: the returned subscription carries the single `history_unavailable`
    /// control frame and reports `closed` on its first pull.
    #[napi]
    pub async fn subscribe_workflow_events(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CoreWorkflowSubscribeRequest = decode(request_json, "request")?;
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: CoreWorkflowSubscription = handle
                .subscribe_workflow_events(&principal, request)
                .await?;
            Ok(response)
        })
        .await
    }

    /// Take the next bounded batch (`<= 16` frames / 1 MiB) of a subscribed
    /// run's stream. One pull may be outstanding per subscription, and a
    /// released, foreign or stale-generation token refuses as `not_found`.
    /// `closed` reports that the stream ended at this batch.
    #[napi]
    pub async fn next_workflow_events(
        &self,
        principal_handle: String,
        subscription_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            let response: CoreWorkflowEventBatch = handle
                .next_workflow_events(&principal, subscription_id)
                .await?;
            Ok(response)
        })
        .await
    }

    /// Release one subscription — the transport's disconnect/exit cleanup.
    ///
    /// A pull blocked on the released token wakes with `closed`, so a
    /// disconnect can never leave the run's subscriber permit held. Releasing
    /// twice (the transport's `close` handler plus its `finally`) is not an
    /// error the caller must handle: the second call closes with `not_found`,
    /// exactly like any other released token.
    #[napi]
    pub async fn release_workflow_events(
        &self,
        principal_handle: String,
        subscription_id: String,
    ) -> Result<Buffer> {
        let handle = self.execution_handle()?;
        self.json_call(principal_handle, async move |core, principal| {
            let _ = &core;
            handle
                .release_workflow_events(&principal, subscription_id)
                .await?;
            Ok(serde_json::Value::Null)
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
    pub async fn delete_preset(
        &self,
        principal_handle: String,
        preset_id: String,
    ) -> Result<Buffer> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agent_host::capability::model::{ProtocolKind, ProviderHealth};
    use nexus_agent_host::config::ProviderConfig;
    use nexus_agent_host::{DiscoverySource, LaunchStrategy, ProviderId, TrustLevel};

    fn configured(id: &str, enabled: bool) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            protocol: "acp".to_string(),
            command: Some("/bin/true".to_string()),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            enabled,
        }
    }

    /// A catalog row whose bounded probe already published `available`.
    fn entry(id: &str, available: bool) -> nexus_agent_host::ProviderCatalogEntry {
        let provider_id = ProviderId::new(id.to_string());
        nexus_agent_host::ProviderCatalogEntry {
            provider_id: provider_id.clone(),
            display_name: id.to_string(),
            protocol_kind: ProtocolKind::Acp,
            launch: LaunchStrategy::Acp {
                command: "/bin/true".to_string(),
                args: Vec::new(),
                env: std::collections::HashMap::new(),
            },
            source: DiscoverySource::Config,
            trust: TrustLevel::Explicit,
            capabilities: nexus_agent_host::capability::model::CapabilityDescriptor::acp_full(),
            health: ProviderHealth {
                provider_id,
                available,
                latency_ms: None,
                message: None,
            },
        }
    }

    fn catalog(entries: Vec<nexus_agent_host::ProviderCatalogEntry>) -> ProviderCatalog {
        ProviderCatalog { entries }
    }

    #[test]
    fn unselected_catalog_presence_is_never_readiness() {
        // A perfectly available catalog with no selected provider: the profile
        // has nothing it can dispatch to, so it is not ready.
        let config = AgentHostConfig::default();
        assert!(!selected_providers_available(
            &config,
            &catalog(vec![entry("dsh-native", true)])
        ));
    }

    #[test]
    fn unprobed_or_unavailable_selected_provider_is_not_ready() {
        // Selected but never probed (the owner-less start marks the row
        // unavailable) — or probed and unavailable: both are not ready.
        let config = AgentHostConfig {
            providers: vec![configured("dsh-native", true)],
            ..AgentHostConfig::default()
        };
        assert!(!selected_providers_available(
            &config,
            &catalog(vec![entry("dsh-native", false)])
        ));
        assert!(!selected_providers_available(
            &config,
            &catalog(vec![entry("mock-acp", true)])
        ));
        assert!(selected_providers_available(
            &config,
            &catalog(vec![entry("dsh-native", true)])
        ));
    }

    #[test]
    fn disabled_selection_does_not_pass_and_partial_selection_does_not_pass() {
        // A disabled entry is not a selection: nothing is selected.
        let only_disabled = AgentHostConfig {
            providers: vec![configured("dsh-native", false)],
            ..AgentHostConfig::default()
        };
        assert!(!selected_providers_available(
            &only_disabled,
            &catalog(vec![entry("dsh-native", true)])
        ));
        // Every selected provider must be available: one unrelated healthy row
        // never substitutes for an unavailable selected one.
        let both = AgentHostConfig {
            providers: vec![configured("dsh-native", true), configured("mock-acp", true)],
            ..AgentHostConfig::default()
        };
        assert!(!selected_providers_available(
            &both,
            &catalog(vec![entry("dsh-native", false), entry("mock-acp", true)])
        ));
        assert!(selected_providers_available(
            &both,
            &catalog(vec![entry("dsh-native", true), entry("mock-acp", true)])
        ));
    }
}
