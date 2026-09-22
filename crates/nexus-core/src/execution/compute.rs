//! Compute run authority (v1.190 P3-T3).
//!
//! The direct Control Room compute lane: invoke a WASM module against a
//! World, then atomically accept or discard its proposals. Moved out of the
//! daemon's `api/handlers/compute_runs.rs` so a non-HTTP caller (the TS
//! service, an independent product) drives the SAME authority instead of
//! re-implementing the accept transaction.
//!
//! # Facade surface (v1.195 P2-T1)
//!
//! The free functions below are the authority. [`ExecutionHandle`] exposes the
//! generated `daemon-api/compute` DTOs on top of them (current-host-contracts
//! §5), so the native/service boundary decodes and encodes exactly what
//! `schemas/` declares instead of hand-written request shapes. Each facade
//! method applies the same owner fence and principal binding as the
//! neighbouring `handle_ops` entry points and then delegates — it re-implements
//! nothing, so there is still ONE discovery/detail/history authority.
//!
//! # Why the accept path is the anchor
//!
//! Accept is the only compute operation with a domain effect. It applies the
//! proposal's state delta, creates its `new_key_blocks`, appends its timeline
//! events as CANON, and flips the run row to `applied` — all in ONE
//! transaction, with the ownership gate performed BEFORE any status
//! pre-check so a foreign run never leaks lifecycle state.
//!
//! Two properties make the single transaction load-bearing:
//!
//! - **Ownership before state.** A caller who does not own the run's World is
//!   refused before the status is inspected. Otherwise the refusal status
//!   itself would disclose whether a foreign run is `succeeded` or `applied`.
//! - **Full rollback.** Every write shares one `sqlx` transaction, so a
//!   foreign delta target, an unknown event id, or a lost accept race leaves
//!   NO partial domain effect. `set_run_applied_in_tx` is the CAS: its
//!   `WHERE status = 'succeeded'` means a concurrent accept updates zero rows
//!   and the whole transaction rolls back at commit.
//!
//! # Capability gating
//!
//! The whole module is behind `feature = "compute"`, which is never part of
//! the default/domain cohort: a domain-only build links no WASM engine and
//! registers no executable module. A caller without the feature cannot reach
//! this code at all, so "compute-off" is a compile-time absence, not a
//! runtime check that could be inverted.

#![cfg(feature = "compute")]

use crate::actor_knowledge::ActorKnowledgeViewService;
use crate::actors::AdmittedActor;
use crate::error::{CoreError, CoreResult};
use crate::execution::lifecycle::ExecutionHandle;
use crate::principal::Principal;
use crate::service::CoreService;
use nexus_contracts::generated::daemon_api::compute::{
    clear_runs_query::ClearRunsQuery,
    clear_runs_response::ClearRunsResponse,
    discard_run_response::{DiscardRunResponse, DiscardRunResponseStatus},
    list_modules_response::{
        ListModulesResponse, NexusComputeModuleSummary, NexusComputeModuleSummaryStatus,
    },
    list_runs_query::ListRunsQuery,
    module_detail::ModuleDetail,
    module_summary::{ModuleSummary, ModuleSummaryStatus},
    run_accept_request::RunAcceptRequest,
    run_accept_response::RunAcceptResponse,
    run_detail::RunDetail,
    run_list_response::{NexusRunSummary, NexusRunSummaryStatus, RunListResponse},
    run_request::RunRequest,
    run_response::RunResponse,
};
use nexus_local_db::compute_runs::{self, list_runs, RunListFilters};
use nexus_orchestration::compute_input_builder::ComputeInputBuilder;
use serde_json::{json, Value};
use std::sync::Arc;

/// Maximum response size (1 MiB) before the proposals payload is truncated.
///
/// The untruncated output stays durable on the run row, so a truncated
/// response is a read-path limit, never data loss.
const RESPONSE_BYTE_CAP: usize = 1024 * 1024;

/// Run-list page size when the caller omits `limit`.
const DEFAULT_RUN_LIST_LIMIT: u32 = 20;

/// Hard cap on the run-list page size.
const MAX_RUN_LIST_LIMIT: u32 = 100;

/// The daemon-wide compute serialization permit, acquired before every
/// invocation.
///
/// `wasmtime`'s epoch counter is engine-GLOBAL: the first watchdog to fire
/// traps EVERY concurrent invocation at the shortest budget. Serializing
/// invocations means each run's watchdog only ever observes its own budget.
/// The permit is held for the whole invocation and released with it, so a
/// queued run arms its watchdog only after the previous one has fully reaped
/// its own.
pub type ComputeSerializer = Arc<tokio::sync::Semaphore>;

/// The narrow owned inputs a compute operation needs.
///
/// Every field is an already-resolved value or a port the core owns — never
/// a transport state object. This is what replaces the daemon's
/// `WorkspaceState` in the dispatch path: a caller supplies exactly the
/// authority the operation consumes, so the core cannot reach back into a
/// transport it does not depend on.
#[derive(Clone)]
pub struct ComputeContext {
    /// The Creator owning this operation, already admitted.
    pub creator_id: String,
    /// The WASM engine, when the build linked one.
    pub engine: Option<Arc<nexus_wasm_host::WasmEngine>>,
    /// The compiled module cache.
    pub cache: Option<Arc<nexus_wasm_host::ModuleCache>>,
    /// Engine-global invocation serializer.
    pub serializer: ComputeSerializer,
}

