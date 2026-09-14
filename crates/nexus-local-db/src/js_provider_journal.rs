//! Durable JS-provider operation journal (V1.189 P4-T2 / LIFE-3).
//!
//! The in-memory native `JsProviderState` is process-scoped; a restart loses it.
//! This journal is the Rust-owned durable record so a previously active
//! non-resumable JS-provider operation is still queryable by the same operation
//! id after the predecessor process exits — reported as `interrupted`, never
//! 404 and never a fabricated terminal. Retention is bounded to
//! [`MAX_JOURNAL_ENTRIES`], pruning the oldest by sequence.
//!
//! This is not business SQL and not a second truth: it mirrors the native
//! provider-callback facts so the query surface can survive a restart.

use sqlx::SqlitePool;

use crate::LocalDbError;

/// Bounded retention, matching the native terminal-operation cap (64).
pub const MAX_JOURNAL_ENTRIES: i64 = 64;

/// One journaled operation row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournaledOperation {
    pub operation_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub status: String,
    pub sequence: i64,
}

fn db_err(e: sqlx::Error) -> LocalDbError {
    LocalDbError::Sqlx(e)
}

/// Upsert an operation and prune the journal to the bounded retention window.
///
/// A row that is already terminal is never downgraded back to `running`, so a
/// restart cannot resurrect a settled operation.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn upsert_operation(
    pool: &SqlitePool,
    operation_id: &str,
    session_id: &str,
    provider_id: &str,
    status: &str,
) -> Result<(), LocalDbError> {
    sqlx::query(
        r"INSERT INTO js_provider_operation_journal
              (operation_id, session_id, provider_id, status, sequence)
          VALUES (?, ?, ?, ?, COALESCE((SELECT MAX(sequence) FROM js_provider_operation_journal), 0) + 1)
          ON CONFLICT(operation_id) DO UPDATE SET
              session_id = excluded.session_id,
              provider_id = excluded.provider_id,
              status = CASE
                  WHEN js_provider_operation_journal.status IN ('finished','failed','interrupted','cancelled')
                      THEN js_provider_operation_journal.status
                  ELSE excluded.status
              END,
              updated_at = datetime('now')",
    )
    .bind(operation_id)
    .bind(session_id)
    .bind(provider_id)
    .bind(status)
    .execute(pool)
    .await
    .map_err(db_err)?;
    prune_to_cap(pool).await
}

/// Fetch a single journaled operation by id.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn get_operation(
    pool: &SqlitePool,
    operation_id: &str,
) -> Result<Option<JournaledOperation>, LocalDbError> {
    let row = sqlx::query_as::<_, (String, String, String, String, i64)>(
        "SELECT operation_id, session_id, provider_id, status, sequence
           FROM js_provider_operation_journal WHERE operation_id = ?",
    )
    .bind(operation_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;
    Ok(row.map(|(operation_id, session_id, provider_id, status, sequence)| {
        JournaledOperation {
            operation_id,
            session_id,
            provider_id,
            status,
            sequence,
        }
    }))
}

/// List journaled operations, newest first, bounded by `limit`.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn list_operations(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<JournaledOperation>, LocalDbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, i64)>(
        "SELECT operation_id, session_id, provider_id, status, sequence
           FROM js_provider_operation_journal ORDER BY sequence DESC LIMIT ?",
    )
    .bind(limit.clamp(1, MAX_JOURNAL_ENTRIES))
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(operation_id, session_id, provider_id, status, sequence)| {
            JournaledOperation {
                operation_id,
                session_id,
                provider_id,
                status,
                sequence,
            }
        })
        .collect())
}

/// List journaled operations for a session, newest first.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn list_session_operations(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<JournaledOperation>, LocalDbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, i64)>(
        "SELECT operation_id, session_id, provider_id, status, sequence
           FROM js_provider_operation_journal WHERE session_id = ? ORDER BY sequence DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(operation_id, session_id, provider_id, status, sequence)| {
            JournaledOperation {
                operation_id,
                session_id,
                provider_id,
                status,
                sequence,
            }
        })
        .collect())
}

