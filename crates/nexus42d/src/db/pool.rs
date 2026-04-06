//! SQLite connection pool wrapper using deadpool-sqlite
//!
//! Since `rusqlite::Connection` is `!Send`, all database operations must
//! go through `SyncWrapper::interact()`, which executes synchronous SQLite
//! calls on a blocking thread pool. The [`PooledConn`] type provides an
//! ergonomic async interface that hides this detail.

use deadpool_sqlite::{Config, InteractError, Pool, PoolError, Runtime};
use std::path::Path;

/// Default pool size for the daemon
pub const DEFAULT_POOL_SIZE: usize = 8;

/// Wrapper around deadpool SQLite connection pool
///
/// Provides async connection retrieval for concurrent handler access
/// to the daemon's SQLite database (WAL mode enabled).
#[derive(Clone)]
pub struct DbPool {
    pool: Pool,
}

impl DbPool {
    /// Create a new connection pool for the given database path
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `max_size` - Maximum number of connections in the pool
    ///
    /// # Errors
    /// Returns `BuildError` if the pool cannot be created (e.g. invalid path)
    pub fn new(db_path: &Path, max_size: usize) -> Result<Self, deadpool_sqlite::BuildError> {
        let cfg = Config::new(db_path);
        let pool = cfg
            .builder(Runtime::Tokio1)
            .expect("builder() is infallible for valid Runtime")
            .max_size(max_size)
            .build()?;
        Ok(Self { pool })
    }

    /// Get a connection from the pool
    ///
    /// Returns a [`PooledConn`] that provides async wrappers around
    /// synchronous SQLite operations. The connection is returned to the
    /// pool when dropped.
    pub async fn get(&self) -> Result<PooledConn, PoolError> {
        self.pool.get().await.map(PooledConn)
    }

    /// Get the current pool status
    pub fn status(&self) -> deadpool_sqlite::Status {
        self.pool.status()
    }
}

/// A pooled SQLite connection with async-friendly wrappers
///
/// Wraps `deadpool_sqlite::Object` to provide ergonomic async methods
/// that internally use `SyncWrapper::interact()` to execute synchronous
/// SQLite calls on a blocking thread.
pub struct PooledConn(deadpool_sqlite::Object);

impl PooledConn {
    /// Execute a SQL statement with parameters
    ///
    /// Returns the number of rows affected.
    pub async fn execute<P>(&self, sql: &str, params: P) -> Result<usize, rusqlite::Error>
    where
        P: rusqlite::Params + Send + 'static,
    {
        let sql = sql.to_string();
        self.0
            .interact(move |conn| conn.execute(&sql, params))
            .await
            .map_err(interact_to_rusqlite_err)?
    }

    /// Execute a query and collect all rows using the provided mapping closure
    pub async fn query_map<T, P, F>(
        &self,
        sql: &str,
        params: P,
        map_row: F,
    ) -> Result<Vec<T>, rusqlite::Error>
    where
        P: rusqlite::Params + Send + 'static,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let sql = sql.to_string();
        self.0
            .interact(move |conn| {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params, map_row)?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row?);
                }
                Ok::<Vec<T>, rusqlite::Error>(results)
            })
            .await
            .map_err(interact_to_rusqlite_err)?
    }

    /// Execute a query and return the first row mapped by the closure
    ///
    /// Returns `Ok(None)` if no rows match, or `Ok(Some(T))` if a row is found.
    pub async fn query_row<T, P, F>(
        &self,
        sql: &str,
        params: P,
        map_row: F,
    ) -> Result<Option<T>, rusqlite::Error>
    where
        P: rusqlite::Params + Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let sql = sql.to_string();
        self.0
            .interact(move |conn| match conn.query_row(&sql, params, map_row) {
                Ok(val) => Ok(Some(val)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            })
            .await
            .map_err(interact_to_rusqlite_err)?
    }

    /// Execute a raw closure against the underlying connection
    ///
    /// Use this for operations not covered by the convenience methods
    /// (e.g., transactions, batch statements).
    pub async fn interact<F, R>(&self, f: F) -> Result<R, InteractError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> R + Send + 'static,
        R: Send + 'static,
    {
        self.0.interact(f).await
    }
}

