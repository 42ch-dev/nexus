//! Workspace `creators` row materialization.
//!
//! The `creators` table lives in the **workspace** state db (per-creator +
//! workspace `SQLite` under ADR-014), while local identities live in the
//! global `~/.nexus42/state.db`. `create_world` prechecks the workspace
//! `creators` table for the owner creator, so any flow that mints a creator
//! outside the workspace db (e.g. `creator register --local`) must also
//! materialize the row here or world creation fails its FK precheck.
//!
//! V1.167 P2 T2: mirrors the daemon-private `upsert_creator_display_name`
//! (crates/nexus-daemon-runtime/src/api/handlers/creators.rs) so the CLI
//! local-register path no longer depends on the undocumented HTTP
//! `PATCH /v1/daemon/creators/{id}` workaround. The daemon copy stays
//! private (no cross-crate refactor this plan).

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Upsert a minimal active `creators` row for `creator_id`.
///
/// UPDATE-else-INSERT mirroring the daemon helper's SQL verbatim:
/// `display_name`, `cached_at` (RFC3339 via `chrono::Utc::now().to_rfc3339()`),
/// `status='active'`, `data='{}'`. Idempotent: re-running updates
/// `display_name`/`cached_at` in place instead of duplicating the row.
///
/// # Errors
///
/// Returns `LocalDbError` if the UPDATE or INSERT fails.
pub async fn ensure_creator_row(
    pool: &SqlitePool,
    creator_id: &str,
    display_name: &str,
) -> Result<(), LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let updated = sqlx::query!(
        "UPDATE creators SET display_name = ?, cached_at = ? WHERE creator_id = ?",
        display_name,
        now,
        creator_id
    )
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        sqlx::query!(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', ?, '{}')",
            creator_id,
            display_name,
            now
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh migrated pool in a tempdir (same pattern as `identity::tests`).
    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn ensure_creator_row_materializes_active_row() {
        let (pool, _dir) = fresh_pool().await;

        ensure_creator_row(&pool, "ctr_localMat", "Local Materializer")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL
        // (20260417_000001_initial.sql).
        let row = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT creator_id, display_name, status, cached_at, data \
             FROM creators WHERE creator_id = ?",
        )
        .bind("ctr_localMat")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.0, "ctr_localMat");
        assert_eq!(row.1, "Local Materializer");
        assert_eq!(row.2, "active");
        assert_eq!(row.4, "{}");
        // cached_at must be RFC3339 — mirrors the daemon helper's
        // `chrono::Utc::now().to_rfc3339()`.
        chrono::DateTime::parse_from_rfc3339(&row.3).expect("cached_at must be RFC3339");
    }

    #[tokio::test]
    async fn ensure_creator_row_is_idempotent_and_updates_in_place() {
        let (pool, _dir) = fresh_pool().await;

        ensure_creator_row(&pool, "ctr_localIdem", "First Name")
            .await
            .unwrap();
        ensure_creator_row(&pool, "ctr_localIdem", "First Name")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators WHERE creator_id = ?")
            .bind("ctr_localIdem")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "re-running must not duplicate the row");

        // UPDATE branch: a changed display name is applied in place.
        ensure_creator_row(&pool, "ctr_localIdem", "Renamed")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL.
        let display_name: String =
            sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
                .bind("ctr_localIdem")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(display_name, "Renamed");
    }
}
