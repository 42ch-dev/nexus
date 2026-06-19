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

use nexus42::commands::creator::world::kb::{kb_adopt_auto, kb_pending, WORLD_KB_FORBIDDEN_CODE};
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_local_db::kb_extract_job::insert_pending_with_llm;

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

// ── Auto-promote adopts high-confidence candidates and skips others ──────────

async fn seed_candidate(pool: &sqlx::SqlitePool, canonical_name: &str, confidence: f64) -> String {
    let payload = serde_json::json!({
        "summary": format!("Auto-promote test entry for {canonical_name}"),
        "attributes": { "novel_category": "character" },
        "tags": ["auto-promote"],
    })
    .to_string();
    let quote = format!("...{canonical_name} stepped through the gate...");
    let row = insert_pending_with_llm(
        pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        canonical_name,
        &payload,
        Some(confidence),
        Some(&quote),
    )
    .await
    .unwrap();
    row.job_id
}

#[tokio::test]
async fn adopt_auto_promote() {
    let (pool, dir) = fresh_pool_and_dir().await;

    let promoted_id = seed_candidate(&pool, "Auto Promote Hero", 0.97).await;
    let skipped_id = seed_candidate(&pool, "Low Confidence Hero", 0.85).await;

    kb_adopt_auto(&pool, OWNER, WORLD, Some(dir.path()), false)
        .await
        .unwrap();

    // One high-confidence candidate becomes a confirmed KeyBlock.
    let kb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = ?")
        .bind(WORLD)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kb_count, 1);

    // The promoted row is flipped and carries audit columns.
    let promoted: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT promotion_status, auto_promoted_at, auto_promoted_reason \
         FROM kb_extract_jobs WHERE job_id = ?",
    )
    .bind(&promoted_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(promoted.0, "confirmed");
    assert!(promoted.1.is_some(), "auto_promoted_at should be set");
    assert!(
        promoted
            .2
            .as_deref()
            .unwrap_or("")
            .contains("confidence=0.97"),
        "expected reason to contain confidence: {:?}",
        promoted.2
    );

    // The low-confidence candidate stays pending and untouched.
    let skipped: (String, Option<String>) = sqlx::query_as(
        "SELECT promotion_status, auto_promoted_at \
         FROM kb_extract_jobs WHERE job_id = ?",
    )
    .bind(&skipped_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(skipped.0, "pending");
    assert!(skipped.1.is_none(), "auto_promoted_at should be NULL");

    // Audit log was written under the correct work_ref path.
    let log_dir = dir
        .path()
        .join("Works")
        .join(WORK_REF)
        .join("Logs")
        .join("kb")
        .join("auto-promoted");
    let entries: Vec<std::path::PathBuf> = std::fs::read_dir(&log_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    assert_eq!(entries.len(), 1, "expected one auto-promote audit log");
    let log_text = std::fs::read_to_string(&entries[0]).unwrap();
    assert!(log_text.contains("Auto-promoted KB candidate"));
    assert!(log_text.contains(&promoted_id));
    assert!(log_text.contains("Auto Promote Hero"));
}

// ── Auto-promote obeys the world owner gate ──────────────────────────────────

#[tokio::test]
async fn adopt_auto_promote_cross_author_returns_403() {
    let (pool, dir) = fresh_pool_and_dir().await;
    seed_candidate(&pool, "Cross Author Hero", 0.97).await;

    let err = kb_adopt_auto(&pool, OTHER, WORLD, Some(dir.path()), false)
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
