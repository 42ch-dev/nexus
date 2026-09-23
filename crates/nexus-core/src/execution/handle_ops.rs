//! Family-typed execution operations on [`ExecutionHandle`] (v1.190 P3-T2).
//!
//! The four operations the execution authority owns for the transport:
//! schedule add/signal, durable workspace commit, and the bounded run-event
//! page, plus the authorized bounded subscription family (P1-T1) that streams
//! the same ring to a native pull/release client. Each is a typed entry point
//! over an authority the handle already owns — the transport keeps its HTTP
//! envelope, status codes and CLI composition; this module only removes the
//! transport's need to reach into scheduler or commit internals directly.
//!
//! Three invariants hold across every operation here:
//!
//! 1. **Owner fence.** Every entry point first checks the coordinator's
//!    draining barrier (the same T1 admission predicate `ensure_driving` and
//!    `admit_schedule` gate on), so an operation reaching a closing owner is
//!    refused with `Closing` rather than writing beside the in-flight drain.
//! 2. **Stored Actor admission.** Schedule eligibility is decided from the
//!    durable row (status/policy re-read per transition), never a
//!    process-global flag.
//! 3. **Gate policy.** A `driven_v1` row is only published after the preset's
//!    declared gates pass — or after an audited `force_gates` bypass — exactly
//!    as the transport path does. The typed seam cannot enqueue a gated preset
//!    on a missing, foreign or invalid Work.
//!
//! The run-event reads (page and subscription) add their own ordering rule:
//! the principal is verified and the run's durable ROOT ownership resolved
//! BEFORE any ring, epoch or cursor is consulted, so neither surface can leak
//! existence, ancestry or history for a run the caller does not own.

use std::sync::Arc;

use nexus_contracts::generated::core::{
    CoreRunEventsResponse, CoreRunEventsResponseEventsItem, CoreRunEventsResponseEventsItemKind,
    CoreRunEventsResponseNextSequence, CoreRunEventsResponseRunId, CoreWorkflowEventBatch,
    CoreWorkflowEventBatchEventsItem, CoreWorkflowEventBatchEventsItemEvent,
    CoreWorkflowSubscribeRequest, CoreWorkflowSubscription,
    CoreWorkflowSubscriptionSubscriptionId,
};
use nexus_contracts::generated::daemon_api::orchestration::sessions::list_sessions_query::ListSessionsQuery;
use nexus_contracts::generated::daemon_api::orchestration::sessions::list_sessions_response::{
    ListSessionsResponse, NexusOrchestrationSessionSummary as ListedSessionWire,
    NexusPaginationInfo as SessionPaginationInfo,
};
use nexus_contracts::generated::daemon_api::orchestration::sessions::session_detail_response::{
    NexusOrchestrationSessionSummary as DetailSessionWire, SessionDetailResponse,
    SessionDetailResponseWorkspaceCommit,
};
use nexus_contracts::generated::daemon_api::schedule::edit_core_context_request::EditCoreContextRequest;
use nexus_contracts::generated::daemon_api::schedule::edit_core_context_response::EditCoreContextResponse;
use nexus_contracts::generated::daemon_api::schedule::inspect_schedule_response::{
    InspectScheduleResponse, NexusScheduleSummary as InspectedScheduleWire,
};
use nexus_contracts::generated::daemon_api::schedule::list_schedules_query::ListSchedulesQuery;
use nexus_contracts::generated::daemon_api::schedule::list_schedules_response::{
    ListSchedulesResponse, NexusPaginationInfo as SchedulePaginationInfo,
    NexusScheduleSummary as ListedScheduleWire,
};
use nexus_contracts::local::orchestration::preset_gate::{FailedGate, PresetGatesFailed};
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AddScheduleResponse, ScheduleConcurrencyRequest, SignalScheduleRequest,
    SignalScheduleResponse,
};
use nexus_contracts::local::schedule::{
    CoreContextVersion, EditOp, ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId,
    ScheduleStatus,
};
use nexus_contracts::CoreRunEventsRequest;
use nexus_orchestration::engine::SessionId;
use nexus_orchestration::preset_gates::{
    evaluate_gates, GateEvalError, PresetInput, PreviousPresetLookup, PreviousPresetResult,
    WorkSnapshot,
};
use nexus_orchestration::schedule::supervisor::ScheduleCancelDisposition;

use crate::error::{CoreError, CoreResult};
use crate::execution::capabilities::{ToolContext, ToolExecuteRequest};
#[cfg(feature = "compute")]
use crate::execution::compute::ComputeContext;
use crate::execution::lifecycle::ExecutionHandle;
use crate::execution::run_events::{
    PageError, PullOutcome, SubscribeError, WorkflowSubscription,
};
use crate::execution::workflow::{RunControlError, RunEventPort, RunSignal};
use crate::principal::Principal;
use crate::PresetError;
use nexus_contracts::generated::core::core_tool_execute_response::CoreToolExecuteResponse;

/// Default page size for a cursorless run-event read.
const DEFAULT_RUN_EVENT_LIMIT: usize = 64;

/// Default page size for a cursorless durable list read (retained default).
const DEFAULT_LIST_LIMIT: i64 = 100;

/// Maximum page size a durable list read serves (retained cap).
const MAX_LIST_LIMIT: i64 = 500;

/// Maximum length for a `force_gates` audit reason (mirrors the transport).
const MAX_REASON_LEN: usize = 512;

impl ExecutionHandle {
    /// Insert a new schedule for the admitted principal's creator.
    ///
    /// The preset's declared gates are evaluated BEFORE any write, and a
    /// `force_gates` bypass records an audit row first. The row is then
    /// inserted through the attached supervisor with its frozen admission
    /// payload (source identity, structured input, role bindings and the
    /// request's own concurrency declaration), so a drive-enabled row is never
    /// observable without the payload its admission will read. Eligibility is
    /// still decided later by the tick, from the STORED row — this call does
    /// not pre-admit.
    ///
    /// # Errors
    /// `Forbidden` when the request names a foreign creator, `Closing` when the
    /// owner is shutting down, `Busy` for a duplicate row, `InvalidInput` for a
    /// malformed `force_gates` request, `Preset` for a failed gate evaluation,
    /// and the mapped storage error on a failed insert.
    pub async fn add_schedule(
        &self,
        principal: &Principal,
        request: AddScheduleRequest,
    ) -> CoreResult<AddScheduleResponse> {
        self.ensure_admitting()?;
        if request.creator_id != principal.creator_id() {
            return Err(CoreError::Forbidden {
                resource: format!(
                    "schedule for creator {} (principal owns {})",
                    request.creator_id,
                    principal.creator_id()
                ),
            });
        }

        // The bypass reason is validated and the bypass AUDITED before anything
        // else can fail. The daemon writes the audit and schedule rows in one
        // transaction; this seam keeps ONE insertion authority (the supervisor,
        // which owns the row+deps+seed transaction) and instead guarantees the
        // audit trail is never missing — an audit row without a schedule records
        // an ATTEMPTED bypass, which is the conservative side.
        //
        // Ordering is load-bearing: an audit written after a fallible
        // precondition would be silently skipped exactly when the operation is
        // refused, so a bypass attempt that never reached a row would leave no
        // trace at all. Every step below this point can therefore refuse the
        // insert and still leave the attempt recorded.
        let audit_reason = validate_bypass_reason(&request)?;
        if let Some(reason) = &audit_reason {
            self.write_force_gates_audit(&request, reason).await?;
        }

        // Resolve the preset ONCE: the same resolution supplies both the gate
        // set that must pass and the frozen source identity, so policy and the
        // frozen payload cannot disagree.
        let loaded = self.resolve_preset(&request.preset_id)?;

        // A bypass skips gate evaluation entirely — it IS the audited override.
        // A normal request must pass every gate the preset declares.
        if audit_reason.is_none() {
            let empty_gates: &[nexus_contracts::local::orchestration::preset_gate::Gate] = &[];
            let gates = loaded.as_ref().map_or(empty_gates, |preset| {
                preset.manifest.preset.gates.as_slice()
            });
            self.enforce_gate_policy(&request, gates).await?;
        }

        let supervisor = self.schedule_supervisor()?;
        let schedule_id = new_schedule_id();
        let schedule = build_schedule(&schedule_id, &request);
        // C-2/I-5: a `driven_v1` row must never be published without its
        // frozen admission payload.
        let descriptor = self.freeze_descriptor(&request, loaded.as_ref())?;
        let seed = request.seed.clone().unwrap_or_default();
        supervisor
            .insert_pending_with_descriptor_and_seed(schedule, descriptor.as_deref(), &seed)
            .await
            .map_err(map_supervisor_error)?;
        Ok(AddScheduleResponse {
            schedule_id,
            status: "pending".to_string(),
            core_context_version: 0,
        })
    }

