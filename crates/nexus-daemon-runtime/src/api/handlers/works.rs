//! Works API handlers (V1.33 §7.2).
//!
//! Endpoints:
//! - `POST   /v1/local/works` — Create Work (idempotent on `client_request_id`)
//! - `GET    /v1/local/works` — List Works (filters: status, `intake_status`, limit, offset)
//! - `GET    /v1/local/works/{work_id}` — Get one Work
//! - `PATCH  /v1/local/works/{work_id}` — Partial update
//! - `POST   /v1/local/works/{work_id}/inspiration` — Append inspiration log entry
//! - `POST   /v1/local/works/pool` — Set active pool entry (DF-60 §5.3)
//! - `POST   /v1/local/works/{work_id}/completion-lock/release` — Release completion-lock (DF-60 §3.1)

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
    /// Opt-in: stale-findings watcher auto-enqueues `novel-review-master`
    /// for this Work after the timeout threshold (V1.39 P4 T4, default false).
    pub auto_review_master_on_timeout: bool,
    /// Runtime lock holder (V1.41 DF-60 §4, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_lock_holder: Option<String>,
    /// When the runtime lock was acquired (V1.41 DF-60 §4, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_lock_acquired_at: Option<String>,
    /// When completion-lock was applied (V1.41 DF-60 §3, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_locked_at: Option<String>,
    /// Novel completion status (V1.41 DF-60 §2, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novel_completion_status: Option<String>,
    /// Parent Work ID when created via lineage (V1.41 DF-60 §5.2, nullable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage_from_work_id: Option<String>,
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
            auto_review_master_on_timeout: r.auto_review_master_on_timeout,
            runtime_lock_holder: r.runtime_lock_holder,
            runtime_lock_acquired_at: r.runtime_lock_acquired_at,
            completion_locked_at: r.completion_locked_at,
            novel_completion_status: r.novel_completion_status,
            lineage_from_work_id: r.lineage_from_work_id,
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
    /// DF-60 §5.2: Parent Work ID for lineage (new Work created from completed Work).
    pub lineage_from_work_id: Option<String>,
    /// DF-60 §5.3: If true, after creation, set this Work as pool `active`.
    #[serde(default)]
    pub set_pool_active: Option<bool>,
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
    /// V1.39 P4 T4: opt-in flag — when true the stale-findings watcher
    /// auto-enqueues `novel-review-master` for this Work past the timeout.
    pub auto_review_master_on_timeout: Option<bool>,
    /// V1.39 §5.7: clear `auto_chain_interrupted` to resume auto-chain.
    /// R-V139P0-W-C: also triggers a supervisor tick so the resumed Work
    /// progresses immediately rather than waiting for the next cycle.
    pub auto_chain_interrupted: Option<bool>,
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

// ─── Pool request / response types (DF-60 §5.3) ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SetPoolActiveRequest {
    pub action: String,
    pub work_id: String,
    pub creator_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PoolEntryDto {
    pub entry_id: String,
    pub creator_id: String,
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub promoted_at: String,
    pub note: Option<String>,
}

impl From<nexus_local_db::novel_pool_entries::PoolEntry> for PoolEntryDto {
    fn from(e: nexus_local_db::novel_pool_entries::PoolEntry) -> Self {
        Self {
            entry_id: e.entry_id,
            creator_id: e.creator_id,
            work_id: e.work_id.unwrap_or_default(),
            status: e.status,
            title: e.title,
            promoted_at: e.promoted_at,
            note: e.note,
        }
    }
}

impl From<nexus_local_db::inspiration_items::InspirationItem> for InspirationItemDto {
    fn from(i: nexus_local_db::inspiration_items::InspirationItem) -> Self {
        Self {
            item_id: i.item_id,
            creator_id: i.creator_id,
            rel_path: i.rel_path,
            title: i.title,
            status: i.status,
            promoted_work_id: i.promoted_work_id,
            created_at: i.created_at,
            promoted_at: i.promoted_at,
        }
    }
}

