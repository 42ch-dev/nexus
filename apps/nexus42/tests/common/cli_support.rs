//! Shared harness helpers for the `nexus42` CLI test binaries.
//!
//! Each integration target includes only the subset it uses, so unused items
//! here are expected and not a sign of dead code:
//!
//! ```ignore
//! #[path = "common/cli_support.rs"]
//! mod cli_support;
//! ```
#![allow(dead_code)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use std::process::Output;

/// Decode a child's stdout as UTF-8. A real binary may emit invalid bytes, so
/// the lossy conversion is deliberate: the assertions read text, not bytes.
pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Decode a child's stderr as UTF-8 (lossy, same rationale as [`stdout`]).
pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Decode a `--json` leaf's stdout as the one JSON document its contract
/// promises.
///
/// An empty stdout is a contract violation, not a parse problem: `serde_json`
/// reports it as `EOF while parsing a value at line 1 column 0`, which reads
/// like a crash instead of "the command printed nothing". Name the empty case
/// here (with exit status and stderr) so the failure says what broke, and keep
/// the same for a truncated/non-JSON body.
pub fn json_stdout(out: &Output) -> serde_json::Value {
    let text = stdout(out);
    let code = out.status.code();
    assert!(
        !text.trim().is_empty(),
        "--json leaf printed empty stdout (exit code {code:?}); stderr: {}",
        stderr(out)
    );
    serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!(
            "--json leaf printed invalid JSON ({error}) (exit code {code:?}); \
             stdout: {text:?}; stderr: {}",
            stderr(out)
        )
    })
}

/// Build a fresh migrated pool holding one owner-scoped world and one
/// `Character` KB entry.
///
/// Returns `(dir, pool, entry_id)`. The tempdir owner comes **first** so that
/// destructuring binds it first and it drops last: the pool must go before the
/// directory it holds open (Windows cannot unlink an open database).
///
/// Callers differ only in the seed identity and body, so the world
/// title/slug and the entry summary/tag are parameters. Every call gets its
/// own tempdir; the caller must keep it alive for as long as the pool is used.
pub async fn fresh_pool_with_block(
    creator_id: &str,
    world_id: &str,
    canon_name: &str,
    title: &str,
    slug: &str,
    summary: &str,
    tag: &str,
) -> (tempfile::TempDir, sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();

    nexus_local_db::kb_store::seed::world(
        &pool, world_id, creator_id, title, slug, "private", "manual",
    )
    .await;

    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::new(world_id, BlockType::Character, canon_name);
    kb.body = Some(KnowledgeEntryBody {
        summary: Some(summary.to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec![tag.to_string()]),
        ..Default::default()
    });
    let result = store.insert_knowledge_entry(kb).await.unwrap();
    (dir, pool, result.entry_id)
}
