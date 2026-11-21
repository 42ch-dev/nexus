//! Test-only pool bootstrap shared by this crate's unit tests.
//!
//! Each call owns its tempdir and pool: tests never share a database or a
//! static pool, and a dropped `TempDir` removes the files with it.

use sqlx::SqlitePool;

/// Open a guarded engine pool in its own tempdir.
///
/// Engine-owned tables (works, chapters, extract jobs) need the engine
/// admission `init_engine_pool` performs; only the pool is kept.
///
/// # Panics
///
/// Panics if the guarded pool cannot be initialized.
pub async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (pool, dir)
}