    /// List the admitted Creator's durable schedules.
    ///
    /// The scope is the STORED ownership: the query is bound to the principal's
    /// creator, and an explicit foreign `creator_id` filter is refused BEFORE
    /// any query runs — never answered with a silently empty page. The response
    /// is the generated snake_case public DTO (the legacy `camelCase` local
    /// summary is not a public read shape).
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` when the
    /// principal was not minted by this service, `Forbidden` for a foreign
    /// creator filter, `InvalidInput` for an unsupported sort key or cursor,
    /// and the mapped storage error otherwise.
    pub async fn list_schedules(
        &self,
        principal: &Principal,
        query: ListSchedulesQuery,
    ) -> CoreResult<ListSchedulesResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let creator_id = principal.creator_id().to_string();
        refuse_foreign_filter(query.creator_id.as_deref(), &creator_id, "schedules")?;

        let order = schedule_order(query.sort.as_deref())?;
        let offset = decode_offset_cursor(query.cursor.as_deref())?;
        let limit = list_limit(query.limit);

        let pool = self.coordinator().pool();
        // The page and its count come from ONE read snapshot, so `has_more` and
        // the page cannot disagree under a concurrent add/delete.
        let mut tx = pool.begin().await.map_err(|e| crate::error::db_err(&e))?;
        // The requested `status` filter is part of the bound value set and the
        // SAME fragment is appended to the page query and its count, so a
        // filtered page can never be paginated against an unfiltered total.
        let status_filter = if query.status.is_some() {
            " AND status = ?"
        } else {
            ""
        };
        // SAFETY: dynamic SQL — only the whitelisted ORDER BY identifiers above
        // and the fixed `status_filter` fragment are interpolated; every value
        // (creator, status, limit, offset) is bound.
        let mut page = sqlx::query_as::<_, ScheduleRow>(sqlx::AssertSqlSafe(format!(
            "SELECT schedule_id, creator_id, preset_id, status, execution_policy,
                    current_session_id, label, current_core_context_version,
                    created_at, updated_at, concurrency_kind
             FROM creator_schedules
             WHERE creator_id = ?{status_filter}
             ORDER BY {order}
             LIMIT ? OFFSET ?"
        )))
        .bind(&creator_id);
        if let Some(status) = query.status.as_deref() {
            page = page.bind(status);
        }
        let rows = page
            .bind(limit)
            .bind(offset)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| crate::error::db_err(&e))?;
        // SAFETY: dynamic SQL — the SAME `status_filter` fragment as the page.
        let mut count = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM creator_schedules WHERE creator_id = ?{status_filter}"
        )))
        .bind(&creator_id);
        if let Some(status) = query.status.as_deref() {
            count = count.bind(status);
        }
        let total: i64 = count
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| crate::error::db_err(&e))?;
        tx.commit().await.map_err(|e| crate::error::db_err(&e))?;

        let has_more = total > offset.saturating_add(limit);
        Ok(ListSchedulesResponse {
            items: rows.into_iter().map(listed_schedule_wire).collect(),
            pagination: SchedulePaginationInfo {
                has_more,
                limit,
                next_cursor: has_more.then(|| encode_offset_cursor(offset.saturating_add(limit))),
            },
        })
    }

    /// Inspect one of the admitted Creator's schedules.
    ///
    /// A foreign id and an absent id close with the SAME `NotFound`: the scope
    /// is the STORED owner (`schedule_id` + `creator_id` in one predicate), so
    /// a caller can never probe another creator's ids or observe a difference
    /// between "not yours" and "does not exist". Read-only: no mutation, no
    /// admission, and therefore no run is ever minted by an inspect.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for an absent or foreign schedule, and the mapped
    /// storage error otherwise.
    pub async fn inspect_schedule(
        &self,
        principal: &Principal,
        schedule_id: String,
    ) -> CoreResult<InspectScheduleResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let row = self.owned_schedule(principal, &schedule_id).await?;
        let pool = self.coordinator().pool();
        let depends_on: Vec<String> = sqlx::query_scalar(
            "SELECT depends_on FROM schedule_dependencies
             WHERE schedule_id = ? ORDER BY depends_on",
        )
        .bind(&schedule_id)
        .fetch_all(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?;

        Ok(InspectScheduleResponse {
            concurrency_kind: row.concurrency_kind.clone(),
            depends_on,
            schedule: inspected_schedule_wire(row),
        })
    }

    /// List the admitted Creator's durable workflow runs.
    ///
    /// The page is read from the DURABLE `orchestration_sessions` rows (root
    /// runs only — `parent_session_id IS NULL`), never from an in-memory engine
    /// map or a Host session list, so a run that already settled (or was
    /// recovered) is listed exactly as it is stored.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `Forbidden` for a foreign creator filter, `InvalidInput` for
    /// an unsupported sort key or cursor, and the mapped storage error.
    pub async fn list_workflow_sessions(
        &self,
        principal: &Principal,
        query: ListSessionsQuery,
    ) -> CoreResult<ListSessionsResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let creator_id = principal.creator_id().to_string();
        refuse_foreign_filter(query.creator_id.as_deref(), &creator_id, "sessions")?;

        let order = session_order(query.sort.as_deref())?;
        let offset = decode_offset_cursor(query.cursor.as_deref())?;
        let limit = list_limit(query.limit);

        let pool = self.coordinator().pool();
        let mut tx = pool.begin().await.map_err(|e| crate::error::db_err(&e))?;
        // SAFETY: dynamic SQL — only the whitelisted ORDER BY identifiers above
        // are interpolated; every value is bound.
        let rows = sqlx::query_as::<_, SessionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT session_id, creator_id, preset_id, status, current_task_id, run_state_json
             FROM orchestration_sessions
             WHERE parent_session_id IS NULL AND creator_id = ?
             ORDER BY {order}
             LIMIT ? OFFSET ?"
        )))
        .bind(&creator_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| crate::error::db_err(&e))?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orchestration_sessions
             WHERE parent_session_id IS NULL AND creator_id = ?",
        )
        .bind(&creator_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| crate::error::db_err(&e))?;
        tx.commit().await.map_err(|e| crate::error::db_err(&e))?;

        let has_more = total > offset.saturating_add(limit);
        Ok(ListSessionsResponse {
            items: rows.into_iter().map(listed_session_wire).collect(),
            pagination: SessionPaginationInfo {
                has_more,
                limit,
                next_cursor: has_more.then(|| encode_offset_cursor(offset.saturating_add(limit))),
            },
        })
    }

    /// Read one durable workflow run of the admitted Creator.
    ///
    /// Root runs only, scoped by the STORED owner: an unknown id, a foreign id
    /// and a CHILD session id all close with the same `NotFound`. A child run is
    /// never independently authorized by stripping `:child:` from a caller's
    /// string — the public session is the root run the schedule owns.
    ///
    /// The same authorized row also carries the OPTIONAL `workspace_commit`
    /// projection (contract §4): the durable revision of this run's own
    /// checkpointed `workspace.commit` capability output, read from
    /// `orchestration_sessions.context_json` — the durable graph context —
    /// AFTER the root row and its stored Creator matched. It is a projection of
    /// an EXISTING checkpoint, never an inference: the run's
    /// `RunStateWire.state_revision` is not a commit revision, file bytes are
    /// not hashed here, and no other context value is exposed.
    ///
    /// `context_json` embeds the run's chat history, so it is never loaded into
    /// memory: the projection is evaluated in SQL with the same
    /// `json_valid`/`json_type` guards the recovery projection uses (a corrupt
    /// or non-object context yields nothing instead of raising). The field is
    /// present only when the LAST capability the graph invoked was
    /// `workspace.commit` AND the stored `_capability_output` is exactly that
    /// capability's successful output shape — `{revision: <non-empty>,
    /// committed: true}` with no other member, i.e. its own
    /// `additionalProperties: false` output schema. Every other shape yields
    /// nothing:
    ///
    /// - a run that never committed (the pair is absent, or names another
    ///   capability): absent;
    /// - a commit that failed or was never checkpointed (the failure record is
    ///   `_capability_error`; a stale/other capability's output does not match
    ///   the shape): absent;
    /// - a malformed/foreign output (wrong types, `committed` not `true`,
    ///   extra members, empty revision): absent.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for an absent, foreign or child session, and the
    /// mapped storage error otherwise.
    pub async fn get_workflow_session(
        &self,
        principal: &Principal,
        session_id: String,
    ) -> CoreResult<SessionDetailResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let pool = self.coordinator().pool();
        let mut row = sqlx::query_as::<_, SessionRow>(
            "SELECT session_id, creator_id, preset_id, status, current_task_id, run_state_json,
                    CASE
                        WHEN json_valid(context_json)
                             AND json_type(context_json, '$.data') = 'object'
                             AND json_extract(context_json, '$.data._capability_name') = 'workspace.commit'
                             AND json_type(context_json, '$.data._capability_output') = 'object'
                             AND json_type(context_json, '$.data._capability_output.revision') = 'text'
                             AND length(json_extract(context_json, '$.data._capability_output.revision')) > 0
                             AND json_type(context_json, '$.data._capability_output.committed') = 'true'
                             AND (SELECT COUNT(*) FROM json_each(
                                      json_extract(context_json, '$.data._capability_output'))) = 2
                        THEN json_extract(context_json, '$.data._capability_output.revision')
                        ELSE NULL
                    END AS workspace_commit_revision
             FROM orchestration_sessions
             WHERE session_id = ? AND parent_session_id IS NULL AND creator_id = ?",
        )
        .bind(&session_id)
        .bind(principal.creator_id())
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("workflow session {session_id}"),
        })?;

        // `committed` is the capability's own post-condition: the SQL
        // projection above only matched an output that carries `true`.
        let revision = row.workspace_commit_revision.take();
        Ok(SessionDetailResponse {
            session: detail_session_wire(row),
            workspace_commit: revision.map(|revision| SessionDetailResponseWorkspaceCommit {
                revision,
                committed: true,
            }),
        })
    }

    /// Append (or structurally edit) a schedule's core context.
    ///
    /// The request is the existing `{op, body?, patch?, path?}` shape with no
    /// caller-supplied version: the version is derived inside the shared
    /// [`CoreContextManager`], which commits the immutable version row and the
    /// schedule's pointer advance in ONE transaction. The Steer order
    /// (append, then resume) therefore observes a durable version before the
    /// resume, and the next execution boundary reads that committed version.
    ///
    /// `replace` stays refused (a user edit never overwrites system-managed
    /// context), and a terminal schedule refuses the edit: neither is a
    /// half-applied version.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for an absent or foreign schedule, `InvalidInput`
    /// for an unknown/`replace` op or a missing op field, `Coded`
    /// (`workflow_state_conflict`) for a terminal schedule or a lost version
    /// race, and the mapped storage error otherwise.
    pub async fn edit_core_context(
        &self,
        principal: &Principal,
        schedule_id: String,
        request: EditCoreContextRequest,
    ) -> CoreResult<EditCoreContextResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let supervisor = self.schedule_supervisor()?;
        let row = self.owned_schedule(principal, &schedule_id).await?;
        if matches!(row.status.as_str(), "completed" | "cancelled" | "failed") {
            return Err(CoreError::Coded {
                code: "workflow_state_conflict".to_string(),
                message: format!(
                    "schedule {schedule_id} is in terminal status '{}'; core-context edits are not allowed",
                    row.status
                ),
            });
        }

        let op = parse_core_context_edit(&request)?;
        let record = supervisor
            .core_context_manager()
            .apply_user_edit(
                &ScheduleId(schedule_id),
                op,
                Some(principal.creator_id().to_string()),
            )
            .await
            .map_err(map_context_error)?;
        Ok(EditCoreContextResponse {
            new_version: i64::from(record.version.0),
        })
    }

    /// Commit a validated change manifest through the durable workspace
    /// authority this owner was bound to.
    ///
    /// The manifest crosses the generated contract; the durable commit keeps
    /// its retained owner, digest idempotence and OCC pre-image checks.
    ///
    /// # Errors
    /// `Forbidden` for a principal with no creator, `Closing` when the owner is
    /// shutting down, `NotFound` when no commit authority is bound, and the
    /// mapped conflict/validation/storage refusal.
    pub async fn commit_workspace(
        &self,
        principal: &Principal,
        request: nexus_contracts::CoreWorkspaceCommitRequest,
    ) -> CoreResult<nexus_contracts::CoreWorkspaceCommitResponse> {
        self.ensure_admitting()?;
        if principal.creator_id().trim().is_empty() {
            return Err(CoreError::Forbidden {
                resource: "workspace commit requires an admitted principal".into(),
            });
        }
        let authority = self
            .workspace_commit_authority()
            .ok_or_else(|| CoreError::NotFound {
                resource: "workspace commit authority (no execution owner bound)".into(),
            })?
            .clone();
        authority.commit(request).await
    }

    /// Apply a lifecycle signal to an existing schedule.
    ///
    /// Ownership is enforced BEFORE mutation: the durable row's `creator_id`
    /// must equal the admitted principal's creator, so a caller cannot pause,
    /// resume or cancel a foreign schedule by guessing its id.
    ///
    /// `resume` and `cancel` are routed to the SAME run the schedule already
    /// owns (v1.195 P0-T4, §3.4):
    ///
    /// - `pause` flips the durable row through the supervisor.
    /// - `resume` on an ADMITTED schedule signals `Resume` to that owned run —
    ///   never a second admission, never a second workflow. A plain resume
    ///   does not continue a human wait: the run keeps its durable wait token
    ///   and the signal closes with the exact `workflow_state_conflict`. A
    ///   successful resume then reconciles the durable row to the run it owns
    ///   (see [`Self::reconcile_resumed_row`]) so list/inspect never keep
    ///   reading a stale `paused`, and re-drives that SAME run through the
    ///   coordinator's single-flight owner — the status mutation alone would
    ///   leave a run whose driver already stopped (a converge/merge park)
    ///   claiming `running` with nobody driving it (v1.195 P0-T6).
    /// - `cancel` on an admitted schedule cancels that owned run through the
    ///   coordinator's single cancel owner (durable cancel-intent fence →
    ///   run token → bounded owned-Host teardown → terminal `cancelled`, or
    ///   `interrupted` when cleanup cannot be confirmed). A schedule that owns
    ///   NO run is cancelled by one CAS on the same fence the admission claim
    ///   writes on, so a concurrent admission either loses that fence or
    ///   supplies the run this cancel then reaches. The response carries the
    ///   DURABLE outcome — a provider acknowledgement is never cancel success,
    ///   and an unconfirmed cleanup is reported as `interrupted`.
    /// - `start`/`advance`/`continue` stay refused: no selected operation uses
    ///   them, and the vocabulary is not extended with invented journeys.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `NotFound` when the schedule
    /// is absent or owned by another creator, `Busy` for an ineligible
    /// transition, `Coded` `workflow_state_conflict`/`workflow_wait_conflict`
    /// when the run's durable state refuses the signal, `InvalidInput` for a
    /// signal this seam does not serve, `Internal` for a storage fault.
    pub async fn signal_schedule(
        &self,
        principal: &Principal,
        schedule_id: String,
        request: SignalScheduleRequest,
    ) -> CoreResult<SignalScheduleResponse> {
        self.ensure_admitting()?;
        let supervisor = self.schedule_supervisor()?;
        self.require_schedule_owner(principal, &schedule_id).await?;

        let status = match request.signal.as_str() {
            "pause" => {
                if !supervisor
                    .pause_schedule(&schedule_id)
                    .await
                    .map_err(map_supervisor_error)?
                {
                    return Err(CoreError::Busy);
                }
                "paused"
            }
            "resume" => {
                let row = self.owned_schedule(principal, &schedule_id).await?;
                match row.current_session_id {
                    // Admitted: the signal goes to the run this schedule
                    // already owns. A manual wait is never implicitly
                    // continued — the engine's wait/in-flight fence refuses
                    // the plain resume with its exact durable conflict.
                    Some(run_id) => {
                        let result = self
                            .coordinator()
                            .signal_run(&SessionId(run_id.clone()), RunSignal::Resume)
                            .await
                            .map_err(map_run_control_error)?;
                        // The durable row must follow the run it owns: `pause`
                        // writes `paused` to the row without touching the run,
                        // so a successful resume must not leave the public
                        // list/inspect projection claiming `paused`.
                        self.reconcile_resumed_row(principal, &schedule_id, &run_id)
                            .await?;
                        // The status mutation alone leaves the run with NO
                        // owner: the driver that stopped on a converge/merge
                        // park does not come back, so `running` would name a
                        // run nothing is driving. Re-drive THIS SAME root
                        // through the single coordinator owner (v1.195 P0-T6).
                        //
                        // Left LAST, after the row followed the run, so the
                        // reconciliation fences against the resumed status
                        // instead of racing the fresh driver. `ensure_driving`
                        // is single-flight and its durable-state gate still
                        // refuses terminal/interrupted/human-wait states, so
                        // a manual wait is never implicitly continued and no
                        // second driver is ever spawned.
                        self.coordinator()
                            .ensure_driving(&SessionId(run_id.clone()))
                            .await
                            .map_err(map_run_control_error)?;
                        return Ok(SignalScheduleResponse {
                            schedule_id,
                            status: result.status,
                            current_wait_id: result.current_wait_id,
                        });
                    }
                    // No run yet: the smart resume admits the row's ONE run
                    // exactly as the clock tick would (it reports `pending`
                    // when admission is not yet possible), so the response
                    // carries the store's answer rather than an assumption.
                    None => {
                        let outcome = supervisor
                            .resume_schedule(&schedule_id)
                            .await
                            .map_err(map_supervisor_error)?;
                        return Ok(SignalScheduleResponse {
                            schedule_id,
                            status: outcome,
                            current_wait_id: None,
                        });
                    }
                }
            }
            "cancel" => {
                // ONE CAS against the admission fence: a row that owns no run
                // is durably `cancelled` here; a row that owns a run (already,
                // or claimed by the concurrent admission that won the fence)
                // supplies that run for the coordinator's cancel.
                let disposition = supervisor
                    .cancel_schedule(&schedule_id, principal.creator_id())
                    .await
                    .map_err(map_supervisor_error)?;
                match disposition {
                    ScheduleCancelDisposition::Cancelled => "cancelled",
                    ScheduleCancelDisposition::Admitted(run_id) => {
                        let result = self
                            .coordinator()
                            .cancel_run(&SessionId(run_id))
                            .await
                            .map_err(map_run_control_error)?;
                        // The durable winner: `cancelled` only when stop is
                        // confirmed, `interrupted` when cleanup is not. A
                        // provider acknowledgement alone never reaches here.
                        return Ok(SignalScheduleResponse {
                            schedule_id,
                            status: result.status,
                            current_wait_id: result.current_wait_id,
                        });
                    }
                    // Nothing was written: the row is already terminal (or
                    // otherwise not cancellable). A completed winner is never
                    // relabelled cancelled.
                    ScheduleCancelDisposition::NotCancelled(durable_status) => {
                        return Err(CoreError::Coded {
                            code: "workflow_state_conflict".to_string(),
                            message: format!(
                                "cannot cancel schedule {schedule_id}: durable status is \
                                 '{durable_status}'"
                            ),
                        });
                    }
                    ScheduleCancelDisposition::Absent => {
                        return Err(CoreError::NotFound {
                            resource: format!("schedule {schedule_id}"),
                        });
                    }
                }
            }
            "start" | "advance" | "continue" => {
                return Err(CoreError::InvalidInput {
                    field: "signal".into(),
                    reason: format!(
                        "signal '{}' is served by the transport's schedule \
                         orchestration, not the core signal seam",
                        request.signal
                    ),
                });
            }
            other => {
                return Err(CoreError::InvalidInput {
                    field: "signal".into(),
                    reason: format!("unknown signal '{other}'"),
                });
            }
        };
        Ok(SignalScheduleResponse {
            schedule_id,
            status: status.to_string(),
            current_wait_id: None,
        })
    }

    /// Reconcile the owning schedule row after an admitted same-run resume.
    ///
    /// `pause` writes `paused` to the schedule row WITHOUT touching the run it
    /// owns, so a successful resume of that same run must not leave the durable
    /// row claiming `paused`: both public projections (list/inspect) read this
    /// row, and the run's durable status is the authority.
    ///
    /// The write is ONE conditional UPDATE, fenced on the SAME identity the
    /// admission claim froze (`schedule_id` + `creator_id` +
    /// `current_session_id`) and on the row still reading `paused`, and it
    /// applies only while the owned run is DURABLY running — the run's status
    /// is read inside the same statement, so there is no read-then-write
    /// window. An intervening cancel, settlement or ownership change therefore
    /// wins the row and is left untouched: a terminal winner is never
    /// overwritten, and nothing is fabricated when the run's authoritative
    /// status is anything else (that row belongs to the terminal settlement
    /// path).
    ///
    /// # Errors
    /// `Internal` for a storage fault.
    async fn reconcile_resumed_row(
        &self,
        principal: &Principal,
        schedule_id: &str,
        run_id: &str,
    ) -> CoreResult<()> {
        let pool = self.coordinator().pool();
        let now = chrono::Utc::now().timestamp();
        // SAFETY: dynamic SQL — one conditional UPDATE; every value is bound
        // and the interpolated text is constant.
        sqlx::query(
            "UPDATE creator_schedules
                SET status = 'running', updated_at = ?
              WHERE schedule_id = ? AND creator_id = ?
                AND current_session_id = ?
                AND status = 'paused'
                AND EXISTS (
                    SELECT 1 FROM orchestration_sessions
                     WHERE session_id = creator_schedules.current_session_id
                       AND status = 'running')",
        )
        .bind(now)
        .bind(schedule_id)
        .bind(principal.creator_id())
        .bind(run_id)
        .execute(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?;
        Ok(())
    }

    /// Dispatch one host tool through the spine.
    ///
    /// The handle supplies the narrow owned context (pool, home, workspace
    /// path, lifecycle scalars, and the live capability holder) and then runs
    /// the SAME admission pipeline, dispatch and audit the transport runs —
    /// so a non-HTTP caller gets identical refusals, including the audit row.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; otherwise the dispatch
    /// refusal (`Coded` carries the retained lowercase wire code).
    pub async fn execute_tool(
        &self,
        principal: &Principal,
        request: ToolExecuteRequest,
    ) -> CoreResult<CoreToolExecuteResponse> {
        self.ensure_admitting()?;
        if principal.creator_id().trim().is_empty() {
            return Err(CoreError::Forbidden {
                resource: "tool execution requires an admitted principal".into(),
            });
        }
        // The principal must belong to THIS owner's service, not merely be
        // well-formed: `verify_principal` checks the open generation, creator
        // and workspace against the service that established this handle, so
        // a principal minted by a DIFFERENT core (another creator, or a stale
        // open) is refused as `AuthRequired` before the context is built.
        // Without this, a caller holding any valid principal could dispatch
        // through this handle's context — the two halves of the authority
        // would be independently obtainable.
        self.linked_core()?.verify_principal(principal)?;
        let context = self.tool_context()?;
        let value = crate::execution::capabilities::execute_tool(&context, &request).await?;
        Ok(CoreToolExecuteResponse {
            success: true,
            result: value,
        })
    }

    /// Invoke a compute module against a World.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `Internal` when this build
    /// linked no WASM engine, and the compute refusal otherwise.
    #[cfg(feature = "compute")]
    pub async fn compute_run(
        &self,
        principal: &Principal,
        request: nexus_contracts::generated::daemon_api::compute::run_request::RunRequest,
    ) -> CoreResult<nexus_contracts::generated::daemon_api::compute::run_response::RunResponse>
    {
        self.ensure_admitting()?;
        // Same principal binding as `execute_tool`: the creator this call
        // authorizes against must be the one THIS service was opened for.
        self.linked_core()?.verify_principal(principal)?;
        let context = self.compute_context(principal)?;
        crate::execution::compute::compute_run(self.linked_core()?.as_ref(), &context, request)
            .await
    }

    /// Accept a succeeded compute run's proposals, atomically.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down; the accept refusal otherwise.
    #[cfg(feature = "compute")]
    pub async fn accept_compute_run(
        &self,
        principal: &Principal,
        run_id: String,
        request: nexus_contracts::generated::daemon_api::compute::run_accept_request::RunAcceptRequest,
    ) -> CoreResult<
        nexus_contracts::generated::daemon_api::compute::run_accept_response::RunAcceptResponse,
    > {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        crate::execution::compute::accept_compute_run(
            self.linked_core()?.as_ref(),
            principal,
            &run_id,
            request,
        )
        .await
    }

    /// Build the narrow owned tool context for this owner.
    ///
    /// The pool, home, workspace root and capability holder come from the
    /// owner itself, so a dispatch can only ever observe the authority this
    /// owner was admitted with. The process facts are supplied by
    /// [`RunnerDeps::runtime_facts`]; when the composer provided none, the
    /// defaults are reported as-is rather than a fabricated running state.
    ///
    /// # Errors
    /// `Internal` when the owner was established without a nexus home, which
    /// the tool surface requires to resolve the active creator.
    fn tool_context(&self) -> CoreResult<ToolContext> {
        let nexus_home = self
            .nexus_home()
            .ok_or_else(|| CoreError::Internal {
                category: "tool dispatch requires a nexus home (owner was established without one)"
                    .to_string(),
            })?
            .to_path_buf();
        Ok(ToolContext {
            pool: (*self.coordinator().pool()).clone(),
            nexus_home,
            workspace_path: self
                .workspace_root()
                .map(|p| p.to_string_lossy().into_owned()),
            runtime_facts: self.runtime_facts().clone(),
            core: self.linked_core().ok(),
            user_capabilities: Some(self.capability_holder()),
        })
    }

    /// Build the compute context for this owner.
    ///
    /// # Errors
    /// `Forbidden` for a principal with no creator.
    #[cfg(feature = "compute")]
    fn compute_context(&self, principal: &Principal) -> CoreResult<ComputeContext> {
        if principal.creator_id().trim().is_empty() {
            return Err(CoreError::Forbidden {
                resource: "compute requires an admitted principal".into(),
            });
        }
        Ok(ComputeContext {
            creator_id: principal.creator_id().to_string(),
            engine: self.compute_engine(),
            cache: self.compute_cache(),
            serializer: self.compute_serializer(),
        })
    }
    /// Read a bounded page of a run's retained events.
    ///
    /// The run is authorized BEFORE the ring or the cursor is touched: the
    /// scope is the STORED owner (a root `orchestration_sessions` row of the
    /// principal's creator), so an unknown run, a FOREIGN run and a child
    /// session id all close with the same `NotFound` and none of them can infer
    /// existence, epoch or payload from this page.
    ///
    /// This numeric-cursor page is the durable read surface; it is NOT the SSE
    /// subscription (that one keeps `<epoch>:<sequence>` cursors). It shares
    /// the same bounded ring, so caps and explicit gaps cannot drift.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for an absent/foreign/child run or a missing
    /// run-event port, `InvalidInput` for an unparsable cursor.
    pub async fn run_events(
        &self,
        principal: &Principal,
        request: CoreRunEventsRequest,
    ) -> CoreResult<CoreRunEventsResponse> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let run_id = request.run_id.as_str().to_string();
        self.owned_root_run(principal, &run_id).await?;
        // Same re-check as the subscription path: the durable check awaits, so
        // a drain that began during it must not be raced by this read either.
        self.ensure_admitting()?;
        let port = self.run_event_port()?;
        run_event_page(&port, request)
    }

    /// Open ONE authorized subscription to a durable run's bounded event
    /// stream (contract §4).
    ///
    /// Ordering is the contract: the principal is verified, the run's STORED
    /// ownership is resolved, and only then is the ring consulted — so an
    /// unknown or foreign run closes with `NotFound` before any ring, epoch or
    /// cursor is disclosed. A malformed or future cursor is a typed
    /// invalid-input refusal (the transport's pre-header 400); an unresumable
    /// history (prior epoch, evicted ring, restart) is NOT an error but the
    /// single `history_unavailable` control frame the caller streams before it
    /// closes.
    ///
    /// The returned token is opaque and bound to the principal's creator, this
    /// owner's core generation and the one root run.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for an absent/foreign/child run or a missing
    /// run-event port, `InvalidInput` for a malformed or future
    /// `last_event_id`, `Busy` when the run's subscriber cap is reached.
    pub async fn subscribe_workflow_events(
        &self,
        principal: &Principal,
        request: CoreWorkflowSubscribeRequest,
    ) -> CoreResult<CoreWorkflowSubscription> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let run_id = request.run_id.as_str().to_string();
        // Authorization FIRST: nothing about the run below this line runs for a
        // caller who does not durably own it.
        self.owned_root_run(principal, &run_id).await?;
        // The drain fence is re-checked after EVERY await on this path: an
        // owner close that began while the durable check was in flight must not
        // be raced. (The window below this point is closed by the registry's
        // seal, which is atomic with the mint.)
        self.ensure_admitting()?;
        if let Some(observer) = self.subscription_observer() {
            observer.owner_resolved().await;
        }
        let port = self.run_event_port()?;
        let inspect_url = run_inspect_url(&run_id);
        let cursor = request.last_event_id.as_ref().map(|c| c.as_str().to_string());
        let creator_id = principal.creator_id().to_string();
        let generation = self.engine_epoch();
        let subscription = match port.subscribe_live(&run_id, cursor.as_deref(), inspect_url) {
            Ok(live) => WorkflowSubscription::live(creator_id, generation, run_id, live),
            // The ring answered that this history cannot be resumed. That is a
            // stream outcome, not a request failure: the caller gets the one
            // control frame and a closed stream.
            Err(SubscribeError::HistoryUnavailable(wire)) => {
                WorkflowSubscription::unresumable(creator_id, generation, run_id, wire.to_frame())
            }
            Err(SubscribeError::MalformedCursor) => {
                return Err(CoreError::InvalidInput {
                    field: "last_event_id".into(),
                    reason: "must be `<UUID epoch>:<decimal sequence>`".into(),
                });
            }
            Err(SubscribeError::FutureCursor) => {
                return Err(CoreError::InvalidInput {
                    field: "last_event_id".into(),
                    reason: "cursor is ahead of the run's retained events".into(),
                });
            }
            Err(SubscribeError::TooManySubscribers) => return Err(CoreError::Busy),
        };
        let Some(subscription) = self.workflow_subscriptions.mint(subscription) else {
            // The owner sealed its table while this call was in flight. The
            // subscription — and with it the ring subscriber permit attached
            // just above — is dropped by the refused mint, so nothing leaks.
            return Err(CoreError::Closing);
        };
        // The seal covers every mint from here on, but a close that sealed
        // right after this mint already withdrew and released the entry; the
        // caller gets that honest refusal instead of a dead token.
        if self.is_draining() {
            self.workflow_subscriptions.take(subscription.id());
            subscription.release();
            return Err(CoreError::Closing);
        }
        Ok(CoreWorkflowSubscription {
            subscription_id: CoreWorkflowSubscriptionSubscriptionId::try_from(
                subscription.id().to_string(),
            )
            .map_err(|err| CoreError::Internal {
                category: format!("workflow subscription token encode: {err}"),
            })?,
        })
    }

    /// Take the next bounded batch of a subscribed run's stream.
    ///
    /// At most 16 frames / 1 MiB, already encoded, in ring order, from the
    /// retained replay and then the live tail. One pull may be outstanding per
    /// subscription. `closed` reports that the stream ended at this batch (the
    /// run's durable terminal frame, a gone ring, or a released/closed
    /// subscription) — after which the token is withdrawn.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for a released, foreign or stale-generation
    /// token, `Busy` when a pull is already outstanding.
    pub async fn next_workflow_events(
        &self,
        principal: &Principal,
        subscription_id: String,
    ) -> CoreResult<CoreWorkflowEventBatch> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let subscription = self.owned_subscription(principal, &subscription_id)?;
        let (frames, closed) = match subscription.pull().await {
            PullOutcome::Batch { frames, closed } => (frames, closed),
            PullOutcome::Busy => return Err(CoreError::Busy),
        };
        let events = frames
            .into_iter()
            .map(|frame| {
                let event = CoreWorkflowEventBatchEventsItemEvent::try_from(frame.event)
                    .map_err(|err| CoreError::Internal {
                        category: format!("run-event name encode: {err}"),
                    })?;
                Ok(CoreWorkflowEventBatchEventsItem {
                    id: frame.id,
                    event,
                    data: frame.data,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        if closed {
            // A closed stream serves no further pull: withdraw the token so a
            // repeat call refuses like any other released one.
            self.workflow_subscriptions.take(&subscription_id);
        }
        Ok(CoreWorkflowEventBatch { events, closed })
    }

    /// Release one subscription (a disconnect, an ended stream).
    ///
    /// The run's subscriber permit is freed and a pull blocked on that
    /// subscription wakes with `closed` instead of waiting for a frame that
    /// can no longer arrive. The owner's close releases every subscription it
    /// still holds, so a transport that never reaches this call leaks nothing.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `AuthRequired` for a foreign
    /// principal, `NotFound` for a released, foreign or stale-generation token.
    pub async fn release_workflow_events(
        &self,
        principal: &Principal,
        subscription_id: String,
    ) -> CoreResult<()> {
        self.ensure_admitting()?;
        self.linked_core()?.verify_principal(principal)?;
        let subscription = self.owned_subscription(principal, &subscription_id)?;
        if self.workflow_subscriptions.take(&subscription_id).is_none() {
            // A concurrent release won the withdrawal: the token is gone, so
            // this call refuses exactly like any other released one.
            return Err(CoreError::NotFound {
                resource: format!("workflow event subscription {subscription_id}"),
            });
        }
        subscription.release();
        Ok(())
    }

    /// The subscribed run-event port, or the same `NotFound` the page read
    /// reports when this owner has no registry attached.
    fn run_event_port(&self) -> CoreResult<Arc<dyn RunEventPort>> {
        self.coordinator()
            .run_event_port()
            .ok_or_else(|| CoreError::NotFound {
                resource: "run event ring (no execution owner attached)".into(),
            })
    }

    /// Verify that the principal durably OWNS `run_id` as a ROOT run.
    ///
    /// The durable run row is the authority: a child session id is never
    /// authorized by stripping `:child:` from the caller's string, and an
    /// absent id, a foreign id and a child id all close with the SAME refusal
    /// so this cannot be used to probe another creator's runs (or their
    /// ancestry) before any ring is touched.
    ///
    /// # Errors
    /// `NotFound` for an absent, foreign or child run; the mapped storage error.
    async fn owned_root_run(&self, principal: &Principal, run_id: &str) -> CoreResult<()> {
        let pool = self.coordinator().pool();
        let owned: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM orchestration_sessions
             WHERE session_id = ? AND parent_session_id IS NULL AND creator_id = ?",
        )
        .bind(run_id)
        .bind(principal.creator_id())
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?;
        owned
            .map(|_| ())
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("workflow session {run_id}"),
            })
    }

    /// Resolve a token this owner still serves AND that was minted for this
    /// principal and this core generation.
    ///
    /// A released, unknown, foreign-creator or stale-generation token all close
    /// with the same `NotFound`, so a token cannot be used to probe another
    /// creator's subscriptions.
    ///
    /// # Errors
    /// `NotFound` for every unresolvable token.
    fn owned_subscription(
        &self,
        principal: &Principal,
        subscription_id: &str,
    ) -> CoreResult<Arc<WorkflowSubscription>> {
        let subscription = self
            .workflow_subscriptions
            .get(subscription_id)
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("workflow event subscription {subscription_id}"),
            })?;
        if !subscription.is_bound_to(principal.creator_id(), self.engine_epoch()) {
            return Err(CoreError::NotFound {
                resource: format!("workflow event subscription {subscription_id}"),
            });
        }
        Ok(subscription)
    }

    /// The owner-level fence every entry point checks.
    ///
    /// Reuses the coordinator's T1 admission barrier (the same predicate
    /// `ensure_driving`/`admit_schedule` gate on), so an operation reaching a
    /// closing owner cannot install an effect beside the in-flight drain.
    fn ensure_admitting(&self) -> CoreResult<()> {
        if self.coordinator().is_draining() {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    /// Read one durable schedule row scoped to the principal's STORED creator.
    ///
    /// The predicate carries BOTH the id and the stored `creator_id`, so a
    /// foreign row and an absent row produce the same `NotFound` — the scope is
    /// the stored owner, never a caller-supplied creator.
    ///
    /// # Errors
    /// `NotFound` for an absent or foreign row; the mapped storage error.
    async fn owned_schedule(&self, principal: &Principal, schedule_id: &str) -> CoreResult<ScheduleRow> {
        let pool = self.coordinator().pool();
        sqlx::query_as::<_, ScheduleRow>(
            "SELECT schedule_id, creator_id, preset_id, status, execution_policy,
                    current_session_id, label, current_core_context_version,
                    created_at, updated_at, concurrency_kind
             FROM creator_schedules
             WHERE schedule_id = ? AND creator_id = ?",
        )
        .bind(schedule_id)
        .bind(principal.creator_id())
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("schedule {schedule_id}"),
        })
    }

    /// Require the durable schedule row to belong to the principal's creator.
    ///
    /// A missing row and a foreign row return the SAME `NotFound`, so a caller
    /// cannot probe for the existence of another creator's schedules. Delegates
    /// to [`Self::owned_schedule`] so every reader shares ONE stored-ownership
    /// predicate.
    async fn require_schedule_owner(
        &self,
        principal: &Principal,
        schedule_id: &str,
    ) -> CoreResult<()> {
        self.owned_schedule(principal, schedule_id).await.map(|_| ())
    }

    /// Write the audited `force_gates` bypass row.
    async fn write_force_gates_audit(
        &self,
        request: &AddScheduleRequest,
        reason: &str,
    ) -> CoreResult<()> {
        let pool = self.coordinator().pool();
        let mut conn = pool.acquire().await.map_err(|e| crate::error::db_err(&e))?;
        let params = nexus_local_db::ForceGatesAuditParams {
            audit_id: format!("fga_{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f")),
            preset_id: request.preset_id.clone(),
            work_id: request
                .input
                .as_ref()
                .and_then(|v| v.get("work_id"))
                .and_then(|w| w.as_str())
                .map(str::to_string)
                .or_else(|| work_id_from_seed(request.seed.as_deref()))
                .unwrap_or_else(|| "unknown".to_string()),
            creator_id: request.creator_id.clone(),
            reason: reason.to_string(),
            forced_at: chrono::Utc::now().to_rfc3339(),
        };
        nexus_local_db::insert_force_gates_audit(&mut conn, &params)
            .await
            .map_err(|e| CoreError::Internal {
                category: format!("failed to write force-gates audit row: {e}"),
            })
    }

    /// Evaluate the preset's declared gates, refusing when any gate fails.
    ///
    /// Only reached for a NON-bypass request: a `force_gates` call is the
    /// audited override and skips evaluation entirely.
    async fn enforce_gate_policy(
        &self,
        request: &AddScheduleRequest,
        gates: &[nexus_contracts::local::orchestration::preset_gate::Gate],
    ) -> CoreResult<()> {
        if gates.is_empty() {
            return Ok(());
        }

        // A gated preset ALWAYS requires work_id for evaluation. Failing closed
        // here is what stops the typed seam enqueuing a gated preset unchecked.
        let work_id = request
            .input
            .as_ref()
            .and_then(|v| v.get("work_id"))
            .and_then(|w| w.as_str())
            .map(str::to_string)
            .or_else(|| work_id_from_seed(request.seed.as_deref()));
        let Some(work_id) = work_id else {
            return Err(gate_failure(
                &request.preset_id,
                "",
                vec![FailedGate {
                    kind: "work_field".to_string(),
                    expected: "work_id must be provided for gated preset".to_string(),
                    actual: "omitted".to_string(),
                    remediation: "Pass work_id via input.work_id or seed.work_id, \
                                  or use force_gates=true with a reason."
                        .to_string(),
                }],
            ));
        };

        // The Work must exist FOR THIS CREATOR: a foreign or unknown Work is a
        // gate failure, never an empty snapshot that silently passes.
        let pool = self.coordinator().pool();
        let row: Option<WorkSnapshotRow> = sqlx::query_as::<_, WorkSnapshotRow>(
            "SELECT work_profile, work_ref, workspace_slug, intake_status, \
                    world_id, status, current_stage, total_planned_chapters \
             FROM works WHERE work_id = ? AND creator_id = ?",
        )
        .bind(&work_id)
        .bind(&request.creator_id)
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?;

        let Some(row) = row else {
            return Err(gate_failure(
                &request.preset_id,
                &work_id,
                vec![FailedGate {
                    kind: "work_field".to_string(),
                    expected: "work must exist for the owning creator".to_string(),
                    actual: format!("work {work_id} not found for this creator"),
                    remediation: "Create the Work first, then schedule the preset.".to_string(),
                }],
            ));
        };

        let snapshot = WorkSnapshot {
            work_id: work_id.clone(),
            creator_id: request.creator_id.clone(),
            work_profile: row.work_profile,
            work_ref: row.work_ref,
            workspace_slug: row.workspace_slug,
            intake_status: row.intake_status,
            world_id: row.world_id,
            status: row.status,
            current_stage: row.current_stage,
            title: None,
            total_planned_chapters: row.total_planned_chapters,
        };

        let mut vars = std::collections::HashMap::new();
        if let Some(obj) = request.input.as_ref().and_then(|v| v.as_object()) {
            for (k, v) in obj {
                vars.insert(k.clone(), v.to_string());
            }
        }
        if let Some(ref wr) = snapshot.work_ref {
            vars.insert("work_ref".to_string(), wr.clone());
        }
        vars.insert("work_id".to_string(), work_id.clone());
        let preset_input = PresetInput { vars };

        // Filesystem-kind gates resolve against the SAME frozen root the engine
        // and commit authority use; without one there is nothing to resolve
        // against, so fall back to the nexus home exactly as the transport does.
        let workspace_root = self
            .workspace_root()
            .map(std::path::Path::to_path_buf)
            .or_else(|| self.nexus_home().map(std::path::Path::to_path_buf))
            .ok_or_else(|| CoreError::Internal {
                category: "gate evaluation requires a workspace root or nexus home".into(),
            })?;
        let lookup = DbPreviousPresetLookup { pool };

        match evaluate_gates(
            gates,
            &request.preset_id,
            &snapshot,
            &preset_input,
            &workspace_root,
            &lookup,
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(failure)) => Err(CoreError::Preset(PresetError::Rejected {
                code: "preset_gates_failed".to_string(),
                message: render_gate_failure(&failure),
            })),
            Err(err) => Err(CoreError::Internal {
                category: format!("gate evaluation error: {err}"),
            }),
        }
    }

    /// The attached schedule supervisor, or a typed `NotFound`.
    fn schedule_supervisor(
        &self,
    ) -> CoreResult<Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>> {
        self.coordinator()
            .schedule_supervisor()
            .ok_or_else(|| CoreError::NotFound {
                resource: "schedule supervisor (no execution owner attached)".into(),
            })
    }

    /// Resolve the preset (and therefore its gates) through the shared resolver.
    ///
    /// A resolution failure is reported, never swallowed: a preset that cannot
    /// be loaded cannot have its gates evaluated, so it must not be enqueued.
    fn resolve_preset(&self, preset_id: &str) -> CoreResult<Option<nexus_preset::LoadedPreset>> {
        // `_system.*` presets are never admitted and carry no gates.
        if preset_id.starts_with("_system.") {
            return Ok(None);
        }
        let home = self.nexus_home().ok_or_else(|| CoreError::Internal {
            category: "schedule insert requires a nexus home to resolve the preset".into(),
        })?;
        let registry = self
            .capability_holder()
            .get()
            .ok_or_else(|| CoreError::Internal {
                category: "capability registry unavailable; cannot resolve preset".into(),
            })?;
        nexus_preset::resolve_preset(preset_id, home, &registry)
            .map(Some)
            .map_err(|e| CoreError::Internal {
                category: format!("failed to resolve preset '{preset_id}': {e}"),
            })
    }

    /// Freeze a new row's admission payload at insertion.
    ///
    /// The frozen identity is the SAME resolution that supplied the gates. Role
    /// bindings prefer the caller's explicit `agent_bindings` (validated to
    /// cover every prompt role the preset declares) and fall back to the
    /// coordinator's configured default provider — never to request-supplied
    /// catalog order.
    fn freeze_descriptor(
        &self,
        request: &AddScheduleRequest,
        loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> CoreResult<Option<Vec<u8>>> {
        use nexus_orchestration::run_state::{AgentBinding, RunDescriptorV1};

        let Some(loaded) = loaded else {
            // `_system.*`: never admitted, so no descriptor is required.
            return Ok(None);
        };
        let source = loaded
            .source_identity
            .clone()
            .ok_or_else(|| CoreError::Internal {
                category: format!(
                    "preset '{}' has no content-addressed source identity",
                    request.preset_id
                ),
            })?;

        let input = request
            .input
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

        // Explicit request bindings win, but only when they cover the preset's
        // declared prompt roles; a partial map is a refusal, not a silent
        // partial binding.
        let roles = nexus_preset::required_prompt_roles(loaded);
        let agent_bindings = if let Some(explicit) = request.agent_bindings.as_ref() {
            let mut bindings = std::collections::HashMap::with_capacity(explicit.len());
            for (role, dto) in explicit {
                bindings.insert(
                    role.clone(),
                    AgentBinding {
                        provider_id: dto.provider_id.clone(),
                        model: dto.model.clone(),
                    },
                );
            }
            let missing: Vec<String> = roles
                .iter()
                .filter(|role| !bindings.contains_key(*role))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "agent_bindings".into(),
                    reason: format!(
                        "preset '{}' requires bindings for role(s) {}",
                        request.preset_id,
                        missing.join(", ")
                    ),
                });
            }
            bindings
        } else if roles.is_empty() {
            std::collections::HashMap::new()
        } else {
            // Bind the coordinator (and the provider string) before the closure
            // below captures it: `self.coordinator()` returns a temporary `Arc`
            // that would otherwise be dropped at the end of the `let-else`.
            let coordinator = self.coordinator();
            let Some(provider_id) = coordinator.binding_provider() else {
                return Err(CoreError::Internal {
                    category: format!(
                        "preset '{}' requires {} prompt role binding(s) but no configured \
                         default binding provider exists",
                        request.preset_id,
                        roles.len()
                    ),
                });
            };
            roles
                .into_iter()
                .map(|role| {
                    (
                        role,
                        AgentBinding {
                            provider_id: provider_id.to_owned(),
                            model: None,
                        },
                    )
                })
                .collect()
        };

        // work_id resolves from the structured input OR a JSON seed, matching
        // the retained add path: a gated preset whose Work is only named in the
        // seed must still freeze that Work into the row/descriptor.
        let work_id = input
            .get("work_id")
            .and_then(|w| w.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| work_id_from_seed(request.seed.as_deref()));

        let descriptor = RunDescriptorV1 {
            creator_id: request.creator_id.clone(),
            work_id,
            workspace_root: self
                .workspace_root()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default(),
            preset_id: request.preset_id.clone(),
            preset_version: 1,
            source,
            input,
            agent_bindings,
            parent_session_id: None,
            graph_name: None,
        };
        serde_json::to_vec(&descriptor)
            .map(Some)
            .map_err(|e| CoreError::Internal {
                category: format!("failed to serialize execution descriptor: {e}"),
            })
    }
}

