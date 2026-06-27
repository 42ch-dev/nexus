//! V1.51 T-A P1 — `creator kb rescan --work <work_ref>` cross-chapter
//! reconciliation hermetic round-trip.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-cross-chapter-rescan.md`
//! Spec: `.mstar/knowledge/world-kb-runtime-architecture.md` §5.5.1
//! CLI:   `.mstar/knowledge/specs/cli-spec.md` §6.2G (`--work` amendment)
//!
//! Drives `nexus42::commands::creator::kb::rescan::kb_rescan_work_hermetic`
//! against a fresh temp DB + temp workspace so the work-scoped rescan can be
//! exercised without `$HOME` or a daemon.
//!
//! Covers:
//! - **AC1**: work-scoped rescan performs cross-chapter reconciliation.
//! - **AC3**: `--dry-run` shows cross-chapter reuse summary without writing.
//! - **AC4**: 3 chapters same canonical entity → 1 updated row (not 3 pending);
//!   3 chapters distinct entities → 3 pending rows; existing KB match → refresh.
//! - **AC5**: advisory lock contention → `CliError::Locked` (exit 75);
//!   I/O failure → `CliError::LockIo` (exit 78).
//!
//! Run with: cargo test -p nexus42 --test kb_rescan

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::kb::rescan::kb_rescan_work_hermetic;
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::list_pending_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use std::path::Path;

const OWNER: &str = "ctr_owner";
const WORLD: &str = "wld_xrescan";
const WORK_ID: &str = "wrk_xrescan";
/// Human-readable `work_ref` (= `works.story_ref`) the CLI targets.
const WORK_REF: &str = "xrescan-novel";

/// Build a fresh migrated pool + seed a world (owned by OWNER), a works row
/// (with `story_ref = WORK_REF` + `world_id = WORLD`). Caller seeds chapters.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Cross-chapter Rescan World",
        "xrescan-world",
        "private",
        "manual",
    )
    .await;
    // SAFETY: test-only seed against the known works table schema.
    sqlx::query(
        "INSERT OR IGNORE INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, world_id, story_ref, created_at, updated_at) \
         VALUES (?, ?, 'ws', 'active', 'Cross-chapter Novel', 'goal', 'idea', 'complete', \
                 ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORK_REF)
    .execute(&pool)
    .await
    .unwrap();
    (pool, dir)
}

/// Seed a `work_chapters` row for `chapter` (volume 1, finalized) pointing at
/// `Works/<WORK_REF>/Stories/<NN>-chapter.md` and write the prose.
async fn seed_chapter(pool: &sqlx::SqlitePool, ws_dir: &Path, chapter: i32, prose: &str) {
    let rel = format!("Works/{WORK_REF}/Stories/{chapter:02}-chapter.md");
    let path = ws_dir.join(&rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, prose).unwrap();
    // SAFETY: test-only seed against the known work_chapters table schema.
    sqlx::query(
        "INSERT OR REPLACE INTO work_chapters \
         (work_id, chapter, volume, slug, planned_word_count, status, body_path, \
          created_at, updated_at) \
         VALUES (?, ?, 1, 'chapter', 100, 'finalized', ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(chapter)
    .bind(&rel)
    .execute(pool)
    .await
    .unwrap();
}

// ── AC1 + AC4: 3 chapters same entity → 1 pending row (not 3) ─────────────

#[tokio::test]
async fn cross_chapter_same_entity_collapses_to_one_pending_row() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Aelin surveyed the quiet harbor.").await;
    seed_chapter(&pool, dir.path(), 3, "Aelin opened the eastern gate.").await;

    let report = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();

    // Cross-chapter aggregation collapsed "Aelin" into one aggregate.
    assert_eq!(
        report.chapters_scanned,
        vec![1, 2, 3],
        "all three chapters must be scanned"
    );
    assert!(
        report.candidates_inserted.iter().any(|n| n == "Aelin"),
        "Aelin aggregate should be inserted: {:?}",
        report.candidates_inserted
    );

    // AC4: exactly ONE pending row (not three) — DB uniqueness collapsed them.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let aelin_rows: Vec<_> = pending
        .iter()
        .filter(|r| r.canonical_name_guess.as_deref() == Some("Aelin"))
        .collect();
    assert_eq!(
        aelin_rows.len(),
        1,
        "3 chapters referencing 'Aelin' must collapse to 1 pending row, got {}: {aelin_rows:?}",
        aelin_rows.len()
    );

    // The merged row carries cross-chapter provenance.
    let row = aelin_rows[0];
    let payload: serde_json::Value =
        serde_json::from_str(row.proposed_payload.as_deref().unwrap_or("{}")).unwrap();
    assert_eq!(
        payload["source_chapters"],
        serde_json::json!([1, 2, 3]),
        "merged row payload must carry source_chapters [1,2,3]: {payload}"
    );
}

