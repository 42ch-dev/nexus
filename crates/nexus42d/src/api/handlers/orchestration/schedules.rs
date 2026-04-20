//! Schedule HTTP handlers: 8 endpoints per WS7 §9.
//!
//! Endpoints:
//! - POST   /schedules — Add schedule
//! - GET    /schedules — List schedules (optional filters)
//! - GET    /schedules/{id} — Inspect schedule
//! - PATCH  /schedules/{id}/core-context — Apply EditOp
//! - GET    /schedules/{id}/core-context — Current content
//! - GET    /schedules/{id}/core-context-history — Version history
//! - POST   /schedules/{id}/signal — Pause/Resume/Cancel/Start/Advance
//! - DELETE /schedules/{id} — Remove (terminal only)

use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use nexus_contracts::local::schedule::http::*;
use nexus_contracts::local::schedule::{
    CoreContextAuthor, EditOp, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper: get supervisor from state, returning 503 if not wired
// ---------------------------------------------------------------------------

fn require_supervisor(
    state: &WorkspaceState,
) -> Result<Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>, (StatusCode, String)>
{
    state.schedule_supervisor().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "schedule supervisor not configured".to_string(),
        )
    })
}

// ---------------------------------------------------------------------------
// POST /schedules — Add schedule
// ---------------------------------------------------------------------------

