//! SQLite connection pool wrapper using deadpool-sqlite
//!
//! Since `rusqlite::Connection` is `!Send`, all database operations must
//! go through `SyncWrapper::interact()`, which executes synchronous SQLite
//! calls on a blocking thread pool. The [`PooledConn`] type provides an
//! ergonomic async interface that hides this detail.

use deadpool_sqlite::{Config, InteractError, Pool, PoolError, Runtime};
use std::path::Path;
use std::time::Duration;

/// Configuration for the SQLite connection pool.
///
/// Controls pool sizing and timeout behaviour. Defaults match the previous
/// hard-coded values (`max_connections: 8`, `timeout: 30 s`).
///
/// # Environment variable overrides
///
/// | Variable | Field | Parsing |
/// |---|---|---|
/// | `NEXUS_DB_POOL_TIMEOUT_SECS` | `timeout` | Parsed as `u64`; falls back to default on missing/invalid |
/// | `NEXUS_DB_POOL_MAX_CONNECTIONS` | `max_connections` | Parsed as `usize`; falls back to default on missing/invalid |
///
/// # Tuning guidance
///
/// - **`timeout`** — maximum time to wait for an available connection.
///   Increase under heavy concurrent load; decrease to fail fast.
/// - **`max_connections`** — upper bound on open SQLite connections.
///   SQLite is file-level-locked, so very high values rarely improve
///   throughput; 8–16 is usually sufficient for the daemon.
///
/// # Pool Tuning Guidance
///
/// The default pool configuration is suitable for single-user local development:
/// - `max_connections: 8` — SQLite WAL mode supports 1 writer + N readers
/// - `timeout: 30s` — connection timeout for pool checkout
///
/// For embedded daemon use, reduce `max_connections` to 2–4.
/// Environment variables: `NEXUS_DB_POOL_TIMEOUT_SECS`, `NEXUS_DB_POOL_MAX_CONNECTIONS`
///
/// # Example
///
/// ```no_run
/// use nexus42d::db::pool::{DbPool, PoolConfig};
/// use std::time::Duration;
///
/// // Use environment overrides (suitable for production):
/// let pool = DbPool::new("nexus.db".as_ref(), PoolConfig::from_env())?;
///
/// // Or build programmatically:
/// let config = PoolConfig::default()
///     .with_timeout(Duration::from_secs(10))
///     .with_max_connections(4);
/// let pool = DbPool::new("nexus.db".as_ref(), config)?;
/// # Ok::<(), deadpool_sqlite::BuildError>(())
/// ```
#[derive(Clone, Debug)]
pub struct PoolConfig {
    /// Maximum time to wait for an available connection from the pool.
    pub timeout: Duration,
    /// Maximum number of connections in the pool.
    pub max_connections: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_connections: 8,
        }
    }
}

impl PoolConfig {
    /// Create a `PoolConfig` reading overrides from environment variables.
    ///
    /// See the [struct-level documentation](PoolConfig) for the variable names
    /// and fallback behaviour.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();

        if let Ok(val) = std::env::var("NEXUS_DB_POOL_TIMEOUT_SECS") {
            if let Ok(secs) = val.parse::<u64>() {
                cfg.timeout = Duration::from_secs(secs);
            }
        }

        if let Ok(val) = std::env::var("NEXUS_DB_POOL_MAX_CONNECTIONS") {
            if let Ok(max) = val.parse::<usize>() {
                cfg.max_connections = max;
            }
        }

        cfg
    }

    /// Set the pool wait timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of connections.
    pub fn with_max_connections(mut self, max_connections: usize) -> Self {
        self.max_connections = max_connections;
        self
    }
}

/// Wrapper around deadpool SQLite connection pool
///
/// Provides async connection retrieval for concurrent handler access
/// to the daemon's SQLite database (WAL mode enabled).
#[derive(Clone)]
pub struct DbPool {
    pool: Pool,
}

impl DbPool {
    /// Create a new connection pool with the given configuration.
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `config`  - Pool sizing and timeout configuration
    ///
    /// # SQLite Locking Strategy
    ///
    /// This pool uses SQLite in WAL (Write-Ahead Logging) mode, which allows:
    /// - Concurrent reads while a write is in progress
    /// - Better performance for read-heavy workloads
    ///
    /// WAL mode mitigates "database is locked" errors common in journal mode.
    /// Write contention is minimal since the daemon serves a single user locally.
    ///
    /// # Errors
    /// Returns `BuildError` if the pool cannot be created (e.g. invalid path)
    pub fn new(db_path: &Path, config: PoolConfig) -> Result<Self, deadpool_sqlite::BuildError> {
        let mut cfg = Config::new(db_path);
        cfg.pool = Some(deadpool_sqlite::PoolConfig {
            max_size: config.max_connections,
            timeouts: deadpool_sqlite::Timeouts {
                wait: Some(config.timeout),
                create: Some(config.timeout),
                recycle: Some(config.timeout),
            },
            ..deadpool_sqlite::PoolConfig::new(config.max_connections)
        });

        let pool = cfg
            .builder(Runtime::Tokio1)
            .expect("builder() is infallible for valid Runtime")
            .build()?;
        Ok(Self { pool })
    }

