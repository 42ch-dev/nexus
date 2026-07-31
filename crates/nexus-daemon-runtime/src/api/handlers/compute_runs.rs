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
use nexus_narrative::NarrativeGateway;
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

    // Build ComputeInput.
    let invocation_params = req.invocation_params.clone();
    let invocation_params_str = serde_json::to_string(&invocation_params).ok();
    let builder =
        ComputeInputBuilder::new(pool.clone(), &req.world_id, manifest, invocation_params);
    let compute_input = builder.build().await.map_err(map_build_error)?;

    // Insert run row.
    let run_id = compute_runs::insert_run(
        pool,
        &req.world_id,
        &req.module_id,
        Some(&module_version),
        invocation_params_str.as_deref(),
    )
    .await
    .map_err(NexusApiError::from)?;

    // Execute compute.
    let engine = resolve_engine(&state)?;
    let output = match engine.compute(&module, &cached.manifest, &compute_input) {
        Ok(o) => o,
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
    let _ = compute_runs::set_run_succeeded(pool, &run_id, &proposals_json)
        .await
        .map_err(NexusApiError::from)?;

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
    Json(_req): Json<RunAcceptRequest>,
) -> Result<Json<RunAcceptResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let run = compute_runs::get_run(pool, &run_id)
        .await
        .map_err(NexusApiError::from)?
        .ok_or_else(|| NexusApiError::NotFound(format!("run {run_id} not found")))?;

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

    let proposals_json = run.proposals_json.as_deref().unwrap_or("{}");
    let output: nexus_wasm_host::ComputeOutput =
        serde_json::from_str(proposals_json).map_err(|e| NexusApiError::Internal {
            code: "DESERIALIZATION_ERROR".to_string(),
            message: format!("parse run proposals: {e}"),
        })?;

    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
    let world_state =
        gw.get_world_state(&run.world_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("world state read: {e}"),
            })?;
    let branch_id = world_state
        .fork_branch_id
        .unwrap_or_else(|| "fbk_root".to_string());

    let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("begin transaction: {e}"),
    })?;

    let accepted_at = chrono::Utc::now().to_rfc3339();

    let state_delta_count = state_delta::apply_state_delta_in_tx(&mut tx, &output.state_delta)
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

    let events_created = output.timeline_events.len();
    let mut timeline_event_ids = Vec::with_capacity(events_created);
    for evt in &output.timeline_events {
        let event_type = "compute_result";
        let result = narrative_write::append_event_with_extensions_in_tx(
            &mut tx,
            &run.world_id,
            &branch_id,
            event_type,
            evt.title.as_deref().map(std::string::String::as_str),
            evt.summary.as_deref(),
            &provenance,
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

        // SAFETY: runtime query — table is kb_key_blocks (vetted DDL).
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, \
              body_json, source_anchor_json, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&kb.entry_id)
        .bind(&kb.world_id)
        .bind(block_type_str)
        .bind(&kb.canonical_name)
        .bind(&kb.status)
        .bind(&body_json)
        .bind(&source_anchor_json)
        .bind(&now)
        .bind(&now)
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
