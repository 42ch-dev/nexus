//! V1.48 P4 serial hardening hermetic acceptance tests.
//!
//! Covers novel-workflow-profile §4.5.7 tests #4 (resume draft row) and
//! #5 (`reconcile-chapters` with DB-as-status-SSOT conflict rules).

/// §4.5.7 #4 — Resume behavior: a new run against a Work with one `draft` row
/// resumes that row and does not create a new row.
///
/// Verifies `next_chapter` returns the existing draft row and that the row
/// count stays exactly one (no duplicate chapter created).
#[tokio::test]
async fn v148_serial_resume_draft_no_duplicate_row() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    let work_id = "wrk_v148_resume_001";
    let now_ts = chrono::Utc::now().timestamp();

    // SAFETY: test-only — minimal works row for FK constraint.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
         long_term_goal, initial_idea, intake_status, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at,
         current_stage, stage_status, current_chapter, auto_chain_enabled,
         auto_chain_interrupted, auto_review_master_on_timeout,
         total_planned_chapters, work_profile)
         VALUES (?, 'ctr_test', 'ws', 'active', 'Resume Test',
         'goal', 'idea', 'complete', '[]',
         'novel-writing', '[]', ?, ?,
         'produce', 'active', 0, 1, 0, 0,
         1, 'novel')",
    )
    .bind(work_id)
    .bind(now_ts)
    .bind(now_ts)
    .execute(&pool)
    .await
    .unwrap();

    // Seed a single chapter and transition it to draft.
    nexus_local_db::work_chapters::seed_chapters(
        &pool,
        work_id,
        "resume-novel",
        1,
        "2026-06-16T10:00:00Z",
    )
    .await
    .unwrap();
    nexus_local_db::work_chapters::update_status(
        &pool,
        work_id,
        1,
        1,
        "draft",
        None,
        "2026-06-16T11:00:00Z",
    )
    .await
    .unwrap();

    // Precondition: exactly one chapter row exists.
    let before_count = nexus_local_db::work_chapters::count_chapters(&pool, work_id)
        .await
        .unwrap();
    assert_eq!(
        before_count,
        1,
        "setup should create exactly one chapter row"
    );

    // Resume selection should return the existing draft row, not create another.
    let next = nexus_local_db::work_chapters::next_chapter(&pool, work_id)
        .await
        .unwrap();
    assert_eq!(
        next,
        Some(1),
        "next_chapter should resume the single draft row (§4.5.7 #4)"
    );

    let after_count = nexus_local_db::work_chapters::count_chapters(&pool, work_id)
        .await
        .unwrap();
    assert_eq!(
        after_count, 1,
        "resume selection must not create a duplicate chapter row (§4.5.7 #4)"
    );

    // The Work is not complete while its only chapter is draft.
    let completed = nexus_local_db::work_chapters::is_work_completed(&pool, work_id)
        .await
        .unwrap();
    assert!(
        !completed,
        "Work with one draft row must not be reported as completed"
    );
}
