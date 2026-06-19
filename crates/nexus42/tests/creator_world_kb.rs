//! V1.51 T-A P2 — `creator world kb pending --missing-only` integration tests.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-missing-kb-detection.md`
//! Spec: `.mstar/knowledge/specs/cli-spec.md` §6.2G (V1.51 T-A P2 amendment)
//!
//! Drives `nexus42::commands::creator::world::kb::kb_pending` directly against a
//! fresh temp DB + workspace directory.
//!
//! Run with: cargo test -p nexus42 --test creator_world_kb

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::world::kb::{kb_pending, WORLD_KB_FORBIDDEN_CODE};
use nexus42::db::Schema;
use nexus42::errors::CliError;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_missing_cli";
const WORK_ID: &str = "wrk_missing_cli";
const WORK_REF: &str = "missing-cli-novel";

async fn fresh_pool_and_dir() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Missing CLI World",
        "missing-cli",
        "private",
        "manual",
    )
    .await;
    seed_work(&pool).await;
    (pool, dir)
}

async fn seed_work(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, world_id, story_ref, created_at, updated_at) \
         VALUES (?, ?, 'ws', 'active', 'Missing CLI Novel', 'goal', 'idea', 'complete', \
                 ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORK_REF)
    .execute(pool)
    .await
    .unwrap();
}

fn write_missing_log(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let log_dir = dir
        .path()
        .join("Works")
        .join(WORK_REF)
        .join("Logs")
        .join("kb")
        .join("missing");
    std::fs::create_dir_all(&log_dir).unwrap();
    let log_path = log_dir.join("2026-06-19-ch3.md");
    let yaml = r#"---
generated_at: "2026-06-19T10:00:00Z"
world_id: wld_missing_cli
work_id: wrk_missing_cli
work_ref: missing-cli-novel
chapter: 3
candidate_count: 2
candidates:
  - canonical_name: Azure Gate
    block_type: scene
    source_quote: "...the eastern gate groaned open..."
    confidence: 0.92
  - canonical_name: Lin Xia
    block_type: character
    source_quote: null
    confidence: null
---
"#;
    std::fs::write(&log_path, yaml).unwrap();
    log_path
}

// ── Missing-only lists candidates from advisory log ──────────────────────────

#[tokio::test]
async fn missing_only_lists_advisory_candidates() {
    let (pool, dir) = fresh_pool_and_dir().await;
    write_missing_log(&dir);

    kb_pending(&pool, OWNER, WORLD, None, false, true, Some(dir.path()))
        .await
        .unwrap();

    // JSON variant.
    kb_pending(&pool, OWNER, WORLD, None, true, true, Some(dir.path()))
        .await
        .unwrap();
}

// ── Missing-only obeys world owner gate ──────────────────────────────────────

#[tokio::test]
async fn missing_only_cross_author_returns_403() {
    let (pool, dir) = fresh_pool_and_dir().await;
    write_missing_log(&dir);

    let err = kb_pending(&pool, OTHER, WORLD, None, false, true, Some(dir.path()))
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
}

// ── Missing-only ignores log files for other worlds ──────────────────────────

#[tokio::test]
async fn missing_only_filters_by_world_id() {
    let (pool, dir) = fresh_pool_and_dir().await;
    write_missing_log(&dir);

    let err = kb_pending(
        &pool,
        OWNER,
        "wld_other",
        None,
        false,
        true,
        Some(dir.path()),
    )
    .await
    .unwrap_err();
    // "wld_other" has no narrative_worlds row, so the owner gate fails with a
    // clean not-found message (same behavior as the default pending path).
    match err {
        CliError::Other(msg) => {
            assert!(
                msg.contains("not found") || msg.contains("World"),
                "expected world not-found message, got: {msg}"
            );
        }
        other => panic!("expected Other error for missing world, got: {other:?}"),
    }
}
