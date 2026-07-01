//! Pending review CRUD operations for memory pipeline.
//!
//! Manages session-end captured queue entries for review → memory promotion.
//! See creator-memory-soul-lifecycle.md §6.2.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Pending review record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct PendingReviewRecord {
    /// Unique identifier for this pending entry.
    pub pending_id: String,
    /// ACP session ID that triggered the capture.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Optional world ID for context.
    pub world_id: Option<String>,
    /// Task kind heuristic (brainstorm, outline, chapter, research, unknown).
    pub task_kind: String,
    /// Raw digest extracted from session.
    pub raw_digest: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Create a new pending review record.
///
/// Inserts the record into the `memory_pending_review` table.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_pending_review(
    pool: &SqlitePool,
    record: &PendingReviewRecord,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        record.pending_id,
        record.session_id,
        record.creator_id,
        record.world_id,
        record.task_kind,
        record.raw_digest,
        record.created_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// List all pending reviews for a creator.
///
/// Returns records ordered by `created_at` descending (most recent first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_pending_reviews(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Vec<PendingReviewRecord>, LocalDbError> {
    let rows = sqlx::query!(
        "SELECT pending_id as \"pending_id!\", session_id as \"session_id!\",
                creator_id as \"creator_id!\", world_id,
                task_kind as \"task_kind!\", raw_digest as \"raw_digest!\",
                created_at as \"created_at!\"
         FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at DESC",
        creator_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PendingReviewRecord {
            pending_id: r.pending_id,
            session_id: r.session_id,
            creator_id: r.creator_id,
            world_id: r.world_id,
            task_kind: r.task_kind,
            raw_digest: r.raw_digest,
            created_at: r.created_at,
        })
        .collect())
}

/// Get a specific pending review by ID.
///
/// Returns None if the record doesn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_pending_review(
    pool: &SqlitePool,
    pending_id: &str,
) -> Result<Option<PendingReviewRecord>, LocalDbError> {
    let row = sqlx::query!(
        "SELECT pending_id as \"pending_id!\", session_id as \"session_id!\",
                creator_id as \"creator_id!\", world_id,
                task_kind as \"task_kind!\", raw_digest as \"raw_digest!\",
                created_at as \"created_at!\"
         FROM memory_pending_review WHERE pending_id = ?",
        pending_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| PendingReviewRecord {
        pending_id: r.pending_id,
        session_id: r.session_id,
        creator_id: r.creator_id,
        world_id: r.world_id,
        task_kind: r.task_kind,
        raw_digest: r.raw_digest,
        created_at: r.created_at,
    }))
}

/// Delete a pending review by ID.
///
/// Returns true if a record was deleted, false if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn delete_pending_review(
    pool: &SqlitePool,
    pending_id: &str,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM memory_pending_review WHERE pending_id = ?",
        pending_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Count pending reviews for a creator.
///
/// Used for queue depth monitoring and review scheduling.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
///
/// # Panics
///
/// Panics if the count is negative (database invariant violation).
pub async fn count_pending_reviews(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<usize, LocalDbError> {
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) as \"count!\" FROM memory_pending_review WHERE creator_id = ?",
        creator_id
    )
    .fetch_one(pool)
    .await?;
    Ok(usize::try_from(count).expect("count is non-negative and fits in usize"))
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

    fn sample_record(pending_id: &str) -> PendingReviewRecord {
        PendingReviewRecord {
            pending_id: pending_id.to_string(),
            session_id: format!("sess_{pending_id}"),
            creator_id: "ctr_test".to_string(),
            world_id: Some("wld_test".to_string()),
            task_kind: "brainstorm".to_string(),
            raw_digest: "Test digest content".to_string(),
            created_at: "2026-04-14T10:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn test_create_and_get_pending_review() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_record("pending_001");
        create_pending_review(&pool, &record).await.unwrap();

        let fetched = get_pending_review(&pool, "pending_001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.pending_id, "pending_001");
        assert_eq!(fetched.creator_id, "ctr_test");
        assert_eq!(fetched.task_kind, "brainstorm");
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_pending_review(&pool, "pending_ghost")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_list_pending_reviews_by_creator() {
        let (pool, _dir) = fresh_pool().await;

        let record1 = sample_record("pending_001");
        let record2 = PendingReviewRecord {
            pending_id: "pending_002".to_string(),
            session_id: "sess_pending_002".to_string(),
            creator_id: "ctr_other".to_string(),
            ..sample_record("pending_002")
        };
        let record3 = PendingReviewRecord {
            pending_id: "pending_003".to_string(),
            session_id: "sess_pending_003".to_string(),
            created_at: "2026-04-14T12:00:00Z".to_string(),
            ..sample_record("pending_003")
        };

        create_pending_review(&pool, &record1).await.unwrap();
        create_pending_review(&pool, &record2).await.unwrap();
        create_pending_review(&pool, &record3).await.unwrap();

        // List for ctr_test should return 2 records (001 and 003)
        let list = list_pending_reviews(&pool, "ctr_test").await.unwrap();
        assert_eq!(list.len(), 2);

        // Ordered by created_at DESC
        assert_eq!(list[0].pending_id, "pending_003");
        assert_eq!(list[1].pending_id, "pending_001");
    }

    #[tokio::test]
    async fn test_delete_pending_review() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_record("pending_001");
        create_pending_review(&pool, &record).await.unwrap();

        assert!(delete_pending_review(&pool, "pending_001").await.unwrap());
        assert!(get_pending_review(&pool, "pending_001")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_false() {
        let (pool, _dir) = fresh_pool().await;
        assert!(!delete_pending_review(&pool, "pending_ghost").await.unwrap());
    }

    #[tokio::test]
    async fn test_count_pending_reviews() {
        let (pool, _dir) = fresh_pool().await;

        assert_eq!(count_pending_reviews(&pool, "ctr_test").await.unwrap(), 0);

        let record1 = sample_record("pending_001");
        let record2 = sample_record("pending_002");
        create_pending_review(&pool, &record1).await.unwrap();
        create_pending_review(&pool, &record2).await.unwrap();

        assert_eq!(count_pending_reviews(&pool, "ctr_test").await.unwrap(), 2);
        assert_eq!(count_pending_reviews(&pool, "ctr_other").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_pending_review_with_null_world_id() {
        let (pool, _dir) = fresh_pool().await;
        let record = PendingReviewRecord {
            pending_id: "pending_null".to_string(),
            session_id: "sess_pending_null".to_string(),
            world_id: None,
            ..sample_record("pending_null")
        };
        create_pending_review(&pool, &record).await.unwrap();

        let fetched = get_pending_review(&pool, "pending_null")
            .await
            .unwrap()
            .unwrap();
        assert!(fetched.world_id.is_none());
    }
}
