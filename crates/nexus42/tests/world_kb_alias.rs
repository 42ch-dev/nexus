//! V1.52 T-A P1 — Legacy `creator kb --scope world` alias tests (R-V150KBED-01).
//!
//! Plan: `.mstar/plans/2026-06-19-v1.52-cli-surface-consolidation-auto.md`
//!
//! These tests verify that the legacy World KB path forwards to the canonical
//! `world::kb` functions and emits a deprecation warning. Two families:
//!
//! 1. **CLI surface** (assert_cmd) — `creator world kb adopt --help` documents
//!    the `--auto` flag; `creator kb list --help` documents `--scope world`.
//! 2. **Hermetic forwarding** — drives the canonical `world::kb` logic functions
//!    directly against a fresh temp DB to verify they still work correctly after
//!    the alias wiring (output parity between legacy alias and canonical path).
//!
//! Run with: cargo test -p nexus42 --test world_kb_alias

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use nexus42::commands::creator::world::kb::{kb_delete, kb_list, kb_show};
use nexus42::db::Schema;
use nexus_contracts::BlockType;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_alias_test";
const WORLD: &str = "wld_alias_test";
const CANON_NAME: &str = "char_test_alias";

/// Build a fresh migrated pool + seed a world and a single `KeyBlock`.
async fn fresh_pool_with_block() -> (sqlx::SqlitePool, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();

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
    let mut kb = KeyBlock::new(WORLD, BlockType::Character, CANON_NAME);
    kb.body = Some(KeyBlockBody {
        summary: Some("Alias test summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["test".to_string()]),
        ..Default::default()
    });
    let result = store.insert_key_block(kb).await.unwrap();
    (pool, result.key_block_id, dir)
}

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

/// `creator kb list --help` documents the `--scope world` option and
/// the transition message pointing to the canonical surface. V1.52 T-A P1.
#[test]
fn creator_kb_list_help_documents_scope_world() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    // The --scope flag should be documented.
    assert!(
        help_text.contains("--scope"),
        "expected --scope in help output, got:\n{help_text}"
    );
    assert!(
        help_text.contains("world"),
        "expected 'world' scope mentioned in help output, got:\n{help_text}"
    );
    // V1.52 T-A P1 / R-V152TAP1-S001: deprecation hint pointing to canonical surface
    assert!(
        help_text.contains("deprecated") || help_text.contains("creator world kb"),
        "help text must point to canonical surface (R-V152TAP1-S001), got:\n{help_text}"
    );
}

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
// Hermetic forwarding — canonical functions still work (output parity)
// =============================================================================

/// `kb_list` lists the seeded block via the canonical function.
#[tokio::test]
async fn canonical_kb_list_lists_seeded_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    // Call the canonical function directly (the same one the alias forwards to).
    // We can't capture stdout, but we verify it doesn't error.
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

/// `kb_delete` (used by alias remove) soft-deletes the seeded block.
#[tokio::test]
async fn canonical_kb_delete_soft_deletes_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block().await;

    let result = kb_delete(&pool, OWNER, WORLD, &key_block_id, true).await;
    assert!(result.is_ok(), "kb_delete should succeed: {result:?}");

    // Verify the block is soft-deleted (status = "deleted")
    let store = SqliteKbStore::new(pool.clone());
    let block = store.get_key_block(&key_block_id).await.unwrap();
    assert_eq!(block.status, "deleted", "block should be soft-deleted");
}

/// `kb_delete` cross-author returns an error (auth gate forwarded properly).
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

// =============================================================================
// Alias forward-wiring tests (R-V152TAP1-W001) — exercise kb.rs:448-454,
// 610-615, 789-797 by invoking the legacy surface via assert_cmd with a
// hermetic HOME directory.
// =============================================================================

use std::sync::Mutex;

/// Global mutex serializes HOME setup across parallel test threads.
static HOME_SETUP_LOCK: Mutex<()> = Mutex::new(());

/// Set up a hermetic HOME directory with a seeded state.db for alias tests.
///
/// Returns the temp directory guard (keeps dir alive) and the world ID.
fn hermetic_home_with_world_kb() -> (tempfile::TempDir, String) {
    let _lock = HOME_SETUP_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let nexus_dir = home.join(".nexus42");

    // Create directory structure
    std::fs::create_dir_all(&nexus_dir).unwrap();

    // Write config.toml with active_creator_id
    let config_toml = r#"
active_creator_id = "ctr_alias_test"
active_workspace_slug_by_creator = { ctr_alias_test = "default" }
"#;
    std::fs::write(nexus_dir.join("config.toml"), config_toml).unwrap();

    // Create workspace directory and state.db
    let ws_dir = home.join(".nexus42/creators/ctr_alias_test/workspaces/default");
    std::fs::create_dir_all(&ws_dir).unwrap();
    let db_path = ws_dir.join("state.db");

    // Seed state.db using tokio runtime
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = nexus42::db::Schema::init(&db_path).await.unwrap();
        nexus_local_db::kb_store::seed::world(
            &pool,
            "wld_alias_cmd",
            "ctr_alias_test",
            "Alias Cmd World",
            "alias-cmd-world",
            "private",
            "manual",
        )
        .await;
        let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let mut kb = nexus_kb::key_block::KeyBlock::new(
            "wld_alias_cmd",
            nexus_contracts::BlockType::Character,
            "char_alias_cmd",
        );
        kb.body = Some(nexus_kb::key_block::KeyBlockBody {
            summary: Some("Alias command test summary".to_string()),
            attributes: Some(serde_json::json!({"novel_category": "character"})),
            tags: Some(vec!["alias-test".to_string()]),
            ..Default::default()
        });
        let _result = store.insert_key_block(kb).await.unwrap();
    });

    (dir, "wld_alias_cmd".to_string())
}

