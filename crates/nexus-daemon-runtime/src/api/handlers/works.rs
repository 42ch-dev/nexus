//! Thin Work HTTP adapters over the guarded core authoring service.
use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::{ListWorksQuery, ListWorksResponse};
use serde::{Deserialize, Serialize};

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
    /// V1.42: volume of the next chapter to work on (cross-volume aware).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_chapter_volume: Option<i32>,
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

impl From<nexus_core::WorkDetails> for WorkApiDto {
    fn from(r: nexus_core::WorkDetails) -> Self {
        Self {
            work_id: r.work_id,
            status: r.status,
            title: r.title,
            long_term_goal: r.long_term_goal,
            initial_idea: r.initial_idea,
            creative_brief: r.creative_brief,
            intake_status: r.intake_status,
            world_id: r.world_id,
            story_ref: r.story_ref,
            inspiration_log: r.inspiration_log,
            primary_preset_id: r.primary_preset_id,
            schedule_ids: r.schedule_ids,
            created_at: r.created_at,
            updated_at: r.updated_at,
            current_stage: r.current_stage,
            stage_status: r.stage_status,
            work_profile: r.work_profile,
            work_ref: r.work_ref,
            total_planned_chapters: r.total_planned_chapters,
            current_chapter: r.current_chapter,
            chapters: r.chapters,
            next_chapter: r.next_chapter,
            next_chapter_volume: r.next_chapter_volume,
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
    ///
    /// If provided, the referenced Work must exist and belong to the active
    /// creator. A nonexistent or cross-creator reference is rejected with 400.
    /// An empty string is also rejected (use `None` / omit the field instead).
    pub lineage_from_work_id: Option<String>,
    /// DF-60 §5.3: If true, after creation, set this Work as pool `active`.
    #[serde(default)]
    pub set_pool_active: Option<bool>,
    /// V1.65: explicit work profile classification (additive optional).
    pub work_profile: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkResponse {
    pub work_id: String,
    pub status: String,
}
fn deserialize_nullable<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}


#[derive(Debug, Deserialize)]
pub struct PatchWorkRequest {
    pub title: Option<String>,
    pub long_term_goal: Option<String>,
    pub creative_brief: Option<String>,
    pub intake_status: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub world_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
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
    /// V1.65: explicit work profile classification (additive optional).
    pub work_profile: Option<String>,
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
    /// V1.42 P-last (R-V141P1-11): hidden from API — local-first, always the active creator.
    #[serde(skip_serializing)]
    pub creator_id: String,
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub promoted_at: String,
    pub note: Option<String>,
}

impl From<nexus_core::WorkPoolEntry> for PoolEntryDto {
    fn from(e: nexus_core::WorkPoolEntry) -> Self {
        Self {
            entry_id: e.entry_id,
            creator_id: e.creator_id,
            work_id: e.work_id,
            status: e.status,
            title: e.title,
            promoted_at: e.promoted_at,
            note: e.note,
        }
    }
}

impl From<nexus_core::WorkInspirationItem> for InspirationItemDto {
    fn from(i: nexus_core::WorkInspirationItem) -> Self {
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

/// Query parameters for `reconcile-chapters` (V1.49 P2, R-V148P4-W2).
///
/// `dry_run=true` selects the preview path (no writes, no lock).
#[derive(Debug, Default, serde::Deserialize)]
pub struct ReconcileDryRunQuery {
    /// When `true`, compute the `ReconcileReport` without writing.
    #[serde(default)]
    pub dry_run: Option<bool>,
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
    /// V1.42 P-last (R-V141P1-11): hidden from API — local-first, always the active creator.
    #[serde(skip_serializing)]
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

pub(crate) fn work_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => NexusApiError::BadRequest { code: field, message: reason },
        nexus_core::CoreError::Forbidden { resource } if resource.starts_with("work_conflict:") => NexusApiError::Conflict(resource[14..].to_owned()),
        nexus_core::CoreError::Forbidden { resource } if resource.starts_with("work_locked:") => NexusApiError::Locked { resource: "work".into(), reason: resource[12..].to_owned() },
        nexus_core::CoreError::Forbidden { resource } if resource.starts_with("work_pool_forbidden:") => NexusApiError::Forbidden { resource: "pool".into(), reason: resource[20..].to_owned() },
        // Core works carries the legacy internal classification verbatim as
        // `<CODE>: <message>` (DATABASE_ERROR, CONTRACT_ERROR — the codes the
        // pre-extraction handlers emitted). Re-emit the code unchanged; every
        // other internal category keeps the shared CORE_ERROR shape.
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some((code, message)) if code == "DATABASE_ERROR" || code == "CONTRACT_ERROR" => {
                NexusApiError::Internal { code: code.to_owned(), message: message.to_owned() }
            }
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

pub async fn create_work(State(state): State<WorkspaceState>, Json(req): Json<CreateWorkRequest>) -> Result<(StatusCode, Json<CreateWorkResponse>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.create_work_with_outcome(&principal, nexus_contracts::CreateWorkRequest { title: req.title, long_term_goal: req.long_term_goal, initial_idea: req.initial_idea, world_id: req.world_id, story_ref: req.story_ref, primary_preset_id: req.primary_preset_id, client_request_id: req.client_request_id, lineage_from_work_id: req.lineage_from_work_id, set_pool_active: req.set_pool_active, work_profile: req.work_profile }).await.map_err(work_error)?;
    let (created, result) = result;
    Ok((if created { StatusCode::CREATED } else { StatusCode::OK }, Json(CreateWorkResponse { work_id: result.work_id, status: result.status })))
}

pub async fn list_works(State(state): State<WorkspaceState>, Query(query): Query<ListWorksQuery>) -> Result<Json<ListWorksResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.list_works(&principal, query).await.map_err(work_error)?;
    Ok(Json(result))
}

pub async fn get_work(State(state): State<WorkspaceState>, Path(work_id): Path<String>) -> Result<Json<WorkApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.get_work(&principal, work_id).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

pub async fn patch_work(State(state): State<WorkspaceState>, Path(work_id): Path<String>, Json(req): Json<PatchWorkRequest>) -> Result<Json<WorkApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let resume_auto_chain = req.auto_chain_interrupted == Some(false)
        && req.current_stage.is_none() && req.stage_status.is_none();
    let result = core.patch_work(&principal, work_id, "http", nexus_core::WorkPatchRequest { title: req.title, long_term_goal: req.long_term_goal, creative_brief: req.creative_brief, intake_status: req.intake_status, status: req.status, world_id: req.world_id, story_ref: req.story_ref, primary_preset_id: req.primary_preset_id, current_stage: req.current_stage, stage_status: req.stage_status, force: req.force, auto_review_master_on_timeout: req.auto_review_master_on_timeout, auto_chain_interrupted: req.auto_chain_interrupted, work_profile: req.work_profile }).await.map_err(work_error)?;
    // Legacy daemon composition only; scheduling remains outside the core Work service.
    if resume_auto_chain {
        if let Some(supervisor) = state.schedule_supervisor() {
            if let Err(error) = supervisor.tick().await {
                tracing::warn!(work_id = %result.work_id, %error, "resume: supervisor tick failed (non-fatal)");
            }
        }
    }
    Ok(Json(result.into()))
}

pub async fn append_inspiration(State(state): State<WorkspaceState>, Path(work_id): Path<String>, Json(req): Json<AppendInspirationRequest>) -> Result<Json<AppendInspirationResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.append_work_inspiration(&principal, work_id, "http", nexus_contracts::AppendInspirationRequest { note: req.note }).await.map_err(work_error)?;
    Ok(Json(AppendInspirationResponse { work_id: result.work_id, inspiration_count: usize::try_from(result.inspiration_count).unwrap_or(usize::MAX) }))
}

pub async fn set_pool_active(State(state): State<WorkspaceState>, Json(req): Json<SetPoolActiveRequest>) -> Result<Json<PoolEntryDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.set_work_pool_active(&principal, nexus_core::SetPoolActiveRequest { action: req.action, work_id: req.work_id, creator_id: req.creator_id }).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

pub async fn release_completion_lock_handler(State(state): State<WorkspaceState>, Path(work_id): Path<String>, Json(req): Json<ReleaseCompletionLockRequest>) -> Result<Json<WorkApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.release_work_completion_lock(&principal, work_id, nexus_contracts::ReleaseCompletionLockRequest { reason: req.reason }).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

pub async fn delete_work(State(state): State<WorkspaceState>, Path(work_id): Path<String>) -> Result<StatusCode, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    core.delete_work(&principal, work_id, "http").await.map_err(work_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reconcile_chapters(State(state): State<WorkspaceState>, Path(work_id): Path<String>, Query(query): Query<ReconcileDryRunQuery>) -> Result<(StatusCode, Json<nexus_local_db::work_chapters::ReconcileReport>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.reconcile_work_chapters(&principal, work_id, "http", nexus_core::ReconcileDryRunQuery { dry_run: query.dry_run }).await.map_err(work_error)?;
    Ok((StatusCode::OK, Json(nexus_local_db::work_chapters::ReconcileReport {
        created: result.created, updated: result.updated, resynced: result.resynced, preserved: result.preserved,
    })))
}

pub async fn list_pool(State(state): State<WorkspaceState>, Query(query): Query<ListPoolQuery>) -> Result<Json<ListPoolResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.list_work_pool(&principal, nexus_core::ListPoolQuery { status: query.status, limit: query.limit, offset: query.offset }).await.map_err(work_error)?;
    Ok(Json(ListPoolResponse { entries: result.entries.into_iter().map(Into::into).collect(), total: result.total, limit: result.limit, offset: result.offset }))
}

pub async fn promote_pool_entry(State(state): State<WorkspaceState>, Json(req): Json<PromotePoolRequest>) -> Result<Json<PoolEntryDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.promote_work_pool_entry(&principal, nexus_core::PromotePoolRequest { work_id: req.work_id, set_default: req.set_default }).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

pub async fn archive_pool_entry_handler(State(state): State<WorkspaceState>, Json(req): Json<ArchivePoolRequest>) -> Result<Json<PoolEntryDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.archive_work_pool_entry(&principal, nexus_core::ArchivePoolRequest { entry_id: req.entry_id }).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

pub async fn add_inspiration(State(state): State<WorkspaceState>, Json(req): Json<AddInspirationRequest>) -> Result<(StatusCode, Json<AddInspirationResponse>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.add_work_inspiration(&principal, nexus_core::AddInspirationRequest { title: req.title }).await.map_err(work_error)?;
    Ok((StatusCode::CREATED, Json(AddInspirationResponse { item_id: result.item_id, rel_path: result.rel_path })))
}

pub async fn list_inspiration(State(state): State<WorkspaceState>, Query(query): Query<ListInspirationQuery>) -> Result<Json<ListInspirationResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.list_work_inspiration(&principal, nexus_core::ListInspirationQuery { status: query.status, limit: query.limit, offset: query.offset }).await.map_err(work_error)?;
    Ok(Json(ListInspirationResponse { items: result.items.into_iter().map(Into::into).collect(), total: result.total, limit: result.limit, offset: result.offset }))
}

pub async fn promote_inspiration_handler(State(state): State<WorkspaceState>, Json(req): Json<PromoteInspirationRequest>) -> Result<Json<PromoteInspirationResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.promote_work_inspiration(&principal, nexus_core::PromoteInspirationRequest { item_id: req.item_id, idea: req.idea, set_default: req.set_default }).await.map_err(work_error)?;
    Ok(Json(PromoteInspirationResponse { work_id: result.work_id, pool_entry_id: result.pool_entry_id }))
}

