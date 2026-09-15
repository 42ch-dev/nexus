//! Work lifecycle and authoring operations over the guarded core workspace.
//! HTTP envelopes stay in adapters; these projections are owned domain values.

use nexus_contracts::{CoreWorkSelection, CreateWorkRequest, CreateWorkResponse, AppendInspirationRequest, AppendInspirationResponse, ReleaseCompletionLockRequest, ListWorksQuery, ListWorksResponse, WorkSummary, PaginationInfo};
use nexus_local_db::works::{self, WorkListFilters, WorkPatch, WorkRecord};
use uuid::Uuid;
use crate::{CoreService, CoreAccess, CoreError, CoreResult, Principal};

#[derive(Debug, Clone)]
pub struct WorkDetails {
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
    pub chapters: Option<Vec<serde_json::Value>>,
    /// Next chapter to work on per §4.5.2 selection (V1.38 P0 — populated for novel profile).
    pub next_chapter: Option<i32>,
    /// V1.42: volume of the next chapter to work on (cross-volume aware).
    pub next_chapter_volume: Option<i32>,
    /// Auto-chain enabled flag (V1.39 §5.4).
    pub auto_chain_enabled: bool,
    /// Currently-running FL-E driver schedule ID (V1.39 §5.4, nullable).
    pub driver_schedule_id: Option<String>,
    /// Set true when auto-chain driver is interrupted externally (V1.39 §5.4).
    pub auto_chain_interrupted: bool,
    /// Opt-in: stale-findings watcher auto-enqueues `novel-review-master`
    /// for this Work after the timeout threshold (V1.39 P4 T4, default false).
    pub auto_review_master_on_timeout: bool,
    /// Runtime lock holder (V1.41 DF-60 §4, nullable).
    pub runtime_lock_holder: Option<String>,
    /// When the runtime lock was acquired (V1.41 DF-60 §4, nullable).
    pub runtime_lock_acquired_at: Option<String>,
    /// When completion-lock was applied (V1.41 DF-60 §3, nullable).
    pub completion_locked_at: Option<String>,
    /// Novel completion status (V1.41 DF-60 §2, nullable).
    pub novel_completion_status: Option<String>,
    /// Parent Work ID when created via lineage (V1.41 DF-60 §5.2, nullable).
    pub lineage_from_work_id: Option<String>,
}

