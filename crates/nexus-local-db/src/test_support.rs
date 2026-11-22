//! Test-only pool bootstraps shared by this crate's unit tests.
// note: Test-only pool bootstraps shared by this crate's unit tests.
//!
//! Each call owns its tempdir and pool: tests never share a database or a
//! static pool. The tempdir is returned first so the caller's
//! `let (_dir, pool) = ...` declares `pool` second and drops it first — Windows
//! cannot unlink a directory a live pool still holds, and `TempDir`'s drop
//! failure is silent, so the wrong order leaks the directory.
//!
//! The file is named `state.db`, the product's name, so a test that has to
//! reopen the database by path can build it from the returned tempdir.

use sqlx::SqlitePool;

/// note: The product's local database filename; the tempdir the helpers return holds it.
const STATE_DB_NAME: &str = "state.db";

/// note: Open a migrated pool in its own tempdir.
///
/// # Panics
///
/// Panics if the pool cannot be opened or the migrations fail.
pub async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join(STATE_DB_NAME);
    let pool = crate::open_pool(&db_path).await.unwrap();
    crate::run_migrations(&pool).await.unwrap();
    (dir, pool)
}

/// note: Open a migrated, engine-owned pool in its own tempdir.
///
/// Engine-owned tables (extract jobs, session/run state, works scheduling) need
/// the engine admission [`crate::init_engine_pool`] performs; only the pool is
/// kept, exactly as the per-module copies did.
///
/// # Panics
///
/// Panics if the guarded pool cannot be initialized.
pub async fn fresh_engine_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join(STATE_DB_NAME);
    let pool = crate::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (dir, pool)
}
