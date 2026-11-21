//! Canonical World KB surface after the v1.52 alias retirement (R19; v1.193 P0-T13).
//!
//! The legacy `creator kb --scope world` forwarding alias and its deprecation
//! warning are gone: the `creator kb` work-scope group is itself retired
//! (V1.231, ADR K1 §5.3), and
//! World-scoped narrative KB entries live on the canonical `creator world kb`
//! surface. What remains covered here is that canonical surface itself: the
//! `list` / `show` read leaves drive the REAL `nexus42` binary against a
//! hermetic direct-core home (`common/direct_actor.rs`) and assert the seeded
//! row in raw stdout, the `kb_delete` auth paths drive the `world::kb` logic
//! functions against a fresh temp DB, and the reachable
//! `creator world kb adopt --help` parser entry is pinned. The former alias
//! wording/forwarding assertions were deleted with the entrance they described
//! (delete incidental wording-only tests rather than repin them).
//!
//! Run with: cargo test -p nexus42 --test `world_kb_alias`

#![allow(clippy::unwrap_used)]

#[path = "common/cli_support.rs"]
mod cli_support;
#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use assert_cmd::Command;
use cli_support::{fresh_pool_with_block, stderr, stdout};
use direct_actor::DirectActor;
use nexus42::commands::creator::world::kb::kb_delete;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_alias_test";
const WORLD: &str = "wld_alias_test";
const CANON_NAME: &str = "char_test_alias";

/// World-KB entry the CLI read tests seed through the core (stored `kb_<hex>`).
const CLI_ENTRY_ID: &str = "kb_5ee70011";
const CLI_CANON_NAME: &str = "char_cli_alias";
const CLI_SUMMARY: &str = "Alias test summary";

/// Seed one body-carrying World character entry through the authorized core.
async fn seed_cli_entry(actor: &DirectActor, world_id: &str) {
    actor
        .seed_world_character_sheet_with_body(
            CLI_ENTRY_ID,
            world_id,
            CLI_CANON_NAME,
            serde_json::json!({
                "summary": CLI_SUMMARY,
                "attributes": {"novel_category": "character"},
                "tags": ["test"],
            })
            .as_object()
            .cloned()
            .unwrap(),
        )
        .await;
}

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

/// `creator world kb adopt --help` pins the full Usage command path and the
/// `--auto` option line (clap renders options as trimmed indented lines).
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

    let usage = help_text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Usage:"))
        .unwrap_or_else(|| panic!("adopt --help has no Usage line:\n{help_text}"));
    assert!(
        usage.starts_with("Usage: nexus42 creator world kb adopt"),
        "unexpected adopt usage path: {usage}"
    );
    let option_tokens: Vec<&str> = help_text
        .lines()
        .skip_while(|line| line.trim() != "Options:")
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert!(
        option_tokens.contains(&"--auto"),
        "adopt --help must list the exact --auto option token:\n{help_text}"
    );
}

// =============================================================================
// Canonical World KB read leaves (the real binary over a direct-core home)
// =============================================================================

/// `creator world kb list --json` prints the entry seeded through the core.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_kb_list_lists_seeded_block() {
    let actor = DirectActor::new().await;
    let world_id = actor.create_world("Alias List World").await;
    seed_cli_entry(&actor, &world_id).await;

    let output = actor.cli(&["creator", "world", "kb", "list", &world_id, "--json"]);
    assert!(
        output.status.success(),
        "kb list failed: {}",
        stderr(&output)
    );

    let raw = stdout(&output);
    assert!(
        raw.contains(CLI_ENTRY_ID),
        "seeded id missing from kb list stdout: {raw}"
    );
    assert!(
        raw.contains(CLI_CANON_NAME),
        "seeded canonical name missing from kb list stdout: {raw}"
    );
    let listed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let item = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key_block_id"] == CLI_ENTRY_ID)
        .expect("seeded entry is listed");
    assert_eq!(item["canonical_name"], CLI_CANON_NAME);
}

/// `creator world kb show --json` prints the seeded entry's id, name and body.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_kb_show_shows_seeded_block() {
    let actor = DirectActor::new().await;
    let world_id = actor.create_world("Alias Show World").await;
    seed_cli_entry(&actor, &world_id).await;

    let output = actor.cli(&[
        "creator",
        "world",
        "kb",
        "show",
        &world_id,
        CLI_ENTRY_ID,
        "--json",
    ]);
    assert!(
        output.status.success(),
        "kb show failed: {}",
        stderr(&output)
    );

    let raw = stdout(&output);
    assert!(
        raw.contains(CLI_ENTRY_ID),
        "seeded id missing from kb show stdout: {raw}"
    );
    assert!(
        raw.contains(CLI_CANON_NAME),
        "seeded canonical name missing from kb show stdout: {raw}"
    );
    assert!(
        raw.contains(CLI_SUMMARY),
        "seeded body summary missing from kb show stdout: {raw}"
    );
    let shown: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(shown["entry_id"], CLI_ENTRY_ID);
    assert_eq!(shown["canonical_name"], CLI_CANON_NAME);
    assert_eq!(shown["body"]["summary"], CLI_SUMMARY);
}

// =============================================================================
// Canonical World KB functions (the owners the retired alias merely forwarded to)
// =============================================================================

/// `kb_delete` soft-deletes the seeded block.
#[tokio::test]
async fn canonical_kb_delete_soft_deletes_block() {
    let (pool, key_block_id, _dir) = fresh_pool_with_block(
        OWNER,
        WORLD,
        CANON_NAME,
        "Alias Test World",
        "alias-test-world",
    )
    .await;

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
    let (pool, key_block_id, _dir) = fresh_pool_with_block(
        OWNER,
        WORLD,
        CANON_NAME,
        "Alias Test World",
        "alias-test-world",
    )
    .await;

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