/// `POST /v1/local/orchestration/schedules` — add a new schedule.
pub async fn add_schedule(
    state: State<WorkspaceState>,
    Json(body): Json<AddScheduleRequest>,
) -> Result<(StatusCode, Json<AddScheduleResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;

    // Generate a schedule ID (simple timestamp-based for pre-1.0)
    let schedule_id = format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));

    let concurrency = match &body.concurrency {
        Some(c) => match c {
            ScheduleConcurrencyRequest::Serial => ScheduleConcurrency::Serial,
            ScheduleConcurrencyRequest::ParallelWith { schedule_ids } => {
                ScheduleConcurrency::ParallelWith(
                    nexus_contracts::local::schedule::ParallelWithIds {
                        schedule_ids: schedule_ids.iter().map(|s| ScheduleId(s.clone())).collect(),
                    },
                )
            }
            ScheduleConcurrencyRequest::ParallelAny => ScheduleConcurrency::ParallelAny,
        },
        None => ScheduleConcurrency::Serial,
    };

    let depends_on: Vec<ScheduleId> = body
        .depends_on
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| ScheduleId(s.clone()))
        .collect();

    use nexus_contracts::local::schedule::{CoreContextVersion, Schedule};
    let schedule = Schedule {
        id: ScheduleId(schedule_id.clone()),
        creator_id: body.creator_id.clone(),
        preset_id: body.preset_id.clone(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency,
        depends_on,
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        // V1.5 WS-D: pass scheduled_at from request (Unix timestamp as string)
        scheduled_at: body.scheduled_at.clone(),
        label: body.label.clone(),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    };

    // Insert the schedule row (R2: duplicate detection)
    supervisor.insert_pending(schedule).await.map_err(|e| {
        if matches!(
            e,
            nexus_orchestration::schedule::supervisor::SupervisorError::DuplicateSchedule { .. }
        ) {
            (
                StatusCode::CONFLICT,
                format!("schedule already exists: {e}"),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to create schedule: {e}"),
            )
        }
    })?;

    // Seed core context v0 if seed is provided
    let mut core_version: u32 = 0;
    if let Some(seed) = &body.seed {
        let mgr = supervisor.core_context_manager();
        let sid = ScheduleId(schedule_id.clone());
        let _record = mgr
            .apply_seed(
                &sid,
                seed,
                CoreContextAuthor::User {
                    id: body.creator_id.clone(),
                },
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to seed core context: {e}"),
                )
            })?;
        core_version = 0;
    }

    Ok((
        StatusCode::CREATED,
        Json(AddScheduleResponse {
            schedule_id,
            status: "pending".to_string(),
            core_context_version: core_version,
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /schedules — List schedules
// ---------------------------------------------------------------------------

/// `GET /v1/local/orchestration/schedules` — list schedules with optional filters.
pub async fn list_schedules(
    state: State<WorkspaceState>,
    Query(query): Query<ListSchedulesQuery>,
) -> Result<(StatusCode, Json<ListSchedulesResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let pool = supervisor.pool();

    // SAFETY: dynamic WHERE clause — filters are appended conditionally at runtime.
    // Compile-time checked macro cannot express variable SQL structure.
    let mut sql = String::from(
        "SELECT schedule_id, creator_id, preset_id, status, label,
                current_core_context_version, created_at, updated_at
         FROM creator_schedules WHERE 1=1",
    );

    if query.creator_id.is_some() {
        sql.push_str(" AND creator_id = ?");
    }
    if query.status.is_some() {
        sql.push_str(" AND status = ?");
    }
    sql.push_str(" ORDER BY created_at DESC");

    // SAFETY: dynamic query — see list_schedules SAFETY comment above.
    let mut q = sqlx::query_as::<_, ListRow>(&sql);
    if let Some(ref cid) = query.creator_id {
        q = q.bind(cid);
    }
    if let Some(ref st) = query.status {
        q = q.bind(st);
    }

    let rows: Vec<ListRow> = q.fetch_all(&*pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let schedules = rows.into_iter().map(|r| r.into_summary()).collect();

    Ok((StatusCode::OK, Json(ListSchedulesResponse { schedules })))
}

// ---------------------------------------------------------------------------
// GET /schedules/{schedule_id} — Inspect schedule
// ---------------------------------------------------------------------------

/// `GET /v1/local/orchestration/schedules/{schedule_id}` — inspect a schedule.
pub async fn inspect_schedule(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
) -> Result<Json<InspectScheduleResponse>, (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let pool = supervisor.pool();

    // SAFETY: runtime `sqlx::query_as` — pool obtained via `supervisor.pool()` which returns
    // `Arc<SqlitePool>` (dereferenced to `&SqlitePool`). Compile-time macros require a
    // `&SqlitePool` at the call site but `supervisor.pool()` does not expose the inner
    // reference with sufficient lifetime for the macro expansion. Could be converted in a
    // future pass by inlining the pool reference.
    let row = sqlx::query_as::<_, InspectRow>(
        "SELECT schedule_id, creator_id, preset_id, status, label,
                current_core_context_version, created_at, updated_at,
                concurrency_kind, concurrency_whitelist
         FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&schedule_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("schedule {schedule_id} not found"),
        )
    })?;

    // Load dependencies
    // SAFETY: runtime `sqlx::query_as` — same pool lifetime constraint as inspect_schedule above.
    let deps: Vec<String> = sqlx::query_as::<_, (String,)>(
        "SELECT depends_on FROM schedule_dependencies WHERE schedule_id = ?",
    )
    .bind(&schedule_id)
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?
    .into_iter()
    .map(|(d,)| d)
    .collect();

    let concurrency_kind = row.concurrency_kind.clone();

    Ok(Json(InspectScheduleResponse {
        schedule: row.into_summary(),
        depends_on: deps,
        concurrency_kind,
    }))
}

// ---------------------------------------------------------------------------
// PATCH /schedules/{id}/core-context — Apply EditOp
// ---------------------------------------------------------------------------

/// `PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context` — apply EditOp.
pub async fn edit_core_context(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
    Json(body): Json<EditCoreContextRequest>,
) -> Result<(StatusCode, Json<EditCoreContextResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let sid = ScheduleId(schedule_id.clone());

    // Check schedule exists and is not terminal
    let status = supervisor.status_of(&schedule_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("schedule {schedule_id} not found: {e}"),
        )
    })?;
    match status {
        ScheduleStatus::Completed | ScheduleStatus::Cancelled | ScheduleStatus::Failed => {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "schedule {schedule_id} is in terminal status {status:?}; edits not allowed"
                ),
            ));
        }
        _ => {}
    }

    let mgr = supervisor.core_context_manager();

    let op = parse_edit_op(&body)?;
    let record = mgr.apply_user_edit(&sid, op, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to edit core context: {e}"),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(EditCoreContextResponse {
            new_version: record.version.0,
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /schedules/{id}/core-context — Current content
// ---------------------------------------------------------------------------

/// `GET /v1/local/orchestration/schedules/{schedule_id}/core-context` — current content.
pub async fn get_core_context(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
) -> Result<Json<CoreContextResponse>, (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let mgr = supervisor.core_context_manager();
    let sid = ScheduleId(schedule_id.clone());

    let snapshot = mgr.current_snapshot(&sid).await.map_err(|e| {
        if e.to_string().contains("not found") {
            (
                StatusCode::NOT_FOUND,
                format!("schedule {schedule_id} not found"),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {e}"),
            )
        }
    })?;

    let (payload_kind, content) = match &snapshot.content {
        nexus_contracts::local::schedule::CoreContextPayload::Text { body } => {
            ("text".to_string(), serde_json::json!({ "text": body }))
        }
        nexus_contracts::local::schedule::CoreContextPayload::Struct { body } => {
            ("struct".to_string(), body.clone())
        }
    };

    let derivation_kind = match &snapshot.derivation {
        nexus_contracts::local::schedule::DerivationStep::Seed { .. } => "seed",
        nexus_contracts::local::schedule::DerivationStep::UserEdit { .. } => "user_edit",
        nexus_contracts::local::schedule::DerivationStep::PresetHook { .. } => "preset_hook",
        nexus_contracts::local::schedule::DerivationStep::LlmSummarize { .. } => "llm_summarize",
        nexus_contracts::local::schedule::DerivationStep::PresetSeedExpansion { .. } => {
            "preset_seed_expansion"
        }
    };

    Ok(Json(CoreContextResponse {
        version: snapshot.version.0,
        payload_kind,
        content,
        derivation_kind: derivation_kind.to_string(),
        created_at: snapshot.created_at,
    }))
}

// ---------------------------------------------------------------------------
// GET /schedules/{id}/core-context-history — Version history
// ---------------------------------------------------------------------------

/// `GET /v1/local/orchestration/schedules/{schedule_id}/core-context-history` — version history.
pub async fn get_core_context_history(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
) -> Result<Json<CoreContextHistoryResponse>, (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let pool = supervisor.pool();

    // Query all versions for this schedule, ordered by version DESC
    // SAFETY: runtime `sqlx::query_as` — same pool lifetime constraint as inspect_schedule above.
    let rows = sqlx::query_as::<_, HistoryRow>(
        "SELECT version, payload_kind, content, derivation_kind,
                created_at
         FROM core_context_versions
         WHERE schedule_id = ?
          ORDER BY version DESC",
    )
    .bind(&schedule_id)
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    if rows.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("no core context history for schedule {schedule_id}"),
        ));
    }

    // By default, only return meta (no content). Content inclusion can be
    // added via query parameter in a future iteration.
    let entries: Vec<CoreContextHistoryEntry> = rows
        .iter()
        .map(|r| CoreContextHistoryEntry {
            version: r.version as u32,
            payload_kind: r.payload_kind.clone(),
            content: None,
            derivation_kind: r.derivation_kind.clone(),
            created_at: r.created_at.to_string(),
        })
        .collect();

    Ok(Json(CoreContextHistoryResponse { entries }))
}