pub async fn archive_inspiration_handler(State(state): State<WorkspaceState>, Json(req): Json<ArchiveInspirationRequest>) -> Result<Json<InspirationItemDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core.archive_work_inspiration(&principal, nexus_core::ArchiveInspirationRequest { item_id: req.item_id }).await.map_err(work_error)?;
    Ok(Json(result.into()))
}

#[cfg(test)]
mod tests_fix_d {
    use super::*;
    use nexus_local_db::works;
    use crate::config::read_active_workspace_slug;
    #[tokio::test]
    async fn create_work_without_world_id_returns_error() {
        let (_tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;

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
            work_profile: None,
        };

        let result = create_work(State(state), Json(req)).await;

        assert!(result.is_err(), "POST /works without world_id must fail");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("world_id_required"),
            "error code should be world_id_required, got: {msg}"
        );

        // QC1 W-1 regression: WORLD_ID_REQUIRED returns 422 (not 400).
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "world_id_required should return 422 (preset_gates_failed-style), got {}",
            err.status_code()
        );
    }

    // ── QC3 W-2: POST validates world_id existence ────────────────────

    #[tokio::test]
    async fn create_work_with_nonexistent_world_id_returns_error() {
        use crate::test_utils;

        let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
        )
        .await;

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
            work_profile: None,
        };

        let result = create_work(State(state), Json(req)).await;
        assert!(result.is_err(), "POST with non-existent world_id must fail");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("invalid_world_id"),
            "error code should be invalid_world_id, got: {msg}"
        );
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_world_id should return 422"
        );
    }

    // ── QC2 W-02: cross-creator world binding rejection ────────────────

    #[tokio::test]
    async fn create_work_with_other_creators_world_id_returns_error() {
        use crate::test_utils;

        let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
        )
        .await;

        // Seed another creator who owns a different world
        // SAFETY: test-only DML
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_other', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
        )
        .await
        .expect("seed other creator");

        // Create a world owned by ctr_other
        let other_world = nexus_local_db::create_world(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
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
            work_profile: None,
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
        test_utils::seed_test_creator_and_world(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
        )
        .await;

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
            work_profile: None,
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
            work_profile: None,
        };

        let result = patch_work(State(state.clone()), Path(work_id.clone()), Json(patch)).await;

        assert!(result.is_err(), "PATCH clearing world_id must be rejected");
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("world_clear_forbidden"),
            "error code should be world_clear_forbidden, got: {msg}"
        );
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "world_clear_forbidden should return 422"
        );

        // Verify world_id is still set
        let work = works::get_work(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
            "test_creator",
            &work_id,
        )
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

        let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        test_utils::seed_test_creator_and_world(
            state
                .pool_or_uninit()
                .expect("test fixture ensures creator DB is open"),
        )
        .await;

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
                work_profile: None,
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
                msg.contains("invalid_world_id"),
                "adversarial world_id '{bad_id}' should produce invalid_world_id, got: {msg}"
            );
        }
    }

    #[test]
    fn read_active_workspace_slug_defaults_when_entry_missing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let nexus_home = tmp.path();
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"ctr_test\"\n[active_workspace_slug_by_creator]\n",
        )
        .expect("write config");

        let slug = read_active_workspace_slug(nexus_home, "ctr_test");
        assert_eq!(
            slug.as_deref(),
            Some("default"),
            "missing slug entry must fall back to default (profile-switch contract)"
        );
    }

    #[test]
    fn read_active_workspace_slug_preserves_explicit_entry() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let nexus_home = tmp.path();
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"ctr_test\"\n[active_workspace_slug_by_creator]\nctr_test = \"ws_custom\"\n",
        )
        .expect("write config");

        let slug = read_active_workspace_slug(nexus_home, "ctr_test");
        assert_eq!(slug.as_deref(), Some("ws_custom"));
    }
    #[test]
    fn work_error_retains_status_and_body() {
        let cases = [
            (
                nexus_core::CoreError::InvalidInput { field: "invalid_action".into(), reason: "unsupported action 'other'; expected 'set_pool_active'".into() },
                NexusApiError::BadRequest { code: "invalid_action".into(), message: "unsupported action 'other'; expected 'set_pool_active'".into() },
                StatusCode::BAD_REQUEST,
            ),
            (
                nexus_core::CoreError::Forbidden { resource: "work_conflict:work wrk_locked is completion-locked since 2026-09-15; use 'creator works completion-lock release' first".into() },
                NexusApiError::Conflict("work wrk_locked is completion-locked since 2026-09-15; use 'creator works completion-lock release' first".into()),
                StatusCode::CONFLICT,
            ),
            (
                nexus_core::CoreError::Forbidden { resource: "work_locked:work wrk_locked is locked by 'driver'; wait for release or check 'creator works status'".into() },
                NexusApiError::Locked { resource: "work".into(), reason: "work wrk_locked is locked by 'driver'; wait for release or check 'creator works status'".into() },
                StatusCode::LOCKED,
            ),
            (
                nexus_core::CoreError::NotFound { resource: "work wrk_missing".into() },
                NexusApiError::NotFound("work wrk_missing".into()),
                StatusCode::NOT_FOUND,
            ),
            (
                nexus_core::CoreError::Forbidden { resource: "work_pool_forbidden:work wrk_x is not in the authoring pool".into() },
                NexusApiError::Forbidden { resource: "pool".into(), reason: "work wrk_x is not in the authoring pool".into() },
                StatusCode::FORBIDDEN,
            ),
        ];
        for (core_error, old_error, status) in cases {
            let migrated = work_error(core_error);
            assert_eq!(migrated.status_code(), status);
            assert_eq!(
                serde_json::to_string(&migrated.to_response_body()).unwrap(),
                serde_json::to_string(&old_error.to_response_body()).unwrap(),
            );
        }
        // The legacy internal classifications (emitted directly by the pre-extraction
        // handlers) ride the `<CODE>: <message>` carrier and are re-emitted verbatim.
        // Body JSON alone cannot distinguish them — the body always reports
        // `error.code = "internal"` — so the carrier `code` field is asserted too.
        for (category, legacy_code, legacy_message) in [
            ("DATABASE_ERROR: INSERT failed: no such table: works", "DATABASE_ERROR", "INSERT failed: no such table: works"),
            ("CONTRACT_ERROR: invalid type: string, expected u32 at line 1 column 5", "CONTRACT_ERROR", "invalid type: string, expected u32 at line 1 column 5"),
        ] {
            let migrated = work_error(nexus_core::CoreError::Internal { category: category.into() });
            let NexusApiError::Internal { code, message } = &migrated else {
                panic!("internal category must stay Internal, got {:?}", migrated);
            };
            assert_eq!((code.as_str(), message.as_str()), (legacy_code, legacy_message));
            assert_eq!(migrated.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
            let old_error = NexusApiError::Internal { code: legacy_code.into(), message: legacy_message.into() };
            assert_eq!(
                serde_json::to_string(&migrated.to_response_body()).unwrap(),
                serde_json::to_string(&old_error.to_response_body()).unwrap(),
            );
        }
        // Non-legacy internal categories keep the shared CORE_ERROR fallback.
        let fallback = work_error(nexus_core::CoreError::Internal { category: "workspace metadata: boom".into() });
        let NexusApiError::Internal { code, .. } = &fallback else {
            panic!("fallback must stay Internal, got {:?}", fallback);
        };
        assert_eq!(code, "CORE_ERROR");
        assert_eq!(fallback.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn nullable_patch_and_ignored_reopen_fields_remain_distinct() {
        let omitted: PatchWorkRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        let cleared: PatchWorkRequest = serde_json::from_value(serde_json::json!({"story_ref": null, "world_id": null})).unwrap();
        let set: PatchWorkRequest = serde_json::from_value(serde_json::json!({"story_ref": "story", "world_id": "world"})).unwrap();
        assert_eq!((omitted.story_ref, omitted.world_id), (None, None));
        assert_eq!((cleared.story_ref, cleared.world_id), (Some(None), Some(None)));
        assert_eq!((set.story_ref, set.world_id), (Some(Some("story".into())), Some(Some("world".into()))));
        let ignored: PatchWorkRequest = serde_json::from_value(serde_json::json!({
            "novel_completion_status": "reopened", "completion_locked_at": null,
            "total_planned_chapters": 99,
        })).unwrap();
        assert!(ignored.status.is_none() && ignored.current_stage.is_none() && ignored.world_id.is_none() && ignored.story_ref.is_none());
    }
    #[tokio::test]
    async fn retained_reopen_is_ignored_and_completion_lock_remains_conflict() {
        let (_tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool_or_uninit().unwrap();
        crate::test_utils::seed_test_creator_and_world(pool).await;
        let request = serde_json::from_value(serde_json::json!({
            "title": "Locked work", "long_term_goal": "write", "initial_idea": "idea",
            "world_id": "wld_test_world",
        })).unwrap();
        let (_, Json(created)) = create_work(State(state.clone()), Json(request)).await.unwrap();
        let ignored_request = || serde_json::from_value(serde_json::json!({
            "novel_completion_status": "reopened", "completion_locked_at": null,
            "total_planned_chapters": 99,
        })).unwrap();
        let Json(unchanged) = patch_work(State(state.clone()), Path(created.work_id.clone()), Json(ignored_request())).await.unwrap();
        assert_eq!(unchanged.novel_completion_status, None);
        assert_eq!(unchanged.total_planned_chapters, None);
        sqlx::query("UPDATE works SET completion_locked_at = '2026-09-15', novel_completion_status = 'completed', total_planned_chapters = 3 WHERE work_id = ?").bind(&created.work_id).execute(pool).await.unwrap();
        let error = patch_work(State(state.clone()), Path(created.work_id.clone()), Json(ignored_request())).await.unwrap_err();
        let expected = NexusApiError::Conflict(format!("work {} is completion-locked since 2026-09-15; use 'creator works completion-lock release' first", created.work_id));
        assert_eq!(error.status_code(), StatusCode::CONFLICT);
        assert_eq!(serde_json::to_string(&error.to_response_body()).unwrap(), serde_json::to_string(&expected.to_response_body()).unwrap());
        let record = works::get_work(pool, "test_creator", &created.work_id).await.unwrap().unwrap();
        assert_eq!(record.completion_locked_at.as_deref(), Some("2026-09-15"));
        assert_eq!(record.total_planned_chapters, Some(3));
    }
    #[tokio::test]
    async fn internal_database_fault_reaches_handler_with_legacy_code() {
        let (_tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool_or_uninit().unwrap();
        crate::test_utils::seed_test_creator_and_world(pool).await;
        sqlx::query("DROP TABLE works").execute(pool).await.unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "title": "Broken", "long_term_goal": "write", "initial_idea": "idea",
            "world_id": "wld_test_world",
        })).unwrap();
        let error = create_work(State(state), Json(request)).await.unwrap_err();
        let NexusApiError::Internal { code, message } = &error else {
            panic!("dropped works table must surface as Internal, got {:?}", error);
        };
        assert_eq!(code, "DATABASE_ERROR");
        assert!(message.contains("works"), "message must carry the sqlite failure, got: {message}");
        assert_eq!(error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.to_response_body().error.code, "internal");
    }
}
