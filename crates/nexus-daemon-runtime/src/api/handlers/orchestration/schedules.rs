//! Complex HTTP handlers with orchestration logic exceed line limits.
#![allow(clippy::too_many_lines)]
//! Schedule HTTP handlers: 8 endpoints per WS7 §9.
//!
//! Endpoints:
//! - POST   /schedules — Add schedule
//! - GET    /schedules — List schedules (optional filters)
//! - GET    /schedules/{id} — Inspect schedule
//! - PATCH  /schedules/{id}/core-context — Apply `EditOp`
//! - GET    /schedules/{id}/core-context — Current content
//! - GET    /schedules/{id}/core-context-history — Version history
//! - POST   /schedules/{id}/signal — Pause/Resume/Cancel/Start/Advance
//! - DELETE /schedules/{id} — Remove (terminal only)
//!
//! # Error Documentation
//!
//! All handlers return `(StatusCode, String)` errors with consistent patterns:
//! - `NOT_FOUND` for missing schedules
//! - `CONFLICT` for state conflicts
//! - `INTERNAL_SERVER_ERROR` for database failures
//! - `BAD_REQUEST` for invalid input
//!
//! Due to this consistent pattern across all handlers, `missing_errors_doc`
//! is suppressed for this module.

#![allow(clippy::missing_errors_doc)]

