//! Connection-local SQLite scalar functions for the workspace writer protocol.
//!
//! All FFI is confined to [`sqlite`].

mod sqlite;

use std::fs::File;
use std::sync::Arc;

use sqlx::sqlite::SqliteConnection;

/// The complete OS admission a guarded connection belongs to.
///
/// These are the *originally acquired* admission lock handles: the shared
/// migration lock and, for an engine owner, the exclusive engine lock. The
/// writer guard and every guarded connection share ownership of one value, so
/// the OS locks stay held exactly as long as the last real guarded connection
/// (or the guard) exists and are released when SQLite destroys a connection's
/// userdata. Nothing here opens another lock file or re-acquires a lock.
pub struct WriterAdmission {
    _migration_lock: File,
    _engine_lock: Option<File>,
}

impl WriterAdmission {
    #[must_use]
    pub fn new(migration_lock: File, engine_lock: Option<File>) -> Self {
        Self {
            _migration_lock: migration_lock,
            _engine_lock: engine_lock,
        }
    }
}

impl std::fmt::Debug for WriterAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriterAdmission")
            .field("engine_owner", &self._engine_lock.is_some())
            .finish_non_exhaustive()
    }
}

/// Immutable connection-local writer context installed as SQLite userdata.
#[derive(Debug, Clone)]
pub struct WriterConnectionContext {
    pub writer_id: String,
    pub protocol_version: i32,
    pub mode: WriterMode,
    pub migration_epoch: i64,
    pub engine_epoch: Option<i64>,
    /// The complete OS admission this connection belongs to, if any.
    ///
    /// Held in the connection's UDF userdata, which SQLite destroys when the
    /// connection is actually disposed — so the admission's OS locks stay held
    /// for exactly as long as this connection can still run statements. It is
    /// `None` for protocol-internal short transactions, which are not a real
    /// user of the admission.
    pub admission: Option<Arc<WriterAdmission>>,
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
