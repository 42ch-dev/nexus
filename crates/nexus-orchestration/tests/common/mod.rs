//! Shared bootstrap for `nexus-orchestration` integration tests.
//!
//! One pool initialisation, reached through the tuple shapes each suite binds.
//! The tuple order is the order that suite bound its owner and pool; `(Arc,
//! owner)` and `(owner, Arc)` are not interchangeable, so each shape is its own
//! thin wrapper. Only the owner differs where a suite re-opens the DB by path:
//! the run-state suite needs a named temp *file*, the rest a temp *dir*.

#![allow(dead_code)] // one binary per test file; each uses a subset

use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;
use tempfile::{NamedTempFile, TempDir};

/// A fresh workspace DB and the temp dir that owns its file.
struct TestDb {
    dir: TempDir,
    pool: SqlitePool,
}

/// The one initialisation both owner shapes share: open the engine pool over
/// `db_path` and drop the writer guard.
async fn engine_pool(db_path: &Path) -> SqlitePool {
    nexus_local_db::init_engine_pool(db_path)
        .await
        .unwrap()
        .clone_pool()
}

/// A temp dir with `state.db` inside it, migrated and open.
async fn initialised() -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let pool = engine_pool(&dir.path().join("state.db")).await;
    TestDb { dir, pool }
}

/// `(TempDir, SqlitePool)` — the plain shape; bind `(_dir, pool)`.
pub async fn test_pool() -> (TempDir, SqlitePool) {
    let TestDb { dir, pool } = initialised().await;
    (dir, pool)
}

/// `(TempDir, Arc<SqlitePool>)` — the same pair with the directory first, for
/// suites that bind `(_dir, pool)` and hand the pool to a supervisor as an
/// `Arc`.
pub async fn arc_pool_dir_first() -> (TempDir, Arc<SqlitePool>) {
    let TestDb { dir, pool } = initialised().await;
    (dir, Arc::new(pool))
}

/// `(Arc<SqlitePool>, NamedTempFile)` — the run-state suite re-opens the DB by
/// `db.path()`, so this shape owns a named temp file instead of a dir; that
/// owner choice is the only step it does not share.
pub async fn arc_pool_pool_first() -> (Arc<SqlitePool>, NamedTempFile) {
    let db = NamedTempFile::new().unwrap();
    let pool = Arc::new(engine_pool(db.path()).await);
    (pool, db)
}
