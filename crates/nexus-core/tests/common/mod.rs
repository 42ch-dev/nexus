//! Shared bootstrap for `nexus-core` integration tests.
//!
//! Two shapes were copied per suite until they drifted: the hermetic `$HOME`
//! (`.nexus42`, `config.toml`, workspace DB path) and the pool opened on it.
//! Each helper below owns one shape; a suite that needs a different one keeps
//! its own function.
//!
//! Every call creates its own tempdir and pool — there is no global pool,
//! cache, or shared mutable state.

#![allow(dead_code)] // one binary per test file; each uses a subset

use std::path::{Path, PathBuf};

use nexus_local_db::writer_protocol::{
    init_engine_pool, join_live_engine_pool, GuardedPool, GuardedPoolOptions,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// A fresh `$HOME` with the workspace DB path `nexus_home_layout` derives.
pub struct TestHome {
    /// Keeps the tempdir alive for the fixture's lifetime.
    pub _tmp: TempDir,
    /// The `$HOME` handed to `CoreOpenOptions`.
    pub user_home: PathBuf,
    /// The workspace state DB inside `user_home`.
    pub db_path: PathBuf,
}

/// Create the `.nexus42` home, the operational workspace dir and `config.toml`
/// for `creator`'s `workspace_slug`, as a real `nexus42` install would.
pub fn seed_home(creator: &str, workspace_slug: &str) -> TestHome {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home,
        creator,
        workspace_slug,
    ))
    .unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{creator}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{creator}\" = \"{workspace_slug}\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, creator, workspace_slug);
    TestHome {
        _tmp: tmp,
        user_home,
        db_path,
    }
}

/// The engine-mode pool guarded by `creator`, as a daemon boot opens it.
///
/// The caller keeps the guard: dropping it is what releases the writer
/// admission, so the fixture decides when that happens.
pub async fn engine_pool(db_path: &Path, creator: &str) -> GuardedPool {
    init_engine_pool(db_path, creator, GuardedPoolOptions::default())
        .await
        .unwrap()
}

/// A plain pool over an initialized workspace DB.
pub async fn plain_pool(db_path: &Path) -> SqlitePool {
    nexus_local_db::open_pool(db_path).await.unwrap()
}

/// A read-only pool over `creator`'s `workspace_slug` workspace DB.
pub async fn read_only_pool(user_home: &Path, creator: &str, workspace_slug: &str) -> SqlitePool {
    let db_path = nexus_home_layout::workspace_state_db_path(user_home, creator, workspace_slug);
    nexus_local_db::open_pool_read_only(&db_path)
        .await
        .expect("open read-only pool")
}

/// Join the live engine admission a booted core already holds.
pub async fn live_write_pool(db_path: &Path) -> (GuardedPool, SqlitePool) {
    let engine = join_live_engine_pool(db_path, GuardedPoolOptions::default())
        .await
        .unwrap()
        .expect("live engine pool");
    let pool = engine.clone_pool();
    (engine, pool)
}