/// Forget an operation whose session was shut down cleanly.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn forget_session(pool: &SqlitePool, session_id: &str) -> Result<(), LocalDbError> {
    sqlx::query("DELETE FROM js_provider_operation_journal WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// Mark every non-terminal journaled operation `interrupted`. Called on open to
/// settle operations orphaned by the predecessor process's exit.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn settle_orphaned_as_interrupted(pool: &SqlitePool) -> Result<u64, LocalDbError> {
    let result = sqlx::query(
        "UPDATE js_provider_operation_journal SET status = 'interrupted', updated_at = datetime('now')
          WHERE status NOT IN ('finished','failed','interrupted','cancelled')",
    )
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(result.rows_affected())
}

async fn prune_to_cap(pool: &SqlitePool) -> Result<(), LocalDbError> {
    sqlx::query(
        "DELETE FROM js_provider_operation_journal WHERE operation_id IN (
             SELECT operation_id FROM js_provider_operation_journal
             ORDER BY sequence DESC LIMIT -1 OFFSET ?
         )",
    )
    .bind(MAX_JOURNAL_ENTRIES)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer_protocol::{init_engine_pool, GuardedPoolOptions};

    /// A real migrated + admitted engine pool, exactly the production shape the
    /// native core uses, so the writer-protocol guards on the journal table are
    /// exercised (a plain migrator pool would bypass admission).
    async fn admitted_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let guarded = init_engine_pool(
            &db_path,
            crate::writer_protocol::BOOTSTRAP_CREATOR_ID,
            GuardedPoolOptions::default(),
        )
        .await
        .expect("open admitted engine pool");
        (guarded.clone_pool(), dir)
    }

    #[tokio::test]
    async fn orphaned_running_operation_settles_to_interrupted() {
        let (pool, _dir) = admitted_pool().await;
        upsert_operation(&pool, "op-1", "sess-1", "mock-acp", "running")
            .await
            .unwrap();
        // The predicate is a running operation, never a fabricated terminal.
        let before = get_operation(&pool, "op-1").await.unwrap().unwrap();
        assert_eq!(before.status, "running");
        assert_eq!(before.session_id, "sess-1");
        // A new process open settles it.
        let affected = settle_orphaned_as_interrupted(&pool).await.unwrap();
        assert_eq!(affected, 1);
        let after = get_operation(&pool, "op-1").await.unwrap().unwrap();
        assert_eq!(after.status, "interrupted");
        assert_eq!(after.operation_id, "op-1");
        assert_eq!(after.session_id, "sess-1");
    }

    #[tokio::test]
    async fn terminal_is_never_downgraded_by_a_late_running_write() {
        let (pool, _dir) = admitted_pool().await;
        upsert_operation(&pool, "op-2", "sess-2", "mock-acp", "running")
            .await
            .unwrap();
        settle_orphaned_as_interrupted(&pool).await.unwrap();
        // A late `running` write must not resurrect a settled operation.
        upsert_operation(&pool, "op-2", "sess-2", "mock-acp", "running")
            .await
            .unwrap();
        assert_eq!(
            get_operation(&pool, "op-2").await.unwrap().unwrap().status,
            "interrupted"
        );
    }

    #[tokio::test]
    async fn journal_is_bounded_to_the_retention_cap() {
        let (pool, _dir) = admitted_pool().await;
        for i in 0..(MAX_JOURNAL_ENTRIES + 10) {
            upsert_operation(
                &pool,
                &format!("op-{i}"),
                &format!("sess-{i}"),
                "mock-acp",
                "finished",
            )
            .await
            .unwrap();
        }
        let all = list_operations(&pool, i64::MAX).await.unwrap();
        assert_eq!(all.len() as i64, MAX_JOURNAL_ENTRIES, "retention must be bounded");
        // The newest survive; the oldest are pruned.
        assert!(get_operation(&pool, "op-0").await.unwrap().is_none());
        assert!(get_operation(&pool, &format!("op-{}", MAX_JOURNAL_ENTRIES + 9))
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn clean_shutdown_forgets_the_session_rows() {
        let (pool, _dir) = admitted_pool().await;
        upsert_operation(&pool, "op-3", "sess-3", "mock-acp", "running")
            .await
            .unwrap();
        upsert_operation(&pool, "op-4", "sess-3", "mock-acp", "finished")
            .await
            .unwrap();
        upsert_operation(&pool, "op-5", "sess-other", "mock-acp", "running")
            .await
            .unwrap();
        forget_session(&pool, "sess-3").await.unwrap();
        assert!(get_operation(&pool, "op-3").await.unwrap().is_none());
        assert!(get_operation(&pool, "op-4").await.unwrap().is_none());
        assert!(list_session_operations(&pool, "sess-3").await.unwrap().is_empty());
        // Another session's row is untouched.
        assert!(get_operation(&pool, "op-5").await.unwrap().is_some());
    }

    /// The admitted engine pool must be allowed to write the new table (the
    /// guards admit the engine owner); a Direct `open_pool` must be fenced.
    /// This proves authorizer/guard behavior, not just compilation.
    #[tokio::test]
    async fn guarded_admission_permits_engine_owner_writes() {
        let (pool, _dir) = admitted_pool().await;
        // Engine owner write succeeds (the DAO call above already proves this,
        // but assert directly here for the guard contract).
        upsert_operation(&pool, "op-g", "sess-g", "mock-acp", "running")
            .await
            .expect("engine owner must be admitted");
        // A separate database, Direct-mode pool: the engine-owned journal table
        // must fence it. Using a distinct file avoids the retained engine guard.
        let direct_dir = tempfile::tempdir().unwrap();
        let direct_path = direct_dir.path().join("direct.db");
        let direct = crate::open_pool(&direct_path)
            .await
            .expect("open direct pool");
        let raw = sqlx::query(
            "INSERT INTO js_provider_operation_journal (operation_id, session_id, provider_id, status, sequence)
             VALUES ('op-direct', 'sess-direct', 'mock-acp', 'running', 9999)",
        )
        .execute(&direct)
        .await;
        assert!(raw.is_err(), "a Direct writer must be fenced for engine-owned journal state");
        let msg = format!("{}", raw.unwrap_err());
        assert!(msg.contains("WRITER_FENCED"), "expected WRITER_FENCED, got: {msg}");
    }

    /// A durable `cancelled` is terminal: a late `running` upsert must not
    /// downgrade it, and settle-on-open must not rewrite it to `interrupted`.
    #[tokio::test]
    async fn cancelled_survives_late_running_upsert_and_settle_orphaned() {
        let (pool, _dir) = admitted_pool().await;
        upsert_operation(&pool, "op-cancel", "sess-c", "mock-acp", "cancelled")
            .await
            .unwrap();
        // A late `running` write (e.g. a stale callback) must not downgrade it.
        upsert_operation(&pool, "op-cancel", "sess-c", "mock-acp", "running")
            .await
            .unwrap();
        assert_eq!(
            get_operation(&pool, "op-cancel").await.unwrap().unwrap().status,
            "cancelled",
            "a late running upsert must not downgrade a cancelled operation"
        );
        // A genuinely running op is still settled (proves the predicate is live).
        upsert_operation(&pool, "op-run", "sess-c", "mock-acp", "running")
            .await
            .unwrap();
        let affected = settle_orphaned_as_interrupted(&pool).await.unwrap();
        assert_eq!(affected, 1, "only the genuinely running op is settled");
        assert_eq!(
            get_operation(&pool, "op-cancel").await.unwrap().unwrap().status,
            "cancelled",
            "settle_orphaned must not rewrite a cancelled operation"
        );
        assert_eq!(
            get_operation(&pool, "op-run").await.unwrap().unwrap().status,
            "interrupted"
        );
    }
}

