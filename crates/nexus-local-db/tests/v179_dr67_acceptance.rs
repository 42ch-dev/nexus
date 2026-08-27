//! DR-67 (v1.179) acceptance tests — novel-workflow-profile §4.5.7 items #1–#5.
//!
//! Pins the five named chapter-state invariants against today's
//! implementation (hermetic DB-level suites, v142 file style):
//!
//! 1. Chapter selection — `next_chapter(work_id)` returns the lowest
//!    eligible row per §4.5.2.
//! 2. `current_chapter` transitions — changes only on finalize, becoming the
//!    just-finalized chapter number (§4.5.2 work-level invariant).
//! 3. Novel completion — fires only when the row set covers
//!    `total_planned_chapters`, every row is `finalized`, and
//!    `intake_status == complete` (V1.44 F-002 volume-aware row-count
//!    contract implemented by `is_work_completed`).
//! 4. Resume — a new run against a Work with one `draft` row resumes that
//!    row and does not create a new row (§4.5.2 / §5.4.5).
//! 5. Reconciliation — reconcile rebuilds missing `work_chapters` rows/files
//!    from `Works/<work_ref>/Stories/` while preserving DB-as-status-SSOT
//!    conflict resolution (§4.5.3).
//!
//! Layer note (#2): the DB layer (`update_status`) never mutates
//! `works.current_chapter` by itself; the finalize advance is an explicit
//! `patch_work(current_chapter = N)` performed by the
//! `novel.chapter_transition` capability (nexus-orchestration) after a
//! successful finalize. This suite pins the DB-visible contract of that
//! split: non-finalize transitions leave `current_chapter` untouched, and
//! the documented finalize advance lands as the just-finalized number.

use nexus_local_db::work_chapters::{
    count_chapters, is_work_completed, next_chapter, reconcile_from_filesystem, seed_chapters,
    update_status,
};
use nexus_local_db::works::WorkPatch;

/// Timestamp helper (ISO 8601, matches seed/update signatures).
const NOW: &str = "2026-08-28T00:00:00Z";

/// Insert a minimal works row (v142 fixture style) with configurable
/// `total_planned_chapters` / `current_chapter` / `intake_status`.
#[allow(clippy::too_many_arguments)]
async fn insert_work(
    pool: &sqlx::SqlitePool,
    work_id: &str,
    total: i32,
    current_chapter: i32,
    intake_status: &str,
) {
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
         long_term_goal, initial_idea, intake_status, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at,
         current_stage, stage_status, current_chapter, total_planned_chapters,
         auto_chain_enabled, auto_chain_interrupted, auto_review_master_on_timeout)
         VALUES (?, 'ctr_dr67', 'ws', 'active', 'DR-67', 'goal', 'idea', ?,
         '[]', 'novel-writing', '[]', ?, ?, 'produce', 'active', ?, ?, 1, 0, 0)",
    )
    .bind(work_id)
    .bind(intake_status)
    .bind(NOW)
    .bind(NOW)
    .bind(current_chapter)
    .bind(total)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert one chapter row at a given status.
