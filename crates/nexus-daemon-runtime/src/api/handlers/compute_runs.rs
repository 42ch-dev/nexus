//! Compute run handlers — the daemon's HTTP face over the core compute lane.
//!
//! v1.190 P3-T3: the compute BODY (module resolution, sandbox invocation,
//! proposal persistence and the atomic accept transaction) moved to
//! [`nexus_core::execution::compute`]. What remains is the daemon's own
//! composition: it resolves the engine, cache and serializer from its runtime
//! state and hands the request to the core.
//!
//! # Which routes these serve
//!
//! - `POST /v1/daemon/compute/run`
//! - `POST /v1/daemon/compute/runs/:run_id/accept`
//! - `POST /v1/daemon/compute/runs/:run_id/discard`
//! - `GET  /v1/daemon/compute/runs`
//! - `GET  /v1/daemon/compute/runs/:run_id`
//! - `DELETE /v1/daemon/compute/runs`
//!
//! # Why the handlers stay here
//!
//! HTTP status codes, query-string shapes and the response envelope are the
//! transport's business. The core returns typed results and typed refusals;
//! the daemon maps them, so the domain never names a status code.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use nexus_contracts::generated::daemon_api::compute::{
    run_accept_request::RunAcceptRequest, run_accept_response::RunAcceptResponse,
    run_detail::RunDetail, run_list_response::RunListResponse, run_request::RunRequest,
    run_response::RunResponse,
};
use nexus_core::execution::compute::ComputeContext;
use nexus_local_db::compute_runs;
use serde::Deserialize;
use serde_json::{json, Value};

/// Query params for `GET /runs`.
#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    /// Restrict to one World.
    pub world_id: Option<String>,
    /// Restrict to one module.
    pub module_id: Option<String>,
    /// Restrict to one status.
    pub status: Option<String>,
    /// Opaque pagination cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

/// Query params for `DELETE /runs`.
#[derive(Debug, Deserialize)]
pub struct DeleteRunsQuery {
    /// Required scope: runs are cleared per World; the caller must own it.
    pub world_id: Option<String>,
    /// Optional terminal-state filter (`applied|discarded|failed`).
    pub status: Option<String>,
}

/// Compose the daemon's runtime state into the core compute context.
///
/// # Errors
/// `ServiceUnavailable` when this daemon was built without a WASM engine or
/// module cache — the compute lanes cannot run, and saying so is honest.
async fn compute_context(state: &WorkspaceState) -> Result<ComputeContext, NexusApiError> {
    let creator_id = crate::config::read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::AuthRequired)?;
    let Some(engine) = state.wasm_engine() else {
        return Err(NexusApiError::service_unavailable(
            "WASM compute engine is not available",
        ));
    };
    let Some(cache) = state.module_cache() else {
        return Err(NexusApiError::service_unavailable(
            "WASM module cache is not available",
        ));
    };
    Ok(ComputeContext {
        creator_id,
        engine: Some(engine),
        cache: Some(cache),
        serializer: state.compute_serializer(),
    })
}

/// `POST /v1/daemon/compute/run` — invoke a module against a World.
///
/// # Errors
/// The core compute refusal, mapped to the daemon envelope.
#[allow(clippy::missing_errors_doc)]
pub async fn run(
    State(state): State<WorkspaceState>,
    Json(request): Json<RunRequest>,
) -> Result<Json<RunResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let context = compute_context(&state).await?;
    nexus_core::execution::compute::compute_run(&core, &context, request)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

/// `POST /v1/daemon/compute/runs/:run_id/accept` — apply proposals atomically.
///
/// # Errors
/// The core accept refusal, mapped to the daemon envelope.
#[allow(clippy::missing_errors_doc)]
pub async fn accept_run(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
    Json(request): Json<RunAcceptRequest>,
) -> Result<Json<RunAcceptResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    nexus_core::execution::compute::accept_compute_run(&core, &principal, &run_id, request)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

/// `POST /v1/daemon/compute/runs/:run_id/discard` — drop proposals.
///
/// # Errors
/// The core discard refusal, mapped to the daemon envelope.
#[allow(clippy::missing_errors_doc)]
pub async fn discard_run(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
) -> Result<Json<Value>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    nexus_core::execution::compute::discard_compute_run(&core, &principal, &run_id)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(json!({ "run_id": run_id, "status": "discarded" })))
}

/// `GET /v1/daemon/compute/runs` — list runs.
///
/// # Errors
/// The core list refusal, mapped to the daemon envelope.
#[allow(clippy::missing_errors_doc)]
pub async fn list_runs_handler(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListRunsQuery>,
) -> Result<Json<RunListResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let query = nexus_core::execution::compute::ComputeRunListQuery {
        world_id: params.world_id,
        module_id: params.module_id,
        status: params.status,
        cursor: params.cursor,
        limit: params.limit,
    };
    nexus_core::execution::compute::list_compute_runs(&core, &principal, query)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

/// `GET /v1/daemon/compute/runs/:run_id` — run detail.
///
/// # Errors
/// The core detail refusal, mapped to the daemon envelope.
#[allow(clippy::missing_errors_doc)]
pub async fn get_run_detail(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunDetail>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    nexus_core::execution::compute::get_compute_run(&core, &principal, &run_id)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

/// `DELETE /v1/daemon/compute/runs` — clear terminal run history.
///
/// Clear is per-World scope (never a world-wide purge): a missing `world_id`
/// is a 422, and `running`/`succeeded` rows are never deletable — a succeeded
/// run still needs review. World ownership is guarded before any DB work.
///
/// # Errors
/// 422 for a missing scope or a non-terminal status filter; 403 for a foreign
/// World; 500 on a storage fault.
#[allow(clippy::missing_errors_doc)]
pub async fn delete_runs(
    State(state): State<WorkspaceState>,
    Query(params): Query<DeleteRunsQuery>,
) -> Result<Json<Value>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        crate::config::read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let world_id = params
        .world_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: "world_id is required to clear run history (scope is per World)".to_string(),
        })?;

    let owned = nexus_local_db::narrative_write::is_world_owned(pool, &creator_id, world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "you do not own this world".to_string(),
        });
    }

    if let Some(ref status) = params.status {
        if !matches!(
            status.as_str(),
            compute_runs::RUN_STATUS_APPLIED
                | compute_runs::RUN_STATUS_DISCARDED
                | compute_runs::RUN_STATUS_FAILED
        ) {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: format!(
                    "status '{status}' cannot be cleared: only terminal states \
                     (applied|discarded|failed) are deletable; running and succeeded \
                     (needs review) runs are kept"
                ),
            });
        }
    }

    let deleted = compute_runs::delete_terminal_runs(pool, world_id, params.status.as_deref())
        .await
        .map_err(NexusApiError::from)?;

    Ok(Json(json!({ "deleted": deleted })))
}
