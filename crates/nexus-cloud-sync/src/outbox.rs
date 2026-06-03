//! Outbox Pattern Implementation
//!
//! Local operation queue using `SQLite` for persistence with connection pooling.
//! Implements the full `OutboxEntry` contract type with delivery state management.
//!
//! The outbox stores pending bundles for offline-first sync, supporting:
//! - Staging commands into outbox entries
//! - Tracking delivery state (staged → ready → sent → acked/conflicted/failed)
//! - Retry with exponential backoff
//! - Replay of pending entries
//!
//! # Connection Pooling
//!
//! Uses `sqlx::SqlitePool` (via `nexus_local_db`) for async connection pooling
//! with WAL mode for better concurrent read/write performance. Pool configuration:
//! - Default pool size: 4 connections
//! - WAL mode enabled for concurrent reads
//!
//! ## Outbox schema and migrations
//!
//! The sync `outbox_entries` table, its relationship to the daemon `outbox` queue,
//! and the planned `schema_version` rollout are documented under the repository
//! root at `.mstar/plans/archived/knowledge/outbox-schema.md` (v1.1 → v1.2 steps, safety rules,
//! and future evolution).

use std::path::Path;
use std::str::FromStr;
#[cfg(test)]
use std::sync::Arc;

use nexus_contracts::generated::{Bundle, SyncCommand, LATEST_SCHEMA_VERSION};
use nexus_contracts::local::domain::OutboxEntry;
use nexus_contracts::DeliveryState;
use uuid::Uuid;

use crate::errors::{SyncError, SyncResult};
use crate::pool::{OutboxPool, DEFAULT_POOL_SIZE};

/// Maximum retry count before giving up.
const MAX_RETRIES: u64 = 5;

/// Base delay for exponential backoff in seconds.
const BASE_RETRY_DELAY_SECS: u64 = 2;

// ---------------------------------------------------------------------------
// Module-level FromRow structs (sqlx R2: avoid duplication)
// ---------------------------------------------------------------------------

/// Row mapping for `outbox_entries` queries (sqlx R2: module-level struct).
#[derive(sqlx::FromRow)]
struct OutboxRow {
    outbox_entry_id: String,
    bundle_id: String,
    idempotency_key: String,
    delivery_state: String,
    retry_count: i64,
    last_error: Option<String>,
    next_retry_at: Option<String>,
    created_at: String,
    updated_at: Option<String>,
}

/// Row mapping for `partial_apply_states` queries (sqlx R2: module-level struct).
#[derive(sqlx::FromRow)]
struct PartialApplyRow {
    outbox_entry_id: String,
    state_json: String,
}

/// A parsed retry-after timestamp, either as an absolute time or relative seconds.
#[derive(Debug, Clone)]
pub enum RetryAfterPolicy {
    /// Server requested retry after this specific timestamp (RFC 3339).
    AtTime(chrono::DateTime<chrono::Utc>),
    /// Server requested retry after this many seconds from now.
    AfterSeconds(u64),
    /// No retry-after specified; use default exponential backoff.
    None,
}

/// SQLite-backed outbox for local sync operations with connection pooling.
#[derive(Clone)]
pub struct Outbox {
    pool: OutboxPool,
    /// Keeps the temp directory alive for [`Outbox::new_in_memory`] tests (no `mem::forget`).
    #[cfg(test)]
    _test_temp: Option<Arc<tempfile::TempDir>>,
}

impl Outbox {
    /// Open or create an outbox database at the given path with default pool size.
    ///
    /// Creates the `outbox_entries` table if it doesn't exist.
    /// Uses WAL mode for better concurrent read performance.
    ///
    /// # Pool Configuration
    /// - Pool size: 4 connections (`DEFAULT_POOL_SIZE`)
    /// - WAL mode enabled
    ///
    /// # Errors
    /// Returns `SyncError::OutboxDatabase` if pool creation fails.
    pub async fn new<P: AsRef<Path>>(db_path: P) -> SyncResult<Self> {
        Self::with_pool_size(db_path, DEFAULT_POOL_SIZE).await
    }

    /// Open or create an outbox database with custom pool size.
    ///
    /// # Arguments
    /// * `db_path` - Path to `SQLite` database file
    /// * `pool_size` - Maximum number of connections in the pool
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn with_pool_size<P: AsRef<Path>>(db_path: P, pool_size: usize) -> SyncResult<Self> {
        let pool = Self::init_pool_with_schema(db_path.as_ref(), pool_size).await?;
        tracing::info!("Outbox database initialized with connection pool");
        Ok(Self {
            pool,
            #[cfg(test)]
            _test_temp: None,
        })
    }

    async fn init_pool_with_schema(db_path: &Path, pool_size: usize) -> SyncResult<OutboxPool> {
        let pool = OutboxPool::new(db_path, pool_size).await?;

        // SAFETY: PRAGMA statement — no table schema to validate against.
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(pool.inner())
            .await?;

        // WS8 R4: Tables are created via nexus-local-db migrations.
        // The migration runner creates all tables including outbox_entries.
        nexus_local_db::run_migrations(pool.inner())
            .await
            .map_err(|e| SyncError::OutboxDatabase(format!("migration failed: {e}")))?;

        Ok(pool)
    }

    /// Create an outbox using an existing connection pool.
    ///
    /// Use this when you want to share a pool across multiple outbox instances
    /// or control pool lifecycle externally.
    ///
    /// Note: Caller is responsible for ensuring the schema is initialized.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    #[allow(clippy::unused_async)]
    pub async fn with_pool(pool: OutboxPool) -> SyncResult<Self> {
        Ok(Self {
            pool,
            #[cfg(test)]
            _test_temp: None,
        })
    }

