//! Work entity CRUD operations (V1.33 work-experience-model §3).
//!
//! Manages the `works` table — long-term creative effort containers
//! with structured briefs, inspiration logs, and schedule linkage.
//! Idempotency via separate `works_idempotency` table.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::error::LocalDbError;

/// Column list for all SELECT queries on works.
pub const WORKS_COLUMNS: &str = "\
    work_id, creator_id, workspace_slug, status, title, long_term_goal, \
    initial_idea, creative_brief, intake_status, world_id, story_ref, \
    inspiration_log, primary_preset_id, schedule_ids, created_at, updated_at, \
    current_stage, stage_status, work_profile, work_ref, total_planned_chapters, current_chapter, \
    auto_chain_enabled, driver_schedule_id, auto_chain_interrupted, auto_review_master_on_timeout, \
    runtime_lock_holder, runtime_lock_acquired_at, completion_locked_at, \
    novel_completion_status, lineage_from_work_id";

/// Map a sqlx row to [`WorkRecord`].
#[must_use]
pub fn row_to_work_record(r: &sqlx::sqlite::SqliteRow) -> WorkRecord {
    WorkRecord {
        work_id: r.get("work_id"),
        creator_id: r.get("creator_id"),
        workspace_slug: r.get("workspace_slug"),
        status: r.get("status"),
        title: r.get("title"),
        long_term_goal: r.get("long_term_goal"),
        initial_idea: r.get("initial_idea"),
        creative_brief: r.get("creative_brief"),
        intake_status: r.get("intake_status"),
        world_id: r.get("world_id"),
        story_ref: r.get("story_ref"),
        inspiration_log: r.get("inspiration_log"),
        primary_preset_id: r.get("primary_preset_id"),
        schedule_ids: r.get("schedule_ids"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        current_stage: r.get("current_stage"),
        stage_status: r.get("stage_status"),
        work_profile: r.get("work_profile"),
        work_ref: r.get("work_ref"),
        total_planned_chapters: r.get("total_planned_chapters"),
        current_chapter: r.get("current_chapter"),
        auto_chain_enabled: r.get("auto_chain_enabled"),
        driver_schedule_id: r.get("driver_schedule_id"),
        auto_chain_interrupted: r.get("auto_chain_interrupted"),
        auto_review_master_on_timeout: r.get("auto_review_master_on_timeout"),
        runtime_lock_holder: r.get("runtime_lock_holder"),
        runtime_lock_acquired_at: r.get("runtime_lock_acquired_at"),
        completion_locked_at: r.get("completion_locked_at"),
        novel_completion_status: r.get("novel_completion_status"),
        lineage_from_work_id: r.get("lineage_from_work_id"),
    }
}

/// Work record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkRecord {
    /// Unique identifier (e.g. `wrk_<uuid>`).
    pub work_id: String,
    /// Owning creator.
    pub creator_id: String,
    /// Workspace slug when created.
    pub workspace_slug: String,
    /// Work status.
    pub status: String,
    /// Human label.
    pub title: String,
    /// What "done" means.
    pub long_term_goal: String,
    /// Raw user input at start.
    pub initial_idea: String,
    /// Structured brief JSON (nullable until intake complete).
    pub creative_brief: Option<String>,
    /// Intake status.
    pub intake_status: String,
    /// Bound narrative world.
    pub world_id: Option<String>,
    /// Preset/manuscript ref.
    pub story_ref: Option<String>,
    /// Append-only inspiration log (JSON text).
    pub inspiration_log: String,
    /// Main production preset.
    pub primary_preset_id: String,
    /// Linked schedule IDs (JSON text).
    pub schedule_ids: String,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
    /// Current FL-E stage (V1.34 creator-workflow §3.1).
    pub current_stage: String,
    /// Current stage status (V1.34 creator-workflow §3.2).
    pub stage_status: String,
    /// Work profile (V1.36 novel-workflow-profile §2.1).
    pub work_profile: Option<String>,
    /// Human slug for Works/ directory (V1.36 §2.1).
    pub work_ref: Option<String>,
    /// Total planned chapters (V1.36 §2.1).
    pub total_planned_chapters: Option<i32>,
    /// Current chapter index (V1.36 §2.1).
    pub current_chapter: i32,
    /// Auto-chain enabled flag (V1.39 §5.4, default true).
    pub auto_chain_enabled: bool,
    /// Currently-running FL-E driver schedule ID (V1.39 §5.4, nullable).
    pub driver_schedule_id: Option<String>,
    /// Set true when auto-chain driver is interrupted externally (V1.39 §5.4).
    pub auto_chain_interrupted: bool,
    /// Opt-in: when true the stale-findings watcher auto-enqueues
    /// `novel-review-master` for this Work after the timeout threshold
    /// (V1.39 P4 T4, default false).
    pub auto_review_master_on_timeout: bool,
    /// Runtime lock holder (V1.41 DF-60 §4): `cli:<pid>:<uuid>` or `daemon:schedule:<id>`.
    pub runtime_lock_holder: Option<String>,
    /// When the runtime lock was acquired (V1.41 DF-60 §4, ISO-8601, nullable).
    pub runtime_lock_acquired_at: Option<String>,
    /// When completion-lock was applied (V1.41 DF-60 §3, ISO-8601, nullable).
    pub completion_locked_at: Option<String>,
    /// Novel completion status (V1.41 DF-60 §2): `finalize_complete` | `reopened` | NULL.
    pub novel_completion_status: Option<String>,
    /// Parent Work ID when created via `run start --from-work` (V1.41 DF-60 §5.2, nullable).
    pub lineage_from_work_id: Option<String>,
}

/// Inspiration log entry — `{at, note}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InspirationLogEntry {
    /// ISO 8601 timestamp.
    pub at: String,
    /// Free-text direction / inspiration.
    pub note: String,
}

/// Filters for listing works.
#[derive(Debug, Clone, Default)]
pub struct WorkListFilters {
    /// Filter by status.
    pub status: Option<String>,
    /// Filter by `intake_status`.
    pub intake_status: Option<String>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Pagination offset.
    pub offset: Option<u32>,
}

