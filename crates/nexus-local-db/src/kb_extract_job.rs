//! KB Extract job queue — SQLite-backed persistence.
//!
//! Each job represents a request to extract a work-scope KB entry into a
//! world-scoped `KeyBlock` via the `kb.extract_work` capability.
//!
//! Lifecycle: `queued` → `running` → `done` | `failed`.
//! SSOT in `nexus-local-db`; no second in-memory queue.

use sqlx::SqlitePool;

/// Row from `kb_extract_jobs`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KbExtractJob {
    /// Unique job ID (`xj_` prefix).
    pub job_id: String,
    /// Owning creator ID.
    pub creator_id: String,
    /// Workspace ID the work entry belongs to.
    pub workspace_id: String,
    /// Work-scope KB entry ID to extract from (V1.29 legacy; still used for idempotency).
    pub work_entry_id: String,
    /// Target world ID for the resulting `KeyBlock`.
    pub world_id: String,
    /// Job status: `queued`, `running`, `done`, `failed`.
    pub status: String,
    /// Error text (set when status is `failed`).
    pub error_text: Option<String>,
    /// When the job was created.
    pub created_at: String,
    /// When the job started running.
    pub started_at: Option<String>,
    /// When the job finished (done or failed).
    pub finished_at: Option<String>,
    /// V1.40 P3: artifact type discriminator (`work_chapter`, `work_section`, etc.).
    pub source_kind: Option<String>,
    /// V1.40 P3: artifact locator (relative path, artifact ID, or reference ID).
    pub source_locator: Option<String>,
    /// V1.40 P3: profile hint for extract prompt (`novel`, `screenplay`, `essay`, `generic`).
    pub profile_hint: Option<String>,
    /// V1.40 P3: work ID for the source work (chapter's parent).
    pub work_id: Option<String>,
}

/// Generate a unique job ID: `xj_` + `UUIDv4` hex string.
///
/// Uses the `uuid` crate for proper `UUIDv4` generation with `xj_` prefix.
/// Collision probability is negligible but handled by the caller via single retry.
fn generate_job_id() -> String {
    format!("xj_{}", uuid::Uuid::new_v4().simple())
}

/// Column list shared across all SELECT queries (avoids drift).
const JOB_COLUMNS: &str = r#"
    job_id as "job_id!",
    creator_id as "creator_id!",
    workspace_id as "workspace_id!",
    work_entry_id as "work_entry_id!",
    world_id as "world_id!",
    status as "status!",
    error_text,
    created_at as "created_at!",
    started_at,
    finished_at,
    source_kind,
    source_locator,
    profile_hint,
    work_id
"#;

/// Fetch a single job by ID using the shared column list.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
async fn fetch_by_id(
    pool: &SqlitePool,
    job_id: &str,
) -> Result<KbExtractJob, sqlx::Error> {
    let query = format!("SELECT {JOB_COLUMNS} FROM kb_extract_jobs WHERE job_id = ?");
    // SAFETY: `JOB_COLUMNS` is a compile-time constant; `job_id` is a bind param.
    sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(job_id)
        .fetch_one(pool)
        .await
}

/// Fetch a single optional job by ID using the shared column list.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
async fn fetch_optional_by_id(
    pool: &SqlitePool,
    job_id: &str,
) -> Result<Option<KbExtractJob>, sqlx::Error> {
    let query = format!("SELECT {JOB_COLUMNS} FROM kb_extract_jobs WHERE job_id = ?");
    // SAFETY: `JOB_COLUMNS` is a compile-time constant; `job_id` is a bind param.
    sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(job_id)
        .fetch_optional(pool)
        .await
}

