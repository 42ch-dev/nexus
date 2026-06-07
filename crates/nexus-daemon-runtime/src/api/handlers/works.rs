//! Works API handlers (V1.33 §7.2).
//!
//! Endpoints:
//! - `POST   /v1/local/works` — Create Work (idempotent on `client_request_id`)
//! - `GET    /v1/local/works` — List Works (filters: status, `intake_status`, limit, offset)
//! - `GET    /v1/local/works/{work_id}` — Get one Work
//! - `PATCH  /v1/local/works/{work_id}` — Partial update
//! - `POST   /v1/local/works/{work_id}/inspiration` — Append inspiration log entry

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_local_db::works::{self, WorkListFilters, WorkPatch, WorkRecord};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Request / Response types ──────────────────────────────────────────────

/// Stable API representation of a Work record (R-V133P1-10).
///
/// Decoupled from `WorkRecord` (DB row) to prevent leaking internal fields
/// like `creator_id` and `workspace_slug` to API consumers.
///
/// JSON columns (`creative_brief`, `inspiration_log`, `schedule_ids`) are
/// parsed from their stored text form into structured JSON types so that
/// API consumers receive native JSON values rather than escaped strings
/// (T1 contract §7.2).
#[derive(Debug, Serialize)]
pub struct WorkApiDto {
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    pub creative_brief: Option<serde_json::Value>,
    pub intake_status: String,
    pub world_id: Option<String>,
    pub story_ref: Option<String>,
    pub inspiration_log: Vec<serde_json::Value>,
    pub primary_preset_id: String,
    pub schedule_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Current FL-E stage (V1.34).
    pub current_stage: String,
    /// Current FL-E stage status (V1.34).
    pub stage_status: String,
    /// Work profile (V1.36 novel-workflow-profile §2.1).
    pub work_profile: Option<String>,
    /// Human slug for Works/ directory (V1.36 §2.1).
    pub work_ref: Option<String>,
    /// Total planned chapters (V1.36 §2.1).
    pub total_planned_chapters: Option<i32>,
    /// Current chapter index (V1.36 §2.1).
    pub current_chapter: i32,
}

