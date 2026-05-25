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
    /// Work-scope KB entry ID to extract from.
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
}

/// Generate a unique job ID: `xj_` + 12 hex chars.
#[allow(clippy::cast_possible_truncation)]
fn generate_job_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = u64::from(now.subsec_nanos()) | (now.as_millis() as u64).wrapping_shl(20);
    format!("xj_{:012x}", millis & 0xFFFF_FFFF_FFFF)
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
    let existing = sqlx::query_as!(
        KbExtractJob,
        r#"SELECT
            job_id as "job_id!",
            creator_id as "creator_id!",
            workspace_id as "workspace_id!",
            work_entry_id as "work_entry_id!",
            world_id as "world_id!",
            status as "status!",
            error_text,
            created_at as "created_at!",
            started_at,
            finished_at
        FROM kb_extract_jobs
        WHERE creator_id = ?
          AND work_entry_id = ?
          AND world_id = ?
          AND status != 'failed'"#,
        creator_id,
        work_entry_id,
        world_id,
    )
    .fetch_optional(pool)
    .await?;

    if let Some(job) = existing {
        return Ok(job);
    }

    // Insert new job.
    let job_id = generate_job_id();
    sqlx::query!(
        r#"INSERT INTO kb_extract_jobs
            (job_id, creator_id, workspace_id, work_entry_id, world_id, status)
           VALUES (?, ?, ?, ?, ?, 'queued')"#,
        job_id,
        creator_id,
        workspace_id,
        work_entry_id,
        world_id,
    )
    .execute(pool)
    .await?;

    // Fetch back to return the full row.
    sqlx::query_as!(
        KbExtractJob,
        r#"SELECT
            job_id as "job_id!",
            creator_id as "creator_id!",
            workspace_id as "workspace_id!",
            work_entry_id as "work_entry_id!",
            world_id as "world_id!",
            status as "status!",
            error_text,
            created_at as "created_at!",
            started_at,
            finished_at
        FROM kb_extract_jobs
        WHERE job_id = ?"#,
        job_id,
    )
    .fetch_one(pool)
    .await
}

/// Get a specific job by ID.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn get(pool: &SqlitePool, job_id: &str) -> Result<Option<KbExtractJob>, sqlx::Error> {
    sqlx::query_as!(
        KbExtractJob,
        r#"SELECT
            job_id as "job_id!",
            creator_id as "creator_id!",
            workspace_id as "workspace_id!",
            work_entry_id as "work_entry_id!",
            world_id as "world_id!",
            status as "status!",
            error_text,
            created_at as "created_at!",
            started_at,
            finished_at
        FROM kb_extract_jobs
        WHERE job_id = ?"#,
        job_id,
    )
    .fetch_optional(pool)
    .await
}

/// List all jobs for a given creator.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn list_by_creator(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Vec<KbExtractJob>, sqlx::Error> {
    sqlx::query_as!(
        KbExtractJob,
        r#"SELECT
            job_id as "job_id!",
            creator_id as "creator_id!",
            workspace_id as "workspace_id!",
            work_entry_id as "work_entry_id!",
            world_id as "world_id!",
            status as "status!",
            error_text,
            created_at as "created_at!",
            started_at,
            finished_at
        FROM kb_extract_jobs
        WHERE creator_id = ?
        ORDER BY created_at DESC"#,
        creator_id,
    )
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
    sqlx::query_as!(
        KbExtractJob,
        r#"SELECT
            job_id as "job_id!",
            creator_id as "creator_id!",
            workspace_id as "workspace_id!",
            work_entry_id as "work_entry_id!",
            world_id as "world_id!",
            status as "status!",
            error_text,
            created_at as "created_at!",
            started_at,
            finished_at
        FROM kb_extract_jobs
        WHERE creator_id = ?
          AND status = 'queued'
        ORDER BY created_at ASC
        LIMIT 1"#,
        creator_id,
    )
    .fetch_optional(pool)
    .await
}

/// Mark a job as running. Sets `started_at` to now.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
pub async fn mark_running(pool: &SqlitePool, job_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE kb_extract_jobs
           SET status = 'running', started_at = datetime('now')
           WHERE job_id = ?"#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
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

        mark_done(&pool, &job.job_id).await.unwrap();
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

        let jobs = list_by_creator(&pool, "ctr_1").await.unwrap();
        assert_eq!(jobs.len(), 2);

        let jobs = list_by_creator(&pool, "ctr_2").await.unwrap();
        assert_eq!(jobs.len(), 1);
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
}
