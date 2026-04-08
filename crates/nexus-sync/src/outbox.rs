//! Outbox Pattern Implementation
//!
//! Local operation queue using SQLite for persistence.
//! Implements the full `OutboxEntry` contract type with delivery state management.
//!
//! The outbox stores pending bundles for offline-first sync, supporting:
//! - Staging commands into outbox entries
//! - Tracking delivery state (staged → ready → sent → acked/conflicted/failed)
//! - Retry with exponential backoff
//! - Replay of pending entries
//!
//! ## Note: `outbox_entries` vs daemon `outbox`
//!
//! The `outbox_entries` table defined here is **intentionally different** from the
//! daemon's `outbox` table (in `nexus42d/src/db/schema.rs`). The daemon's `outbox`
//! is a simple command queue, while `outbox_entries` is a full bundle-level sync
//! outbox with idempotency keys, retry tracking, and delivery state management.
//! They serve different purposes and should NOT be merged.

use std::path::Path;

use nexus_contracts::generated::{Bundle, OutboxEntry, SyncCommand, LATEST_SCHEMA_VERSION};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::errors::{SyncError, SyncResult};

/// Maximum retry count before giving up.
const MAX_RETRIES: u64 = 5;

/// Base delay for exponential backoff in seconds.
const BASE_RETRY_DELAY_SECS: u64 = 2;

/// Delivery states for outbox entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryState {
    Staged,
    Ready,
    Sent,
    Acked,
    Conflicted,
    Failed,
}