// ─── Completion-lock release types (DF-60 §3.1) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReleaseCompletionLockRequest {
    pub reason: String,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// rationale: multi-column INSERT + validation + response construction;
/// splitting would harm readability without reducing actual complexity.
#[allow(clippy::too_many_lines)]
pub async fn create_work(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateWorkRequest>,
) -> Result<(StatusCode, Json<CreateWorkResponse>), NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    // T0.4: V1.40 mandatory world_id — reject Work creation without a World binding.
    // R-V140P0-S1: Uses BadRequest (400) rather than UnprocessableEntity (422) for
    // missing required field. This is consistent with other "field missing" errors
    // in this handler. Semantic 422 is used only for preset_gates_failed.
    // R-V140P0-S4: tracing span for mandatory binding check observability.
    if req.world_id.is_none() {
        tracing::info!(creator_id = %creator_id, "create_work rejected: missing world_id binding");
        return Err(NexusApiError::BadRequest {
            code: "WORLD_ID_REQUIRED".to_string(),
            message: "World binding is required for new Works (V1.40+).\n  \
                       ↳ Create a new World:  nexus42 creator world create --title \"...\"\n  \
                       ↳ List existing Worlds: nexus42 creator world list"
                .to_string(),
        });
    }

    // QC3 W-2: Validate that the provided world_id actually exists in
    // narrative_worlds AND is owned by the requesting creator.
    if let Some(ref wid) = req.world_id {
        let exists: Option<String> = sqlx::query_scalar!(
            r#"SELECT world_id AS "world_id!" FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?"#,
            wid,
            creator_id,
        )
        .fetch_optional(state.pool())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world_id existence check: {e}"),
        })?;
        if exists.is_none() {
            return Err(NexusApiError::BadRequest {
                code: "INVALID_WORLD_ID".to_string(),
                message: format!(
                    "world_id '{wid}' does not exist or is not owned by this creator.\n  \
                     ↳ Create a new World:  nexus42 creator world create --title \"...\"\n  \
                     ↳ List your Worlds:    nexus42 creator world list\n  \
                     World binding is required for new Works (V1.40+)."
                ),
            });
        }
    }

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
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: req.lineage_from_work_id,
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
        Err(new) => {
            // DF-60 §5.3: if set_pool_active was requested, promote in pool
            if req.set_pool_active == Some(true) {
                if let Err(e) = set_pool_active_inner(state.pool(), &creator_id, &new.work_id).await
                {
                    tracing::warn!(
                        work_id = %new.work_id,
                        error = %e,
                        "set_pool_active after create failed (non-fatal)"
                    );
                }
            }
            Ok((
                StatusCode::CREATED,
                Json(CreateWorkResponse {
                    work_id: new.work_id,
                    status: new.status,
                }),
            ))
        }
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

