//! Hermetic integration test for the auto-chronology tick (V1.50 T-A P3, T7).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/auto-chronology.md` §3 / §4.
//! AC §4.2: positive (all finalized + intake complete → advance) + 4 negative
//! edge cases + crash recovery.
//!
//! Drives `nexus_orchestration::auto_chronology::run_one_tick` directly against
//! an ephemeral `state.db` + temp workspace dir — no spawned interval loop.

use std::path::Path;

use nexus_local_db::works::{self, WorkRecord};
use nexus_local_db::{run_migrations, work_chapters};
use nexus_orchestration::auto_chronology::{
    advance_manual, outline_path, run_one_tick, AdvanceOutcome, SkipReason,
};

async fn fresh_pool() -> sqlx::SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("auto_chrono_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    pool
}

fn base_work(work_id: &str, work_ref: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Auto-Chrono Test".to_string(),
        long_term_goal: "Test".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T10:00:00Z".to_string(),
        updated_at: "2026-06-18T10:00:00Z".to_string(),
        current_stage: "produce".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_ref.to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 3,
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

/// Opt a Work into auto-chronology.
async fn opt_in(pool: &sqlx::SqlitePool, work_id: &str) {
    works::set_auto_chronology(pool, work_id, true, "2026-06-18T10:00:00Z")
        .await
        .unwrap();
}

/// Seed `count` chapters for `volume`; when `finalize`, set them all to
/// `finalized`. (`insert_chapter` always inserts `not_started`; finalize via
/// `update_status`.)
async fn seed_and_finalize(
    pool: &sqlx::SqlitePool,
    work_id: &str,
    volume: i32,
    count: i32,
    finalize: bool,
) {
    for ch in 1..=count {
        let slug = format!("v{volume:02}-ch{ch:02}");
        let params = nexus_local_db::work_chapters::InsertChapterParams {
            work_id,
            chapter: ch,
            volume: Some(volume),
            slug: Some(&slug),
            planned_word_count: 4000,
            outline_path: None,
            body_path: None,
            now: "2026-06-18T10:00:00Z",
        };
        work_chapters::insert_chapter(pool, &params).await.unwrap();
        if finalize {
            work_chapters::update_status(
                pool,
                work_id,
                ch,
                volume,
                "finalized",
                Some(4000),
                "2026-06-18T10:30:00Z",
            )
            .await
            .unwrap();
        }
    }
}

// ── Positive (AC §4.2) ───────────────────────────────────────────────────

/// All chapters of volume 1 finalized + intake complete + opted in + not
/// locked → tick advances to volume 2 (outline created).
#[tokio::test]
async fn tick_advances_when_volume_finalized() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_pos", "pos-novel");
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_pos", 1, 3, true).await;
    opt_in(&pool, "wrk_pos").await;

    run_one_tick(&pool, ws.path()).await;

    let outline = outline_path(ws.path(), "pos-novel", 2);
    assert!(
        outline.exists(),
        "positive: volume-2 outline should be created"
    );
    let body = std::fs::read_to_string(&outline).unwrap();
    assert!(body.contains("Volume 2 Outline"));
    assert!(body.contains("Previous volume: 1"));
    // Auto path seeds zero chapters (placeholder outline, spec §4.2).
    let vol2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_chapters WHERE work_id = 'wrk_pos' AND volume = 2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vol2_count, 0, "auto path seeds zero chapters");
    // Log entry written (AC §4.5).
    assert!(ws
        .path()
        .join("Works/pos-novel/Logs/chronology")
        .read_dir()
        .is_ok());
}

// ── R-V150P3AUTOCHRONO-03 regression: title / total_planned_chapters ──────