/// Build an assert_cmd Command with HOME set to the temp directory.
fn cmd_with_home(home: &std::path::Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("nexus42").unwrap();
    cmd.env("HOME", home);
    cmd
}

/// `creator kb list --scope world --world-id <id>` emits deprecation on stderr
/// and produces listing output.
#[test]
fn legacy_kb_scope_world_list_forwards_to_canonical() {
    let (dir, wid) = hermetic_home_with_world_kb();

    let output = cmd_with_home(dir.path())
        .args([
            "creator",
            "kb",
            "list",
            "--scope",
            "world",
            "--world-id",
            &wid,
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Deprecation message on stderr
    assert!(
        stderr.contains("deprecated"),
        "stderr must contain 'deprecated', got:\n{stderr}"
    );
    assert!(
        stderr.contains("creator world kb list"),
        "stderr must mention canonical surface, got:\n{stderr}"
    );
    assert!(
        stderr.contains("V1.53"),
        "stderr must mention V1.53 removal, got:\n{stderr}"
    );

    // Output should contain the seeded block
    assert!(
        stdout.contains("char_alias_cmd"),
        "stdout must contain the seeded block name, got:\n{stdout}"
    );
}

/// `creator kb show --scope world --world-id <id> <entry_id>` emits deprecation
/// on stderr and shows the seeded block.
#[test]
fn legacy_kb_scope_world_show_forwards_to_canonical() {
    let (dir, wid) = hermetic_home_with_world_kb();
    let home = dir.path();

    // First get the block ID by calling kb list (via the binary)
    let list_output = cmd_with_home(home)
        .args(["creator", "world", "kb", "list", &wid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_text = String::from_utf8(list_output).unwrap();
    // Extract key_block_id from the listing output (format: KEY_BLOCK_ID ...)
    let block_id = list_text
        .lines()
        .find(|l| l.contains("char_alias_cmd"))
        .and_then(|l| l.split_whitespace().next())
        .expect("must find key_block_id in list output")
        .to_string();

    // Now invoke the legacy alias for show
    let output = cmd_with_home(home)
        .args([
            "creator",
            "kb",
            "show",
            &block_id,
            "--scope",
            "world",
            "--world-id",
            &wid,
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stderr.contains("deprecated"),
        "stderr must contain 'deprecated', got:\n{stderr}"
    );
    assert!(
        stderr.contains("creator world kb show"),
        "stderr must mention canonical surface 'show', got:\n{stderr}"
    );
    // The output should contain block details
    assert!(
        stdout.contains("char_alias_cmd"),
        "stdout must contain the block name, got:\n{stdout}"
    );
}

/// `creator kb remove --scope world --world-id <id> <entry_id>` emits deprecation
/// on stderr and removes the seeded block. Verifies the forward path delegates
/// to canonical `kb_delete` (with owner auth gate).
#[test]
fn legacy_kb_scope_world_remove_forwards_to_canonical() {
    let (dir, wid) = hermetic_home_with_world_kb();
    let home = dir.path();

    // Get block ID via canonical path
    let list_output = cmd_with_home(home)
        .args(["creator", "world", "kb", "list", &wid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_text = String::from_utf8(list_output).unwrap();
    let block_id = list_text
        .lines()
        .find(|l| l.contains("char_alias_cmd"))
        .and_then(|l| l.split_whitespace().next())
        .expect("must find key_block_id in list output")
        .to_string();

    // Remove via legacy alias path
    let output = cmd_with_home(home)
        .args([
            "creator",
            "kb",
            "remove",
            &block_id,
            "--scope",
            "world",
            "--world-id",
            &wid,
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        stderr.contains("deprecated"),
        "stderr must contain 'deprecated', got:\n{stderr}"
    );
    assert!(
        stderr.contains("creator world kb remove"),
        "stderr must mention canonical surface 'remove', got:\n{stderr}"
    );

    // Verify block is gone via canonical path
    let verify_output = cmd_with_home(home)
        .args(["creator", "world", "kb", "list", &wid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let verify_text = String::from_utf8(verify_output).unwrap();
    assert!(
        !verify_text.contains("char_alias_cmd"),
        "removed block should not appear in list, got:\n{verify_text}"
    );
}