// ---------------------------------------------------------------------------
// POST /schedules/{id}/signal — Pause/Resume/Cancel/Start/Advance
// ---------------------------------------------------------------------------

/// `POST /v1/local/orchestration/schedules/{schedule_id}/signal` — pause/resume/cancel/start.
pub async fn signal_schedule(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
    Json(body): Json<SignalScheduleRequest>,
) -> Result<(StatusCode, Json<SignalScheduleResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let pool = supervisor.pool();
    let now = chrono::Utc::now().timestamp();

    // SAFETY: runtime `sqlx::query_as` — same pool lifetime constraint as inspect_schedule above.
    let current_status_str = sqlx::query_as::<_, (String,)>(
        "SELECT status FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&schedule_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let (current_status_str,) = current_status_str.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("schedule {schedule_id} not found"),
        )
    })?;

    // Reject all signals for failed schedules
    if current_status_str == "failed" {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "cannot signal failed schedule {schedule_id}: failed schedules require manual intervention"
            ),
        ));
    }

    let new_status = match body.signal.as_str() {
        "start" => match current_status_str.as_str() {
            "pending" => "running",
            _ => {
                return Err((
                        StatusCode::CONFLICT,
                        format!(
                            "cannot start schedule {schedule_id}: current status is {current_status_str}"
                        ),
                    ));
            }
        },
        "pause" => {
            // R1+R4: Use supervisor method for consistent DB + cache update
            let paused = supervisor.pause_schedule(&schedule_id).await.map_err(|e| {
                if matches!(
                    e,
                    nexus_orchestration::schedule::supervisor::SupervisorError::NotFound(_)
                ) {
                    (
                        StatusCode::NOT_FOUND,
                        format!("schedule {schedule_id} not found"),
                    )
                } else {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("database error: {e}"),
                    )
                }
            })?;
            if !paused {
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "cannot pause schedule {schedule_id}: current status is {current_status_str}"
                    ),
                ));
            }

            return Ok((
                StatusCode::OK,
                Json(SignalScheduleResponse {
                    schedule_id,
                    status: "paused".to_string(),
                }),
            ));
        }
        "resume" => {
            // R3+R7: Smart resume — direct to Running if admitted, else Pending
            let new_status = supervisor.resume_schedule(&schedule_id).await.map_err(|e| {
                if matches!(
                    e,
                    nexus_orchestration::schedule::supervisor::SupervisorError::NotFound(_)
                ) {
                    (StatusCode::NOT_FOUND, format!("schedule {schedule_id} not found"))
                } else if matches!(
                    e,
                    nexus_orchestration::schedule::supervisor::SupervisorError::InvalidTransition(..)
                ) {
                    (StatusCode::CONFLICT, format!("cannot resume schedule {schedule_id}: current status is {current_status_str}"))
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("database error: {e}"))
                }
            })?;

            return Ok((
                StatusCode::OK,
                Json(SignalScheduleResponse {
                    schedule_id,
                    status: new_status,
                }),
            ));
        }
        "cancel" => match current_status_str.as_str() {
            "pending" | "running" | "paused" => {
                // Use supervisor for consistent DB + running cache update.
                // R1 fix: log pause error at warn! level instead of silently ignoring.
                // The pause failure must NOT block the cancel operation — the
                // schedule enters terminal state regardless.
                if current_status_str == "running" {
                    if let Err(e) = supervisor.pause_schedule(&schedule_id).await {
                        tracing::warn!(
                            "pause failed during cancel for schedule {}: {}; continuing with cancel",
                            schedule_id,
                            e
                        );
                    }
                }

                // SAFETY: runtime `sqlx::query` — same pool lifetime constraint as inspect_schedule above.
                sqlx::query(
                        "UPDATE creator_schedules SET status = 'cancelled', terminated_at = ?, updated_at = ?
                         WHERE schedule_id = ?",
                    )
                    .bind(now)
                    .bind(&schedule_id)
                    .execute(&*pool)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("database error: {e}")))?;

                return Ok((
                    StatusCode::OK,
                    Json(SignalScheduleResponse {
                        schedule_id,
                        status: "cancelled".to_string(),
                    }),
                ));
            }
            _ => {
                return Err((
                        StatusCode::CONFLICT,
                        format!(
                            "cannot cancel schedule {schedule_id}: current status is {current_status_str}"
                        ),
                    ));
            }
        },
        "advance" => {
            // Advance is a pass-through signal for the session engine;
            // for now just confirm the schedule is running.
            match current_status_str.as_str() {
                "running" => {
                    return Ok((
                        StatusCode::OK,
                        Json(SignalScheduleResponse {
                            schedule_id,
                            status: "running".to_string(),
                        }),
                    ));
                }
                _ => {
                    return Err((
                        StatusCode::CONFLICT,
                        format!(
                            "cannot advance schedule {schedule_id}: current status is {current_status_str}"
                        ),
                    ));
                }
            }
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "unknown signal '{}'; expected start|pause|resume|cancel|advance",
                    body.signal
                ),
            ));
        }
    };

    // SAFETY: runtime `sqlx::query` — same pool lifetime constraint as inspect_schedule above.
    sqlx::query(
        "UPDATE creator_schedules SET status = ?, updated_at = ?
         WHERE schedule_id = ?",
    )
    .bind(new_status)
    .bind(now)
    .bind(&schedule_id)
    .execute(&*pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    // After starting or resuming, trigger a supervisor tick to potentially
    // start dependent schedules.
    if new_status == "running" || new_status == "pending" {
        let _ = supervisor.tick().await;
    }

    Ok((
        StatusCode::OK,
        Json(SignalScheduleResponse {
            schedule_id,
            status: new_status.to_string(),
        }),
    ))
}