impl From<WorkRecord> for WorkApiDto {
    fn from(r: WorkRecord) -> Self {
        // Parse JSON columns. Best-effort: if a column is malformed, fall back
        // to a sensible default rather than 500. The DB writes these via Rust
        // types so they should be valid JSON.
        let creative_brief = r
            .creative_brief
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let inspiration_log = serde_json::from_str(&r.inspiration_log).unwrap_or_default();
        let schedule_ids = serde_json::from_str(&r.schedule_ids).unwrap_or_default();

        Self {
            work_id: r.work_id,
            status: r.status,
            title: r.title,
            long_term_goal: r.long_term_goal,
            initial_idea: r.initial_idea,
            creative_brief,
            intake_status: r.intake_status,
            world_id: r.world_id,
            story_ref: r.story_ref,
            inspiration_log,
            primary_preset_id: r.primary_preset_id,
            schedule_ids,
            created_at: r.created_at,
            updated_at: r.updated_at,
            current_stage: r.current_stage,
            stage_status: r.stage_status,
            work_profile: r.work_profile,
            work_ref: r.work_ref,
            total_planned_chapters: r.total_planned_chapters,
            current_chapter: r.current_chapter,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkRequest {
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    pub world_id: Option<String>,
    pub story_ref: Option<String>,
    pub primary_preset_id: Option<String>,
    /// If provided and a Work with the same creator + `client_request_id` exists,
    /// return the existing `work_id` (idempotent).
    pub client_request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkResponse {
    pub work_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListWorksQuery {
    pub status: Option<String>,
    pub intake_status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListWorksResponse {
    pub works: Vec<WorkSummary>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct WorkSummary {
    pub work_id: String,
    pub title: String,
    pub status: String,
    pub intake_status: String,
    pub primary_preset_id: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchWorkRequest {
    pub title: Option<String>,
    pub long_term_goal: Option<String>,
    pub creative_brief: Option<String>,
    pub intake_status: Option<String>,
    pub status: Option<String>,
    pub world_id: Option<Option<String>>,
    pub story_ref: Option<Option<String>>,
    pub primary_preset_id: Option<String>,
    /// V1.34 FL-E: update the current stage.
    pub current_stage: Option<String>,
    /// V1.34 FL-E: update the stage status.
    pub stage_status: Option<String>,
    /// V1.34 FL-E: bypass stage-order gates (equivalent to CLI `--force`).
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AppendInspirationRequest {
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct AppendInspirationResponse {
    pub work_id: String,
    pub inspiration_count: usize,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

pub async fn create_work(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateWorkRequest>,
) -> Result<(StatusCode, Json<CreateWorkResponse>), NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work_id = format!("wrk_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let preset_id = req
        .primary_preset_id
        .clone()
        .unwrap_or_else(|| "novel-writing".to_string());

    let record = WorkRecord {
        work_id: work_id.clone(),
        creator_id: creator_id.clone(),
        workspace_slug,
        status: "active".to_string(),
        title: req.title,
        long_term_goal: req.long_term_goal,
        initial_idea: req.initial_idea,
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: req.world_id,
        story_ref: req.story_ref,
        inspiration_log: String::from("[]"),
        primary_preset_id: preset_id,
        schedule_ids: String::from("[]"),
        created_at: now.clone(),
        updated_at: now.clone(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
        work_profile: None,
        work_ref: None,
        total_planned_chapters: None,
        current_chapter: 0,
    };

    // R-V133P1-01: Atomic create + idempotency in single transaction
    let crid = req.client_request_id.as_deref();
    let result = works::create_work_atomic(state.pool(), &record, crid)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    match result {
        // Idempotent replay — existing Work found
        Ok(existing) => Ok((
            StatusCode::OK,
            Json(CreateWorkResponse {
                work_id: existing.work_id,
                status: existing.status,
            }),
        )),
        // New Work created (no client_request_id)
        Err(new) => Ok((
            StatusCode::CREATED,
            Json(CreateWorkResponse {
                work_id: new.work_id,
                status: new.status,
            }),
        )),
    }
}

pub async fn list_works(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListWorksQuery>,
) -> Result<Json<ListWorksResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let filters = WorkListFilters {
        status: query.status,
        intake_status: query.intake_status,
        limit: query.limit,
        offset: query.offset,
    };

    // R-V133P1-11 v2: list + count in a shared transaction so total and
    // records are consistent even under concurrent writes.
    // R-V133P1-11 v3: warn on failure for observability before mapping to error response.
    let (records, total) =
        works::list_and_count_works(state.pool(), &creator_id, &workspace_slug, &filters)
            .await
            .map_err(|e| {
                tracing::warn!(
                    error = %e,
                    "list_and_count_works failed for creator {creator_id} — \
                     pagination metadata unavailable"
                );
                NexusApiError::Internal {
                    code: "DATABASE_ERROR".to_string(),
                    message: e.to_string(),
                }
            })?;
    let total: usize = total as usize;

    let works_list: Vec<WorkSummary> = records
        .into_iter()
        .map(|r| WorkSummary {
            work_id: r.work_id,
            title: r.title,
            status: r.status,
            intake_status: r.intake_status,
            primary_preset_id: r.primary_preset_id,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(ListWorksResponse {
        works: works_list,
        total,
    }))
}

pub async fn get_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let mut record = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    // V1.36 P4 (T1): auto-promote works.status to 'completed' when all
    // chapters are finalized per novel-workflow-profile §6.1.
    if record.status != "completed" && record.work_profile.as_deref() == Some("novel") {
        match nexus_local_db::work_chapters::is_work_completed(state.pool(), &work_id).await {
            Ok(true) => {
                let now = chrono::Utc::now().to_rfc3339();
                let patch = WorkPatch {
                    status: Some("completed".to_string()),
                    ..Default::default()
                };
                match works::patch_work(state.pool(), &creator_id, &work_id, &patch, &now).await {
                    Ok(updated) => {
                        tracing::info!(
                            target: "novel.completion",
                            work_id = %work_id,
                            creator_id = %creator_id,
                            work_ref = ?updated.work_ref,
                            total_planned_chapters = ?updated.total_planned_chapters,
                            "Auto-promoted work to 'completed' (all chapters finalized)"
                        );
                        record = updated;
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "novel.completion",
                            work_id = %work_id,
                            error = %e,
                            "Failed to auto-promote work to 'completed'"
                        );
                    }
                }
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    target: "novel.completion",
                    work_id = %work_id,
                    error = %e,
                    "Failed to check work completion status"
                );
            }
        }
    }

    Ok(Json(WorkApiDto::from(record)))
}

/// Handle PATCH with stage changes: gate validation + atomic transaction (R-FL-E-05 + R-FL-E-07).
async fn patch_work_stage(
    state: &WorkspaceState,
    creator_id: &str,
    work_id: &str,
    req: &PatchWorkRequest,
    now: &str,
) -> Result<WorkRecord, NexusApiError> {
    let current = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    let target_stage = req
        .current_stage
        .as_deref()
        .unwrap_or(&current.current_stage);

    if req.current_stage.is_some() {
        let force = req.force.unwrap_or(false);
        let work_state = nexus_orchestration::stage_gates::WorkStageState {
            current_stage: current.current_stage.clone(),
            stage_status: current.stage_status.clone(),
            intake_status: current.intake_status.clone(),
        };
        nexus_orchestration::stage_gates::check_stage_advance(&work_state, target_stage, force)
            .map_err(|e| NexusApiError::BadRequest {
                code: "INVALID_STAGE".to_string(),
                message: e.message,
            })?;
    }

    let target_status = req.stage_status.as_deref().unwrap_or(&current.stage_status);

    // R-CURSOR-PR42-03: Validate stage_status transitions to terminal states
    // even when no explicit current_stage change is provided. Without this,
    // PATCH {"stage_status":"complete"} bypasses all FL-E gates.
    if req.stage_status.is_some() && req.current_stage.is_none() {
        let force = req.force.unwrap_or(false);
        check_stage_status_transition(&current.stage_status, target_status, force)?;
    }

    // Apply non-stage fields first if present
    let has_non_stage = req.title.is_some()
        || req.long_term_goal.is_some()
        || req.creative_brief.is_some()
        || req.intake_status.is_some()
        || req.status.is_some()
        || req.world_id.is_some()
        || req.story_ref.is_some()
        || req.primary_preset_id.is_some();

    if has_non_stage {
        let non_stage_patch = WorkPatch {
            title: req.title.clone(),
            long_term_goal: req.long_term_goal.clone(),
            creative_brief: req.creative_brief.clone().map(Some),
            intake_status: req.intake_status.clone(),
            status: req.status.clone(),
            world_id: req.world_id.clone(),
            story_ref: req.story_ref.clone(),
            primary_preset_id: req.primary_preset_id.clone(),
            schedule_ids: None,
            current_stage: None,
            stage_status: None,
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: None,
        };
        works::patch_work(state.pool(), creator_id, work_id, &non_stage_patch, now)
            .await
            .map_err(|e| match &e {
                nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                    NexusApiError::NotFound(format!("work {work_id}"))
                }
                _ => NexusApiError::Internal {
                    code: "DATABASE_ERROR".to_string(),
                    message: e.to_string(),
                },
            })?;
    }

    let updated = works::advance_work_stage_atomic(
        state.pool(),
        creator_id,
        work_id,
        target_stage,
        target_status,
        now,
    )
    .await
    .map_err(|e| match &e {
        nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
            NexusApiError::NotFound(format!("work {work_id}"))
        }
        nexus_local_db::LocalDbError::ConstraintViolation { constraint, .. } => {
            NexusApiError::Conflict(constraint.clone())
        }
        _ => NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        },
    })?;

    tracing::info!(
        target: "fl_e.audit",
        work_id = %work_id,
        current_stage = %updated.current_stage,
        stage_status = %updated.stage_status,
        "FL-E stage updated via PATCH (atomic)"
    );

    Ok(updated)
}

pub async fn patch_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<PatchWorkRequest>,
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Stage changes use gate validation + atomic transaction (R-FL-E-05 + R-FL-E-07).
    if req.current_stage.is_some() || req.stage_status.is_some() {
        let updated = patch_work_stage(&state, &creator_id, &work_id, &req, &now).await?;
        return Ok(Json(WorkApiDto::from(updated)));
    }

    // Non-stage PATCH: use regular patch
    let patch = WorkPatch {
        title: req.title,
        long_term_goal: req.long_term_goal,
        creative_brief: req.creative_brief.map(Some),
        intake_status: req.intake_status,
        status: req.status,
        world_id: req.world_id,
        story_ref: req.story_ref,
        primary_preset_id: req.primary_preset_id,
        schedule_ids: None,
        current_stage: None,
        stage_status: None,
        work_profile: None,
        work_ref: None,
        total_planned_chapters: None,
        current_chapter: None,
    };

    let updated = works::patch_work(state.pool(), &creator_id, &work_id, &patch, &now)
        .await
        .map_err(|e| match &e {
            nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                NexusApiError::NotFound(format!("work {work_id}"))
            }
            _ => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            },
        })?;

    Ok(Json(WorkApiDto::from(updated)))
}

pub async fn append_inspiration(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<AppendInspirationRequest>,
) -> Result<Json<AppendInspirationResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Build JSON for inspiration entry
    let entry = serde_json::json!({
        "at": now.clone(),
        "note": req.note,
    });
    let entry_json = serde_json::to_string(&entry).unwrap_or_default();

    // R-V133P1-04: append_inspiration now uses tx + Rust append and returns updated record
    let updated = works::append_inspiration(state.pool(), &creator_id, &work_id, &entry_json, &now)
        .await
        .map_err(|e| match &e {
            nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                NexusApiError::NotFound(format!("work {work_id}"))
            }
            _ => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            },
        })?;