// ── AC4 complement: 3 distinct entities → 3 pending rows ──────────────────

#[tokio::test]
async fn cross_chapter_distinct_entities_produce_separate_pending_rows() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Bran surveyed the harbor.").await;
    seed_chapter(&pool, dir.path(), 3, "Cael opened the eastern gate.").await;

    let report = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    assert_eq!(report.candidates_inserted.len(), 3);

    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(
        pending.len(),
        3,
        "3 distinct entities → 3 pending rows, got {}: {pending:?}",
        pending.len()
    );
}

// ── AC3: dry-run shows cross-chapter reuse summary without writing ─────────

#[tokio::test]
async fn cross_chapter_dry_run_shows_reuse_summary_without_writing() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Aelin surveyed the harbor.").await;
    seed_chapter(&pool, dir.path(), 3, "Aelin opened the gate.").await;

    let dry = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, true)
        .await
        .unwrap();
    assert!(dry.dry_run);
    assert_eq!(dry.chapters_scanned, vec![1, 2, 3]);

    // Cross-chapter reuse summary names Aelin + the 3 chapters.
    let aelin_reuse = dry
        .cross_chapter_reuse
        .iter()
        .find(|r| r.canonical_name == "Aelin")
        .unwrap_or_else(|| {
            panic!(
                "dry-run must list Aelin reuse: {:?}",
                dry.cross_chapter_reuse
            )
        });
    assert_eq!(aelin_reuse.source_chapters, vec![1, 2, 3]);
    assert!(
        !aelin_reuse.existing_kb_row,
        "no confirmed KeyBlock yet → existing_kb_row false"
    );

    // Nothing was written: no pending candidate, no KeyBlock.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(
        pending.is_empty(),
        "dry-run must not write candidates, got {pending:?}"
    );
    let blocks = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(blocks.is_empty(), "dry-run must not write KeyBlocks");
}

// ── AC4: existing confirmed KeyBlock match → body refresh, candidate reuse ─

#[tokio::test]
async fn cross_chapter_existing_kb_match_refreshes_body() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;

    // First scan: produces a pending "Aelin" candidate.
    kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let aelin = pending
        .iter()
        .find(|r| r.canonical_name_guess.as_deref() == Some("Aelin"))
        .unwrap();
    // Adopt → confirmed KeyBlock.
    nexus42::commands::creator::world::kb::kb_adopt(
        &pool,
        OWNER,
        &aelin.job_id,
        Some(dir.path()),
        false,
    )
    .await
    .unwrap();

    // Add 2 more chapters referencing Aelin; rescan refreshes the KB body and
    // the reuse summary now reports an existing KB row.
    seed_chapter(&pool, dir.path(), 2, "Aelin surveyed the harbor.").await;
    seed_chapter(&pool, dir.path(), 3, "Aelin opened the gate.").await;
    let report = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();

    let aelin_reuse = report
        .cross_chapter_reuse
        .iter()
        .find(|r| r.canonical_name == "Aelin")
        .unwrap();
    assert!(
        aelin_reuse.existing_kb_row,
        "after adopt, dry/non-dry reuse summary must report existing KB row: {:?}",
        report.cross_chapter_reuse
    );
    // AC4: the confirmed row is terminal (§5.5.2) → the rescan must NOT create
    // a duplicate pending candidate for the same canonical_name.
    let pending_after = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let aelin_pending = pending_after
        .iter()
        .filter(|r| r.canonical_name_guess.as_deref() == Some("Aelin"))
        .count();
    assert_eq!(
        aelin_pending, 0,
        "confirmed row must not be duplicated as a new pending candidate"
    );
    // The confirmed KeyBlock is intact + singular.
    let blocks = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    let aelin_kb = blocks
        .iter()
        .filter(|kb| kb.canonical_name == "Aelin")
        .count();
    assert_eq!(aelin_kb, 1, "exactly one confirmed KeyBlock for Aelin");
}

// ── Reconciliation: stale candidate removed when name vanishes from all ────

