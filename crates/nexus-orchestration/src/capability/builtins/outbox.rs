//! `outbox.flush` and `outbox.compact` capability implementations.
//!
//! V1.59 P1 T4: Wired from stubs to real pool-backed implementations.
//! Both capabilities operate locally on `outbox_entries` via the injected
//! `sqlx::SqlitePool` — they do NOT depend on `nexus-cloud-sync`.
//!
//! Semantics and test vectors are documented in the Draft overlay spec:
//! `.mstar/knowledge/specs/outbox-consolidation.md` §4–5.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::Value;

// ---------------------------------------------------------------------------
// outbox.flush
// ---------------------------------------------------------------------------

/// Flush pending outbox entries by marking them as acknowledged.
///
/// **Pool-backed (production)**: transitions `staged`/`ready` entries
/// to `acked` state. Platform is paused — this is a local-only DB operation.
///
/// **Standalone (no pool)**: returns `CapabilityError::Internal`.
pub struct OutboxFlush {
    pool: Option<sqlx::SqlitePool>,
}

impl OutboxFlush {
    /// Create a standalone (pool-less) instance for testing/placeholder mode.
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Create a pool-backed instance for production use.
    #[must_use]
    pub const fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool: Some(pool) }
    }
}

impl Default for OutboxFlush {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for OutboxFlush {
    fn name(&self) -> &'static str {
        "outbox.flush"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"limit":{"type":"integer","minimum":0,"default":0}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"flushed":{"type":"integer","minimum":0}},"required":["flushed"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            CapabilityError::Internal(
                "outbox.flush: no database pool — use with_pool()".to_string(),
            )
        })?;

        let limit: i64 = input
            .get("limit")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);

        let now = chrono::Utc::now().to_rfc3339();

        let flushed = if limit > 0 {
            // Limit the number of entries flushed in this batch.
            // SQLite does not support LIMIT in UPDATE with ORDER BY directly,
            // so we use a subquery to select the IDs to update.
            let rows = sqlx::query_scalar!(
                "SELECT outbox_entry_id as \"outbox_entry_id!\" FROM outbox_entries
                 WHERE delivery_state IN ('staged', 'ready')
                 ORDER BY created_at ASC
                 LIMIT ?",
                limit
            )
            .fetch_all(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("outbox.flush select failed: {e}")))?;

            if rows.is_empty() {
                0u64
            } else {
                // Build parameter list: collect owned values first to avoid lifetime issues.
                let placeholders: Vec<String> = rows.iter().map(|_| "?".to_string()).collect();
                // SAFETY: dynamic SQL for batch UPDATE with variable number of placeholders;
                // values are string literals from the database (outbox_entry_id), not user input.
                let sql = format!(
                    "UPDATE outbox_entries SET delivery_state = 'acked', updated_at = ?
                     WHERE outbox_entry_id IN ({})",
                    placeholders.join(",")
                );

                // Bind timestamp + each entry ID as owned values.
                let mut query = sqlx::query(&sql).bind(now);
                for id in &rows {
                    query = query.bind(id.clone());
                }
                let result = query.execute(pool).await.map_err(|e| {
                    CapabilityError::Internal(format!("outbox.flush update failed: {e}"))
                })?;

                result.rows_affected()
            }
        } else {
            // No limit: flush ALL pending entries.
            let result = sqlx::query!(
                "UPDATE outbox_entries
                 SET delivery_state = 'acked', updated_at = ?
                 WHERE delivery_state IN ('staged', 'ready')",
                now
            )
            .execute(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("outbox.flush update failed: {e}")))?;

            result.rows_affected()
        };

        tracing::info!(flushed = flushed, limit = limit, "outbox.flush completed");

        Ok(serde_json::json!({"flushed": flushed}))
    }
}

// ---------------------------------------------------------------------------
// outbox.compact
// ---------------------------------------------------------------------------

/// Compact outbox table by removing old acknowledged entries.
///
/// **Pool-backed (production)**: deletes `acked` entries older than
/// a configurable retention window (default 7 days). Returns counts
/// of removed and retained entries.
///
/// **Standalone (no pool)**: returns `CapabilityError::Internal`.
pub struct OutboxCompact {
    pool: Option<sqlx::SqlitePool>,
}