impl From<WorkRecord> for WorkDetails {
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
            chapters: None,            // populated by enrich_with_chapters()
            next_chapter: None,        // populated by enrich_with_chapters()
            next_chapter_volume: None, // populated by enrich_with_chapters()
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
#[derive(Debug, Clone, Default)]
pub struct WorkPatchRequest {
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
#[derive(Debug, Clone)]
pub struct SetPoolActiveRequest {
    pub action: String,
    pub work_id: String,
    pub creator_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkPoolEntry {
    pub entry_id: String,
    /// V1.42 P-last (R-V141P1-11): hidden from API — local-first, always the active creator.
    pub creator_id: String,
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub promoted_at: String,
    pub note: Option<String>,
}

impl From<nexus_local_db::novel_pool_entries::PoolEntry> for WorkPoolEntry {
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

impl From<nexus_local_db::inspiration_items::InspirationItem> for WorkInspirationItem {
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
#[derive(Debug, Clone)]
pub struct ReconcileDryRunQuery {
    /// When `true`, compute the `ReconcileReport` without writing.
    pub dry_run: Option<bool>,
}
#[derive(Debug, Clone)]
pub struct ListPoolQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ListPoolResponse {
    pub entries: Vec<WorkPoolEntry>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct PromotePoolRequest {
    pub work_id: String,
    /// If true, also set as pool active (redundant since promote always sets active).
    pub set_default: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ArchivePoolRequest {
    pub entry_id: String,
}

#[derive(Debug, Clone)]
pub struct AddInspirationRequest {
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct AddInspirationResponse {
    pub item_id: String,
    pub rel_path: String,
}

#[derive(Debug, Clone)]
pub struct ListInspirationQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ListInspirationResponse {
    pub items: Vec<WorkInspirationItem>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct WorkInspirationItem {
    pub item_id: String,
    /// V1.42 P-last (R-V141P1-11): hidden from API — local-first, always the active creator.
    pub creator_id: String,
    pub rel_path: String,
    pub title: String,
    pub status: String,
    pub promoted_work_id: Option<String>,
    pub created_at: String,
    pub promoted_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromoteInspirationRequest {
    pub item_id: String,
    /// Optional idea override for the new Work's `initial_idea`.
    pub idea: Option<String>,
    /// If true, set as pool active after creation.
    pub set_default: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PromoteInspirationResponse {
    pub work_id: String,
    pub pool_entry_id: String,
}

#[derive(Debug, Clone)]
pub struct ArchiveInspirationRequest {
    pub item_id: String,
}

#[derive(Debug, Clone)]
pub struct WorkReconcileReport {
    pub created: u32,
    pub updated: u32,
    pub resynced: u32,
    pub preserved: u32,
}


#[derive(Debug, thiserror::Error)]
enum WorkFault {
    #[error("{message}")]
    BadRequest { code: String, message: String },
    #[error("{message}")]
    Internal { code: String, message: String },
    #[error("{0}")]
    Conflict(String),
    #[error("{reason}")]
    Locked { resource: String, reason: String },
    #[error("{reason}")]
    Forbidden { resource: String, reason: String },
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}
impl From<WorkFault> for CoreError {
    fn from(error: WorkFault) -> Self {
        match error {
            WorkFault::BadRequest { code, message } => Self::InvalidInput { field: code, reason: message },
            // The legacy internal classification (DATABASE_ERROR, CONTRACT_ERROR) rides
            // verbatim as `<CODE>: <message>`; the daemon `work_error` adapter re-emits
            // the code instead of collapsing it to the shared `CORE_ERROR` shape.
            WorkFault::Internal { code, message } => Self::Internal { category: format!("{code}: {message}") },
            WorkFault::Conflict(message) => Self::Forbidden { resource: format!("work_conflict:{message}") },
            WorkFault::Locked { reason, .. } => Self::Forbidden { resource: format!("work_locked:{reason}") },
            WorkFault::Forbidden { reason, .. } => Self::Forbidden { resource: format!("work_pool_forbidden:{reason}") },
            WorkFault::NotFound(resource) => Self::NotFound { resource },
            WorkFault::Core(error) => error,
        }
    }
}

fn wire_cast<T: serde::Serialize, U: serde::de::DeserializeOwned>(value: T) -> Result<U, WorkFault> {
    serde_json::to_value(value).and_then(serde_json::from_value).map_err(|error| WorkFault::Internal { code: "CONTRACT_ERROR".into(), message: error.to_string() })
}
impl CoreService {
    pub async fn create_work_with_outcome(&self, principal: &Principal, req: CreateWorkRequest) -> CoreResult<(bool, CreateWorkResponse)> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        create_work(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn list_works(&self, principal: &Principal, query: ListWorksQuery) -> CoreResult<ListWorksResponse> {
        self.verify_principal(principal)?;
        let response = list_works(self, principal, query).await.map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }
    pub async fn get_work(&self, principal: &Principal, work_id: String) -> CoreResult<WorkDetails> {
        self.verify_principal(principal)?;
        let response = get_work(self, principal, work_id).await.map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }
    pub async fn patch_work(&self, principal: &Principal, work_id: String, req: WorkPatchRequest) -> CoreResult<WorkDetails> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        patch_work(self, principal, work_id, req).await.map_err(CoreError::from)
    }
    pub async fn append_work_inspiration(&self, principal: &Principal, work_id: String, req: AppendInspirationRequest) -> CoreResult<AppendInspirationResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        append_inspiration(self, principal, work_id, req).await.map_err(CoreError::from)
    }
    pub async fn set_work_pool_active(&self, principal: &Principal, req: SetPoolActiveRequest) -> CoreResult<WorkPoolEntry> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        set_pool_active(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn release_work_completion_lock(&self, principal: &Principal, work_id: String, req: ReleaseCompletionLockRequest) -> CoreResult<WorkDetails> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        release_completion_lock_handler(self, principal, work_id, req).await.map_err(CoreError::from)
    }
    pub async fn delete_work(&self, principal: &Principal, work_id: String) -> CoreResult<()> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        delete_work(self, principal, work_id).await.map_err(CoreError::from)
    }
    pub async fn reconcile_work_chapters(&self, principal: &Principal, work_id: String, dry_run_query: ReconcileDryRunQuery) -> CoreResult<WorkReconcileReport> {
        self.verify_principal(principal)?;
        if !dry_run_query.dry_run.unwrap_or(false) { self.require_work_write()?; }
        reconcile_chapters(self, principal, work_id, dry_run_query).await
            .map(|report| WorkReconcileReport { created: report.created, updated: report.updated, resynced: report.resynced, preserved: report.preserved })
            .map_err(CoreError::from)
    }
    pub async fn list_work_pool(&self, principal: &Principal, query: ListPoolQuery) -> CoreResult<ListPoolResponse> {
        self.verify_principal(principal)?;
        let response = list_pool(self, principal, query).await.map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }
    pub async fn promote_work_pool_entry(&self, principal: &Principal, req: PromotePoolRequest) -> CoreResult<WorkPoolEntry> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        promote_pool_entry(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn archive_work_pool_entry(&self, principal: &Principal, req: ArchivePoolRequest) -> CoreResult<WorkPoolEntry> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        archive_pool_entry_handler(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn add_work_inspiration(&self, principal: &Principal, req: AddInspirationRequest) -> CoreResult<AddInspirationResponse> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        add_inspiration(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn list_work_inspiration(&self, principal: &Principal, query: ListInspirationQuery) -> CoreResult<ListInspirationResponse> {
        self.verify_principal(principal)?;
        let response = list_inspiration(self, principal, query).await.map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }
    pub async fn promote_work_inspiration(&self, principal: &Principal, req: PromoteInspirationRequest) -> CoreResult<PromoteInspirationResponse> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        promote_inspiration_handler(self, principal, req).await.map_err(CoreError::from)
    }
    pub async fn archive_work_inspiration(&self, principal: &Principal, req: ArchiveInspirationRequest) -> CoreResult<WorkInspirationItem> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        archive_inspiration_handler(self, principal, req).await.map_err(CoreError::from)
    }

    pub async fn create_work(&self, principal: &Principal, request: CreateWorkRequest) -> CoreResult<CreateWorkResponse> {
        self.create_work_with_outcome(principal, request).await.map(|(_, response)| response)
    }

    pub async fn select_work(&self, principal: &Principal, work_id: String) -> CoreResult<CoreWorkSelection> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        self.get_work(principal, work_id.clone()).await?;
        self.verify_principal(principal)?;
        nexus_local_db::novel_pool_entries::promote_to_active(&self.inner.pool, principal.creator_id(), &work_id)
            .await.map_err(crate::error::local_db_err)?;
        Ok(CoreWorkSelection {
            work_id: work_id.try_into().map_err(|_| CoreError::InvalidInput {
                field: "work_id".into(), reason: "must not be empty".into(),
            })?,
            active: true,
        })
    }

    pub(crate) async fn resolve_owned_work(&self, principal: &Principal, work_id: &str) -> CoreResult<WorkRecord> {
        self.verify_principal(principal)?;
        let work = works::get_work(&self.inner.pool, principal.creator_id(), work_id)
            .await.map_err(crate::error::local_db_err)?
            .filter(|work| work.workspace_slug == principal.workspace_slug())
            .ok_or_else(|| CoreError::NotFound { resource: format!("work {work_id}") })?;
        self.verify_principal(principal)?;
        Ok(work)
    }

    fn require_work_write(&self) -> CoreResult<()> {
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden { resource: "work: read-only core access".into() });
        }
        Ok(())
    }

    fn work_workspace_path(&self, principal: &Principal) -> CoreResult<Option<String>> {
        self.verify_principal(principal)?;
        let meta = self.inner.nexus_home.join("creators").join(principal.creator_id())
            .join("workspaces").join(principal.workspace_slug()).join("meta.json");
        let text = match std::fs::read_to_string(&meta) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(CoreError::Internal { category: format!("workspace metadata: {error}") }),
        };
        let metadata: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| CoreError::Internal { category: format!("workspace metadata: {error}") })?;
        Ok(metadata.get("creative_root").and_then(serde_json::Value::as_str).map(str::to_owned))
    }
}

fn checked_work_directory(workspace: &std::path::Path, work_ref: &str) -> std::io::Result<std::path::PathBuf> {
    if !is_valid_work_ref(work_ref) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid Work directory reference"));
    }
    let root = workspace.canonicalize()?;
    let works = root.join("Works");
    let path = works.join(work_ref);
    for candidate in [&works, &path] {
        match candidate.canonicalize() {
            Ok(resolved) if !resolved.starts_with(&works) => {
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Work directory escapes workspace"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(path)
}

async fn create_work(service: &CoreService, principal: &Principal, req: CreateWorkRequest) -> Result<(bool, CreateWorkResponse), WorkFault> {
    let pool = &service.inner.pool;
    let creator_id =
        principal.creator_id().to_string();
    let workspace_slug = principal.workspace_slug().to_string();

    // T0.4: V1.40 mandatory world_id — reject Work creation without a World binding.
    // R-V140P0-S1: Uses BadRequest (400) rather than UnprocessableEntity (422) for
    // missing required field. This is consistent with other "field missing" errors
    // in this handler. Semantic 422 is used only for preset_gates_failed.
    // R-V140P0-S4: tracing span for mandatory binding check observability.
    if req.world_id.is_none() {
        tracing::info!(creator_id = %creator_id, "create_work rejected: missing world_id binding");
        return Err(WorkFault::BadRequest {
            code: "world_id_required".to_string(),
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
        .fetch_optional(pool)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world_id existence check: {e}"),
        })?;
        if exists.is_none() {
            return Err(WorkFault::BadRequest {
                code: "invalid_world_id".to_string(),
                message: format!(
                    "world_id '{wid}' does not exist or is not owned by this creator.\n  \
                     ↳ Create a new World:  nexus42 creator world create --title \"...\"\n  \
                     ↳ List your Worlds:    nexus42 creator world list\n  \
                     World binding is required for new Works (V1.40+)."
                ),
            });
        }
    }

    // PR #53 review: validate lineage_from_work_id if present.
    // The referenced Work must exist, belong to the active creator, and be non-empty.
    if let Some(ref lineage_id) = req.lineage_from_work_id {
        if lineage_id.is_empty() {
            return Err(WorkFault::BadRequest {
                code: "invalid_lineage".to_string(),
                message: "lineage_from_work_id must not be empty; omit the field if no lineage is intended.".to_string(),
            });
        }
        let lineage_work = works::get_work(pool, &creator_id, lineage_id)
            .await
            .map_err(|e| WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("lineage_from_work_id lookup: {e}"),
            })?;
        if lineage_work.is_none() {
            return Err(WorkFault::BadRequest {
                code: "invalid_lineage".to_string(),
                message: format!(
                    "lineage_from_work_id '{lineage_id}' does not exist or is not owned by this creator."
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
        work_profile: req.work_profile,
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
    service.verify_principal(principal)?;
    let result = works::create_work_atomic(pool, &record, crid)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    match result {
        // Idempotent replay — existing Work found
        Ok(existing) => Ok((
            false,
            CreateWorkResponse {
                work_id: existing.work_id,
                status: existing.status,
            },
        )),
        // New Work created (no client_request_id)
        Err(new) => {
            // DF-60 §5.3: if set_pool_active was requested, promote in pool
            if req.set_pool_active == Some(true) {
                if let Err(e) = set_pool_active_inner(pool, &creator_id, &new.work_id).await {
                    tracing::warn!(
                        work_id = %new.work_id,
                        error = %e,
                        "set_pool_active after create failed (non-fatal)"
                    );
                }
            }
            Ok((
                true,
                CreateWorkResponse {
                    work_id: new.work_id,
                    status: new.status,
                },
            ))
        }
    }
}

async fn list_works(service: &CoreService, principal: &Principal, query: ListWorksQuery) -> Result<ListWorksResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();
    let workspace_slug = principal.workspace_slug().to_string();

    // F-F1 (V1.67): parse requested sort; default is empty (DB layer falls back
    // to `updated_at DESC`).
    let sort_terms = parse_sort_terms(
        query.sort.as_deref(),
        &["updated_at", "title", "status", "intake_status"],
        "work",
    )?;

    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = u32::try_from(query.limit.unwrap_or(100))
        .unwrap_or(100)
        .min(500);

    let filters = WorkListFilters {
        status: query.status,
        intake_status: query.intake_status,
        limit: Some(limit),
        offset: Some(offset),
        order_by: sort_terms,
    };

    let (records, total) = works::list_and_count_works(
        &service.inner.pool,
        &creator_id,
        &workspace_slug,
        &filters,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            error = %e,
            "list_works failed for creator {creator_id} — pagination unavailable"
        );
        WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        }
    })?;

    let has_more = u64::from(total) > u64::from(offset).saturating_add(u64::from(limit));
    let next_cursor = if has_more {
        Some(encode_offset_cursor(offset.saturating_add(limit)))
    } else {
        None
    };

    let items: Vec<WorkSummary> = records
        .into_iter()
        .map(|r| WorkSummary {
            work_id: r.work_id,
            title: r.title,
            status: r.status,
            intake_status: r.intake_status,
            primary_preset_id: r.primary_preset_id,
            updated_at: r.updated_at,
            completion_locked_at: r.completion_locked_at,
        })
        .collect();

    Ok(ListWorksResponse {
        items: wire_cast(items)?,
        pagination: wire_cast(PaginationInfo {
            limit: i64::from(limit),
            next_cursor,
            has_more,
        })?,
    })
}

async fn get_work(service: &CoreService, principal: &Principal, work_id: String) -> Result<WorkDetails, WorkFault> {
    let pool = &service.inner.pool;
    let creator_id =
        principal.creator_id().to_string();

    let mut record = service.resolve_owned_work(principal, &work_id).await?;

    // V1.36 P4 (T1): auto-promote works.status to 'completed' when all
    // chapters are finalized per novel-workflow-profile §6.1.
    if service.inner.access != CoreAccess::ReadOnly && record.status != "completed" && record.work_profile.as_deref() == Some("novel") {
        match nexus_local_db::work_chapters::is_work_completed(pool, &work_id).await {
            Ok(true) => {
                let now = chrono::Utc::now().to_rfc3339();
                // V1.38 P0 (T7): set works.status = 'completed' per §6.1.
                // novel_completion_status column not yet migrated — will be
                // added in a future schema change; tracked as residual.
                let patch = WorkPatch {
                    status: Some("completed".to_string()),
                    ..Default::default()
                };
                service.verify_principal(principal)?;
                match works::patch_work(pool, &creator_id, &work_id, &patch, &now).await {
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

    // V1.55 P2: auto-promote game-bible works.status to 'completed' when all
    // critical Design/*.md sections are accepted per game-bible-profile.md §8.
    if service.inner.access != CoreAccess::ReadOnly && record.status != "completed" && record.work_profile.as_deref() == Some("game_bible") {
        let workspace_path = service.work_workspace_path(principal)?.unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            match nexus_local_db::work_chapters::is_game_bible_design_complete(
                pool,
                &work_id,
                workspace_dir,
            )
            .await
            {
                Ok(true) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    let patch = WorkPatch {
                        status: Some("completed".to_string()),
                        ..Default::default()
                    };
                    service.verify_principal(principal)?;
                    match works::patch_work(pool, &creator_id, &work_id, &patch, &now).await {
                        Ok(updated) => {
                            tracing::info!(
                                target: "completion",
                                work_id = %work_id,
                                creator_id = %creator_id,
                                work_ref = ?updated.work_ref,
                                "game-bible: Auto-promoted work to 'completed' \
                                 (all critical Design sections accepted)"
                            );
                            record = updated;
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "completion",
                                work_id = %work_id,
                                error = %e,
                                "game-bible: Failed to auto-promote work to 'completed'"
                            );
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "completion",
                        work_id = %work_id,
                        error = %e,
                        "game-bible: Failed to check design completion status"
                    );
                }
            }
        }
    }

    // V1.60 P1: auto-promote script works.status to 'completed' when all
    // critical script sections are accepted per script-profile.md §8.
    if service.inner.access != CoreAccess::ReadOnly && record.status != "completed" && record.work_profile.as_deref() == Some("script") {
        let workspace_path = service.work_workspace_path(principal)?.unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            match nexus_local_db::work_chapters::is_script_complete(pool, &work_id, workspace_dir)
                .await
            {
                Ok(true) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    let patch = WorkPatch {
                        status: Some("completed".to_string()),
                        ..Default::default()
                    };
                    service.verify_principal(principal)?;
                    match works::patch_work(pool, &creator_id, &work_id, &patch, &now).await {
                        Ok(updated) => {
                            tracing::info!(
                                target: "completion",
                                work_id = %work_id,
                                creator_id = %creator_id,
                                work_ref = ?updated.work_ref,
                                "script: Auto-promoted work to 'completed' \
                                 (all critical script sections accepted)"
                            );
                            record = updated;
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "completion",
                                work_id = %work_id,
                                error = %e,
                                "script: Failed to auto-promote work to 'completed'"
                            );
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "completion",
                        work_id = %work_id,
                        error = %e,
                        "script: Failed to check script completion status"
                    );
                }
            }
        }
    }

    // V1.63 P0: auto-promote essay works.status to 'completed' when
    // Drafts/draft.md frontmatter status == finalized AND intake complete.
    if service.inner.access != CoreAccess::ReadOnly && record.status != "completed" && record.work_profile.as_deref() == Some("essay") {
        let workspace_path = service.work_workspace_path(principal)?.unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            match nexus_local_db::work_chapters::is_essay_complete(pool, &work_id, workspace_dir)
                .await
            {
                Ok(true) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    let patch = WorkPatch {
                        status: Some("completed".to_string()),
                        ..Default::default()
                    };
                    service.verify_principal(principal)?;
                    match works::patch_work(pool, &creator_id, &work_id, &patch, &now).await {
                        Ok(updated) => {
                            tracing::info!(
                                target: "completion",
                                work_id = %work_id,
                                creator_id = %creator_id,
                                work_ref = ?updated.work_ref,
                                "essay: Auto-promoted work to 'completed' \
                                 (draft finalized + intake complete)"
                            );
                            record = updated;
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "completion",
                                work_id = %work_id,
                                error = %e,
                                "essay: Failed to auto-promote work to 'completed'"
                            );
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "completion",
                        work_id = %work_id,
                        error = %e,
                        "essay: Failed to check essay completion status"
                    );
                }
            }
        }
    }

    Ok(enrich_with_chapters(pool, record).await)
}

async fn patch_work(service: &CoreService, principal: &Principal, work_id: String, req: WorkPatchRequest) -> Result<WorkDetails, WorkFault> {
    let pool = &service.inner.pool;
    let creator_id =
        principal.creator_id().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // DF-60 §4: guard mutating operations against completion-lock and runtime-lock
    let current_work = service.resolve_owned_work(principal, &work_id).await?;

    if current_work.completion_locked_at.is_some() {
        return Err(WorkFault::Conflict(
            format!(
                "work {work_id} is completion-locked since {}; use 'creator works completion-lock release' first",
                current_work.completion_locked_at.as_deref().unwrap_or("?")
            ),
        ));
    }

    if let Some(ref holder) = current_work.runtime_lock_holder {
        // V1.42 P0: allow stale locks to be force-cleared by acquire below.
        let ttl = nexus_local_db::ttl_from_env();
        if !nexus_local_db::is_lock_stale(&current_work, ttl) {
            return Err(WorkFault::Locked {
                resource: "work".to_string(),
                reason: format!(
                    "work {work_id} is locked by '{holder}'; wait for release or check 'creator works status'"
                ),
            });
        }
    }

    // V1.42 P0 (T2): Acquire runtime lock for this mutating operation.
    service.verify_principal(principal)?;
    let lock = RuntimeLockGuard::acquire(pool, &creator_id, &work_id).await?;

    // Capture every result before releasing: validation/DB failures must not
    // strand the runtime holder, including the stage-change branch.
    let result = async {
        service.verify_principal(principal)?;
        if req.current_stage.is_some() || req.stage_status.is_some() {
            patch_work_stage(service, principal, &creator_id, &work_id, &req, &now).await
        } else {
            apply_non_stage_fields(pool, &creator_id, &work_id, &req, &now).await?;
            works::get_work(pool, &creator_id, &work_id).await
                .map_err(crate::error::local_db_err)?
                .ok_or_else(|| WorkFault::NotFound(format!("work {work_id}")))
        }
    }.await;
    lock.release().await;
    result.map(WorkDetails::from)
}

async fn append_inspiration(service: &CoreService, principal: &Principal, work_id: String, req: AppendInspirationRequest) -> Result<AppendInspirationResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // V1.42.1 (R-V142-MERGE-CI-001): Verify work exists BEFORE acquiring the
    // runtime lock. The previous ordering (P0 T2 wiring in e8993870) acquired
    // the lock before the existence check, which caused `acquire_runtime_lock`
    // to fail with `MissingVersionKey` → mapped to 500 instead of 404. It also
    // leaked the holder column for any DB-level reads until the row was
    // touched again. Mirrors the pattern in `patch_work` and
    // `reconcile_work_chapters` (existence check first, lock after).
    let work = nexus_local_db::works::get_work(&service.inner.pool, &creator_id, &work_id)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| WorkFault::NotFound(format!("work {work_id}")))?;

