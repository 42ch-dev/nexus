//! Test-only pool bootstraps shared by this crate's unit tests.
//!
//! Each call owns its tempdir and pool: tests never share a database or a
//! static pool, and a dropped `TempDir` removes the files with it.

use sqlx::SqlitePool;

/// Open a migrated pool in its own tempdir.
///
/// # Panics
///
/// Panics if the pool cannot be opened or the migrations fail.
pub async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = crate::open_pool(&db_path).await.unwrap();
    crate::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Open a migrated, engine-owned pool in its own tempdir.
///
/// Engine-owned tables (extract jobs, session/run state, works scheduling) need
/// the engine admission [`crate::init_engine_pool`] performs; only the pool is
/// kept, exactly as the per-module copies did.
///
/// # Panics
///
/// Panics if the guarded pool cannot be initialized.
pub async fn fresh_engine_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = crate::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (pool, dir)
}