impl ComputeContext {
    /// The engine, or a typed refusal naming the missing capability.
    fn engine(&self) -> CoreResult<Arc<nexus_wasm_host::WasmEngine>> {
        self.engine.clone().ok_or_else(|| CoreError::Internal {
            category: "WASM compute engine is not available".to_string(),
        })
    }

    /// The module cache, or a typed refusal.
    fn cache(&self) -> CoreResult<Arc<nexus_wasm_host::ModuleCache>> {
        self.cache.clone().ok_or_else(|| CoreError::Internal {
            category: "WASM module cache is not available".to_string(),
        })
    }
}

/// List installed compute modules.
///
/// Reads the compiled-in module registry, so it requires no pool and no
/// owner: a module listing is machine capability, not domain state. The
/// return type is the generated `module-summary` DTO (the wasm-host registry
/// re-exports the same generated type).
///
/// # Errors
/// Currently infallible; returns `Result` so the transport surface stays
/// uniform with the other compute operations.
pub fn list_compute_modules() -> CoreResult<Vec<ModuleSummary>> {
    Ok(nexus_wasm_host::list_modules())
}

/// Read one module's manifest detail.
///
/// # Errors
/// `NotFound` when no module with `module_id` is installed; `Internal` when
/// an installed module's manifest is present but unparsable.
pub fn get_compute_module(module_id: &str) -> CoreResult<ModuleDetail> {
    match nexus_wasm_host::get_module(module_id) {
        Ok(Some(detail)) => Ok(detail),
        Ok(None) => Err(CoreError::NotFound {
            resource: format!("module '{module_id}' not found"),
        }),
        Err(err) => Err(CoreError::Internal {
            category: format!("module '{module_id}' manifest is invalid: {err}"),
        }),
    }
}

/// Invoke a compute module against a World.
///
/// Ownership is checked before the module is resolved, so a foreign World is
/// refused without disclosing whether the named module exists.
///
/// # Errors
/// `Forbidden` for a World the creator does not own, `NotFound` for an
/// unknown module, `Preset`-style coded refusals for sandbox limits and
/// invalid input, `Internal` for storage faults.
#[allow(clippy::too_many_lines)] // one linear domain operation
pub async fn compute_run(
    core: &CoreService,
    context: &ComputeContext,
    request: RunRequest,
) -> CoreResult<RunResponse> {
    let pool = &core.inner.pool;
    let creator_id = context.creator_id.as_str();

    ensure_world_owned(pool, creator_id, &request.world_id).await?;

    let cache = context.cache()?;
    let cached = cache
        .get(&request.module_id)
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("module '{}' not found", request.module_id),
        })?;
    let module = cached.module.clone();
    let module_version = cached.manifest.version.clone();
    let manifest = cached.manifest.clone();

    // Resolve the run's branch BEFORE assembling input so the module observes
    // exactly the position snapshotted onto the run row.
    let (branch_id, timeline_head_event_id) =
        resolve_run_branch(pool, &request.world_id, request.branch_id.as_deref()).await?;

    let invocation_params = request.invocation_params.clone();
    let invocation_params_str = serde_json::to_string(&invocation_params).ok();
    // The module input is a model-facing payload, so it reads through the
    // admitted Creator ActorView — never a management review (durable §4.1/§4.2).
    // A Creator whose holder registry row is missing refuses here instead of
    // falling back to a World-wide read.
    let selection = ActorKnowledgeViewService::new(pool.clone())
        .actor_view_scope(
            creator_id,
            &AdmittedActor::Creator {
                creator_id: creator_id.to_string(),
            },
            &request.world_id,
            None,
        )
        .await?;
    let builder = ComputeInputBuilder::new(
        pool.clone(),
        &request.world_id,
        manifest,
        invocation_params,
        selection,
    )
    .with_narrative_position(branch_id.clone(), timeline_head_event_id.clone());
    let compute_input = builder.build().await.map_err(map_build_error)?;

    let run_id = compute_runs::insert_run(
        pool,
        &request.world_id,
        &request.module_id,
        Some(&module_version),
        Some(&branch_id),
        timeline_head_event_id.as_deref(),
        invocation_params_str.as_deref(),
    )
    .await
    .map_err(crate::error::local_db_err)?;

    let engine = context.engine()?;
    let permit = context
        .serializer
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| CoreError::Internal {
            category: "compute serializer closed".to_string(),
        })?;
    let engine_task = engine.clone();
    let module_task = module.clone();
    let manifest_task = cached.manifest.clone();
    let input_task = compute_input.clone();

    // `engine.compute` is CPU-bound for up to the wall-time budget: run it on
    // the blocking pool, never inline on an async worker (a long run would
    // stall the whole runtime).
    let compute_result = match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        engine_task.compute(&module_task, &manifest_task, &input_task)
    })
    .await
    {
        Ok(result) => result,
        Err(join_err) => {
            // The blocking task panicked. Best-effort compensating persist so
            // the row is not stuck in 'running' forever; retries on the same
            // run_id are forbidden by the status guard.
            persist_failure(
                pool,
                &run_id,
                json!({
                    "code": "internal",
                    "message": format!("compute task join failed: {join_err}"),
                }),
            )
            .await;
            return Err(CoreError::Internal {
                category: format!("compute task join failed: {join_err}"),
            });
        }
    };

    let output = match compute_result {
        Ok(output) => output,
        Err(nexus_wasm_host::ComputeError::InputValidationFailed(entries)) => {
            // Manifest validation failures are a CLIENT-input problem, not an
            // internal fault. The run still fails (row persisted Failed) and
            // the refusal stays an honest coded error with per-entry detail —
            // invalid entries are never silently skipped.
            let entries_value = serde_json::to_value(&entries).unwrap_or_else(|_| json!([]));
            // The persisted row and the response carry the SAME structured
            // detail by construction, so `GET /runs/{id}` and the refusal
            // agree on `details.invalid_entries`.
            let details = json!({ "invalid_entries": entries_value });
            persist_failure(
                pool,
                &run_id,
                json!({
                    "code": "invalid_input",
                    "message": format!(
                        "compute input validation failed: {} invalid entry(ies); the run was not applied",
                        entries.len()
                    ),
                    "details": details,
                }),
            )
            .await;
            return Err(CoreError::InputValidation { details });
        }
        Err(err) => {
            let error_code = compute_error_code(&err);
            persist_failure(
                pool,
                &run_id,
                json!({ "code": error_code, "message": err.to_string() }),
            )
            .await;
            // Sandbox limits are an input/budget refusal; everything else is
            // an internal fault. The code is preserved so the transport can
            // render the retained 422 body verbatim.
            if error_code == "internal" {
                return Err(CoreError::Internal {
                    category: err.to_string(),
                });
            }
            return Err(coded_refusal(error_code, err.to_string()));
        }
    };

    let proposals_json = serde_json::to_string(&output).map_err(|e| CoreError::Internal {
        category: format!("serialize compute output: {e}"),
    })?;
    if let Err(db_err) = compute_runs::set_run_succeeded(pool, &run_id, &proposals_json).await {
        // Compensating failure persist so the row is not stuck in 'running'
        // forever (it can never be accepted, and retries on the same run_id
        // are forbidden by the status guard).
        persist_failure(
            pool,
            &run_id,
            json!({
                "code": "internal",
                "message": format!("failed to persist succeeded outcome: {db_err}"),
            }),
        )
        .await;
        return Err(crate::error::local_db_err(db_err));
    }

    let proposals_raw: Value = serde_json::from_str(&proposals_json).unwrap_or(Value::Null);
    let created_at = chrono::Utc::now();

    let response: RunResponse = serde_json::from_value(json!({
        "run_id": &run_id,
        "status": "succeeded",
        "module_id": &request.module_id,
        "module_version": &module_version,
        "created_at": created_at.to_rfc3339(),
        "proposals": &proposals_raw,
    }))
    .map_err(|err| CoreError::Internal {
        category: format!("build run response: {err}"),
    })?;

    // Response cap: when the full payload is over budget, truncate all four
    // proposal parts. The untruncated output remains durable on the row.
    if serde_json::to_vec(&response).unwrap_or_default().len() > RESPONSE_BYTE_CAP {
        return serde_json::from_value(json!({
            "run_id": run_id,
            "status": "succeeded",
            "module_id": request.module_id,
            "module_version": module_version,
            "truncated": true,
            "created_at": created_at.to_rfc3339(),
            "proposals": build_truncated_proposals(),
        }))
        .map_err(|err| CoreError::Internal {
            category: format!("build truncated run response: {err}"),
        });
    }

    Ok(response)
}