/// The retained public inspect path for one durable run (W4/O3 route).
///
/// The `history_unavailable` control frame carries it so a subscriber that
/// cannot resume the stream re-reads the run instead of an invented history
/// (contract §4). The path is the frozen public route, so the control frame
/// names the SAME resource the caller inspects.
fn run_inspect_url(run_id: &str) -> String {
    format!("/v1/daemon/orchestration/sessions/{run_id}")
}

/// Read a bounded page of a run's retained events.
///
/// The page comes from the SAME bounded ring the SSE transport replays from
/// (through the run-event port), so the item/byte caps and the explicit-gap
/// frames are identical on both surfaces. `resync_required` reports a
/// retention-trimmed cursor.
///
/// # Errors
/// `NotFound` when no ring is retained for `run_id`; `InvalidInput` for an
/// unparsable cursor.
pub fn run_event_page(
    port: &Arc<dyn RunEventPort>,
    request: CoreRunEventsRequest,
) -> CoreResult<CoreRunEventsResponse> {
    // `after_sequence` is a validated decimal-string newtype and `limit` a
    // `NonZeroU64`, so both are already shape-checked by the contract.
    let after = request
        .after_sequence
        .as_ref()
        .map(|cursor| {
            cursor
                .as_str()
                .parse::<u64>()
                .map_err(|_| CoreError::InvalidInput {
                    field: "after_sequence".into(),
                    reason: "must be a non-negative decimal sequence".into(),
                })
        })
        .transpose()?;
    let limit = usize::try_from(request.limit.get()).unwrap_or(DEFAULT_RUN_EVENT_LIMIT);
    // Unwrap the request's newtypes once, up front: the ring takes `&str` and
    // the response needs the owned id, so binding it here avoids cloning.
    let run_id = String::from(request.run_id);
    let page = port
        .read_page(&run_id, after, limit)
        .map_err(|err| match err {
            PageError::UnknownRun(run_id) => CoreError::NotFound {
                resource: format!("run event ring {run_id}"),
            },
        })?;

    // `kind` is a minLength-1 newtype. The ring only ever writes non-empty
    // event names, so a conversion failure is a ring bug worth surfacing rather
    // than papering over with a fabricated kind.
    let events =
        page.frames
            .into_iter()
            .map(|frame| {
                let kind = CoreRunEventsResponseEventsItemKind::try_from(frame.kind.as_str())
                    .map_err(|err| CoreError::Internal {
                        category: format!("run-event kind encode: {err}"),
                    })?;
                Ok(CoreRunEventsResponseEventsItem {
                    sequence: frame.sequence,
                    kind,
                    payload: serde_json::from_str::<serde_json::Value>(&frame.data)
                        .ok()
                        .and_then(|v| v.as_object().cloned())
                        .unwrap_or_default(),
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
    Ok(CoreRunEventsResponse {
        run_id: CoreRunEventsResponseRunId::try_from(run_id).map_err(|err| {
            CoreError::InvalidInput {
                field: "run_id".into(),
                reason: err.to_string(),
            }
        })?,
        events,
        next_sequence: CoreRunEventsResponseNextSequence::try_from(page.next_sequence.to_string())
            .map_err(|err| CoreError::Internal {
                category: format!("run-event watermark encode: {err}"),
            })?,
        terminal: page.terminal,
        resync_required: page.resync_required,
    })
}

/// One durable `creator_schedules` row as read by the public list/inspect.
///
/// `concurrency_kind` is read by inspect (the list selects it too, so a single
/// row shape serves both reads).
#[derive(sqlx::FromRow)]
struct ScheduleRow {
    schedule_id: String,
    creator_id: String,
    preset_id: String,
    status: String,
    execution_policy: String,
    current_session_id: Option<String>,
    label: Option<String>,
    current_core_context_version: i64,
    created_at: i64,
    updated_at: i64,
    concurrency_kind: String,
}

/// One durable root `orchestration_sessions` row as read by the public list and
/// the session detail.
#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id: String,
    creator_id: String,
    preset_id: String,
    status: String,
    current_task_id: Option<String>,
    /// Raw durable v1 run state — the actionable failure record lives here.
    run_state_json: Option<Vec<u8>>,
    /// The run's checkpointed `workspace.commit` revision, projected in SQL
    /// from `context_json` (contract §4). `None` unless the durable graph
    /// context holds that capability's exact successful output.
    ///
    /// `#[sqlx(default)]`: the LIST page deliberately does not select it — a
    /// page of rows must never evaluate an unbounded graph-context blob that
    /// embeds chat history — so only the detail read carries the column.
    #[sqlx(default)]
    workspace_commit_revision: Option<String>,
}

/// Project a stored schedule row into the LIST wire item.
///
/// `execution` stays absent: neither the acceptance scenarios (S0-2/S0-5) nor
/// any consumer reads the A2/A7 run projection yet, and the schema marks the
/// field optional. The durable identity/status/policy/pointer are what the
/// overlay and inspect are specified against.
fn listed_schedule_wire(row: ScheduleRow) -> ListedScheduleWire {
    ListedScheduleWire {
        created_at: row.created_at.to_string(),
        creator_id: row.creator_id,
        current_core_context_version: row.current_core_context_version,
        current_session_id: row.current_session_id,
        execution: None,
        execution_policy: row.execution_policy,
        label: row.label,
        preset_id: row.preset_id,
        schedule_id: row.schedule_id,
        status: row.status,
        updated_at: row.updated_at.to_string(),
    }
}

/// Project a stored schedule row into the INSPECT wire item. The generated
/// codegen inlines the shared summary schema per response module, so the two
/// wire items are distinct types with the same shape.
fn inspected_schedule_wire(row: ScheduleRow) -> InspectedScheduleWire {
    InspectedScheduleWire {
        created_at: row.created_at.to_string(),
        creator_id: row.creator_id,
        current_core_context_version: row.current_core_context_version,
        current_session_id: row.current_session_id,
        execution: None,
        execution_policy: row.execution_policy,
        label: row.label,
        preset_id: row.preset_id,
        schedule_id: row.schedule_id,
        status: row.status,
        updated_at: row.updated_at.to_string(),
    }
}

/// Project a stored run row into the LIST wire item.
fn listed_session_wire(row: SessionRow) -> ListedSessionWire {
    let failure_reason = failure_reason(&row);
    ListedSessionWire {
        creator_id: row.creator_id,
        current_task_id: row.current_task_id,
        execution: None,
        failure_reason,
        preset_id: row.preset_id,
        session_id: row.session_id,
        status: row.status,
    }
}

/// Project a stored run row into the DETAIL wire item (same fields, distinct
/// generated type from `session_detail_response`).
fn detail_session_wire(row: SessionRow) -> DetailSessionWire {
    let failure_reason = failure_reason(&row);
    DetailSessionWire {
        creator_id: row.creator_id,
        current_task_id: row.current_task_id,
        execution: None,
        failure_reason,
        preset_id: row.preset_id,
        session_id: row.session_id,
        status: row.status,
    }
}

/// Actionable failure reason from the durable v1 run state (e.g. an
/// unconfirmed cancel cleanup that left the run `interrupted`).
fn failure_reason(row: &SessionRow) -> Option<String> {
    row.run_state_json
        .as_deref()
        .and_then(|blob| {
            serde_json::from_slice::<nexus_orchestration::run_state::RunStateV1>(blob).ok()
        })
        .and_then(|state| state.failure)
        .map(|failure| failure.message)
}

/// Refuse an explicit creator filter that is not the admitted Creator.
///
/// Query scope is the active Creator; a foreign filter is a refusal BEFORE any
/// query runs, never a silently empty (and therefore misleading) page.
///
/// # Errors
/// `Forbidden` naming both creators.
fn refuse_foreign_filter(
    filter: Option<&str>,
    creator_id: &str,
    resource: &str,
) -> CoreResult<()> {
    match filter {
        Some(other) if other != creator_id => Err(CoreError::Forbidden {
            resource: format!("{resource} for creator {other} (principal owns {creator_id})"),
        }),
        _ => Ok(()),
    }
}

/// Page size for a durable list read: the retained default, capped at the
/// retained maximum.
fn list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT)
}

/// Decode the opaque offset cursor (the retained `v1:<offset>` grammar).
///
/// # Errors
/// `InvalidInput` for a cursor this owner did not mint.
fn decode_offset_cursor(cursor: Option<&str>) -> CoreResult<i64> {
    let Some(raw) = cursor else {
        return Ok(0);
    };
    raw.strip_prefix("v1:")
        .and_then(|offset| offset.parse::<i64>().ok())
        .filter(|offset| *offset >= 0)
        .ok_or_else(|| CoreError::InvalidInput {
            field: "cursor".into(),
            reason: "invalid pagination cursor; pass the `next_cursor` value returned by the \
                     previous response unchanged"
                .into(),
        })
}

/// Encode an offset cursor (opaque to clients).
fn encode_offset_cursor(offset: i64) -> String {
    format!("v1:{offset}")
}

/// Order terms for the durable schedule list.
///
/// `schemas/daemon-api/schedule/list-schedules-query.schema.json`: allowed keys
/// `created_at` (**default**), `updated_at`, `status`, `preset_id`, `label`,
/// `-` prefix for descending. The default direction is the retained daemon's
/// newest-first `created_at DESC`.
///
/// # Errors
/// `InvalidInput` for a key outside that set.
fn schedule_order(sort: Option<&str>) -> CoreResult<String> {
    order_by(
        sort,
        &["created_at", "updated_at", "status", "preset_id", "label"],
        "created_at DESC",
        "schedule_id",
        "schedule",
    )
}

/// Order terms for the durable orchestration-session list.
///
/// `schemas/daemon-api/orchestration/sessions/list-sessions-query.schema.json`:
/// allowed keys `session_id` (**default**), `creator_id`, `preset_id`,
/// `status`, `-` prefix for descending. The default key is deliberately
/// `session_id` (ascending) and NOT the schedule default: a key outside the
/// session schema's allowed set would make the documented default unsortable.
///
/// # Errors
/// `InvalidInput` for a key outside that set.
fn session_order(sort: Option<&str>) -> CoreResult<String> {
    order_by(
        sort,
        &["session_id", "creator_id", "preset_id", "status"],
        "session_id ASC",
        "session_id",
        "session",
    )
}

/// Build the `ORDER BY` clause from the request's sort terms.
///
/// `default_order` is the calling resource's OWN schema-declared default (the
/// two list schemas declare different defaults), so no shared fallback can
/// contradict a schema. Only whitelisted column names are interpolated, and the
/// resource's id is always appended as a deterministic tie-break so two rows
/// sharing a sort key cannot swap across pages of an offset cursor.
///
/// # Errors
/// `InvalidInput` for a sort key outside the schema's allowed set.
fn order_by(
    sort: Option<&str>,
    allowed: &[&str],
    default_order: &str,
    tie_break: &str,
    resource: &str,
) -> CoreResult<String> {
    let mut clauses: Vec<String> = Vec::new();
    if let Some(raw) = sort {
        for term in raw.split(',') {
            let term = term.trim();
            if term.is_empty() {
                continue;
            }
            let (descending, key) = term
                .strip_prefix('-')
                .map_or((false, term), |stripped| (true, stripped));
            if !allowed.contains(&key) {
                return Err(CoreError::InvalidInput {
                    field: "sort".into(),
                    reason: format!(
                        "unsupported {resource} sort key '{key}'; allowed: {}",
                        allowed.join(", ")
                    ),
                });
            }
            clauses.push(format!("{key} {}", if descending { "DESC" } else { "ASC" }));
        }
    }
    if clauses.is_empty() {
        clauses.push(default_order.to_string());
    }
    if !clauses.iter().any(|clause| clause.starts_with(tie_break)) {
        clauses.push(format!("{tie_break} ASC"));
    }
    Ok(clauses.join(", "))
}

/// Translate the generated PATCH body into the derivation edit op.
///
/// `replace` is a KNOWN op the core still refuses (a user edit may not
/// overwrite system-managed context): refused here as invalid input so the
/// refusal is a 400, never a storage-layer 500 after a payload was computed.
///
/// # Errors
/// `InvalidInput` for an unknown op, `replace`, or a missing op field.
fn parse_core_context_edit(request: &EditCoreContextRequest) -> CoreResult<EditOp> {
    match request.op.as_str() {
        "append" => Ok(EditOp::Append {
            body: required_field(request.body.as_ref(), "append", "body")?.clone(),
        }),
        "struct_merge" => Ok(EditOp::StructMerge {
            patch: required_field(request.patch.as_ref(), "struct_merge", "patch")?.clone(),
        }),
        "struct_remove" => Ok(EditOp::StructRemove {
            path: required_field(request.path.as_ref(), "struct_remove", "path")?.clone(),
        }),
        "replace" => Err(CoreError::InvalidInput {
            field: "op".into(),
            reason: "op 'replace' is not allowed: a user edit may only append or \
                     structurally merge (system-managed context is never overwritten)"
                .into(),
        }),
        other => Err(CoreError::InvalidInput {
            field: "op".into(),
            reason: format!("unknown op '{other}'; expected append|struct_merge|struct_remove"),
        }),
    }
}

/// Borrow an op's required field, or refuse with the field's name.
///
/// # Errors
/// `InvalidInput` naming the missing field.
fn required_field<'a, T>(value: Option<&'a T>, op: &str, field: &str) -> CoreResult<&'a T> {
    value.ok_or_else(|| CoreError::InvalidInput {
        field: field.into(),
        reason: format!("{op} requires the '{field}' field"),
    })
}