/// Insert a new job row, retrying once on PRIMARY KEY collision (R18).
///
/// `UUIDv4` collision is astronomically unlikely; this guard is defensive only.
async fn insert_with_retry(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_id: &str,
    work_entry_id: &str,
    world_id: &str,
    source_kind: Option<&str>,
    source_locator: Option<&str>,
    profile_hint: Option<&str>,
    work_id: Option<&str>,
) -> Result<KbExtractJob, sqlx::Error> {
    for _ in 0..2 {
        let job_id = generate_job_id();
        // SAFETY: static INSERT with bind params; no user-controlled identifiers.
        let result = sqlx::query(
            "INSERT INTO kb_extract_jobs \
             (job_id, creator_id, workspace_id, work_entry_id, world_id, status, \
              source_kind, source_locator, profile_hint, work_id) \
             VALUES (?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?)",
        )
        .bind(&job_id)
        .bind(creator_id)
        .bind(workspace_id)
        .bind(work_entry_id)
        .bind(world_id)
        .bind(source_kind)
        .bind(source_locator)
        .bind(profile_hint)
        .bind(work_id)
        .execute(pool)
        .await;

        match result {
            Ok(_) => {
                return fetch_by_id(pool, &job_id).await;
            }
            Err(sqlx::Error::Database(ref db_err)) if db_err.code().as_deref() == Some("1555") => {
                // SQLite UNIQUE constraint violation (code 1555) — retry with new UUID
            }
            Err(e) => return Err(e),
        }
    }
    // Should never reach here with UUIDv4
    Err(sqlx::Error::Configuration(
        "UNIQUE constraint violation after retry — impossible with UUIDv4".into(),
    ))
}

/// Enqueue a new extract job. Idempotent: if a non-failed job already exists
/// for the same `(creator_id, work_entry_id, world_id)`, returns the existing job.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn enqueue(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_id: &str,
    work_entry_id: &str,
    world_id: &str,
) -> Result<KbExtractJob, sqlx::Error> {
    // Check for existing non-failed job (idempotency).
    let query = format!(
        "SELECT {JOB_COLUMNS} FROM kb_extract_jobs \
         WHERE creator_id = ? AND work_entry_id = ? AND world_id = ? AND status != 'failed'"
    );
    // SAFETY: JOB_COLUMNS constant; bind params.
    let existing = sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(creator_id)
        .bind(work_entry_id)
        .bind(world_id)
        .fetch_optional(pool)
        .await?;

    if let Some(job) = existing {
        return Ok(job);
    }

    // Insert new job with retry on PRIMARY KEY collision.
    insert_with_retry(
        pool, creator_id, workspace_id, work_entry_id, world_id,
        None, None, None, None,
    )
    .await
}

/// Enqueue a new extract job with artifact locator fields (V1.40 P3).
///
/// Idempotent: if a non-failed job already exists for the same
/// `(creator_id, work_entry_id, world_id)`, returns the existing job.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub fn enqueue_with_artifact<'a>(
    pool: &'a SqlitePool,
    creator_id: &'a str,
    workspace_id: &'a str,
    work_entry_id: &'a str,
    world_id: &'a str,
    source_kind: Option<&'a str>,
    source_locator: Option<&'a str>,
    profile_hint: Option<&'a str>,
    work_id: Option<&'a str>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<KbExtractJob, sqlx::Error>> + 'a>> {
    Box::pin(async move {
        // Check for existing non-failed job (idempotency).
        let query = format!(
            "SELECT {JOB_COLUMNS} FROM kb_extract_jobs \
             WHERE creator_id = ? AND work_entry_id = ? AND world_id = ? AND status != 'failed'"
        );
        let existing = sqlx::query_as::<_, KbExtractJob>(&query)
            .bind(creator_id)
            .bind(work_entry_id)
            .bind(world_id)
            .fetch_optional(pool)
            .await?;

        if let Some(job) = existing {
            return Ok(job);
        }

        insert_with_retry(
            pool, creator_id, workspace_id, work_entry_id, world_id,
            source_kind, source_locator, profile_hint, work_id,
        )
        .await
    })
}

/// Get a specific job by ID.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn get(pool: &SqlitePool, job_id: &str) -> Result<Option<KbExtractJob>, sqlx::Error> {
    fetch_optional_by_id(pool, job_id).await
}

