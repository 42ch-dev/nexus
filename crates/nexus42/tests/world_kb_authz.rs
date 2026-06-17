//! Hermetic authorization tests for `creator world kb` edit/delete (V1.50 T-B P0).
//!
//! Plan: `2026-06-18-v1.50-kb-editor-cli`
//!
//! Verifies the author identity gate: edit/delete require the active creator to
//! own the world (`narrative_worlds.owner_creator_id`). A cross-author attempt
//! returns a `403` error carrying the stable code `WORLD_KB_FORBIDDEN`, while the
//! world owner can still edit/delete. Drives `kb_edit`/`kb_delete` directly
//! against a fresh temp DB (no `$HOME`, no daemon).
//!
//! Run with: cargo test -p nexus42 --test world_kb_authz

use nexus42::commands::creator::world::kb::{kb_delete, kb_edit, WORLD_KB_FORBIDDEN_CODE};
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_contracts::BlockType;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_owner";
const INTRUDER: &str = "ctr_intruder";
const WORLD: &str = "wld_test";
const CANON_NAME: &str = "char_hero";

const VALID_BODY: &str =
    r#"{"summary":"updated","attributes":{"novel_category":"character"},"tags":["novel"]}"#;

/// Fresh pool + world owned by [`OWNER`] + one novel-valid KeyBlock.
async fn fresh_pool_with_block() -> (sqlx::SqlitePool, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();

    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Test World",
        "test-world",
        "private",
        "manual",
    )
    .await;

    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KeyBlock::new(WORLD, BlockType::Character, CANON_NAME);
    kb.body = Some(KeyBlockBody {
        summary: Some("Original summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["novel".to_string()]),
    });
    let result = store.insert_key_block(kb).await.unwrap();
    (pool, result.key_block_id, dir)
}

/// Cross-author `kb_edit` returns `403 WORLD_KB_FORBIDDEN` and does not mutate the row.
#[tokio::test]
async fn cross_author_edit_returns_403() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    let err = kb_edit(&pool, INTRUDER, WORLD, &block_id, VALID_BODY, false)
        .await
        .unwrap_err();

    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403, "cross-author edit must return HTTP 403");
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "error message must carry stable code {WORLD_KB_FORBIDDEN_CODE}: {message}"
            );
            assert!(
                message.contains(INTRUDER) && message.contains(OWNER),
                "error message must name both the active creator and the world owner: {message}"
            );
        }
        other => panic!("expected CliError::Api {{ status: 403, .. }}, got: {other:?}"),
    }

    // Row must be unchanged.
    let block = SqliteKbStore::new(pool)
        .get_key_block(&block_id)
        .await
        .unwrap();
    assert_eq!(
        block.body.as_ref().and_then(|b| b.summary.as_deref()),
        Some("Original summary"),
        "cross-author edit must not mutate the block"
    );
}

/// Cross-author `kb_delete` returns `403 WORLD_KB_FORBIDDEN` and the row survives.
#[tokio::test]
async fn cross_author_delete_returns_403() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    let err = kb_delete(&pool, INTRUDER, WORLD, &block_id, true)
        .await
        .unwrap_err();

    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403, "cross-author delete must return HTTP 403");
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "error message must carry stable code {WORLD_KB_FORBIDDEN_CODE}: {message}"
            );
        }
        other => panic!("expected CliError::Api {{ status: 403, .. }}, got: {other:?}"),
    }

    // Block must still be listed (not soft-deleted).
    let blocks = SqliteKbStore::new(pool).list_by_world(WORLD).await.unwrap();
    assert_eq!(
        blocks.len(),
        1,
        "cross-author delete must not remove the block"
    );
}

/// Positive control: the world owner can edit and delete.
#[tokio::test]
async fn owner_can_edit_and_delete() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    // Owner edit succeeds.
    kb_edit(&pool, OWNER, WORLD, &block_id, VALID_BODY, false)
        .await
        .unwrap();

    // Owner delete succeeds.
    kb_delete(&pool, OWNER, WORLD, &block_id, true)
        .await
        .unwrap();

    let blocks = SqliteKbStore::new(pool).list_by_world(WORLD).await.unwrap();
    assert!(
        blocks.is_empty(),
        "owner delete should remove the block from listing"
    );
}

/// A non-existent world yields a clean not-found error (not a 403).
#[tokio::test]
async fn edit_on_missing_world_is_not_a_403() {
    let dir = tempfile::tempdir().unwrap();
    let pool = Schema::init(&dir.path().join("state.db")).await.unwrap();

    let err = kb_edit(&pool, OWNER, "wld_ghost", "kb_none", VALID_BODY, false)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains(WORLD_KB_FORBIDDEN_CODE),
        "missing world must not surface the forbidden code: {msg}"
    );
    assert!(
        msg.contains("not found"),
        "missing world must surface a not-found error: {msg}"
    );
}
