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

/// Maximum retry count before giving up.
const MAX_RETRIES: u64 = 5;

/// Base delay for exponential backoff in seconds.
const BASE_RETRY_DELAY_SECS: u64 = 2;

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
                ON outbox_entries(bundle_id);",
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
                WHERE delivery_state IN ('staged', 'failed');",
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
    pub fn mark_conflicted(&self, outbox_entry_id: &str, error: &str) -> SyncResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = self.conn.execute(
            "UPDATE outbox_entries
             SET delivery_state = 'conflicted', last_error = ?1, updated_at = ?2
             WHERE outbox_entry_id = ?3 AND delivery_state = 'sent'",
            params![error, now, outbox_entry_id],
        )?;

        if rows == 0 {
            return Err(SyncError::OutboxEntryNotFound {
                id: outbox_entry_id.to_string(),
            });
        }

        tracing::warn!(
            outbox_entry_id = %outbox_entry_id,
            error = %error,
            "Marked as conflicted"
        );
        Ok(())
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
    pub fn replay(&self) -> SyncResult<Vec<OutboxEntry>> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT outbox_entry_id, bundle_id, idempotency_key, delivery_state,
                    retry_count, last_error, next_retry_at, created_at, updated_at
             FROM outbox_entries
             WHERE delivery_state IN ('staged', 'ready')
                OR (delivery_state = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?1)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::SyncCommand;

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
            deltas: vec![serde_json::json!({"delta_type": "world", "operation": "create"})],
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
}
