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
        before_count, 1,
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

/// §4.5.7 #5 — `creator run reconcile-chapters <work_id>` rebuilds missing
/// `work_chapters` rows from `Stories/` and preserves DB-as-status-SSOT when
/// filesystem frontmatter disagrees with the DB row (§4.5.3).
#[tokio::test]
async fn v148_serial_reconcile_preserves_db_status_and_creates_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("state.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    let work_id = "wrk_v148_reconcile_001";
    let work_ref = "reconcile-novel";
    let now_ts = chrono::Utc::now().timestamp();

    // SAFETY: test-only — minimal works row for FK constraint.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
         long_term_goal, initial_idea, intake_status, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at,
         current_stage, stage_status, current_chapter, auto_chain_enabled,
         auto_chain_interrupted, auto_review_master_on_timeout,
         total_planned_chapters, work_profile, story_ref)
         VALUES (?, 'ctr_test', 'ws', 'active', 'Reconcile Test',
         'goal', 'idea', 'complete', '[]',
         'novel-writing', '[]', ?, ?,
         'produce', 'active', 0, 1, 0, 0,
         2, 'novel', ?)",
    )
    .bind(work_id)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_ref)
    .execute(&pool)
    .await
    .unwrap();

    // Pre-seed chapter 1 as draft in DB (this row exists but the file will
    // claim it is finalized — DB must win per §4.5.3).
    nexus_local_db::work_chapters::seed_chapters(
        &pool,
        work_id,
        work_ref,
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

    // Create Stories/ with two files:
    // - ch01-finalized.md: frontmatter says finalized, but DB says draft.
    // - ch02-extra.md: no DB row, so reconcile should create one.
    let stories_dir = tmp.path().join("Works").join(work_ref).join("Stories");
    std::fs::create_dir_all(&stories_dir).unwrap();

    std::fs::write(
        stories_dir.join("ch01-finalized.md"),
        "---\ntitle: Chapter One\nchapter: 1\nstatus: finalized\nword_count: 5100\n---\nDraft body still here.",
    )
    .unwrap();
    std::fs::write(
        stories_dir.join("ch02-extra.md"),
        "---\ntitle: Chapter Two\nchapter: 2\nstatus: not_started\n---\nSecond chapter body.",
    )
    .unwrap();

    let report = nexus_local_db::work_chapters::reconcile_from_filesystem(
        &pool,
        work_id,
        work_ref,
        tmp.path(),
        "2026-06-16T12:00:00Z",
        false,
    )
    .await
    .unwrap();

    // ch01: DB status preserved (SSOT), word_count mirrored, frontmatter
    // re-synced → counts as both updated and resynced.
    // ch02: new row from file → counts as created.
    assert_eq!(report.created, 1, "ch02 file should create one new DB row");
    assert_eq!(
        report.updated, 1,
        "ch01 word_count should be mirrored while DB status is preserved"
    );
    assert_eq!(
        report.resynced, 1,
        "ch01 frontmatter should be re-synced to DB status"
    );
    assert_eq!(
        report.preserved, 0,
        "no rows are identical to filesystem state"
    );

    // DB status for ch01 must remain draft (SSOT).
    let ch1 = nexus_local_db::work_chapters::get_chapter(&pool, work_id, 1, 1)
        .await
        .unwrap()
        .expect("ch1 row should exist");
    assert_eq!(
        ch1.status, "draft",
        "DB status must win over filesystem frontmatter per §4.5.3"
    );

    // File content for ch01 must be preserved (body still readable).
    let ch1_file = std::fs::read_to_string(stories_dir.join("ch01-finalized.md")).unwrap();
    assert!(
        ch1_file.contains("Draft body still here."),
        "reconcile must preserve chapter file content"
    );

    // After reconcile, file frontmatter should be re-synced to DB status.
    assert!(
        ch1_file.contains("status: draft"),
        "reconcile should re-sync file frontmatter to DB status per §4.5.3"
    );

    // ch02 row should have been created from the file.
    let ch2 = nexus_local_db::work_chapters::get_chapter(&pool, work_id, 2, 1)
        .await
        .unwrap()
        .expect("ch2 row should be created from file");
    assert_eq!(ch2.status, "not_started");
    assert_eq!(
        ch2.body_path.as_deref(),
        Some("Works/reconcile-novel/Stories/ch02-extra.md")
    );
}