/// Accept a succeeded run's proposals: apply them ATOMICALLY.
///
/// The ownership gate runs FIRST — before the status pre-check — so a foreign
/// run never leaks lifecycle state (refused as ownership, not as status).
///
/// Everything that mutates domain state shares ONE transaction:
///
/// 1. the state delta (`apply_state_delta_in_tx`),
/// 2. the new key blocks (`create_key_blocks_in_tx`),
/// 3. the accepted timeline events, appended as CANON (the Timeline
///    projection reads canon only, so an accepted run must surface as a
///    Narrative node),
/// 4. the run row flip to `applied` — the CAS, whose
///    `WHERE status = 'succeeded'` makes a concurrent accept update zero rows
///    and roll the whole transaction back.
///
/// `request.timeline_event_ids_to_accept` subsets the appended events by the
/// stable index-based ids (`evt_<index>`) the proposals carry. An unknown id
/// refuses the whole accept BEFORE any write; absent/null/empty accepts all.
///
/// # Errors
/// `Forbidden` when the creator does not own the run's World, `NotFound` for
/// an unknown run, a coded conflict when the run is already `applied` or
/// `discarded`, `InvalidInput` for a non-`succeeded` run or an unknown event
/// id, `Internal` for storage faults.
#[allow(clippy::too_many_lines)] // one linear domain operation
pub async fn accept_compute_run(
    core: &CoreService,
    principal: &Principal,
    run_id: &str,
    request: RunAcceptRequest,
) -> CoreResult<RunAcceptResponse> {
    let pool = &core.inner.pool;
    let creator_id = principal.creator_id();

    let run = compute_runs::get_run(pool, run_id)
        .await
        .map_err(crate::error::local_db_err)?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("run {run_id} not found"),
        })?;

    ensure_world_owned(pool, creator_id, &run.world_id).await?;

    match run.status.as_str() {
        "succeeded" => {}
        "applied" => {
            return Err(coded_conflict(format!(
                "run {run_id} has already been accepted"
            )));
        }
        "discarded" => {
            return Err(coded_conflict(format!(
                "run {run_id} has already been discarded"
            )));
        }
        other => {
            return Err(coded_refusal(
                "invalid_state",
                format!("run {run_id} is in status '{other}', must be 'succeeded' to accept"),
            ));
        }
    }

    let proposals_json = run.proposals_json.as_deref().unwrap_or("{}");
    let output: nexus_wasm_host::ComputeOutput =
        serde_json::from_str(proposals_json).map_err(|e| CoreError::Internal {
            category: format!("parse run proposals: {e}"),
        })?;

    // Accept appends to the branch SNAPSHOTTED at run time, not the current
    // fork head — the fork may have moved between run and accept. Pre-fix
    // rows carry a NULL `branch_id`; fall back to the legacy constant to
    // preserve their behavior exactly.
    let branch_id = run
        .branch_id
        .clone()
        .unwrap_or_else(|| "fbk_root".to_string());

    let selected_event_indices = select_event_indices(
        request.timeline_event_ids_to_accept.as_deref(),
        output.timeline_events.len(),
    )?;

    let mut tx = pool.begin().await.map_err(|e| CoreError::Internal {
        category: format!("begin transaction: {e}"),
    })?;

    let accepted_at = chrono::Utc::now().to_rfc3339();

    // Every delta target must resolve inside the run's world — a foreign
    // target rejects the whole accept (full rollback on drop).
    let state_delta_count = nexus_orchestration::state_delta::apply_state_delta_in_tx(
        &mut tx,
        &run.world_id,
        &output.state_delta,
    )
    .await
    .map_err(map_delta_error)?;

    let new_entries_created =
        create_key_blocks_in_tx(&mut tx, &run.world_id, &output.new_key_blocks).await?;

    let provenance = serde_json::to_string(&json!({
        "compute": {
            "module_id": run.module_id,
            "module_version": run.module_version,
            "run_id": run_id,
            "source_kind": "direct_invoke",
        }
    }))
    .map_err(|e| CoreError::Internal {
        category: format!("build compute provenance: {e}"),
    })?;

    let events_created = selected_event_indices
        .as_ref()
        .map_or(output.timeline_events.len(), std::collections::HashSet::len);
    let mut timeline_event_ids = Vec::with_capacity(events_created);
    for (index, evt) in output.timeline_events.iter().enumerate() {
        if selected_event_indices
            .as_ref()
            .is_some_and(|selected| !selected.contains(&index))
        {
            continue;
        }
        // Canon (not provisional): an accepted Run is author-committed world
        // truth. Discard/failed paths never reach this write.
        let affected_json = if evt.affected_key_block_ids.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&evt.affected_key_block_ids).map_err(|e| {
                    CoreError::Internal {
                        category: format!("serialize affected_key_block_ids: {e}"),
                    }
                })?,
            )
        };
        let result = nexus_local_db::narrative_write::append_event_canon_with_extensions_in_tx(
            &mut tx,
            &run.world_id,
            &branch_id,
            "compute_result",
            evt.title.as_deref().map(String::as_str),
            evt.summary.as_deref(),
            &provenance,
            affected_json.as_deref(),
            // modules_json: compute proposals do not carry `modules` on the
            // domain wire; the persistence plumbing exists for future writers.
            None,
        )
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("append timeline event: {e}"),
        })?;
        timeline_event_ids.push(result.event_id);
    }

    // The CAS: `WHERE status = 'succeeded'` means a concurrent accept updates
    // zero rows, and the error rolls the whole transaction back.
    compute_runs::set_run_applied_in_tx(&mut tx, run_id, &accepted_at)
        .await
        .map_err(|e| match e {
            nexus_local_db::LocalDbError::ConstraintViolation { .. } => {
                coded_conflict(format!("run {run_id} has already been accepted"))
            }
            other => crate::error::local_db_err(other),
        })?;

    tx.commit().await.map_err(|e| CoreError::Internal {
        category: format!("commit: {e}"),
    })?;

    serde_json::from_value(json!({
        "run_id": run_id,
        "status": "applied",
        "applied": {
            "state_delta_count": state_delta_count,
            "events_created": events_created,
            "new_entries_created": new_entries_created,
        },
        "timeline_event_ids": timeline_event_ids,
    }))
    .map_err(|err| CoreError::Internal {
        category: format!("build accept response: {err}"),
    })
}

