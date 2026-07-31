//! Compute run handlers — direct Control Room lane (V1.147 P0).
//!
//! Five routes:
//! - `POST /v1/daemon/compute/run` — invoke a WASM module against a World
//! - `POST /v1/daemon/compute/runs/:run_id/accept` — atomically apply proposals
//! - `POST /v1/daemon/compute/runs/:run_id/discard` — discard proposals
//! - `GET  /v1/daemon/compute/runs` — cursor-paginated list
//! - `GET  /v1/daemon/compute/runs/:run_id` — detail with proposals
//!
//! Error conventions: all handlers return `NexusApiError` via `?` propagation.
//! Individual error variants are documented in the route spec §4.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use nexus_contracts::generated::daemon_api::compute::{
    run_accept_request::RunAcceptRequest,
    run_accept_response::RunAcceptResponse,
    run_detail::RunDetail,
    run_list_response::{NexusRunSummary, NexusRunSummaryStatus, RunListResponse},
    run_request::RunRequest,
    run_response::RunResponse,
};
use nexus_local_db::compute_runs::{self, list_runs, RunListFilters};
use nexus_local_db::narrative_write;
use nexus_orchestration::compute_input_builder::ComputeInputBuilder;
use nexus_orchestration::state_delta;
use nexus_wasm_host::{ComputeError, WasmEngine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Maximum HTTP response size (1 MiB) before truncation.
const RESPONSE_BYTE_CAP: usize = 1024 * 1024;

fn resolve_engine(state: &WorkspaceState) -> Result<Arc<WasmEngine>, NexusApiError> {
    state
        .wasm_engine()
        .ok_or_else(|| NexusApiError::service_unavailable("WASM compute engine is not available"))
}

fn resolve_cache(
    state: &WorkspaceState,
) -> Result<Arc<nexus_wasm_host::ModuleCache>, NexusApiError> {
    state
        .module_cache()
        .ok_or_else(|| NexusApiError::service_unavailable("WASM module cache is not available"))
}

/// Query params for `GET /runs`.
#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pub world_id: Option<String>,
    pub module_id: Option<String>,
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

// ── POST /v1/daemon/compute/run ──────────────────────────────────────────
///
/// # Errors
///
/// Returns 403 when the world is not owned, 404 when the module is not found,
/// 422 for compute errors (fuel, wall time, memory, trap, module error), 422
/// when no computable entries exist, and 500 for internal failures.
/// # Panics
///
/// Panics if `RESPONSE_BYTE_CAP` cannot be represented as a `usize` (never —
/// the constant is 1 MiB).
#[allow(clippy::missing_errors_doc, clippy::too_many_lines)]
pub async fn run(
    State(state): State<WorkspaceState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Ownership gate.
    let owned = narrative_write::is_world_owned(pool, &creator_id, &req.world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {}", req.world_id),
            reason: "you do not own this world".to_string(),
        });
    }

    // Resolve compiled module.
    let cache = resolve_cache(&state)?;
    let cached = cache
        .get(&req.module_id)
        .ok_or_else(|| NexusApiError::NotFound(format!("module '{}' not found", req.module_id)))?;
    let module = cached.module.clone();
    let module_version = cached.manifest.version.clone();
    let manifest = cached.manifest.clone();

    // F-002: resolve the run's branch BEFORE assembling input so the module
    // sees exactly the position that is snapshotted onto the run row.
    // Defaults to the World root branch; unknown/other-world branch → 422.
    let (branch_id, timeline_head_event_id) =
        resolve_run_branch(pool, &req.world_id, req.branch_id.as_deref()).await?;

    // Build ComputeInput.
    let invocation_params = req.invocation_params.clone();
    let invocation_params_str = serde_json::to_string(&invocation_params).ok();
    let builder =
        ComputeInputBuilder::new(pool.clone(), &req.world_id, manifest, invocation_params)
            .with_narrative_position(branch_id.clone(), timeline_head_event_id.clone());
    let compute_input = builder.build().await.map_err(map_build_error)?;

    // Insert run row (F-003: snapshot branch + timeline head).
    let run_id = compute_runs::insert_run(
        pool,
        &req.world_id,
        &req.module_id,
        Some(&module_version),
        Some(&branch_id),
        timeline_head_event_id.as_deref(),
        invocation_params_str.as_deref(),
    )
    .await
    .map_err(NexusApiError::from)?;

    // Execute compute.
    // W-1: `engine.compute` is CPU-bound for up to the wall-time budget —
    // run it on the blocking pool (repo convention), never inline on an
    // async worker (a long run would stall the whole daemon HTTP runtime).
    // W-2: the wasmtime epoch counter is engine-GLOBAL — the first watchdog
    // to fire traps every concurrent invocation at the shortest budget.
    // The daemon-wide `Semaphore(1)` serializes invocations so each run's
    // watchdog only ever observes its own budget.
    let engine = resolve_engine(&state)?;
    let permit = state
        .compute_serializer()
        .acquire_owned()
        .await
        .map_err(|_| NexusApiError::Internal {
            code: "COMPUTE_SERIALIZER".to_string(),
            message: "compute serializer closed".to_string(),
        })?;
    let engine_task = engine.clone();
    let module_task = module.clone();
    let manifest_task = cached.manifest.clone();
    let input_task = compute_input.clone();
    let compute_result = match tokio::task::spawn_blocking(move || {
        // The permit is held for the whole invocation (dropped when compute
        // returns) — a queued run only arms its watchdog after the previous
        // run has fully reaped its own.
        let _permit = permit;
        engine_task.compute(&module_task, &manifest_task, &input_task)
    })
    .await
    {
        Ok(result) => result,
        Err(join_err) => {
            // qc3 N-1: the blocking task panicked — best-effort compensating
            // persist so the row is not stuck in 'running' forever (mirrors
            // F-006's compensation when set_run_succeeded persistence fails;
            // retries on the same run_id are forbidden by the status guard).
            let compensation_error = serde_json::to_string(&json!({
                "code": "internal",
                "message": format!("compute task join failed: {join_err}"),
            }))
            .unwrap_or_else(|_| r#"{"code":"internal"}"#.to_string());
            if let Err(db_err) =
                compute_runs::set_run_failed(pool, &run_id, &compensation_error).await
            {
                tracing::error!(
                    run_id = %run_id,
                    error = %db_err,
                    "compensating set_run_failed after compute task join failure also failed"
                );
            }
            return Err(NexusApiError::Internal {
                code: "COMPUTE_TASK_JOIN".to_string(),
                message: format!("compute task join failed: {join_err}"),
            });
        }
    };

    let output = match compute_result {
        Ok(o) => o,
        Err(ComputeError::InputValidationFailed(entries)) => {
            // V1.147 P3 F2 (stat-less poison → 500): manifest validation
            // failures are a client-input problem, not an internal fault.
            // The run still fails (row persisted Failed) and the HTTP error is
            // an honest 422 `invalid_input` with per-entry detail (entry id +
            // reason) — invalid entries are never silently skipped.
            let entries_value = serde_json::to_value(&entries).unwrap_or_else(|_| json!([]));
            let error_json = serde_json::to_string(&json!({
                "code": "invalid_input",
                "message": format!(
                    "compute input validation failed: {} invalid entry(ies); the run was not applied",
                    entries.len()
                ),
                "invalid_entries": entries_value,
            }))
            .unwrap_or_else(|_| r#"{"code":"invalid_input"}"#.to_string());
            if let Err(db_err) = compute_runs::set_run_failed(pool, &run_id, &error_json).await {
                tracing::error!(
                    run_id = %run_id,
                    error = %db_err,
                    "failed to persist set_run_failed on input validation failure"
                );
            }
            return Err(NexusApiError::InputValidationFailed {
                details: json!({ "invalid_entries": entries_value }),
            });
        }
        Err(e) => {
            let error_code = compute_error_code(&e);
            let error_json = serde_json::to_string(&json!({
                "code": error_code,
                "message": e.to_string(),
            }))
            .unwrap_or_else(|_| format!("{{\"message\":\"{e}\"}}"));
            // Persist the failed row regardless of HTTP outcome.
            if let Err(db_err) = compute_runs::set_run_failed(pool, &run_id, &error_json).await {
                tracing::error!(
                    run_id = %run_id,
                    error = %db_err,
                    "failed to persist set_run_failed on compute error"
                );
            }

            // Route spec §2.1 step 6 + §4: sandbox errors → 422, internal → 500.
            if error_code == "internal" {
                return Err(NexusApiError::Internal {
                    code: "COMPUTE_ERROR".to_string(),
                    message: e.to_string(),
                });
            }
            return Err(NexusApiError::BadRequest {
                code: error_code.to_string(),
                message: e.to_string(),
            });
        }
    };

    let proposals_json = serde_json::to_string(&output).map_err(|e| NexusApiError::Internal {
        code: "SERIALIZATION_ERROR".to_string(),
        message: format!("serialize compute output: {e}"),
    })?;
    if let Err(db_err) = compute_runs::set_run_succeeded(pool, &run_id, &proposals_json).await {
        // F-006: compensating failure persist so the row is not stuck in
        // 'running' forever (it can never be accepted and retries on the
        // same run_id are forbidden by the status guard).
        let compensation_error = serde_json::to_string(&json!({
            "code": "internal",
            "message": format!("failed to persist succeeded outcome: {db_err}"),
        }))
        .unwrap_or_else(|_| r#"{"code":"internal"}"#.to_string());
        if let Err(comp_err) =
            compute_runs::set_run_failed(pool, &run_id, &compensation_error).await
        {
            tracing::error!(
                run_id = %run_id,
                error = %comp_err,
                "compensating set_run_failed after set_run_succeeded failure also failed"
            );
        }
        return Err(NexusApiError::from(db_err));
    }

    let proposals_raw: Value = serde_json::from_str(&proposals_json).unwrap_or(Value::Null);

    // Build the full RunResponse and measure its serialized size against the
    // 1 MiB response cap (plan Global Constraints).  When over the cap,
    // truncate all 4 parts of the proposals payload; the full output remains
    // in `proposals_json` (persisted in the session row).
    let created_at = chrono::Utc::now();
    let resp_candidate: RunResponse = serde_json::from_value(json!({
        "run_id": &run_id,
        "status": "succeeded",
        "module_id": &req.module_id,
        "module_version": &module_version,
        "created_at": created_at.to_rfc3339(),
        "proposals": &proposals_raw,
    }))
    .map_err(|err| NexusApiError::Internal {
        code: "SERIALIZATION_ERROR".to_string(),
        message: format!("build run response: {err}"),
    })?;

    let resp_bytes = serde_json::to_vec(&resp_candidate).unwrap_or_default();
    if resp_bytes.len() > RESPONSE_BYTE_CAP {
        let truncated_proposals = build_truncated_proposals();
        let resp: RunResponse = serde_json::from_value(json!({
            "run_id": run_id,
            "status": "succeeded",
            "module_id": req.module_id,
            "module_version": module_version,
            "truncated": true,
            "created_at": created_at.to_rfc3339(),
            "proposals": truncated_proposals,
        }))
        .map_err(|err| NexusApiError::Internal {
            code: "SERIALIZATION_ERROR".to_string(),
            message: format!("build truncated run response: {err}"),
        })?;
        return Ok(Json(resp));
    }

    Ok(Json(resp_candidate))
}

// ── POST /v1/daemon/compute/runs/:run_id/accept ──────────────────────────

#[allow(clippy::missing_errors_doc, clippy::too_many_lines)]
pub async fn accept_run(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
    Json(req): Json<RunAcceptRequest>,
) -> Result<Json<RunAcceptResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let run = compute_runs::get_run(pool, &run_id)
        .await
        .map_err(NexusApiError::from)?
        .ok_or_else(|| NexusApiError::NotFound(format!("run {run_id} not found")))?;

    // Ownership gate FIRST (plan Global Constraints) — before any status
    // pre-check, so a foreign run never leaks lifecycle state (403, not 422).
    let owned = narrative_write::is_world_owned(pool, &creator_id, &run.world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {}", run.world_id),
            reason: "you do not own the world this run targets".to_string(),
        });
    }

    match run.status.as_str() {
        "succeeded" => {}
        "applied" => {
            return Err(NexusApiError::Conflict(format!(
                "run {run_id} has already been accepted"
            )));
        }
        "discarded" => {
            return Err(NexusApiError::Conflict(format!(
                "run {run_id} has already been discarded"
            )));
        }
        _ => {
            return Err(NexusApiError::BadRequest {
                code: "invalid_state".to_string(),
                message: format!(
                    "run {run_id} is in status '{}', must be 'succeeded' to accept",
                    run.status
                ),
            });
        }
    }

    let proposals_json = run.proposals_json.as_deref().unwrap_or("{}");
    let output: nexus_wasm_host::ComputeOutput =
        serde_json::from_str(proposals_json).map_err(|e| NexusApiError::Internal {
            code: "DESERIALIZATION_ERROR".to_string(),
            message: format!("parse run proposals: {e}"),
        })?;

    // F-003: Accept appends timeline events to the branch SNAPSHOTTED at run
    // time (not the current fork head — the fork may have changed between
    // run and accept).  Pre-fix rows have NULL `branch_id`; fall back to the
    // legacy constant to preserve their behavior exactly.
    let branch_id = run
        .branch_id
        .clone()
        .unwrap_or_else(|| "fbk_root".to_string());

    // W2: subset-accept — when `timeline_event_ids_to_accept` is present,
    // only the referenced proposed events are appended.  Stable ids are the
    // index-based `evt_<index>` assigned by position in the proposals'
    // `timeline_events` array.  Unknown ids reject the whole Accept (422)
    // BEFORE any write.  Absent/null → accept all (N1: the wire field is
    // nullable `["array", "null"]`; an explicit JSON `null` must deserialize
    // and behave exactly like an absent field).  Empty array → accept all.
    let selected_event_indices: Option<std::collections::HashSet<usize>> =
        match req.timeline_event_ids_to_accept.as_deref() {
            None | Some([]) => None,
            Some(ids) => {
                let mut selected = std::collections::HashSet::with_capacity(ids.len());
                for id in ids {
                    let index = id
                        .strip_prefix("evt_")
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|i| *i < output.timeline_events.len());
                    match index {
                        Some(i) => {
                            selected.insert(i);
                        }
                        None => {
                            return Err(NexusApiError::BadRequest {
                                code: "invalid_input".to_string(),
                                message: format!(
                                    "timeline_event_ids_to_accept references unknown event id \
                                     '{id}' (proposals contain {} timeline events; ids are \
                                     'evt_0'..'evt_{}')",
                                    output.timeline_events.len(),
                                    output.timeline_events.len().saturating_sub(1)
                                ),
                            });
                        }
                    }
                }
                Some(selected)
            }
        };

    let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("begin transaction: {e}"),
    })?;

    let accepted_at = chrono::Utc::now().to_rfc3339();

    // F-001: every delta target must resolve inside the run's world — a
    // foreign target rejects the whole Accept (full rollback on drop).
    let state_delta_count =
        state_delta::apply_state_delta_in_tx(&mut tx, &run.world_id, &output.state_delta)
            .await
            .map_err(map_delta_error)?;

    let new_entries_created =
        create_key_blocks_in_tx(&mut tx, &run.world_id, &output.new_key_blocks).await?;

    // Build compute provenance per plan Global Constraints:
    // {"compute": {"module_id", "module_version", "run_id", "source_kind": "direct_invoke"}}
    let provenance = serde_json::to_string(&json!({
        "compute": {
            "module_id": run.module_id,
            "module_version": run.module_version,
            "run_id": run_id,
            "source_kind": "direct_invoke",
        }
    }))
    .map_err(|e| NexusApiError::Internal {
        code: "SERIALIZATION_ERROR".to_string(),
        message: format!("build compute provenance: {e}"),
    })?;

    let events_created = selected_event_indices
        .as_ref()
        .map_or(output.timeline_events.len(), std::collections::HashSet::len);
    let mut timeline_event_ids = Vec::with_capacity(events_created);
    for (index, evt) in output.timeline_events.iter().enumerate() {
        if let Some(ref selected) = selected_event_indices {
            if !selected.contains(&index) {
                continue;
            }
        }
        let event_type = "compute_result";
        // Canon (not provisional): an accepted Run is author-committed world
        // truth and the P2 Timeline projection reads `canon` events only —
        // dogfood AC-I3.3 (node must appear on the Narrative layer after
        // Accept). Discard/failed paths never reach this write.
        // QC S-affected: persist the proposal's affected entries so the P2
        // inspector's "Affected knowledge" section resolves them against the
        // KB graph (empty proposals → NULL column, honest).
        let affected_json = if evt.affected_key_block_ids.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&evt.affected_key_block_ids).map_err(|e| {
                    NexusApiError::Internal {
                        code: "SERIALIZATION_ERROR".to_string(),
                        message: format!("serialize affected_key_block_ids: {e}"),
                    }
                })?,
            )
        };
        let result = narrative_write::append_event_canon_with_extensions_in_tx(
            &mut tx,
            &run.world_id,
            &branch_id,
            event_type,
            evt.title.as_deref().map(std::string::String::as_str),
            evt.summary.as_deref(),
            &provenance,
            affected_json.as_deref(),
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("append timeline event: {e}"),
        })?;
        timeline_event_ids.push(result.event_id);
    }

    compute_runs::set_run_applied_in_tx(&mut tx, &run_id, &accepted_at)
        .await
        .map_err(|e| {
            if matches!(e, nexus_local_db::LocalDbError::ConstraintViolation { .. }) {
                NexusApiError::Conflict(format!("run {run_id} has already been accepted"))
            } else {
                NexusApiError::from(e)
            }
        })?;

    tx.commit().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("commit: {e}"),
    })?;

    let resp: RunAcceptResponse = serde_json::from_value(json!({
        "run_id": run_id,
        "status": "applied",
        "applied": {
            "state_delta_count": state_delta_count,
            "events_created": events_created,
            "new_entries_created": new_entries_created,
        },
        "timeline_event_ids": timeline_event_ids,
    }))
    .map_err(|err| NexusApiError::Internal {
        code: "SERIALIZATION_ERROR".to_string(),
        message: format!("build accept response: {err}"),
    })?;

    Ok(Json(resp))
}

