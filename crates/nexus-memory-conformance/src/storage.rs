use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::FixtureError;

/// The fixture-owned schema, embedded at compile time.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// File-backed SQLite store owned outright by the fixture provider.
pub(crate) struct FixtureStore {
    pool: SqlitePool,
}

impl FixtureStore {
    /// Open (creating when absent) the scenario database file, then apply the
    /// fixture migrations. Reopening the same file is a no-op migration pass
    /// over the rows already stored in it.
    pub(crate) async fn open(database_path: &Path) -> Result<Self, FixtureError> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            // Two admitted clients write the same space, so readers must not
            // block the writer for the duration of a page read.
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// The pool every fixture operation runs its SQL against.
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Close every connection. The next open re-reads the same file.
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }
}
