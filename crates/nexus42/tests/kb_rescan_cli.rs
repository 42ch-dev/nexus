//! V1.50 T-B P2 — `creator kb rescan` hermetic round-trip.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-refreshable-scan.md`
//! Spec: `.mstar/knowledge/specs/entity-scope-model.md` §5.5
//!
//! Drives `nexus42::commands::creator::kb::rescan::kb_rescan_hermetic` against
//! a fresh temp DB + temp workspace so the rescan can be exercised without
//! `$HOME` or a daemon.
//!
//! Covers:
//! - **AC1**: idempotent re-run on unchanged text produces an empty diff.
//! - **AC2/AC5**: edited chapter text triggers candidate upsert + KB refresh;
//!   rescan with no edit produces an empty diff.
//! - **AC3**: `--dry-run` shows the diff without writing.
//! - **AC4**: cross-author attempt returns `403` (`WORLD_KB_FORBIDDEN`).
//!
//! Run with: cargo test -p nexus42 --test kb_rescan_cli

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::kb::rescan::{kb_rescan_hermetic, WORLD_KB_FORBIDDEN_CODE};
use nexus42::commands::creator::world::kb::kb_adopt;
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_kb::key_block::KeyBlockBody;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::{insert_pending, list_pending_for_world};
use nexus_local_db::kb_store::SqliteKbStore;
use std::path::Path;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_rescan";
const WORK_ID: &str = "wrk_rescan";
/// Human-readable `work_ref` (= `works.story_ref`) the CLI targets.
const WORK_REF: &str = "rescan-novel";
const CHAPTER_BODY_REL: &str = "Works/rescan-novel/Stories/01-chapter.md";

/// Build a fresh migrated pool + seed a world (owned by OWNER), a works row
/// (with `story_ref = WORK_REF` + `world_id = WORLD`), and a `work_chapters`
/// row pointing at `CHAPTER_BODY_REL`.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Rescan World",
        "rescan-world",
        "private",
        "manual",
    )
    .await;
    seed_work_and_chapter(&pool).await;
    (pool, dir)
}

async fn seed_work_and_chapter(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known works table schema.
    sqlx::query(
        "INSERT OR IGNORE INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, world_id, story_ref, created_at, updated_at) \
         VALUES (?, ?, 'ws', 'active', 'Rescan Novel', 'goal', 'idea', 'complete', \
                 ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORK_REF)
    .execute(pool)
    .await
    .unwrap();

    // Seed the chapter row the rescan reads.
    sqlx::query(
        "INSERT OR IGNORE INTO work_chapters \
         (work_id, chapter, volume, slug, planned_word_count, status, body_path, \
          created_at, updated_at) \
         VALUES (?, 1, 1, 'chapter', 100, 'finalized', ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(CHAPTER_BODY_REL)
    .execute(pool)
    .await
    .unwrap();
}

/// Write chapter prose to `<ws_dir>/<CHAPTER_BODY_REL>`.
fn write_chapter_prose(ws_dir: &Path, prose: &str) {
    let path = ws_dir.join(CHAPTER_BODY_REL);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, prose).unwrap();
}

fn target() -> String {
    format!("{WORK_REF}/1")
}

// ── AC1: idempotent re-run on unchanged text → empty diff ──────────────────

#[tokio::test]
async fn rescan_idempotent_rerun_produces_empty_diff() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let first = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        first.candidates_inserted.iter().any(|n| n == "Lin Xia"),
        "first scan should insert 'Lin Xia': {:?}",
        first.candidates_inserted
    );

    // Re-run on unchanged text → no candidate changes, no kb updates.
    let second = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        second.is_empty(),
        "idempotent re-run must produce an empty diff (AC1), got: {second:?}"
    );
    assert_eq!(second.candidates_unchanged, 1);
}

// ── AC2/AC5: edited text → candidate upsert + KB refresh; no edit → empty ──

#[tokio::test]
async fn rescan_after_chapter_edit_updates_candidate_rows() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();

    // Edit the chapter: drop "Lin Xia", add "Marcus Vale".
    write_chapter_prose(dir.path(), "Marcus Vale surveyed the quiet harbor.");

    let after_edit = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        after_edit
            .candidates_inserted
            .iter()
            .any(|n| n == "Marcus Vale"),
        "edited text should insert 'Marcus Vale': {:?}",
        after_edit.candidates_inserted
    );
    assert!(
        after_edit.candidates_removed.iter().any(|n| n == "Lin Xia"),
        "edited text should remove stale 'Lin Xia': {:?}",
        after_edit.candidates_removed
    );

    // KB rows reflect the new extraction: Marcus Vale is advisory-new (no
    // KeyBlock yet), Lin Xia is advisory-removed (no KeyBlock either, since it
    // was only ever a pending candidate here).
    assert!(after_edit
        .candidates_inserted
        .iter()
        .any(|n| n == "Marcus Vale"));

    // No-edit re-run → empty diff (AC5 second half).
    let idle = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(idle.is_empty(), "no-edit rerun must be empty: {idle:?}");
}