    // V1.39 §5.6 (T6): Single FL-E driver invariant.
    // If the Work has an active auto-chain driver, reject side input
    // to prevent concurrent schedule conflicts.
    if work.auto_chain_enabled && work.driver_schedule_id.is_some() {
        return Err(WorkFault::Conflict(format!(
            "AUTO_CHAIN_DRIVER_ACTIVE: Work {} has an active auto-chain driver schedule ({}). \
             Side input is not allowed while auto-chain is running. \
             Wait for the current stage to complete or pause the driver first.",
            work_id,
            work.driver_schedule_id.as_deref().unwrap_or("?")
        )));
    }

    // V1.42 P0 (T2): Acquire runtime lock for this mutating operation.
    service.verify_principal(principal)?;
    let lock = RuntimeLockGuard::acquire(&service.inner.pool, &creator_id, &work_id).await?;

    // Build JSON for inspiration entry
    let entry = serde_json::json!({
        "at": now.clone(),
        "note": req.note,
    });
    let entry_json = serde_json::to_string(&entry).unwrap_or_default();

    // R-V133P1-04: append_inspiration now uses tx + Rust append and returns updated record
    //
    // V1.42.1 (R-V142-MERGE-CI-001): release before propagating DB error.
    // The work existence was verified at the top of this handler; a
    // `MissingVersionKey` here would mean concurrent deletion (race), so the
    // 404 mapping is preserved while still releasing the lock.
    let updated = match works::append_inspiration(
        &service.inner.pool,
        &creator_id,
        &work_id,
        &entry_json,
        &now,
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            lock.release().await;
            return Err(match &e {
                nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                    WorkFault::NotFound(format!("work {work_id}"))
                }
                _ => WorkFault::Internal {
                    code: "DATABASE_ERROR".to_string(),
                    message: e.to_string(),
                },
            });
        }
    };

