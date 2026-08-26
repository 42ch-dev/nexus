//! Local identity CRUD operations for `SQLite`.
//!
//! Provides async functions to create, read, list, and update local identities
//! in the `local_identities` table. These are used by both CLI and daemon.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Row representation for the `local_identities` table.
#[derive(Debug, Clone)]
pub struct LocalIdentityRow {
    pub creator_id: String,
    pub identity_type: String,
    pub display_name: Option<String>,
    pub created_at: String,
    pub platform_linked: bool,
    pub platform_creator_id: Option<String>,
}

/// Create a new local identity in the database.
///
/// # Errors
///
/// Returns `LocalDbError` if the insert fails (e.g. duplicate `creator_id`).
pub async fn create_local_identity(
    pool: &SqlitePool,
    creator_id: &str,
    identity_type: &str,
    display_name: Option<&str>,
    created_at: &str,
) -> Result<LocalIdentityRow, LocalDbError> {
    sqlx::query!(
        "INSERT INTO local_identities (creator_id, identity_type, display_name, created_at, platform_linked)
         VALUES (?, ?, ?, ?, 0)",
        creator_id,
        identity_type,
        display_name,
        created_at
    )
    .execute(pool)
    .await?;

    Ok(LocalIdentityRow {
        creator_id: creator_id.to_string(),
        identity_type: identity_type.to_string(),
        display_name: display_name.map(std::string::ToString::to_string),
        created_at: created_at.to_string(),
        platform_linked: false,
        platform_creator_id: None,
    })
}

/// Get a local identity by `creator_id`.
///
/// Returns `None` if no identity exists with the given ID.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_local_identity(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<LocalIdentityRow>, LocalDbError> {
    let row = sqlx::query!(
        "SELECT creator_id as \"creator_id!\", identity_type as \"identity_type!\",
                display_name, created_at as \"created_at!\", platform_linked as \"platform_linked!\",
                platform_creator_id
         FROM local_identities WHERE creator_id = ?",
        creator_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| LocalIdentityRow {
        creator_id: r.creator_id,
        identity_type: r.identity_type,
        display_name: r.display_name,
        created_at: r.created_at,
        platform_linked: r.platform_linked != 0,
        platform_creator_id: r.platform_creator_id,
    }))
}

/// List all local identities.
///
/// Returns all identities sorted by creation time (oldest first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_local_identities(
    pool: &SqlitePool,
) -> Result<Vec<LocalIdentityRow>, LocalDbError> {
    let rows = sqlx::query!(
        "SELECT creator_id as \"creator_id!\", identity_type as \"identity_type!\",
                display_name, created_at as \"created_at!\", platform_linked as \"platform_linked!\",
                platform_creator_id
         FROM local_identities ORDER BY created_at"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| LocalIdentityRow {
            creator_id: r.creator_id,
            identity_type: r.identity_type,
            display_name: r.display_name,
            created_at: r.created_at,
            platform_linked: r.platform_linked != 0,
            platform_creator_id: r.platform_creator_id,
        })
        .collect())
}

/// Link a local identity to a platform Creator.
///
/// Sets `platform_linked = true` and stores the `platform_creator_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the identity does not exist.
pub async fn link_to_platform(
    pool: &SqlitePool,
    creator_id: &str,
    platform_creator_id: &str,
) -> Result<(), LocalDbError> {
    let result = sqlx::query!(
        "UPDATE local_identities SET platform_linked = 1, platform_creator_id = ?
         WHERE creator_id = ? AND platform_linked = 0",
        platform_creator_id,
        creator_id
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        let existing = get_local_identity(pool, creator_id).await?;
        match existing {
            Some(row) if row.platform_linked => {
                return Err(LocalDbError::IdentityAlreadyLinked {
                    creator_id: creator_id.to_string(),
                });
            }
            _ => {
                return Err(LocalDbError::IdentityNotFound {
                    creator_id: creator_id.to_string(),
                });
            }
        }
    }

    Ok(())
}

/// Unlink a local identity from its platform Creator.
///
/// Sets `platform_linked = false` and clears `platform_creator_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the identity does not exist or is not currently linked.
pub async fn unlink_from_platform(pool: &SqlitePool, creator_id: &str) -> Result<(), LocalDbError> {
    let result = sqlx::query!(
        "UPDATE local_identities SET platform_linked = 0, platform_creator_id = NULL
         WHERE creator_id = ? AND platform_linked = 1",
        creator_id
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        let existing = get_local_identity(pool, creator_id).await?;
        match existing {
            Some(row) if !row.platform_linked => {
                return Err(LocalDbError::IdentityNotLinked {
                    creator_id: creator_id.to_string(),
                });
            }
            _ => {
                return Err(LocalDbError::IdentityNotFound {
                    creator_id: creator_id.to_string(),
                });
            }
        }
    }

    Ok(())
}