/// Fields that can be patched on a Work.
#[derive(Debug, Clone, Default)]
pub struct WorkPatch {
    /// New title.
    pub title: Option<String>,
    /// New `long_term_goal`.
    pub long_term_goal: Option<String>,
    /// New creative brief JSON.
    pub creative_brief: Option<Option<String>>,
    /// New `intake_status`.
    pub intake_status: Option<String>,
    /// New status.
    pub status: Option<String>,
    /// New `world_id`.
    pub world_id: Option<Option<String>>,
    /// New `story_ref`.
    pub story_ref: Option<Option<String>>,
    /// New `primary_preset_id`.
    pub primary_preset_id: Option<String>,
    /// New `schedule_ids` JSON.
    pub schedule_ids: Option<String>,
    /// New `current_stage` (V1.34 FL-E).
    pub current_stage: Option<String>,
    /// New `stage_status` (V1.34 FL-E).
    pub stage_status: Option<String>,
    /// New `work_profile` (V1.36 novel-workflow-profile §2.1).
    pub work_profile: Option<Option<String>>,
    /// New `work_ref` (V1.36 §2.1).
    pub work_ref: Option<Option<String>>,
    /// New `total_planned_chapters` (V1.36 §2.1).
    pub total_planned_chapters: Option<Option<i32>>,
    /// New `current_chapter` (V1.36 §2.1).
    pub current_chapter: Option<i32>,
    /// New `auto_chain_enabled` (V1.39 §5.4).
    pub auto_chain_enabled: Option<bool>,
    /// New `driver_schedule_id` (V1.39 §5.4, nullable).
    pub driver_schedule_id: Option<Option<String>>,
    /// New `auto_chain_interrupted` (V1.39 §5.4).
    pub auto_chain_interrupted: Option<bool>,
    /// New `auto_review_master_on_timeout` opt-in flag (V1.39 P4 T4).
    pub auto_review_master_on_timeout: Option<bool>,
    /// New `runtime_lock_holder` (V1.41 DF-60 §4, nullable).
    pub runtime_lock_holder: Option<Option<String>>,
    /// New `runtime_lock_acquired_at` (V1.41 DF-60 §4, nullable).
    pub runtime_lock_acquired_at: Option<Option<String>>,
    /// New `completion_locked_at` (V1.41 DF-60 §3, nullable).
    pub completion_locked_at: Option<Option<String>>,
    /// New `novel_completion_status` (V1.41 DF-60 §2, nullable).
    pub novel_completion_status: Option<Option<String>>,
    /// New `lineage_from_work_id` (V1.41 DF-60 §5.2, nullable).
    pub lineage_from_work_id: Option<Option<String>>,
}

/// Create a new Work (simple, non-transactional).
///
/// Prefer [`create_work_tx`] when idempotency is required (single tx
/// wrapping check + create + idempotency record).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_work(pool: &SqlitePool, record: &WorkRecord) -> Result<(), LocalDbError> {
    // SAFETY: INSERT against works table — runtime query because the table
    // was added in the same migration cycle and sqlx prepare hasn't run yet.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, long_term_goal,
         initial_idea, creative_brief, intake_status, world_id, story_ref, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at, current_stage, stage_status,
         work_profile, work_ref, total_planned_chapters, current_chapter,
         auto_chain_enabled, driver_schedule_id, auto_chain_interrupted,
         auto_review_master_on_timeout,
         runtime_lock_holder, runtime_lock_acquired_at, completion_locked_at,
         novel_completion_status, lineage_from_work_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?,
                 NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(&record.work_id)
    .bind(&record.creator_id)
    .bind(&record.workspace_slug)
    .bind(&record.status)
    .bind(&record.title)
    .bind(&record.long_term_goal)
    .bind(&record.initial_idea)
    .bind(&record.creative_brief)
    .bind(&record.intake_status)
    .bind(&record.world_id)
    .bind(&record.story_ref)
    .bind(&record.inspiration_log)
    .bind(&record.primary_preset_id)
    .bind(&record.schedule_ids)
    .bind(&record.created_at)
    .bind(&record.updated_at)
    .bind(&record.current_stage)
    .bind(&record.stage_status)
    .bind(&record.work_profile)
    .bind(&record.work_ref)
    .bind(record.total_planned_chapters)
    .bind(record.current_chapter)
    .bind(record.auto_chain_enabled)
    .bind(record.auto_chain_interrupted)
    .bind(record.auto_review_master_on_timeout)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a Work row inside an existing transaction.