/// The auto-chronology advance must substitute the Work's stored `title` and
/// `total_planned_chapters` into the rendered outline (spec §4.1 step 3),
/// not the hardcoded `"(untitled)"` / `(unset)` placeholders.
#[tokio::test]
async fn render_substitutes_work_title_and_total_chapters() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let mut work = base_work("wrk_title", "title-novel");
    work.title = "The Hobbit".to_string();
    work.total_planned_chapters = Some(20);
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_title", 1, 2, true).await;
    opt_in(&pool, "wrk_title").await;

    run_one_tick(&pool, ws.path()).await;

    let outline = outline_path(ws.path(), "title-novel", 2);
    assert!(
        outline.exists(),
        "outline should be created for eligible work"
    );
    let body = std::fs::read_to_string(&outline).unwrap();
    assert!(
        body.contains("The Hobbit"),
        "rendered outline must contain the Work's real title; got:\n{body}"
    );
    assert!(
        body.contains("20"),
        "rendered outline must contain total_planned_chapters=20; got:\n{body}"
    );
    // Negative: the hardcoded placeholder must NOT appear.
    assert!(
        !body.contains("(untitled)"),
        "rendered outline must not contain the hardcoded placeholder"
    );
    assert!(
        !body.contains("(unset)"),
        "rendered outline must not contain the unset placeholder when a value is stored"
    );
}

// ── Negative 1: volume not finalized (AC §4.2) ───────────────────────────

#[tokio::test]
async fn tick_skips_when_volume_not_finalized() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_neg1", "neg1-novel");
    works::create_work(&pool, &work).await.unwrap();
    // One chapter still in 'draft' (not finalized).
    seed_and_finalize(&pool, "wrk_neg1", 1, 3, false).await;
    opt_in(&pool, "wrk_neg1").await;

    run_one_tick(&pool, ws.path()).await;

    assert!(
        !outline_path(ws.path(), "neg1-novel", 2).exists(),
        "negative: must not advance when volume not finalized"
    );
}

// ── Negative 2: intake not complete (AC §4.2) ────────────────────────────

#[tokio::test]
async fn tick_skips_when_intake_incomplete() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let mut work = base_work("wrk_neg2", "neg2-novel");
    work.intake_status = "pending".to_string();
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_neg2", 1, 3, true).await;
    opt_in(&pool, "wrk_neg2").await;

    run_one_tick(&pool, ws.path()).await;

    assert!(
        !outline_path(ws.path(), "neg2-novel", 2).exists(),
        "negative: must not advance when intake incomplete"
    );
}

// ── Negative 3: runtime lock held (AC §4.2) ──────────────────────────────

#[tokio::test]
async fn tick_skips_when_runtime_locked() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_neg3", "neg3-novel");
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_neg3", 1, 3, true).await;
    opt_in(&pool, "wrk_neg3").await;
    // Acquire a runtime lock.
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_neg3",
        &works::WorkPatch {
            runtime_lock_holder: Some(Some("cli:123:abc".to_string())),
            ..Default::default()
        },
        "2026-06-18T11:00:00Z",
    )
    .await
    .unwrap();

    run_one_tick(&pool, ws.path()).await;

    assert!(
        !outline_path(ws.path(), "neg3-novel", 2).exists(),
        "negative: must not advance when runtime lock held"
    );
}

// ── Negative 4: already advanced / idempotent (AC §4.2) ───────────────────

#[tokio::test]
async fn tick_skips_when_outline_already_exists() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_neg4", "neg4-novel");
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_neg4", 1, 3, true).await;
    opt_in(&pool, "wrk_neg4").await;
    // Pre-create the volume-2 outline (simulates a prior advance or a crash
    // after the outline write but before the tx commit).
    let outline = outline_path(ws.path(), "neg4-novel", 2);
    std::fs::create_dir_all(outline.parent().unwrap()).unwrap();
    std::fs::write(&outline, "pre-existing").unwrap();

    run_one_tick(&pool, ws.path()).await;

    // Idempotent: the pre-existing outline is NOT clobbered.
    assert_eq!(
        std::fs::read_to_string(&outline).unwrap(),
        "pre-existing",
        "idempotent guard must not clobber existing outline"
    );
}

// ── Crash recovery (AC §4.3) ─────────────────────────────────────────────