/// Delete a local identity by `creator_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the delete fails.
pub async fn delete_local_identity(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM local_identities WHERE creator_id = ?",
        creator_id
    )
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
    async fn create_and_get_identity() {
        let (pool, _dir) = fresh_pool().await;
        let row = create_local_identity(
            &pool,
            "ctr_localTest123",
            "persistent",
            Some("Test User"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(row.creator_id, "ctr_localTest123");
        assert_eq!(row.identity_type, "persistent");
        assert_eq!(row.display_name, Some("Test User".to_string()));

        let fetched = get_local_identity(&pool, "ctr_localTest123").await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.creator_id, "ctr_localTest123");
    }

    // V1.176 P0 QC fix wave (qc3 S-002): the unique partial index on
    // persistent `display_name` rejects a second INSERT with the same name —
    // an honest failure instead of a silent duplicate. Two concurrent
    // 0-match reads can both mint; the index fences the second write.
    #[tokio::test]
    async fn duplicate_persistent_display_name_is_rejected() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localDup1",
            "persistent",
            Some("Same Name"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        let err = create_local_identity(
            &pool,
            "ctr_localDup2",
            "persistent",
            Some("Same Name"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .expect_err("second same-name persistent INSERT must fail");

        // Surface as a unique-constraint failure, not a silent duplicate row.
        let is_unique_violation = matches!(
            &err,
            LocalDbError::Sqlx(sqlx::Error::Database(db)) if db.is_unique_violation()
        );
        assert!(
            is_unique_violation,
            "expected a unique-constraint error, got: {err:?}"
        );
    }

    // qc3 S-002 companion: anonymous rows and nameless persistent rows are
    // NOT constrained — the index is partial (persistent + non-NULL name).
    #[tokio::test]
    async fn duplicate_nameless_persistent_rows_are_allowed() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localNoName1",
            "persistent",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
        create_local_identity(
            &pool,
            "ctr_localNoName2",
            "persistent",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .expect("nameless persistent rows are not name-constrained");
    }

    #[tokio::test]
    async fn get_nonexistent_identity() {
        let (pool, _dir) = fresh_pool().await;
        let result = get_local_identity(&pool, "ctr_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_identities() {
        let (pool, _dir) = fresh_pool().await;

        create_local_identity(
            &pool,
            "ctr_anonAaa",
            "anonymous",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
        create_local_identity(
            &pool,
            "ctr_localBbb",
            "persistent",
            Some("User B"),
            "2026-01-02T00:00:00Z",
        )
        .await
        .unwrap();

        let list = list_local_identities(&pool).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].creator_id, "ctr_anonAaa");
        assert_eq!(list[1].creator_id, "ctr_localBbb");
    }

    #[tokio::test]
    async fn list_identities_empty() {
        let (pool, _dir) = fresh_pool().await;
        let list = list_local_identities(&pool).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn link_identity_to_platform() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localLink",
            "persistent",
            Some("Test"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        link_to_platform(&pool, "ctr_localLink", "ctr_Platform123")
            .await
            .unwrap();

        let row = get_local_identity(&pool, "ctr_localLink")
            .await
            .unwrap()
            .unwrap();
        assert!(row.platform_linked);
        assert_eq!(row.platform_creator_id, Some("ctr_Platform123".to_string()));
    }

    #[tokio::test]
    async fn link_nonexistent_identity() {
        let (pool, _dir) = fresh_pool().await;
        let result = link_to_platform(&pool, "ctr_nonexistent", "ctr_Platform123").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LocalDbError::IdentityNotFound { .. })));
    }

    #[tokio::test]
    async fn link_already_linked_identity() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localAlreadyLinked",
            "persistent",
            Some("Test"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        // First link succeeds
        link_to_platform(&pool, "ctr_localAlreadyLinked", "ctr_Platform123")
            .await
            .unwrap();

        // Second link fails with IdentityAlreadyLinked
        let result = link_to_platform(&pool, "ctr_localAlreadyLinked", "ctr_Another456").await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(LocalDbError::IdentityAlreadyLinked { .. })
        ));
    }

    #[tokio::test]
    async fn delete_identity() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localDel",
            "persistent",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(get_local_identity(&pool, "ctr_localDel")
            .await
            .unwrap()
            .is_some());

        let deleted = delete_local_identity(&pool, "ctr_localDel").await.unwrap();
        assert!(deleted);

        assert!(get_local_identity(&pool, "ctr_localDel")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_identity() {
        let (pool, _dir) = fresh_pool().await;
        let deleted = delete_local_identity(&pool, "ctr_nonexistent")
            .await
            .unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn unlink_identity_from_platform() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localUnlink",
            "persistent",
            Some("Test"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        link_to_platform(&pool, "ctr_localUnlink", "ctr_Platform123")
            .await
            .unwrap();

        let row = get_local_identity(&pool, "ctr_localUnlink")
            .await
            .unwrap()
            .unwrap();
        assert!(row.platform_linked);

        unlink_from_platform(&pool, "ctr_localUnlink")
            .await
            .unwrap();

        let row = get_local_identity(&pool, "ctr_localUnlink")
            .await
            .unwrap()
            .unwrap();
        assert!(!row.platform_linked);
        assert!(row.platform_creator_id.is_none());
    }

    #[tokio::test]
    async fn unlink_nonexistent_identity() {
        let (pool, _dir) = fresh_pool().await;
        let result = unlink_from_platform(&pool, "ctr_nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LocalDbError::IdentityNotFound { .. })));
    }

    #[tokio::test]
    async fn unlink_already_unlinked_identity() {
        let (pool, _dir) = fresh_pool().await;
        create_local_identity(
            &pool,
            "ctr_localNeverLinked",
            "persistent",
            Some("Test"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        let result = unlink_from_platform(&pool, "ctr_localNeverLinked").await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(LocalDbError::IdentityNotLinked { .. })
        ));
    }
}
