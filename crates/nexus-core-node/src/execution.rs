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
//! The control reads and the core-context edit (v1.195 P0-T6) are the SAME
//! `ExecutionHandle` authority as the mutations: schedule list/inspect, the
//! durable run list/detail and the core-context append all read or advance
//! rows this owner already owns. Core-context HISTORY browsing stays
//! unrouted — no producer exists for it on this owner, and this surface does
//! not invent one.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_agent_host::config::AgentHostConfig;
use nexus_agent_host::discovery::ProviderCatalog;
use nexus_agent_host::{HostFacade, HostManager};
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
    ///   workspace composition refuses with `uninitialized`). Every other
    ///   refusal — not the engine owner, an owner already exists, closing, a
    ///   selected root that moved after this admission was pinned, a rival
    ///   commit authority — propagates as its own typed wire error: a duplicate
    ///   or stale boot must never look like a quiet success.
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
        let engine_epoch = match core
            .start_hosted_execution(host, providers, timeouts)
            .await
        {
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
    use nexus_agent_host::capability::model::{ProviderHealth, ProtocolKind};
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
