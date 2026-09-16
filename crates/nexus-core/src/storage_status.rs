//! Storage diagnostics for the active workspace state DB (v1.190 P2-T0).
//!
//! Extracted from the CLI `system db status` command: schema versions,
//! health check, table list, and the journal/foreign-key pragmas — read
//! through a read-only pool so diagnostics never create, migrate, or seed
//! the database. Selecting a workspace
//! ([`CoreHomeService::select_workspace`]) is the initialization path.

use nexus_home_layout::active_context::try_resolve_state_db_path;
use nexus_local_db::{open_pool_read_only, read_versions, validate, RuntimeRole, SchemaVersions};

use crate::error::{local_db_err, CoreError, CoreResult};
use crate::home::CoreHomeService;

/// Read-only storage diagnostics for one state DB.
#[derive(Debug, Clone)]
pub struct CoreStorageStatus {
    /// Resolved state DB path under the ADR-014 layout.
    pub db_path: std::path::PathBuf,
    /// Schema versions when both version keys are present and parseable.
    pub versions: Option<CoreStorageVersions>,
    /// `true` when the read-only health check
    /// ([`nexus_local_db::validate`], CLI role) passes.
    pub healthy: bool,
    /// The health/versions failure reason when the store is not healthy.
    pub health_error: Option<String>,
    /// Existing tables, sorted by name.
    pub tables: Vec<String>,
    /// `PRAGMA journal_mode` value, when readable.
    pub journal_mode: Option<String>,
    /// `PRAGMA foreign_keys` value, when readable.
    pub foreign_keys: Option<i64>,
}

/// The two persisted schema version lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreStorageVersions {
    pub db_schema_version: u32,
    pub schema_version: u32,
}

impl From<SchemaVersions> for CoreStorageVersions {
    fn from(v: SchemaVersions) -> Self {
        Self {
            db_schema_version: v.db_schema_version,
            schema_version: v.schema_version,
        }
    }
}

impl CoreHomeService {
    /// Inspect the active selection's workspace state DB read-only.
    ///
    /// # Errors
    /// Returns [`CoreError::Uninitialized`] when no workspace selection can
    /// be resolved or the state DB does not exist (diagnostics never
    /// initialize storage), and the mapped storage error when the read-only
    /// pool cannot be opened.
    pub async fn storage_status(&self) -> CoreResult<CoreStorageStatus> {
        let db_path = try_resolve_state_db_path(&self.user_home, &self.nexus_home)
            .ok_or(CoreError::Uninitialized)?;
        if !db_path.exists() {
            return Err(CoreError::Uninitialized);
        }
        let pool = open_pool_read_only(&db_path).await.map_err(local_db_err)?;

        // Versions + health: collected honestly (no error→default fallback);
        // a failing check is reported with its reason, not swallowed.
        let (versions, health_error) = match read_versions(&pool).await {
            Ok(v) => {
                let health = validate(&pool, RuntimeRole::Cli).await;
                match health {
                    Ok(()) => (Some(CoreStorageVersions::from(v)), None),
                    Err(e) => (Some(CoreStorageVersions::from(v)), Some(e.to_string())),
                }
            }
            Err(e) => (None, Some(e.to_string())),
        };

        let tables_raw: Vec<Option<String>> =
            sqlx::query_scalar!("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .map_err(|e| crate::error::db_err(&e))?;
        let tables: Vec<String> = tables_raw.into_iter().flatten().collect();

        // SAFETY: PRAGMA statements — no table schema to validate against
        // (same precedent as the CLI db status command).
        let journal_mode: Option<String> = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
        let foreign_keys: Option<i32> = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
        pool.close().await;

        Ok(CoreStorageStatus {
            db_path,
            versions,
            healthy: health_error.is_none(),
            health_error,
            tables,
            journal_mode,
            foreign_keys: foreign_keys.map(i64::from),
        })
    }
}
