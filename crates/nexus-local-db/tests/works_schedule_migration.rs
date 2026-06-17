//! Hermetic migration test for `works.schedule_json` (V1.50 T-A P0 T1).
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-cron-foundation.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §2.
//!
//! Covers:
//! - **Forward**: `run_migrations` adds the column; existing Works get NULL
//!   (= use defaults; spec §2.3).
//! - **Rollback**: `ALTER TABLE … DROP COLUMN` (SQLite 3.35+) simulates a
//!   down-migration; the column is removed and DAO calls fail gracefully.
//! - **DAO round-trip**: `set_schedule_json` → `get_schedule_json` preserves
//!   the blob; empty string and NULL both resolve to "use defaults".
//!
//! sqlx::migrate in this repo is forward-only (no `.down.sql` convention),
//! so "rollback" is simulated with a manual `DROP COLUMN`.

use sqlx::Row;

/// Helper: fresh pool with all migrations applied.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Build a minimal `WorkRecord` for integration tests (mirrors the
/// `#[cfg(test)] sample_work_for_test` in `works.rs`, duplicated here because
/// that helper is `pub(crate)` and thus invisible to the `tests/` crate).
#[allow(clippy::needless_pass_by_value)]
fn sample_work(work_id: &str, work_ref: &str) -> nexus_local_db::works::WorkRecord {
    use nexus_local_db::works::WorkRecord;
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "default".to_string(),
        status: "draft".to_string(),
        title: "Test Novel".to_string(),
        long_term_goal: "Test goal".to_string(),
        initial_idea: "An idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T00:00:00Z".to_string(),
        updated_at: "2026-06-18T00:00:00Z".to_string(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_ref.to_string()),
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

/// Insert a minimal Work row so the DAO has something to target.
async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, work_ref: &str) {
    let record = sample_work(work_id, work_ref);
    nexus_local_db::works::create_work(pool, &record)
        .await
        .unwrap();
}

#[tokio::test]
async fn forward_migration_adds_schedule_json_column_nullable() {
    let (pool, _dir) = fresh_pool().await;

    // Column exists and is nullable.
    // SAFETY: test-only PRAGMA inspection of the works table schema.
    let row = sqlx::query("PRAGMA table_info(works)")
        .fetch_all(&pool)
        .await
        .unwrap();
    let has_col = row.iter().any(|r| {
        let name: String = r.get("name");
        name == "schedule_json"
    });
    assert!(
        has_col,
        "works.schedule_json column must exist after migration"
    );

    // Existing Work gets NULL (= use defaults).
    seed_work(&pool, "wrk_fwd", "fwd-ref").await;
    let opt = nexus_local_db::works::get_schedule_json(&pool, "wrk_fwd")
        .await
        .unwrap();
    assert!(
        opt.is_none(),
        "newly-migrated Work must read schedule_json as None (use defaults)"
    );
}

#[tokio::test]
async fn rollback_drops_schedule_json_column() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool, "wrk_rb", "rb-ref").await;

    // Simulate a down-migration: DROP COLUMN (SQLite 3.35+).
    // The T-A P1 partial index `idx_works_schedule_json_nonempty`
    // (`202606180003_works_schedule_json_partial_idx.sql`, renumbered by
    // R-V150P2CRONRV-01) references this column, and SQLite refuses DROP
    // COLUMN while an index still depends on it ("error in index ... no such
    // column"). A faithful rollback reverses the index creation first —
    // mirrors the reverse of the T-A P1 migration. W-QC3-01 fix applied here
    // and in T-B P2 fix-wave.
    // SAFETY: test-only DDL simulating rollback of the T1 + T-A P1 migrations.
    sqlx::query("DROP INDEX IF EXISTS idx_works_schedule_json_nonempty")
        .execute(&pool)
        .await
        .expect("drop dependent partial index before DROP COLUMN");
    sqlx::query("ALTER TABLE works DROP COLUMN schedule_json")
        .execute(&pool)
        .await
        .expect("SQLite must support DROP COLUMN (>=3.35)");

    // Column is gone.
    let row = sqlx::query("PRAGMA table_info(works)")
        .fetch_all(&pool)
        .await
        .unwrap();
    let has_col = row.iter().any(|r| {
        let name: String = r.get("name");
        name == "schedule_json"
    });
    assert!(
        !has_col,
        "works.schedule_json column must be gone after simulated rollback"
    );

    // Other columns / rows survive the rollback (data preserved).
    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_rb")
        .await
        .unwrap();
    assert!(work.is_some(), "Work row must survive DROP COLUMN");
}

#[tokio::test]
async fn dao_set_then_get_round_trips_blob() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool, "wrk_rt", "rt-ref").await;

    let blob = r#"{"tz":"Asia/Shanghai","roles":{"brainstorm":{"cron":"0 3,9,15,21 * * *","enabled":true}}}"#;
    nexus_local_db::works::set_schedule_json(&pool, "wrk_rt", blob, "2026-06-18T00:00:00Z")
        .await
        .unwrap();

    let got = nexus_local_db::works::get_schedule_json(&pool, "wrk_rt")
        .await
        .unwrap()
        .expect("schedule_json must be read back after set");
    assert_eq!(got, blob);
}

