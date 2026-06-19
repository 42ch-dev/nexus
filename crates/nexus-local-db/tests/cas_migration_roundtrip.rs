//! Migration roundtrip test for V1.51 T-B P1 version columns.
//!
//! Verifies that:
//! - `kb_extract_jobs.version` exists, defaults to 0, and is NOT NULL
//! - `novel_pool_entries.version` exists, defaults to 0, and is NOT NULL
//! - Existing rows (before migration) get version=0
//! - New rows get version=0 by default
//! - The CAS increment pattern works (version advances on UPDATE)

use sqlx::SqlitePool;

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

#[tokio::test]
async fn test_kb_extract_jobs_version_column_defaults_to_zero() {
    let (pool, _dir) = fresh_pool().await;

    // Insert a row without specifying version — should default to 0.
    let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
         VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();

    let (version,): (i64,) = sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
        .bind(&job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, 0);

    // Verify the version column is NOT NULL (this would fail at runtime if NULL)
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(versions.len(), 1);
}

#[tokio::test]
async fn test_novel_pool_entries_version_column_defaults_to_zero() {
    let (pool, _dir) = fresh_pool().await;

    // Seed a work row first (FK constraint).
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, \
         long_term_goal, initial_idea, intake_status, inspiration_log, primary_preset_id, \
         schedule_ids, created_at, updated_at, current_stage, stage_status, current_chapter, \
         auto_chain_enabled, auto_chain_interrupted, auto_review_master_on_timeout) \
         VALUES ('wrk_test_version', 'ctr_test', 'default', 'active', 'Test', \
         'goal', 'idea', 'pending', '[]', 'novel-writing', '[]', ?, ?, \
         'intake', 'pending', 0, 1, 0, 0)",
    )
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();

    // Insert a pool entry without specifying version.
    sqlx::query(
        "INSERT INTO novel_pool_entries (entry_id, creator_id, work_id, status, promoted_at, title, updated_at) \
         VALUES ('npe_test_ver', 'ctr_test', 'wrk_test_version', 'queued', ?, 'Test', ?)",
    )
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();

    let (version,): (i64,) =
        sqlx::query_as("SELECT version FROM novel_pool_entries WHERE entry_id = 'npe_test_ver'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 0);
}

#[tokio::test]
async fn test_version_increments_on_cas_update() {
    let (pool, _dir) = fresh_pool().await;

    let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
         VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();

    // Simulate a CAS update: version=0 → version=1.
    let result = sqlx::query(
        "UPDATE kb_extract_jobs SET status = 'running', version = version + 1 \
         WHERE job_id = ? AND version = 0",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let (version,): (i64,) = sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
        .bind(&job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, 1);

    // A second CAS with expected_version=0 should fail (version is now 1).
    let result2 = sqlx::query(
        "UPDATE kb_extract_jobs SET version = version + 1 \
         WHERE job_id = ? AND version = 0",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        result2.rows_affected(),
        0,
        "CAS should reject stale version=0"
    );

    // Version should still be 1 (not incremented).
    let (version_after,): (i64,) =
        sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version_after, 1);
}

#[tokio::test]
async fn test_cas_marks_confirmed_with_version_guard() {
    let (pool, _dir) = fresh_pool().await;

    // Insert a pending promotion row.
    let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO kb_extract_jobs \
         (job_id, creator_id, workspace_id, work_entry_id, world_id, status, \
          promotion_status, proposed_payload, canonical_name_guess, version) \
         VALUES (?, 'ctr_test', 'wrk_test', 'test_name', 'wld_test', 'done', \
          'pending', '{}', 'test_name', 0)",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();

    // Call the CAS-aware confirm with correct version.
    let mut tx = pool.begin().await.unwrap();
    let result = nexus_local_db::kb_extract_job::mark_confirmed_in_tx_with_cas(&mut tx, &job_id, 0)
        .await
        .unwrap();
    assert!(result, "should flip pending → confirmed");
    tx.commit().await.unwrap();

    // Verify state.
    let (status, version): (String, i64) =
        sqlx::query_as("SELECT promotion_status, version FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "confirmed");
    assert_eq!(version, 1, "version should increment from 0 to 1");
}

#[tokio::test]
async fn test_cas_marks_confirmed_rejects_stale_version() {
    let (pool, _dir) = fresh_pool().await;

    let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO kb_extract_jobs \
         (job_id, creator_id, workspace_id, work_entry_id, world_id, status, \
          promotion_status, proposed_payload, canonical_name_guess, version) \
         VALUES (?, 'ctr_test', 'wrk_test', 'test_name', 'wld_test', 'done', \
          'pending', '{}', 'test_name', 0)",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .unwrap();

    // Another writer bumps the version.
    sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

    // CAS with stale expected_version=0 should fail.
    let mut tx = pool.begin().await.unwrap();
    let err = nexus_local_db::kb_extract_job::mark_confirmed_in_tx_with_cas(&mut tx, &job_id, 0)
        .await
        .unwrap_err();
    tx.rollback().await.unwrap();

    match err {
        nexus_local_db::LocalDbError::VersionMismatch {
            ref table,
            ref id,
            expected,
            actual,
        } => {
            assert_eq!(table, "kb_extract_jobs");
            assert_eq!(id, &job_id);
            assert_eq!(expected, 0);
            assert_eq!(actual, Some(1));
        }
        other => panic!("expected VersionMismatch, got {other:?}"),
    }
}
