//! Workspace Management Module

pub mod manager;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared workspace state
#[derive(Clone)]
pub struct WorkspaceState {
    db: Arc<Mutex<Option<Connection>>>,
    nexus_home: PathBuf,
    db_path: PathBuf,
    started_at: std::time::Instant,
    workspace_path: Arc<std::sync::Mutex<Option<String>>>,
}

impl WorkspaceState {
    /// Create a WorkspaceState for testing purposes.
    /// Not intended for production use.
    pub fn new_for_testing(
        nexus_home: PathBuf,
        db_path: PathBuf,
        workspace_path: Option<String>,
    ) -> Self {
        let conn = Connection::open(&db_path).expect("Failed to open test database");
        Self {
            db: Arc::new(Mutex::new(Some(conn))),
            nexus_home,
            db_path,
            started_at: std::time::Instant::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(workspace_path)),
        }
    }

    /// Initialize workspace state — create nexus home and SQLite database
    pub fn initialize() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
            .join(".nexus42");

        std::fs::create_dir_all(&home)?;

        let db_path = home.join("state.db");
        let conn = Connection::open(&db_path)?;

        // Initialize database schema
        init_db_schema(&conn)?;

        Ok(Self {
            db: Arc::new(Mutex::new(Some(conn))),
            nexus_home: home,
            db_path,
            started_at: std::time::Instant::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Get database connection
    pub async fn db(&self) -> Option<Connection> {
        // SQLite connections aren't Send, so we work with the lock pattern
        // For now, return a direct reference through the lock
        // In production, we'd use a connection pool
        let guard = self.db.lock().await;
        guard.as_ref().and_then(|_c| {
            // SQLite Connection isn't Clone; in production use r2d2 connection pool.
            // For V1.0 skeleton, open a new connection per request.
            Connection::open(&self.db_path).ok()
        })
    }

    /// Check if workspace is initialized
    pub async fn is_initialized(&self) -> bool {
        self.workspace_path.lock().unwrap().is_some()
    }

    /// Get workspace path
    pub fn workspace_path(&self) -> Option<String> {
        self.workspace_path.lock().unwrap().clone()
    }

    /// Get database path
    pub fn database_path(&self) -> String {
        self.db_path.display().to_string()
    }

    /// Get nexus home directory
    pub fn nexus_home(&self) -> &PathBuf {
        &self.nexus_home
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
        let guard = self.db.lock().await;
        if let Some(conn) = guard.as_ref() {
            conn.execute(
                "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('workspace_path', ?1)",
                rusqlite::params![path],
            )?;
        }

        // Update in-memory state so is_initialized() returns true
        *self.workspace_path.lock().unwrap() = Some(path.to_string());

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

        // Set up the database schema (same as init_db_schema)
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspace_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT DEFAULT (datetime('now')));"
        ).unwrap();
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

/// Initialize the SQLite database schema
fn init_db_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;

         -- Workspace metadata (key-value store)
         CREATE TABLE IF NOT EXISTS workspace_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL,
             updated_at TEXT DEFAULT (datetime('now'))
         );

         -- Creator cache
         CREATE TABLE IF NOT EXISTS creators (
             creator_id TEXT PRIMARY KEY,
             display_name TEXT NOT NULL,
             status TEXT NOT NULL DEFAULT 'active',
             cached_at TEXT NOT NULL,
             data TEXT NOT NULL
         );

         -- Reference source registry
         CREATE TABLE IF NOT EXISTS reference_sources (
             reference_source_id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL DEFAULT 'local',
             source_type TEXT NOT NULL,
             uri TEXT NOT NULL,
             title TEXT NOT NULL,
             tags TEXT,
             content_hash TEXT,
             scan_status TEXT NOT NULL DEFAULT 'pending',
             created_at TEXT NOT NULL,
             updated_at TEXT
         );

         -- Outbox queue for sync commands
         CREATE TABLE IF NOT EXISTS outbox (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             command_type TEXT NOT NULL,
             payload TEXT NOT NULL,
             status TEXT NOT NULL DEFAULT 'pending',
             created_at TEXT NOT NULL,
             sent_at TEXT,
             error TEXT
         );

         -- Insert default workspace metadata
         INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('schema_version', '1');
        ",
    )?;

    Ok(())
}
