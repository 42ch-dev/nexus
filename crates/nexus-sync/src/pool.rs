//! SQLite connection pool for nexus-sync
//!
//! Provides async connection pooling via `sqlx::SqlitePool`, re-exported from
//! `nexus-local-db`. The pool uses WAL mode for better concurrent read/write
//! performance.
//!
//! # Usage
//!
//! ```ignore
//! use nexus_sync::pool::{OutboxPool, DEFAULT_POOL_SIZE};
//!
//! let pool = OutboxPool::new(&db_path, DEFAULT_POOL_SIZE).await?;
//! let outbox = Outbox::with_pool(pool).await?;
//! ```

use std::path::Path;

/// Default pool size for nexus-sync
pub const DEFAULT_POOL_SIZE: usize = 4;

/// Connection pool wrapper for SQLite outbox operations
///
/// Wraps `sqlx::SqlitePool` and provides the same pool interface.
#[derive(Clone)]
pub struct OutboxPool {
    pool: nexus_local_db::SqlitePool,
}

impl OutboxPool {
    /// Create a new connection pool for the given database path
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `max_size` - Maximum number of connections in the pool
    ///
    /// # Errors
    /// Returns `LocalDbError` if the pool cannot be created (e.g. invalid path)
    pub async fn new(
        db_path: &Path,
        max_size: usize,
    ) -> Result<Self, nexus_local_db::LocalDbError> {
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max_size as u32)
            .connect(&url)
            .await
            .map_err(nexus_local_db::LocalDbError::from)?;
        // SAFETY: PRAGMA statement — no table schema to validate against.
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        // SAFETY: PRAGMA statement — no table schema to validate against.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;
        Ok(Self { pool })
    }

    /// Get the underlying `SqlitePool` reference
    pub fn inner(&self) -> &nexus_local_db::SqlitePool {
        &self.pool
    }
}

impl From<nexus_local_db::SqlitePool> for OutboxPool {
    fn from(pool: nexus_local_db::SqlitePool) -> Self {
        Self { pool }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test database with schema
    fn create_test_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        // Create the file so sqlx can open it
        std::fs::File::create(&db_path).unwrap();

        (tmp, db_path)
    }

    #[tokio::test]
    async fn pool_creates_successfully() {
        let (_tmp, db_path) = create_test_db();
        let pool = OutboxPool::new(&db_path, 2)
            .await
            .expect("Pool creation should succeed");
        // Pool is usable
        let _ = pool.inner();
    }

    #[tokio::test]
    async fn pool_get_returns_working_connection() {
        let (_tmp, db_path) = create_test_db();

        let pool = OutboxPool::new(&db_path, 2).await.unwrap();

        // SAFETY: test-only DDL — creates a temporary test table; no production schema dependency.
        sqlx::query("CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, val TEXT NOT NULL)")
            .execute(pool.inner())
            .await
            .unwrap();

        // SAFETY: test-only DML — inserts test data into temporary test table.
        sqlx::query("INSERT INTO test (val) VALUES (?)")
            .bind("hello")
            .execute(pool.inner())
            .await
            .unwrap();

        // SAFETY: test-only read — verifies inserted test data in temporary test table.
        let row: (String,) = sqlx::query_as("SELECT val FROM test WHERE id = 1")
            .fetch_one(pool.inner())
            .await
            .unwrap();
        assert_eq!(row.0, "hello");
    }

    #[tokio::test]
    async fn pool_supports_concurrent_access() {
        let (_tmp, db_path) = create_test_db();

        let pool = OutboxPool::new(&db_path, 4).await.unwrap();

        // SAFETY: test-only DDL — creates a temporary test table; no production schema dependency.
        sqlx::query("CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, val TEXT NOT NULL)")
            .execute(pool.inner())
            .await
            .unwrap();

        // Spawn 4 concurrent tasks that each insert and read
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let pool = pool.clone();
                tokio::spawn(async move {
                    let task_val = format!("task-{}", i);
                    // SAFETY: test-only DML — inserts test data into temporary test table.
                    sqlx::query("INSERT INTO test (val) VALUES (?)")
                        .bind(&task_val)
                        .execute(pool.inner())
                        .await
                        .unwrap();

                    // SAFETY: test-only read — verifies inserted test data in temporary test table.
                    let row: (String,) = sqlx::query_as("SELECT val FROM test WHERE val = ?")
                        .bind(&task_val)
                        .fetch_one(pool.inner())
                        .await
                        .unwrap();
                    format!("got: {}", row.0)
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.starts_with("got: task-"));
        }
    }
}