/// Map a core-context refusal onto the neutral taxonomy.
fn map_context_error(err: nexus_orchestration::schedule::derivation::CoreContextError) -> CoreError {
    use nexus_orchestration::schedule::derivation::CoreContextError as E;
    match err {
        E::NotFound(schedule_id) => CoreError::NotFound {
            resource: format!("schedule {schedule_id}"),
        },
        E::VersionNotFound(schedule_id, version) => CoreError::NotFound {
            resource: format!("core-context version {version} of schedule {schedule_id}"),
        },
        E::UserEditValidation(reason) | E::PresetHookValidation(reason) => CoreError::InvalidInput {
            field: "op".into(),
            reason,
        },
        // A lost pointer advance: the append and its version row were rolled
        // back, so this is a state conflict (the caller re-reads and retries),
        // never a successful version.
        E::VersionRace(schedule_id, version) => CoreError::Coded {
            code: "workflow_state_conflict".to_string(),
            message: format!(
                "core-context version race on schedule {schedule_id}: the pointer no longer \
                 names version {version}"
            ),
        },
        // More than one schedule names the run: the ownership the whole
        // core-context contract rests on is ambiguous, so the request is a
        // state conflict rather than a silent pick of one schedule's context.
        E::AmbiguousOwnership(run_id, owners, schedules) => CoreError::Coded {
            code: "workflow_state_conflict".to_string(),
            message: format!(
                "run {run_id} is named as current_session_id by {owners} schedules \
                 ({schedules}); refusing to choose one"
            ),
        },
        E::Serde(e) => CoreError::InvalidInput {
            field: "op".into(),
            reason: e.to_string(),
        },
        E::Database(e) => crate::error::db_err(&e),
    }
}

