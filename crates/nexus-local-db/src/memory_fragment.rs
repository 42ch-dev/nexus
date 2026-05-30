//! Memory fragment CRUD operations for review pipeline.
//!
//! Manages lightweight keyword-indexed fragments from review decisions.
//! See creator-memory-soul-lifecycle-v1.md §7.2.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// Memory fragment record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct MemoryFragmentRecord {
    /// Unique identifier for this fragment.
    pub fragment_id: String,
    /// ACP session ID that generated this fragment.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Keywords extracted from digest (stored as JSON array).
    pub keywords: String,
    /// Short summary of the fragment.
    pub summary: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Optional TTL (e.g., "30d", "90d").
    pub ttl: Option<String>,
}

/// Create a new memory fragment.
///
/// Inserts the fragment into the `memory_fragments` table.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_fragment(
    pool: &SqlitePool,
    fragment: &MemoryFragmentRecord,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT INTO memory_fragments (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        fragment.fragment_id,
        fragment.session_id,
        fragment.creator_id,
        fragment.keywords,
        fragment.summary,
        fragment.created_at,
        fragment.ttl
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// List all fragments for a creator.
///
/// Returns records ordered by `created_at` descending (most recent first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_fragments(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Vec<MemoryFragmentRecord>, LocalDbError> {
    let rows = sqlx::query!(
        "SELECT fragment_id as \"fragment_id!\", session_id as \"session_id!\",
                creator_id as \"creator_id!\", keywords as \"keywords!\",
                summary as \"summary!\", created_at as \"created_at!\", ttl
         FROM memory_fragments WHERE creator_id = ? ORDER BY created_at DESC",
        creator_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| MemoryFragmentRecord {
            fragment_id: r.fragment_id,
            session_id: r.session_id,
            creator_id: r.creator_id,
            keywords: r.keywords,
            summary: r.summary,
            created_at: r.created_at,
            ttl: r.ttl,
        })
        .collect())
}

/// List all fragments for a specific session.
///
/// Returns records for a given `session_id` (useful for session-level review).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_fragments_by_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<MemoryFragmentRecord>, LocalDbError> {
    let rows = sqlx::query!(
        "SELECT fragment_id as \"fragment_id!\", session_id as \"session_id!\",
                creator_id as \"creator_id!\", keywords as \"keywords!\",
                summary as \"summary!\", created_at as \"created_at!\", ttl
         FROM memory_fragments WHERE session_id = ? ORDER BY created_at DESC",
        session_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| MemoryFragmentRecord {
            fragment_id: r.fragment_id,
            session_id: r.session_id,
            creator_id: r.creator_id,
            keywords: r.keywords,
            summary: r.summary,
            created_at: r.created_at,
            ttl: r.ttl,
        })
        .collect())
}

/// Delete a fragment by ID.
///
/// Returns true if a record was deleted, false if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn delete_fragment(pool: &SqlitePool, fragment_id: &str) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM memory_fragments WHERE fragment_id = ?",
        fragment_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Get all deduped keywords for a creator.
///
/// Returns a union of all keywords from all fragments for this creator.
/// Used for context assembly §9.2 fragment keywords block.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_all_keywords(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Vec<String>, LocalDbError> {
    let keywords_rows = sqlx::query_scalar!(
        "SELECT keywords as \"keywords!\" FROM memory_fragments WHERE creator_id = ?",
        creator_id
    )
    .fetch_all(pool)
    .await?;

    // Parse each JSON array and collect unique keywords
    let mut all_keywords: Vec<String> = Vec::new();
    for row in keywords_rows {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&row) {
            for kw in keywords {
                if !all_keywords.contains(&kw) {
                    all_keywords.push(kw);
                }
            }
        }
    }

    Ok(all_keywords)
}