async fn insert_work_tx(
    tx: &mut Transaction<'_, Sqlite>,
    record: &WorkRecord,
) -> Result<(), LocalDbError> {
    // SAFETY: INSERT against works table — runtime query because the table
    // was added in the same migration cycle and sqlx prepare hasn't run yet.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, long_term_goal,
         initial_idea, creative_brief, intake_status, world_id, story_ref, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at, current_stage, stage_status,
         work_profile, work_ref, total_planned_chapters, current_chapter,
         auto_chain_enabled, driver_schedule_id, auto_chain_interrupted,
         auto_review_master_on_timeout,
         runtime_lock_holder, runtime_lock_acquired_at, completion_locked_at,
         novel_completion_status, lineage_from_work_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?,
                 NULL, NULL, NULL, NULL, ?)",
    )
    .bind(&record.work_id)
    .bind(&record.creator_id)
    .bind(&record.workspace_slug)
    .bind(&record.status)
    .bind(&record.title)
    .bind(&record.long_term_goal)
    .bind(&record.initial_idea)
    .bind(&record.creative_brief)
    .bind(&record.intake_status)
    .bind(&record.world_id)
    .bind(&record.story_ref)
    .bind(&record.inspiration_log)
    .bind(&record.primary_preset_id)
    .bind(&record.schedule_ids)
    .bind(&record.created_at)
    .bind(&record.updated_at)
    .bind(&record.current_stage)
    .bind(&record.stage_status)
    .bind(&record.work_profile)
    .bind(&record.work_ref)
    .bind(record.total_planned_chapters)
    .bind(record.current_chapter)
    .bind(record.auto_chain_enabled)
    .bind(record.auto_chain_interrupted)
    .bind(record.auto_review_master_on_timeout)
    .bind(&record.lineage_from_work_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Record an idempotency mapping after creating a Work.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or unique constraint violated.
pub async fn record_idempotency(
    pool: &SqlitePool,
    creator_id: &str,
    client_request_id: &str,
    work_id: &str,
    created_at: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: INSERT against works_idempotency table — runtime query because
    // the table was added in the same migration cycle and sqlx prepare hasn't run yet.
    sqlx::query(
        "INSERT INTO works_idempotency (creator_id, client_request_id, work_id, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(creator_id)
    .bind(client_request_id)
    .bind(work_id)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record idempotency mapping inside an existing transaction.
async fn record_idempotency_tx(
    tx: &mut Transaction<'_, Sqlite>,
    creator_id: &str,
    client_request_id: &str,
    work_id: &str,
    created_at: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: INSERT against works_idempotency — runtime query.
    sqlx::query(
        "INSERT INTO works_idempotency (creator_id, client_request_id, work_id, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(creator_id)
    .bind(client_request_id)
    .bind(work_id)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Look up idempotency key inside an existing transaction.
/// Returns the `work_id` if found, `None` otherwise.
async fn find_idempotency_key_tx(
    tx: &mut Transaction<'_, Sqlite>,
    creator_id: &str,
    client_request_id: &str,
) -> Result<Option<String>, LocalDbError> {
    // SAFETY: SELECT against works_idempotency — runtime query.
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT work_id FROM works_idempotency WHERE creator_id = ? AND client_request_id = ?",
    )
    .bind(creator_id)
    .bind(client_request_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(wid,)| wid))
}

/// Atomically create a Work with idempotency check and recording in a single transaction.
///
/// Returns `Ok(Ok(existing_record))` if the `(creator_id, client_request_id)` mapping
/// already exists (idempotent replay), or `Ok(Ok(new_record))` for fresh creation.
/// Returns `Ok(Err(record))` when no `client_request_id` was provided (simple create).
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn create_work_atomic(
    pool: &SqlitePool,
    record: &WorkRecord,
    client_request_id: Option<&str>,
) -> Result<Result<WorkRecord, WorkRecord>, LocalDbError> {
    let mut tx = pool.begin().await?;

    if let Some(crid) = client_request_id {
        // Check idempotency table inside tx
        if let Some(existing_wid) =
            find_idempotency_key_tx(&mut tx, &record.creator_id, crid).await?
        {
            // Idempotent replay — fetch the existing work and return it
            let existing = get_work(pool, &record.creator_id, &existing_wid).await?;
            // We don't need the tx anymore (read-only path found existing)
            tx.rollback().await?;
            return Ok(Ok(existing.unwrap_or_else(|| record.clone())));
        }

        // Not found — create + record atomically
        insert_work_tx(&mut tx, record).await?;
        record_idempotency_tx(
            &mut tx,
            &record.creator_id,
            crid,
            &record.work_id,
            &record.created_at,
        )
        .await?;
    } else {
        // No idempotency key — just create
        insert_work_tx(&mut tx, record).await?;
    }
    tx.commit().await?;
    Ok(Err(record.clone()))
}

/// Find a Work by its idempotency key.
///
/// Returns `None` if no mapping exists for the given `(creator_id, client_request_id)`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn find_work_by_client_request_id(
    pool: &SqlitePool,
    creator_id: &str,
    client_request_id: &str,
) -> Result<Option<WorkRecord>, LocalDbError> {
    // SAFETY: SELECT against works_idempotency table — runtime query because
    // the table was added in the same migration cycle and sqlx prepare hasn't run yet.
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT work_id FROM works_idempotency WHERE creator_id = ? AND client_request_id = ?",
    )
    .bind(creator_id)
    .bind(client_request_id)
    .fetch_optional(pool)
    .await?;

    let Some((work_id,)) = row else {
        return Ok(None);
    };

    get_work(pool, creator_id, &work_id).await
}

/// Get a single Work by ID.
///
/// Returns `None` if the record doesn't exist or belongs to a different creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Option<WorkRecord>, LocalDbError> {
    // SAFETY: SELECT against works table — runtime query because the table
    // was added in the same migration cycle and sqlx prepare hasn't run yet.
    let row = sqlx::query(&format!(
        "SELECT {WORKS_COLUMNS} FROM works WHERE work_id = ? AND creator_id = ?"
    ))
    .bind(work_id)
    .bind(creator_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_work_record))
}

/// List Works for a creator with optional filters.
///
/// Returns records ordered by `updated_at` descending (most recent first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_works(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    filters: &WorkListFilters,
) -> Result<Vec<WorkRecord>, LocalDbError> {
    list_works_inner(pool, creator_id, workspace_slug, filters).await
}

/// Count total Works matching the given filters (ignores limit/offset).
///
/// Used by the list handler to return the true total row count for
/// pagination, independent of the page size.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_works(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    filters: &WorkListFilters,
) -> Result<u32, LocalDbError> {
    count_works_inner(pool, creator_id, workspace_slug, filters).await
}

/// List and count Works in a shared transaction for consistent pagination metadata.
///
/// Runs both `SELECT ... FROM works` and `SELECT COUNT(*) FROM works` inside a
/// single `BEGIN IMMEDIATE` / `COMMIT` so that concurrent writes cannot cause the
/// `total` to diverge from the actual row set.
///
/// # Errors
///
/// Returns `LocalDbError` if the transaction or any query within it fails.
pub async fn list_and_count_works(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    filters: &WorkListFilters,
) -> Result<(Vec<WorkRecord>, u32), LocalDbError> {
    let mut tx: Transaction<'_, Sqlite> = pool.begin().await?;
    let records = list_works_inner(&mut *tx, creator_id, workspace_slug, filters).await?;
    let total = count_works_inner(&mut *tx, creator_id, workspace_slug, filters).await?;
    tx.commit().await?;
    Ok((records, total))
}

