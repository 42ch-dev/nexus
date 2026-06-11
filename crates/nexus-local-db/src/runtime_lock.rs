//! Runtime lock acquire/release for per-Work concurrency control.
//!
//! Spec: `novel-multi-work-lifecycle.md` §4 (V1.42 P0 production wiring).
//!
//! Two holder formats:
//! - `cli:<caller_info>:<uuid>` — synchronous CLI mutating commands
//! - `daemon:schedule:<schedule_id>` — daemon auto-chain schedule
//!
//! TTL stale recovery: if `runtime_lock_acquired_at` is older than the
//! configured threshold (default 2h), the daemon may clear the holder
//! before a new acquire.

use sqlx::SqlitePool;

use crate::error::LocalDbError;
use crate::works::{get_work, WorkRecord};

/// Default TTL threshold for stale lock recovery (2 hours in seconds).
pub const DEFAULT_RUNTIME_LOCK_TTL_SECS: i64 = 7200;

/// Build a CLI-format holder string: `cli:<caller_info>:<uuid>`.
#[must_use]
pub fn cli_holder(caller_info: &str) -> String {
    format!("cli:{caller_info}:{}", uuid::Uuid::new_v4())
}

/// Build a daemon-format holder string: `daemon:schedule:<schedule_id>`.
#[must_use]
pub fn schedule_holder(schedule_id: &str) -> String {
    format!("daemon:schedule:{schedule_id}")
}

/// Result of a lock acquisition attempt.
#[derive(Debug, Clone)]
pub enum AcquireResult {
    /// Lock acquired successfully.
    Acquired {
        /// The holder string that was written.
        holder: String,
    },
    /// Lock is held by another process.
    Locked {
        /// The current holder.
        holder: String,
        /// When it was acquired (ISO-8601).
        acquired_at: Option<String>,
    },
}

/// Attempt to acquire the runtime lock for a Work.
///
/// If the Work is already locked, returns `AcquireResult::Locked`.
/// If `force_stale` is true and the lock is older than `ttl_secs`,
/// the stale lock is cleared first and re-acquired.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the Work doesn't exist.
pub async fn acquire_runtime_lock(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    holder: &str,
    ttl_secs: i64,
    force_stale: bool,
) -> Result<AcquireResult, LocalDbError> {
    let work = get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("works/{work_id}"),
        })?;

    if let Some(ref existing) = work.runtime_lock_holder {
        // Lock is present. Check if we should force-clear a stale one.
        if force_stale && is_lock_stale(&work, ttl_secs) {
            tracing::info!(
                work_id = %work_id,
                old_holder = %existing,
                new_holder = %holder,
                ttl_secs = ttl_secs,
                "runtime_lock: clearing stale holder"
            );
        } else {
            return Ok(AcquireResult::Locked {
                holder: existing.clone(),
                acquired_at: work.runtime_lock_acquired_at.clone(),
            });
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    // SAFETY: Dynamic SQL required for conditional lock acquire.
    // All values are bound parameters.
    let sql = "UPDATE works SET runtime_lock_holder = ?, runtime_lock_acquired_at = ?, updated_at = ? \
               WHERE work_id = ? AND creator_id = ?";
    sqlx::query(sql)
        .bind(holder)
        .bind(&now)
        .bind(&now)
        .bind(work_id)
        .bind(creator_id)
        .execute(pool)
        .await
        .map_err(LocalDbError::from)?;

    Ok(AcquireResult::Acquired {
        holder: holder.to_string(),
    })
}

/// Release the runtime lock for a Work.
///
/// Only releases if the current holder matches `expected_holder`.
/// This prevents one process from releasing another's lock.
///
/// Returns `Ok(true)` if the lock was released, `Ok(false)` if the
/// lock was already gone or held by a different process.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn release_runtime_lock(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    expected_holder: &str,
) -> Result<bool, LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    // SAFETY: Dynamic SQL required for conditional lock release.
    // Only clears if holder matches to prevent cross-process release.
    let sql = "UPDATE works SET runtime_lock_holder = NULL, runtime_lock_acquired_at = NULL, updated_at = ? \
               WHERE work_id = ? AND creator_id = ? AND runtime_lock_holder = ?";
    let result = sqlx::query(sql)
        .bind(&now)
        .bind(work_id)
        .bind(creator_id)
        .bind(expected_holder)
        .execute(pool)
        .await
        .map_err(LocalDbError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Check if a Work's runtime lock is stale (older than TTL).
///
/// Returns `false` if no lock is present or no `acquired_at` timestamp.
#[must_use]
pub fn is_lock_stale(work: &WorkRecord, ttl_secs: i64) -> bool {
    let Some(ref acquired_at_str) = work.runtime_lock_acquired_at else {
        return false;
    };
    let Some(holder) = &work.runtime_lock_holder else {
        return false;
    };

    let Ok(acquired) = chrono::DateTime::parse_from_rfc3339(acquired_at_str) else {
        tracing::warn!(
            holder = %holder,
            acquired_at = %acquired_at_str,
            "runtime_lock: malformed acquired_at timestamp, treating as stale"
        );
        // Malformed timestamp — treat as stale so it can be recovered.
        return true;
    };

    let elapsed = chrono::Utc::now().signed_duration_since(acquired.with_timezone(&chrono::Utc));
    elapsed.num_seconds() > ttl_secs
}

/// Clear any stale runtime locks on a Work if TTL exceeded.
///
/// Returns `Ok(true)` if a stale lock was cleared, `Ok(false)` otherwise.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn clear_stale_lock(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    ttl_secs: i64,
) -> Result<bool, LocalDbError> {
    let work = get_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("works/{work_id}"),
        })?;

    if !is_lock_stale(&work, ttl_secs) {
        return Ok(false);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let stale_holder = work.runtime_lock_holder.as_deref().unwrap_or("?");
    tracing::info!(
        work_id = %work_id,
        stale_holder = %stale_holder,
        ttl_secs = ttl_secs,
        "runtime_lock: clearing stale lock"
    );

    // SAFETY: Dynamic SQL for conditional stale lock clear.
    let sql = "UPDATE works SET runtime_lock_holder = NULL, runtime_lock_acquired_at = NULL, updated_at = ? \
               WHERE work_id = ? AND creator_id = ?";
    sqlx::query(sql)
        .bind(&now)
        .bind(work_id)
        .bind(creator_id)
        .execute(pool)
        .await
        .map_err(LocalDbError::from)?;

    Ok(true)
}