// ── POST /v1/daemon/compute/runs/:run_id/discard ────────────────────────

#[allow(clippy::missing_errors_doc)]
pub async fn discard_run(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
) -> Result<Json<Value>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let run = compute_runs::get_run(pool, &run_id)
        .await
        .map_err(NexusApiError::from)?
        .ok_or_else(|| NexusApiError::NotFound(format!("run {run_id} not found")))?;

    let owned = narrative_write::is_world_owned(pool, &creator_id, &run.world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {}", run.world_id),
            reason: "you do not own the world this run targets".to_string(),
        });
    }

    compute_runs::set_run_discarded(pool, &run_id)
        .await
        .map_err(|e| {
            if matches!(&e, nexus_local_db::LocalDbError::ConstraintViolation { .. }) {
                NexusApiError::Conflict(format!("run {run_id} is not in 'succeeded' status"))
            } else {
                NexusApiError::from(e)
            }
        })?;

    Ok(Json(json!({"run_id": run_id, "status": "discarded"})))
}

// ── GET /v1/daemon/compute/runs ──────────────────────────────────────────

#[allow(clippy::missing_errors_doc)]
pub async fn list_runs_handler(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListRunsQuery>,
) -> Result<Json<RunListResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let owned_worlds =
        list_owned_world_ids(pool, &creator_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    let filters = RunListFilters {
        world_id: params.world_id,
        module_id: params.module_id,
        status: params.status,
        creator_world_ids: Some(owned_worlds),
    };
    let limit = params.limit.unwrap_or(20).min(100);
    let (items, next_cursor) = list_runs(pool, &filters, params.cursor.as_deref(), limit)
        .await
        .map_err(NexusApiError::from)?;

    let has_more = next_cursor.is_some();
    let summaries: Vec<NexusRunSummary> = items
        .into_iter()
        .map(|r| NexusRunSummary {
            run_id: r.run_id,
            world_id: r.world_id,
            module_id: r.module_id,
            module_version: r.module_version.unwrap_or_default(),
            status: match r.status.as_str() {
                "running" => NexusRunSummaryStatus::Running,
                "succeeded" => NexusRunSummaryStatus::Succeeded,
                "applied" => NexusRunSummaryStatus::Applied,
                "discarded" => NexusRunSummaryStatus::Discarded,
                // "failed" and anything else
                _ => NexusRunSummaryStatus::Failed,
            },
            created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                .unwrap_or_else(|_| chrono::DateTime::UNIX_EPOCH.into())
                .with_timezone(&chrono::Utc),
            updated_at: r.updated_at.map(|ts| {
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .unwrap_or_else(|_| chrono::DateTime::UNIX_EPOCH.into())
                    .with_timezone(&chrono::Utc)
            }),
            accepted_at: r.accepted_at.map(|ts| {
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .unwrap_or_else(|_| chrono::DateTime::UNIX_EPOCH.into())
                    .with_timezone(&chrono::Utc)
            }),
        })
        .collect();

    Ok(Json(RunListResponse {
        items: summaries,
        has_more,
        next_cursor,
    }))
}

