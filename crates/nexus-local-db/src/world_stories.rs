//! World-story association CRUD operations for novel-writing preset.
//!
//! Maps story references (directories under `Stories/`) to their parent world
//! and tracks chapter metadata for sync module consumption.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::LocalDbError;

/// World-story association record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct WorldStory {
    /// Unique identifier with `wsr_` prefix.
    pub id: String,
    /// Foreign key to world (`wld_` prefix).
    pub world_id: String,
    /// Story reference (directory name under `Stories/`).
    pub story_ref: String,
    /// Absolute path to `Stories/<story_ref>/`.
    pub workspace_path: String,
    /// Number of chapters detected.
    pub chapter_count: i64,
    /// First chapter file name (optional).
    pub first_chapter_id: Option<String>,
    /// Most recent chapter file name (optional).
    pub latest_chapter_id: Option<String>,
    /// Story status: `draft` | `review` | `final` | `published`.
    pub status: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
}

/// Generate a new world-story ID with `wsr_` prefix.
#[must_use]
pub fn generate_id() -> String {
    format!("wsr_{}", Uuid::new_v4().simple())
}

/// Create a new world-story association.
///
/// Uses `ON CONFLICT` upsert: if `(world_id, story_ref)` already exists,
/// only `updated_at` is refreshed and the existing row is returned.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_world_story(
    pool: &SqlitePool,
    world_id: &str,
    story_ref: &str,
    workspace_path: &str,
) -> Result<WorldStory, LocalDbError> {
    let id = generate_id();
    let row = sqlx::query!(
        r#"INSERT INTO world_stories (id, world_id, story_ref, workspace_path)
          VALUES (?, ?, ?, ?)
          ON CONFLICT(world_id, story_ref) DO UPDATE SET updated_at = datetime('now')
          RETURNING
            id as "id!",
            world_id as "world_id!",
            story_ref as "story_ref!",
            workspace_path as "workspace_path!",
            chapter_count as "chapter_count!",
            first_chapter_id,
            latest_chapter_id,
            status as "status!",
            created_at as "created_at!",
            updated_at as "updated_at!""#,
        id,
        world_id,
        story_ref,
        workspace_path
    )
    .fetch_one(pool)
    .await?;

    Ok(WorldStory {
        id: row.id,
        world_id: row.world_id,
        story_ref: row.story_ref,
        workspace_path: row.workspace_path,
        chapter_count: row.chapter_count,
        first_chapter_id: row.first_chapter_id,
        latest_chapter_id: row.latest_chapter_id,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// List all stories for a given world.
///
/// Returns records ordered by `created_at` ascending (oldest first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_by_world(
    pool: &SqlitePool,
    world_id: &str,
) -> Result<Vec<WorldStory>, LocalDbError> {
    let rows = sqlx::query!(
        r#"SELECT
            id as "id!",
            world_id as "world_id!",
            story_ref as "story_ref!",
            workspace_path as "workspace_path!",
            chapter_count as "chapter_count!",
            first_chapter_id,
            latest_chapter_id,
            status as "status!",
            created_at as "created_at!",
            updated_at as "updated_at!"
          FROM world_stories WHERE world_id = ? ORDER BY created_at"#,
        world_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| WorldStory {
            id: r.id,
            world_id: r.world_id,
            story_ref: r.story_ref,
            workspace_path: r.workspace_path,
            chapter_count: r.chapter_count,
            first_chapter_id: r.first_chapter_id,
            latest_chapter_id: r.latest_chapter_id,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// Get a specific world-story association by ID.
///
/// Returns `None` if the record doesn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_by_id(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<WorldStory>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT
            id as "id!",
            world_id as "world_id!",
            story_ref as "story_ref!",
            workspace_path as "workspace_path!",
            chapter_count as "chapter_count!",
            first_chapter_id,
            latest_chapter_id,
            status as "status!",
            created_at as "created_at!",
            updated_at as "updated_at!"
          FROM world_stories WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| WorldStory {
        id: r.id,
        world_id: r.world_id,
        story_ref: r.story_ref,
        workspace_path: r.workspace_path,
        chapter_count: r.chapter_count,
        first_chapter_id: r.first_chapter_id,
        latest_chapter_id: r.latest_chapter_id,
        status: r.status,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Update chapter metadata after a sync scan.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_chapters(
    pool: &SqlitePool,
    id: &str,
    chapter_count: i64,
    first_chapter_id: Option<&str>,
    latest_chapter_id: Option<&str>,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        r#"UPDATE world_stories
          SET chapter_count = ?,
              first_chapter_id = ?,
              latest_chapter_id = ?,
              updated_at = datetime('now')
          WHERE id = ?"#,
        chapter_count,
        first_chapter_id,
        latest_chapter_id,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Update story status.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), LocalDbError> {
    sqlx::query!(
        r#"UPDATE world_stories
          SET status = ?, updated_at = datetime('now')
          WHERE id = ?"#,
        status,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a world-story association.
///
/// Returns `true` if a record was deleted, `false` if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn delete_world_story(pool: &SqlitePool, id: &str) -> Result<bool, LocalDbError> {
    let result = sqlx::query!("DELETE FROM world_stories WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    async fn test_create_world_story() {
        let (pool, _dir) = fresh_pool().await;
        let story = create_world_story(&pool, "wld_abc123", "my-novel", "/Stories/my-novel/")
            .await
            .unwrap();

        assert!(story.id.starts_with("wsr_"));
        assert_eq!(story.world_id, "wld_abc123");
        assert_eq!(story.story_ref, "my-novel");
        assert_eq!(story.workspace_path, "/Stories/my-novel/");
        assert_eq!(story.chapter_count, 0);
        assert!(story.first_chapter_id.is_none());
        assert!(story.latest_chapter_id.is_none());
        assert_eq!(story.status, "draft");

        // Verify retrieval
        let fetched = get_by_id(&pool, &story.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, story.id);
        assert_eq!(fetched.world_id, "wld_abc123");
    }

    #[tokio::test]
    async fn test_create_world_story_idempotent() {
        let (pool, _dir) = fresh_pool().await;
        let story1 = create_world_story(&pool, "wld_abc", "novel-a", "/Stories/novel-a/")
            .await
            .unwrap();

        // Insert same (world_id, story_ref) again — should upsert
        let story2 = create_world_story(&pool, "wld_abc", "novel-a", "/Stories/novel-a/")
            .await
            .unwrap();

        // Same row ID (existing preserved on conflict)
        assert_eq!(story1.id, story2.id);
        // Timestamp updated
        assert!(story2.updated_at >= story1.updated_at);

        // Only one row in table
        let all = list_by_world(&pool, "wld_abc").await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_world() {
        let (pool, _dir) = fresh_pool().await;

        create_world_story(&pool, "wld_world1", "story-a", "/Stories/story-a/")
            .await
            .unwrap();
        create_world_story(&pool, "wld_world1", "story-b", "/Stories/story-b/")
            .await
            .unwrap();
        create_world_story(&pool, "wld_world2", "story-c", "/Stories/story-c/")
            .await
            .unwrap();

        let world1 = list_by_world(&pool, "wld_world1").await.unwrap();
        assert_eq!(world1.len(), 2);
        assert_eq!(world1[0].story_ref, "story-a");
        assert_eq!(world1[1].story_ref, "story-b");

        let world2 = list_by_world(&pool, "wld_world2").await.unwrap();
        assert_eq!(world2.len(), 1);
        assert_eq!(world2[0].story_ref, "story-c");

        let empty = list_by_world(&pool, "wld_nonexistent").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_by_id(&pool, "wsr_ghost").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_chapters() {
        let (pool, _dir) = fresh_pool().await;
        let story = create_world_story(&pool, "wld_test", "novel-x", "/Stories/novel-x/")
            .await
            .unwrap();

        update_chapters(
            &pool,
            &story.id,
            5,
            Some("ch01-first-light.md"),
            Some("ch05-reckoning.md"),
        )
        .await
        .unwrap();

        let fetched = get_by_id(&pool, &story.id).await.unwrap().unwrap();
        assert_eq!(fetched.chapter_count, 5);
        assert_eq!(
            fetched.first_chapter_id.as_deref(),
            Some("ch01-first-light.md")
        );
        assert_eq!(
            fetched.latest_chapter_id.as_deref(),
            Some("ch05-reckoning.md")
        );
    }

    #[tokio::test]
    async fn test_update_chapters_with_nulls() {
        let (pool, _dir) = fresh_pool().await;
        let story = create_world_story(&pool, "wld_null", "novel-y", "/Stories/novel-y/")
            .await
            .unwrap();

        update_chapters(&pool, &story.id, 0, None, None)
            .await
            .unwrap();

        let fetched = get_by_id(&pool, &story.id).await.unwrap().unwrap();
        assert_eq!(fetched.chapter_count, 0);
        assert!(fetched.first_chapter_id.is_none());
        assert!(fetched.latest_chapter_id.is_none());
    }

    #[tokio::test]
    async fn test_update_status() {
        let (pool, _dir) = fresh_pool().await;
        let story = create_world_story(&pool, "wld_status", "novel-z", "/Stories/novel-z/")
            .await
            .unwrap();

        assert_eq!(story.status, "draft");

        update_status(&pool, &story.id, "review").await.unwrap();

        let fetched = get_by_id(&pool, &story.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, "review");

        update_status(&pool, &story.id, "published").await.unwrap();

        let fetched = get_by_id(&pool, &story.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, "published");
    }

    #[tokio::test]
    async fn test_delete() {
        let (pool, _dir) = fresh_pool().await;
        let story = create_world_story(&pool, "wld_del", "novel-del", "/Stories/novel-del/")
            .await
            .unwrap();

        assert!(delete_world_story(&pool, &story.id).await.unwrap());
        assert!(get_by_id(&pool, &story.id).await.unwrap().is_none());

        // Delete nonexistent returns false
        assert!(!delete_world_story(&pool, &story.id).await.unwrap());
    }

    #[tokio::test]
    async fn test_generate_id_format() {
        let id = generate_id();
        assert!(id.starts_with("wsr_"));
        // UUID without hyphens: 32 hex chars
        let uuid_part = &id[4..];
        assert_eq!(uuid_part.len(), 32);
        assert!(uuid_part.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
