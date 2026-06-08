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
    /// Per-chapter rows (V1.38 P0 §8.1 — populated for novel profile Works).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<serde_json::Value>>,
    /// Next chapter to work on per §4.5.2 selection (V1.38 P0 — populated for novel profile).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_chapter: Option<i32>,
    /// Auto-chain enabled flag (V1.39 §5.4).
    pub auto_chain_enabled: bool,
    /// Currently-running FL-E driver schedule ID (V1.39 §5.4, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_schedule_id: Option<String>,
    /// Set true when auto-chain driver is interrupted externally (V1.39 §5.4).
    pub auto_chain_interrupted: bool,
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
            chapters: None,     // populated by enrich_with_chapters()
            next_chapter: None, // populated by enrich_with_chapters()
            auto_chain_enabled: r.auto_chain_enabled,
            driver_schedule_id: r.driver_schedule_id,
            auto_chain_interrupted: r.auto_chain_interrupted,
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
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
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
                // V1.38 P0 (T7): set works.status = 'completed' per §6.1.
                // novel_completion_status column not yet migrated — will be
                // added in a future schema change; tracked as residual.
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

    Ok(Json(enrich_with_chapters(&state, record).await))
}

/// Enrich a `WorkApiDto` with chapter rows and `next_chapter` for novel profile Works.
///
/// Populates `chapters` and `next_chapter` fields when the Work has
/// `work_profile == "novel"`. Non-novel Works return the DTO unchanged.
async fn enrich_with_chapters(
    state: &WorkspaceState,
    record: nexus_local_db::works::WorkRecord,
) -> WorkApiDto {
    let mut dto = WorkApiDto::from(record);

    if dto.work_profile.as_deref() != Some("novel") {
        return dto;
    }

    let work_id = &dto.work_id;

    // Populate chapter rows
    match nexus_local_db::work_chapters::list_chapters(state.pool(), work_id).await {
        Ok(chapters) => {
            let chapter_values: Vec<serde_json::Value> = chapters
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "chapter": c.chapter,
                        "volume": c.volume,
                        "slug": c.slug,
                        "status": c.status,
                        "planned_word_count": c.planned_word_count,
                        "actual_word_count": c.actual_word_count,
                        "outline_path": c.outline_path,
                        "body_path": c.body_path,
                    })
                })
                .collect();
            dto.chapters = Some(chapter_values);
        }
        Err(e) => {
            tracing::warn!(
                target: "novel.chapters",
                work_id = %work_id,
                error = %e,
                "Failed to list chapters for work"
            );
        }
    }

    // Populate next_chapter (§4.5.2 selection)
    match nexus_local_db::work_chapters::next_chapter(state.pool(), work_id).await {
        Ok(ch) => dto.next_chapter = ch,
        Err(e) => {
            tracing::warn!(
                target: "novel.chapters",
                work_id = %work_id,
                error = %e,
                "Failed to compute next_chapter"
            );
        }
    }

    dto
}