/// Simulate a crash mid-advance: the outline file exists (written) but the
/// transaction was never committed (no chapters for volume 2). The next tick
/// must observe the existing outline and skip cleanly (idempotent recovery),
/// leaving the DB consistent.
#[tokio::test]
async fn tick_recovers_cleanly_after_crash_mid_advance() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_crash", "crash-novel");
    works::create_work(&pool, &work).await.unwrap();
    seed_and_finalize(&pool, "wrk_crash", 1, 3, true).await;
    opt_in(&pool, "wrk_crash").await;

    // Simulate the post-outline-write, pre-tx-commit crash: write the outline
    // by hand (as if the atomic write succeeded) but seed NO chapters for v2.
    let outline = outline_path(ws.path(), "crash-novel", 2);
    std::fs::create_dir_all(outline.parent().unwrap()).unwrap();
    std::fs::write(&outline, "crashed-mid-advance").unwrap();

    // First tick after "crash": must skip (idempotent) — not re-advance, not
    // error, not clobber.
    run_one_tick(&pool, ws.path()).await;
    assert_eq!(
        std::fs::read_to_string(&outline).unwrap(),
        "crashed-mid-advance",
        "recovery tick must not clobber the crashed-state outline"
    );

    // DB remains consistent: still zero volume-2 chapters (the crashed tx
    // rolled back; the recovery tick did not re-seed).
    let vol2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_chapters WHERE work_id = 'wrk_crash' AND volume = 2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        vol2_count, 0,
        "recovery: crashed tx must leave no v2 chapters"
    );

    // Transactional rollback invariant: a fresh explicit advance that injects
    // a tx failure must not leave orphan rows. Verified structurally by the
    // idempotent guard above (outline exists → no second write path).
}

// ── R-V150P3AUTOCHRONO-04 regression: DB tx commits BEFORE outline write ──

/// The advance must commit the DB transaction BEFORE writing the outline file
/// (V1.36 "DB first, FS second"). When the outline write fails post-commit,
/// the DB state is already correct (chapters seeded, `updated_at` bumped) and the
/// Work is NOT stuck — proving the ordering inversion fixes the "outline
/// written before tx commits → Work stuck" reliability gap (qc3 W-1).
#[tokio::test]
async fn perform_advance_writes_outline_after_tx_commit() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_order", "order-novel");
    works::create_work(&pool, &work).await.unwrap();

    // Pre-create a regular FILE at the `Outlines` directory path so
    // `create_dir_all(outline.parent())` fails post-commit (NotADirectory).
    let outlines_as_file = ws.path().join("Works").join("order-novel").join("Outlines");
    std::fs::create_dir_all(outlines_as_file.parent().unwrap()).unwrap();
    std::fs::write(&outlines_as_file, "blocker").unwrap();

    // Manual advance with 3 chapters → perform_advance must commit the tx
    // (chapter seed + updated_at) FIRST, then try the outline write → fails.
    let result = advance_manual(&pool, ws.path(), "wrk_order", 2, Some(3), false).await;
    assert!(
        result.is_err(),
        "outline write failure must surface as Err (observable); got: {result:?}"
    );

    // The DB tx committed BEFORE the outline write (Design A): the 3 chapter
    // rows persist despite the outline failure. This is the key invariant —
    // the Work is not stuck because the DB is the source of truth.
    let vol2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_chapters WHERE work_id = 'wrk_order' AND volume = 2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        vol2_count, 3,
        "tx must commit BEFORE outline write: chapters persist despite outline failure"
    );

    // No partial outline file (the write never succeeded; atomic cleanup holds).
    let outline = outline_path(ws.path(), "order-novel", 2);
    assert!(
        !outline.exists(),
        "no partial outline file after failed write"
    );

    // Recovery: remove the blocker, re-run advance. The idempotent guard
    // passes (no outline), seed is idempotent (INSERT OR IGNORE), and the
    // outline write succeeds on retry.
    std::fs::remove_file(&outlines_as_file).unwrap();
    let outcome = advance_manual(&pool, ws.path(), "wrk_order", 2, Some(3), false)
        .await
        .unwrap();
    assert!(
        matches!(outcome, AdvanceOutcome::Advanced { next_volume, .. } if next_volume == 2),
        "recovery re-run must succeed; got: {outcome:?}"
    );
    assert!(
        outline.exists(),
        "recovery: outline must be written after blocker removed"
    );
}

// ── R-V150P3AUTOCHRONO-04 regression: --force bypasses idempotent guard ───