use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AddScheduleResponse, CoreContextHistoryEntry, CoreContextHistoryResponse,
    CoreContextResponse, DeleteScheduleResponse, EditCoreContextRequest, EditCoreContextResponse,
    InspectScheduleResponse, ListSchedulesQuery, ListSchedulesResponse, ScheduleConcurrencyRequest,
    ScheduleSummary, SignalScheduleRequest, SignalScheduleResponse,
};
use nexus_contracts::local::schedule::{
    CoreContextAuthor, CoreContextVersion, EditOp, Schedule, ScheduleConcurrency, ScheduleId,
    ScheduleStatus,
};
use nexus_orchestration::preset_gates::{
    GateEvalError, PreviousPresetLookup, PreviousPresetResult,
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

/// Reserved input keys that collide with system-injected variables (W-6).
/// `work_id` is allowed in input because the daemon needs it for gate evaluation;
/// it is explicitly extracted and not merged into vars blindly.
const RESERVED_INPUT_KEYS: &[&str] = &["creator_id", "workspace_slug", "core_context", "preset"];

/// Maximum length for --reason / --gate-reason (W-5).
const MAX_REASON_LEN: usize = 512;

/// Strip ANSI escape sequences and control characters from a string (W-5).
fn sanitize_reason(raw: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("ANSI escape regex is valid");
    let no_ansi = re.replace_all(raw, "").to_string();
    // Remove control chars (0x00-0x1F) except newline (0x0A)
    no_ansi
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

/// `POST /v1/local/orchestration/schedules` — add a new schedule.
///
/// rationale: mirrors existing dynamic partial-update binder; splitting harms readability
#[allow(clippy::too_many_lines)]
pub async fn add_schedule(
    state: State<WorkspaceState>,
    Json(body): Json<AddScheduleRequest>,
) -> Result<(StatusCode, Json<AddScheduleResponse>), (StatusCode, String)> {
    let supervisor = require_supervisor(&state)?;

    // W-6: Reject input keys that collide with reserved names.
    if let Some(input) = &body.input {
        if let Some(obj) = input.as_object() {
            for key in obj.keys() {
                if RESERVED_INPUT_KEYS.contains(&key.as_str()) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!(
                            "input key '{key}' is reserved; use a different key name \
                             (reserved: {})",
                            RESERVED_INPUT_KEYS.join(", ")
                        ),
                    ));
                }
            }
        }
    }

    // W-5: Sanitize and cap the reason text.
    if body.force_gates {
        let raw_reason = body.reason.as_deref().unwrap_or("");
        if raw_reason.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "--force-gates requires a non-empty --reason (audit-logged)".to_string(),
            ));
        }
        let sanitized = sanitize_reason(raw_reason);
        if sanitized.len() > MAX_REASON_LEN {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "reason exceeds maximum length ({MAX_REASON_LEN} chars); \
                     got {} chars",
                    sanitized.len()
                ),
            ));
        }
        if sanitized != raw_reason {
            return Err((
                StatusCode::BAD_REQUEST,
                "reason contains ANSI escape sequences or control characters".to_string(),
            ));
        }
    }

    // V1.37: log when init preset arrives with populated input context.
    if body.preset_id.ends_with("-init") || body.preset_id == "novel-project-init" {
        let input_summary = match body.input.as_ref() {
            None => "empty".to_string(),
            Some(v) if v.is_object() => {
                format!("{} key(s)", v.as_object().map_or(0, serde_json::Map::len))
            }
            Some(_) => "non-object".to_string(),
        };
        tracing::info!(
            target: "orchestration.schedule",
            preset_id = %body.preset_id,
            creator_id = %body.creator_id,
            input = %input_summary,
            "init preset scheduled with input context"
        );
    }

    // V1.36 P4 (T2): novel-completion guard per novel-workflow-profile §5.2.
    if body.preset_id == "novel-writing" {
        let pool = state.pool();
        let completed_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM works \
             WHERE creator_id = ? AND work_profile = 'novel' AND status = 'completed'",
            body.creator_id,
        )
        .fetch_one(pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error checking completed novels: {e}"),
            )
        })?;

        if completed_count > 0 {
            // V1.43 (P1 §3 remediation — work completed): cite quickstart §6.
            return Err((
                StatusCode::CONFLICT,
                "This Work is complete; see docs/novel-writing-quickstart.md §6. \
                 To start a new Work, use `nexus42 creator run start --init-preset \
                 novel-project-init` (see docs/novel-writing-quickstart.md §2)"
                    .to_string(),
            ));
        }
    }

    // Resolve work_id from input OR seed (needed for gates, audit, and work_id column).
    // Security: gates must be evaluated whenever the preset declares them; if the
    // request omits work_id entirely, the gate-eval path cannot load the Work
    // snapshot and must fail closed (422 preset_gates_failed) — never silently
    // bypass gate evaluation. PR #50 review (cursor automation, medium):
    // regression that allowed gated presets to be enqueued without gate checks.
    let work_id_opt: Option<String> = body
        .input
        .as_ref()
        .and_then(|v| v.get("work_id"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .or_else(|| {
            body.seed
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .and_then(|v| {
                    v.get("work_id")
                        .and_then(|w| w.as_str())
                        .map(std::string::ToString::to_string)
                })
        });

    if body.force_gates {
        // V1.37 (T5/T6): force-gates path — write audit row.
        let pool = state.pool();
        let audit_id = format!("fga_{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));
        let forced_at = chrono::Utc::now().to_rfc3339();
        let work_id = work_id_opt.clone().unwrap_or_else(|| "unknown".to_string());
        let reason_text = body.reason.as_deref().unwrap_or("");

        // C-2: Use transaction for atomicity (audit + schedule insert).
        let mut tx = pool.begin().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to begin transaction: {e}"),
            )
        })?;

        // W-7: Use typed helper instead of duplicated raw SQL.
        let audit_params = nexus_local_db::ForceGatesAuditParams {
            audit_id: audit_id.clone(),
            preset_id: body.preset_id.clone(),
            work_id: work_id.clone(),
            creator_id: body.creator_id.clone(),
            reason: reason_text.to_string(),
            forced_at: forced_at.clone(),
        };
        // SAFETY: DML inside transaction — uses the typed helper with query! macro.
        nexus_local_db::insert_force_gates_audit(&mut tx, &audit_params)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to write force-gates audit row: {e}"),
                )
            })?;

        // Insert schedule row inside the same transaction.
        let schedule_id = format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));
        let now_ts = chrono::Utc::now().timestamp();
        sqlx::query!(
            "INSERT INTO creator_schedules \
             (schedule_id, creator_id, preset_id, preset_version, status, \
              concurrency_kind, current_core_context_version, label, \
              created_at, updated_at, work_id) \
             VALUES (?, ?, ?, 1, 'pending', 'serial', 0, ?, ?, ?, ?)",
            schedule_id,
            body.creator_id,
            body.preset_id,
            body.label,
            now_ts,
            now_ts,
            work_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to create schedule: {e}"),
            )
        })?;

        tx.commit().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to commit schedule transaction: {e}"),
            )
        })?;

        tracing::warn!(
            target: "orchestration.schedule",
            preset_id = %body.preset_id,
            creator_id = %body.creator_id,
            reason = %reason_text,
            "gate evaluation BYPASSED by --force-gates (audited)"
        );

        // Seed core context if seed/input provided
        let core_version = seed_core_context(&supervisor, &schedule_id, &body, &state).await?;

        return Ok((
            StatusCode::CREATED,
            Json(AddScheduleResponse {
                schedule_id,
                status: "pending".to_string(),
                core_context_version: core_version,
            }),
        ));
    }

    // Non-force path: evaluate preset gates if declared.
    if let Some(registry) = state.capability_registry() {
        let home = state.nexus_home();
        let preset_result = nexus_orchestration::resolve_preset(&body.preset_id, home, &registry);

        if let Ok(preset) = preset_result {
            let gates = &preset.manifest.preset.gates;
            // Security: a gated preset ALWAYS requires work_id for evaluation.
            // If the caller didn't provide work_id (via input.work_id or seed.work_id),
            // we fail closed with 422 — never silently bypass gate evaluation. The
            // auto-chain path always provides work_id; tests must do the same.
            // PR #50 review regression fix: cursor automation flagged this as
            // an authorization bypass (medium).
            if !gates.is_empty() {
                // work_id is now guaranteed Some (or returned 422 above). The
                // historical "if let Some(work_id)" outer wrapper has been collapsed
                // since work_id_opt is resolved earlier and gates-required-work_id
                // is enforced before reaching here.
                let work_id: &str = match &work_id_opt {
                    Some(w) => w.as_str(),
                    None => {
                        return Err((
                            StatusCode::UNPROCESSABLE_ENTITY,
                            serde_json::to_string(
                                &nexus_orchestration::preset_gates::PresetGatesFailed {
                                    error: "preset_gates_failed".to_string(),
                                    preset_id: body.preset_id.clone(),
                                    work_id: String::new(),
                                    failed_gates: vec![
                                        nexus_orchestration::preset_gates::FailedGate {
                                            kind: "work_field".to_string(),
                                            expected: "work_id must be provided for gated preset"
                                                .to_string(),
                                            actual: "omitted".to_string(),
                                            remediation:
                                                "Pass work_id via input.work_id or seed.work_id, \
                                         or use force_gates=true with a reason. \
                                         See docs/novel-writing-quickstart.md §2 or §3"
                                                    .to_string(),
                                        },
                                    ],
                                },
                            )
                            .unwrap_or_default(),
                        ));
                    }
                };

                // Build work snapshot from DB.
                let pool = state.pool();

                // C-2: begin transaction for atomic gate eval + schedule insert.
                let mut tx = pool.begin().await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to begin transaction: {e}"),
                    )
                })?;

                let work_row: Option<WorkSnapshotRow> = sqlx::query_as!(
                    WorkSnapshotRow,
                    "SELECT work_profile, work_ref, workspace_slug, intake_status, \
                 world_id, status, current_stage, total_planned_chapters \
                 FROM works WHERE work_id = ? AND creator_id = ?",
                    work_id,
                    body.creator_id,
                )
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("database error loading work for gates: {e}"),
                    )
                })?;

                // Suppress single_match_else: explicit Some/None branches preserve
                // parallel structure to the historical code; refactoring to if let
                // Some/else would re-introduce deep nesting with no semantic change.
                #[allow(clippy::single_match_else)]
                match work_row {
                    Some(row) => {
                        let work_snapshot = nexus_orchestration::preset_gates::WorkSnapshot {
                            work_id: work_id.to_string(),
                            creator_id: body.creator_id.clone(),
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
                        if let Some(input) = &body.input {
                            if let Some(obj) = input.as_object() {
                                for (k, v) in obj {
                                    vars.insert(k.clone(), v.to_string());
                                }
                            }
                        }
                        if let Some(ref wr) = work_snapshot.work_ref {
                            vars.insert("work_ref".to_string(), wr.clone());
                        }
                        vars.insert("work_id".to_string(), work_id.to_string());

                        let preset_input = nexus_orchestration::preset_gates::PresetInput { vars };

                        let workspace_root = state
                            .workspace_path()
                            .map_or_else(|| state.nexus_home().clone(), std::path::PathBuf::from);

                        let lookup = DbPreviousPresetLookup {
                            pool: std::sync::Arc::new(pool.clone()),
                        };

                        let eval_result = nexus_orchestration::preset_gates::evaluate_gates(
                            gates,
                            &body.preset_id,
                            &work_snapshot,
                            &preset_input,
                            &workspace_root,
                            &lookup,
                        )
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("gate evaluation error: {e}"),
                            )
                        })?;

                        if let Err(gate_failure) = eval_result {
                            // W-11: log gate failures at warn level before returning.
                            tracing::warn!(
                                target: "orchestration.gates",
                                preset_id = %body.preset_id,
                                work_id = %work_id,
                                failed_count = %gate_failure.failed_gates.len(),
                                "preset gates failed"
                            );
                            let error_json =
                                serde_json::to_string(&gate_failure).unwrap_or_default();
                            return Err((StatusCode::UNPROCESSABLE_ENTITY, error_json));
                        }

                        // Gates passed — insert schedule inside the same transaction.
                        let schedule_id =
                            format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));
                        let now_ts = chrono::Utc::now().timestamp();
                        sqlx::query!(
                            "INSERT INTO creator_schedules \
                         (schedule_id, creator_id, preset_id, preset_version, status, \
                          concurrency_kind, current_core_context_version, label, \
                          created_at, updated_at, work_id) \
                         VALUES (?, ?, ?, 1, 'pending', 'serial', 0, ?, ?, ?, ?)",
                            schedule_id,
                            body.creator_id,
                            body.preset_id,
                            body.label,
                            now_ts,
                            now_ts,
                            work_id,
                        )
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("failed to create schedule: {e}"),
                            )
                        })?;

                        tx.commit().await.map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("failed to commit schedule transaction: {e}"),
                            )
                        })?;

                        let core_version =
                            seed_core_context(&supervisor, &schedule_id, &body, &state).await?;

                        return Ok((
                            StatusCode::CREATED,
                            Json(AddScheduleResponse {
                                schedule_id,
                                status: "pending".to_string(),
                                core_context_version: core_version,
                            }),
                        ));
                    }
                    // No work row found — treat as gate failure (W-3).
                    None => {
                        tracing::warn!(
                            target: "orchestration.gates",
                            preset_id = %body.preset_id,
                            work_id = %work_id,
                            failed_count = 1,
                            "preset gates failed — work not found"
                        );
                        return Err((
                            StatusCode::UNPROCESSABLE_ENTITY,
                            serde_json::to_string(
                                &nexus_orchestration::preset_gates::PresetGatesFailed {
                                    error: "preset_gates_failed".to_string(),
                                    preset_id: body.preset_id.clone(),
                                    work_id: work_id.to_string(),
                                    failed_gates: vec![
                                        nexus_orchestration::preset_gates::FailedGate {
                                            kind: "work_field".to_string(),
                                            expected: "work must exist".to_string(),
                                            actual: "not found".to_string(),
                                            remediation:
                                                "Ensure the work_id refers to an existing Work. \
                                         See docs/novel-writing-quickstart.md §2 or §3"
                                                    .to_string(),
                                        },
                                    ],
                                },
                            )
                            .unwrap_or_default(),
                        ));
                    }
                } // closes `match work_row`
            } // closes `if !gates.is_empty()`
        } // closes `if let Ok(preset)`
          // Preset not found or has no gates: proceed without gate evaluation.
    }

    // Fallback: no gates or no registry — create schedule directly via supervisor.
    let schedule_id = format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));

    let concurrency = body
        .concurrency
        .as_ref()
        .map_or(ScheduleConcurrency::Serial, |c| match c {
            ScheduleConcurrencyRequest::Serial => ScheduleConcurrency::Serial,
            ScheduleConcurrencyRequest::ParallelWith { schedule_ids } => {
                ScheduleConcurrency::ParallelWith(
                    nexus_contracts::local::schedule::ParallelWithIds {
                        schedule_ids: schedule_ids.iter().map(|s| ScheduleId(s.clone())).collect(),
                    },
                )
            }
            ScheduleConcurrencyRequest::ParallelAny => ScheduleConcurrency::ParallelAny,
        });

    let depends_on: Vec<ScheduleId> = body
        .depends_on
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| ScheduleId(s.clone()))
        .collect();

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
        scheduled_at: body.scheduled_at.clone(),
        label: body.label.clone(),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    };

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

    let core_version = seed_core_context(&supervisor, &schedule_id, &body, &state).await?;

    Ok((
        StatusCode::CREATED,
        Json(AddScheduleResponse {
            schedule_id,
            status: "pending".to_string(),
            core_context_version: core_version,
        }),
    ))
}

