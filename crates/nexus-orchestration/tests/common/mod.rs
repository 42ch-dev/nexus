//! Shared bootstrap for `nexus-orchestration` integration tests.
//!
//! One pool initialisation, reached through the tuple shapes each suite binds.
//! Each shape is its own thin wrapper, and its name says which element is the
//! owner and which is the pool. Only the owner differs where a suite re-opens
//! the DB by path: the run-state suite needs a named temp *file*, the rest a
//! temp *dir*.

#![allow(dead_code)] // one binary per test file; each uses a subset

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use tempfile::{NamedTempFile, TempDir};

use nexus_capability::{CapabilityError, WorkspaceExecutor};
use nexus_contracts::local::orchestration::{
    WorkArtifactCommitInput, WorkArtifactCommitOutput, WorkspaceCommitInput, WorkspaceCommitOutput,
    WorkspaceOpenInput, WorkspaceOpenOutput,
};

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

/// `(NamedTempFile, Arc<SqlitePool>)` — the run-state suite re-opens the DB by
/// `db.path()`, so this shape owns a named temp file instead of a dir; that
/// owner choice is the only step it does not share. The owner is the first
/// element so the pool, declared second, drops first.
pub async fn arc_pool_file_first() -> (NamedTempFile, Arc<SqlitePool>) {
    let db = NamedTempFile::new().unwrap();
    let pool = Arc::new(engine_pool(db.path()).await);
    (db, pool)
}

/// Test executor: commits Work bytes at the target the production port builds
/// (`<workspace_root>/Works/<work_ref>/<rel_child>`), without the DB lock/row
/// checks the core port performs.
pub struct DirectWriteExecutor {
    workspace_root: PathBuf,
}

impl DirectWriteExecutor {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceExecutor for DirectWriteExecutor {
    fn workspace_root(&self) -> Result<PathBuf, CapabilityError> {
        Ok(self.workspace_root.clone())
    }

    async fn open(
        &self,
        _input: WorkspaceOpenInput,
    ) -> Result<WorkspaceOpenOutput, CapabilityError> {
        Err(CapabilityError::WorkerUnavailable)
    }

    async fn commit(
        &self,
        _input: WorkspaceCommitInput,
    ) -> Result<WorkspaceCommitOutput, CapabilityError> {
        Err(CapabilityError::WorkerUnavailable)
    }

    async fn commit_work_artifact(
        &self,
        input: WorkArtifactCommitInput,
    ) -> Result<WorkArtifactCommitOutput, CapabilityError> {
        let target = Path::new(&input.workspace_root)
            .join("Works")
            .join(&input.work_ref)
            .join(&input.rel_child);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CapabilityError::Internal(format!("mkdir: {e}")))?;
        }
        std::fs::write(&target, &input.content)
            .map_err(|e| CapabilityError::Internal(format!("write: {e}")))?;
        Ok(WorkArtifactCommitOutput { committed: true })
    }
}