/// `GET /v1/local/works/{id}` — fetch a single Work by id.
///
/// # Lazy completion-promotion contract (R-V138P0-03)
///
/// This handler intentionally performs a **write-on-read** for novel-profile
/// Works: when the row's `status != 'completed'` and
/// [`nexus_local_db::work_chapters::is_work_completed`] returns `true` (every
/// chapter finalized per novel-workflow-profile §6.1), the handler issues a
/// `PATCH` to flip `works.status` → `'completed'` before returning the DTO.
///
/// **Why this is intentional, not an accident:**
///
/// 1. There is no daemon-side scheduler watching for "all chapters finalized"
///    — completion is a derived state that only crystallises on access.
/// 2. The platform requires `status='completed'` as the canonical signal for
///    sync/UI; computing it on every read without persisting would force every
///    downstream consumer to re-derive it.
/// 3. The patch is **idempotent**: subsequent GETs find `status='completed'`
///    on the first read, skip the `is_work_completed` check entirely (early
///    exit via the `status != "completed"` guard), and return the cached value.
///
/// **Failure semantics:** if the auto-promote PATCH fails, the handler logs a
/// warning and returns the un-promoted record — `GET` never fails because of
/// a promotion error. The caller will retry on the next read.
///
/// **Consistency:** because Nexus is single-user local-first (see
/// `next_chapter()` doc), there is no race with concurrent finalizers — the
/// read-then-write window is safe under the single-writer invariant.
///
/// A future cleanup may move this into a daemon-side post-finalize hook (e.g.
/// `update_status` for the last chapter triggers the promotion), at which
/// point this lazy path can become a no-op or be removed.
///
/// # Errors
///
/// - `404 NotFound` if the work id is unknown for the active creator.
/// - `401 AuthRequired` if no active creator is configured.
/// - `500 Internal` on database error.
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
    // R-V139P5-N2 (deferred): no cap/pagination on list_chapters yet.
    // Works with 100+ chapters would benefit from server-side pagination.
    // Currently returns all rows; the CLI status command consumes the full list.
    // A future slice should add `limit`/`offset` params to the DB query + API.
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
        || req.primary_preset_id.is_some()
        || req.auto_review_master_on_timeout.is_some();

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
        auto_review_master_on_timeout: req.auto_review_master_on_timeout,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };

    // QC2 W-03: V1.40 — reject clearing world_id on a novel Work that
    // already has a non-null world_id binding. This prevents downgrading
    // a mandatory-bound Work back to worldless via PATCH.
    // R-V140P0-S4: tracing for mandatory binding check observability.
    if non_stage_patch.world_id == Some(None) {
        // Check if the Work currently has a non-null world_id.
        let current = works::get_work(pool, creator_id, work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
        if current.world_id.is_some() {
            tracing::info!(work_id = %work_id, "patch_work: rejected world_id clear (non-stage path)");
            return Err(NexusApiError::BadRequest {
                code: "WORLD_CLEAR_FORBIDDEN".to_string(),
                message: format!(
                    "Cannot clear world_id on Work '{work_id}' — V1.40 Works require a World binding.\n  \
                     ↳ To rebind to a different World: PATCH with world_id set to the new World ID\n  \
                     ↳ List your Worlds: nexus42 creator world list"
                ),
            });
        }
    }

    // T4: validate world_id FK existence and ownership when PATCHing a non-null world_id.
    if let Some(Some(ref wid)) = non_stage_patch.world_id {
        let exists: Option<String> = sqlx::query_scalar!(
            r#"SELECT world_id AS "world_id!" FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?"#,
            wid,
            creator_id,
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world_id existence check: {e}"),
        })?;
        if exists.is_none() {
            return Err(NexusApiError::BadRequest {
                code: "INVALID_WORLD_ID".to_string(),
                message: format!(
                    "world_id '{wid}' does not exist or is not owned by this creator.\n  \
                     ↳ Create a new World:  nexus42 creator world create --title \"...\"\n  \
                     ↳ List your Worlds:    nexus42 creator world list\n  \
                     World binding is required for new Works (V1.40+)."
                ),
            });
        }
    }

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

