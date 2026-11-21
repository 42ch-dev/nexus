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

/// Build a fresh migrated pool holding one owner-scoped world and one
/// `Character` KB entry, and return the entry id plus the tempdir that owns the
/// database.
///
/// Callers differ only in the seed identity, so the world title/slug are
/// parameters. Every call gets its own tempdir; the caller must keep it alive
/// for as long as the pool is used.
pub async fn fresh_pool_with_block(
    creator_id: &str,
    world_id: &str,
    canon_name: &str,
    title: &str,
    slug: &str,
) -> (sqlx::SqlitePool, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();

    nexus_local_db::kb_store::seed::world(
        &pool,
        world_id,
        creator_id,
        title,
        slug,
        "private",
        "manual",
    )
    .await;

    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::new(world_id, BlockType::Character, canon_name);
    kb.body = Some(KnowledgeEntryBody {
        summary: Some("Original summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["novel".to_string()]),
        ..Default::default()
    });
    let result = store.insert_knowledge_entry(kb).await.unwrap();
    (pool, result.entry_id, dir)
}