    // Derive count from post-state (not pre-fetch + 1)
    let count = serde_json::from_str::<serde_json::Value>(&updated.inspiration_log)
        .ok()
        .and_then(|v| v.as_array().map(Vec::len))
        .unwrap_or(0);

    // V1.42 P0 (T2): Release runtime lock before returning.
    lock.release().await;

    Ok(AppendInspirationResponse {
        work_id,
        inspiration_count: i64::try_from(count).unwrap_or(i64::MAX),
    })
}

async fn set_pool_active(service: &CoreService, principal: &Principal, req: SetPoolActiveRequest) -> Result<WorkPoolEntry, WorkFault> {
    // IDOR fix: read active creator from config, reject body mismatch.
    let active_creator =
        principal.creator_id().to_string();
    if req.creator_id.is_some() && req.creator_id != Some(active_creator.clone()) {
        return Err(WorkFault::Forbidden {
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
        return Err(WorkFault::BadRequest {
            code: "invalid_action".to_string(),
            message: format!(
                "unsupported action '{}'; expected 'set_pool_active'",
                req.action
            ),
        });
    }

    service.resolve_owned_work(principal, &req.work_id).await?;

    let entry = set_pool_active_inner(&service.inner.pool, &creator_id, &req.work_id)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok(entry)
}

async fn release_completion_lock_handler(service: &CoreService, principal: &Principal, work_id: String, req: ReleaseCompletionLockRequest) -> Result<WorkDetails, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    // Step 1: Look up the Work record
    let work = service.resolve_owned_work(principal, &work_id).await?;

    // Verify the work is actually completion-locked
    if work.completion_locked_at.is_none() {
        return Err(WorkFault::BadRequest {
            code: "not_locked".to_string(),
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
    service.verify_principal(principal)?;

    let updated = works::patch_work(&service.inner.pool, &creator_id, &work_id, &patch, &now)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Step 3: Delete on-disk lock file (best-effort; DB is SSOT)
    if let Some(ref work_ref) = updated.work_ref {
        let workspace_path = service.work_workspace_path(principal)?.unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            if let Err(e) = checked_work_directory(workspace_dir, work_ref).and_then(|_| {
                nexus_local_db::work_stage::release_completion_lock(workspace_dir, work_ref)
            }) {
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

    Ok(WorkDetails::from(updated))
}

async fn delete_work(service: &CoreService, principal: &Principal, work_id: String) -> Result<(), WorkFault> {
    let pool = &service.inner.pool;
    let creator_id =
        principal.creator_id().to_string();

    // Rule 1: existence check BEFORE lock acquire (V1.42.1 hotfix rule).
    let work = service.resolve_owned_work(principal, &work_id).await?;

    // Reject while completion-locked or actively locked — author must release
    // those first. Cascading a delete under either state would silently drop
    // an in-flight authoring session.
    if work.completion_locked_at.is_some() {
        return Err(WorkFault::Conflict(format!(
            "work {work_id} is completion-locked since {}; use 'creator works completion-lock release' first",
            work.completion_locked_at.as_deref().unwrap_or("?")
        )));
    }
    if let Some(ref holder) = work.runtime_lock_holder {
        let ttl = nexus_local_db::ttl_from_env();
        if !nexus_local_db::is_lock_stale(&work, ttl) {
            return Err(WorkFault::Locked {
                resource: "work".to_string(),
                reason: format!(
                    "work {work_id} is locked by '{holder}'; wait for release or check 'creator works status'"
                ),
            });
        }
    }

    // Acquire the runtime lock for this mutating operation.
    service.verify_principal(principal)?;
    let lock = RuntimeLockGuard::acquire(pool, &creator_id, &work_id).await?;

    // Hard-delete the Work row. SQLite FK cascades drop the children:
    // work_chapters / work_chapter_volumes / findings / novel_pool_entries /
    // reading_progress / reading_annotations. inspiration_items.promoted_work_id
    // and works.lineage_from_work_id cascade SET NULL.
    //
    // SAFETY: DELETE matches works DDL in 20260604_works_table.sql; runtime
    // sqlx::query is acceptable per the narrative_write.rs precedent for
    // schema-mutable writes and avoids regenerating the .sqlx/ cache.
    let deleted = match sqlx::query("DELETE FROM works WHERE work_id = ? AND creator_id = ?")
        .bind(&work_id)
        .bind(&creator_id)
        .execute(pool)
        .await
    {
        Ok(res) => res.rows_affected(),
        Err(e) => {
            lock.release().await;
            return Err(WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("delete_work failed: {e}"),
            });
        }
    };

    lock.release().await;

    if deleted == 0 {
        // Concurrent delete raced between get_work and DELETE. Treat as 404 —
        // the row is gone regardless of who removed it.
        return Err(WorkFault::NotFound(format!("work {work_id}")));
    }

    // Best-effort on-disk cleanup: remove the Work directory (Manuscripts +
    // Outlines). DB is SSOT — orphan files here do not corrupt state.
    if let Some(ref work_ref) = work.work_ref {
        let workspace_path = service.work_workspace_path(principal)?.unwrap_or_default();
        if !workspace_path.is_empty() {
            let workspace_dir = std::path::Path::new(&workspace_path);
            let works_dir = match checked_work_directory(workspace_dir, work_ref) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(work_id = %work_id, %error, "Skipping unsafe Work directory cleanup");
                    return Ok(());
                }
            };
            if works_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&works_dir) {
                    tracing::warn!(
                        work_id = %work_id,
                        work_ref = %work_ref,
                        path = %works_dir.display(),
                        error = %e,
                        "Work directory deletion failed (non-fatal; DB is SSOT)"
                    );
                }
            }
        }
    }

    tracing::info!(
        target: "works.delete",
        work_id = %work_id,
        creator_id = %creator_id,
        "Work hard-deleted (cascade via FK)"
    );

    Ok(())
}

async fn reconcile_chapters(service: &CoreService, principal: &Principal, work_id: String, dry_run_query: ReconcileDryRunQuery) -> Result<nexus_local_db::work_chapters::ReconcileReport, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();
    let pool = &service.inner.pool;
    let dry_run = dry_run_query.dry_run.unwrap_or(false);

    // Get the Work record to find work_ref
    let work = service.resolve_owned_work(principal, &work_id).await?;

    let work_ref = work
        .story_ref
        .as_deref()
        .ok_or_else(|| WorkFault::BadRequest {
            code: "precondition_failed".to_string(),
            message: "`story_ref` (work_ref) not set on Work; run novel-project-init first"
                .to_string(),
        })?;

    // Defense in depth: validate work_ref slug before passing to filesystem
    // layer. Matches the same policy as
    // `nexus-orchestration::capability::builtins::novel_scaffold_sanitize::validate_work_ref`.
    if !is_valid_work_ref(work_ref) {
        return Err(WorkFault::BadRequest {
            code: "invalid_work_ref".to_string(),
            message: format!(
                "work_ref '{work_ref}' is not a valid slug (expected [a-z0-9][a-z0-9-]{{0,63}})"
            ),
        });
    }

    // Resolve workspace root from state
    let workspace_path_str = service.work_workspace_path(principal)?.unwrap_or_default();
    let workspace_root = std::path::Path::new(&workspace_path_str);

    let now = chrono::Utc::now().to_rfc3339();

    // V1.49 P2 (R-V148P4-W2): dry-run path — compute the report only, with no
    // runtime-lock acquire and no filesystem/DB writes (overlay §8.2).
    if dry_run {
        tracing::info!(
            work_id = %work_id,
            work_ref = %work_ref,
            "dry-run reconcile for work_id={work_id}"
        );
        let report = nexus_local_db::work_chapters::reconcile_from_filesystem(
            pool,
            &work_id,
            work_ref,
            workspace_root,
            &now,
            true,
        )
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("reconcile preview failed: {e}"),
        })?;
        return Ok(report);
    }

    // V1.49 P3 (R-V148P4-W3): shorten the runtime-lock window.
    //
    // The previous implementation acquired the lock for the *entire* reconcile
    // — filesystem walk + per-chapter DB reads + writes + file frontmatter
    // syncs — blocking other schedule operations on the same Work for the
    // full duration. Reconcile is now split into a read-only
    // [`compute_reconcile_diff`] (unlocked; the slow walk + reads live here)
    // and a write-only [`apply_reconcile_diff`] (the only phase that needs the
    // lock). Trade-off: under concurrent reconcile + mutate from the same
    // client the diff may be stale by the time the lock is re-acquired; this is
    // accepted for the local-first single-writer daemon model (documented in
    // the plan). The V1.48 P4-fix1 lock-release-on-error guarantee is
    // preserved on the apply phase below.
    let diff = nexus_local_db::work_chapters::compute_reconcile_diff(
        pool,
        &work_id,
        work_ref,
        workspace_root,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("reconcile diff failed: {e}"),
    })?;

    // Phase B: acquire the lock and apply only the writes.
    service.verify_principal(principal)?;
    let lock = RuntimeLockGuard::acquire(pool, &creator_id, &work_id).await?;
    let lock_acquired_at = chrono::Utc::now();
    tracing::info!(
        work_id = %work_id,
        acquired_at = %lock_acquired_at.to_rfc3339(),
        pending_ops = diff.ops.len(),
        "runtime_lock: acquired for reconcile write phase"
    );

    // V1.48 P4-fix1 (W-2 qc3): release the lock on both Ok and Err paths.
    // The previous `?` propagation on the reconcile call could return early
    // without releasing the runtime lock, leaving the Work unwritable until
    // restart. Mirrors the V1.42.1 hotfix pattern (R-V142-MERGE-CI-001).
    let report = match nexus_local_db::work_chapters::apply_reconcile_diff(
        pool, &work_id, &now, &diff,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            lock.release().await;
            return Err(WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("reconcile apply failed: {e}"),
            });
        }
    };

    // V1.42 P0 (T2): Release runtime lock before returning.
    lock.release().await;
    let held_ms = (chrono::Utc::now() - lock_acquired_at)
        .num_milliseconds()
        .max(0);
    tracing::info!(
        work_id = %work_id,
        held_ms,
        "runtime_lock: released after reconcile write phase"
    );

    Ok(report)
}