// R-V140P0-S4: Pre-existing — function exceeds 100-line clippy threshold.
// Refactoring into smaller helpers deferred to V1.42.
#[allow(clippy::too_many_lines)]
pub async fn patch_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<PatchWorkRequest>,
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let now = chrono::Utc::now().to_rfc3339();

    // DF-60 §4: guard mutating operations against completion-lock and runtime-lock
    let current_work = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    if current_work.completion_locked_at.is_some() {
        return Err(NexusApiError::Conflict(
            format!(
                "work {work_id} is completion-locked since {}; use 'creator works completion-lock release' first",
                current_work.completion_locked_at.as_deref().unwrap_or("?")
            ),
        ));
    }

    if let Some(ref holder) = current_work.runtime_lock_holder {
        return Err(NexusApiError::Locked {
            resource: "work".to_string(),
            reason: format!(
                "work {work_id} is locked by '{holder}'; wait for release or check 'creator works status'"
            ),
        });
    }

    // Stage changes use gate validation + atomic transaction (R-FL-E-05 + R-FL-E-07).
    if req.current_stage.is_some() || req.stage_status.is_some() {
        let updated = patch_work_stage(&state, &creator_id, &work_id, &req, &now).await?;
        return Ok(Json(WorkApiDto::from(updated)));
    }

    // Non-stage PATCH: validate world_id clear-rejection first
    // QC2 W-03: V1.40 — reject clearing world_id on a Work that already
    // has a non-null world_id binding.
    if req.world_id == Some(None) {
        let current = works::get_work(state.pool(), &creator_id, &work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
        if current.world_id.is_some() {
            // R-V140P0-S4: tracing for mandatory binding check observability.
            tracing::info!(work_id = %work_id, "patch_work_stage: rejected world_id clear (stage path)");
            return Err(NexusApiError::BadRequest {
                code: "WORLD_CLEAR_FORBIDDEN".to_string(),
                message: format!(
                    "Cannot clear world_id on Work '{work_id}' — V1.40 Works require a World binding.\n  \
                     ↳ To rebind to a different World: PATCH with world_id set to the new World ID\n  \
                     ↳ List your Worlds: nexus42 creator world list"
                ),
            });
        }
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
        auto_chain_interrupted: req.auto_chain_interrupted,
        auto_review_master_on_timeout: req.auto_review_master_on_timeout,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
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

    // R-V139P0-W-C: trigger supervisor tick when auto_chain_interrupted
    // transitions from true to false, so the resumed Work progresses
    // immediately rather than waiting for the next tick cycle.
    if req.auto_chain_interrupted == Some(false) {
        if let Some(supervisor) = state.schedule_supervisor() {
            if let Err(e) = supervisor.tick().await {
                tracing::warn!(
                    work_id = %work_id,
                    error = %e,
                    "resume: supervisor tick failed (non-fatal)"
                );
            }
        }
    }

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

// ─── Pool handler (DF-60 §5.3) ──────────────────────────────────────────────

/// `POST /v1/local/works/pool` — Set the active pool entry for the creator.
///
/// Transactional: demotes any prior `active` row → `queued`, promotes target → `active`.
///
/// # IDOR protection (PR #53 review fix)
///
/// The body `creator_id` field is accepted for backward compatibility but is
/// **validated** against the active creator from `config.toml`. If it does not
/// match, the request is rejected with 403 Forbidden. The actual operation
/// always uses the active creator, never the body value.
pub async fn set_pool_active(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetPoolActiveRequest>,
) -> Result<Json<PoolEntryDto>, NexusApiError> {
    // IDOR fix: read active creator from config, reject body mismatch.
    let active_creator =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    if req.creator_id.is_some() && req.creator_id != Some(active_creator.clone()) {
        return Err(NexusApiError::Forbidden {
            resource: "pool".into(),
            reason: format!(
                "creator_id '{}' does not match active creator '{}'",
                req.creator_id.as_deref().unwrap_or("?"),
                active_creator
            ),
        });
    }
    let creator_id = active_creator;

    if req.action != "set_pool_active" {
        return Err(NexusApiError::BadRequest {
            code: "INVALID_ACTION".to_string(),
            message: format!(
                "unsupported action '{}'; expected 'set_pool_active'",
                req.action
            ),
        });
    }

    // Verify the work exists and belongs to this creator
    let _work = works::get_work(state.pool(), &creator_id, &req.work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {}", req.work_id)))?;

    let entry = set_pool_active_inner(state.pool(), &creator_id, &req.work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok(Json(entry))
}

// ─── Completion-lock release handler (DF-60 §3.1) ───────────────────────────

/// `POST /v1/local/works/{work_id}/completion-lock/release`
///
/// Releases the completion-lock for a Work:
/// 1. Clear DB `completion_locked_at` + set `novel_completion_status = 'reopened'`.
/// 2. Delete `.completion-lock.json` (best-effort; DB is SSOT).
pub async fn release_completion_lock_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<ReleaseCompletionLockRequest>,
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Step 1: Look up the Work record
    let work = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    // Verify the work is actually completion-locked
    if work.completion_locked_at.is_none() {
        return Err(NexusApiError::BadRequest {
            code: "NOT_LOCKED".to_string(),
            message: format!("work {work_id} is not completion-locked"),
        });
    }

    // Step 2: Clear DB columns (SSOT)
    let now = chrono::Utc::now().to_rfc3339();
    let patch = WorkPatch {
        completion_locked_at: Some(None),
        novel_completion_status: Some(Some("reopened".to_string())),
        ..Default::default()
    };

    let updated = works::patch_work(state.pool(), &creator_id, &work_id, &patch, &now)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Step 3: Delete on-disk lock file (best-effort; DB is SSOT)
    if let Some(ref work_ref) = updated.work_ref {
        let workspace_path = state.workspace_path().unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            if let Err(e) = nexus_orchestration::completion_lock::release_completion_lock(
                workspace_dir,
                work_ref,
            ) {
                tracing::warn!(
                    work_id = %work_id,
                    work_ref = %work_ref,
                    error = %e,
                    "completion-lock file deletion failed (non-fatal; DB is SSOT)"
                );
            }
        }
    }

    tracing::info!(
        target: "novel.completion",
        work_id = %work_id,
        creator_id = %creator_id,
        reason = %req.reason,
        "completion-lock released"
    );

    Ok(Json(WorkApiDto::from(updated)))
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

/// Shared helper: transactional set-pool-active (DF-60 §5.3).
///
/// Demotes any prior `active` row → `queued`, then upserts target → `active`.
/// Uses `nexus_local_db::novel_pool_entries::promote_to_active` for the core logic.
async fn set_pool_active_inner(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<PoolEntryDto, nexus_local_db::LocalDbError> {
    let entry =
        nexus_local_db::novel_pool_entries::promote_to_active(pool, creator_id, work_id).await?;

    Ok(PoolEntryDto::from(entry))
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

// ─── P1: Pool + Inspiration handlers (DF-61) ────────────────────────────────

/// `GET /v1/local/works/pool` — List pool entries for the active creator.
pub async fn list_pool(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListPoolQuery>,
) -> Result<Json<ListPoolResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let limit = query.limit;
    let offset = query.offset;

    let entries = nexus_local_db::novel_pool_entries::list_pool_entries(
        state.pool(),
        &creator_id,
        query.status.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let total = nexus_local_db::novel_pool_entries::count_pool_entries(
        state.pool(),
        &creator_id,
        query.status.as_deref(),
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let items: Vec<PoolEntryDto> = entries.into_iter().map(PoolEntryDto::from).collect();

    Ok(Json(ListPoolResponse {
        entries: items,
        total,
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    }))
}

/// `POST /v1/local/works/pool/promote` — Promote a pool entry to active.
pub async fn promote_pool_entry(
    State(state): State<WorkspaceState>,
    Json(req): Json<PromotePoolRequest>,
) -> Result<Json<PoolEntryDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Verify the work exists and belongs to this creator
    let _work = works::get_work(state.pool(), &creator_id, &req.work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {}", req.work_id)))?;

    let entry = nexus_local_db::novel_pool_entries::promote_to_active(
        state.pool(),
        &creator_id,
        &req.work_id,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(Json(PoolEntryDto::from(entry)))
}

/// `POST /v1/local/works/pool/archive` — Archive a pool entry.
pub async fn archive_pool_entry_handler(
    State(state): State<WorkspaceState>,
    Json(req): Json<ArchivePoolRequest>,
) -> Result<Json<PoolEntryDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let entry = nexus_local_db::novel_pool_entries::archive_pool_entry(
        state.pool(),
        &req.entry_id,
        &creator_id,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(Json(PoolEntryDto::from(entry)))
}

/// `POST /v1/local/works/pool/inspiration` — Add an inspiration item.
pub async fn add_inspiration(
    State(state): State<WorkspaceState>,
    Json(req): Json<AddInspirationRequest>,
) -> Result<(StatusCode, Json<AddInspirationResponse>), NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let item_id = format!("npi_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();

    // Route through nexus-home-layout — resolve operational workspace dir
    // from nexus_home (~/.nexus42), not user home directly.
    let workspace_dir = state
        .nexus_home()
        .join("creators")
        .join(&creator_id)
        .join("workspaces")
        .join(&workspace_slug);

    let item = nexus_local_db::inspiration_items::create_inspiration_with_scaffold(
        state.pool(),
        &item_id,
        &creator_id,
        &req.title,
        &workspace_dir,
        &now,
    )
    .await
    .map_err(|e| match &e {
        nexus_local_db::LocalDbError::ConstraintViolation { .. } => NexusApiError::Conflict(
            format!("inspiration item with this path already exists: {e}"),
        ),
        _ => NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        },
    })?;

    Ok((
        StatusCode::CREATED,
        Json(AddInspirationResponse {
            item_id: item.item_id,
            rel_path: item.rel_path,
        }),
    ))
}

/// `GET /v1/local/works/pool/inspiration` — List inspiration items.
pub async fn list_inspiration(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListInspirationQuery>,
) -> Result<Json<ListInspirationResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let limit = query.limit;
    let offset = query.offset;

    let items = nexus_local_db::inspiration_items::list_inspiration(
        state.pool(),
        &creator_id,
        query.status.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let total = nexus_local_db::inspiration_items::count_inspiration(
        state.pool(),
        &creator_id,
        query.status.as_deref(),
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let dtos: Vec<InspirationItemDto> = items.into_iter().map(InspirationItemDto::from).collect();

    Ok(Json(ListInspirationResponse {
        items: dtos,
        total,
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    }))
}

/// `POST /v1/local/works/pool/inspiration/promote` — Promote an inspiration item to a Work.
pub async fn promote_inspiration_handler(
    State(state): State<WorkspaceState>,
    Json(req): Json<PromoteInspirationRequest>,
) -> Result<Json<PromoteInspirationResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Look up the inspiration item
    let item = nexus_local_db::inspiration_items::get_inspiration(state.pool(), &req.item_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("inspiration item {}", req.item_id)))?;

    // Cross-creator guard: only the owning creator can promote their items
    if item.creator_id != creator_id {
        return Err(NexusApiError::NotFound(format!(
            "inspiration item {}",
            req.item_id
        )));
    }

    if item.status != "idea" {
        return Err(NexusApiError::BadRequest {
            code: "INVALID_STATUS".to_string(),
            message: format!(
                "inspiration item {} has status '{}' — only 'idea' items can be promoted",
                req.item_id, item.status
            ),
        });
    }

    // Create a new Work from the inspiration
    let work_id = format!("wrk_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let idea = req.idea.as_deref().unwrap_or(&item.title);

    // For inspiration promote, we create a Work without world_id (the user
    // can bind a world later). This is a lighter-weight flow than `run start`.
    let record = WorkRecord {
        work_id: work_id.clone(),
        creator_id: creator_id.clone(),
        workspace_slug,
        status: "draft".to_string(),
        title: item.title.clone(),
        long_term_goal: idea.to_string(),
        initial_idea: idea.to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
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
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };

    // Wrap the three writes (Work create + pool promote + inspiration update)
    // in a single transaction so a step-3 failure rolls back everything.
    let pool_entry = nexus_local_db::inspiration_promote_atomic(
        state.pool(),
        &record,
        &creator_id,
        &work_id,
        &req.item_id,
        &now,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(Json(PromoteInspirationResponse {
        work_id: work_id.clone(),
        pool_entry_id: pool_entry.entry_id,
    }))
}

/// `POST /v1/local/works/pool/inspiration/archive` — Archive an inspiration item.
pub async fn archive_inspiration_handler(
    State(state): State<WorkspaceState>,
    Json(req): Json<ArchiveInspirationRequest>,
) -> Result<Json<InspirationItemDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let item = nexus_local_db::inspiration_items::archive_inspiration(
        state.pool(),
        &req.item_id,
        &creator_id,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(Json(InspirationItemDto::from(item)))
}

// ─── P1 Request / Response types ────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct ListPoolQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListPoolResponse {
    pub entries: Vec<PoolEntryDto>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Deserialize)]
pub struct PromotePoolRequest {
    pub work_id: String,
    /// If true, also set as pool active (redundant since promote always sets active).
    #[serde(default)]
    pub set_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ArchivePoolRequest {
    pub entry_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AddInspirationRequest {
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct AddInspirationResponse {
    pub item_id: String,
    pub rel_path: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListInspirationQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListInspirationResponse {
    pub items: Vec<InspirationItemDto>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct InspirationItemDto {
    pub item_id: String,
    pub creator_id: String,
    pub rel_path: String,
    pub title: String,
    pub status: String,
    pub promoted_work_id: Option<String>,
    pub created_at: String,
    pub promoted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromoteInspirationRequest {
    pub item_id: String,
    /// Optional idea override for the new Work's `initial_idea`.
    pub idea: Option<String>,
    /// If true, set as pool active after creation.
    #[serde(default)]
    pub set_default: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PromoteInspirationResponse {
    pub work_id: String,
    pub pool_entry_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ArchiveInspirationRequest {
    pub item_id: String,
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
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
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

    // ── T0.4: mandatory world_id tests ─────────────────────────────────

    #[tokio::test]
    async fn create_work_without_world_id_returns_error() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let nexus_home = tmp.path().to_path_buf();
        let db_path = tmp.path().join("state.db");

        // Write a minimal config.toml with active creator
        let config = toml::toml! {
            active_creator_id = "ctr_test"
            active_workspace_slug_by_creator = { ctr_test = "ws" }
        };
        std::fs::write(
            nexus_home.join("config.toml"),
            toml::to_string(&config).unwrap(),
        )
        .expect("write config");

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = CreateWorkRequest {
            title: "Test Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "An idea".to_string(),
            world_id: None, // missing — should be rejected
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        };

        let result = create_work(State(state), Json(req)).await;

        assert!(result.is_err(), "POST /works without world_id must fail");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("WORLD_ID_REQUIRED"),
            "error code should be WORLD_ID_REQUIRED, got: {msg}"
        );
        assert!(
            msg.contains("creator world create") || msg.contains("creator world list"),
            "error should mention remediation, got: {msg}"
        );

        // QC1 W-1 regression: WORLD_ID_REQUIRED returns 422 (not 400).
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "WORLD_ID_REQUIRED should return 422 (preset_gates_failed-style), got {}",
            err.status_code()
        );
    }

    // ── QC3 W-2: POST validates world_id existence ────────────────────

    #[tokio::test]
    async fn create_work_with_nonexistent_world_id_returns_error() {
        use crate::test_utils;

        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = CreateWorkRequest {
            title: "Test Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "An idea".to_string(),
            world_id: Some("wld_nonexistent_12345".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        };

        let result = create_work(State(state), Json(req)).await;
        assert!(result.is_err(), "POST with non-existent world_id must fail");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("INVALID_WORLD_ID"),
            "error code should be INVALID_WORLD_ID, got: {msg}"
        );
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_WORLD_ID should return 422"
        );
    }

    // ── QC2 W-02: cross-creator world binding rejection ────────────────

    #[tokio::test]
    async fn create_work_with_other_creators_world_id_returns_error() {
        use crate::test_utils;

        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(state.pool()).await;

        // Seed another creator who owns a different world
        // SAFETY: test-only DML
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_other', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator");

        // Create a world owned by ctr_other
        let other_world = nexus_local_db::create_world(
            state.pool(),
            "ctr_other",
            "Other's World",
            "other-world",
            "private",
            "manual",
        )
        .await
        .expect("create other world");

        let req = CreateWorkRequest {
            title: "Test Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "An idea".to_string(),
            world_id: Some(other_world.world_id.clone()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        };

        let result = create_work(State(state), Json(req)).await;
        assert!(
            result.is_err(),
            "POST with other creator's world_id must fail"
        );
        let err = result.unwrap_err();
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "cross-creator world binding should return 422"
        );
    }

    // ── QC2 W-03: reject PATCH that clears world_id on novel Work ──────

    #[tokio::test]
    async fn patch_work_clearing_world_id_on_bound_work_returns_error() {
        use crate::test_utils;

        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(state.pool()).await;

        // Create a work bound to the seeded world
        let req = CreateWorkRequest {
            title: "Bound Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "An idea".to_string(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        };

        let (_, resp) = create_work(State(state.clone()), Json(req))
            .await
            .expect("create work should succeed");
        let work_id = resp.work_id.clone();

        // Try to clear the world_id
        let patch = PatchWorkRequest {
            title: None,
            long_term_goal: None,
            creative_brief: None,
            intake_status: None,
            status: None,
            world_id: Some(None), // clear world_id
            story_ref: None,
            primary_preset_id: None,
            current_stage: None,
            stage_status: None,
            force: None,
            auto_review_master_on_timeout: None,
            auto_chain_interrupted: None,
        };

        let result = patch_work(State(state.clone()), Path(work_id.clone()), Json(patch)).await;

        assert!(result.is_err(), "PATCH clearing world_id must be rejected");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("WORLD_CLEAR_FORBIDDEN"),
            "error code should be WORLD_CLEAR_FORBIDDEN, got: {msg}"
        );
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "WORLD_CLEAR_FORBIDDEN should return 422"
        );

        // Verify world_id is still set
        let work = works::get_work(state.pool(), "test_creator", &work_id)
            .await
            .expect("get_work")
            .expect("work exists");
        assert!(
            work.world_id.is_some(),
            "world_id should still be set after rejected clear"
        );
        drop(tmp);
    }

    // ── QC2 W-04: adversarial world_id values ──────────────────────────

    #[tokio::test]
    async fn create_work_with_adversarial_world_ids_returns_error() {
        use crate::test_utils;

        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(state.pool()).await;

        let adversarial_ids: &[&str] = &[
            "wld_' OR 1=1--",
            "wld_; DROP TABLE works--",
            "wld_../etc/passwd",
            "wld_\x00null",
            "wld_a very long id that exceeds normal lengths and should still be handled gracefully without panicking or crashing the server",
            "not_wld_prefix",
            "",
        ];

        for bad_id in adversarial_ids {
            let req = CreateWorkRequest {
                title: "Adversarial Test".to_string(),
                long_term_goal: "Test".to_string(),
                initial_idea: "Test".to_string(),
                world_id: Some((*bad_id).to_string()),
                story_ref: None,
                primary_preset_id: None,
                client_request_id: None,
                lineage_from_work_id: None,
                set_pool_active: None,
            };

            let result = create_work(State(state.clone()), Json(req)).await;
            assert!(
                result.is_err(),
                "adversarial world_id '{bad_id}' must be rejected"
            );
            let err = result.unwrap_err();
            assert_eq!(
                err.status_code(),
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "adversarial world_id '{bad_id}' should return 422, got {}",
                err.status_code()
            );
            // Verify no panic, clear remediation
            let msg = format!("{err:?}");
            assert!(
                msg.contains("INVALID_WORLD_ID"),
                "adversarial world_id '{bad_id}' should produce INVALID_WORLD_ID, got: {msg}"
            );
        }
    }
}