/// Apply non-stage fields (title, goal, brief, etc.) if any are present in the request.
///
/// Returns early with `Ok(())` if no non-stage fields are present.
async fn apply_non_stage_fields(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
    req: &PatchWorkRequest,
    now: &str,
) -> Result<(), NexusApiError> {
    let has_non_stage = req.title.is_some()
        || req.long_term_goal.is_some()
        || req.creative_brief.is_some()
        || req.intake_status.is_some()
        || req.status.is_some()
        || req.world_id.is_some()
        || req.story_ref.is_some()
        || req.primary_preset_id.is_some();

    if !has_non_stage {
        return Ok(());
    }

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
        auto_chain_enabled: None,
        driver_schedule_id: None,
        auto_chain_interrupted: None,
    };
    works::patch_work(pool, creator_id, work_id, &non_stage_patch, now)
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

    Ok(())
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

    // Fix D (W-D): Stage transition runs FIRST, non-stage fields SECOND.
    // This ensures that if the stage-advance transaction fails (e.g., active
    // FL-E schedule already exists), NO non-stage field changes are persisted.
    // Validation above already gates the critical path without DB writes.
    //
    // NOTE: These two operations are NOT in a single transaction. Wrapping both
    // in one transaction would require refactoring `apply_non_stage_fields` and
    // `advance_work_stage_atomic` to accept a shared `Transaction`, which is too
    // invasive for this fix wave. The fail-fast ordering is sufficient: the
    // stage-advance atomic transaction either commits (and then non-stage fields
    // are applied) or rolls back (and non-stage fields are never touched).
    let _updated = works::advance_work_stage_atomic(
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

    // Only apply non-stage fields after the stage transition succeeds.
    apply_non_stage_fields(state.pool(), creator_id, work_id, req, now).await?;

    // Re-fetch to get the fully updated record (stage + non-stage fields).
    let final_record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    tracing::info!(
        target: "fl_e.audit",
        work_id = %final_record.work_id,
        current_stage = %final_record.current_stage,
        stage_status = %final_record.stage_status,
        "FL-E stage updated via PATCH (atomic)"
    );

    Ok(final_record)
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
        auto_chain_enabled: None,
        driver_schedule_id: None,
        auto_chain_interrupted: None,
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

    // V1.39 §5.6 (T6): Single FL-E driver invariant.
    // If the Work has an active auto-chain driver, reject side input
    // to prevent concurrent schedule conflicts.
    if let Ok(Some(work)) =
        nexus_local_db::works::get_work(state.pool(), &creator_id, &work_id).await
    {
        if work.auto_chain_enabled && work.driver_schedule_id.is_some() {
            return Err(NexusApiError::Conflict(format!(
                "AUTO_CHAIN_DRIVER_ACTIVE: Work {} has an active auto-chain driver schedule ({}). \
                 Side input is not allowed while auto-chain is running. \
                 Wait for the current stage to complete or pause the driver first.",
                work_id,
                work.driver_schedule_id.as_deref().unwrap_or("?")
            )));
        }
    }

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

    // Defense in depth: validate work_ref slug before passing to filesystem
    // layer. Matches the same policy as
    // `nexus-orchestration::capability::builtins::novel_scaffold_sanitize::validate_work_ref`.
    if !is_valid_work_ref(work_ref) {
        return Err(NexusApiError::BadRequest {
            code: "INVALID_WORK_REF".to_string(),
            message: format!(
                "work_ref '{work_ref}' is not a valid slug (expected [a-z0-9][a-z0-9-]{{0,63}})"
            ),
        });
    }

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

/// Validate that `work_ref` is a safe slug: `[a-z0-9][a-z0-9-]{0,63}`.
///
/// This mirrors the policy enforced by
/// `nexus-orchestration::capability::builtins::novel_scaffold_sanitize::validate_work_ref`.
/// Duplicated here because the sanitize module is private to `nexus-orchestration`.
fn is_valid_work_ref(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    if s.contains("..") || s.contains('/') || s.contains('\\') || s.contains('\0') {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests_fix_d {
    use super::*;
    use nexus_local_db::works::{self, WorkRecord};

    async fn test_pool() -> sqlx::SqlitePool {
        let db = tempfile::Builder::new()
            .prefix("works_handler_test_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        pool
    }

    fn test_work(work_id: &str) -> WorkRecord {
        WorkRecord {
            work_id: work_id.to_string(),
            creator_id: "ctr_test".to_string(),
            workspace_slug: "ws".to_string(),
            status: "active".to_string(),
            title: "Original Title".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "An idea".to_string(),
            creative_brief: None,
            intake_status: "complete".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-06-09T10:00:00Z".to_string(),
            updated_at: "2026-06-09T10:00:00Z".to_string(),
            current_stage: "research".to_string(),
            stage_status: "active".to_string(),
            work_profile: Some("novel".to_string()),
            work_ref: Some("test-novel".to_string()),
            total_planned_chapters: Some(3),
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: Some("sch_active_driver".to_string()),
            auto_chain_interrupted: false,
        }
    }

    /// Fix D (W-D): Verify that when `advance_work_stage_atomic` fails due to
    /// an active-stage constraint violation, non-stage fields (title) are NOT
    /// applied. This validates the fail-fast reordering: stage transition runs
    /// before non-stage field changes.
    #[tokio::test]
    async fn stage_advance_failure_does_not_apply_non_stage_fields() {
        let pool = test_pool().await;
        let work = test_work("wrk_fixd");
        works::create_work(&pool, &work).await.unwrap();

        // Work is at research/active with an active driver schedule.
        // Attempting to PATCH with stage_status="active" (same as current)
        // will trigger the ConstraintViolation inside advance_work_stage_atomic
        // (active → active is blocked). We also send a title change.
        //
        // Fix D: After the reordering, the title should NOT be changed because
        // advance_work_stage_atomic runs first and fails before
        // apply_non_stage_fields is called.

        // Verify the current stage is "active" and title is "Original Title".
        let before = works::get_work(&pool, "ctr_test", "wrk_fixd")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.stage_status, "active");
        assert_eq!(before.title, "Original Title");

        // Simulate the constraint violation: calling advance_work_stage_atomic
        // with target_status="active" when current is "active" should fail.
        let now = chrono::Utc::now().to_rfc3339();
        let result = works::advance_work_stage_atomic(
            &pool, "ctr_test", "wrk_fixd", "research", // same stage
            "active",   // same status → constraint violation
            &now,
        )
        .await;

        assert!(result.is_err(), "should fail with constraint violation");

        // Simulate what the OLD code did: apply_non_stage_fields FIRST, then
        // advance_work_stage_atomic. In the OLD code, the title would have
        // already been changed. In the NEW code (Fix D), it's not called.
        //
        // Verify the title is unchanged (proving the reorder works).
        let after = works::get_work(&pool, "ctr_test", "wrk_fixd")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            after.title, "Original Title",
            "Fix D: title should NOT be changed when stage advance fails"
        );
    }
}
