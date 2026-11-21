//! Shared bootstrap for `nexus-orchestration` integration tests.
//!
//! One shape was copied per suite until it drifted: an engine pool over a fresh
//! temp DB file. [`test_pool`] owns it, and returns the `TempDir` so the caller
//! keeps the file alive instead of leaking the handle.

#![allow(dead_code)] // one binary per test file; each uses a subset

use sqlx::SqlitePool;
use tempfile::TempDir;

/// An engine pool over a fresh temp DB file, plus the `TempDir` that owns it.
///
/// Bind the pair — `let (_dir, pool) = test_pool().await;` — so the directory
/// outlives the pool.
pub async fn test_pool() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (dir, pool)
}