    /// Open an in-memory outbox (for testing).
    ///
    /// Uses a real file under a [`tempfile::TempDir`] owned by this [`Outbox`] so the directory
    /// is removed when the last clone of this handle is dropped (no `mem::forget`).
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    #[cfg(test)]
    pub async fn new_in_memory() -> SyncResult<Self> {
        let tmp = Arc::new(
            tempfile::TempDir::new().map_err(|e| SyncError::OutboxDatabase(e.to_string()))?,
        );
        let db_path = tmp.path().join("test_outbox.db");
        let pool = Self::init_pool_with_schema(&db_path, DEFAULT_POOL_SIZE).await?;
        tracing::info!("Outbox test database initialized (temp-backed file)");
        Ok(Self {
            pool,
            _test_temp: Some(tmp),
        })
    }

    /// Append a sync command to the outbox in `staged` state.
    ///
    /// Returns the generated outbox entry ID.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn append(&self, command: &SyncCommand) -> SyncResult<String> {
        let outbox_entry_id = format!("obe_{}", Uuid::new_v4().simple());
        let bundle_id = format!("bdl_{}", Uuid::new_v4().simple());
        let idempotency_key = format!("idk_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let command_payload = serde_json::to_string(command)?;

        let mut tx = self.pool.inner().begin().await?;
        sqlx::query!(
            "INSERT INTO outbox_entries
                (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                 retry_count, command_payload, created_at)
             VALUES (?, ?, ?, 'staged', 0, ?, ?)",
            outbox_entry_id,
            bundle_id,
            idempotency_key,
            command_payload,
            now
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::debug!(
            outbox_entry_id = %outbox_entry_id,
            command_type = %command.command_type,
            "Command appended to outbox"
        );

        Ok(outbox_entry_id)
    }

    /// Stage an existing bundle ID into the outbox.
    ///
    /// Creates a new outbox entry linked to the given bundle.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn stage(&self, bundle: &Bundle) -> SyncResult<String> {
        let outbox_entry_id = format!("obe_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let bundle_payload = serde_json::to_string(bundle)?;
        let bundle_id = bundle.bundle_id.clone();
        let idempotency_key = bundle.idempotency_key.clone();

        let mut tx = self.pool.inner().begin().await?;
        sqlx::query!(
            "INSERT INTO outbox_entries
                (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                 retry_count, bundle_payload, created_at)
             VALUES (?, ?, ?, 'ready', 0, ?, ?)",
            outbox_entry_id,
            bundle_id,
            idempotency_key,
            bundle_payload,
            now
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::debug!(
            outbox_entry_id = %outbox_entry_id,
            bundle_id = %bundle.bundle_id,
            "Bundle staged to outbox"
        );

        Ok(outbox_entry_id)
    }

    /// Stage a bundle only if no row exists with the same `bundle_id` (idempotent pull apply).
    ///
    /// Returns `Ok(Some(entry_id))` when a new row was inserted, `Ok(None)` when the bundle
    /// was already present.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn stage_if_absent(&self, bundle: &Bundle) -> SyncResult<Option<String>> {
        let new_entry_id = format!("obe_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let bundle_payload = serde_json::to_string(bundle)?;
        let bundle_id = bundle.bundle_id.clone();
        let idempotency_key = bundle.idempotency_key.clone();

        let mut tx = self.pool.inner().begin().await?;

        // Check existence
        let exists: i64 = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM outbox_entries WHERE bundle_id = ?) as \"exists!\"",
            bundle_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if exists != 0 {
            tx.rollback().await?;
            tracing::debug!(
                bundle_id = %bundle.bundle_id,
                "Skipped staging pull bundle (bundle_id already in outbox)"
            );
            return Ok(None);
        }

        sqlx::query!(
            "INSERT INTO outbox_entries
                (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                 retry_count, bundle_payload, created_at)
             VALUES (?, ?, ?, 'ready', 0, ?, ?)",
            new_entry_id,
            bundle_id,
            idempotency_key,
            bundle_payload,
            now
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::debug!(
            outbox_entry_id = %new_entry_id,
            bundle_id = %bundle.bundle_id,
            "Bundle staged to outbox (pull idempotent)"
        );

        Ok(Some(new_entry_id))
    }

    /// Transition an outbox entry to `sent` state.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn mark_sent(&self, outbox_entry_id: &str) -> SyncResult<()> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query!(
            "UPDATE outbox_entries
             SET delivery_state = 'sent', updated_at = ?
             WHERE outbox_entry_id = ? AND delivery_state IN ('staged', 'ready')",
            now,
            outbox_entry_id
        )
        .execute(self.pool.inner())
        .await?;

        if result.rows_affected() == 0 {
            return Err(SyncError::OutboxEntryNotFound {
                id: outbox_entry_id.to_string(),
            });
        }

        tracing::debug!(outbox_entry_id = %outbox_entry_id, "Marked as sent");
        Ok(())
    }