    // Derive count from post-state (not pre-fetch + 1)
    let count = serde_json::from_str::<serde_json::Value>(&updated.inspiration_log)
        .ok()
        .and_then(|v| v.as_array().map(Vec::len))
        .unwrap_or(0);

    Ok(Json(AppendInspirationResponse {
        work_id,
        inspiration_count: count,
    }))
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Validate `stage_status` transitions to terminal states (R-CURSOR-PR42-03).
///
/// Rejects direct status promotion to `complete`/`skipped` without an explicit
/// stage change unless `force` is true. This prevents PATCH requests with only
/// `{"stage_status":"complete"}` from bypassing all FL-E gate validation.
///
/// Allowed transitions without force:
/// - `pending` → `active` (schedule starts the stage)
/// - `active` → `pending` (schedule reset)
///
/// Blocked transitions without force:
/// - `pending` → `complete` or `skipped` (must go through gate or force)
/// - `active` → `complete` or `skipped` (schedule completion or force)
fn check_stage_status_transition(
    current_status: &str,
    target_status: &str,
    force: bool,
) -> Result<(), NexusApiError> {
    // Terminal status values that require gate validation or explicit force.
    const TERMINAL_STATUSES: &[&str] = &["complete", "skipped"];

    if force {
        return Ok(());
    }

    if TERMINAL_STATUSES.contains(&target_status) && !TERMINAL_STATUSES.contains(&current_status) {
        return Err(NexusApiError::BadRequest {
            code: "INVALID_STATUS_TRANSITION".to_string(),
            message: format!(
                "Cannot set stage_status to '{target_status}' without an explicit stage advance. \
                 Use PATCH with current_stage to advance through FL-E gates, or set force=true to override."
            ),
        });
    }

    Ok(())
}

/// Read active `creator_id` from CLI config.
pub fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Read active workspace slug from CLI config.
pub fn read_active_workspace_slug(
    nexus_home: &std::path::Path,
    creator_id: &str,
) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_workspace_slug_by_creator")
        .and_then(|v| v.get(creator_id))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

// ---------------------------------------------------------------------------
// Reconcile chapters (V1.36 §4.1.2, §8)
// ---------------------------------------------------------------------------

/// Reconcile `work_chapters` from filesystem for a Work.
///
/// `POST /v1/local/works/{work_id}/reconcile-chapters`
pub async fn reconcile_chapters(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
) -> Result<
    (
        StatusCode,
        Json<nexus_local_db::work_chapters::ReconcileReport>,
    ),
    NexusApiError,
> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let pool = state.pool();

    // Get the Work record to find work_ref
    let work = works::get_work(pool, &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("get_work failed: {e}"),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("Work '{work_id}' not found")))?;

    let work_ref = work
        .story_ref
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "PRECONDITION_FAILED".to_string(),
            message: "`story_ref` (work_ref) not set on Work; run novel-project-init first"
                .to_string(),
        })?;

    // Resolve workspace root from state
    let workspace_path_str = state.workspace_path().unwrap_or_default();
    let workspace_root = std::path::Path::new(&workspace_path_str);

    let now = chrono::Utc::now().to_rfc3339();
    let report = nexus_local_db::work_chapters::reconcile_from_filesystem(
        pool,
        &work_id,
        work_ref,
        workspace_root,
        &now,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("reconcile failed: {e}"),
    })?;

    Ok((StatusCode::OK, Json(report)))
}