/// Read the current TTL threshold from the environment variable
/// `NEXUS_RUNTIME_LOCK_TTL_SECS`, falling back to the default.
#[must_use]
pub fn ttl_from_env() -> i64 {
    std::env::var("NEXUS_RUNTIME_LOCK_TTL_SECS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_RUNTIME_LOCK_TTL_SECS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::works;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn sample_work(work_id: &str) -> WorkRecord {
        works::sample_work_for_test(work_id)
    }

    #[tokio::test]
    async fn test_acquire_and_release_lock() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_lock_01");
        works::create_work(&pool, &record).await.unwrap();

        let holder = cli_holder("test_caller");
        let result = acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_01", &holder, 7200, false)
            .await
            .unwrap();

        match result {
            AcquireResult::Acquired { holder: h } => assert_eq!(h, holder),
            AcquireResult::Locked { .. } => panic!("should have acquired"),
        }

        // Verify lock is present on the Work
        let work = works::get_work(&pool, "ctr_test", "wrk_lock_01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(work.runtime_lock_holder.as_deref(), Some(holder.as_str()));
        assert!(work.runtime_lock_acquired_at.is_some());

        // Release
        let released = release_runtime_lock(&pool, "ctr_test", "wrk_lock_01", &holder)
            .await
            .unwrap();
        assert!(released);

        let work = works::get_work(&pool, "ctr_test", "wrk_lock_01")
            .await
            .unwrap()
            .unwrap();
        assert!(work.runtime_lock_holder.is_none());
        assert!(work.runtime_lock_acquired_at.is_none());
    }

    #[tokio::test]
    async fn test_second_acquire_fails_when_locked() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_lock_02");
        works::create_work(&pool, &record).await.unwrap();

        let holder1 = cli_holder("caller_1");
        let holder2 = cli_holder("caller_2");

        let result1 = acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_02", &holder1, 7200, false)
            .await
            .unwrap();
        assert!(matches!(result1, AcquireResult::Acquired { .. }));

        let result2 = acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_02", &holder2, 7200, false)
            .await
            .unwrap();
        match result2 {
            AcquireResult::Locked { holder, .. } => assert_eq!(holder, holder1),
            AcquireResult::Acquired { .. } => panic!("should not acquire when locked"),
        }
    }

    #[tokio::test]
    async fn test_release_wrong_holder_noop() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_lock_03");
        works::create_work(&pool, &record).await.unwrap();

        let holder = cli_holder("real_owner");
        acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_03", &holder, 7200, false)
            .await
            .unwrap();

        let released = release_runtime_lock(&pool, "ctr_test", "wrk_lock_03", "wrong_owner")
            .await
            .unwrap();
        assert!(!released);

        // Lock should still be held
        let work = works::get_work(&pool, "ctr_test", "wrk_lock_03")
            .await
            .unwrap()
            .unwrap();
        assert!(work.runtime_lock_holder.is_some());
    }

    #[tokio::test]
    async fn test_stale_lock_recovery() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_lock_04");
        works::create_work(&pool, &record).await.unwrap();

        let holder = cli_holder("stale_caller");
        acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_04", &holder, 7200, false)
            .await
            .unwrap();

        // Manually backdate the acquired_at to simulate stale lock (3 hours ago)
        let three_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
        let patch = works::WorkPatch {
            runtime_lock_acquired_at: Some(Some(three_hours_ago)),
            ..Default::default()
        };
        works::patch_work(&pool, "ctr_test", "wrk_lock_04", &patch, &chrono::Utc::now().to_rfc3339())
            .await
            .unwrap();

        // Acquire with force_stale=true should succeed
        let new_holder = cli_holder("new_caller");
        let result = acquire_runtime_lock(
            &pool,
            "ctr_test",
            "wrk_lock_04",
            &new_holder,
            7200,
            true,
        )
        .await
        .unwrap();

        match result {
            AcquireResult::Acquired { holder: h } => assert_eq!(h, new_holder),
            AcquireResult::Locked { .. } => panic!("should have force-acquired stale lock"),
        }

        let work = works::get_work(&pool, "ctr_test", "wrk_lock_04")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(work.runtime_lock_holder.as_deref(), Some(new_holder.as_str()));
    }

    #[tokio::test]
    async fn test_clear_stale_lock() {
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_lock_05");
        works::create_work(&pool, &record).await.unwrap();

        let holder = cli_holder("stale_owner");
        acquire_runtime_lock(&pool, "ctr_test", "wrk_lock_05", &holder, 7200, false)
            .await
            .unwrap();

        // Backdate to 5 hours ago
        let five_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(5)).to_rfc3339();
        let patch = works::WorkPatch {
            runtime_lock_acquired_at: Some(Some(five_hours_ago)),
            ..Default::default()
        };
        works::patch_work(&pool, "ctr_test", "wrk_lock_05", &patch, &chrono::Utc::now().to_rfc3339())
            .await
            .unwrap();

        let cleared = clear_stale_lock(&pool, "ctr_test", "wrk_lock_05", 7200)
            .await
            .unwrap();
        assert!(cleared);

        let work = works::get_work(&pool, "ctr_test", "wrk_lock_05")
            .await
            .unwrap()
            .unwrap();
        assert!(work.runtime_lock_holder.is_none());
    }

    #[tokio::test]
    async fn test_is_lock_stale_no_lock() {
        let work = sample_work("wrk_stale_none");
        assert!(!is_lock_stale(&work, 7200));
    }

    #[tokio::test]
    async fn test_is_lock_stale_malformed_timestamp() {
        let mut work = sample_work("wrk_stale_malformed");
        work.runtime_lock_holder = Some("cli:bad:ts".to_string());
        work.runtime_lock_acquired_at = Some("not-a-timestamp".to_string());
        // Malformed → stale (recoverable)
        assert!(is_lock_stale(&work, 7200));
    }

    #[test]
    fn test_cli_holder_format() {
        let holder = cli_holder("pid123");
        assert!(holder.starts_with("cli:pid123:"));
        // UUID part should be parseable
        let parts: Vec<&str> = holder.split(':').collect();
        assert_eq!(parts.len(), 3);
        assert!(uuid::Uuid::parse_str(parts[2]).is_ok());
    }

    #[test]
    fn test_schedule_holder_format() {
        let holder = schedule_holder("ACH20260611120000123");
        assert_eq!(holder, "daemon:schedule:ACH20260611120000123");
    }

    #[tokio::test]
    async fn test_concurrent_mutation_second_fails() {
        // AC1: two concurrent mutating operations on same Work → second fails with holder hint
        let (pool, _dir) = fresh_pool().await;
        let record = sample_work("wrk_concurrent");
        works::create_work(&pool, &record).await.unwrap();

        let holder_a = cli_holder("process_a");
        let holder_b = schedule_holder("SCH20260611120000");

        // First acquire succeeds
        let result_a = acquire_runtime_lock(
            &pool, "ctr_test", "wrk_concurrent", &holder_a, 7200, false,
        )
        .await
        .unwrap();
        assert!(matches!(result_a, AcquireResult::Acquired { .. }));

        // Second acquire fails with holder hint
        let result_b = acquire_runtime_lock(
            &pool, "ctr_test", "wrk_concurrent", &holder_b, 7200, false,
        )
        .await
        .unwrap();
        match result_b {
            AcquireResult::Locked { holder, .. } => {
                assert_eq!(holder, holder_a);
            }
            AcquireResult::Acquired { .. } => panic!("second acquire should fail"),
        }
    }
}