/// List fragments for a creator with optional keyword filter and limit.
///
/// When `keyword` is `Some`, only fragments whose `keywords` JSON array contains
/// the given keyword (case-insensitive `LIKE` match) are returned.
/// Results are ordered by `created_at` descending, limited to `limit` rows.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_fragments_filtered(
    pool: &SqlitePool,
    creator_id: &str,
    keyword: Option<&str>,
    limit: u32,
) -> Result<Vec<MemoryFragmentRecord>, LocalDbError> {
    // SAFETY: keyword and limit are used in a parameterized query (no injection risk).
    // Dynamic SQL is used because the optional keyword filter changes the WHERE clause
    // structure, which cannot be expressed with sqlx compile-time macros alone.
    let rows = if let Some(kw) = keyword {
        let pattern = format!("%\"{kw}\"%");
        sqlx::query(
            "SELECT fragment_id as \"fragment_id!\", session_id as \"session_id!\",
                    creator_id as \"creator_id!\", keywords as \"keywords!\",
                    summary as \"summary!\", created_at as \"created_at!\", ttl
             FROM memory_fragments
             WHERE creator_id = ? AND keywords LIKE ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(creator_id)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT fragment_id as \"fragment_id!\", session_id as \"session_id!\",
                    creator_id as \"creator_id!\", keywords as \"keywords!\",
                    summary as \"summary!\", created_at as \"created_at!\", ttl
             FROM memory_fragments
             WHERE creator_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(creator_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|row| MemoryFragmentRecord {
            fragment_id: row.get("fragment_id"),
            session_id: row.get("session_id"),
            creator_id: row.get("creator_id"),
            keywords: row.get("keywords"),
            summary: row.get("summary"),
            created_at: row.get("created_at"),
            ttl: row.get("ttl"),
        })
        .collect())
}

/// Count fragments for a creator with optional keyword filter.
///
/// Returns the number of matching fragments without fetching full rows.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_fragments(
    pool: &SqlitePool,
    creator_id: &str,
    keyword: Option<&str>,
) -> Result<u32, LocalDbError> {
    // SAFETY: same rationale as list_fragments_filtered — dynamic WHERE for optional keyword.
    let count: i64 = if let Some(kw) = keyword {
        let pattern = format!("%\"{kw}\"%");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_fragments
             WHERE creator_id = ? AND keywords LIKE ?",
        )
        .bind(creator_id)
        .bind(&pattern)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_fragments WHERE creator_id = ?")
            .bind(creator_id)
            .fetch_one(pool)
            .await?
    };
    Ok(u32::try_from(count).unwrap_or(0))
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

    fn sample_fragment(fragment_id: &str) -> MemoryFragmentRecord {
        MemoryFragmentRecord {
            fragment_id: fragment_id.to_string(),
            session_id: "sess_test".to_string(),
            creator_id: "ctr_test".to_string(),
            keywords: "[\"keyword1\", \"keyword2\"]".to_string(),
            summary: "Test fragment summary".to_string(),
            created_at: "2026-04-14T10:00:00Z".to_string(),
            ttl: Some("30d".to_string()),
        }
    }

    #[tokio::test]
    async fn test_create_and_list_fragment() {
        let (pool, _dir) = fresh_pool().await;
        let fragment = sample_fragment("frag_001");
        create_fragment(&pool, &fragment).await.unwrap();

        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fragment_id, "frag_001");
    }

    #[tokio::test]
    async fn test_list_fragments_by_session() {
        let (pool, _dir) = fresh_pool().await;

        let fragment1 = sample_fragment("frag_001");
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            session_id: "sess_other".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&pool, &fragment1).await.unwrap();
        create_fragment(&pool, &fragment2).await.unwrap();

        let list = list_fragments_by_session(&pool, "sess_test").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fragment_id, "frag_001");
    }

    #[tokio::test]
    async fn test_delete_fragment() {
        let (pool, _dir) = fresh_pool().await;
        let fragment = sample_fragment("frag_001");
        create_fragment(&pool, &fragment).await.unwrap();

        assert!(delete_fragment(&pool, "frag_001").await.unwrap());
        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_false() {
        let (pool, _dir) = fresh_pool().await;
        assert!(!delete_fragment(&pool, "frag_ghost").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_all_keywords_dedupes() {
        let (pool, _dir) = fresh_pool().await;

        let fragment1 = MemoryFragmentRecord {
            fragment_id: "frag_001".to_string(),
            keywords: "[\"alpha\", \"beta\"]".to_string(),
            ..sample_fragment("frag_001")
        };
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            keywords: "[\"beta\", \"gamma\"]".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&pool, &fragment1).await.unwrap();
        create_fragment(&pool, &fragment2).await.unwrap();

        let keywords = get_all_keywords(&pool, "ctr_test").await.unwrap();
        assert_eq!(keywords.len(), 3);
        assert!(keywords.contains(&"alpha".to_string()));
        assert!(keywords.contains(&"beta".to_string()));
        assert!(keywords.contains(&"gamma".to_string()));
    }

    #[tokio::test]
    async fn test_get_all_keywords_empty_creator() {
        let (pool, _dir) = fresh_pool().await;
        let keywords = get_all_keywords(&pool, "ctr_ghost").await.unwrap();
        assert!(keywords.is_empty());
    }

    #[tokio::test]
    async fn test_fragment_with_null_ttl() {
        let (pool, _dir) = fresh_pool().await;
        let fragment = MemoryFragmentRecord {
            fragment_id: "frag_null".to_string(),
            ttl: None,
            ..sample_fragment("frag_null")
        };
        create_fragment(&pool, &fragment).await.unwrap();

        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert!(list[0].ttl.is_none());
    }

    #[tokio::test]
    async fn test_fragment_ordering_by_created_at() {
        let (pool, _dir) = fresh_pool().await;

        let fragment1 = sample_fragment("frag_001");
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            created_at: "2026-04-14T12:00:00Z".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&pool, &fragment1).await.unwrap();
        create_fragment(&pool, &fragment2).await.unwrap();

        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert_eq!(list[0].fragment_id, "frag_002"); // Later timestamp first
        assert_eq!(list[1].fragment_id, "frag_001");
    }

    #[tokio::test]
    async fn test_get_all_keywords_handles_invalid_json() {
        let (pool, _dir) = fresh_pool().await;

        // Insert with valid JSON
        let fragment1 = sample_fragment("frag_001");
        create_fragment(&pool, &fragment1).await.unwrap();

        // Insert with invalid JSON (should be ignored gracefully)
        // SAFETY: test-only dynamic data injection for invalid JSON edge case.
        sqlx::query(
            "INSERT INTO memory_fragments (fragment_id, session_id, creator_id, keywords)
             VALUES ('frag_bad', 'sess_test', 'ctr_test', 'not valid json')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let keywords = get_all_keywords(&pool, "ctr_test").await.unwrap();
        // Should still return keywords from valid fragment
        assert!(keywords.contains(&"keyword1".to_string()));
        assert!(keywords.contains(&"keyword2".to_string()));
    }
}