// ---------------------------------------------------------------------------
// DELETE /schedules/{id} — Remove terminal schedule
// ---------------------------------------------------------------------------

/// `DELETE /v1/local/orchestration/schedules/{schedule_id}` — remove terminal schedule.
///
/// **R5 — Delete cascade**: For non-terminal schedules, cancels the active
/// session (if any), NULLs out `current_session_id`, then cancels the schedule
/// before deletion. Terminal schedules are deleted directly.
pub async fn delete_schedule(
    state: State<WorkspaceState>,
    Path(schedule_id): Path<String>,
) -> Result<(StatusCode, Json<DeleteScheduleResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;
    let pool = supervisor.pool();

    // Check if the schedule exists
    let current_status = supervisor.status_of(&schedule_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("schedule {schedule_id} not found: {e}"),
        )
    })?;

    match current_status {
        ScheduleStatus::Completed | ScheduleStatus::Cancelled | ScheduleStatus::Failed => {
            // Terminal: delete directly. FK CASCADE handles dependencies and core_context_versions.
        }
        _ => {
            // Non-terminal: must cancel first.
            // R5: Cancel active session if current_session_id is set, then NULL it out.
            // SAFETY: runtime `sqlx::query_as` — pool lifetime constraint.
            let session_row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT current_session_id FROM creator_schedules WHERE schedule_id = ?",
            )
            .bind(&schedule_id)
            .fetch_optional(&*pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("database error: {e}"),
                )
            })?;

            if let Some((Some(sid),)) = session_row {
                // Cancel the active session by updating its status
                let now = chrono::Utc::now().timestamp();
                // SAFETY: runtime `sqlx::query` — DML for session cancellation.
                sqlx::query(
                    "UPDATE orchestration_sessions SET status = 'cancelled', updated_at = ?
                         WHERE session_id = ? AND status = 'running'",
                )
                .bind(now)
                .bind(&sid)
                .execute(&*pool)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to cancel session: {e}"),
                    )
                })?;
            }

            // NULL out current_session_id on the schedule
            // SAFETY: runtime `sqlx::query` — pool lifetime constraint.
            sqlx::query(
                "UPDATE creator_schedules SET current_session_id = NULL, updated_at = ?
                 WHERE schedule_id = ?",
            )
            .bind(chrono::Utc::now().timestamp())
            .bind(&schedule_id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("database error: {e}"),
                )
            })?;

            // Cancel the schedule
            let now = chrono::Utc::now().timestamp();
            // SAFETY: runtime `sqlx::query` — pool lifetime constraint.
            sqlx::query(
                "UPDATE creator_schedules SET status = 'cancelled', terminated_at = ?, updated_at = ?
                 WHERE schedule_id = ?",
            )
            .bind(now)
            .bind(now)
            .bind(&schedule_id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("database error: {e}"),
                )
            })?;
        }
    }

    // Delete the schedule row (FK CASCADE handles schedule_dependencies and core_context_versions)
    // NOTE: static query — could use compile-time macro in future refactor.
    // Currently uses runtime `query` for consistency with dynamic pattern elsewhere in this module.
    sqlx::query("DELETE FROM creator_schedules WHERE schedule_id = ?")
        .bind(&schedule_id)
        .execute(&*pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {e}"),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(DeleteScheduleResponse { deleted: true }),
    ))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse the HTTP EditCoreContextRequest into an EditOp.