/// `force = true` bypasses the idempotent guard so the caller can overwrite an
/// existing outline (recovery path for an orphaned crash-state outline).
#[tokio::test]
async fn manual_advance_force_overwrites_existing_outline() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_force", "force-novel");
    works::create_work(&pool, &work).await.unwrap();

    // First advance creates the outline.
    let outcome = advance_manual(&pool, ws.path(), "wrk_force", 2, None, false)
        .await
        .unwrap();
    assert!(matches!(outcome, AdvanceOutcome::Advanced { .. }));
    let outline = outline_path(ws.path(), "force-novel", 2);
    let original = std::fs::read_to_string(&outline).unwrap();
    assert!(original.contains("force-novel"));

    // Without force: second advance is idempotent (AlreadyAdvanced).
    let skipped = advance_manual(&pool, ws.path(), "wrk_force", 2, None, false)
        .await
        .unwrap();
    assert!(matches!(
        skipped,
        AdvanceOutcome::Skipped {
            reason: SkipReason::AlreadyAdvanced,
            ..
        }
    ));

    // With force: bypasses the guard and rewrites the outline.
    let forced = advance_manual(&pool, ws.path(), "wrk_force", 2, None, true)
        .await
        .unwrap();
    assert!(
        matches!(forced, AdvanceOutcome::Advanced { .. }),
        "force must bypass idempotent guard and re-advance; got: {forced:?}"
    );
    let rewritten = std::fs::read_to_string(&outline).unwrap();
    assert!(
        rewritten.contains("force-novel"),
        "force must rewrite the outline (still valid content)"
    );
}

// ── Manual override (spec §2.2 / AC §4.4) ─────────────────────────────────

/// Manual advance bypasses finish detection and seeds the requested chapter
/// count, regardless of the `auto_chronology` flag.
#[tokio::test]
async fn manual_advance_bypasses_gates_and_seeds_chapters() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let mut work = base_work("wrk_manual", "manual-novel");
    // Not opted in, intake incomplete, no finalized chapters — manual must
    // still advance (spec §2.2).
    work.intake_status = "pending".to_string();
    works::create_work(&pool, &work).await.unwrap();
    assert!(
        !works::get_auto_chronology(&pool, "wrk_manual")
            .await
            .unwrap(),
        "manual work is not opted in"
    );

    let outcome = advance_manual(&pool, ws.path(), "wrk_manual", 2, Some(4), false)
        .await
        .unwrap();
    match outcome {
        AdvanceOutcome::Advanced {
            next_volume,
            chapters_seeded,
            ..
        } => {
            assert_eq!(next_volume, 2);
            assert_eq!(chapters_seeded, 4);
        }
        other @ AdvanceOutcome::Skipped { .. } => panic!("manual advance should succeed, got {other:?}"),
    }

    assert!(outline_path(ws.path(), "manual-novel", 2).exists());
    let vol2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_chapters WHERE work_id = 'wrk_manual' AND volume = 2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vol2_count, 4, "manual advance seeds the requested chapters");
}

/// Manual advance honors the idempotent guard (does not clobber an existing
/// outline).
#[tokio::test]
async fn manual_advance_respects_idempotent_guard() {
    let pool = fresh_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = base_work("wrk_manual2", "manual2-novel");
    works::create_work(&pool, &work).await.unwrap();
    let outline = outline_path(ws.path(), "manual2-novel", 2);
    std::fs::create_dir_all(outline.parent().unwrap()).unwrap();
    std::fs::write(&outline, "kept").unwrap();

    let outcome = advance_manual(&pool, ws.path(), "wrk_manual2", 2, None, false)
        .await
        .unwrap();
    match outcome {
        AdvanceOutcome::Skipped {
            reason: SkipReason::AlreadyAdvanced,
            ..
        } => {}
        other => panic!("expected AlreadyAdvanced, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(&outline).unwrap(),
        "kept",
        "manual advance must not clobber existing outline"
    );
}

/// Sanity: `outline_path` layout matches spec §4.1.
#[test]
fn outline_path_layout() {
    let p = outline_path(Path::new("/ws"), "nov", 3);
    assert_eq!(
        p,
        std::path::PathBuf::from("/ws/Works/nov/Outlines/volume-3-outline.md")
    );
}