    /// Create a connection pool with default configuration.
    ///
    /// Convenience wrapper around [`DbPool::new`] with [`PoolConfig::default`].
    /// Useful in tests and simple setups where custom configuration is unnecessary.
    pub fn with_defaults(db_path: &Path) -> Result<Self, deadpool_sqlite::BuildError> {
        Self::new(db_path, PoolConfig::default())
    }

    /// Get a connection from the pool
    ///
    /// Returns a [`PooledConn`] that provides async wrappers around
    /// synchronous SQLite operations. The connection is returned to the
    /// pool when dropped.
    pub async fn get(&self) -> Result<PooledConn, PoolError> {
        self.pool.get().await.map(PooledConn)
    }

    /// Returns pool status information.
    ///
    /// Currently unused but retained for the planned `/health` monitoring endpoint (V1.2).
    #[allow(dead_code)]
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
        let pool = DbPool::new(&db_path, PoolConfig::default().with_max_connections(2))
            .expect("Pool creation should succeed");
        assert_eq!(pool.status().size, 0, "Pool should start empty");
    }

    #[tokio::test]
    async fn pool_get_returns_working_connection() {
        let (_tmp, db_path) = create_test_db();

        let pool = DbPool::new(&db_path, PoolConfig::default().with_max_connections(2)).unwrap();
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

        let pool = DbPool::new(&db_path, PoolConfig::default().with_max_connections(4)).unwrap();
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
        let pool = DbPool::new(
            &db_path,
            PoolConfig::default()
                .with_max_connections(1)
                .with_timeout(Duration::from_millis(50)),
        )
        .expect("Pool creation should succeed");

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

    // ── PoolConfig tests ──────────────────────────────────────────

    #[test]
    fn pool_config_default_values() {
        let cfg = PoolConfig::default();
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_connections, 8);
    }

    #[test]
    fn pool_config_builder_chaining() {
        let cfg = PoolConfig::default()
            .with_timeout(Duration::from_secs(5))
            .with_max_connections(16);
        assert_eq!(cfg.timeout, Duration::from_secs(5));
        assert_eq!(cfg.max_connections, 16);
    }

    #[test]
    fn pool_config_from_env_uses_defaults_when_unset() {
        // Ensure env vars are NOT set for this test
        std::env::remove_var("NEXUS_DB_POOL_TIMEOUT_SECS");
        std::env::remove_var("NEXUS_DB_POOL_MAX_CONNECTIONS");

        let cfg = PoolConfig::from_env();
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_connections, 8);
    }

    #[test]
    fn pool_config_from_env_reads_valid_values() {
        std::env::set_var("NEXUS_DB_POOL_TIMEOUT_SECS", "10");
        std::env::set_var("NEXUS_DB_POOL_MAX_CONNECTIONS", "4");

        let cfg = PoolConfig::from_env();
        assert_eq!(cfg.timeout, Duration::from_secs(10));
        assert_eq!(cfg.max_connections, 4);

        // Clean up
        std::env::remove_var("NEXUS_DB_POOL_TIMEOUT_SECS");
        std::env::remove_var("NEXUS_DB_POOL_MAX_CONNECTIONS");
    }

    #[test]
    fn pool_config_from_env_ignores_invalid_values() {
        std::env::set_var("NEXUS_DB_POOL_TIMEOUT_SECS", "not_a_number");
        std::env::set_var("NEXUS_DB_POOL_MAX_CONNECTIONS", "abc");

        let cfg = PoolConfig::from_env();
        // Should fall back to defaults
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_connections, 8);

        // Clean up
        std::env::remove_var("NEXUS_DB_POOL_TIMEOUT_SECS");
        std::env::remove_var("NEXUS_DB_POOL_MAX_CONNECTIONS");
    }

    #[test]
    fn with_defaults_creates_pool() {
        let (_tmp, db_path) = create_test_db();
        let pool = DbPool::with_defaults(&db_path).expect("Pool creation should succeed");
        assert_eq!(pool.status().size, 0, "Pool should start empty");
    }
}
