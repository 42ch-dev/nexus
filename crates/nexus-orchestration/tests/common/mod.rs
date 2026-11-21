//! Shared bootstrap for `nexus-orchestration` integration tests.
//!
//! Two shapes were copied per suite until they drifted: an engine pool over a
//! temp DB kept alive for the whole test binary, and an engine pool returned
//! with the `TempDir` that owns its file. Arc-returning variants stay bespoke
//! in their suites.

#![allow(dead_code)] // one binary per test file; each uses a subset

use sqlx::SqlitePool;
use tempfile::TempDir;

/// An engine pool over a fresh temp DB file.
///
/// The file handle is leaked (as every copy did) so the DB outlives the
/// caller's local scope for the rest of the test binary.
pub async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("nexus_orchestration_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);
    nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool()
}

/// An engine pool plus the `TempDir` that owns its file.
pub async fn fresh_pool() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (pool, dir)
}
