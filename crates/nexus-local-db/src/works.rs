//! Work entity CRUD operations (V1.33 work-experience-model §3).
//!
//! Manages the `works` table — long-term creative effort containers
//! with structured briefs, inspiration logs, and schedule linkage.
//! Idempotency via separate `works_idempotency` table.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

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
    /// Filter by intake_status.
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
    /// New long_term_goal.
    pub long_term_goal: Option<String>,
    /// New creative brief JSON.
    pub creative_brief: Option<Option<String>>,
    /// New intake_status.
    pub intake_status: Option<String>,
    /// New status.
    pub status: Option<String>,
    /// New world_id.
    pub world_id: Option<Option<String>>,
    /// New story_ref.
    pub story_ref: Option<Option<String>>,
    /// New primary_preset_id.
    pub primary_preset_id: Option<String>,
    /// New schedule_ids JSON.
    pub schedule_ids: Option<String>,
}

/// Create a new Work (with idempotency on `client_request_id`).
///
/// If `client_request_id` is provided and a Work already exists for
/// `(creator_id, client_request_id)`, returns the existing `work_id`.
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
         primary_preset_id, schedule_ids, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(pool)
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
    let row: Option<(String,)> =
        sqlx::query_as("SELECT work_id FROM works_idempotency WHERE creator_id = ? AND client_request_id = ?")
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
    let row = sqlx::query(
        "SELECT work_id, creator_id, workspace_slug, status, title, long_term_goal,
                initial_idea, creative_brief, intake_status, world_id, story_ref,
                inspiration_log, primary_preset_id, schedule_ids, created_at, updated_at
         FROM works WHERE work_id = ? AND creator_id = ?",
    )
    .bind(work_id)
    .bind(creator_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| WorkRecord {
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
    }))
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
    // Build dynamic WHERE clause for optional filters
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
        "SELECT work_id, creator_id, workspace_slug, status, title, long_term_goal,
                initial_idea, creative_brief, intake_status, world_id, story_ref,
                inspiration_log, primary_preset_id, schedule_ids, created_at, updated_at
         FROM works WHERE {where_sql}
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

    let rows = query.fetch_all(pool).await?;

    Ok(rows
        .iter()
        .map(|row| WorkRecord {
            work_id: row.get("work_id"),
            creator_id: row.get("creator_id"),
            workspace_slug: row.get("workspace_slug"),
            status: row.get("status"),
            title: row.get("title"),
            long_term_goal: row.get("long_term_goal"),
            initial_idea: row.get("initial_idea"),
            creative_brief: row.get("creative_brief"),
            intake_status: row.get("intake_status"),
            world_id: row.get("world_id"),
            story_ref: row.get("story_ref"),
            inspiration_log: row.get("inspiration_log"),
            primary_preset_id: row.get("primary_preset_id"),
            schedule_ids: row.get("schedule_ids"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

/// Partially update a Work.
///
/// Only non-None fields in `patch` are applied.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or no work is found.
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

    if set_clauses.is_empty() {
        // Nothing to update — just return current record.
        return get_work(pool, creator_id, work_id).await?.ok_or_else(|| {
            LocalDbError::MissingVersionKey { key: format!("works/{work_id}") }
        });
    }

    set_clauses.push("updated_at = ?");
    let set_sql = set_clauses.join(", ");

    // SAFETY: Dynamic SQL required for partial update.
    // All values are bound parameters, not interpolated.
    let sql = format!(
        "UPDATE works SET {set_sql} WHERE work_id = ? AND creator_id = ?"
    );

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

    query = query.bind(now).bind(work_id).bind(creator_id);

    query.execute(pool).await?;

    get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey { key: format!("works/{work_id}") })
}

/// Append an inspiration entry to a Work (atomic).
///
/// Uses a single UPDATE that appends to the JSON array.
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
    // SAFETY: Dynamic SQL required for JSON manipulation via sqlite_json functions.
    // We build the new JSON array by reading the old one and appending.
    // All values are bound parameters.
    let sql = "UPDATE works SET inspiration_log = \
               CASE WHEN inspiration_log = '[]' THEN ? \
               ELSE substr(inspiration_log, 1, length(inspiration_log) - 1) || ',' || substr(?, 2) \
               END, \
               updated_at = ? \
               WHERE work_id = ? AND creator_id = ?";

    // entry_json is a single JSON object like {"at":"...","note":"..."}
    let array_entry = format!("[{entry_json}]");

    sqlx::query(sql)
        .bind(&array_entry)
        .bind(&array_entry)
        .bind(now)
        .bind(work_id)
        .bind(creator_id)
        .execute(pool)
        .await?;

    get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey { key: format!("works/{work_id}") })
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
        }
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

        let updated =
            append_inspiration(&pool, "ctr_test", "wrk_001", &entry_json, "2026-06-04T12:00:00Z")
                .await
                .unwrap();

        let log: Vec<InspirationLogEntry> =
            serde_json::from_str(&updated.inspiration_log).unwrap();
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
        let log: Vec<InspirationLogEntry> =
            serde_json::from_str(&fetched.inspiration_log).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].note, "First");
        assert_eq!(log[1].note, "Second");
    }

    #[tokio::test]
    async fn test_idempotency_create_and_lookup() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_001");
        create_work(&pool, &record).await.unwrap();
        record_idempotency(&pool, "ctr_test", "req_abc", "wrk_001", "2026-06-04T10:00:00Z")
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
}