async fn insert_chapter_row(pool: &sqlx::SqlitePool, work_id: &str, chapter: i32, status: &str) {
    sqlx::query(
        "INSERT INTO work_chapters
         (work_id, volume, chapter, slug, status, created_at, updated_at)
         VALUES (?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(work_id)
    .bind(chapter)
    .bind(format!("ch{chapter:02}"))
    .bind(status)
    .bind(NOW)
    .bind(NOW)
    .execute(pool)
    .await
    .unwrap();
}

/// Read `works.current_chapter` back.
async fn read_current_chapter(pool: &sqlx::SqlitePool, work_id: &str) -> i32 {
    sqlx::query_scalar("SELECT current_chapter FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Read one chapter's status back.
async fn read_chapter_status(pool: &sqlx::SqlitePool, work_id: &str, chapter: i32) -> String {
    sqlx::query_scalar(
        "SELECT status FROM work_chapters WHERE work_id = ? AND volume = 1 AND chapter = ?",
    )
    .bind(work_id)
    .bind(chapter)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// §4.5.7 #1 — Chapter selection: mixed-status 3-chapter Works select the
/// lowest eligible row per §4.5.2.
#[tokio::test]
async fn w179_dr67_01_next_chapter_selects_lowest_eligible() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Work A: ch1 finalized, ch2 draft, ch3 not_started.
    // Lowest eligible is the ch2 draft (resume rule: no EARLIER not_started
    // row exists), not the later not_started ch3.
    insert_work(&pool, "dr67_sel_a", 3, 1, "complete").await;
    insert_chapter_row(&pool, "dr67_sel_a", 1, "finalized").await;
    insert_chapter_row(&pool, "dr67_sel_a", 2, "draft").await;
    insert_chapter_row(&pool, "dr67_sel_a", 3, "not_started").await;
    assert_eq!(
        next_chapter(&pool, "dr67_sel_a").await.unwrap(),
        Some(2),
        "§4.5.2: draft with no earlier not_started row is selected"
    );

    // Work B: ch1/ch2 finalized, ch3 not_started → selects ch3.
    insert_work(&pool, "dr67_sel_b", 3, 2, "complete").await;
    insert_chapter_row(&pool, "dr67_sel_b", 1, "finalized").await;
    insert_chapter_row(&pool, "dr67_sel_b", 2, "finalized").await;
    insert_chapter_row(&pool, "dr67_sel_b", 3, "not_started").await;
    assert_eq!(
        next_chapter(&pool, "dr67_sel_b").await.unwrap(),
        Some(3),
        "§4.5.2: lowest not_started row is selected"
    );

    // Work C: all finalized → None (novel-completion signal, §4.5.2 step 3).
    insert_work(&pool, "dr67_sel_c", 3, 3, "complete").await;
    for ch in 1..=3 {
        insert_chapter_row(&pool, "dr67_sel_c", ch, "finalized").await;
    }
    assert_eq!(
        next_chapter(&pool, "dr67_sel_c").await.unwrap(),
        None,
        "§4.5.2: no eligible row → novel-completion"
    );
}

/// §4.5.7 #2 — `current_chapter` changes only on finalize and then equals the
/// just-finalized chapter number (§4.5.2 work-level invariant; see the
/// module-level layer note).
#[tokio::test]
async fn w179_dr67_02_current_chapter_advances_only_on_finalize() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Work starts at ch1 finalized (current_chapter = 1); ch2 in progress.
    insert_work(&pool, "dr67_cc", 3, 1, "complete").await;
    insert_chapter_row(&pool, "dr67_cc", 1, "finalized").await;
    insert_chapter_row(&pool, "dr67_cc", 2, "not_started").await;
    insert_chapter_row(&pool, "dr67_cc", 3, "not_started").await;
    assert_eq!(read_current_chapter(&pool, "dr67_cc").await, 1);

    // Non-finalize transitions must NOT move current_chapter.
    update_status(&pool, "dr67_cc", 2, 1, "outlined", None, NOW)
        .await
        .unwrap();
    update_status(&pool, "dr67_cc", 2, 1, "draft", None, NOW)
        .await
        .unwrap();
    assert_eq!(
        read_current_chapter(&pool, "dr67_cc").await,
        1,
        "§4.5.2: drafting must not advance current_chapter"
    );
    assert_eq!(read_chapter_status(&pool, "dr67_cc", 2).await, "draft");

    // Finalize ch2: the transition lands, then the finalize advance
    // (patch_work(current_chapter) performed by novel.chapter_transition)
    // sets it to the just-finalized number.
    update_status(&pool, "dr67_cc", 2, 1, "finalized", Some(4321), NOW)
        .await
        .unwrap();
    let patch = WorkPatch {
        current_chapter: Some(2),
        ..WorkPatch::default()
    };
    nexus_local_db::works::patch_work(&pool, "ctr_dr67", "dr67_cc", &patch, NOW)
        .await
        .unwrap();
    assert_eq!(
        read_current_chapter(&pool, "dr67_cc").await,
        2,
        "§4.5.2: current_chapter becomes the just-finalized chapter number"
    );

    // The latest-finalized pointer wins: finalizing ch3 moves it to 3.
    update_status(&pool, "dr67_cc", 3, 1, "finalized", None, NOW)
        .await
        .unwrap();
    let patch = WorkPatch {
        current_chapter: Some(3),
        ..WorkPatch::default()
    };
    nexus_local_db::works::patch_work(&pool, "ctr_dr67", "dr67_cc", &patch, NOW)
        .await
        .unwrap();
    assert_eq!(read_current_chapter(&pool, "dr67_cc").await, 3);
}

/// §4.5.7 #3 — Completion fires only when the row set covers
/// `total_planned_chapters`, every row is `finalized`, and
/// `intake_status == complete` — the V1.44 F-002 volume-aware row-count
/// contract implemented by `is_work_completed` (`work_chapters.rs`). Per the
/// workflow-profile.md §4.5.7 annotation, §6.1's
/// `current_chapter >= total_planned_chapters` conjunct is NOT enforced by
/// this DAO today (spec-pointer gap, historical) — every fixture below with
/// a full finalized row set sets `current_chapter == total`, which keeps the
/// unenforced conjunct out of the assertion path by construction.
#[tokio::test]
async fn w179_dr67_03_completion_requires_all_rows_finalized_count_match_and_intake() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Complete: 3 rows exist == total_planned_chapters 3, all finalized,
    // intake complete → complete.
    insert_work(&pool, "dr67_done", 3, 3, "complete").await;
    for ch in 1..=3 {
        insert_chapter_row(&pool, "dr67_done", ch, "finalized").await;
    }
    assert!(
        is_work_completed(&pool, "dr67_done").await.unwrap(),
        "V1.44 row-count contract: rows cover total, all finalized, intake \
         complete → complete"
    );

    // Missing intake: identical row set except intake_status pending → NOT
    // complete.
    insert_work(&pool, "dr67_intake", 3, 3, "pending").await;
    for ch in 1..=3 {
        insert_chapter_row(&pool, "dr67_intake", ch, "finalized").await;
    }
    assert!(
        !is_work_completed(&pool, "dr67_intake").await.unwrap(),
        "intake_status != complete blocks completion"
    );

    // Un-finalized row: ch3 still draft → NOT complete.
    insert_work(&pool, "dr67_draft", 3, 2, "complete").await;
    insert_chapter_row(&pool, "dr67_draft", 1, "finalized").await;
    insert_chapter_row(&pool, "dr67_draft", 2, "finalized").await;
    insert_chapter_row(&pool, "dr67_draft", 3, "draft").await;
    assert!(
        !is_work_completed(&pool, "dr67_draft").await.unwrap(),
        "a non-finalized row blocks completion"
    );

    // Under-seeded: total_planned_chapters = 4 but only 3 rows exist (count
    // mismatch) → NOT complete.
    insert_work(&pool, "dr67_under", 4, 3, "complete").await;
    for ch in 1..=3 {
        insert_chapter_row(&pool, "dr67_under", ch, "finalized").await;
    }
    assert!(
        !is_work_completed(&pool, "dr67_under").await.unwrap(),
        "the row set must cover every planned chapter before completion"
    );
}

/// §4.5.7 #4 — Resume: a new run against a Work with one `draft` row resumes
/// that row and does not create a new row (§4.5.2 resume + §5.4.5 seeding
/// idempotency).
#[tokio::test]
async fn w179_dr67_04_resume_reuses_draft_row_without_creating_new() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    insert_work(&pool, "dr67_resume", 3, 1, "complete").await;
    seed_chapters(&pool, "dr67_resume", "wrk_dr67", 3, NOW)
        .await
        .unwrap();
    // First run finalized ch1 and left ch2 as a draft.
    update_status(&pool, "dr67_resume", 1, 1, "finalized", None, NOW)
        .await
        .unwrap();
    update_status(&pool, "dr67_resume", 2, 1, "draft", None, NOW)
        .await
        .unwrap();

    // A new run selects the existing draft row.
    assert_eq!(
        next_chapter(&pool, "dr67_resume").await.unwrap(),
        Some(2),
        "§4.5.2: the draft row is resumed"
    );

    // Re-running the seeding path (init idempotency, §5.4.5) must not create
    // a new row nor reset the draft.
    seed_chapters(&pool, "dr67_resume", "wrk_dr67", 3, NOW)
        .await
        .unwrap();
    assert_eq!(
        count_chapters(&pool, "dr67_resume").await.unwrap(),
        3,
        "no new chapter row may be created by a new run"
    );
    assert_eq!(
        read_chapter_status(&pool, "dr67_resume", 2).await,
        "draft",
        "resume must not reset the in-progress draft row"
    );
    assert_eq!(
        next_chapter(&pool, "dr67_resume").await.unwrap(),
        Some(2),
        "selection still points at the same draft row"
    );
}

/// §4.5.7 #5 — Reconciliation rebuilds missing rows/files from
/// `Works/<work_ref>/Stories/` while preserving DB-as-status-SSOT (§4.5.3).
#[tokio::test]
async fn w179_dr67_05_reconcile_rebuilds_missing_rows_with_db_status_ssot() {
    let dir = tempfile::tempdir().unwrap();
    let workspace_root = dir.path();
    let work_id = "dr67_recon";
    let work_ref = "wrk_dr67";

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp_db.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    insert_work(&pool, work_id, 2, 1, "complete").await;
    // ch1 exists in DB as finalized (the DB is the status SSOT).
    insert_chapter_row(&pool, work_id, 1, "finalized").await;
    // ch2 has a file but NO DB row (missing row to rebuild).

    let stories = workspace_root.join("Works").join(work_ref).join("Stories");
    std::fs::create_dir_all(&stories).unwrap();

    // ch1 file frontmatter DISAGREES with the DB (draft vs finalized):
    // §4.5.3 — the DB wins and the file is re-synced to the DB status.
    std::fs::write(
        stories.join("ch01-first.md"),
        "---\nstatus: draft\nword_count: 1000\n---\n\nChapter one body.\n",
    )
    .unwrap();

    // ch2 file exists with frontmatter but no DB row → rebuild path.
    std::fs::write(
        stories.join("ch02-second.md"),
        "---\nstatus: draft\nword_count: 2500\n---\n\nChapter two body.\n",
    )
    .unwrap();

    let report = reconcile_from_filesystem(&pool, work_id, work_ref, workspace_root, NOW, false)
        .await
        .unwrap();
    assert_eq!(report.created, 1, "missing ch2 row is rebuilt from file");
    assert_eq!(
        report.resynced, 1,
        "conflicting ch1 frontmatter is re-synced"
    );

    // Rebuilt row: chapter number from the filename, frontmatter status
    // mirrored (no DB row existed to disagree with).
    assert_eq!(
        read_chapter_status(&pool, work_id, 2).await,
        "draft",
        "rebuild applies the file frontmatter status when no DB row exists"
    );

    // DB-as-SSOT: ch1's DB row is untouched by reconcile...
    assert_eq!(
        read_chapter_status(&pool, work_id, 1).await,
        "finalized",
        "§4.5.3: DB row wins — reconcile must not change DB status"
    );
    // ...and the file frontmatter was re-synced to the DB status.
    let ch1 = std::fs::read_to_string(stories.join("ch01-first.md")).unwrap();
    assert!(
        ch1.contains("status: finalized"),
        "§4.5.3: frontmatter re-synced to the DB status; got: {ch1}"
    );

    // Selection after reconcile sees the rebuilt row (2 = the draft).
    assert_eq!(next_chapter(&pool, work_id).await.unwrap(), Some(2));
}
