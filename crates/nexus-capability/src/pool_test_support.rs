//! Test-only pool bootstrap shared by this crate's unit tests.
//!
//! Each call owns its tempdir and pool: tests never share a database or a
//! static pool, and a dropped `TempDir` removes the files with it.
// why: per-test isolation — no shared static pool ever leaks between tests.

use sqlx::SqlitePool;

/// note: open a guarded engine pool in its own tempdir.
///
/// The tempdir is returned **first** so `let (_dir, pool) = …` declares the
/// pool second and therefore drops it first: on Windows a directory cannot be
/// unlinked while the pool still holds its files, so the reverse order leaked
/// the tempdir.
///
/// Engine-owned tables (works, chapters, extract jobs) need the engine
/// admission `init_engine_pool` performs; only the pool is kept.
///
/// # Panics
///
/// Panics if the guarded pool cannot be initialized.
pub async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    (dir, pool)
}