/// Convert `InteractError` to a `rusqlite::Error` for ergonomic error handling
fn interact_to_rusqlite_err(e: InteractError) -> rusqlite::Error {
    match e {
        InteractError::Panic(p) => {
            rusqlite::Error::InvalidParameterName(format!("Connection interact panicked: {:?}", p))
        }
        InteractError::Aborted => {
            rusqlite::Error::InvalidParameterName("Connection interact aborted".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test database with schema
    fn create_test_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, val TEXT NOT NULL);",
        )
        .unwrap();
        drop(conn);

        (tmp, db_path)
    }

    #[test]
    fn pool_creates_successfully() {
        let (_tmp, db_path) = create_test_db();
        let pool = DbPool::new(&db_path, 2).expect("Pool creation should succeed");
        assert_eq!(pool.status().size, 0, "Pool should start empty");
    }

    #[tokio::test]
    async fn pool_get_returns_working_connection() {
        let (_tmp, db_path) = create_test_db();

        let pool = DbPool::new(&db_path, 2).unwrap();
        let conn = pool.get().await.expect("Should get connection");

        conn.execute("INSERT INTO test (val) VALUES (?1)", ["hello"])
            .await
            .unwrap();

        let val: Option<String> = conn
            .query_row("SELECT val FROM test WHERE id = 1", [], |row| row.get(0))
            .await
            .unwrap();
        assert_eq!(val, Some("hello".to_string()));

        drop(conn);
        assert_eq!(pool.status().size, 1, "Connection should be returned");
    }

    #[tokio::test]
    async fn pool_supports_concurrent_access() {
        let (_tmp, db_path) = create_test_db();

        let pool = DbPool::new(&db_path, 4).unwrap();
        let pool_clone = pool.clone();

        // Spawn 4 concurrent tasks that each insert and read
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let p = pool_clone.clone();
                tokio::spawn(async move {
                    let conn = p.get().await.unwrap();
                    conn.execute(
                        "INSERT INTO test (val) VALUES (?1)",
                        [format!("task-{}", i)],
                    )
                    .await
                    .unwrap();

                    let task_val = format!("task-{}", i);
                    let val: Option<String> = conn
                        .query_row("SELECT val FROM test WHERE val = ?1", [task_val], |row| {
                            row.get(0)
                        })
                        .await
                        .unwrap();
                    format!("got: {}", val.unwrap())
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

    #[tokio::test]
    async fn pool_exhaustion_returns_error_gracefully() {
        let (_tmp, db_path) = create_test_db();

        // Build pool with max_size = 1 and short wait timeout so test doesn't hang
        let mut cfg = Config::new(&db_path);
        let pool_config = deadpool_sqlite::PoolConfig::new(1);
        cfg.pool = Some(deadpool_sqlite::PoolConfig {
            max_size: 1,
            timeouts: deadpool_sqlite::Timeouts::wait_millis(50),
            ..pool_config
        });
        let inner_pool = cfg
            .builder(Runtime::Tokio1)
            .expect("builder() is infallible for valid Runtime")
            .build()
            .expect("Pool creation should succeed");
        let pool = DbPool { pool: inner_pool };

        // Acquire the only connection and hold it
        let conn = pool.get().await.expect("Should get first connection");
        assert_eq!(pool.status().size, 1);

        // Attempting to get a second connection should fail with a timeout
        let result = pool.get().await;
        assert!(result.is_err(), "Expected pool exhaustion error");

        // Verify the error is a pool error (not a panic) — inspect via match
        let _pool_err = match result {
            Err(e) => {
                // Expected: PoolError::Timeout or any other pool-level error
                // The key invariant is we get a PoolError, not a panic
                let _msg = format!("{e}");
                e
            }
            Ok(_) => panic!("Expected pool exhaustion error, got Ok"),
        };

        // Drop the held connection — pool should recover
        drop(conn);
        assert_eq!(
            pool.status().size,
            1,
            "Connection should be returned to pool"
        );
    }
}