// ── GET /v1/daemon/compute/runs/:run_id ──────────────────────────────────

#[allow(clippy::missing_errors_doc)]
pub async fn get_run_detail(
    State(state): State<WorkspaceState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunDetail>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let run = compute_runs::get_run(pool, &run_id)
        .await
        .map_err(NexusApiError::from)?
        .ok_or_else(|| NexusApiError::NotFound(format!("run {run_id} not found")))?;

    let owned = narrative_write::is_world_owned(pool, &creator_id, &run.world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {}", run.world_id),
            reason: "you do not own the world this run targets".to_string(),
        });
    }

    let detail: RunDetail = serde_json::from_value(json!({
        "run_id": run.run_id,
        "world_id": run.world_id,
        "module_id": run.module_id,
        "module_version": run.module_version.unwrap_or_default(),
        "status": run.status,
        "proposals": run.proposals_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "error": run.error_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "invocation_params": run.invocation_params_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "created_at": run.created_at,
        "updated_at": run.updated_at,
        "accepted_at": run.accepted_at,
    }))
    .map_err(|err| NexusApiError::Internal {
        code: "SERIALIZATION_ERROR".to_string(),
        message: format!("build run detail response: {err}"),
    })?;

    Ok(Json(detail))
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Resolve the effective run branch for `POST /run` (F-002).
///
/// - `req_branch_id == None` → the World root branch
///   (`narrative_worlds.root_fork_branch_id`, falling back to the legacy
///   `"fbk_root"` constant when unset).
/// - `req_branch_id == Some(id)` → must be a branch of the owned world:
///   the root branch, or a branch that has timeline events in this world.
///   The local DB keeps no `fork_branches` table (fork branches are
///   in-memory in `nexus-narrative`), so the durable branch registry is
///   `root_fork_branch_id` + the `branch_id`s materialized on
///   `narrative_timeline_events`.  Unknown / other-world branches → 422
///   `invalid_input`.
///
/// **Branch parity with the invoke path (V1.147 P3):** the preset invoke path
/// (`narrative.compute`) never accepts a caller-supplied branch — it derives
/// the branch from world state (`get_world_state().fork_branch_id` →
/// `"fbk_root"` fallback), so it can only ever bind the world root or an
/// event-bearing branch of the owned world. This resolver enforces the same
/// membership semantics for the direct lane's caller-supplied `branch_id`:
/// anything outside {world root, event-bearing branches of the owned world}
/// is rejected with 422 `invalid_input` — an unknown branch, or a branch
/// whose events live in another world, can never be bound.
///
/// **Lazy-fork limit (documented pre-1.0 persistence limit):** fork branches
/// are established lazily — a new branch exists in the durable registry only
/// once its first timeline event is appended (`world-delta-propose-apply.md`:
/// "the new branch is established by its first appended event"). A freshly
/// created but empty fork (no events yet) is therefore indistinguishable from
/// an unknown branch here and is rejected with 422. This mirrors the invoke
/// path, which also cannot bind an empty fork (its `fork_branch_id` derives
/// from the same event-based registry). Resolving empty forks would require a
/// durable fork-branch table — tracked as a pre-1.0 limitation, not an open
/// correctness hole (membership fails closed; no foreign branch is reachable).
///
/// Returns `(branch_id, timeline_head_event_id)`: the head is the world's
/// `current_timeline_head_id` for the root branch, and the branch's own
/// latest event (by sequence) for named branches.
///
/// # Errors
///
/// Returns 422 `invalid_input` for unknown branches, 500 on DB failure.
async fn resolve_run_branch(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    req_branch_id: Option<&str>,
) -> Result<(String, Option<String>), NexusApiError> {
    let row = sqlx::query!(
        "SELECT root_fork_branch_id, current_timeline_head_id \
         FROM narrative_worlds WHERE world_id = ?",
        world_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?
    .ok_or_else(|| NexusApiError::BadRequest {
        code: "invalid_input".to_string(),
        message: format!("world '{world_id}' not found"),
    })?;

    let root = row
        .root_fork_branch_id
        .unwrap_or_else(|| "fbk_root".to_string());
    match req_branch_id {
        None => Ok((root, row.current_timeline_head_id)),
        Some(req) if req == root => Ok((req.to_string(), row.current_timeline_head_id)),
        Some(req) => {
            // Branch membership: the branch must have timeline events in
            // this world (the durable branch registry).
            let branch_head = sqlx::query_scalar!(
                "SELECT timeline_event_id FROM narrative_timeline_events \
                 WHERE world_id = ? AND branch_id = ? ORDER BY sequence_no DESC LIMIT 1",
                world_id,
                req,
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
            match branch_head {
                Some(Some(head)) => Ok((req.to_string(), Some(head))),
                // `Some(None)` / `None`: no event on that branch in this
                // world → the branch is unknown here (sqlx reports the PK
                // column as nullable; a real row always has a value).
                Some(None) | None => Err(NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message: format!("branch '{req}' does not exist under world '{world_id}'"),
                }),
            }
        }
    }
}

async fn list_owned_world_ids(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    // Compile-time checked query (daemon-runtime AGENTS.md mandatory rule).
    // `world_id` is TEXT PRIMARY KEY in narrative_worlds (NOT NULL), but
    // the sqlx offline cache may report it as nullable; filter None away
    // (impossible in practice — every narrative_worlds row has a PK).
    let rows = sqlx::query_scalar!(
        r#"SELECT world_id as "world_id!" FROM narrative_worlds WHERE owner_creator_id = ?"#,
        creator_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn map_build_error(
    e: nexus_orchestration::compute_input_builder::ComputeBuildError,
) -> NexusApiError {
    use nexus_orchestration::compute_input_builder::ComputeBuildError;
    match e {
        ComputeBuildError::NoComputableEntries
        | ComputeBuildError::ReferencedEntryNotInWorld(_)
        | ComputeBuildError::ReferencedEntryNotFound(_) => NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: e.to_string(),
        },
        ComputeBuildError::Store(se) => NexusApiError::from(se),
        ComputeBuildError::Narrative(_)
        | ComputeBuildError::KbStore(_)
        | ComputeBuildError::Internal(_) => NexusApiError::Internal {
            code: "INTERNAL_ERROR".to_string(),
            message: e.to_string(),
        },
    }
}

const fn compute_error_code(e: &ComputeError) -> &'static str {
    match e {
        ComputeError::OutOfFuel => "compute_fuel_exhausted",
        ComputeError::WallTimeExceeded => "compute_wall_time_exceeded",
        ComputeError::MemoryCapExceeded => "compute_memory_cap_exceeded",
        ComputeError::Trap { .. } => "compute_module_trapped",
        ComputeError::ModuleComputeFailed { .. } => "compute_module_error",
        // V1.147 P3 F2: manifest-schema validation failures are input
        // problems, not internal faults → 422 `invalid_input`. The aggregated
        // per-entry form is handled with full detail in the run handler; the
        // single-aspect form (invocation / battle_report) falls through here.
        ComputeError::ManifestValidationFailed { .. } | ComputeError::InputValidationFailed(_) => {
            "invalid_input"
        }
        _ => "internal",
    }
}

/// Build a truncated proposals object when the full `RunResponse` exceeds
/// [`RESPONSE_BYTE_CAP`].  All 4 parts of the compute output are replaced with
/// empty/stub values; the untruncated output remains in `proposals_json` on the
/// session row.
///
/// Truncation strategy (per compute-output schema):
/// - `state_delta`     → empty `[]` (satisifes `"type": "array"`)
/// - `timeline_events` → empty `[]`
/// - `new_key_blocks`  → empty `[]`
/// - `battle_report`   → `{"kind": "truncated", ...}` (valid per
///   `additionalProperties: true`)
fn build_truncated_proposals() -> Value {
    let reason = format!(
        "response exceeds {RESPONSE_BYTE_CAP} bytes; full output available in GET /runs/:id"
    );
    json!({
        "schema_version": 1,
        "state_delta": [],
        "timeline_events": [],
        "new_key_blocks": [],
        "battle_report": {
            "kind": "truncated",
            "_truncated": true,
            "reason": reason,
        },
    })
}

fn map_delta_error(e: nexus_orchestration::capability::CapabilityError) -> NexusApiError {
    use nexus_orchestration::capability::CapabilityError;
    match e {
        CapabilityError::InputInvalid(msg) => NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: msg,
        },
        _ => NexusApiError::Internal {
            code: "INTERNAL_ERROR".to_string(),
            message: e.to_string(),
        },
    }
}