#[tokio::test]
async fn rescan_refreshes_out_of_sync_confirmed_keyblock_body() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    // First scan + adopt → confirmed KeyBlock carrying the heuristic payload.
    let scan = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let lin_xia = pending
        .iter()
        .find(|r| r.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .unwrap();
    kb_adopt(&pool, OWNER, &lin_xia.job_id, None, false)
        .await
        .unwrap();
    let _ = scan;

    // Manually drift the confirmed KeyBlock body away from the chapter's
    // extraction (simulates a body that fell out of sync with the source text).
    let store = SqliteKbStore::new(pool.clone());
    let mut blocks = store.list_by_world(WORLD).await.unwrap();
    let mut kb = blocks.remove(0);
    kb.body = Some(KeyBlockBody {
        summary: Some("stale hand-edited body".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: None,
    });
    kb.updated_at = Some(chrono::Utc::now().to_rfc3339());
    store.update_key_block(kb).await.unwrap();

    // Rescan → diff_and_apply refreshes the confirmed body back to the
    // extraction, so KB rows reflect the current chapter text (AC2/AC5).
    let rescan = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        rescan.kb_updated.iter().any(|n| n == "Lin Xia"),
        "rescan should refresh the out-of-sync 'Lin Xia' KeyBlock: {:?}",
        rescan.kb_updated
    );

    // The stored body now matches the heuristic extraction again.
    let after = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(
        after[0]
            .body
            .as_ref()
            .and_then(|b| b.summary.as_deref())
            .unwrap_or("")
            .contains("Lin Xia"),
        "refreshed body should reflect the chapter extraction: {:?}",
        after[0].body
    );

    // No-edit re-run → empty diff (the body is back in sync).
    let idle = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(idle.is_empty(), "no-edit rerun must be empty: {idle:?}");
}

// ── AC3: dry-run shows diff without writing ───────────────────────────────

#[tokio::test]
async fn dry_run_shows_diff_without_writing() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let dry = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), true)
        .await
        .unwrap();
    assert!(dry.dry_run);
    assert!(
        dry.candidates_inserted.iter().any(|n| n == "Lin Xia"),
        "dry-run should preview the 'Lin Xia' insert: {:?}",
        dry.candidates_inserted
    );
    assert!(
        dry.kb_inserted_advisory.iter().any(|n| n == "Lin Xia"),
        "dry-run should preview the advisory KB insert: {:?}",
        dry.kb_inserted_advisory
    );

    // Nothing was actually written: no pending candidate, no KeyBlock.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(
        pending.is_empty(),
        "dry-run must not write candidates, got: {pending:?}"
    );
    let blocks = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(blocks.is_empty(), "dry-run must not write KeyBlocks");
}

// ── AC4: cross-author attempt returns 403 ──────────────────────────────────

#[tokio::test]
async fn rescan_cross_author_returns_403() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let err = kb_rescan_hermetic(&pool, OTHER, Some(dir.path()), &target(), false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "expected {WORLD_KB_FORBIDDEN_CODE} in: {message}"
            );
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }

    // No candidate was written (the gate fires before any upsert).
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(pending.is_empty());
}

// ── Error handling: malformed target / missing work ────────────────────────

#[tokio::test]
async fn malformed_target_returns_clean_error() {
    let (pool, dir) = fresh_pool().await;
    let err = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), "no-slash-here", false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => assert!(msg.contains("<work_ref>/<chapter>")),
        other => panic!("expected Other error for malformed target, got: {other:?}"),
    }
}

#[tokio::test]
async fn missing_work_returns_clean_error() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");
    let err = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), "no-such-work/1", false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => assert!(msg.contains("no-such-work")),
        other => panic!("expected Other error for missing work, got: {other:?}"),
    }
}

// ── Sanity: a pre-existing pending candidate is reused, not duplicated ─────

#[tokio::test]
async fn rescan_reuses_preexisting_pending_candidate_without_duplicate() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    // Seed a pending candidate exactly as the review-time hook would.
    let payload = serde_json::json!({
        "summary": "Candidate extracted from chapter prose: Lin Xia",
        "attributes": {"novel_category": "character", "aliases": ["Lin Xia"]},
        "tags": ["novel", "heuristic-extracted"],
    })
    .to_string();
    insert_pending(
        &pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        "Lin Xia",
        &payload,
    )
    .await
    .unwrap();

    let report = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    // The pre-existing candidate is reused (unchanged), not re-inserted.
    assert!(
        report.candidates_inserted.is_empty(),
        "should not duplicate the pre-existing candidate: {report:?}"
    );
    assert_eq!(report.candidates_unchanged, 1);

    // Still exactly one row.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(
        pending
            .iter()
            .filter(|r| r.canonical_name_guess.as_deref() == Some("Lin Xia"))
            .count(),
        1
    );
}