/// Discard a succeeded run's proposals.
///
/// No domain effect: the proposals are dropped and the row flips to
/// `discarded`. The World is left exactly as it was.
///
/// # Errors
/// `Forbidden` for a foreign World, `NotFound` for an unknown run, a coded
/// conflict when the row is not `succeeded`, `Internal` for storage faults.
pub async fn discard_compute_run(
    core: &CoreService,
    principal: &Principal,
    run_id: &str,
) -> CoreResult<()> {
    let pool = &core.inner.pool;
    let creator_id = principal.creator_id();

    let run = compute_runs::get_run(pool, run_id)
        .await
        .map_err(crate::error::local_db_err)?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("run {run_id} not found"),
        })?;

    ensure_world_owned(pool, creator_id, &run.world_id).await?;

    compute_runs::set_run_discarded(pool, run_id)
        .await
        .map_err(|e| match e {
            nexus_local_db::LocalDbError::ConstraintViolation { .. } => {
                coded_conflict(format!("run {run_id} is not in 'succeeded' status"))
            }
            other => crate::error::local_db_err(other),
        })?;

    Ok(())
}

/// Clear a World's TERMINAL run history, returning how many rows went (C8).
///
/// Clear is World-scoped, never a world-wide purge: `query.world_id` is
/// required by the generated query, and ownership is verified BEFORE any row
/// is touched, so a foreign World is refused (and its history never disclosed)
/// rather than cleared.
///
/// Deleting a run row is not an UNDO. An accepted run's effect — the applied
/// state delta, the new key blocks, the CANON `compute_result` timeline
/// events — lives in the World, not on the run row, so Clear drops history
/// without reverting anything its owner accepted (retained C8: "does not undo
/// an already accepted World effect"). The same reason keeps `running` and
/// `succeeded` rows: a succeeded run still needs review.
///
/// # Retention
/// [`nexus_local_db::compute_runs::delete_terminal_runs`] owns the predicate.
/// Only `applied` | `discarded` | `failed` are matched, and the `run_id IS NOT
/// NULL` clause keeps Clear inside the direct lane, so peer/spoke adapter rows
/// cannot be reached at all. `query.status` narrows Clear to ONE terminal
/// state; the generated `clear-runs-query` schema admits only terminal values,
/// and the storage predicate re-validates as defense in depth — a
/// status/CAS-suppressing "clear everything" is not expressible here.
///
/// # Errors
/// `WorldOwnerDenied` when the principal's creator does not own the World;
/// `Internal` for storage faults.
pub async fn clear_compute_runs(
    core: &CoreService,
    principal: &Principal,
    query: ClearRunsQuery,
) -> CoreResult<ClearRunsResponse> {
    let pool = &core.inner.pool;
    let ClearRunsQuery { status, world_id } = query;

    ensure_world_owned(pool, principal.creator_id(), &world_id).await?;

    let status = status.map(|status| status.to_string());
    let deleted = compute_runs::delete_terminal_runs(pool, &world_id, status.as_deref())
        .await
        .map_err(crate::error::local_db_err)?;

    // The wire field is `i64` (the generated schema's integer) while SQLite
    // reports a `u64`. A row count is bounded by the table, so the conversion
    // is total in practice; refuse rather than truncate if it ever is not.
    let deleted = i64::try_from(deleted).map_err(|_| CoreError::Internal {
        category: format!("cleared run count {deleted} does not fit the response field"),
    })?;

    Ok(ClearRunsResponse { deleted })
}