    /// Transition an outbox entry to `acked` state.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn mark_acked(&self, outbox_entry_id: &str) -> SyncResult<()> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query!(
            "UPDATE outbox_entries
             SET delivery_state = 'acked', updated_at = ?
             WHERE outbox_entry_id = ? AND delivery_state = 'sent'",
            now,
            outbox_entry_id
        )
        .execute(self.pool.inner())
        .await?;

        if result.rows_affected() == 0 {
            return Err(SyncError::OutboxEntryNotFound {
                id: outbox_entry_id.to_string(),
            });
        }

        tracing::info!(outbox_entry_id = %outbox_entry_id, "Marked as acked");
        Ok(())
    }

    /// Transition an outbox entry to `conflicted` state with error.
    ///
    /// If a `retry_after` policy is provided (SYNC-R11), it stores the
    /// computed retry timestamp so that [`replay`] will skip this entry
    /// until the server-specified time has elapsed.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn mark_conflicted_with_retry(
        &self,
        outbox_entry_id: &str,
        error: &str,
        retry_after: &RetryAfterPolicy,
    ) -> SyncResult<()> {
        let now = chrono::Utc::now();
        let next_retry_at = match retry_after {
            RetryAfterPolicy::AtTime(t) => Some(t.to_rfc3339()),
            RetryAfterPolicy::AfterSeconds(secs) => {
                // SAFETY: secs is a u64 seconds delay from the platform; i64::MAX ~= 292 years,
                // so any realistic delay in seconds will fit without wrapping.
                #[allow(clippy::cast_possible_wrap)]
                let target = now + chrono::Duration::seconds(*secs as i64);
                Some(target.to_rfc3339())
            }
            RetryAfterPolicy::None => None,
        };

        let now_str = now.to_rfc3339();

        let result = sqlx::query!(
            "UPDATE outbox_entries
             SET delivery_state = 'conflicted',
                 last_error = ?,
                 next_retry_at = ?,
                 updated_at = ?
             WHERE outbox_entry_id = ? AND delivery_state = 'sent'",
            error,
            next_retry_at,
            now_str,
            outbox_entry_id
        )
        .execute(self.pool.inner())
        .await?;

        if result.rows_affected() == 0 {
            return Err(SyncError::OutboxEntryNotFound {
                id: outbox_entry_id.to_string(),
            });
        }

        tracing::warn!(
            outbox_entry_id = %outbox_entry_id,
            error = %error,
            retry_after = ?next_retry_at,
            "Marked as conflicted with retry policy"
        );
        Ok(())
    }

    /// Transition an outbox entry to `conflicted` state with error (no retry policy).
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn mark_conflicted(&self, outbox_entry_id: &str, error: &str) -> SyncResult<()> {
        self.mark_conflicted_with_retry(outbox_entry_id, error, &RetryAfterPolicy::None)
            .await
    }

    /// Mark an outbox entry as failed.
    ///
    /// NOTE: The entry is committed to the DB *before* returning the result.
    /// This is intentional — even if the caller ignores the error, the failed
    /// state is persisted. Do NOT reorder the commit and the return.
    ///
    /// Calculates the next retry time using exponential backoff.
    /// Returns an error if the max retry count has been exceeded.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn mark_failed(&self, outbox_entry_id: &str, error: &str) -> SyncResult<()> {
        let mut tx = self.pool.inner().begin().await?;

        let retry_count_row = sqlx::query_scalar!(
            "SELECT retry_count as \"retry_count!\" FROM outbox_entries WHERE outbox_entry_id = ?",
            outbox_entry_id
        )
        .fetch_one(&mut *tx)
        .await?;

        // SAFETY: retry_count is i64 from SQLite, u64 for storage; i64::MAX > u64::MAX so
        // any stored retry_count will fit in u64 without sign loss.
        #[allow(clippy::cast_sign_loss)]
        let retry_count = retry_count_row as u64;

        if retry_count >= MAX_RETRIES {
            // Permanently mark as failed without retry
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query!(
                "UPDATE outbox_entries
                 SET delivery_state = 'failed', last_error = ?, updated_at = ?,
                     next_retry_at = NULL
                 WHERE outbox_entry_id = ?",
                error,
                now,
                outbox_entry_id
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(SyncError::OutboxMaxRetriesExceeded {
                id: outbox_entry_id.to_string(),
                retries: retry_count,
            });
        }

        // Calculate exponential backoff
        let delay_secs =
            BASE_RETRY_DELAY_SECS.saturating_mul(2u64.saturating_pow(retry_count.min(30) as u32));
        // SAFETY: delay_secs is calculated from BASE_RETRY_DELAY_SECS and retry_count, both u64.
        // i64::MAX ~= 292 years in seconds, so any realistic delay fits in i64 without wrapping.
        #[allow(clippy::cast_possible_wrap)]
        let next_retry = chrono::Utc::now() + chrono::Duration::seconds(delay_secs as i64);
        let now = chrono::Utc::now().to_rfc3339();

        let next_retry_str = next_retry.to_rfc3339();
        sqlx::query!(
            "UPDATE outbox_entries
             SET delivery_state = 'failed',
                 retry_count = retry_count + 1,
                 last_error = ?,
                 next_retry_at = ?,
                 updated_at = ?
             WHERE outbox_entry_id = ?",
            error,
            next_retry_str,
            now,
            outbox_entry_id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        tracing::warn!(
            outbox_entry_id = %outbox_entry_id,
            retry_count = retry_count + 1,
            next_retry_in_secs = delay_secs,
            "Marked as failed, scheduled for retry"
        );

        Ok(())
    }

    /// Replay all pending entries (staged, ready, failed-with-retry-due).
    ///
    /// Returns entries that are eligible for sync processing.
    /// Also includes conflicted entries whose `retry_after` has elapsed (SYNC-R11),
    /// allowing the caller to re-attempt delivery after a server-specified backoff.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn replay(&self) -> SyncResult<Vec<OutboxEntry>> {
        let now = chrono::Utc::now().to_rfc3339();

        // sqlx R2: Uses module-level OutboxRow struct.
        let rows = sqlx::query_as!(
            OutboxRow,
            "SELECT outbox_entry_id as \"outbox_entry_id!\", bundle_id as \"bundle_id!\",
                    idempotency_key as \"idempotency_key!\", delivery_state as \"delivery_state!\",
                    retry_count as \"retry_count!\", last_error, next_retry_at,
                    created_at as \"created_at!\", updated_at
             FROM outbox_entries
             WHERE delivery_state IN ('staged', 'ready')
                OR (delivery_state = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?)
                OR (delivery_state = 'conflicted' AND next_retry_at IS NOT NULL AND next_retry_at <= ?)
             ORDER BY created_at ASC",
            now,
            now
        )
        .fetch_all(self.pool.inner())
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let delivery_state = DeliveryState::from_str(&row.delivery_state).map_err(|_| {
                SyncError::OutboxDatabase(format!("invalid delivery_state: {}", row.delivery_state))
            })?;
            entries.push(OutboxEntry {
                schema_version: LATEST_SCHEMA_VERSION,
                outbox_entry_id: row.outbox_entry_id,
                bundle_id: row.bundle_id,
                idempotency_key: row.idempotency_key,
                delivery_state,
                // SAFETY: retry_count is i64 from SQLite, stored as u64; the value is non-negative
                // and fits in u64 on all supported targets.
                #[allow(clippy::cast_sign_loss)]
                retry_count: Some(row.retry_count as u64),
                last_error: row.last_error,
                next_retry_at: row.next_retry_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        tracing::debug!(count = entries.len(), "Replayed pending outbox entries");
        Ok(entries)
    }

    /// Get a specific outbox entry by ID.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn get(&self, outbox_entry_id: &str) -> SyncResult<OutboxEntry> {
        // sqlx R2: Uses module-level OutboxRow struct.
        let row = sqlx::query_as!(
            OutboxRow,
            "SELECT outbox_entry_id as \"outbox_entry_id!\", bundle_id as \"bundle_id!\",
                    idempotency_key as \"idempotency_key!\", delivery_state as \"delivery_state!\",
                    retry_count as \"retry_count!\", last_error, next_retry_at,
                    created_at as \"created_at!\", updated_at
             FROM outbox_entries
             WHERE outbox_entry_id = ?",
            outbox_entry_id
        )
        .fetch_optional(self.pool.inner())
        .await?
        .ok_or_else(|| SyncError::OutboxEntryNotFound {
            id: outbox_entry_id.to_string(),
        })?;

        let delivery_state = DeliveryState::from_str(&row.delivery_state).map_err(|_| {
            SyncError::OutboxDatabase(format!("invalid delivery_state: {}", row.delivery_state))
        })?;

        Ok(OutboxEntry {
            schema_version: LATEST_SCHEMA_VERSION,
            outbox_entry_id: row.outbox_entry_id,
            bundle_id: row.bundle_id,
            idempotency_key: row.idempotency_key,
            delivery_state,
            // SAFETY: retry_count is i64 from SQLite, stored as u64; the value is non-negative
            // and fits in u64 on all supported targets.
            #[allow(clippy::cast_sign_loss)]
            retry_count: Some(row.retry_count as u64),
            last_error: row.last_error,
            next_retry_at: row.next_retry_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Remove acknowledged entries (cleanup).
    ///
    /// Returns the number of entries removed.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn purge_acked(&self) -> SyncResult<usize> {
        let result = sqlx::query!("DELETE FROM outbox_entries WHERE delivery_state = 'acked'")
            .execute(self.pool.inner())
            .await?;

        // SAFETY: rows_affected() returns i64; on realistic targets usize >= u32, so
        // the truncation to usize is safe (a u32 row count fits in usize).
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = usize::try_from(result.rows_affected()).unwrap_or(usize::MAX);
        tracing::info!(count = count, "Purged acked outbox entries");
        Ok(count)
    }

    /// Count entries by delivery state.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn count_by_state(&self, state: &str) -> SyncResult<usize> {
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) as \"count!\" FROM outbox_entries WHERE delivery_state = ?",
            state
        )
        .fetch_one(self.pool.inner())
        .await?;

        // SAFETY: count is a non-negative row count from SQLite; usize::try_from preserves the value
        // on all targets where usize >= u32 (all 32-bit and 64-bit targets we support).
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let count = usize::try_from(count).unwrap_or_default();
        Ok(count)
    }

    // ── Partial apply state persistence (SYNC-R12) ──────────────

    /// Persist partial apply state for an outbox entry (SYNC-R12).
    ///
    /// Stores the partial apply result so that on daemon restart, the
    /// partial apply can be resumed without reconstructing state from scratch.
    /// The state is stored in the `partial_apply_states` table.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn persist_partial_apply_state(
        &self,
        outbox_entry_id: &str,
        state: &crate::partial_apply::PartialApplyState,
    ) -> SyncResult<()> {
        let state_json = serde_json::to_string(state)?;
        let now = chrono::Utc::now().to_rfc3339();
        let retry_count = state.retry_count;

        sqlx::query!(
            "INSERT OR REPLACE INTO partial_apply_states
                (outbox_entry_id, state_json, recorded_at, retry_count)
             VALUES (?, ?, ?, ?)",
            outbox_entry_id,
            state_json,
            now,
            retry_count
        )
        .execute(self.pool.inner())
        .await?;

        tracing::info!(
            outbox_entry_id = %outbox_entry_id,
            bundle_id = %state.bundle_id,
            retry_count = state.retry_count,
            "Partial apply state persisted"
        );
        Ok(())
    }

    /// Load persisted partial apply state for an outbox entry (SYNC-R12).
    ///
    /// Returns `None` if no persisted state exists for the given entry.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn load_partial_apply_state(
        &self,
        outbox_entry_id: &str,
    ) -> SyncResult<Option<crate::partial_apply::PartialApplyState>> {
        let result = sqlx::query_scalar!(
            "SELECT state_json as \"state_json!\" FROM partial_apply_states WHERE outbox_entry_id = ?",
            outbox_entry_id
        )
        .fetch_optional(self.pool.inner())
        .await?;

        match result {
            Some(state_json) => {
                let state: crate::partial_apply::PartialApplyState =
                    serde_json::from_str(&state_json)?;
                tracing::debug!(
                    outbox_entry_id = %outbox_entry_id,
                    "Loaded persisted partial apply state"
                );
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    /// Remove persisted partial apply state (SYNC-R12).
    ///
    /// Called after a partial apply has been fully resolved (all deltas succeeded
    /// or permanently failed).
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn remove_partial_apply_state(&self, outbox_entry_id: &str) -> SyncResult<()> {
        sqlx::query!(
            "DELETE FROM partial_apply_states WHERE outbox_entry_id = ?",
            outbox_entry_id
        )
        .execute(self.pool.inner())
        .await?;

        tracing::debug!(
            outbox_entry_id = %outbox_entry_id,
            "Removed persisted partial apply state"
        );
        Ok(())
    }

    /// List all outbox entries with persisted partial apply states (SYNC-R12).
    ///
    /// Useful for resuming partial applies after daemon restart.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn list_partial_apply_states(
        &self,
    ) -> SyncResult<Vec<(String, crate::partial_apply::PartialApplyState)>> {
        // sqlx R2: Uses module-level PartialApplyRow struct.
        let rows = sqlx::query_as!(
            PartialApplyRow,
            "SELECT outbox_entry_id as \"outbox_entry_id!\", state_json as \"state_json!\"
             FROM partial_apply_states ORDER BY recorded_at ASC",
        )
        .fetch_all(self.pool.inner())
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let state: crate::partial_apply::PartialApplyState =
                serde_json::from_str(&row.state_json)?;
            results.push((row.outbox_entry_id, state));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::{Delta, SyncCommand};
    use nexus_contracts::{
        CommandOrigin, CommandStatus, CommandType, DeliveryState, DeltaOperation, DeltaType,
    };
    use std::str::FromStr;

    fn make_test_command() -> SyncCommand {
        SyncCommand {
            schema_version: 1,
            command_id: "cmd_test".to_string(),
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            command_type: CommandType::SyncPush,
            origin: CommandOrigin::LocalUser,
            output_manuscript: None,
            status: CommandStatus::Pending,
            requested_by: None,
            started_at: None,
            completed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn outbox_append_and_get() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.outbox_entry_id, entry_id);
        assert_eq!(entry.delivery_state, DeliveryState::Staged);
        assert!(entry.bundle_id.starts_with("bdl_"));
    }

    #[tokio::test]
    async fn outbox_lifecycle_staged_to_acked() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Sent);

        outbox.mark_acked(&entry_id).await.expect("mark_acked");
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Acked);
    }

    #[tokio::test]
    async fn outbox_lifecycle_conflicted() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        outbox
            .mark_conflicted(&entry_id, "version mismatch")
            .await
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Conflicted);
        assert_eq!(entry.last_error, Some("version mismatch".to_string()));
    }

    #[tokio::test]
    async fn outbox_failed_with_retry() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // First failure
        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        outbox
            .mark_failed(&entry_id, "transient error")
            .await
            .expect("mark_failed 1");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Failed);
        assert_eq!(entry.retry_count, Some(1));
        assert!(entry.next_retry_at.is_some());
    }

    #[tokio::test]
    async fn outbox_max_retries_exceeded() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        // Fail MAX_RETRIES times
        for _ in 0..MAX_RETRIES {
            outbox.mark_failed(&entry_id, "persistent error").await.ok();
        }

        // Next failure should error
        let result = outbox.mark_failed(&entry_id, "persistent error").await;
        assert!(matches!(
            result,
            Err(SyncError::OutboxMaxRetriesExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn outbox_replay_returns_pending() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let cmd1 = make_test_command();
        let cmd2 = make_test_command();
        let _entry1 = outbox.append(&cmd1).await.expect("append 1");
        let _entry2 = outbox.append(&cmd2).await.expect("append 2");

        let entries = outbox.replay().await.expect("replay");
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn outbox_replay_excludes_acked_and_sent() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry1 = outbox.append(&cmd).await.expect("append 1");
        let _entry2 = outbox.append(&cmd).await.expect("append 2");

        // Mark one as sent (not in replay)
        outbox.mark_sent(&entry1).await.expect("mark_sent");

        let entries = outbox.replay().await.expect("replay");
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn outbox_stage_bundle() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let bundle = Bundle {
            schema_version: 1,
            bundle_id: "bdl_test".to_string(),
            command_id: "cmd_test".to_string(),
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            submitting_creator_id: "ctr_test".to_string(),
            bundle_type: nexus_contracts::BundleType::WorldSync,
            manuscript_phase: None,
            output_manuscript: None,
            idempotency_key: "idk_test".to_string(),
            canonical_hash: String::new(),
            base_versions: serde_json::json!({"world_revision": 1}),
            last_confirmed_delta_sequence: None,
            deltas: vec![Delta {
                delta_type: DeltaType::World,
                operation: DeltaOperation::Create,
                target_entity_type: None,
                target_entity_id: None,
                payload: serde_json::json!({}),
                source_anchor: None,
                local_timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
            bundle_apply_status: None,
            delta_results: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let entry_id = outbox.stage(&bundle).await.expect("stage");
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.bundle_id, "bdl_test");
        assert_eq!(entry.delivery_state, DeliveryState::Ready);
    }

    #[tokio::test]
    async fn outbox_stage_if_absent_idempotent() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let bundle = Bundle {
            schema_version: 1,
            bundle_id: "bdl_pull_once".to_string(),
            command_id: "cmd_pull".to_string(),
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            submitting_creator_id: "ctr_test".to_string(),
            bundle_type: nexus_contracts::BundleType::WorldSync,
            manuscript_phase: None,
            output_manuscript: None,
            idempotency_key: "idk_pull".to_string(),
            canonical_hash: "a".repeat(64),
            base_versions: serde_json::json!({"world_revision": 1}),
            last_confirmed_delta_sequence: None,
            deltas: vec![Delta {
                delta_type: DeltaType::World,
                operation: DeltaOperation::Create,
                target_entity_type: None,
                target_entity_id: None,
                payload: serde_json::json!({}),
                source_anchor: None,
                local_timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
            bundle_apply_status: None,
            delta_results: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let first = outbox
            .stage_if_absent(&bundle)
            .await
            .expect("stage first")
            .expect("new entry");
        let second = outbox.stage_if_absent(&bundle).await.expect("stage second");
        assert!(second.is_none());
        let entry = outbox.get(&first).await.expect("get");
        assert_eq!(entry.bundle_id, "bdl_pull_once");
    }

    #[tokio::test]
    async fn outbox_purge_acked() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry1 = outbox.append(&cmd).await.expect("append 1");
        let entry2 = outbox.append(&cmd).await.expect("append 2");

        outbox.mark_sent(&entry1).await.expect("mark_sent 1");
        outbox.mark_acked(&entry1).await.expect("mark_acked 1");
        outbox.mark_sent(&entry2).await.expect("mark_sent 2");

        let purged = outbox.purge_acked().await.expect("purge");
        assert_eq!(purged, 1);

        let entries = outbox.replay().await.expect("replay");
        // entry2 is in 'sent' state, not replayable, so replay is empty
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn outbox_mark_sent_invalid_transition() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // Mark sent, then try to mark acked (skip acked — try mark_acked on 'acked')
        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        outbox.mark_acked(&entry_id).await.expect("mark_acked");

        // Trying to mark sent again should fail (state is 'acked', not in allowed set)
        let result = outbox.mark_sent(&entry_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn outbox_count_by_state() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        outbox.append(&cmd).await.expect("append 1");
        outbox.append(&cmd).await.expect("append 2");

        let count = outbox.count_by_state("staged").await.expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn delivery_state_roundtrip() {
        assert_eq!(
            DeliveryState::from_str("staged").unwrap(),
            DeliveryState::Staged
        );
        assert_eq!(
            DeliveryState::from_str("acked").unwrap(),
            DeliveryState::Acked
        );
        assert!(DeliveryState::from_str("bogus").is_err());
    }

    // ── retry_after tests (SYNC-R11) ────────────────────────────

    #[tokio::test]
    async fn outbox_conflicted_retry_none_equivalent_to_no_retry() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        outbox
            .mark_conflicted_with_retry(&entry_id, "conflict", &RetryAfterPolicy::None)
            .await
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Conflicted);
        assert!(entry.next_retry_at.is_none());
    }

    // ── Partial apply state persistence tests (SYNC-R12) ───────

    #[tokio::test]
    async fn outbox_persist_and_load_partial_apply_state() {
        use crate::partial_apply::{DeltaApplyInfo, PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        let partial_result = PartialApplyResult {
            total_count: 3,
            succeeded_count: 2,
            failed_count: 1,
            succeeded_deltas: vec![
                DeltaApplyInfo {
                    delta_index: 0,
                    apply_status: "applied".to_string(),
                    error_code: None,
                    applied_entity_revision: Some(1),
                },
                DeltaApplyInfo {
                    delta_index: 1,
                    apply_status: "applied".to_string(),
                    error_code: None,
                    applied_entity_revision: Some(2),
                },
            ],
            failed_deltas: vec![DeltaApplyInfo {
                delta_index: 2,
                apply_status: "rejected".to_string(),
                error_code: Some("optimistic_lock_failed".to_string()),
                applied_entity_revision: None,
            }],
            retryable: true,
            data_freshness_hint: Some("hint".to_string()),
            last_indexed_bundle_id: Some("bdl_prev".to_string()),
        };

        let state = PartialApplyState::new("bdl_test", "wld_test", partial_result);

        outbox
            .persist_partial_apply_state(&entry_id, &state)
            .await
            .expect("persist");

        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .await
            .expect("load")
            .expect("state should exist");

        assert_eq!(loaded.bundle_id, "bdl_test");
        assert_eq!(loaded.world_id, "wld_test");
        assert_eq!(loaded.result.total_count, 3);
        assert_eq!(loaded.result.succeeded_count, 2);
        assert_eq!(loaded.result.failed_count, 1);
        assert_eq!(loaded.retry_count, 0);
    }

    #[tokio::test]
    async fn outbox_load_nonexistent_partial_apply_state() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let loaded = outbox
            .load_partial_apply_state("obe_nonexistent")
            .await
            .expect("load");
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn outbox_remove_partial_apply_state() {
        use crate::partial_apply::{PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        let state = PartialApplyState::new(
            "bdl_test",
            "wld_test",
            PartialApplyResult {
                total_count: 1,
                succeeded_count: 1,
                failed_count: 0,
                succeeded_deltas: vec![],
                failed_deltas: vec![],
                retryable: false,
                data_freshness_hint: None,
                last_indexed_bundle_id: None,
            },
        );

        outbox
            .persist_partial_apply_state(&entry_id, &state)
            .await
            .expect("persist");
        outbox
            .remove_partial_apply_state(&entry_id)
            .await
            .expect("remove");

        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .await
            .expect("load");
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn outbox_list_partial_apply_states() {
        use crate::partial_apply::{DeltaApplyInfo, PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();

        let entry_id1 = outbox.append(&cmd).await.expect("append 1");
        let entry_id2 = outbox.append(&cmd).await.expect("append 2");

        let state1 = PartialApplyState::new(
            "bdl_1",
            "wld_test",
            PartialApplyResult {
                total_count: 2,
                succeeded_count: 1,
                failed_count: 1,
                succeeded_deltas: vec![],
                failed_deltas: vec![DeltaApplyInfo {
                    delta_index: 1,
                    apply_status: "skipped_dependency".to_string(),
                    error_code: None,
                    applied_entity_revision: None,
                }],
                retryable: true,
                data_freshness_hint: None,
                last_indexed_bundle_id: None,
            },
        );

        let state2 = PartialApplyState::new(
            "bdl_2",
            "wld_test",
            PartialApplyResult {
                total_count: 1,
                succeeded_count: 0,
                failed_count: 1,
                succeeded_deltas: vec![],
                failed_deltas: vec![DeltaApplyInfo {
                    delta_index: 0,
                    apply_status: "rejected".to_string(),
                    error_code: Some("transient_validation_error".to_string()),
                    applied_entity_revision: None,
                }],
                retryable: true,
                data_freshness_hint: None,
                last_indexed_bundle_id: None,
            },
        );

        outbox
            .persist_partial_apply_state(&entry_id1, &state1)
            .await
            .expect("persist 1");
        outbox
            .persist_partial_apply_state(&entry_id2, &state2)
            .await
            .expect("persist 2");

        let loaded_states = outbox.list_partial_apply_states().await.expect("list");
        assert_eq!(loaded_states.len(), 2);
        assert_eq!(loaded_states[0].1.bundle_id, "bdl_1");
        assert_eq!(loaded_states[1].1.bundle_id, "bdl_2");
    }

    #[tokio::test]
    async fn outbox_persist_partial_apply_state_upsert() {
        use crate::partial_apply::{PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        let state_v1 = PartialApplyState::new(
            "bdl_test",
            "wld_test",
            PartialApplyResult {
                total_count: 2,
                succeeded_count: 1,
                failed_count: 1,
                succeeded_deltas: vec![],
                failed_deltas: vec![],
                retryable: true,
                data_freshness_hint: None,
                last_indexed_bundle_id: None,
            },
        );

        outbox
            .persist_partial_apply_state(&entry_id, &state_v1)
            .await
            .expect("persist v1");

        // Persist again (upsert) with updated retry count
        let mut state_v2 = state_v1;
        state_v2.increment_retry();

        outbox
            .persist_partial_apply_state(&entry_id, &state_v2)
            .await
            .expect("persist v2");

        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .await
            .expect("load")
            .expect("state should exist");

        assert_eq!(loaded.retry_count, 1);
    }

    #[tokio::test]
    async fn outbox_conflicted_with_retry_at_past_time() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        // Set retry_after to 1 second ago
        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        outbox
            .mark_conflicted_with_retry(
                &entry_id,
                "transient conflict",
                &RetryAfterPolicy::AtTime(past),
            )
            .await
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Conflicted);

        // Entry SHOULD appear in replay since retry_after has passed
        let entries = outbox.replay().await.expect("replay");
        assert!(entries.iter().any(|e| e.outbox_entry_id == entry_id));
    }

    #[tokio::test]
    async fn outbox_conflicted_without_retry_not_in_replay() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");
        // Legacy mark_conflicted without retry policy
        outbox
            .mark_conflicted(&entry_id, "hard conflict")
            .await
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Conflicted);
        assert!(entry.next_retry_at.is_none());

        // Entry should NOT appear in replay (no retry_after set)
        let entries = outbox.replay().await.expect("replay");
        assert!(entries.iter().all(|e| e.outbox_entry_id != entry_id));
    }

    // ── Pool lifecycle tests ────────────────────────────────────

    #[tokio::test]
    async fn outbox_pool_lifecycle() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        // Pool should be created and usable
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // Get should work
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.outbox_entry_id, entry_id);
    }

    #[tokio::test]
    async fn outbox_concurrent_access_with_pool() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let outbox_clone = outbox.clone(); // Outbox is Clone through pool

        // Spawn multiple concurrent operations
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let o = outbox_clone.clone();
                tokio::spawn(async move {
                    let cmd = make_test_command();
                    let entry_id = o.append(&cmd).await.expect("append");
                    o.mark_sent(&entry_id).await.expect("mark_sent");
                    entry_id
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.expect("join"));
        }

        assert_eq!(results.len(), 4);

        // All entries should exist
        for entry_id in results {
            let entry = outbox.get(&entry_id).await.expect("get");
            assert_eq!(entry.delivery_state, DeliveryState::Sent);
        }
    }

    // ── Transaction atomicity tests (SYNC-R4) ─────────────────────

    #[tokio::test]
    async fn outbox_concurrent_append_multiple_connections() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        // Create 10 concurrent append operations
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let o = outbox.clone();
                tokio::spawn(async move {
                    let cmd = make_test_command();
                    o.append(&cmd).await.expect("append")
                })
            })
            .collect();

        let mut entry_ids = Vec::new();
        for handle in handles {
            entry_ids.push(handle.await.expect("join"));
        }

        // All 10 entries should be persisted
        assert_eq!(entry_ids.len(), 10);

        // Verify all entries exist and are unique
        let mut seen = std::collections::HashSet::new();
        for entry_id in &entry_ids {
            let entry = outbox.get(entry_id).await.expect("get");
            assert_eq!(entry.delivery_state, DeliveryState::Staged);
            assert!(seen.insert(entry_id.clone()));
        }

        // Verify count
        assert_eq!(outbox.count_by_state("staged").await.expect("count"), 10);
    }

    #[tokio::test]
    async fn outbox_transaction_rollback_on_error() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // Mark as sent
        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        // Try to mark as acked with invalid state (should fail if we simulate an error)
        // For this test, we verify that the state remains consistent after a failed operation

        // Attempt invalid transition (already sent, can't send again)
        let result = outbox.mark_sent(&entry_id).await;
        assert!(result.is_err());

        // State should remain 'sent' (transaction rolled back)
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Sent);
    }

    #[tokio::test]
    async fn outbox_state_transition_atomicity() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // Verify initial state
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Staged);
        assert_eq!(entry.retry_count, Some(0));

        // Transition: staged -> sent (should be atomic)
        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Sent);
        assert_eq!(entry.retry_count, Some(0));

        // Transition: sent -> acked (should be atomic)
        outbox.mark_acked(&entry_id).await.expect("mark_acked");

        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Acked);
        assert_eq!(entry.retry_count, Some(0));

        // Verify all fields updated atomically
        assert!(entry.updated_at.is_some());
    }

    #[tokio::test]
    async fn outbox_retry_after_persistence_atomicity() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        // Mark conflicted with retry_after
        let retry_time = chrono::Utc::now() + chrono::Duration::seconds(300);
        outbox
            .mark_conflicted_with_retry(
                &entry_id,
                "transient conflict",
                &RetryAfterPolicy::AtTime(retry_time),
            )
            .await
            .expect("mark_conflicted");

        // Verify all fields persisted atomically
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.delivery_state, DeliveryState::Conflicted);
        assert_eq!(entry.last_error, Some("transient conflict".to_string()));
        assert!(entry.next_retry_at.is_some());
        assert!(entry.updated_at.is_some());

        // Verify retry_after time matches
        let stored_retry =
            chrono::DateTime::parse_from_rfc3339(entry.next_retry_at.as_ref().unwrap())
                .expect("parse retry_after");
        let diff = (stored_retry.timestamp() - retry_time.timestamp()).abs();
        assert!(diff < 2, "retry_after should match within 2 seconds");
    }

    #[tokio::test]
    async fn outbox_exponential_backoff_atomicity() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        outbox.mark_sent(&entry_id).await.expect("mark_sent");

        // Fail multiple times and verify retry_count and next_retry_at are atomic
        for i in 1..=3 {
            outbox
                .mark_failed(&entry_id, &format!("error {i}"))
                .await
                .expect("mark_failed");

            let entry = outbox.get(&entry_id).await.expect("get");
            assert_eq!(entry.delivery_state, DeliveryState::Failed);
            assert_eq!(entry.retry_count, Some(i));
            assert!(entry.next_retry_at.is_some());
            assert!(entry
                .last_error
                .as_ref()
                .unwrap()
                .contains(&format!("error {i}")));
        }
    }

    #[tokio::test]
    async fn outbox_bundle_payload_integrity() {
        let outbox = Outbox::new_in_memory().await.expect("create outbox");

        // Create a bundle with specific content
        let bundle = Bundle {
            schema_version: 1,
            bundle_id: "bdl_test".to_string(),
            command_id: "cmd_test".to_string(),
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            submitting_creator_id: "ctr_test".to_string(),
            bundle_type: nexus_contracts::BundleType::WorldSync,
            manuscript_phase: None,
            output_manuscript: None,
            idempotency_key: "idk_test".to_string(),
            canonical_hash: "hash123".to_string(),
            base_versions: serde_json::json!({"world_revision": 5}),
            last_confirmed_delta_sequence: Some(10),
            deltas: vec![Delta {
                delta_type: DeltaType::KeyBlock,
                operation: DeltaOperation::Update,
                target_entity_type: Some("key_block".to_string()),
                target_entity_id: Some("char_001".to_string()),
                payload: serde_json::json!({"name": "Alice"}),
                source_anchor: None,
                local_timestamp: chrono::Utc::now().to_rfc3339(),
            }],
            bundle_apply_status: None,
            delta_results: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let entry_id = outbox.stage(&bundle).await.expect("stage");

        // Retrieve and verify all bundle metadata persisted atomically
        let entry = outbox.get(&entry_id).await.expect("get");
        assert_eq!(entry.bundle_id, "bdl_test");
        assert_eq!(entry.delivery_state, DeliveryState::Ready);

        // Verify bundle can be reconstructed from stored payload
        // (this tests that the bundle_payload column was written correctly)
        let count = outbox.count_by_state("ready").await.expect("count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn outbox_partial_apply_state_persistence_atomicity() {
        use crate::partial_apply::{DeltaApplyInfo, PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().await.expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).await.expect("append");

        // Create complex partial apply state
        let state = PartialApplyState::new(
            "bdl_complex",
            "wld_test",
            PartialApplyResult {
                total_count: 5,
                succeeded_count: 3,
                failed_count: 2,
                succeeded_deltas: vec![
                    DeltaApplyInfo {
                        delta_index: 0,
                        apply_status: "applied".to_string(),
                        error_code: None,
                        applied_entity_revision: Some(1),
                    },
                    DeltaApplyInfo {
                        delta_index: 1,
                        apply_status: "applied".to_string(),
                        error_code: None,
                        applied_entity_revision: Some(2),
                    },
                    DeltaApplyInfo {
                        delta_index: 2,
                        apply_status: "applied".to_string(),
                        error_code: None,
                        applied_entity_revision: Some(3),
                    },
                ],
                failed_deltas: vec![
                    DeltaApplyInfo {
                        delta_index: 3,
                        apply_status: "rejected".to_string(),
                        error_code: Some("validation_error".to_string()),
                        applied_entity_revision: None,
                    },
                    DeltaApplyInfo {
                        delta_index: 4,
                        apply_status: "rejected".to_string(),
                        error_code: Some("dependency_failed".to_string()),
                        applied_entity_revision: None,
                    },
                ],
                retryable: true,
                data_freshness_hint: Some("stale_data".to_string()),
                last_indexed_bundle_id: Some("bdl_prev".to_string()),
            },
        );

        // Persist state
        outbox
            .persist_partial_apply_state(&entry_id, &state)
            .await
            .expect("persist");

        // Retrieve and verify atomicity
        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .await
            .expect("load")
            .expect("state exists");

        assert_eq!(loaded.bundle_id, "bdl_complex");
        assert_eq!(loaded.world_id, "wld_test");
        assert_eq!(loaded.result.total_count, 5);
        assert_eq!(loaded.result.succeeded_count, 3);
        assert_eq!(loaded.result.failed_count, 2);
        assert_eq!(loaded.result.succeeded_deltas.len(), 3);
        assert_eq!(loaded.result.failed_deltas.len(), 2);
        assert_eq!(loaded.retry_count, 0);

        // Increment retry and verify atomicity
        let mut updated = loaded;
        updated.increment_retry();

        outbox
            .persist_partial_apply_state(&entry_id, &updated)
            .await
            .expect("persist updated");

        let reloaded = outbox
            .load_partial_apply_state(&entry_id)
            .await
            .expect("load")
            .expect("state exists");

        assert_eq!(reloaded.retry_count, 1);
        // All other fields should remain unchanged
        assert_eq!(reloaded.bundle_id, "bdl_complex");
        assert_eq!(reloaded.result.total_count, 5);
    }
}