/// List jobs for a given creator, bounded by `limit` (R20).
///
/// Returns at most `limit` jobs ordered by creation date (newest first).
/// Use a reasonable default (e.g. 100) to avoid unbounded result sets.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn list_by_creator(
    pool: &SqlitePool,
    creator_id: &str,
    limit: u32,
) -> Result<Vec<KbExtractJob>, sqlx::Error> {
    let query = format!(
        "SELECT {JOB_COLUMNS} FROM kb_extract_jobs \
         WHERE creator_id = ? ORDER BY created_at DESC LIMIT ?"
    );
    // SAFETY: JOB_COLUMNS constant; bind params.
    sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(creator_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Fetch the next queued job (oldest first) for a given creator.
///
/// Returns `None` if no queued jobs exist.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn next_queued(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<KbExtractJob>, sqlx::Error> {
    let query = format!(
        "SELECT {JOB_COLUMNS} FROM kb_extract_jobs \
         WHERE creator_id = ? AND status = 'queued' ORDER BY created_at ASC LIMIT 1"
    );
    // SAFETY: JOB_COLUMNS constant; bind params.
    sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(creator_id)
        .fetch_optional(pool)
        .await
}

/// Mark a job as running. Sets `started_at` to now.
///
/// TD-V130-06: Only transitions from `queued` status. If the job is not in
/// `queued` status (e.g. already `running`, `done`, or `failed`), this is a
/// no-op. This prevents a race where a completed/done job gets marked running
/// by a stale `mark_running` call.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn mark_running(pool: &SqlitePool, job_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE kb_extract_jobs
           SET status = 'running', started_at = datetime('now')
           WHERE job_id = ? AND status = 'queued'"#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically claim the oldest queued job for a given creator (R15).
///
/// Performs SELECT + UPDATE in a single `SQLite` transaction to prevent
/// concurrent workers from double-claiming the same job.
///
/// Returns `Some(job)` if a queued job was found and claimed, `None` otherwise.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn claim_job(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<KbExtractJob>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Select oldest queued job for this creator.
    let query = format!(
        "SELECT {JOB_COLUMNS} FROM kb_extract_jobs \
         WHERE creator_id = ? AND status = 'queued' ORDER BY created_at ASC LIMIT 1"
    );
    let job = sqlx::query_as::<_, KbExtractJob>(&query)
        .bind(creator_id)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(job) = job else {
        tx.rollback().await?;
        return Ok(None);
    };

    // Atomically mark as running within the same transaction.
    let result = sqlx::query!(
        r#"UPDATE kb_extract_jobs
           SET status = 'running', started_at = datetime('now')
           WHERE job_id = ? AND status = 'queued'"#,
        job.job_id,
    )
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(None);
    }

    tx.commit().await?;

    // Re-fetch to get the updated started_at timestamp.
    let claimed = fetch_by_id(pool, &job.job_id).await?;
    Ok(Some(claimed))
}