async fn list_pool(service: &CoreService, principal: &Principal, query: ListPoolQuery) -> Result<ListPoolResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    let limit = query.limit;
    let offset = query.offset;

    let entries = nexus_local_db::novel_pool_entries::list_pool_entries(
        &service.inner.pool,
        &creator_id,
        query.status.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let total = nexus_local_db::novel_pool_entries::count_pool_entries(
        &service.inner.pool,
        &creator_id,
        query.status.as_deref(),
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let items: Vec<WorkPoolEntry> = entries.into_iter().map(WorkPoolEntry::from).collect();

    Ok(ListPoolResponse {
        entries: items,
        total,
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    })
}

async fn promote_pool_entry(service: &CoreService, principal: &Principal, req: PromotePoolRequest) -> Result<WorkPoolEntry, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    // Verify the work exists and belongs to this creator
    service.resolve_owned_work(principal, &req.work_id).await?;

    let entry = nexus_local_db::novel_pool_entries::promote_to_active(
        &service.inner.pool,
        &creator_id,
        &req.work_id,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(WorkPoolEntry::from(entry))
}

async fn archive_pool_entry_handler(service: &CoreService, principal: &Principal, req: ArchivePoolRequest) -> Result<WorkPoolEntry, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    let entry = nexus_local_db::novel_pool_entries::archive_pool_entry(
        &service.inner.pool,
        &req.entry_id,
        &creator_id,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(WorkPoolEntry::from(entry))
}

async fn add_inspiration(service: &CoreService, principal: &Principal, req: AddInspirationRequest) -> Result<AddInspirationResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();
    let workspace_slug = principal.workspace_slug().to_string();

    let item_id = format!("npi_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();

    // Route through nexus-home-layout — resolve operational workspace dir
    // from nexus_home (~/.nexus42), not user home directly.
    let workspace_dir = service.inner.nexus_home
        .join("creators")
        .join(&creator_id)
        .join("workspaces")
        .join(&workspace_slug);

    let item = nexus_local_db::inspiration_items::create_inspiration_with_scaffold(
        &service.inner.pool,
        &item_id,
        &creator_id,
        &req.title,
        &workspace_dir,
        &now,
    )
    .await
    .map_err(|e| match &e {
        nexus_local_db::LocalDbError::ConstraintViolation { .. } => WorkFault::Conflict(
            format!("inspiration item with this path already exists: {e}"),
        ),
        _ => WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        },
    })?;

    Ok(AddInspirationResponse {
        item_id: item.item_id,
        rel_path: item.rel_path,
    })
}