async fn list_works_inner<'e, E: sqlx::Executor<'e, Database = Sqlite>>(
    executor: E,
    creator_id: &str,
    workspace_slug: &str,
    filters: &WorkListFilters,
) -> Result<Vec<WorkRecord>, LocalDbError> {
    let mut where_clauses = vec![
        "creator_id = ?".to_string(),
        "workspace_slug = ?".to_string(),
    ];

    if filters.status.is_some() {
        where_clauses.push("status = ?".to_string());
    }
    if filters.intake_status.is_some() {
        where_clauses.push("intake_status = ?".to_string());
    }

    let where_sql = where_clauses.join(" AND ");

    let limit = filters.limit.unwrap_or(100);
    let offset = filters.offset.unwrap_or(0);

    // SAFETY: Dynamic SQL required for optional WHERE filters.
    // All user inputs are passed as bound parameters, not interpolated.
    let sql = format!(
        "SELECT {WORKS_COLUMNS} FROM works WHERE {where_sql}
         ORDER BY updated_at DESC
         LIMIT ? OFFSET ?"
    );

    let mut query = sqlx::query(&sql).bind(creator_id).bind(workspace_slug);

    if let Some(ref s) = filters.status {
        query = query.bind(s);
    }
    if let Some(ref s) = filters.intake_status {
        query = query.bind(s);
    }

    query = query.bind(limit).bind(offset);

    let rows = query.fetch_all(executor).await?;

    Ok(rows.iter().map(row_to_work_record).collect())
}

async fn count_works_inner<'e, E: sqlx::Executor<'e, Database = Sqlite>>(
    executor: E,
    creator_id: &str,
    workspace_slug: &str,
    filters: &WorkListFilters,
) -> Result<u32, LocalDbError> {
    let mut where_clauses = vec![
        "creator_id = ?".to_string(),
        "workspace_slug = ?".to_string(),
    ];

    if filters.status.is_some() {
        where_clauses.push("status = ?".to_string());
    }
    if filters.intake_status.is_some() {
        where_clauses.push("intake_status = ?".to_string());
    }

    let where_sql = where_clauses.join(" AND ");

    // SAFETY: Dynamic SQL required for optional WHERE filters.
    // All user inputs are passed as bound parameters, not interpolated.
    let sql = format!("SELECT COUNT(*) AS cnt FROM works WHERE {where_sql}");

    let mut query = sqlx::query(&sql).bind(creator_id).bind(workspace_slug);

    if let Some(ref s) = filters.status {
        query = query.bind(s);
    }
    if let Some(ref s) = filters.intake_status {
        query = query.bind(s);
    }

    let row = query.fetch_one(executor).await?;
    // COUNT(*) returns non-negative; u32::try_from is safe for all practical row counts.
    let count: i64 = row.get("cnt");
    Ok(u32::try_from(count).unwrap_or(0))
}

