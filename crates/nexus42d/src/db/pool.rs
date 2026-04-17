//! SQLite connection pool wrapper using sqlx
//!
//! Provides async connection pooling for concurrent handler access
//! to the daemon's SQLite database (WAL mode enabled).

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
/// # async fn example() -> Result<(), nexus_local_db::LocalDbError> {
/// // Use environment overrides (suitable for production):
/// let _pool = DbPool::new("nexus.db".as_ref(), PoolConfig::from_env()).await?;
///
/// // Or build programmatically:
/// let config = PoolConfig::default()
///     .with_timeout(Duration::from_secs(10))
///     .with_max_connections(4);
/// let _pool = DbPool::new("nexus.db".as_ref(), config).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct PoolConfig {
    /// Maximum time to wait for an available connection from the pool.
    pub timeout: Duration,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
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
            if let Ok(max) = val.parse::<u32>() {
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
    pub fn with_max_connections(mut self, max_connections: u32) -> Self {
        self.max_connections = max_connections;
        self
    }
}

/// Wrapper around sqlx SQLite connection pool
///
/// Provides async connection retrieval for concurrent handler access
/// to the daemon's SQLite database (WAL mode enabled).
#[derive(Clone)]
pub struct DbPool {
    pool: sqlx::SqlitePool,
    /// Stored max_connections value for monitoring (sqlx doesn't expose a getter).
    max_connections: u32,
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
    /// Returns `LocalDbError` if the pool cannot be created (e.g. invalid path)
    pub async fn new(
        db_path: &Path,
        config: PoolConfig,
    ) -> Result<Self, nexus_local_db::LocalDbError> {
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.timeout)
            .connect(&url)
            .await
            .map_err(nexus_local_db::LocalDbError::from)?;
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;
        Ok(Self {
            pool,
            max_connections: config.max_connections,
        })
    }

    /// Create a connection pool with default configuration.
    ///
    /// Convenience wrapper around [`DbPool::new`] with [`PoolConfig::default`].
    /// Useful in tests and simple setups where custom configuration is unnecessary.
    pub async fn with_defaults(db_path: &Path) -> Result<Self, nexus_local_db::LocalDbError> {
        Self::new(db_path, PoolConfig::default()).await
    }

    /// Get a reference to the underlying sqlx pool.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Returns pool status information.
    ///
    /// Provides observability for database connection pool metrics.
    pub fn status(&self) -> PoolStatus {
        PoolStatus {
            max_size: self.max_connections as usize,
            size: self.pool.size() as usize,
        }
    }
}

/// Pool status information for monitoring.
#[derive(Debug, Clone)]
pub struct PoolStatus {
    /// Maximum pool capacity
    pub max_size: usize,
    /// Current total connections
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test database with schema via nexus_local_db
    async fn create_test_pool() -> (tempfile::TempDir, std::path::PathBuf, DbPool) {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        nexus_local_db::seed_versions(&pool).await.unwrap();

        let db_pool = DbPool::new(&db_path, PoolConfig::default().with_max_connections(2))
            .await
            .expect("Pool creation should succeed");

        (tmp, db_path, db_pool)
    }

    #[tokio::test]
    async fn pool_creates_successfully() {
        let (_tmp, _db_path, pool) = create_test_pool().await;
        let status = pool.status();
        assert!(
            status.size <= status.max_size,
            "Pool size {} should not exceed max {}",
            status.size,
            status.max_size,
        );
    }

    #[tokio::test]
    async fn pool_supports_concurrent_access() {
        let (_tmp, _db_path, pool) = create_test_pool().await;

        // Spawn 4 concurrent tasks that each insert and read
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let p = pool.clone();
                tokio::spawn(async move {
                    sqlx::query("INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?1, ?2, 'active', '2026-01-01T00:00:00Z', '{}')")
                        .bind(format!("ctr-{}", i))
                        .bind(format!("Creator {}", i))
                        .execute(p.pool())
                        .await
                        .unwrap();

                    let creator_id = format!("ctr-{}", i);
                    let row: Option<(String,)> = sqlx::query_as(
                        "SELECT display_name FROM creators WHERE creator_id = ?1"
                    )
                    .bind(&creator_id)
                    .fetch_optional(p.pool())
                    .await
                    .unwrap();
                    format!("got: {:?}", row)
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.starts_with("got: Some"));
        }
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
}