async fn list_inspiration(service: &CoreService, principal: &Principal, query: ListInspirationQuery) -> Result<ListInspirationResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    let limit = query.limit;
    let offset = query.offset;

    let items = nexus_local_db::inspiration_items::list_inspiration(
        &service.inner.pool,
        &creator_id,
        query.status.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let total = nexus_local_db::inspiration_items::count_inspiration(
        &service.inner.pool,
        &creator_id,
        query.status.as_deref(),
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let dtos: Vec<WorkInspirationItem> = items.into_iter().map(WorkInspirationItem::from).collect();

    Ok(ListInspirationResponse {
        items: dtos,
        total,
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    })
}

async fn promote_inspiration_handler(service: &CoreService, principal: &Principal, req: PromoteInspirationRequest) -> Result<PromoteInspirationResponse, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    // Look up the inspiration item
    let item =
        nexus_local_db::inspiration_items::get_inspiration(&service.inner.pool, &req.item_id)
            .await
            .map_err(|e| WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| WorkFault::NotFound(format!("inspiration item {}", req.item_id)))?;

    // Cross-creator guard: only the owning creator can promote their items
    if item.creator_id != creator_id {
        return Err(WorkFault::NotFound(format!(
            "inspiration item {}",
            req.item_id
        )));
    }

    if item.status != "idea" {
        return Err(WorkFault::BadRequest {
            code: "invalid_status".to_string(),
            message: format!(
                "inspiration item {} has status '{}' — only 'idea' items can be promoted",
                req.item_id, item.status
            ),
        });
    }

    // Create a new Work from the inspiration
    let work_id = format!("wrk_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let workspace_slug = principal.workspace_slug().to_string();

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
    service.verify_principal(principal)?;
    let pool_entry = nexus_local_db::inspiration_promote_atomic(
        &service.inner.pool,
        &record,
        &creator_id,
        &work_id,
        &req.item_id,
        &now,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(PromoteInspirationResponse {
        work_id: work_id.clone(),
        pool_entry_id: pool_entry.entry_id,
    })
}

async fn archive_inspiration_handler(service: &CoreService, principal: &Principal, req: ArchiveInspirationRequest) -> Result<WorkInspirationItem, WorkFault> {
    let creator_id =
        principal.creator_id().to_string();

    let item = nexus_local_db::inspiration_items::archive_inspiration(
        &service.inner.pool,
        &req.item_id,
        &creator_id,
    )
    .await
    .map_err(|e| WorkFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(WorkInspirationItem::from(item))
}
async fn enrich_with_chapters(
    pool: &sqlx::SqlitePool,
    record: nexus_local_db::works::WorkRecord,
) -> WorkDetails {
    let mut dto = WorkDetails::from(record);

    if dto.work_profile.as_deref() != Some("novel") {
        return dto;
    }

    let work_id = &dto.work_id;

    // Populate chapter rows
    // R-V139P5-N2 (deferred): no cap/pagination on list_chapters yet.
    // Works with 100+ chapters would benefit from server-side pagination.
    // Currently returns all rows; the CLI status command consumes the full list.
    // A future slice should add `limit`/`offset` params to the DB query + API.
    match nexus_local_db::work_chapters::list_chapters(pool, work_id).await {
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

    // Populate next_chapter (§4.5.2 selection) and next_chapter_volume (V1.42)
    match nexus_local_db::work_chapters::next_chapter_volume_aware(pool, work_id).await {
        Ok(Some((volume, chapter))) => {
            dto.next_chapter = Some(chapter);
            dto.next_chapter_volume = Some(volume);
        }
        Ok(None) => {
            // No more chapters to work on
        }
        Err(e) => {
            tracing::warn!(
                target: "novel.chapters",
                work_id = %work_id,
                error = %e,
                "Failed to compute next_chapter (volume-aware)"
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
    req: &WorkPatchRequest,
    now: &str,
) -> Result<(), WorkFault> {
    let has_non_stage = req.title.is_some()
        || req.long_term_goal.is_some()
        || req.creative_brief.is_some()
        || req.intake_status.is_some()
        || req.status.is_some()
        || req.world_id.is_some()
        || req.story_ref.is_some()
        || req.primary_preset_id.is_some()
        || req.auto_review_master_on_timeout.is_some()
        || req.auto_chain_interrupted.is_some()
        || req.work_profile.is_some();

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
        work_profile: req.work_profile.clone().map(Some),
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

    // QC2 W-03: V1.40 — reject clearing world_id on a novel Work that
    // already has a non-null world_id binding. This prevents downgrading
    // a mandatory-bound Work back to worldless via PATCH.
    // R-V140P0-S4: tracing for mandatory binding check observability.
    if non_stage_patch.world_id == Some(None) {
        // Check if the Work currently has a non-null world_id.
        let current = works::get_work(pool, creator_id, work_id)
            .await
            .map_err(|e| WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| WorkFault::NotFound(format!("work {work_id}")))?;
        if current.world_id.is_some() {
            tracing::info!(work_id = %work_id, "patch_work: rejected world_id clear (non-stage path)");
            return Err(WorkFault::BadRequest {
                code: "world_clear_forbidden".to_string(),
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
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world_id existence check: {e}"),
        })?;
        if exists.is_none() {
            return Err(WorkFault::BadRequest {
                code: "invalid_world_id".to_string(),
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
                WorkFault::NotFound(format!("work {work_id}"))
            }
            _ => WorkFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            },
        })?;

    Ok(())
}

/// Handle PATCH with stage changes: gate validation + atomic transaction (R-FL-E-05 + R-FL-E-07).
async fn patch_work_stage(
    service: &CoreService,
    principal: &Principal,
    creator_id: &str,
    work_id: &str,
    req: &WorkPatchRequest,
    now: &str,
) -> Result<WorkRecord, WorkFault> {
    let pool = &service.inner.pool;
    let current = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| WorkFault::NotFound(format!("work {work_id}")))?;

    let target_stage = req
        .current_stage
        .as_deref()
        .unwrap_or(&current.current_stage);

    if req.current_stage.is_some() {
        let force = req.force.unwrap_or(false);
        let work_state = nexus_local_db::work_stage::WorkStageState {
            current_stage: current.current_stage.clone(),
            stage_status: current.stage_status.clone(),
            intake_status: current.intake_status.clone(),
        };
        nexus_local_db::work_stage::check_stage_advance(&work_state, target_stage, force)
            .map_err(|e| WorkFault::BadRequest {
                code: "invalid_stage".to_string(),
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
    service.verify_principal(principal)?;
    let _updated = works::advance_work_stage_atomic(
        pool,
        creator_id,
        work_id,
        target_stage,
        target_status,
        now,
    )
    .await
    .map_err(|e| match &e {
        nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
            WorkFault::NotFound(format!("work {work_id}"))
        }
        nexus_local_db::LocalDbError::ConstraintViolation { constraint, .. } => {
            WorkFault::Conflict(constraint.clone())
        }
        _ => WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        },
    })?;

    // Only apply non-stage fields after the stage transition succeeds.
    service.verify_principal(principal)?;
    apply_non_stage_fields(pool, creator_id, work_id, req, now).await?;

    // Re-fetch to get the fully updated record (stage + non-stage fields).
    let final_record = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| WorkFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| WorkFault::NotFound(format!("work {work_id}")))?;

    tracing::info!(
        target: "fl_e.audit",
        work_id = %final_record.work_id,
        current_stage = %final_record.current_stage,
        stage_status = %final_record.stage_status,
        "FL-E stage updated via PATCH (atomic)"
    );

    Ok(final_record)
}
fn check_stage_status_transition(
    current_status: &str,
    target_status: &str,
    force: bool,
) -> Result<(), WorkFault> {
    // Terminal status values that require gate validation or explicit force.
    const TERMINAL_STATUSES: &[&str] = &["complete", "skipped"];

    if force {
        return Ok(());
    }

    if TERMINAL_STATUSES.contains(&target_status) && !TERMINAL_STATUSES.contains(&current_status) {
        return Err(WorkFault::BadRequest {
            code: "invalid_status_transition".to_string(),
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
) -> Result<WorkPoolEntry, nexus_local_db::LocalDbError> {
    let entry =
        nexus_local_db::novel_pool_entries::promote_to_active(pool, creator_id, work_id).await?;

    Ok(WorkPoolEntry::from(entry))
}
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

struct RuntimeLockGuard { pool: sqlx::SqlitePool, creator_id: String, work_id: String, holder: String }
impl RuntimeLockGuard {
    async fn acquire(pool: &sqlx::SqlitePool, creator_id: &str, work_id: &str) -> Result<Self, WorkFault> {
        let holder = nexus_local_db::cli_holder("core");
        let acquired = nexus_local_db::acquire_runtime_lock(pool, creator_id, work_id, &holder, nexus_local_db::ttl_from_env(), true)
            .await.map_err(crate::error::local_db_err)?;
        match acquired {
            nexus_local_db::AcquireResult::Acquired { .. } => Ok(Self { pool: pool.clone(), creator_id: creator_id.into(), work_id: work_id.into(), holder }),
            nexus_local_db::AcquireResult::Locked { holder, .. } => Err(WorkFault::Locked { resource: "work".into(), reason: format!("work {work_id} is locked by '{holder}'; wait for release or check 'creator works status'") }),
        }
    }
    async fn release(self) {
        if let Err(error) = nexus_local_db::release_runtime_lock(&self.pool, &self.creator_id, &self.work_id, &self.holder).await {
            tracing::warn!(work_id = %self.work_id, %error, "runtime lock release failed");
        }
    }
}

// Single authority for the Work list cursor/sort grammar: the offset-backed `v1:` cursor
// and `-key,key` sort terms (defaults, bounds, trimming, empty comma terms, descending
// terms, `<resource>_sort_invalid` / `invalid_input` errors) match the pre-extraction
// daemon `list_works` behavior exactly. Consumed only by `list_works` here; the daemon
// Work route forwards the query unchanged and never re-parses. Daemon `api::pagination`
// / `api::sort` remain separate helpers for other handler families — not for Work.
fn decode_offset_cursor(cursor: &Option<String>) -> Result<u32, WorkFault> {
    let Some(raw) = cursor else { return Ok(0); };
    raw.strip_prefix("v1:").and_then(|offset| offset.parse::<u32>().ok()).ok_or_else(|| WorkFault::BadRequest {
        code: "invalid_input".into(),
        message: "invalid pagination cursor; pass the `next_cursor` value returned by the previous response unchanged".into(),
    })
}
fn encode_offset_cursor(offset: u32) -> String { format!("v1:{offset}") }
fn parse_sort_terms(input: Option<&str>, allowed_keys: &[&str], resource: &str) -> Result<Vec<(String, bool)>, WorkFault> {
    let Some(input) = input else { return Ok(Vec::new()); };
    let mut terms = Vec::new();
    for raw in input.split(',') {
        let raw = raw.trim();
        if raw.is_empty() { continue; }
        let (ascending, key) = raw.strip_prefix('-').map_or((true, raw), |stripped| (false, stripped));
        if !allowed_keys.contains(&key) {
            return Err(WorkFault::BadRequest { code: format!("{resource}_sort_invalid"), message: format!("unsupported sort key '{key}'; allowed: {}", allowed_keys.join(", ")) });
        }
        terms.push((key.to_owned(), ascending));
    }
    Ok(terms)
}
