//! Hermetic tests for the `creator world kb` author CLI surface (V1.50 T-B P0).
//!
//! Plan: `2026-06-18-v1.50-kb-editor-cli`
//!
//! Two test families live here:
//!
//! 1. **CLI surface** (assert_cmd) — `creator world kb --help` lists the
//!    `list`/`show`/`edit`/`delete` subcommands and each subcommand exposes the
//!    expected flags (`--json`, `--body`, `--yes`/`-y`).
//! 2. **Hermetic round-trip** — drives `nexus42::commands::creator::world::kb`
//!    logic functions directly against a fresh temp DB (`Schema::init` + public
//!    seed helpers) to exercise list/show/edit/delete without `$HOME` or a daemon.
//!
//! Run with: cargo test -p nexus42 --test world_kb_cli

use assert_cmd::Command;
use nexus42::commands::creator::world::kb::{kb_delete, kb_edit, kb_list, kb_show};
use nexus42::db::Schema;
use nexus_contracts::BlockType;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_owner";
const WORLD: &str = "wld_test";
const CANON_NAME: &str = "char_hero";

/// Read the body summary for assertion convenience.
fn summary_of(block: &KeyBlock) -> Option<&str> {
    block.body.as_ref().and_then(|b| b.summary.as_deref())
}

/// Build a fresh migrated pool + seed a world owned by [`OWNER`] and a single
/// provisional `KeyBlock` (with a valid novel body) in [`WORLD`].
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
        ..Default::default()
    });
    let result = store.insert_key_block(kb).await.unwrap();
    (pool, result.key_block_id, dir)
}

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

/// `creator world kb --help` lists all four subcommands.
#[test]
fn world_kb_help_lists_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "kb", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "show", "edit", "delete"] {
        assert!(
            help_text.contains(subcmd),
            "creator world kb --help must list '{subcmd}' subcommand: {help_text}"
        );
    }
}

/// `creator world kb list --help` documents the world_ref arg and `--json`.
#[test]
fn world_kb_list_help_shows_expected_text() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "kb", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("world_ref") || help_text.contains("WORLD_REF"),
        "list --help must show the world_ref argument: {help_text}"
    );
    assert!(
        help_text.contains("--json"),
        "list --help must list --json flag: {help_text}"
    );
}

/// `creator world kb edit --help` documents `--body` (required for edit).
#[test]
fn world_kb_edit_help_shows_body_flag() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "kb", "edit", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("--body"),
        "edit --help must list --body flag: {help_text}"
    );
}

/// `creator world kb delete --help` documents `--yes` / `-y`.
#[test]
fn world_kb_delete_help_shows_yes_flag() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "kb", "delete", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("--yes"),
        "delete --help must list --yes flag: {help_text}"
    );
    assert!(
        help_text.contains("-y"),
        "delete --help must list the -y short form: {help_text}"
    );
}

// =============================================================================
// Hermetic round-trip (fresh pool per test — no $HOME, no daemon)
// =============================================================================

/// `kb_list` returns the seeded block (human + JSON paths do not error).
#[tokio::test]
async fn list_returns_seeded_block() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    kb_list(&pool, WORLD, false).await.unwrap();
    kb_list(&pool, WORLD, true).await.unwrap();

    let blocks = SqliteKbStore::new(pool).list_by_world(WORLD).await.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].key_block_id, block_id);
    assert_eq!(blocks[0].canonical_name, CANON_NAME);
}

/// `kb_show` returns the full block, including the seeded body.
#[tokio::test]
async fn show_returns_full_block() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    kb_show(&pool, WORLD, &block_id, false).await.unwrap();
    kb_show(&pool, WORLD, &block_id, true).await.unwrap();

    let block = SqliteKbStore::new(pool)
        .get_key_block(&block_id)
        .await
        .unwrap();
    assert_eq!(summary_of(&block), Some("Original summary"));
}

/// `kb_edit` updates the body in place and the change is persisted.
#[tokio::test]
async fn edit_updates_body_in_place() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;
    let new_body = serde_json::json!({
        "summary": "Updated summary",
        "attributes": {"novel_category": "character", "traits": ["brave"]},
        "tags": ["novel", "hero"]
    })
    .to_string();

    kb_edit(&pool, OWNER, WORLD, &block_id, &new_body, false)
        .await
        .unwrap();

    let block = SqliteKbStore::new(pool)
        .get_key_block(&block_id)
        .await
        .unwrap();
    assert_eq!(summary_of(&block), Some("Updated summary"));
    let tags = block
        .body
        .as_ref()
        .and_then(|b| b.tags.as_ref())
        .expect("tags present");
    assert!(tags.contains(&"hero".to_string()));
}

/// `kb_edit` re-runs Novel validation: a body missing `novel_category` is rejected
/// and the original body is left unchanged.
#[tokio::test]
async fn edit_rejects_body_missing_novel_category() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;
    let bad_body = serde_json::json!({
        "summary": "No category",
        "attributes": {"traits": ["old"]},
        "tags": []
    })
    .to_string();

    let err = kb_edit(&pool, OWNER, WORLD, &block_id, &bad_body, false)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("ValidationError") && msg.contains("novel_category"),
        "expected ValidationError mentioning novel_category, got: {msg}"
    );

    let block = SqliteKbStore::new(pool)
        .get_key_block(&block_id)
        .await
        .unwrap();
    assert_eq!(summary_of(&block), Some("Original summary"));
}

/// `kb_delete` soft-deletes the block; `list` no longer returns it, the row is retained.
#[tokio::test]
async fn delete_soft_deletes_block() {
    let (pool, block_id, _dir) = fresh_pool_with_block().await;

    kb_delete(&pool, OWNER, WORLD, &block_id, true)
        .await
        .unwrap();

    let blocks = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(blocks.is_empty(), "deleted block should not be listed");

    let block = SqliteKbStore::new(pool)
        .get_key_block(&block_id)
        .await
        .unwrap();
    assert_eq!(block.status, "deleted");
}
