//! Tests for V1.42 migration fixes (W-01 idempotency, W-02 index coverage).

/// W-01: Verify the V1.42 migration SQL is idempotent.
///
/// Simulates running the raw DDL on an already-migrated DB by:
/// 1. Running migrations normally (creates the new schema).
/// 2. Inserting some data into work_chapters.
/// 3. Running the V1.42 migration DDL manually (simulating a re-run).
/// 4. Asserting data is preserved and no error occurs.
#[tokio::test]
async fn w01_v142_migration_idempotent() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Insert test data into work_chapters (requires a works row first).
    let now_ts = chrono::Utc::now().timestamp();
    // SAFETY: test-only — minimal works row for FK constraint.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
         long_term_goal, initial_idea, intake_status, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at,
         current_stage, stage_status, current_chapter, auto_chain_enabled,
         auto_chain_interrupted, auto_review_master_on_timeout)
         VALUES ('w01_work', 'w01_creator', 'ws', 'active', 'W-01 Test',
         'goal', 'idea', 'complete', '[]',
         'novel-writing', '[]', ?, ?,
         'produce', 'active', 1, 1, 0, 0)",
    )
    .bind(now_ts)
    .bind(now_ts)
    .execute(&pool)
    .await
    .unwrap();

    // Insert chapter rows with the new composite PK.
    // SAFETY: test-only — DML for chapter seeding.
    sqlx::query(
        "INSERT INTO work_chapters
           (work_id, volume, chapter, slug, status, created_at, updated_at)
         VALUES ('w01_work', 1, 1, 'ch01', 'finalized', ?, ?)",
    )
    .bind(now_ts)
    .bind(now_ts)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO work_chapters
           (work_id, volume, chapter, slug, status, created_at, updated_at)
         VALUES ('w01_work', 1, 2, 'ch02', 'not_started', ?, ?)",
    )
    .bind(now_ts)
    .bind(now_ts)
    .execute(&pool)
    .await
    .unwrap();

    // Now simulate re-running the V1.42 migration DDL manually.
    // This should succeed because of the DROP TABLE IF EXISTS guard.
    // We re-run each statement individually. Statements are extracted
    // by removing full-line comments and splitting on semicolons.
    let migration_sql = include_str!("../migrations/202606110001_v142_multi_volume_pk.sql");

    // First, strip all full-line comments to avoid semicolons within comments
    // from splitting statements incorrectly.
    let sql_no_comments: String = migration_sql
        .lines()
        .filter(|line| {
            let stripped = line.trim();
            !stripped.starts_with("--") && !stripped.is_empty()
        })
        .collect::<Vec<&str>>()
        .join("\n");

    // Split by semicolons and execute each non-empty statement.
    for stmt in sql_no_comments.split(';') {
        let sql = stmt.trim();
        if sql.is_empty() {
            continue;
        }
        // This should NOT fail — the DROP IF EXISTS guard ensures the
        // legacy table is cleaned up, and IF NOT EXISTS on indexes is safe.
        sqlx::query(sql).execute(&pool).await.unwrap_or_else(|e| {
            panic!("V1.42 migration re-run failed on statement:\n{sql}\nError: {e}");
        });
    }

    // Verify data survived the re-run.
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM work_chapters WHERE work_id = 'w01_work'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 2, "data should survive migration re-run");

    // Verify the composite PK is intact.
    let pk_check: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM pragma_table_info('work_chapters') WHERE pk > 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        pk_check.0, 3,
        "composite PK should have 3 columns (work_id, volume, chapter)"
    );
}

/// W-02: Verify the volume-aware next-chapter index exists and covers the
/// query pattern.
///
/// The index `(work_id, status, volume, chapter)` is designed to serve the
/// `next_chapter_volume_aware` query: `WHERE work_id = ? AND status IN (...)
/// ORDER BY volume, chapter LIMIT 1`. This test verifies the index exists
/// and that SQLite can use it (or an equivalent covering index) for the query.
#[tokio::test]
async fn w02_volume_aware_index_coverage() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Check the index exists.
    let idx_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_work_chapters_next_volume_aware'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        idx_count.0, 1,
        "idx_work_chapters_next_volume_aware index should exist"
    );

    // Verify the index covers the expected columns.
    // SAFETY: test-only — introspects index definition from sqlite_master.
    let idx_sql: (String,) = sqlx::query_as(
        "SELECT sql FROM sqlite_master WHERE type='index' AND name='idx_work_chapters_next_volume_aware'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let sql = idx_sql.0.to_lowercase();
    assert!(
        sql.contains("work_id")
            && sql.contains("status")
            && sql.contains("volume")
            && sql.contains("chapter"),
        "index should cover (work_id, status, volume, chapter), got: {sql}"
    );

    // Verify EXPLAIN QUERY PLAN does not show a full SCAN TABLE.
    // The planner may choose the autoindex or our index; both are acceptable
    // as long as there's no full table scan.
    // SAFETY: test-only — EXPLAIN QUERY PLAN diagnostic.
    let plan_rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN SELECT volume, chapter FROM work_chapters \
         WHERE work_id = ? AND status IN ('not_started', 'outlined', 'draft') \
         ORDER BY volume ASC, chapter ASC LIMIT 1",
    )
    .bind("test_work_id")
    .fetch_all(&pool)
    .await
    .unwrap();

    let plan_text: String = plan_rows
        .iter()
        .map(|(_, _, _, detail)| detail.clone())
        .collect::<Vec<String>>()
        .join("\n");

    // There should be no full "SCAN TABLE work_chapters" (full scan without index).
    assert!(
        !plan_text.contains("SCAN TABLE work_chapters"),
        "EXPLAIN QUERY PLAN should not show full table scan.\n\
         Plan:\n{plan_text}"
    );
}