impl DeliveryState {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Staged => "staged",
            Self::Ready => "ready",
            Self::Sent => "sent",
            Self::Acked => "acked",
            Self::Conflicted => "conflicted",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> SyncResult<Self> {
        match s {
            "staged" => Ok(Self::Staged),
            "ready" => Ok(Self::Ready),
            "sent" => Ok(Self::Sent),
            "acked" => Ok(Self::Acked),
            "conflicted" => Ok(Self::Conflicted),
            "failed" => Ok(Self::Failed),
            other => Err(SyncError::OutboxInvalidState {
                expected: "known state".to_string(),
                actual: other.to_string(),
            }),
        }
    }
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

/// SQLite-backed outbox for local sync operations.
pub struct Outbox {
    conn: Connection,
}

impl Outbox {
    /// Open or create an outbox database at the given path.
    ///
    /// Creates the outbox_entries table if it doesn't exist.
    pub fn new<P: AsRef<Path>>(db_path: P) -> SyncResult<Self> {
        let conn = Connection::open(db_path)?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS outbox_entries (
                outbox_entry_id   TEXT PRIMARY KEY,
                bundle_id         TEXT NOT NULL,
                idempotency_key   TEXT NOT NULL,
                delivery_state    TEXT NOT NULL DEFAULT 'staged',
                retry_count       INTEGER NOT NULL DEFAULT 0,
                last_error        TEXT,
                next_retry_at     TEXT,
                command_payload   TEXT NOT NULL DEFAULT '{}',
                bundle_payload    TEXT,
                created_at        TEXT NOT NULL,
                updated_at        TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_outbox_delivery_state
                ON outbox_entries(delivery_state);

            CREATE INDEX IF NOT EXISTS idx_outbox_next_retry
                ON outbox_entries(next_retry_at)
                WHERE delivery_state IN ('staged', 'failed');

            CREATE INDEX IF NOT EXISTS idx_outbox_bundle_id
                ON outbox_entries(bundle_id);

            CREATE TABLE IF NOT EXISTS partial_apply_states (
                outbox_entry_id   TEXT PRIMARY KEY,
                state_json        TEXT NOT NULL,
                recorded_at       TEXT NOT NULL,
                retry_count       INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        tracing::info!("Outbox database initialized");

        Ok(Self { conn })
    }

    /// Open an in-memory outbox (for testing).
    #[cfg(test)]
    pub fn new_in_memory() -> SyncResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS outbox_entries (
                outbox_entry_id   TEXT PRIMARY KEY,
                bundle_id         TEXT NOT NULL,
                idempotency_key   TEXT NOT NULL,
                delivery_state    TEXT NOT NULL DEFAULT 'staged',
                retry_count       INTEGER NOT NULL DEFAULT 0,
                last_error        TEXT,
                next_retry_at     TEXT,
                command_payload   TEXT NOT NULL DEFAULT '{}',
                bundle_payload    TEXT,
                created_at        TEXT NOT NULL,
                updated_at        TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_outbox_delivery_state
                ON outbox_entries(delivery_state);

            CREATE INDEX IF NOT EXISTS idx_outbox_next_retry
                ON outbox_entries(next_retry_at)
                WHERE delivery_state IN ('staged', 'failed');

            CREATE TABLE IF NOT EXISTS partial_apply_states (
                outbox_entry_id   TEXT PRIMARY KEY,
                state_json        TEXT NOT NULL,
                recorded_at       TEXT NOT NULL,
                retry_count       INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(Self { conn })
    }

    /// Append a sync command to the outbox in `staged` state.
    ///
    /// Returns the generated outbox entry ID.
    pub fn append(&self, command: &SyncCommand) -> SyncResult<String> {
        let outbox_entry_id = format!("obe_{}", Uuid::new_v4().simple());
        let bundle_id = format!("bdl_{}", Uuid::new_v4().simple());
        let idempotency_key = format!("idk_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let command_payload = serde_json::to_string(command)?;

        let txn = self.conn.unchecked_transaction()?;
        txn.execute(
            "INSERT INTO outbox_entries
                (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                 retry_count, command_payload, created_at)
             VALUES (?1, ?2, ?3, 'staged', 0, ?4, ?5)",
            params![
                outbox_entry_id,
                bundle_id,
                idempotency_key,
                command_payload,
                now,
            ],
        )?;
        txn.commit()?;

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
    pub fn stage(&self, bundle: &Bundle) -> SyncResult<String> {
        let outbox_entry_id = format!("obe_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let bundle_payload = serde_json::to_string(bundle)?;

        let txn = self.conn.unchecked_transaction()?;
        txn.execute(
            "INSERT INTO outbox_entries
                (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                 retry_count, bundle_payload, created_at)
             VALUES (?1, ?2, ?3, 'ready', 0, ?4, ?5)",
            params![
                outbox_entry_id,
                bundle.bundle_id,
                bundle.idempotency_key,
                bundle_payload,
                now,
            ],
        )?;
        txn.commit()?;

        tracing::debug!(
            outbox_entry_id = %outbox_entry_id,
            bundle_id = %bundle.bundle_id,
            "Bundle staged to outbox"
        );

        Ok(outbox_entry_id)
    }

    /// Transition an outbox entry to `sent` state.
    pub fn mark_sent(&self, outbox_entry_id: &str) -> SyncResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = self.conn.execute(
            "UPDATE outbox_entries
             SET delivery_state = 'sent', updated_at = ?1
             WHERE outbox_entry_id = ?2 AND delivery_state IN ('staged', 'ready')",
            params![now, outbox_entry_id],
        )?;

        if rows == 0 {
            return Err(SyncError::OutboxEntryNotFound {
                id: outbox_entry_id.to_string(),
            });
        }

        tracing::debug!(outbox_entry_id = %outbox_entry_id, "Marked as sent");
        Ok(())
    }

    /// Transition an outbox entry to `acked` state.
    pub fn mark_acked(&self, outbox_entry_id: &str) -> SyncResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = self.conn.execute(
            "UPDATE outbox_entries
             SET delivery_state = 'acked', updated_at = ?1
             WHERE outbox_entry_id = ?2 AND delivery_state = 'sent'",
            params![now, outbox_entry_id],
        )?;

        if rows == 0 {
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
    pub fn mark_conflicted_with_retry(
        &self,
        outbox_entry_id: &str,
        error: &str,
        retry_after: &RetryAfterPolicy,
    ) -> SyncResult<()> {
        let now = chrono::Utc::now();
        let next_retry_at = match retry_after {
            RetryAfterPolicy::AtTime(t) => Some(t.to_rfc3339()),
            RetryAfterPolicy::AfterSeconds(secs) => {
                let target = now + chrono::Duration::seconds(*secs as i64);
                Some(target.to_rfc3339())
            }
            RetryAfterPolicy::None => None,
        };

        let rows = self.conn.execute(
            "UPDATE outbox_entries
             SET delivery_state = 'conflicted',
                 last_error = ?1,
                 next_retry_at = ?2,
                 updated_at = ?3
             WHERE outbox_entry_id = ?4 AND delivery_state = 'sent'",
            params![error, next_retry_at, now.to_rfc3339(), outbox_entry_id],
        )?;

        if rows == 0 {
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
    pub fn mark_conflicted(&self, outbox_entry_id: &str, error: &str) -> SyncResult<()> {
        self.mark_conflicted_with_retry(outbox_entry_id, error, &RetryAfterPolicy::None)
    }

    /// Transition an outbox entry to `failed` state with retry scheduling.
    ///
    /// Calculates the next retry time using exponential backoff.
    /// Returns an error if the max retry count has been exceeded.
    pub fn mark_failed(&self, outbox_entry_id: &str, error: &str) -> SyncResult<()> {
        let txn = self.conn.unchecked_transaction()?;

        let retry_count: u64 = txn
            .query_row(
                "SELECT retry_count FROM outbox_entries WHERE outbox_entry_id = ?1",
                params![outbox_entry_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v as u64)?;

        if retry_count >= MAX_RETRIES {
            // Permanently mark as failed without retry
            let now = chrono::Utc::now().to_rfc3339();
            txn.execute(
                "UPDATE outbox_entries
                 SET delivery_state = 'failed', last_error = ?1, updated_at = ?2,
                     next_retry_at = NULL
                 WHERE outbox_entry_id = ?3",
                params![error, now, outbox_entry_id],
            )?;
            txn.commit()?;
            return Err(SyncError::OutboxMaxRetriesExceeded {
                id: outbox_entry_id.to_string(),
                retries: retry_count,
            });
        }

        // Calculate exponential backoff
        let delay_secs =
            BASE_RETRY_DELAY_SECS.saturating_mul(2u64.saturating_pow(retry_count.min(30) as u32));
        let next_retry = chrono::Utc::now() + chrono::Duration::seconds(delay_secs as i64);
        let now = chrono::Utc::now().to_rfc3339();

        txn.execute(
            "UPDATE outbox_entries
             SET delivery_state = 'failed',
                 retry_count = retry_count + 1,
                 last_error = ?1,
                 next_retry_at = ?2,
                 updated_at = ?3
             WHERE outbox_entry_id = ?4",
            params![error, next_retry.to_rfc3339(), now, outbox_entry_id],
        )?;

        txn.commit()?;

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
    pub fn replay(&self) -> SyncResult<Vec<OutboxEntry>> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                    retry_count, last_error, next_retry_at, created_at, updated_at
             FROM outbox_entries
             WHERE delivery_state IN ('staged', 'ready')
                OR (delivery_state = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?1)
                OR (delivery_state = 'conflicted' AND next_retry_at IS NOT NULL AND next_retry_at <= ?1)
             ORDER BY created_at ASC",
        )?;

        let entries = stmt
            .query_map(params![now], |row| {
                Ok(OutboxEntry {
                    schema_version: LATEST_SCHEMA_VERSION,
                    outbox_entry_id: row.get(0)?,
                    bundle_id: row.get(1)?,
                    idempotency_key: row.get(2)?,
                    delivery_state: row.get(3)?,
                    retry_count: Some(row.get(4)?),
                    last_error: row.get(5)?,
                    next_retry_at: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        tracing::debug!(count = entries.len(), "Replayed pending outbox entries");
        Ok(entries)
    }

    /// Get a specific outbox entry by ID.
    pub fn get(&self, outbox_entry_id: &str) -> SyncResult<OutboxEntry> {
        let entry = self.conn.query_row(
            "SELECT outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                    retry_count, last_error, next_retry_at, created_at, updated_at
             FROM outbox_entries
             WHERE outbox_entry_id = ?1",
            params![outbox_entry_id],
            |row| {
                Ok(OutboxEntry {
                    schema_version: LATEST_SCHEMA_VERSION,
                    outbox_entry_id: row.get(0)?,
                    bundle_id: row.get(1)?,
                    idempotency_key: row.get(2)?,
                    delivery_state: row.get(3)?,
                    retry_count: Some(row.get(4)?),
                    last_error: row.get(5)?,
                    next_retry_at: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )?;

        Ok(entry)
    }

    /// Remove acknowledged entries (cleanup).
    ///
    /// Returns the number of entries removed.
    pub fn purge_acked(&self) -> SyncResult<usize> {
        let rows = self.conn.execute(
            "DELETE FROM outbox_entries WHERE delivery_state = 'acked'",
            [],
        )?;
        tracing::info!(count = rows, "Purged acked outbox entries");
        Ok(rows)
    }

    /// Count entries by delivery state.
    pub fn count_by_state(&self, state: &str) -> SyncResult<usize> {
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM outbox_entries WHERE delivery_state = ?1",
            params![state],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    // ── Partial apply state persistence (SYNC-R12) ──────────────

    /// Persist partial apply state for an outbox entry (SYNC-R12).
    ///
    /// Stores the partial apply result so that on daemon restart, the
    /// partial apply can be resumed without reconstructing state from scratch.
    /// The state is stored in the `partial_apply_states` table.
    pub fn persist_partial_apply_state(
        &self,
        outbox_entry_id: &str,
        state: &crate::partial_apply::PartialApplyState,
    ) -> SyncResult<()> {
        let state_json = serde_json::to_string(state)?;
        let now = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT OR REPLACE INTO partial_apply_states
                (outbox_entry_id, state_json, recorded_at, retry_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![outbox_entry_id, state_json, now, state.retry_count],
        )?;

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
    pub fn load_partial_apply_state(
        &self,
        outbox_entry_id: &str,
    ) -> SyncResult<Option<crate::partial_apply::PartialApplyState>> {
        let result = self.conn.query_row(
            "SELECT state_json FROM partial_apply_states WHERE outbox_entry_id = ?1",
            params![outbox_entry_id],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(json) => {
                let state: crate::partial_apply::PartialApplyState = serde_json::from_str(&json)?;
                tracing::debug!(
                    outbox_entry_id = %outbox_entry_id,
                    "Loaded persisted partial apply state"
                );
                Ok(Some(state))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SyncError::from(e)),
        }
    }

    /// Remove persisted partial apply state (SYNC-R12).
    ///
    /// Called after a partial apply has been fully resolved (all deltas succeeded
    /// or permanently failed).
    pub fn remove_partial_apply_state(&self, outbox_entry_id: &str) -> SyncResult<()> {
        self.conn.execute(
            "DELETE FROM partial_apply_states WHERE outbox_entry_id = ?1",
            params![outbox_entry_id],
        )?;
        tracing::debug!(
            outbox_entry_id = %outbox_entry_id,
            "Removed persisted partial apply state"
        );
        Ok(())
    }

    /// List all outbox entries with persisted partial apply states (SYNC-R12).
    ///
    /// Useful for resuming partial applies after daemon restart.
    pub fn list_partial_apply_states(
        &self,
    ) -> SyncResult<Vec<(String, crate::partial_apply::PartialApplyState)>> {
        let mut stmt = self.conn.prepare(
            "SELECT outbox_entry_id, state_json FROM partial_apply_states ORDER BY recorded_at ASC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut results = Vec::new();
        for (entry_id, json) in rows {
            let state: crate::partial_apply::PartialApplyState = serde_json::from_str(&json)?;
            results.push((entry_id, state));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::{Delta, SyncCommand};

    fn make_test_command() -> SyncCommand {
        SyncCommand {
            schema_version: 1,
            command_id: "cmd_test".to_string(),
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            command_type: "sync_push".to_string(),
            origin: "local_user".to_string(),
            output_manuscript: None,
            status: "pending".to_string(),
            requested_by: None,
            started_at: None,
            completed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn outbox_append_and_get() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.outbox_entry_id, entry_id);
        assert_eq!(entry.delivery_state, "staged");
        assert!(entry.bundle_id.starts_with("bdl_"));
    }

    #[test]
    fn outbox_lifecycle_staged_to_acked() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");
        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "sent");

        outbox.mark_acked(&entry_id).expect("mark_acked");
        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "acked");
    }

    #[test]
    fn outbox_lifecycle_conflicted() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");
        outbox
            .mark_conflicted(&entry_id, "version mismatch")
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "conflicted");
        assert_eq!(entry.last_error, Some("version mismatch".to_string()));
    }

    #[test]
    fn outbox_failed_with_retry() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        // First failure
        outbox.mark_sent(&entry_id).expect("mark_sent");
        outbox
            .mark_failed(&entry_id, "transient error")
            .expect("mark_failed 1");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "failed");
        assert_eq!(entry.retry_count, Some(1));
        assert!(entry.next_retry_at.is_some());
    }

    #[test]
    fn outbox_max_retries_exceeded() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");

        // Fail MAX_RETRIES times
        for _ in 0..MAX_RETRIES {
            outbox.mark_failed(&entry_id, "persistent error").ok();
        }

        // Next failure should error
        let result = outbox.mark_failed(&entry_id, "persistent error");
        assert!(matches!(
            result,
            Err(SyncError::OutboxMaxRetriesExceeded { .. })
        ));
    }

    #[test]
    fn outbox_replay_returns_pending() {
        let outbox = Outbox::new_in_memory().expect("create outbox");

        let cmd1 = make_test_command();
        let cmd2 = make_test_command();
        let _entry1 = outbox.append(&cmd1).expect("append 1");
        let _entry2 = outbox.append(&cmd2).expect("append 2");

        let entries = outbox.replay().expect("replay");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn outbox_replay_excludes_acked_and_sent() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry1 = outbox.append(&cmd).expect("append 1");
        let _entry2 = outbox.append(&cmd).expect("append 2");

        // Mark one as sent (not in replay)
        outbox.mark_sent(&entry1).expect("mark_sent");

        let entries = outbox.replay().expect("replay");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn outbox_stage_bundle() {
        let outbox = Outbox::new_in_memory().expect("create outbox");

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
                delta_type: "world".to_string(),
                operation: "create".to_string(),
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

        let entry_id = outbox.stage(&bundle).expect("stage");
        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.bundle_id, "bdl_test");
        assert_eq!(entry.delivery_state, "ready");
    }

    #[test]
    fn outbox_purge_acked() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry1 = outbox.append(&cmd).expect("append 1");
        let entry2 = outbox.append(&cmd).expect("append 2");

        outbox.mark_sent(&entry1).expect("mark_sent 1");
        outbox.mark_acked(&entry1).expect("mark_acked 1");
        outbox.mark_sent(&entry2).expect("mark_sent 2");

        let purged = outbox.purge_acked().expect("purge");
        assert_eq!(purged, 1);

        let entries = outbox.replay().expect("replay");
        // entry2 is in 'sent' state, not replayable, so replay is empty
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn outbox_mark_sent_invalid_transition() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        // Mark sent, then try to mark acked (skip acked — try mark_acked on 'acked')
        outbox.mark_sent(&entry_id).expect("mark_sent");
        outbox.mark_acked(&entry_id).expect("mark_acked");

        // Trying to mark sent again should fail (state is 'acked', not in allowed set)
        let result = outbox.mark_sent(&entry_id);
        assert!(result.is_err());
    }

    #[test]
    fn outbox_count_by_state() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        outbox.append(&cmd).expect("append 1");
        outbox.append(&cmd).expect("append 2");

        let count = outbox.count_by_state("staged").expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn delivery_state_roundtrip() {
        assert_eq!(
            DeliveryState::parse("staged").unwrap(),
            DeliveryState::Staged
        );
        assert_eq!(DeliveryState::parse("acked").unwrap(), DeliveryState::Acked);
        assert!(DeliveryState::parse("bogus").is_err());
    }

    // ── retry_after tests (SYNC-R11) ────────────────────────────

    #[test]
    fn outbox_conflicted_retry_none_equivalent_to_no_retry() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");
        outbox
            .mark_conflicted_with_retry(&entry_id, "conflict", &RetryAfterPolicy::None)
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "conflicted");
        assert!(entry.next_retry_at.is_none());
    }

    // ── Partial apply state persistence tests (SYNC-R12) ───────

    #[test]
    fn outbox_persist_and_load_partial_apply_state() {
        use crate::partial_apply::{DeltaApplyInfo, PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

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
            .expect("persist");

        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .expect("load")
            .expect("state should exist");

        assert_eq!(loaded.bundle_id, "bdl_test");
        assert_eq!(loaded.world_id, "wld_test");
        assert_eq!(loaded.result.total_count, 3);
        assert_eq!(loaded.result.succeeded_count, 2);
        assert_eq!(loaded.result.failed_count, 1);
        assert_eq!(loaded.retry_count, 0);
    }

    #[test]
    fn outbox_load_nonexistent_partial_apply_state() {
        let outbox = Outbox::new_in_memory().expect("create outbox");

        let loaded = outbox
            .load_partial_apply_state("obe_nonexistent")
            .expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn outbox_remove_partial_apply_state() {
        use crate::partial_apply::{PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

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
            .expect("persist");
        outbox
            .remove_partial_apply_state(&entry_id)
            .expect("remove");

        let loaded = outbox.load_partial_apply_state(&entry_id).expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn outbox_list_partial_apply_states() {
        use crate::partial_apply::{DeltaApplyInfo, PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();

        let entry_id1 = outbox.append(&cmd).expect("append 1");
        let entry_id2 = outbox.append(&cmd).expect("append 2");

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
            .expect("persist 1");
        outbox
            .persist_partial_apply_state(&entry_id2, &state2)
            .expect("persist 2");

        let states = outbox.list_partial_apply_states().expect("list");
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].1.bundle_id, "bdl_1");
        assert_eq!(states[1].1.bundle_id, "bdl_2");
    }

    #[test]
    fn outbox_persist_partial_apply_state_upsert() {
        use crate::partial_apply::{PartialApplyResult, PartialApplyState};

        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

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
            .expect("persist v1");

        // Persist again (upsert) with updated retry count
        let mut state_v2 = state_v1;
        state_v2.increment_retry();

        outbox
            .persist_partial_apply_state(&entry_id, &state_v2)
            .expect("persist v2");

        let loaded = outbox
            .load_partial_apply_state(&entry_id)
            .expect("load")
            .expect("state should exist");

        assert_eq!(loaded.retry_count, 1);
    }

    #[test]
    fn outbox_conflicted_with_retry_at_past_time() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");

        // Set retry_after to 1 second ago
        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        outbox
            .mark_conflicted_with_retry(
                &entry_id,
                "transient conflict",
                &RetryAfterPolicy::AtTime(past),
            )
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "conflicted");

        // Entry SHOULD appear in replay since retry_after has passed
        let entries = outbox.replay().expect("replay");
        assert!(entries.iter().any(|e| e.outbox_entry_id == entry_id));
    }

    #[test]
    fn outbox_conflicted_without_retry_not_in_replay() {
        let outbox = Outbox::new_in_memory().expect("create outbox");
        let cmd = make_test_command();
        let entry_id = outbox.append(&cmd).expect("append");

        outbox.mark_sent(&entry_id).expect("mark_sent");
        // Legacy mark_conflicted without retry policy
        outbox
            .mark_conflicted(&entry_id, "hard conflict")
            .expect("mark_conflicted");

        let entry = outbox.get(&entry_id).expect("get");
        assert_eq!(entry.delivery_state, "conflicted");
        assert!(entry.next_retry_at.is_none());

        // Entry should NOT appear in replay (no retry_after set)
        let entries = outbox.replay().expect("replay");
        assert!(entries.iter().all(|e| e.outbox_entry_id != entry_id));
    }
}