/// Build a durable pending row from the generated add request.
///
/// Every payload-bearing field crosses: `creator/preset/label/scheduled_at` and
/// the dependency list verbatim, and the request's concurrency declaration
/// (including a `parallel_with` whitelist) maps onto the stored concurrency.
fn build_schedule(schedule_id: &str, request: &AddScheduleRequest) -> Schedule {
    let concurrency = match request.concurrency.as_ref() {
        None | Some(ScheduleConcurrencyRequest::Serial) => ScheduleConcurrency::Serial,
        Some(ScheduleConcurrencyRequest::ParallelAny) => ScheduleConcurrency::ParallelAny,
        Some(ScheduleConcurrencyRequest::ParallelWith { schedule_ids }) => {
            ScheduleConcurrency::ParallelWith(ParallelWithIds {
                schedule_ids: schedule_ids
                    .iter()
                    .map(|id| ScheduleId(id.clone()))
                    .collect(),
            })
        }
    };
    Schedule {
        id: ScheduleId(schedule_id.to_string()),
        creator_id: request.creator_id.clone(),
        preset_id: request.preset_id.clone(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency,
        depends_on: request
            .depends_on
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|id| ScheduleId(id.clone()))
            .collect(),
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: request.scheduled_at.clone(),
        label: request.label.clone(),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    }
}