/// Partially update a Work.
///
/// Only non-None fields in `patch` are applied.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or no work is found.
// SAFETY: Dynamic SQL builder with conditional SET clauses — splitting would
// require duplicating the transaction logic. The line count is inherent to the
// 22-column schema + 10 conditional set_clauses.
#[allow(clippy::too_many_lines)]
pub async fn patch_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    patch: &WorkPatch,
    now: &str,
) -> Result<WorkRecord, LocalDbError> {
    let mut set_clauses = Vec::new();

    if patch.title.is_some() {
        set_clauses.push("title = ?");
    }
    if patch.long_term_goal.is_some() {
        set_clauses.push("long_term_goal = ?");
    }
    if patch.creative_brief.is_some() {
        set_clauses.push("creative_brief = ?");
    }
    if patch.intake_status.is_some() {
        set_clauses.push("intake_status = ?");
    }
    if patch.status.is_some() {
        set_clauses.push("status = ?");
    }
    if patch.world_id.is_some() {
        set_clauses.push("world_id = ?");
    }
    if patch.story_ref.is_some() {
        set_clauses.push("story_ref = ?");
    }
    if patch.primary_preset_id.is_some() {
        set_clauses.push("primary_preset_id = ?");
    }
    if patch.schedule_ids.is_some() {
        set_clauses.push("schedule_ids = ?");
    }
    if patch.current_stage.is_some() {
        set_clauses.push("current_stage = ?");
    }
    if patch.stage_status.is_some() {
        set_clauses.push("stage_status = ?");
    }
    if patch.work_profile.is_some() {
        set_clauses.push("work_profile = ?");
    }
    if patch.work_ref.is_some() {
        set_clauses.push("work_ref = ?");
    }
    if patch.total_planned_chapters.is_some() {
        set_clauses.push("total_planned_chapters = ?");
    }
    if patch.current_chapter.is_some() {
        set_clauses.push("current_chapter = ?");
    }
    if patch.auto_chain_enabled.is_some() {
        set_clauses.push("auto_chain_enabled = ?");
    }
    if patch.driver_schedule_id.is_some() {
        set_clauses.push("driver_schedule_id = ?");
    }
    if patch.auto_chain_interrupted.is_some() {
        set_clauses.push("auto_chain_interrupted = ?");
    }
    if patch.auto_review_master_on_timeout.is_some() {
        set_clauses.push("auto_review_master_on_timeout = ?");
    }
    if patch.runtime_lock_holder.is_some() {
        set_clauses.push("runtime_lock_holder = ?");
    }
    if patch.runtime_lock_acquired_at.is_some() {
        set_clauses.push("runtime_lock_acquired_at = ?");
    }
    if patch.completion_locked_at.is_some() {
        set_clauses.push("completion_locked_at = ?");
    }
    if patch.novel_completion_status.is_some() {
        set_clauses.push("novel_completion_status = ?");
    }
    if patch.lineage_from_work_id.is_some() {
        set_clauses.push("lineage_from_work_id = ?");
    }

    if set_clauses.is_empty() {
        // Nothing to update — just return current record.
        return get_work(pool, creator_id, work_id).await?.ok_or_else(|| {
            LocalDbError::MissingVersionKey {
                key: format!("works/{work_id}"),
            }
        });
    }

    set_clauses.push("updated_at = ?");
    let set_sql = set_clauses.join(", ");

    // SAFETY: Dynamic SQL required for partial update.
    // All values are bound parameters, not interpolated.
    let sql = format!("UPDATE works SET {set_sql} WHERE work_id = ? AND creator_id = ?");

    let mut query = sqlx::query(&sql);

    if let Some(ref v) = patch.title {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.long_term_goal {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.creative_brief {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref v) = patch.intake_status {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.status {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.world_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.story_ref {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref v) = patch.primary_preset_id {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.schedule_ids {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.current_stage {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.stage_status {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.work_profile {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.work_ref {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.total_planned_chapters {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<i32>::None),
        }
    }
    if let Some(ref v) = patch.current_chapter {
        query = query.bind(v);
    }
    if let Some(v) = patch.auto_chain_enabled {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.driver_schedule_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(v) = patch.auto_chain_interrupted {
        query = query.bind(v);
    }
    if let Some(v) = patch.auto_review_master_on_timeout {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.runtime_lock_holder {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.runtime_lock_acquired_at {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.completion_locked_at {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.novel_completion_status {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.lineage_from_work_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }

    query = query.bind(now).bind(work_id).bind(creator_id);

    query.execute(pool).await?;

    get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("works/{work_id}"),
        })
}

/// Patch a Work inside an existing transaction (V1.37 R-V136P1-02).
///
/// Same logic as [`patch_work`] but uses a caller-provided transaction
/// so the patch can be atomic with a prior `seed_chapters_tx` call.
///
/// Returns `Ok(true)` if at least one column changed, `Ok(false)` if
/// the patch was a no-op (all fields matched existing values).
///
/// Note: does NOT return the updated `WorkRecord` because reading back
/// inside the same transaction is complex; callers can read after commit.
///
/// # Errors
///
/// Returns `LocalDbError` if the UPDATE fails.
///
/// rationale: mirrors existing dynamic partial-update binder; splitting harms readability
#[allow(clippy::too_many_lines)]
pub async fn patch_work_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    creator_id: &str,
    work_id: &str,
    patch: &WorkPatch,
    now: &str,
) -> Result<bool, LocalDbError> {
    let mut set_clauses = Vec::new();

    if patch.title.is_some() {
        set_clauses.push("title = ?");
    }
    if patch.long_term_goal.is_some() {
        set_clauses.push("long_term_goal = ?");
    }
    if patch.creative_brief.is_some() {
        set_clauses.push("creative_brief = ?");
    }
    if patch.intake_status.is_some() {
        set_clauses.push("intake_status = ?");
    }
    if patch.status.is_some() {
        set_clauses.push("status = ?");
    }
    if patch.world_id.is_some() {
        set_clauses.push("world_id = ?");
    }
    if patch.story_ref.is_some() {
        set_clauses.push("story_ref = ?");
    }
    if patch.primary_preset_id.is_some() {
        set_clauses.push("primary_preset_id = ?");
    }
    if patch.schedule_ids.is_some() {
        set_clauses.push("schedule_ids = ?");
    }
    if patch.current_stage.is_some() {
        set_clauses.push("current_stage = ?");
    }
    if patch.stage_status.is_some() {
        set_clauses.push("stage_status = ?");
    }
    if patch.work_profile.is_some() {
        set_clauses.push("work_profile = ?");
    }
    if patch.work_ref.is_some() {
        set_clauses.push("work_ref = ?");
    }
    if patch.total_planned_chapters.is_some() {
        set_clauses.push("total_planned_chapters = ?");
    }
    if patch.current_chapter.is_some() {
        set_clauses.push("current_chapter = ?");
    }
    if patch.auto_chain_enabled.is_some() {
        set_clauses.push("auto_chain_enabled = ?");
    }
    if patch.driver_schedule_id.is_some() {
        set_clauses.push("driver_schedule_id = ?");
    }
    if patch.auto_chain_interrupted.is_some() {
        set_clauses.push("auto_chain_interrupted = ?");
    }
    if patch.auto_review_master_on_timeout.is_some() {
        set_clauses.push("auto_review_master_on_timeout = ?");
    }
    if patch.runtime_lock_holder.is_some() {
        set_clauses.push("runtime_lock_holder = ?");
    }
    if patch.runtime_lock_acquired_at.is_some() {
        set_clauses.push("runtime_lock_acquired_at = ?");
    }
    if patch.completion_locked_at.is_some() {
        set_clauses.push("completion_locked_at = ?");
    }
    if patch.novel_completion_status.is_some() {
        set_clauses.push("novel_completion_status = ?");
    }
    if patch.lineage_from_work_id.is_some() {
        set_clauses.push("lineage_from_work_id = ?");
    }

    if set_clauses.is_empty() {
        return Ok(false);
    }

    set_clauses.push("updated_at = ?");
    let set_sql = set_clauses.join(", ");

    // SAFETY: Dynamic SQL required for partial update.
    // All values are bound parameters, not interpolated.
    let sql = format!("UPDATE works SET {set_sql} WHERE work_id = ? AND creator_id = ?");

    let mut query = sqlx::query(&sql);

    // Bind parameters in the same order as set_clauses
    if let Some(ref v) = patch.title {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.long_term_goal {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.creative_brief {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref v) = patch.intake_status {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.status {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.world_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.story_ref {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref v) = patch.primary_preset_id {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.schedule_ids {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.current_stage {
        query = query.bind(v);
    }
    if let Some(ref v) = patch.stage_status {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.work_profile {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.work_ref {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.total_planned_chapters {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<i32>::None),
        }
    }
    if let Some(ref v) = patch.current_chapter {
        query = query.bind(v);
    }
    if let Some(v) = patch.auto_chain_enabled {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.driver_schedule_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(v) = patch.auto_chain_interrupted {
        query = query.bind(v);
    }
    if let Some(v) = patch.auto_review_master_on_timeout {
        query = query.bind(v);
    }
    if let Some(ref opt_val) = patch.runtime_lock_holder {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.runtime_lock_acquired_at {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.completion_locked_at {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.novel_completion_status {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }
    if let Some(ref opt_val) = patch.lineage_from_work_id {
        match opt_val {
            Some(v) => query = query.bind(v),
            None => query = query.bind(Option::<String>::None),
        }
    }

    query = query.bind(now).bind(work_id).bind(creator_id);

    query.execute(&mut **tx).await?;
    Ok(true)
}

/// Append an inspiration entry to a Work (atomic via transaction).
///
/// Reads the current `inspiration_log`, appends the new entry in Rust,
/// and writes back the full array inside a single transaction. This is
/// robust to whitespace/non-compact JSON and avoids fragile substr/CASE logic.
///
/// Returns the updated `WorkRecord` after the append.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or no work is found.
pub async fn append_inspiration(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    entry_json: &str,
    now: &str,
) -> Result<WorkRecord, LocalDbError> {
    let mut tx = pool.begin().await?;

    // Read current inspiration_log inside tx
    // SAFETY: Dynamic SQL required for JSON manipulation via transaction.
    // All values are bound parameters.
    let row = sqlx::query(&format!(
        "SELECT {WORKS_COLUMNS} FROM works WHERE work_id = ? AND creator_id = ?"
    ))
    .bind(work_id)
    .bind(creator_id)
    .fetch_optional(&mut *tx)
    .await?;

    let current =
        row.as_ref()
            .map(row_to_work_record)
            .ok_or_else(|| LocalDbError::MissingVersionKey {
                key: format!("works/{work_id}"),
            })?;

    // Append new entry in Rust
    let mut log: Vec<serde_json::Value> =
        serde_json::from_str(&current.inspiration_log).unwrap_or_default();
    let entry: serde_json::Value =
        serde_json::from_str(entry_json).unwrap_or_else(|_| serde_json::json!({}));
    log.push(entry);
    let new_log = serde_json::to_string(&log).unwrap_or_default();

    // Write back
    // SAFETY: UPDATE with bounded column list — runtime query.
    sqlx::query(
        "UPDATE works SET inspiration_log = ?, updated_at = ? WHERE work_id = ? AND creator_id = ?",
    )
    .bind(&new_log)
    .bind(now)
    .bind(work_id)
    .bind(creator_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Return updated record (derive from current + new log)
    let mut updated = current;
    updated.inspiration_log = new_log;
    updated.updated_at = now.to_string();
    Ok(updated)
}

// ── V1.34 FL-E stage functions ──────────────────────────────────────────────

/// Update the FL-E stage and status on a Work (V1.34).
///
/// Returns the updated `WorkRecord`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or no work is found.
pub async fn update_work_stage(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    current_stage: &str,
    stage_status: &str,
    now: &str,
) -> Result<WorkRecord, LocalDbError> {
    // SAFETY: UPDATE with bounded column list — runtime query.
    sqlx::query(
        "UPDATE works SET current_stage = ?, stage_status = ?, updated_at = ?
         WHERE work_id = ? AND creator_id = ?",
    )
    .bind(current_stage)
    .bind(stage_status)
    .bind(now)
    .bind(work_id)
    .bind(creator_id)
    .execute(pool)
    .await?;

    get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("works/{work_id}"),
        })
}

/// Get the FL-E stage info for a Work (V1.34).
///
/// Returns `(current_stage, stage_status)` or `None` if the work doesn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_work_stage(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Option<(String, String)>, LocalDbError> {
    // SAFETY: SELECT against works table — runtime query.
    let row = sqlx::query(
        "SELECT current_stage, stage_status FROM works WHERE work_id = ? AND creator_id = ?",
    )
    .bind(work_id)
    .bind(creator_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.get("current_stage"), r.get("stage_status"))))
}

/// Check whether a Work has an active FL-E stage schedule (V1.34 spec §2 invariant #4).
///
/// An "active FL-E stage schedule" is detected by checking if the work's
/// `stage_status` is `active`. This is a lightweight check that avoids querying
/// the `creator_schedules` table directly, since the stage advance flow sets
/// `stage_status = 'active'` only when creating a stage schedule.
///
/// Returns `true` if the work has an active stage, `false` otherwise.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn has_active_fl_e_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<bool, LocalDbError> {
    // SAFETY: SELECT against works table — runtime query.
    let row = sqlx::query("SELECT stage_status FROM works WHERE work_id = ? AND creator_id = ?")
        .bind(work_id)
        .bind(creator_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.is_some_and(|r: sqlx::sqlite::SqliteRow| {
        let status: String = r.get("stage_status");
        status == "active"
    }))
}

/// Atomically advance a Work's FL-E stage (V1.34, R-FL-E-07 TOCTOU fix).
///
/// Wraps the read-check-update sequence in a single `BEGIN IMMEDIATE` / `COMMIT`
/// transaction so that concurrent `stage advance` calls cannot race past each
/// other (spec §2 invariant #4, spec §3.3).
///
/// # Errors
///
/// Returns `LocalDbError` if:
/// - The work does not exist
/// - An active FL-E schedule already exists for this work
/// - The database query fails
pub async fn advance_work_stage_atomic(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    target_stage: &str,
    target_status: &str,
    now: &str,
) -> Result<WorkRecord, LocalDbError> {
    let mut tx: Transaction<'_, Sqlite> = pool.begin().await?;

    // Step 1: SELECT current state inside transaction
    // SAFETY: SELECT against works table — runtime query.
    let current: Option<WorkRecord> = sqlx::query(&format!(
        "SELECT {WORKS_COLUMNS} FROM works WHERE work_id = ? AND creator_id = ?"
    ))
    .bind(work_id)
    .bind(creator_id)
    .fetch_optional(&mut *tx)
    .await?
    .as_ref()
    .map(row_to_work_record);

    let current = current.ok_or_else(|| LocalDbError::MissingVersionKey {
        key: format!("works/{work_id}"),
    })?;

    // Step 2: Check no active FL-E schedule exists (TOCTOU-safe)
    if current.stage_status == "active" && target_status == "active" {
        return Err(LocalDbError::ConstraintViolation {
            table: "works".to_string(),
            constraint: format!(
                "active FL-E stage schedule already exists for work {work_id} \
                 (stage: {}, status: {})",
                current.current_stage, current.stage_status
            ),
        });
    }

    // Step 3: UPDATE within the same transaction
    // SAFETY: UPDATE against works table — runtime query.
    sqlx::query(
        "UPDATE works SET current_stage = ?, stage_status = ?, updated_at = ?
         WHERE work_id = ? AND creator_id = ?",
    )
    .bind(target_stage)
    .bind(target_status)
    .bind(now)
    .bind(work_id)
    .bind(creator_id)
    .execute(&mut *tx)
    .await?;

    // Step 4: COMMIT
    tx.commit().await?;

    // Fetch the updated record for return
    get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("works/{work_id}"),
        })
}

/// Ordered list of FL-E stages — re-exported from `nexus_contracts` (single source of truth).
pub use nexus_contracts::local::orchestration::FL_E_STAGES;

/// Returns the index of a stage in the FL-E linear order — re-exported from `nexus_contracts`.
pub use nexus_contracts::local::orchestration::stage_index;

/// Shared test helper — creates a minimal [`WorkRecord`] for unit tests.
#[cfg(test)]
#[must_use]
pub(crate) fn sample_work_for_test(work_id: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "default".to_string(),
        status: "draft".to_string(),
        title: "My Novel".to_string(),
        long_term_goal: "Write a great novel".to_string(),
        initial_idea: "A sci-fi thriller".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-04T10:00:00Z".to_string(),
        updated_at: "2026-06-04T10:00:00Z".to_string(),
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
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn sample_work(work_id: &str) -> WorkRecord {
        super::sample_work_for_test(work_id)
    }

    #[tokio::test]
    async fn test_create_and_get_work() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();

        let fetched = get_work(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.work_id, "wrk_001");
        assert_eq!(fetched.creator_id, "ctr_test");
        assert_eq!(fetched.status, "draft");
        assert_eq!(fetched.title, "My Novel");
        assert!(fetched.creative_brief.is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_work(&pool, "ctr_test", "wrk_ghost")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_list_works_by_creator() {
        let (pool, _dir) = fresh_pool().await;

        let record1 = sample_work("wrk_001");
        let mut record2 = sample_work("wrk_002");
        record2.creator_id = "ctr_other".to_string();
        let mut record3 = sample_work("wrk_003");
        record3.updated_at = "2026-06-04T12:00:00Z".to_string();

        create_work(&pool, &record1).await.unwrap();
        create_work(&pool, &record2).await.unwrap();
        create_work(&pool, &record3).await.unwrap();

        let list = list_works(&pool, "ctr_test", "default", &WorkListFilters::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        // Ordered by updated_at DESC
        assert_eq!(list[0].work_id, "wrk_003");
        assert_eq!(list[1].work_id, "wrk_001");
    }

    #[tokio::test]
    async fn test_list_works_with_status_filter() {
        let (pool, _dir) = fresh_pool().await;

        let record1 = sample_work("wrk_001");
        let mut record2 = sample_work("wrk_002");
        record2.status = "active".to_string();

        create_work(&pool, &record1).await.unwrap();
        create_work(&pool, &record2).await.unwrap();

        let filters = WorkListFilters {
            status: Some("active".to_string()),
            ..Default::default()
        };
        let list = list_works(&pool, "ctr_test", "default", &filters)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].work_id, "wrk_002");
    }

    #[tokio::test]
    async fn test_patch_work() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();

        let patch = WorkPatch {
            title: Some("Updated Title".to_string()),
            status: Some("active".to_string()),
            ..Default::default()
        };
        let updated = patch_work(&pool, "ctr_test", "wrk_001", &patch, "2026-06-04T11:00:00Z")
            .await
            .unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.status, "active");
        assert_eq!(updated.updated_at, "2026-06-04T11:00:00Z");
    }

    #[tokio::test]
    async fn test_append_inspiration() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();

        let entry = InspirationLogEntry {
            at: "2026-06-04T12:00:00Z".to_string(),
            note: "New direction".to_string(),
        };
        let entry_json = serde_json::to_string(&entry).unwrap();

        let updated = append_inspiration(
            &pool,
            "ctr_test",
            "wrk_001",
            &entry_json,
            "2026-06-04T12:00:00Z",
        )
        .await
        .unwrap();

        let log: Vec<InspirationLogEntry> = serde_json::from_str(&updated.inspiration_log).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].note, "New direction");
    }

    #[tokio::test]
    async fn test_append_multiple_inspirations() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();

        let entry1 = InspirationLogEntry {
            at: "2026-06-04T12:00:00Z".to_string(),
            note: "First".to_string(),
        };
        let entry2 = InspirationLogEntry {
            at: "2026-06-04T13:00:00Z".to_string(),
            note: "Second".to_string(),
        };

        append_inspiration(
            &pool,
            "ctr_test",
            "wrk_001",
            &serde_json::to_string(&entry1).unwrap(),
            "2026-06-04T12:00:00Z",
        )
        .await
        .unwrap();
        append_inspiration(
            &pool,
            "ctr_test",
            "wrk_001",
            &serde_json::to_string(&entry2).unwrap(),
            "2026-06-04T13:00:00Z",
        )
        .await
        .unwrap();

        let fetched = get_work(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap()
            .unwrap();
        let log: Vec<InspirationLogEntry> = serde_json::from_str(&fetched.inspiration_log).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].note, "First");
        assert_eq!(log[1].note, "Second");
    }

    #[tokio::test]
    async fn test_idempotency_create_and_lookup() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();
        record_idempotency(
            &pool,
            "ctr_test",
            "req_abc",
            "wrk_001",
            "2026-06-04T10:00:00Z",
        )
        .await
        .unwrap();

        let found = find_work_by_client_request_id(&pool, "ctr_test", "req_abc")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.work_id, "wrk_001");

        assert!(
            find_work_by_client_request_id(&pool, "ctr_test", "req_missing")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_patch_work_clears_nullable_field() {
        let (pool, _dir) = fresh_pool().await;
        let mut record = sample_work("wrk_001");
        record.world_id = Some("wld_test".to_string());
        create_work(&pool, &record).await.unwrap();

        let patch = WorkPatch {
            world_id: Some(None),
            ..Default::default()
        };
        let updated = patch_work(&pool, "ctr_test", "wrk_001", &patch, "2026-06-04T11:00:00Z")
            .await
            .unwrap();
        assert!(updated.world_id.is_none());
    }

    // ── V1.34 FL-E stage tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_new_work_has_default_stage() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_stage_001");
        create_work(&pool, &record).await.unwrap();

        let fetched = get_work(&pool, "ctr_test", "wrk_stage_001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.current_stage, "intake");
        assert_eq!(fetched.stage_status, "pending");
    }

    #[tokio::test]
    async fn test_get_work_stage() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_stage_002");
        create_work(&pool, &record).await.unwrap();

        let stage = get_work_stage(&pool, "ctr_test", "wrk_stage_002")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stage.0, "intake");
        assert_eq!(stage.1, "pending");
    }

    #[tokio::test]
    async fn test_get_work_stage_nonexistent() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_work_stage(&pool, "ctr_test", "wrk_ghost")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_update_work_stage() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_stage_003");
        create_work(&pool, &record).await.unwrap();

        let updated = update_work_stage(
            &pool,
            "ctr_test",
            "wrk_stage_003",
            "research",
            "active",
            "2026-06-05T10:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(updated.current_stage, "research");
        assert_eq!(updated.stage_status, "active");
    }

    #[tokio::test]
    async fn test_update_work_stage_nonexistent_fails() {
        let (pool, _dir) = fresh_pool().await;
        let result = update_work_stage(
            &pool,
            "ctr_test",
            "wrk_ghost",
            "research",
            "active",
            "2026-06-05T10:00:00Z",
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_stage_index_known_stages() {
        assert_eq!(stage_index("intake"), Some(0));
        assert_eq!(stage_index("research"), Some(1));
        assert_eq!(stage_index("produce"), Some(2));
        assert_eq!(stage_index("review"), Some(3));
        assert_eq!(stage_index("persist"), Some(4));
    }

    #[test]
    fn test_stage_index_unknown_returns_none() {
        assert_eq!(stage_index("unknown"), None);
        assert_eq!(stage_index("INTAKE"), None);
    }

    #[test]
    fn test_strict_linear_advance_no_skip_without_force() {
        // R-FL-E-03: validate that stage_index enforces linear order.
        // intake (0) → research (1) is valid; intake → produce (2) is a skip.
        let intake_idx = stage_index("intake").unwrap();
        let research_idx = stage_index("research").unwrap();
        let produce_idx = stage_index("produce").unwrap();

        // Valid advance: intake → research (adjacent)
        assert_eq!(research_idx, intake_idx + 1);

        // Invalid skip: intake → produce (not adjacent)
        assert_ne!(produce_idx, intake_idx + 1);
        assert!(produce_idx > intake_idx + 1);
    }

    #[tokio::test]
    async fn test_has_active_fl_e_schedule_false_for_new_work() {
        // R-FL-E-01: new work has stage_status='pending', not active
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_active_001");
        create_work(&pool, &record).await.unwrap();

        let has_active = has_active_fl_e_schedule(&pool, "ctr_test", "wrk_active_001")
            .await
            .unwrap();
        assert!(!has_active, "new work should not have active schedule");
    }

    #[tokio::test]
    async fn test_has_active_fl_e_schedule_true_after_advance() {
        // R-FL-E-01: after stage advance (stage_status='active'), should report active
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_active_002");
        create_work(&pool, &record).await.unwrap();

        // Simulate advance: set stage_status='active'
        update_work_stage(
            &pool,
            "ctr_test",
            "wrk_active_002",
            "research",
            "active",
            "2026-06-05T10:00:00Z",
        )
        .await
        .unwrap();

        let has_active = has_active_fl_e_schedule(&pool, "ctr_test", "wrk_active_002")
            .await
            .unwrap();
        assert!(
            has_active,
            "work with stage_status=active should report active"
        );
    }

    #[tokio::test]
    async fn test_reject_double_active_schedule() {
        // R-FL-E-01 regression: advancing from an active stage to another should
        // fail at the CLI level. This test validates the DB helper returns the
        // correct state for the CLI to check.
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_active_003");
        create_work(&pool, &record).await.unwrap();

        // Advance to research (active)
        update_work_stage(
            &pool,
            "ctr_test",
            "wrk_active_003",
            "research",
            "active",
            "2026-06-05T10:00:00Z",
        )
        .await
        .unwrap();

        // The DB helper should report active — CLI would check this before
        // allowing another advance
        let has_active = has_active_fl_e_schedule(&pool, "ctr_test", "wrk_active_003")
            .await
            .unwrap();
        assert!(has_active, "should detect existing active schedule");

        // Complete the stage
        update_work_stage(
            &pool,
            "ctr_test",
            "wrk_active_003",
            "research",
            "complete",
            "2026-06-05T12:00:00Z",
        )
        .await
        .unwrap();

        let has_active_after_complete =
            has_active_fl_e_schedule(&pool, "ctr_test", "wrk_active_003")
                .await
                .unwrap();
        assert!(
            !has_active_after_complete,
            "completed stage should not be active"
        );
    }

    #[tokio::test]
    async fn test_patch_work_stage_fields() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_stage_004");
        create_work(&pool, &record).await.unwrap();

        let patch = WorkPatch {
            current_stage: Some("produce".to_string()),
            stage_status: Some("active".to_string()),
            ..Default::default()
        };
        let updated = patch_work(
            &pool,
            "ctr_test",
            "wrk_stage_004",
            &patch,
            "2026-06-05T11:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(updated.current_stage, "produce");
        assert_eq!(updated.stage_status, "active");
    }

    /// Fix E (W-E): Verify the partial index for auto-chain boot resume exists
    /// after migration. The index covers (auto_chain_enabled, auto_chain_interrupted,
    /// status) with a partial WHERE clause for auto_chain_enabled = 1.
    #[tokio::test]
    async fn test_auto_chain_resume_index_exists() {
        let (pool, _dir) = fresh_pool().await;

        // Query sqlite_master to verify the index was created by the migration.
        let index_sql: Option<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'index' AND name = 'works_auto_chain_resume'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();

        assert!(
            index_sql.is_some(),
            "Fix E: works_auto_chain_resume index should exist after migration"
        );

        let sql = index_sql.unwrap();
        assert!(
            sql.contains("auto_chain_enabled"),
            "index should cover auto_chain_enabled: {sql}"
        );
        assert!(
            sql.contains("auto_chain_interrupted"),
            "index should cover auto_chain_interrupted: {sql}"
        );
        assert!(sql.contains("status"), "index should cover status: {sql}");
    }
}