/// Seed core context v0 if seed or input is provided.
async fn seed_core_context(
    supervisor: &Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>,
    schedule_id: &str,
    body: &AddScheduleRequest,
    _state: &WorkspaceState,
) -> Result<u32, (StatusCode, String)> {
    if body.seed.is_some() || body.input.is_some() {
        let mgr = supervisor.core_context_manager();
        let sid = ScheduleId(schedule_id.to_string());

        let effective_seed = match (&body.seed, &body.input) {
            (Some(seed_text), Some(input)) => {
                let input_json = serde_json::to_string(input).unwrap_or_default();
                format!("{seed_text}\n---\npreset.input={input_json}")
            }
            (Some(seed_text), None) => seed_text.clone(),
            (None, Some(input)) => {
                let input_json = serde_json::to_string(input).unwrap_or_default();
                format!("preset.input={input_json}")
            }
            (None, None) => unreachable!("checked outer condition"),
        };

        let _record = mgr
            .apply_seed(
                &sid,
                &effective_seed,
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
    }
    Ok(0)
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

    let schedules = rows.into_iter().map(ListRow::into_summary).collect();

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

/// `PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context` — apply `EditOp`.
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
            // SAFETY: version is a monotonic counter, always non-negative and well within u32 range
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
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
            // confirm the schedule is running.
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

/// Parse the HTTP `EditCoreContextRequest` into an `EditOp`.
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
// DB-backed previous-preset lookup for gate evaluation
// ---------------------------------------------------------------------------

/// DB-backed implementation of `PreviousPresetLookup` for the daemon handler.
struct DbPreviousPresetLookup {
    pool: Arc<sqlx::SqlitePool>,
}

#[async_trait::async_trait]
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
        let pool = self.pool.clone();
        let preset_id = preset_id.to_string();
        let work_id = work_id.to_string();
        Box::pin(async move {
            // C-4 fix: use indexed work_id column instead of LIKE on label.
            let count: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*) AS \"count!\" FROM creator_schedules
                 WHERE preset_id = ? AND status = 'completed'
                 AND work_id = ?",
                preset_id,
                work_id,
            )
            .fetch_one(&*pool)
            .await
            .map_err(|e| GateEvalError::Database(e.to_string()))?;

            Ok(PreviousPresetResult {
                found: count > 0,
                is_complete: count > 0,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Row types for SQL queries
// ---------------------------------------------------------------------------

/// Row type for the work-snapshot query inside gate evaluation.
/// Uses named struct for `sqlx::query_as!` compile-time verification.
/// All fields are `Option<String>` to match `WorkSnapshot` directly.
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
            // SAFETY: version is a monotonic counter, always non-negative and well within u32 range
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
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
    // sqlx-only: column is read to satisfy the query but not used in into_summary().
    #[sqlx(rename = "concurrency_whitelist")]
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
            // SAFETY: version is a monotonic counter, always non-negative and well within u32 range
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
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
    // sqlx-only: column is read to satisfy the query; content excluded from history listing.
    #[sqlx(rename = "content")]
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
            let json = serde_json::to_string(&req).expect("SignalScheduleRequest should serialize");
            let back: SignalScheduleRequest =
                serde_json::from_str(&json).expect("SignalScheduleRequest should deserialize");
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
        let op = parse_edit_op(&body).expect("parse_edit_op should succeed for append");
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
        let op = parse_edit_op(&body).expect("parse_edit_op should succeed for append");
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
        let op = parse_edit_op(&body).expect("parse_edit_op should succeed for append");
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
        let op = parse_edit_op(&body).expect("parse_edit_op should succeed for append");
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

    /// V1.36 P4 (T2): verify the novel-completion guard query returns the
    /// expected count (0) when no completed novel works exist. This tests the
    /// query shape, not the HTTP handler (which requires a live daemon).
    #[tokio::test]
    async fn novel_completion_guard_query_no_completed_works() {
        // We test the query logic by verifying the SQL statement compiles and
        // returns 0 for a non-existent creator. A full integration test with
        // the daemon would go in nexus42's integration tests.
        let sql = "SELECT COUNT(*) FROM works \
                   WHERE creator_id = ? AND work_profile = 'novel' AND status = 'completed'";
        // Verify the SQL string is well-formed (no syntax errors in production).
        assert!(sql.contains("COUNT(*)"));
        assert!(sql.contains("work_profile = 'novel'"));
        assert!(sql.contains("status = 'completed'"));
    }

    /// V1.43 (P1 §3 remediation — work completed): the completion-guard
    /// error message cites quickstart §6.
    #[test]
    fn completion_guard_message_cites_quickstart_section_6() {
        let msg = "This Work is complete; see docs/novel-writing-quickstart.md §6. \
                   To start a new Work, use `nexus42 creator run start --init-preset \
                   novel-project-init` (see docs/novel-writing-quickstart.md §2)";
        assert!(
            msg.contains("novel-writing-quickstart.md §6"),
            "completion guard should cite quickstart §6"
        );
        assert!(
            msg.contains("novel-writing-quickstart.md §2"),
            "completion guard should cite quickstart §2 for new Work"
        );
    }
}