/// Build a `preset_gates_failed` refusal from the failed-gate list.
fn gate_failure(preset_id: &str, work_id: &str, failed_gates: Vec<FailedGate>) -> CoreError {
    CoreError::Preset(PresetError::Rejected {
        code: "preset_gates_failed".to_string(),
        message: render_gate_failure(&PresetGatesFailed {
            error: "preset_gates_failed".to_string(),
            preset_id: preset_id.to_string(),
            work_id: work_id.to_string(),
            failed_gates,
        }),
    })
}

/// Render a gate failure into the retained human-readable message.
fn render_gate_failure(failure: &PresetGatesFailed) -> String {
    use std::fmt::Write as _;

    let mut msg = format!("preset gates failed for '{}'", failure.preset_id);
    for gate in &failure.failed_gates {
        let _ = write!(
            msg,
            "; {}: expected {}, got {}",
            gate.kind, gate.expected, gate.actual
        );
    }
    msg
}

/// Validate a `force_gates` bypass request, returning the audited reason.
///
/// Pure and free of I/O on purpose: it runs BEFORE any fallible precondition,
/// so the decision to audit is made without touching the store. A non-bypass
/// request validates nothing and yields `None`.
///
/// # Errors
/// `InvalidInput` when a bypass is requested with a missing, oversized, or
/// control-character-bearing reason — a silent bypass is never permitted.
fn validate_bypass_reason(request: &AddScheduleRequest) -> CoreResult<Option<String>> {
    if !request.force_gates {
        return Ok(None);
    }
    let raw = request.reason.as_deref().unwrap_or("");
    if raw.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "reason".into(),
            reason: "force_gates requires a non-empty reason (audit-logged)".into(),
        });
    }
    let sanitized = sanitize_reason(raw);
    if sanitized.len() > MAX_REASON_LEN {
        return Err(CoreError::InvalidInput {
            field: "reason".into(),
            reason: format!(
                "reason exceeds maximum length ({MAX_REASON_LEN} chars); got {} chars",
                sanitized.len()
            ),
        });
    }
    if sanitized != raw {
        return Err(CoreError::InvalidInput {
            field: "reason".into(),
            reason: "reason contains ANSI escape sequences or control characters".to_string(),
        });
    }
    Ok(Some(raw.to_string()))
}