impl OutboxCompact {
    /// Create a standalone (pool-less) instance for testing/placeholder mode.
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Create a pool-backed instance for production use.
    #[must_use]
    pub const fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool: Some(pool) }
    }
}

impl Default for OutboxCompact {
    fn default() -> Self {
        Self::new()
    }
}

/// Default retention window in days.
const DEFAULT_RETENTION_DAYS: i64 = 7;

#[async_trait]
impl Capability for OutboxCompact {
    fn name(&self) -> &'static str {
        "outbox.compact"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"retentionDays":{"type":"integer","minimum":1,"default":7}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"removed":{"type":"integer","minimum":0},"retained":{"type":"integer","minimum":0}},"required":["removed","retained"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            CapabilityError::Internal(
                "outbox.compact: no database pool — use with_pool()".to_string(),
            )
        })?;

        let retention_days: i64 = input
            .get("retentionDays")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(DEFAULT_RETENTION_DAYS)
            .max(0); // Clamp to non-negative; 0 means "remove all acked"

        // Compute cutoff timestamp.
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
        let cutoff_str = cutoff.to_rfc3339();

        // Delete old acked entries.
        let removed = sqlx::query!(
            "DELETE FROM outbox_entries
             WHERE delivery_state = 'acked'
               AND (updated_at IS NULL OR updated_at < ?)",
            cutoff_str
        )
        .execute(pool)
        .await
        .map_err(|e| CapabilityError::Internal(format!("outbox.compact delete failed: {e}")))?
        .rows_affected();

        // Count remaining acked entries.
        let retained: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) as \"count!\" FROM outbox_entries WHERE delivery_state = 'acked'"
        )
        .fetch_one(pool)
        .await
        .map_err(|e| CapabilityError::Internal(format!("outbox.compact count failed: {e}")))?;

        // SAFETY: COUNT(*) returns a non-negative integer; usize is at least u32 on all targets.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let retained = usize::try_from(retained).unwrap_or(usize::MAX);

        tracing::info!(
            removed = removed,
            retained = retained,
            retention_days = retention_days,
            "outbox.compact completed"
        );

        Ok(serde_json::json!({"removed": removed, "retained": retained}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an in-memory pool with the `outbox_entries` table.
    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("create in-memory pool");
        // SAFETY: DDL statement — not supported by compile-time checked macros.
        sqlx::query(
            "CREATE TABLE outbox_entries (
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
            )",
        )
        .execute(&pool)
        .await
        .expect("create outbox_entries table");
        pool
    }

    /// Helper: insert a test entry with given state and optional updated_at.
    async fn insert_entry(
        pool: &sqlx::SqlitePool,
        entry_id: &str,
        state: &str,
        updated_at: Option<&str>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let updated_val = updated_at.map_or("NULL".to_string(), |u| format!("'{u}'"));
        // SAFETY: test-only DDL — dynamic SQL for flexible test helper.
        sqlx::query(&format!(
            "INSERT INTO outbox_entries
             (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
              retry_count, created_at, updated_at)
             VALUES ('{entry_id}', 'bdl_{entry_id}', 'idk_{entry_id}', '{state}', 0, '{now}', {updated_val})"
        ))
        .execute(pool)
        .await
        .expect("insert test entry");
    }

    // ── outbox.flush tests ────────────────────────────────────────────

    #[tokio::test]
    async fn outbox_flush_no_pool_returns_internal_error() {
        let cap = OutboxFlush::new();
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        match err {
            CapabilityError::Internal(msg) => {
                assert!(msg.contains("no database pool"));
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn outbox_flush_no_entries() {
        let pool = test_pool().await;
        let cap = OutboxFlush::with_pool(pool);
        let out = cap.run(serde_json::json!({"limit": 100})).await.unwrap();
        assert_eq!(out["flushed"], 0);
    }

    #[tokio::test]
    async fn outbox_flush_all_pending() {
        let pool = test_pool().await;
        insert_entry(&pool, "obe_1", "staged", None).await;
        insert_entry(&pool, "obe_2", "ready", None).await;
        insert_entry(&pool, "obe_3", "acked", None).await; // should NOT be flushed

        let cap = OutboxFlush::with_pool(pool.clone());
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert_eq!(out["flushed"], 2); // only staged + ready

        // Verify states
        let states: Vec<(String,)> =
            sqlx::query_as("SELECT delivery_state FROM outbox_entries ORDER BY outbox_entry_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let states: Vec<&str> = states.iter().map(|(s,)| s.as_str()).collect();
        assert_eq!(states, vec!["acked", "acked", "acked"]);
    }

    #[tokio::test]
    async fn outbox_flush_with_limit() {
        let pool = test_pool().await;
        insert_entry(&pool, "obe_a", "staged", None).await;
        insert_entry(&pool, "obe_b", "staged", None).await;
        insert_entry(&pool, "obe_c", "staged", None).await;

        let cap = OutboxFlush::with_pool(pool.clone());
        let out = cap.run(serde_json::json!({"limit": 2})).await.unwrap();
        assert_eq!(out["flushed"], 2);

        // One entry should still be staged
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) as \"count!\" FROM outbox_entries WHERE delivery_state = 'staged'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    // ── outbox.compact tests ──────────────────────────────────────────

    #[tokio::test]
    async fn outbox_compact_no_pool_returns_internal_error() {
        let cap = OutboxCompact::new();
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        match err {
            CapabilityError::Internal(msg) => {
                assert!(msg.contains("no database pool"));
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn outbox_compact_no_entries() {
        let pool = test_pool().await;
        let cap = OutboxCompact::with_pool(pool);
        let out = cap
            .run(serde_json::json!({"retentionDays": 7}))
            .await
            .unwrap();
        assert_eq!(out["removed"], 0);
        assert_eq!(out["retained"], 0);
    }

    #[tokio::test]
    async fn outbox_compact_old_acked_removed() {
        let pool = test_pool().await;
        // Insert an entry with old updated_at
        let old_time = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        // SAFETY: test-only DDL — insert with explicit updated_at for compaction test.
        sqlx::query(&format!(
            "INSERT INTO outbox_entries
             (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
              retry_count, created_at, updated_at)
             VALUES ('obe_old', 'bdl_old', 'idk_old', 'acked', 0, '{old_time}', '{old_time}')"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let cap = OutboxCompact::with_pool(pool.clone());
        let out = cap
            .run(serde_json::json!({"retentionDays": 7}))
            .await
            .unwrap();
        assert_eq!(out["removed"], 1);
        assert_eq!(out["retained"], 0);
    }

    #[tokio::test]
    async fn outbox_compact_recent_acked_preserved() {
        let pool = test_pool().await;
        let recent = chrono::Utc::now().to_rfc3339();
        // SAFETY: test-only DDL — insert with explicit updated_at.
        sqlx::query(&format!(
            "INSERT INTO outbox_entries
             (outbox_entry_id, bundle_id, idempotency_key, delivery_state,
              retry_count, created_at, updated_at)
             VALUES ('obe_recent', 'bdl_r', 'idk_r', 'acked', 0, '{recent}', '{recent}')"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let cap = OutboxCompact::with_pool(pool.clone());
        let out = cap
            .run(serde_json::json!({"retentionDays": 365}))
            .await
            .unwrap();
        assert_eq!(out["removed"], 0);
        assert_eq!(out["retained"], 1);
    }

    #[tokio::test]
    async fn outbox_compact_only_targets_acked() {
        let pool = test_pool().await;
        insert_entry(&pool, "obe_staged", "staged", None).await;
        insert_entry(&pool, "obe_failed", "failed", None).await;

        let cap = OutboxCompact::with_pool(pool.clone());
        let out = cap
            .run(serde_json::json!({"retentionDays": 0}))
            .await
            .unwrap();
        assert_eq!(out["removed"], 0);
        assert_eq!(out["retained"], 0);

        // Non-acked entries are untouched
        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) as \"count!\" FROM outbox_entries")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }
}