/// List runs for the creator's owned Worlds, cursor-paginated.
///
/// The query is the generated `list-runs-query` DTO, so the native/service
/// boundary and this authority agree on one query shape instead of two
/// structurally identical Rust declarations.
///
/// # Errors
/// `InvalidInput` for a negative `limit`; `Internal` for storage faults.
pub async fn list_compute_runs(
    core: &CoreService,
    principal: &Principal,
    query: ListRunsQuery,
) -> CoreResult<RunListResponse> {
    let pool = &core.inner.pool;
    let creator_id = principal.creator_id();

    let ListRunsQuery {
        world_id,
        module_id,
        status,
        cursor,
        limit,
    } = query;

    let owned_worlds = list_owned_world_ids(pool, creator_id).await?;

    // The wire carries an unbounded integer while the durable reader takes a
    // `u32`. A negative value is a client-input fault and is refused rather
    // than coerced into a page size the caller never asked for.
    let requested = limit.unwrap_or(i64::from(DEFAULT_RUN_LIST_LIMIT));
    let limit = u32::try_from(requested)
        .map_err(|_| {
            coded_refusal(
                "invalid_input",
                format!("limit must be a non-negative integer, got {requested}"),
            )
        })?
        .min(MAX_RUN_LIST_LIMIT);

    let filters = RunListFilters {
        world_id,
        module_id,
        // The generated status enum renders its exact wire spelling, which is
        // the durable column's vocabulary.
        status: status.map(|status| status.to_string()),
        creator_world_ids: Some(owned_worlds),
    };
    let (items, next_cursor) = list_runs(pool, &filters, cursor.as_deref(), limit)
        .await
        .map_err(crate::error::local_db_err)?;

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
                // "failed" and anything else.
                _ => NexusRunSummaryStatus::Failed,
            },
            created_at: parse_rfc3339(&r.created_at),
            updated_at: r.updated_at.as_deref().map(parse_rfc3339),
            accepted_at: r.accepted_at.as_deref().map(parse_rfc3339),
        })
        .collect();

    Ok(RunListResponse {
        items: summaries,
        has_more,
        next_cursor,
    })
}