#[tokio::test]
async fn dao_empty_string_resets_to_defaults() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool, "wrk_empty", "empty-ref").await;

    // Set a blob, then reset to empty (defaults).
    nexus_local_db::works::set_schedule_json(
        &pool,
        "wrk_empty",
        r#"{"tz":"UTC"}"#,
        "2026-06-18T00:00:00Z",
    )
    .await
    .unwrap();
    nexus_local_db::works::set_schedule_json(&pool, "wrk_empty", "", "2026-06-18T00:00:01Z")
        .await
        .unwrap();

    let opt = nexus_local_db::works::get_schedule_json(&pool, "wrk_empty")
        .await
        .unwrap();
    assert!(
        opt.is_none(),
        "empty string schedule_json must resolve to None (use defaults)"
    );
}

#[tokio::test]
async fn dao_set_on_missing_work_errors() {
    let (pool, _dir) = fresh_pool().await;
    let err =
        nexus_local_db::works::set_schedule_json(&pool, "wrk_ghost", "{}", "2026-06-18T00:00:00Z")
            .await
            .unwrap_err();
    assert!(
        matches!(err, nexus_local_db::LocalDbError::MissingVersionKey { .. }),
        "set_schedule_json on a missing Work must return MissingVersionKey, got {err:?}"
    );
}

#[tokio::test]
async fn resolve_work_id_by_ref_or_id_matches_both() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool, "wrk_match", "match-ref").await;

    // By work_ref.
    let by_ref = nexus_local_db::works::resolve_work_id_by_ref_or_id(
        &pool,
        "ctr_test",
        "default",
        "match-ref",
    )
    .await
    .unwrap();
    assert_eq!(by_ref.as_deref(), Some("wrk_match"));

    // By work_id.
    let by_id = nexus_local_db::works::resolve_work_id_by_ref_or_id(
        &pool,
        "ctr_test",
        "default",
        "wrk_match",
    )
    .await
    .unwrap();
    assert_eq!(by_id.as_deref(), Some("wrk_match"));

    // Neither → None.
    let none = nexus_local_db::works::resolve_work_id_by_ref_or_id(
        &pool,
        "ctr_test",
        "default",
        "no-such-ref",
    )
    .await
    .unwrap();
    assert!(none.is_none());
}

/// R-V150-WLA-06 (V1.50 P-last WL-A / cron-foundation qc1 S6): when the
/// `work_ref` of one row collides with the `work_id` of another row (a
/// theoretical edge case — slugs and `wrk_...` IDs normally live in
/// different namespaces), `resolve_work_id_by_ref_or_id` must
/// deterministically prefer the row whose `work_ref` matches (the "ref
/// wins over id" precedence the doc comment promises). The pre-fix query
/// had `LIMIT 1` without `ORDER BY`, leaving the choice non-deterministic.
#[tokio::test]
async fn resolve_work_id_by_ref_or_id_prefers_work_ref_match_on_collision() {
    let (pool, _dir) = fresh_pool().await;
    // Row A: work_id = "wrk_collision", work_ref = "shared-token".
    seed_work(&pool, "wrk_collision", "shared-token").await;
    // Row B: work_id = "shared-token" (collides with A's work_ref), work_ref unset.
    let mut row_b = sample_work("shared-token", "");
    row_b.work_ref = None;
    nexus_local_db::works::create_work(&pool, &row_b)
        .await
        .unwrap();

    // Both rows match the positional `shared-token` (A via work_ref, B
    // via work_id). The query must return A's work_id ("wrk_collision")
    // because work_ref wins over id per the doc contract.
    let resolved = nexus_local_db::works::resolve_work_id_by_ref_or_id(
        &pool,
        "ctr_test",
        "default",
        "shared-token",
    )
    .await
    .unwrap();
    assert_eq!(
        resolved.as_deref(),
        Some("wrk_collision"),
        "work_ref match must win over work_id match on collision; \
         the ORDER BY work_ref IS NULL clause in resolve_work_id_by_ref_or_id \
         guarantees the row with a non-null work_ref sorts first"
    );
}

#[tokio::test]
async fn list_works_schedule_returns_all_works() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool, "wrk_a", "ref-a").await;
    seed_work(&pool, "wrk_b", "ref-b").await;
    nexus_local_db::works::set_schedule_json(
        &pool,
        "wrk_a",
        r#"{"tz":"UTC"}"#,
        "2026-06-18T00:00:00Z",
    )
    .await
    .unwrap();

    let rows = nexus_local_db::works::list_works_schedule(&pool, "ctr_test", "default", None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    let a = rows.iter().find(|r| r.work_id == "wrk_a").unwrap();
    assert_eq!(a.work_ref.as_deref(), Some("ref-a"));
    assert!(a.schedule_json.is_some());
    let b = rows.iter().find(|r| r.work_id == "wrk_b").unwrap();
    assert!(b.schedule_json.is_none(), "unset Work must read as None");
}

/// R-V150P0-W4: `list_works_schedule` honors the `limit` cap.
#[tokio::test]
async fn list_works_schedule_applies_limit() {
    let (pool, _dir) = fresh_pool().await;
    for i in 0..5u32 {
        seed_work(&pool, &format!("wrk_l{i}"), &format!("l{i}-ref")).await;
    }
    let rows = nexus_local_db::works::list_works_schedule(&pool, "ctr_test", "default", Some(2))
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "limit=2 must cap the result to 2 rows (got {})",
        rows.len()
    );

    // None → default cap (100) returns all 5 seeded rows.
    let rows_all = nexus_local_db::works::list_works_schedule(&pool, "ctr_test", "default", None)
        .await
        .unwrap();
    assert_eq!(rows_all.len(), 5);
}
