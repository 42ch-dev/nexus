//! Memory fragment CRUD operations for review pipeline.
//!
//! Manages lightweight keyword-indexed fragments from review decisions.
//! See creator-memory-soul-lifecycle.md §7.2.

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
    /// World context (nullable provenance tag; no FK).
    pub world_id: Option<String>,
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
        "INSERT INTO memory_fragments (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        fragment.fragment_id,
        fragment.session_id,
        fragment.creator_id,
        fragment.keywords,
        fragment.summary,
        fragment.created_at,
        fragment.ttl,
        fragment.world_id
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
                summary as \"summary!\", created_at as \"created_at!\", ttl, world_id
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
            world_id: r.world_id,
        })
        .collect())
}

/// List a bounded page of fragments for a creator.
///
/// Same projection + ordering as [`list_fragments`] (`created_at DESC`) but
/// pushes `LIMIT` into SQL so the daemon never materializes the full set before
/// truncating. This is the bounded counterpart used by the `fragments` list
/// endpoint's no-keyword path (R-V178P0-QC3-002 / W-QC3-002). For total row
/// counts ≤ `limit` the returned set is identical to `list_fragments` followed
/// by an in-Rust `truncate(limit)`; the only difference is that the cap is now
/// enforced server-side.
///
/// `limit` is an `i64` (the `LIMIT` bind type) and is expected to be already
/// clamped to `1..=MAX_LIMIT` by the caller.
///
/// When `world_id` is `Some`, only fragments matching that world are returned;
/// `None` means no world filter (Creator SOUL whole).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_fragments_limited(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryFragmentRecord>, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    // Optional world_id produces two WHERE clause variants;
    // all values are parameterized with .bind() to prevent injection.
    let rows = if let Some(wid) = world_id {
        sqlx::query(
            "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
             FROM memory_fragments WHERE creator_id = ? AND world_id = ?
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(creator_id)
        .bind(wid)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
             FROM memory_fragments WHERE creator_id = ?
             ORDER BY created_at DESC LIMIT ?",
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
            world_id: row.get("world_id"),
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
                summary as \"summary!\", created_at as \"created_at!\", ttl, world_id
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
            world_id: r.world_id,
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

/// List fragments for a creator with optional keyword and world filters and limit.
///
/// When `keyword` is `Some`, only fragments whose `keywords` JSON array contains
/// the given keyword (case-insensitive `LIKE` match) are returned.
/// When `world_id` is `Some`, only fragments matching that world are returned;
/// `None` means no world filter (Creator SOUL whole).
/// Results are ordered by `created_at` descending, limited to `limit` rows.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_fragments_filtered(
    pool: &SqlitePool,
    creator_id: &str,
    keyword: Option<&str>,
    world_id: Option<&str>,
    limit: u32,
) -> Result<Vec<MemoryFragmentRecord>, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    // Optional keyword + optional world_id filters produce 4 WHERE clause variants;
    // all values are parameterized with .bind() to prevent injection.
    // R-V133P4-04: removed `!` suffix from column aliases — those are compile-time
    // sqlx markers and cause runtime lookup failures with runtime sqlx::query().
    let rows = match (keyword, world_id) {
        (Some(kw), Some(wid)) => {
            let pattern = format!("%\"{kw}\"%");
            sqlx::query(
                "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
                 FROM memory_fragments
                 WHERE creator_id = ? AND keywords LIKE ? AND world_id = ?
                 ORDER BY created_at DESC
                 LIMIT ?",
            )
            .bind(creator_id)
            .bind(&pattern)
            .bind(wid)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(kw), None) => {
            let pattern = format!("%\"{kw}\"%");
            sqlx::query(
                "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
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
        }
        (None, Some(wid)) => {
            sqlx::query(
                "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
                 FROM memory_fragments
                 WHERE creator_id = ? AND world_id = ?
                 ORDER BY created_at DESC
                 LIMIT ?",
            )
            .bind(creator_id)
            .bind(wid)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, None) => {
            sqlx::query(
                "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id
                 FROM memory_fragments
                 WHERE creator_id = ?
                 ORDER BY created_at DESC
                 LIMIT ?",
            )
            .bind(creator_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
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
            world_id: row.get("world_id"),
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
            world_id: None,
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

    #[tokio::test]
    async fn test_world_id_propagation() {
        let (pool, _dir) = fresh_pool().await;
        let fragment = MemoryFragmentRecord {
            world_id: Some("world_alpha".to_string()),
            ..sample_fragment("frag_world")
        };
        create_fragment(&pool, &fragment).await.unwrap();

        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fragment_id, "frag_world");
        assert_eq!(list[0].world_id.as_deref(), Some("world_alpha"));
    }

    #[tokio::test]
    async fn test_world_id_null_by_default() {
        let (pool, _dir) = fresh_pool().await;
        let fragment = sample_fragment("frag_null_world");
        create_fragment(&pool, &fragment).await.unwrap();

        let list = list_fragments(&pool, "ctr_test").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].world_id, None);
    }

    #[tokio::test]
    async fn test_list_fragments_filtered_world_filter() {
        let (pool, _dir) = fresh_pool().await;

        let frag_world_a = MemoryFragmentRecord {
            fragment_id: "frag_a".to_string(),
            world_id: Some("world_a".to_string()),
            ..sample_fragment("frag_a")
        };
        let frag_world_b = MemoryFragmentRecord {
            fragment_id: "frag_b".to_string(),
            world_id: Some("world_b".to_string()),
            ..sample_fragment("frag_b")
        };
        let frag_null_world = MemoryFragmentRecord {
            fragment_id: "frag_null".to_string(),
            world_id: None,
            ..sample_fragment("frag_null")
        };

        create_fragment(&pool, &frag_world_a).await.unwrap();
        create_fragment(&pool, &frag_world_b).await.unwrap();
        create_fragment(&pool, &frag_null_world).await.unwrap();

        // Filter by world_a — should return exactly 1
        let filtered = list_fragments_filtered(&pool, "ctr_test", None, Some("world_a"), 10)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].fragment_id, "frag_a");

        // Filter by world_b
        let filtered_b = list_fragments_filtered(&pool, "ctr_test", None, Some("world_b"), 10)
            .await
            .unwrap();
        assert_eq!(filtered_b.len(), 1);
        assert_eq!(filtered_b[0].fragment_id, "frag_b");

        // No world filter → all 3
        let all = list_fragments_filtered(&pool, "ctr_test", None, None, 10)
            .await
            .unwrap();
        assert_eq!(all.len(), 3);

        // keyword + world filter combination
        let both =
            list_fragments_filtered(&pool, "ctr_test", Some("keyword1"), Some("world_a"), 10)
                .await
                .unwrap();
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].fragment_id, "frag_a");
    }
}