#[tokio::test]
async fn cross_chapter_stale_candidate_removed_when_name_vanishes_from_all() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Bran surveyed the harbor.").await;

    kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(pending.len(), 2);

    // Edit BOTH chapters: drop "Aelin" and "Bran", add "Cael".
    seed_chapter(&pool, dir.path(), 1, "Cael walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Cael surveyed the harbor.").await;
    let report = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    assert!(
        report.candidates_removed.iter().any(|n| n == "Aelin"),
        "Aelin vanished from all chapters → removed: {:?}",
        report.candidates_removed
    );
    assert!(
        report.candidates_removed.iter().any(|n| n == "Bran"),
        "Bran vanished from all chapters → removed: {:?}",
        report.candidates_removed
    );

    let pending_after = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let names: Vec<_> = pending_after
        .iter()
        .filter_map(|r| r.canonical_name_guess.as_deref())
        .collect();
    assert!(!names.contains(&"Aelin"), "Aelin must be gone: {names:?}");
    assert!(!names.contains(&"Bran"), "Bran must be gone: {names:?}");
    assert!(names.contains(&"Cael"), "Cael must be present: {names:?}");
}

// ── Idempotent re-run on unchanged text → empty candidate diff ─────────────

#[tokio::test]
async fn cross_chapter_idempotent_rerun_produces_empty_candidate_diff() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;
    seed_chapter(&pool, dir.path(), 2, "Aelin surveyed the harbor.").await;

    let first = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    assert!(first.candidates_inserted.iter().any(|n| n == "Aelin"));

    // Re-run on unchanged text → no candidate changes.
    let second = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap();
    assert!(
        second.candidates_inserted.is_empty(),
        "idempotent rerun must not re-insert: {second:?}"
    );
    assert!(
        second.candidates_updated.is_empty(),
        "idempotent rerun must not update: {second:?}"
    );
    assert_eq!(second.candidates_unchanged, 1);
}

// ── AC5: advisory lock contention → CliError::Locked (exit 75) ─────────────

#[tokio::test]
async fn cross_chapter_lock_contention_returns_e_lock() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;

    // Hold the advisory lock from the test side first.
    let work_dir = dir.path().join("Works").join(WORK_REF);
    std::fs::create_dir_all(&work_dir).unwrap();
    let _guard = nexus_local_db::file_lock::try_acquire(&work_dir, "test:holder").unwrap();

    // Non-dry work rescan tries to acquire the same lock → Locked.
    let err = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap_err();
    match err {
        CliError::Locked { holder_name, .. } => {
            assert_eq!(holder_name, "test:holder");
        }
        other => panic!("expected CliError::Locked (exit 75), got {other:?}"),
    }

    // No candidate was written (the lock fires before the upsert).
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(pending.is_empty(), "lock contention must not write rows");
}

// ── AC5: dry-run never acquires the lock (read-only) ───────────────────────

#[tokio::test]
async fn cross_chapter_dry_run_succeeds_under_lock_contention() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;

    let work_dir = dir.path().join("Works").join(WORK_REF);
    std::fs::create_dir_all(&work_dir).unwrap();
    let _guard = nexus_local_db::file_lock::try_acquire(&work_dir, "test:holder").unwrap();

    // Dry-run is read-only → no lock acquired → succeeds.
    let dry = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, true)
        .await
        .unwrap();
    assert!(dry.dry_run);
}

// ── AC5: I/O failure → CliError::LockIo (exit 78) ──────────────────────────

#[tokio::test]
async fn cross_chapter_lock_io_failure_returns_e_lock_io() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked into the tavern.").await;

    // Create a DIRECTORY at the .lock path so opening it as a file fails with
    // an I/O error (EISDIR) → FileLockError::Io → CliError::LockIo (exit 78).
    let lock_path = dir.path().join("Works").join(WORK_REF).join(".lock");
    std::fs::create_dir_all(&lock_path).unwrap();

    let err = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), WORK_REF, false)
        .await
        .unwrap_err();
    match err {
        CliError::LockIo(_) => { /* expected — exit 78 */ }
        CliError::Locked { .. } => {
            panic!("expected CliError::LockIo (I/O failure → exit 78), got Locked (exit 75)");
        }
        other => panic!("expected CliError::LockIo (exit 78), got {other:?}"),
    }
}

// ── Error handling: missing work / worldless work ──────────────────────────

#[tokio::test]
async fn cross_chapter_missing_work_returns_clean_error() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked in.").await;
    let err = kb_rescan_work_hermetic(&pool, OWNER, Some(dir.path()), "no-such-work", false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => assert!(msg.contains("no-such-work")),
        other => panic!("expected Other error for missing work, got {other:?}"),
    }
}

#[tokio::test]
async fn cross_chapter_cross_author_returns_403() {
    let (pool, dir) = fresh_pool().await;
    seed_chapter(&pool, dir.path(), 1, "Aelin walked in.").await;
    let err = kb_rescan_work_hermetic(&pool, "ctr_other", Some(dir.path()), WORK_REF, false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(message.contains("WORLD_KB_FORBIDDEN"));
        }
        other => panic!("expected Api 403, got {other:?}"),
    }
}