fn parse_edit_op(body: &EditCoreContextRequest) -> Result<EditOp, (StatusCode, String)> {
    match body.op.as_str() {
        "append" => {
            let text = body.body.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "append requires 'body' field".to_string(),
                )
            })?;
            Ok(EditOp::Append { body: text.clone() })
        }
        "replace" => {
            let text = body.body.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "replace requires 'body' field".to_string(),
                )
            })?;
            Ok(EditOp::Replace { body: text.clone() })
        }
        "struct_merge" => {
            let patch = body.patch.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "struct_merge requires 'patch' field".to_string(),
                )
            })?;
            Ok(EditOp::StructMerge {
                patch: patch.clone(),
            })
        }
        "struct_remove" => {
            let path = body.path.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "struct_remove requires 'path' field".to_string(),
                )
            })?;
            Ok(EditOp::StructRemove { path: path.clone() })
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("unknown op '{other}'; expected append|replace|struct_merge|struct_remove"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Row types for SQL queries
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ListRow {
    schedule_id: String,
    creator_id: String,
    preset_id: String,
    status: String,
    label: Option<String>,
    current_core_context_version: i64,
    created_at: i64,
    updated_at: i64,
}

impl ListRow {
    fn into_summary(self) -> ScheduleSummary {
        ScheduleSummary {
            schedule_id: self.schedule_id,
            creator_id: self.creator_id,
            preset_id: self.preset_id,
            status: self.status,
            label: self.label,
            current_core_context_version: self.current_core_context_version as u32,
            created_at: self.created_at.to_string(),
            updated_at: self.updated_at.to_string(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct InspectRow {
    schedule_id: String,
    creator_id: String,
    preset_id: String,
    status: String,
    label: Option<String>,
    current_core_context_version: i64,
    created_at: i64,
    updated_at: i64,
    concurrency_kind: String,
    #[allow(dead_code)]
    concurrency_whitelist: Option<String>,
}

impl InspectRow {
    fn into_summary(self) -> ScheduleSummary {
        ScheduleSummary {
            schedule_id: self.schedule_id,
            creator_id: self.creator_id,
            preset_id: self.preset_id,
            status: self.status,
            label: self.label,
            current_core_context_version: self.current_core_context_version as u32,
            created_at: self.created_at.to_string(),
            updated_at: self.updated_at.to_string(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct HistoryRow {
    version: i64,
    payload_kind: String,
    #[allow(dead_code)]
    content: Vec<u8>,
    derivation_kind: String,
    created_at: i64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_request_parse_valid_signals() {
        for signal in &["pause", "resume", "cancel", "start", "advance"] {
            let req = SignalScheduleRequest {
                signal: signal.to_string(),
            };
            let json = serde_json::to_string(&req).unwrap();
            let back: SignalScheduleRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(back.signal, *signal);
        }
    }

    #[test]
    fn parse_edit_op_append() {
        let body = EditCoreContextRequest {
            op: "append".to_string(),
            body: Some("hello".to_string()),
            patch: None,
            path: None,
        };
        let op = parse_edit_op(&body).unwrap();
        assert!(matches!(op, EditOp::Append { .. }));
    }

    #[test]
    fn parse_edit_op_replace() {
        let body = EditCoreContextRequest {
            op: "replace".to_string(),
            body: Some("new".to_string()),
            patch: None,
            path: None,
        };
        let op = parse_edit_op(&body).unwrap();
        assert!(matches!(op, EditOp::Replace { .. }));
    }

    #[test]
    fn parse_edit_op_struct_merge() {
        let body = EditCoreContextRequest {
            op: "struct_merge".to_string(),
            body: None,
            patch: Some(serde_json::json!({"key": "val"})),
            path: None,
        };
        let op = parse_edit_op(&body).unwrap();
        assert!(matches!(op, EditOp::StructMerge { .. }));
    }

    #[test]
    fn parse_edit_op_struct_remove() {
        let body = EditCoreContextRequest {
            op: "struct_remove".to_string(),
            body: None,
            patch: None,
            path: Some("key".to_string()),
        };
        let op = parse_edit_op(&body).unwrap();
        assert!(matches!(op, EditOp::StructRemove { .. }));
    }

    #[test]
    fn parse_edit_op_unknown_fails() {
        let body = EditCoreContextRequest {
            op: "explode".to_string(),
            body: None,
            patch: None,
            path: None,
        };
        assert!(parse_edit_op(&body).is_err());
    }

    #[test]
    fn parse_edit_op_append_without_body_fails() {
        let body = EditCoreContextRequest {
            op: "append".to_string(),
            body: None,
            patch: None,
            path: None,
        };
        assert!(parse_edit_op(&body).is_err());
    }
}