/// Strip ANSI escape sequences and control characters from a reason string.
///
/// Mirrors the transport's sanitizer without pulling `regex` into this cohort:
/// the `ESC [ params letter` grammar is matched by hand.
fn sanitize_reason(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while matches!(chars.peek(), Some(d) if d.is_ascii_digit() || *d == ';') {
                    chars.next();
                }
                chars.next(); // the terminating letter
            }
            continue;
        }
        if c.is_control() && c != '\n' {
            continue;
        }
        out.push(c);
    }
    out
}

/// Fresh schedule id in the daemon's retained `SCH<timestamp>` shape.
fn new_schedule_id() -> String {
    format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"))
}

/// `work_id` from a JSON seed, mirroring the transport's fallback.
fn work_id_from_seed(seed: Option<&str>) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(seed?)
        .ok()
        .and_then(|v| {
            v.get("work_id")
                .and_then(|w| w.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

/// One Work-snapshot row for gate evaluation.
#[derive(sqlx::FromRow)]
struct WorkSnapshotRow {
    work_profile: Option<String>,
    work_ref: Option<String>,
    workspace_slug: Option<String>,
    intake_status: Option<String>,
    world_id: Option<String>,
    status: Option<String>,
    current_stage: Option<String>,
    total_planned_chapters: Option<i64>,
}

/// `PreviousPreset` gate lookup over the Creator DB.
struct DbPreviousPresetLookup {
    pool: Arc<sqlx::SqlitePool>,
}

impl PreviousPresetLookup for DbPreviousPresetLookup {
    fn lookup(
        &self,
        preset_id: &str,
        work_id: &str,
        _creator_id: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<PreviousPresetResult, GateEvalError>>
                + Send
                + '_,
        >,
    > {
        let pool = Arc::clone(&self.pool);
        let preset_id = preset_id.to_string();
        let work_id = work_id.to_string();
        Box::pin(async move {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM creator_schedules \
                 WHERE preset_id = ? AND status = 'completed' AND work_id = ?",
            )
            .bind(&preset_id)
            .bind(&work_id)
            .fetch_one(pool.as_ref())
            .await
            .map_err(|e| GateEvalError::Database(e.to_string()))?;
            Ok(PreviousPresetResult {
                found: count > 0,
                is_complete: count > 0,
            })
        })
    }
}

/// Map a supervisor refusal onto the neutral taxonomy.
fn map_supervisor_error(
    err: nexus_orchestration::schedule::supervisor::SupervisorError,
) -> CoreError {
    use nexus_orchestration::schedule::supervisor::SupervisorError as E;
    match err {
        E::NotFound(_) => CoreError::NotFound {
            resource: "schedule".into(),
        },
        E::InvalidTransition(..) | E::DuplicateSchedule { .. } => CoreError::Busy,
        E::Database(e) => crate::error::db_err(&e),
    }
}

/// Map a coordinator control refusal onto the neutral taxonomy (§3.4).
///
/// The two control conflicts keep their retained lowercase wire codes and
/// their durable detail (the current status / the exact wait token) so the
/// adapter renders a 409 instead of a 500 for a legitimate control race:
///
/// - a lost/stale human-wait token is `workflow_wait_conflict` (the wait is
///   NOT consumed and no second driver starts);
/// - a run in a state that refuses the signal — terminal, interrupted, still
///   waiting, or mid-step — is `workflow_state_conflict`.
fn map_run_control_error(err: RunControlError) -> CoreError {
    match err {
        RunControlError::ScheduleNotFound(session_id) => CoreError::NotFound {
            resource: format!("workflow session {session_id}"),
        },
        RunControlError::WaitConflict {
            session_id,
            status,
            current_wait_id,
        } => CoreError::Coded {
            code: "workflow_wait_conflict".to_string(),
            message: format!(
                "run {session_id} is waiting (status {status}, current_wait_id \
                 {current_wait_id:?}); the exact durable wait token is required"
            ),
        },
        RunControlError::StateConflict(session_id, reason) => CoreError::Coded {
            code: "workflow_state_conflict".to_string(),
            message: format!("run {session_id} refuses the signal: {reason}"),
        },
        RunControlError::ReconstructionUnavailable {
            session_id,
            reason,
        } => CoreError::Coded {
            code: "workflow_state_conflict".to_string(),
            message: format!(
                "run {session_id} cannot be reconstructed ({reason}); the human wait is \
                 preserved and legal actions are cancel-only"
            ),
        },
        RunControlError::NotEligible(schedule_id, reason) => CoreError::Coded {
            code: "workflow_state_conflict".to_string(),
            message: format!("schedule {schedule_id} is not eligible: {reason}"),
        },
        RunControlError::Closing => CoreError::Closing,
        // Retryable capacity refusal (never a 500): no drive started and no
        // durable work was touched.
        RunControlError::RunEventCapacity(_) => CoreError::Busy,
        RunControlError::NoWorkspace(reason) => CoreError::ServiceUnavailable(reason),
        other @ (RunControlError::PresetLoad(..)
        | RunControlError::Admission(_)
        | RunControlError::Drive(_)
        | RunControlError::ScheduleUpdate(_)) => CoreError::Internal {
            category: other.to_string(),
        },
    }
}
