//! Connection-local SQLite scalar functions for the workspace writer protocol.
//!
//! All FFI is confined to [`sqlite`].

mod sqlite;

use sqlx::sqlite::SqliteConnection;

/// Immutable connection-local writer context installed as SQLite userdata.
#[derive(Debug, Clone)]
pub struct WriterConnectionContext {
    pub writer_id: String,
    pub protocol_version: i32,
    pub mode: WriterMode,
    pub migration_epoch: i64,
    pub engine_epoch: Option<i64>,
}

/// Writer admission mode mirrored in SQL as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterMode {
    Direct,
    Engine,
    Migration,
}

impl WriterMode {
    #[must_use]
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Engine => "engine",
            Self::Migration => "migration",
        }
    }
}

/// Install the five protocol scalar functions on a single SQLite connection.
pub async fn install_writer_functions(
    conn: &mut SqliteConnection,
    context: WriterConnectionContext,
) -> Result<(), sqlx::Error> {
    sqlite::install_writer_functions(conn, context).await
}