/// Create new `WorldKbEntry` records inside a TX.
async fn create_key_blocks_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    world_id: &str,
    blocks: &[serde_json::Map<String, Value>],
) -> Result<usize, NexusApiError> {
    use nexus_spoke_adapter::conversion::spoke_to_world_kb;

    let mut created = 0usize;
    for kb_map in blocks {
        let spoke: nexus_knowledge::world_kb::KnowledgeEntry =
            serde_json::from_value(Value::Object(kb_map.clone())).map_err(|e| {
                NexusApiError::Internal {
                    code: "DESERIALIZATION_ERROR".to_string(),
                    message: format!("decode new_key_block: {e}"),
                }
            })?;
        let kb = spoke_to_world_kb(spoke);

        if kb.world_id != world_id {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: format!(
                    "new_key_block '{}' targets world '{}', not admitted world '{world_id}'",
                    kb.entry_id, kb.world_id
                ),
            });
        }

        let block_type_str = serde_json::to_string(&kb.block_type).unwrap_or_default();
        let block_type_str = block_type_str.trim_matches('"');
        let body_json = kb
            .body
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let source_anchor_json = kb
            .source_anchor
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let now = chrono::Utc::now().to_rfc3339();

        // Compile-time checked query (F-004 fix — the Accept TX is the
        // highest-risk write path, so its SQL must be offline-validated).
        sqlx::query!(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, \
              body_json, source_anchor_json, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            kb.entry_id,
            kb.world_id,
            block_type_str,
            kb.canonical_name,
            kb.status,
            body_json,
            source_anchor_json,
            now,
            now,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("insert new_key_block '{}': {e}", kb.entry_id),
        })?;

        created += 1;
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::daemon_api::compute::run_response::RunResponse;

    #[test]
    fn build_truncated_proposals_round_trips_to_typed_run_response() {
        let truncated = build_truncated_proposals();
        // Verify the JSON shape satisfies compute-output schema requirements.
        assert_eq!(truncated["schema_version"], json!(1));
        assert_eq!(truncated["state_delta"], json!([]));
        assert_eq!(truncated["timeline_events"], json!([]));
        assert_eq!(truncated["new_key_blocks"], json!([]));
        assert_eq!(truncated["battle_report"]["kind"], json!("truncated"));

        // Round-trip through the typed RunResponse: the only honest truncation
        // signal is the first-class `truncated` field — not a battle_report
        // stubbed value (those are silently dropped by the schema-typed
        // deserializer; QC residual F1).
        let resp: RunResponse = serde_json::from_value(json!({
            "run_id": "run_test",
            "status": "succeeded",
            "module_id": "mod",
            "module_version": "1.0.0",
            "truncated": true,
            "created_at": "2026-07-31T12:00:00Z",
            "proposals": truncated,
        }))
        .expect("truncated RunResponse must deserialize");
        assert!(
            resp.truncated,
            "truncated field must survive the typed wire round-trip"
        );
    }

    #[test]
    fn run_response_default_truncated_is_false() {
        let resp: RunResponse = serde_json::from_value(json!({
            "run_id": "run_test",
            "status": "succeeded",
            "module_id": "mod",
            "module_version": "1.0.0",
            "created_at": "2026-07-31T12:00:00Z",
            "proposals": {
                "schema_version": 1,
                "state_delta": [],
                "timeline_events": [],
                "new_key_blocks": [],
                "battle_report": {"kind": "combat"}
            },
        }))
        .expect("normal RunResponse must deserialize");
        assert!(
            !resp.truncated,
            "non-truncated response must have truncated=false"
        );
    }
}
