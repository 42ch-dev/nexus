//! Force-gates audit log persistence (V1.37 §7.9.3).
//!
//! Append-only table recording every gate-bypass event.

use crate::LocalDbError;

/// Parameters for inserting a force-gates audit row.
#[derive(Debug, Clone)]
pub struct ForceGatesAuditParams {
    /// Unique audit ID (e.g. `fga_<timestamp>`).
    pub audit_id: String,
    /// Preset ID that was force-started.
    pub preset_id: String,
    /// Work ID.
    pub work_id: String,
    /// Creator ID who authorized the bypass.
    pub creator_id: String,
    /// User-provided reason text.
    pub reason: String,
    /// ISO-8601 timestamp.
    pub forced_at: String,
}

/// Insert a force-gates audit row.
///
/// # Errors
///
/// Returns `LocalDbError` if the insert fails.
pub async fn insert_force_gates_audit(
    pool: &sqlx::SqlitePool,
    params: &ForceGatesAuditParams,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT INTO force_gates_audit (audit_id, preset_id, work_id, creator_id, forced, reason, forced_at)
         VALUES (?, ?, ?, ?, TRUE, ?, ?)",
        params.audit_id,
        params.preset_id,
        params.work_id,
        params.creator_id,
        params.reason,
        params.forced_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Query all audit rows for a given creator, ordered by `forced_at` DESC.
///
/// # Errors
///
/// Returns `LocalDbError` if the query fails.
pub async fn list_force_gates_audit(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
) -> Result<Vec<ForceGatesAuditRow>, LocalDbError> {
    // SAFETY: runtime `sqlx::query_as` — SQLite BOOLEAN maps to bool differently
    // across sqlx compile-time vs runtime. Using runtime query to avoid type mismatch.
    let rows = sqlx::query_as::<_, ForceGatesAuditRow>(
        "SELECT audit_id, preset_id, work_id, creator_id, forced, reason, forced_at
         FROM force_gates_audit
         WHERE creator_id = ?
         ORDER BY forced_at DESC",
    )
    .bind(creator_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A force-gates audit row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ForceGatesAuditRow {
    /// Unique audit ID.
    pub audit_id: String,
    /// Preset ID that was force-started.
    pub preset_id: String,
    /// Work ID.
    pub work_id: String,
    /// Creator ID who authorized the bypass.
    pub creator_id: String,
    /// Whether gates were forced.
    pub forced: bool,
    /// User-provided reason text.
    pub reason: Option<String>,
    /// ISO-8601 timestamp.
    pub forced_at: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_and_query_audit_row() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        // Create the table
        sqlx::query(
            "CREATE TABLE force_gates_audit (
                audit_id   TEXT PRIMARY KEY,
                preset_id  TEXT NOT NULL,
                work_id    TEXT NOT NULL,
                creator_id TEXT NOT NULL,
                forced     BOOLEAN NOT NULL DEFAULT TRUE,
                reason     TEXT,
                forced_at  TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create table");

        let params = ForceGatesAuditParams {
            audit_id: "fga_20260608".to_string(),
            preset_id: "novel-writing".to_string(),
            work_id: "wrk_test".to_string(),
            creator_id: "ctr_test".to_string(),
            reason: "emergency override".to_string(),
            forced_at: "2026-06-08T12:00:00Z".to_string(),
        };

        insert_force_gates_audit(&pool, &params)
            .await
            .expect("insert");

        let rows = list_force_gates_audit(&pool, "ctr_test")
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.audit_id, "fga_20260608");
        assert_eq!(row.preset_id, "novel-writing");
        assert_eq!(row.work_id, "wrk_test");
        assert!(row.forced);
        assert_eq!(row.reason.as_deref(), Some("emergency override"));
    }
}