/// Mark a job as done. Sets `finished_at` to now.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn mark_done(pool: &SqlitePool, job_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE kb_extract_jobs
           SET status = 'done', finished_at = datetime('now')
           WHERE job_id = ?"#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a job as failed. Sets `finished_at` to now and records error text.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn mark_failed(
    pool: &SqlitePool,
    job_id: &str,
    error_text: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE kb_extract_jobs
           SET status = 'failed', error_text = ?, finished_at = datetime('now')
           WHERE job_id = ?"#,
        error_text,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn test_enqueue_and_get() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc123", "wld_1")
            .await
            .unwrap();
        assert!(job.job_id.starts_with("xj_"));
        assert_eq!(job.status, "queued");
        assert_eq!(job.creator_id, "ctr_1");
        // UUID format: xj_ + 32 hex chars
        let uuid_part = &job.job_id[3..];
        assert_eq!(uuid_part.len(), 32);
        assert!(uuid_part.chars().all(|c| c.is_ascii_hexdigit()));

        let fetched = get(&pool, &job.job_id).await.unwrap().unwrap();
        assert_eq!(fetched.work_entry_id, "kb_abc123");
    }

    #[tokio::test]
    async fn test_enqueue_idempotent() {
        let (pool, _dir) = fresh_pool().await;
        let job1 = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();
        let job2 = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();
        assert_eq!(job1.job_id, job2.job_id);
    }

    #[tokio::test]
    async fn test_lifecycle_queued_running_done() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();

        mark_running(&pool, &job.job_id).await.unwrap();
        let j = get(&pool, &job.job_id).await.unwrap().unwrap();
        assert_eq!(j.status, "running");
        assert!(j.started_at.is_some());

        mark_done(&pool, &j.job_id).await.unwrap();
        let j = get(&pool, &job.job_id).await.unwrap().unwrap();
        assert_eq!(j.status, "done");
        assert!(j.finished_at.is_some());
    }

    #[tokio::test]
    async fn test_lifecycle_queued_running_failed() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();

        mark_running(&pool, &job.job_id).await.unwrap();
        mark_failed(&pool, &job.job_id, "LLM returned invalid JSON")
            .await
            .unwrap();

        let j = get(&pool, &job.job_id).await.unwrap().unwrap();
        assert_eq!(j.status, "failed");
        assert_eq!(j.error_text.as_deref(), Some("LLM returned invalid JSON"));
    }

    #[tokio::test]
    async fn test_failed_allows_re_enqueue() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();
        mark_running(&pool, &job.job_id).await.unwrap();
        mark_failed(&pool, &job.job_id, "error").await.unwrap();

        // After failure, re-enqueue should create a new job.
        let job2 = enqueue(&pool, "ctr_1", "wrk_1", "kb_abc", "wld_1")
            .await
            .unwrap();
        assert_ne!(job.job_id, job2.job_id);
    }

    #[tokio::test]
    async fn test_list_by_creator() {
        let (pool, _dir) = fresh_pool().await;
        enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();
        enqueue(&pool, "ctr_1", "wrk_1", "kb_b", "wld_1")
            .await
            .unwrap();
        enqueue(&pool, "ctr_2", "wrk_1", "kb_c", "wld_1")
            .await
            .unwrap();

        let jobs = list_by_creator(&pool, "ctr_1", 100).await.unwrap();
        assert_eq!(jobs.len(), 2);

        let jobs = list_by_creator(&pool, "ctr_2", 100).await.unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_creator_bounded() {
        let (pool, _dir) = fresh_pool().await;
        for i in 0..5 {
            enqueue(&pool, "ctr_1", "wrk_1", &format!("kb_{i}"), "wld_1")
                .await
                .unwrap();
        }

        // Limit of 3 should return only 3
        let jobs = list_by_creator(&pool, "ctr_1", 3).await.unwrap();
        assert_eq!(jobs.len(), 3);

        // Limit of 100 returns all
        let jobs = list_by_creator(&pool, "ctr_1", 100).await.unwrap();
        assert_eq!(jobs.len(), 5);
    }

    #[tokio::test]
    async fn test_next_queued() {
        let (pool, _dir) = fresh_pool().await;
        assert!(next_queued(&pool, "ctr_1").await.unwrap().is_none());

        let j1 = enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();
        let _j2 = enqueue(&pool, "ctr_1", "wrk_1", "kb_b", "wld_1")
            .await
            .unwrap();

        let next = next_queued(&pool, "ctr_1").await.unwrap().unwrap();
        assert_eq!(next.job_id, j1.job_id);

        mark_running(&pool, &j1.job_id).await.unwrap();
        let next = next_queued(&pool, "ctr_1").await.unwrap().unwrap();
        assert!(next.job_id.starts_with("xj_"));
        assert_ne!(next.job_id, j1.job_id);
    }

    // ── K1: Atomic claim_job tests ──────────────────────────────────

    #[tokio::test]
    async fn test_claim_job_selects_oldest_queued() {
        let (pool, _dir) = fresh_pool().await;
        let j1 = enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();
        let _j2 = enqueue(&pool, "ctr_1", "wrk_1", "kb_b", "wld_1")
            .await
            .unwrap();

        let claimed = claim_job(&pool, "ctr_1").await.unwrap().unwrap();
        assert_eq!(claimed.job_id, j1.job_id);
        assert_eq!(claimed.status, "running");
        assert!(claimed.started_at.is_some());
    }

    #[tokio::test]
    async fn test_claim_job_returns_none_when_empty() {
        let (pool, _dir) = fresh_pool().await;
        assert!(claim_job(&pool, "ctr_1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_claim_job_skips_non_queued() {
        let (pool, _dir) = fresh_pool().await;
        let j1 = enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();
        mark_running(&pool, &j1.job_id).await.unwrap();

        // Only running jobs — nothing to claim
        assert!(claim_job(&pool, "ctr_1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_claim_job_concurrent_double_claim_prevented() {
        let (pool, _dir) = fresh_pool().await;
        // Enqueue a single job
        let _j = enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();

        // Two concurrent claimers — SQLite may return BUSY for one of them,
        // which we treat as "did not claim".
        let pool1 = pool.clone();
        let pool2 = pool.clone();
        let h1 = tokio::spawn(async move { claim_job(&pool1, "ctr_1").await });
        let h2 = tokio::spawn(async move { claim_job(&pool2, "ctr_1").await });

        let r1 = h1.await.unwrap().ok().flatten();
        let r2 = h2.await.unwrap().ok().flatten();

        // Exactly one should succeed (the other gets SQLITE_BUSY → Err, or
        // finds no queued row → None).
        let claimed_count = r1.is_some() as usize + r2.is_some() as usize;
        assert!(
            claimed_count == 1,
            "expected exactly one claim to succeed, got {claimed_count}"
        );
    }

    #[tokio::test]
    async fn test_claim_job_then_full_lifecycle() {
        let (pool, _dir) = fresh_pool().await;
        let j = enqueue(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
            .await
            .unwrap();

        let claimed = claim_job(&pool, "ctr_1").await.unwrap().unwrap();
        assert_eq!(claimed.job_id, j.job_id);
        assert_eq!(claimed.status, "running");

        mark_done(&pool, &claimed.job_id).await.unwrap();
        let done = get(&pool, &claimed.job_id).await.unwrap().unwrap();
        assert_eq!(done.status, "done");
    }

    // ── V1.40 P3: Artifact locator tests ────────────────────────────

    #[tokio::test]
    async fn test_enqueue_with_artifact_fields() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue_with_artifact(
            &pool,
            "ctr_1",
            "wrk_1",
            "kb_chapter_03",
            "wld_1",
            Some("work_chapter"),
            Some("Works/my-novel/Chapters/03.md"),
            Some("novel"),
            Some("wrk_novel_abc"),
        )
        .await
        .unwrap();

        assert!(job.job_id.starts_with("xj_"));
        assert_eq!(job.source_kind.as_deref(), Some("work_chapter"));
        assert_eq!(
            job.source_locator.as_deref(),
            Some("Works/my-novel/Chapters/03.md")
        );
        assert_eq!(job.profile_hint.as_deref(), Some("novel"));
        assert_eq!(job.work_id.as_deref(), Some("wrk_novel_abc"));
    }

    #[tokio::test]
    async fn test_enqueue_with_artifact_idempotent() {
        let (pool, _dir) = fresh_pool().await;
        let job1 = enqueue_with_artifact(
            &pool,
            "ctr_1",
            "wrk_1",
            "kb_chapter_03",
            "wld_1",
            Some("work_chapter"),
            Some("Works/novel/Chapters/03.md"),
            Some("novel"),
            Some("wrk_abc"),
        )
        .await
        .unwrap();

        // Same work_entry_id + world_id → idempotent return
        let job2 = enqueue_with_artifact(
            &pool,
            "ctr_1",
            "wrk_1",
            "kb_chapter_03",
            "wld_1",
            Some("work_chapter"),
            Some("Works/novel/Chapters/03.md"),
            Some("novel"),
            Some("wrk_abc"),
        )
        .await
        .unwrap();

        assert_eq!(job1.job_id, job2.job_id);
    }

    #[tokio::test]
    async fn test_enqueue_without_artifact_has_null_fields() {
        let (pool, _dir) = fresh_pool().await;
        let job = enqueue(&pool, "ctr_1", "wrk_1", "kb_legacy", "wld_1")
            .await
            .unwrap();
        assert!(job.source_kind.is_none());
        assert!(job.source_locator.is_none());
        assert!(job.profile_hint.is_none());
        assert!(job.work_id.is_none());
    }
}
