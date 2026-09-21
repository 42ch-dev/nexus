//! Canonical World KB surface after the v1.52 alias retirement (R19; v1.193 P0-T13).
//!
//! The legacy `creator kb --scope world` forwarding alias and its deprecation
//! warning are gone: `creator kb` serves the work-scope file index only, and
//! World-scoped narrative KB entries live on the canonical `creator world kb`
//! surface. What remains covered here is that canonical surface itself — the
//! `world::kb` logic functions (`kb_list` / `kb_show` / `kb_delete`) driven
//! hermetically against a fresh temp DB, plus the reachable
//! `creator world kb adopt --help` parser entry. The former alias
//! wording/forwarding assertions were deleted with the entrance they described
//! (delete incidental wording-only tests rather than repin them).
//!
//! Run with: cargo test -p nexus42 --test `world_kb_alias`

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use nexus42::commands::creator::world::kb::{kb_delete, kb_list, kb_show};
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_alias_test";
const WORLD: &str = "wld_alias_test";
const CANON_NAME: &str = "char_test_alias";

/// Build a fresh migrated pool + seed a world and a single `KnowledgeEntryRecord`.
async fn fresh_pool_with_block() -> (sqlx::SqlitePool, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();

    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Alias Test World",
        "alias-test-world",
        "private",
        "manual",
    )
    .await;

    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::new(WORLD, BlockType::Character, CANON_NAME);
    kb.body = Some(KnowledgeEntryBody {
        summary: Some("Alias test summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["test".to_string()]),
        ..Default::default()
    });
    let result = store.insert_knowledge_entry(kb).await.unwrap();
    (pool, result.entry_id, dir)
}

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

/// `creator world kb adopt --help` should be reachable even if `--auto` has
/// not yet landed from T-A P0. This test verifies the help output is present
/// (the `--auto` flag assertion is conditional — T-A P0 deliverable).
#[test]
fn creator_world_kb_adopt_help_is_reachable() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "kb", "adopt", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("adopt"),
        "expected 'adopt' in help output, got:\n{help_text}"
    );
    // Note: --auto flag is a T-A P0 deliverable; this assertion is a forward
    // compatibility check. If T-A P0 has landed, --auto should be documented.
    let has_auto = help_text.contains("--auto");
    if !has_auto {
        eprintln!(
            "NOTE: `--auto` flag not yet present on `creator world kb adopt --help`. \
             This is expected if T-A P0 has not yet merged into iteration/v1.52."
        );
    }
}

// =============================================================================
// Canonical World KB functions (the owners the retired alias merely forwarded to)
// =============================================================================

/// `kb_list` lists the seeded block via the canonical function.
#[tokio::test]
async fn canonical_kb_list_lists_seeded_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    let result = kb_list(&pool, WORLD, false).await;
    assert!(result.is_ok(), "kb_list should succeed: {result:?}");
    drop(key_block_id);
}

/// `kb_show` shows the seeded block via the canonical function.
#[tokio::test]
async fn canonical_kb_show_shows_seeded_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    let result = kb_show(&pool, WORLD, &key_block_id, false).await;
    assert!(result.is_ok(), "kb_show should succeed: {result:?}");
}

/// `kb_delete` soft-deletes the seeded block.
#[tokio::test]
async fn canonical_kb_delete_soft_deletes_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    let result = kb_delete(&pool, OWNER, WORLD, &key_block_id, true).await;
    assert!(result.is_ok(), "kb_delete should succeed: {result:?}");

    // Verify the block is soft-deleted (status = "deleted")
    let store = SqliteKbStore::new(pool.clone());
    let block = store.get_knowledge_entry(&key_block_id).await.unwrap();
    assert_eq!(block.status, "deleted", "block should be soft-deleted");
}

/// `kb_delete` cross-author returns an error (owner auth gate).
#[tokio::test]
async fn canonical_kb_delete_cross_author_rejects() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    // A different creator should not be able to delete.
    let result = kb_delete(&pool, "ctr_stranger", WORLD, &key_block_id, true).await;
    assert!(result.is_err(), "cross-author delete should fail");
    if let Err(e) = result {
        let msg = format!("{e}");
        assert!(
            msg.contains("403") || msg.contains("WORLD_KB_FORBIDDEN"),
            "cross-author error should mention auth, got: {msg}"
        );
    }
}
