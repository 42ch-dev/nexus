//! Workspace Management Module
//!
//! # Mutex Poisoning Policy
//!
//! This crate uses `unwrap_or_else` on mutex locks to recover from poisoned mutexes.
//! A poisoned mutex means a thread panicked while holding the lock. Rather than
//! crashing the entire daemon, we recover the lock and log a warning. The data
//! may be in an inconsistent state, but for a local development tool this is
//! preferable to a hard crash.

pub mod manager;

use crate::db::pool::{DbPool, PoolConfig, PooledConn};
use nexus_sync::outbox::Outbox;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared workspace state
#[derive(Clone)]
pub struct WorkspaceState {
    db: DbPool,
    /// Nexus-sync Outbox for bundle-level sync operations.
    /// Uses a separate SQLite database at `{nexus_home}/sync/outbox.db`
    /// with its own connection pool for async operations.
    outbox: Arc<Option<Outbox>>,
    nexus_home: PathBuf,
    db_path: PathBuf,
    started_at: std::time::Instant,
    workspace_path: Arc<std::sync::Mutex<Option<String>>>,
}

impl WorkspaceState {
    /// Create a WorkspaceState for testing purposes.
    /// Not intended for production use.
    ///
    /// Creates a connection pool with a single connection for test isolation.
    /// Does NOT initialize the Outbox (sync operations will return NotConfigured).
    pub fn new_for_testing(
        nexus_home: PathBuf,
        db_path: PathBuf,
        workspace_path: Option<String>,
    ) -> Self {
        let db = DbPool::new(&db_path, PoolConfig::default().with_max_connections(1))
            .expect("Failed to create test database pool");
        Self {
            db,
            outbox: Arc::new(None),
            nexus_home,
            db_path,
            started_at: std::time::Instant::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(workspace_path)),
        }
    }

    /// Create a WorkspaceState for testing with an outbox.
    ///
    /// Initializes a temporary outbox database for testing sync operations.
    #[cfg(test)]
    pub async fn new_for_testing_with_outbox(
        nexus_home: PathBuf,
        db_path: PathBuf,
        workspace_path: Option<String>,
    ) -> Self {
        let db = DbPool::new(&db_path, PoolConfig::default().with_max_connections(1))
            .expect("Failed to create test database pool");

        // Create outbox at the standard sync directory
        let sync_dir = nexus_home.join("sync");
        std::fs::create_dir_all(&sync_dir).expect("Failed to create sync directory");
        let outbox_path = sync_dir.join("outbox.db");
        let outbox = Outbox::new(&outbox_path)
            .await
            .expect("Failed to create test outbox");

        Self {
            db,
            outbox: Arc::new(Some(outbox)),
            nexus_home,
            db_path,
            started_at: std::time::Instant::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(workspace_path)),
        }
    }

    /// Initialize workspace state — create nexus home, SQLite database,
    /// and sync outbox.
    pub async fn initialize() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
            .join(".nexus42");

        std::fs::create_dir_all(&home)?;

        let db_path = home.join("state.db");

        // Create the database file and initialize schema
        let init_conn = rusqlite::Connection::open(&db_path)?;
        crate::db::schema::Schema::init(&init_conn)?;
        drop(init_conn);

        // Create connection pool with environment-configurable settings
        let db = DbPool::new(&db_path, PoolConfig::from_env())?;

        // Initialize nexus-sync outbox at {home}/sync/outbox.db
        let sync_dir = home.join("sync");
        std::fs::create_dir_all(&sync_dir)?;
        let outbox_path = sync_dir.join("outbox.db");
        let outbox = Outbox::new(&outbox_path).await?;
        tracing::info!("Sync outbox initialized at {:?}", outbox_path);

        Ok(Self {
            db,
            outbox: Arc::new(Some(outbox)),
            nexus_home: home,
            db_path,
            started_at: std::time::Instant::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Get database connection from the pool
    ///
    /// Returns a [`PooledConn`] that provides async-friendly methods for
    /// executing SQL. The connection is returned to the pool when dropped.
    pub async fn db(&self) -> Result<PooledConn, deadpool_sqlite::PoolError> {
        self.db.get().await
    }

    /// Get the nexus-sync Outbox for sync operations.
    ///
    /// Returns `None` if the outbox was not initialized (e.g., in test contexts
    /// using `new_for_testing`).
    pub fn outbox(&self) -> Option<&Outbox> {
        self.outbox.as_ref().as_ref()
    }

    /// Check if workspace is initialized
    pub async fn is_initialized(&self) -> bool {
        self.workspace_path
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("workspace_path mutex poisoned, recovering");
                poisoned.into_inner()
            })
            .is_some()
    }

    /// Get workspace path
    pub fn workspace_path(&self) -> Option<String> {
        self.workspace_path
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("workspace_path mutex poisoned, recovering");
                poisoned.into_inner()
            })
            .clone()
    }

    /// Get database path
    pub fn database_path(&self) -> String {
        self.db_path.display().to_string()
    }

    /// Get nexus home directory
    pub fn nexus_home(&self) -> &PathBuf {
        &self.nexus_home
    }

    /// Get a clone of the database pool (for TokenManager, etc.)
    pub fn db_pool(&self) -> DbPool {
        self.db.clone()
    }

    /// Get uptime in seconds
    pub async fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Initialize a workspace at the given path
    pub async fn init_workspace(&self, path: &str) -> anyhow::Result<()> {
        let workspace_dir = std::path::Path::new(path);
        let nexus_dir = workspace_dir.join(".nexus42");

        std::fs::create_dir_all(&nexus_dir)?;
        std::fs::create_dir_all(workspace_dir.join("Stories"))?;
        std::fs::create_dir_all(workspace_dir.join("References"))?;

        // Store workspace path in the database
        let path_owned = path.to_string();
        let conn = self
            .db()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('workspace_path', ?1)",
            [path_owned],
        )
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // Update in-memory state so is_initialized() returns true
        *self.workspace_path.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("workspace_path mutex poisoned, recovering");
            poisoned.into_inner()
        }) = Some(path.to_string());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_workspace_sets_is_initialized() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workspace_dir = tmp.path().join("my-workspace");

        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        // Set up the database schema
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::db::schema::Schema::init(&conn).unwrap();
        drop(conn);

        let state = WorkspaceState::new_for_testing(
            nexus_home, db_path, None, // no workspace path set initially
        );

        // Before init: is_initialized should be false
        assert!(
            !state.is_initialized().await,
            "is_initialized() should return false before init_workspace()"
        );

        // Initialize workspace
        let path_str = workspace_dir.display().to_string();
        state.init_workspace(&path_str).await.unwrap();

        // After init: is_initialized should be true
        assert!(
            state.is_initialized().await,
            "is_initialized() should return true after init_workspace()"
        );

        // workspace_path() should return the path
        assert_eq!(state.workspace_path(), Some(path_str));
    }
}
