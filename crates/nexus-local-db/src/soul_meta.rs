//! SOUL metadata persistence (local `SQLite`).
//!
//! Tracks per-creator SOUL.md metadata for fast lookups without file I/O.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// SOUL metadata record.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SoulMeta {
    pub creator_id: String,
    pub file_path: String,
    /// Schema version of the soul's data. Changed from `u32` to `i64` during
    /// the WS8 sqlx migration (rusqlite → sqlx) for `SQLx` type compatibility.
    pub schema_version: i64,
    pub personality_hash: Option<String>,
    pub experience_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Upsert SOUL metadata (insert or update).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn upsert(pool: &SqlitePool, meta: &SoulMeta) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT INTO soul_meta (creator_id, file_path, schema_version, personality_hash, experience_hash, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(creator_id) DO UPDATE SET
           file_path = excluded.file_path,
           schema_version = excluded.schema_version,
           personality_hash = excluded.personality_hash,
           experience_hash = excluded.experience_hash,
           updated_at = excluded.updated_at",
        meta.creator_id,
        meta.file_path,
        meta.schema_version,
        meta.personality_hash,
        meta.experience_hash,
        meta.created_at,
        meta.updated_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Get SOUL metadata for a creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get(pool: &SqlitePool, creator_id: &str) -> Result<Option<SoulMeta>, LocalDbError> {
    let row = sqlx::query_as!(
        SoulMeta,
        "SELECT creator_id as \"creator_id!\", file_path as \"file_path!\",
                schema_version as \"schema_version!\",
                personality_hash, experience_hash,
                created_at as \"created_at!\", updated_at as \"updated_at!\"
         FROM soul_meta WHERE creator_id = ?",
        creator_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Delete SOUL metadata for a creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn delete(pool: &SqlitePool, creator_id: &str) -> Result<bool, LocalDbError> {
    let result = sqlx::query!("DELETE FROM soul_meta WHERE creator_id = ?", creator_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn upsert_and_get() {
        let (pool, _dir) = fresh_pool().await;
        let meta = SoulMeta {
            creator_id: "ctr_test".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: Some("abc".to_string()),
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&pool, &meta).await.unwrap();

        let fetched = get(&pool, "ctr_test").await.unwrap().unwrap();
        assert_eq!(fetched.creator_id, "ctr_test");
        assert_eq!(fetched.personality_hash.as_deref(), Some("abc"));
        assert_eq!(fetched.experience_hash, None);
    }

    #[tokio::test]
    async fn upsert_updates_existing() {
        let (pool, _dir) = fresh_pool().await;
        let meta = SoulMeta {
            creator_id: "ctr_test".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: None,
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&pool, &meta).await.unwrap();

        let updated = SoulMeta {
            personality_hash: Some("new_hash".to_string()),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            ..meta
        };
        upsert(&pool, &updated).await.unwrap();

        let fetched = get(&pool, "ctr_test").await.unwrap().unwrap();
        assert_eq!(fetched.personality_hash.as_deref(), Some("new_hash"));
        assert_eq!(fetched.updated_at, "2026-01-02T00:00:00Z");
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get(&pool, "ctr_ghost").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_existing() {
        let (pool, _dir) = fresh_pool().await;
        let meta = SoulMeta {
            creator_id: "ctr_del".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: None,
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&pool, &meta).await.unwrap();
        assert!(delete(&pool, "ctr_del").await.unwrap());
        assert!(get(&pool, "ctr_del").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_false() {
        let (pool, _dir) = fresh_pool().await;
        assert!(!delete(&pool, "ctr_ghost").await.unwrap());
    }
}