/// Read one run's detail, including its proposals and error.
///
/// # Errors
/// `Forbidden` for a foreign World, `NotFound` for an unknown run,
/// `Internal` for storage or serialization faults.
pub async fn get_compute_run(
    core: &CoreService,
    principal: &Principal,
    run_id: &str,
) -> CoreResult<RunDetail> {
    let pool = &core.inner.pool;

    let run = compute_runs::get_run(pool, run_id)
        .await
        .map_err(crate::error::local_db_err)?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("run {run_id} not found"),
        })?;

    ensure_world_owned(pool, principal.creator_id(), &run.world_id).await?;

    serde_json::from_value(json!({
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
    .map_err(|err| CoreError::Internal {
        category: format!("build run detail response: {err}"),
    })
}

// ---------------------------------------------------------------------------
// Generated-DTO facade on the execution owner
// ---------------------------------------------------------------------------

/// The Compute operation surface the native/service boundary drives.
///
/// Each method is a typed entry point over the authority functions above: the
/// owner fence and the principal binding are applied EXACTLY as the
/// neighbouring `handle_ops` entry points apply them, and the operation is
/// then delegated, so a non-HTTP caller cannot reach a second implementation
/// of discovery, run detail, history or clear. Nothing here inspects the pool,
/// the registry or the run rows directly — a missing authority is a typed
/// refusal from the authority, never an empty success.
///
/// `compute_run` and `accept_compute_run` already exist in `handle_ops` with
/// the retained signatures (current-host-contracts §5) and are not duplicated.
impl ExecutionHandle {
    /// The owner-level admission fence every entry point checks.
    ///
    /// The same predicate the neighbouring `handle_ops` entry points apply —
    /// the coordinator's draining barrier, read through this handle's public
    /// `is_draining` view — so the two surfaces cannot disagree about when the
    /// owner has stopped accepting work. (`handle_ops::ensure_admitting` is
    /// module-private and cannot be shared, hence the local name.)
    fn ensure_not_draining(&self) -> CoreResult<()> {
        if self.is_draining() {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    /// List the installed compute modules (C1).
    ///
    /// The registry is compiled in, so the list is machine capability rather
    /// than domain state: there is no per-creator filter. The principal is
    /// still verified, so this surface cannot be driven through a foreign or
    /// stale principal handle.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service.
    pub fn list_compute_modules(&self, principal: &Principal) -> CoreResult<ListModulesResponse> {
        self.ensure_not_draining()?;
        self.linked_core()?.verify_principal(principal)?;
        let items = crate::execution::compute::list_compute_modules()?
            .into_iter()
            .map(module_summary_row)
            .collect();
        // The registry is compiled in and complete: there is no page 2.
        Ok(ListModulesResponse {
            items,
            has_more: false,
        })
    }

    /// Read one installed module's manifest detail — the invocation schema Run
    /// Studio renders (C2).
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service; `NotFound` for an
    /// unknown module; `Internal` for an unparsable embedded manifest.
    pub fn get_compute_module(
        &self,
        principal: &Principal,
        module_id: String,
    ) -> CoreResult<ModuleDetail> {
        self.ensure_not_draining()?;
        self.linked_core()?.verify_principal(principal)?;
        crate::execution::compute::get_compute_module(&module_id)
    }

    /// Read one run's detail: its proposals, or the recorded failure (C4).
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service; `Forbidden` for a
    /// run of a World the principal's creator does not own; `NotFound` for an
    /// unknown run; `Internal` for storage faults.
    pub async fn get_compute_run(
        &self,
        principal: &Principal,
        run_id: String,
    ) -> CoreResult<RunDetail> {
        self.ensure_not_draining()?;
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        crate::execution::compute::get_compute_run(core.as_ref(), principal, &run_id).await
    }

    /// List the principal creator's runs, cursor-paginated (C5).
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service; `InvalidInput` for a
    /// negative `limit`; `Internal` for storage faults.
    pub async fn list_compute_runs(
        &self,
        principal: &Principal,
        query: ListRunsQuery,
    ) -> CoreResult<RunListResponse> {
        self.ensure_not_draining()?;
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        crate::execution::compute::list_compute_runs(core.as_ref(), principal, query).await
    }

    /// Discard a succeeded run's proposals; the World is left untouched (C7).
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service; `Forbidden` for a
    /// foreign World; `NotFound` for an unknown run; a coded conflict when the
    /// row is not `succeeded`; `Internal` for storage faults.
    pub async fn discard_compute_run(
        &self,
        principal: &Principal,
        run_id: String,
    ) -> CoreResult<DiscardRunResponse> {
        self.ensure_not_draining()?;
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        crate::execution::compute::discard_compute_run(core.as_ref(), principal, &run_id).await?;
        Ok(DiscardRunResponse {
            run_id,
            status: DiscardRunResponseStatus::Discarded,
        })
    }

    /// Clear the creator's TERMINAL run history for ONE owned World (C8),
    /// returning the generated `{deleted}` count.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; `AuthRequired` when the
    /// principal does not belong to this owner's service; `WorldOwnerDenied`
    /// for a World the principal's creator does not own; `Internal` for
    /// storage faults.
    pub async fn clear_compute_runs(
        &self,
        principal: &Principal,
        query: ClearRunsQuery,
    ) -> CoreResult<ClearRunsResponse> {
        self.ensure_not_draining()?;
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        crate::execution::compute::clear_compute_runs(core.as_ref(), principal, query).await
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Convert one registry summary into a list-response row.
///
/// `list-modules-response.schema.json` references the summary shape, which
/// codegen inlines into a second generated struct. The two are structurally
/// identical but nominally distinct, so the mapping is written out: a field
/// added to either schema then fails to compile here instead of being
/// silently dropped from the wire.
fn module_summary_row(summary: ModuleSummary) -> NexusComputeModuleSummary {
    NexusComputeModuleSummary {
        module_id: summary.module_id,
        name: summary.name,
        version: summary.version,
        description: summary.description,
        required_key_block_types: summary.required_key_block_types,
        battle_report_kind: summary.battle_report_kind,
        status: match summary.status {
            ModuleSummaryStatus::Ok => NexusComputeModuleSummaryStatus::Ok,
            ModuleSummaryStatus::Broken => NexusComputeModuleSummaryStatus::Broken,
        },
    }
}

/// A coded refusal: the transport renders the retained lowercase wire code.
///
/// The retained compute codes (`invalid_state`, `compute_fuel_exhausted`, …)
/// and the already-accepted/discarded conflict all promise a specific
/// `(status, code)` pair on the wire. `CoreError::Coded` carries that pair so
/// the adapter can render it verbatim.
fn coded_refusal(code: &str, message: impl Into<String>) -> CoreError {
    CoreError::Coded {
        code: code.to_string(),
        message: message.into(),
    }
}

/// A coded 409-class conflict (an already-applied/discarded run).
fn coded_conflict(message: impl Into<String>) -> CoreError {
    coded_refusal("conflict", message)
}

/// Refuse unless `creator_id` owns `world_id`.
///
/// Runs BEFORE any status or existence check on a run, so a foreign run never
/// leaks lifecycle state.
async fn ensure_world_owned(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    match nexus_local_db::narrative_write::is_world_owned(pool, creator_id, world_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(CoreError::WorldOwnerDenied {
            world_id: world_id.to_string(),
            reason: "you do not own this world".to_string(),
        }),
        Err(e) => Err(CoreError::Internal {
            category: format!("world ownership check: {e}"),
        }),
    }
}

/// Resolve the effective run branch and its timeline head.
///
/// - absent → the World root branch, whose head is the World's
///   `current_timeline_head_id`;
/// - present → must be the root branch or a branch that has timeline events
///   in this World. The local DB keeps no `fork_branches` table (fork
///   branches are in-memory in `nexus-narrative`), so the durable branch
///   registry is the root id plus the `branch_id`s materialized on
///   `narrative_timeline_events`.
///
/// A freshly created but EMPTY fork is therefore indistinguishable from an
/// unknown branch and is refused. This mirrors the preset invoke path, which
/// also binds from the same event-based registry: membership fails closed, so
/// no foreign branch is ever reachable.
async fn resolve_run_branch(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    requested: Option<&str>,
) -> CoreResult<(String, Option<String>)> {
    let row = sqlx::query!(
        "SELECT root_fork_branch_id, current_timeline_head_id \
         FROM narrative_worlds WHERE world_id = ?",
        world_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| CoreError::Internal {
        category: format!("resolve run branch: {e}"),
    })?
    .ok_or_else(|| coded_refusal("invalid_input", format!("world '{world_id}' not found")))?;

    let root = row
        .root_fork_branch_id
        .unwrap_or_else(|| "fbk_root".to_string());
    match requested {
        None => Ok((root, row.current_timeline_head_id)),
        Some(req) if req == root => Ok((req.to_string(), row.current_timeline_head_id)),
        Some(req) => {
            let branch_head = sqlx::query_scalar!(
                "SELECT timeline_event_id FROM narrative_timeline_events \
                 WHERE world_id = ? AND branch_id = ? ORDER BY sequence_no DESC LIMIT 1",
                world_id,
                req,
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| CoreError::Internal {
                category: format!("resolve run branch head: {e}"),
            })?;
            match branch_head {
                Some(Some(head)) => Ok((req.to_string(), Some(head))),
                // `Some(None)` / `None`: no event on that branch in this
                // world, so the branch is unknown here.
                Some(None) | None => Err(coded_refusal(
                    "invalid_input",
                    format!("branch '{req}' does not exist under world '{world_id}'"),
                )),
            }
        }
    }
}

/// The World ids `creator_id` owns.
async fn list_owned_world_ids(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
) -> CoreResult<Vec<String>> {
    let rows = sqlx::query_scalar!(
        r#"SELECT world_id as "world_id!" FROM narrative_worlds WHERE owner_creator_id = ?"#,
        creator_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| CoreError::Internal {
        category: format!("list owned worlds: {e}"),
    })?;
    Ok(rows)
}

/// Resolve `evt_<index>` ids to proposal indices, or `None` for "accept all".
///
/// An unknown id refuses the whole accept BEFORE any write.
fn select_event_indices(
    requested: Option<&[String]>,
    available: usize,
) -> CoreResult<Option<std::collections::HashSet<usize>>> {
    match requested {
        None | Some([]) => Ok(None),
        Some(ids) => {
            let mut selected = std::collections::HashSet::with_capacity(ids.len());
            for id in ids {
                let index = id
                    .strip_prefix("evt_")
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|i| *i < available);
                match index {
                    Some(i) => {
                        selected.insert(i);
                    }
                    None => {
                        return Err(coded_refusal(
                            "invalid_input",
                            format!(
                                "timeline_event_ids_to_accept references unknown event id \
                                 '{id}' (proposals contain {available} timeline events; ids are \
                                 'evt_0'..'evt_{}')",
                                available.saturating_sub(1)
                            ),
                        ));
                    }
                }
            }
            Ok(Some(selected))
        }
    }
}

/// Best-effort compensating failure persist.
///
/// A failure here is logged, never propagated: the caller is already
/// returning the ORIGINAL error, and replacing it with a storage error would
/// hide the real cause.
async fn persist_failure(pool: &sqlx::SqlitePool, run_id: &str, error: Value) {
    let payload =
        serde_json::to_string(&error).unwrap_or_else(|_| r#"{"code":"internal"}"#.to_string());
    if let Err(db_err) = compute_runs::set_run_failed(pool, run_id, &payload).await {
        tracing::error!(
            run_id = %run_id,
            error = %db_err,
            "compensating set_run_failed also failed"
        );
    }
}

/// Map a sandbox error onto its retained lowercase wire code.
const fn compute_error_code(e: &nexus_wasm_host::ComputeError) -> &'static str {
    use nexus_wasm_host::ComputeError;
    match e {
        ComputeError::OutOfFuel => "compute_fuel_exhausted",
        ComputeError::WallTimeExceeded => "compute_wall_time_exceeded",
        ComputeError::MemoryCapExceeded => "compute_memory_cap_exceeded",
        ComputeError::Trap { .. } => "compute_module_trapped",
        ComputeError::ModuleComputeFailed { .. } => "compute_module_error",
        // Manifest-schema validation failures are input problems, not internal
        // faults. The aggregated per-entry form is handled with full detail in
        // `compute_run`; the single-aspect form falls through here.
        ComputeError::ManifestValidationFailed { .. } | ComputeError::InputValidationFailed(_) => {
            "invalid_input"
        }
        _ => "internal",
    }
}

/// Map an input-builder failure onto the core taxonomy.
fn map_build_error(e: nexus_orchestration::compute_input_builder::ComputeBuildError) -> CoreError {
    use nexus_orchestration::compute_input_builder::ComputeBuildError;
    match e {
        ComputeBuildError::NoComputableEntries
        | ComputeBuildError::ReferencedEntryNotInWorld(_)
        | ComputeBuildError::ReferencedEntryNotFound(_) => {
            coded_refusal("invalid_input", e.to_string())
        }
        ComputeBuildError::Store(se) => crate::error::db_err(&se),
        ComputeBuildError::Narrative(_)
        | ComputeBuildError::KbStore(_)
        | ComputeBuildError::Internal(_) => CoreError::Internal {
            category: e.to_string(),
        },
    }
}

/// Map a state-delta application failure onto the core taxonomy.
fn map_delta_error(e: nexus_orchestration::capability::CapabilityError) -> CoreError {
    use nexus_orchestration::capability::CapabilityError;
    match e {
        CapabilityError::InputInvalid(msg) => coded_refusal("invalid_input", msg),
        other => CoreError::Internal {
            category: other.to_string(),
        },
    }
}

/// Create new key blocks inside the accept transaction.
///
/// The lane is World-owned only: a block naming another world (or carrying a
/// non-World owner) refuses the whole accept.
async fn create_key_blocks_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    world_id: &str,
    blocks: &[serde_json::Map<String, Value>],
) -> CoreResult<usize> {
    use nexus_spoke_adapter::conversion::spoke_to_knowledge_record;

    let mut created = 0usize;
    for kb_map in blocks {
        let spoke: nexus_knowledge::world_kb::KnowledgeEntry =
            serde_json::from_value(Value::Object(kb_map.clone())).map_err(|e| {
                CoreError::Internal {
                    category: format!("decode new_key_block: {e}"),
                }
            })?;
        let kb = spoke_to_knowledge_record(spoke).map_err(|e| CoreError::Internal {
            category: format!("decode new_key_block owner: {e}"),
        })?;

        // World-scoped insert lane: reject any non-World owner.
        if kb.world_id() != Some(world_id) {
            return Err(coded_refusal(
                "invalid_input",
                format!(
                    "new_key_block '{}' targets world '{}', not admitted world '{world_id}'",
                    kb.entry_id,
                    kb.world_id().unwrap_or_default()
                ),
            ));
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

        // The lane is World-owned only (world_id param), so owner_kind='world'
        // and the non-World owner columns are NULL. The native governance
        // columns are left at their shared defaults (NULL/NULL): v1.191 P1 T3
        // removed the legacy `creator_only` column from `kb_key_blocks`, and
        // compute output never assigns a holder or disclosure (authoring
        // admission owns that).
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, owner_kind, world_id, character_id, \
              actor_world_binding_id, block_type, canonical_name, status, \
              body_json, source_anchor_json, created_at, updated_at) \
             VALUES (?, 'world', ?, NULL, NULL, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&kb.entry_id)
        .bind(world_id)
        .bind(block_type_str)
        .bind(&kb.canonical_name)
        .bind(&kb.status)
        .bind(&body_json)
        .bind(&source_anchor_json)
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("insert new_key_block '{}': {e}", kb.entry_id),
        })?;

        created += 1;
    }

    Ok(created)
}

/// Build the truncated proposals payload for an over-budget response.
fn build_truncated_proposals() -> Value {
    json!({
        "schema_version": 1,
        "state_delta": [],
        "timeline_events": [],
        "new_key_blocks": [],
        "battle_report": {
            "kind": "truncated",
            "_truncated": true,
            "reason": format!(
                "response exceeds {RESPONSE_BYTE_CAP} bytes; full output available in run detail"
            ),
        },
    })
}

/// Parse an RFC 3339 timestamp, falling back to the Unix epoch.
fn parse_rfc3339(raw: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .unwrap_or_else(|_| chrono::DateTime::UNIX_EPOCH.into())
        .with_timezone(&chrono::Utc)
}
