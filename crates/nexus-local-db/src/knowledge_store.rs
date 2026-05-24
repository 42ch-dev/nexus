//! SQLite-backed `KnowledgeStore` implementation.
//!
//! Implements the `KnowledgeStore` trait from `nexus-knowledge` using the
//! workspace `state.db` pool. Uses runtime `sqlx::query()` with SAFETY
//! comments (matching the `narrative_write.rs` pattern) to avoid
//! requiring `cargo sqlx prepare` for the new `knowledge_entries` table.
//!
//! # User-scope isolation
//!
//! Every SQL statement includes `WHERE user_id = ?` to enforce strict
//! per-user scoping. No query can return or modify entries belonging
//! to a different user.

use nexus_knowledge::errors::KnowledgeError;
use nexus_knowledge::knowledge::{KnowledgeEntry, KnowledgeQuery, KnowledgeResult, KnowledgeTag};
use nexus_knowledge::store::KnowledgeStore;
use sqlx::SqlitePool;
use std::sync::Arc;

/// SQLite-backed knowledge store for User-scoped entries.
///
/// Thread-safe via `Arc<SqlitePool>`. All operations enforce `user_id`
/// scoping at the SQL level.
pub struct SqliteKnowledgeStore {
    pool: Arc<SqlitePool>,
}

impl SqliteKnowledgeStore {
    /// Create a new store backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Serialize tags to JSON string for storage.
    fn tags_to_json(tags: &[KnowledgeTag]) -> String {
        let tag_strs: Vec<&str> = tags.iter().map(KnowledgeTag::as_str).collect();
        serde_json::to_string(&tag_strs).unwrap_or_else(|_| "[]".to_string())
    }

    /// Deserialize tags from JSON string.
    fn json_to_tags(json: &str) -> Vec<KnowledgeTag> {
        let tag_strs: Vec<String> = serde_json::from_str(json).unwrap_or_default();
        tag_strs
            .into_iter()
            .map(|s| KnowledgeTag::new(&s))
            .collect()
    }

    /// Convert a database row to a `KnowledgeEntry`.
    fn row_to_entry(
        entry_id: String,
        user_id: String,
        tags_json: &str,
        content: String,
        reference_uri: Option<String>,
        created_at: String,
        updated_at: String,
    ) -> KnowledgeEntry {
        KnowledgeEntry {
            id: entry_id,
            user_id,
            tags: Self::json_to_tags(tags_json),
            content,
            reference_uri,
            created_at,
            updated_at,
        }
    }
}

#[async_trait::async_trait]
impl KnowledgeStore for SqliteKnowledgeStore {
    async fn store(&self, entry: KnowledgeEntry) -> Result<KnowledgeEntry, KnowledgeError> {
        if entry.user_id.trim().is_empty() {
            return Err(KnowledgeError::ValidationError(
                "user_id must not be empty".to_string(),
            ));
        }
        if entry.content.trim().is_empty() {
            return Err(KnowledgeError::ValidationError(
                "content must not be empty".to_string(),
            ));
        }

        let tags_json = Self::tags_to_json(&entry.tags);

        // SAFETY: INSERT matches knowledge_entries DDL in 20260526_knowledge_entries.sql
        sqlx::query(
            "INSERT INTO knowledge_entries \
                (entry_id, user_id, tags_json, content, reference_uri, created_at, updated_at) \
               VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.id)
        .bind(&entry.user_id)
        .bind(&tags_json)
        .bind(&entry.content)
        .bind(&entry.reference_uri)
        .bind(&entry.created_at)
        .bind(&entry.updated_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?;

        Ok(entry)
    }

    async fn get(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError> {
        // SAFETY: SELECT against knowledge_entries table with user_id scope
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
            ),
        >(
            "SELECT entry_id, user_id, tags_json, content, reference_uri, created_at, updated_at \
              FROM knowledge_entries \
             WHERE user_id = ? AND entry_id = ?",
        )
        .bind(user_id)
        .bind(entry_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?;

        Ok(row.map(|(id, uid, tj, c, ru, ca, ua)| Self::row_to_entry(id, uid, &tj, c, ru, ca, ua)))
    }

    async fn list(&self, query: &KnowledgeQuery) -> Result<KnowledgeResult, KnowledgeError> {
        let limit = query.effective_limit();
        let offset = query.effective_offset();

        // Build WHERE clause dynamically for optional filters
        let mut count_sql =
            String::from("SELECT COUNT(*) FROM knowledge_entries WHERE user_id = ?");
        let mut select_sql = String::from(
            "SELECT entry_id, user_id, tags_json, content, reference_uri, created_at, updated_at \
              FROM knowledge_entries WHERE user_id = ?",
        );

        // Text filter
        if query.text.is_some() {
            count_sql.push_str(" AND content LIKE ?");
            select_sql.push_str(" AND content LIKE ?");
        }

        // Tag filter — require all tags (JSON array contains check)
        // Simple approach: check each tag via LIKE on tags_json
        if let Some(ref tags) = query.tags {
            for _tag in tags {
                count_sql.push_str(" AND tags_json LIKE ?");
                select_sql.push_str(" AND tags_json LIKE ?");
            }
        }

        // Order and pagination
        select_sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        // Build and execute count query
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        count_query = count_query.bind(&query.user_id);
        if let Some(ref text) = query.text {
            count_query = count_query.bind(format!("%{text}%"));
        }
        if let Some(ref tags) = query.tags {
            for tag in tags {
                let pattern = format!("\"{}\"", tag.as_str().replace('"', "\\\""));
                count_query = count_query.bind(format!("%{pattern}%"));
            }
        }
        let total_count: u32 = count_query
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?
            .try_into()
            .unwrap_or(0);

        // Build and execute select query
        let mut select_query = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
            ),
        >(&select_sql);
        select_query = select_query.bind(&query.user_id);
        if let Some(ref text) = query.text {
            select_query = select_query.bind(format!("%{text}%"));
        }
        if let Some(ref tags) = query.tags {
            for tag in tags {
                let pattern = format!("\"{}\"", tag.as_str().replace('"', "\\\""));
                select_query = select_query.bind(format!("%{pattern}%"));
            }
        }
        select_query = select_query.bind(limit);
        select_query = select_query.bind(offset);

        let rows = select_query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?;

        let entries: Vec<KnowledgeEntry> = rows
            .into_iter()
            .map(|(id, uid, tj, c, ru, ca, ua)| Self::row_to_entry(id, uid, &tj, c, ru, ca, ua))
            .collect();

        Ok(KnowledgeResult::new(entries, total_count, limit, offset))
    }

    async fn search(
        &self,
        user_id: &str,
        text: &str,
        tags: Option<&[KnowledgeTag]>,
        limit: u32,
        offset: u32,
    ) -> Result<KnowledgeResult, KnowledgeError> {
        let mut query = KnowledgeQuery::for_user(user_id)
            .with_text(text)
            .with_limit(limit)
            .with_offset(offset);
        if let Some(t) = tags {
            query = query.with_tags(t.to_vec());
        }
        self.list(&query).await
    }

    async fn delete(&self, user_id: &str, entry_id: &str) -> Result<bool, KnowledgeError> {
        // SAFETY: DELETE against knowledge_entries with user_id scope
        let result =
            sqlx::query("DELETE FROM knowledge_entries WHERE user_id = ? AND entry_id = ?")
                .bind(user_id)
                .bind(entry_id)
                .execute(&*self.pool)
                .await
                .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_tags(
        &self,
        user_id: &str,
        entry_id: &str,
        new_tags: Vec<KnowledgeTag>,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError> {
        let tags_json = Self::tags_to_json(&new_tags);
        let now = chrono::Utc::now().to_rfc3339();

        // SAFETY: UPDATE against knowledge_entries with user_id scope
        let result = sqlx::query(
            "UPDATE knowledge_entries SET tags_json = ?, updated_at = ? \
             WHERE user_id = ? AND entry_id = ?",
        )
        .bind(&tags_json)
        .bind(&now)
        .bind(user_id)
        .bind(entry_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| KnowledgeError::ValidationError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        // Fetch the updated entry
        self.get(user_id, entry_id).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seeded_store() -> (SqliteKnowledgeStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        let store = SqliteKnowledgeStore::new(pool);

        let entries = vec![
            KnowledgeEntry::new(
                "user_1",
                vec![KnowledgeTag::new("rust"), KnowledgeTag::new("tutorial")],
                "Rust ownership and borrowing",
            ),
            KnowledgeEntry::new(
                "user_1",
                vec![KnowledgeTag::new("rust"), KnowledgeTag::new("async")],
                "Tokio async runtime overview",
            ),
            KnowledgeEntry::new(
                "user_1",
                vec![KnowledgeTag::new("design")],
                "System design patterns for microservices",
            ),
            KnowledgeEntry::new(
                "user_2",
                vec![KnowledgeTag::new("rust")],
                "Another user's Rust notes",
            ),
        ];
        for entry in entries {
            store.store(entry).await.unwrap();
        }
        (store, dir)
    }

    #[tokio::test]
    async fn store_and_get() {
        let (pool, _dir) = fresh_pool().await;
        let store = SqliteKnowledgeStore::new(pool);
        let entry = KnowledgeEntry::new("user_1", vec![KnowledgeTag::new("test")], "Test content");
        let id = entry.id.clone();
        let stored = store.store(entry).await.unwrap();
        assert_eq!(stored.id, id);

        let retrieved = store.get("user_1", &id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "Test content");
    }

    #[tokio::test]
    async fn get_wrong_user_returns_none() {
        let (store, _dir) = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let first_id = entries[0].id.clone();

        let result = store.get("user_2", &first_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_all_for_user() {
        let (store, _dir) = seeded_store().await;
        let result = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap();
        assert_eq!(result.total_count, 3);
        assert_eq!(result.entries.len(), 3);
    }

    #[tokio::test]
    async fn list_filter_by_tags() {
        let (store, _dir) = seeded_store().await;
        let result = store
            .list(&KnowledgeQuery::for_user("user_1").with_tags(vec![KnowledgeTag::new("rust")]))
            .await
            .unwrap();
        assert_eq!(result.total_count, 2);
        for entry in &result.entries {
            assert!(entry.tags.contains(&KnowledgeTag::new("rust")));
        }
    }

    #[tokio::test]
    async fn search_by_text() {
        let (store, _dir) = seeded_store().await;
        let result = store
            .search("user_1", "ownership", None, 50, 0)
            .await
            .unwrap();
        assert_eq!(result.total_count, 1);
        assert!(result.entries[0].content.contains("ownership"));
    }

    #[tokio::test]
    async fn delete_entry() {
        let (store, _dir) = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let id = entries[0].id.clone();

        let deleted = store.delete("user_1", &id).await.unwrap();
        assert!(deleted);

        let retrieved = store.get("user_1", &id).await.unwrap();
        assert!(retrieved.is_none());

        let result = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap();
        assert_eq!(result.total_count, 2);
    }

    #[tokio::test]
    async fn delete_wrong_user_returns_false() {
        let (store, _dir) = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let id = entries[0].id.clone();

        let deleted = store.delete("user_2", &id).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn update_tags() {
        let (store, _dir) = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let id = entries[0].id.clone();

        let updated = store
            .update_tags(
                "user_1",
                &id,
                vec![KnowledgeTag::new("updated"), KnowledgeTag::new("tag")],
            )
            .await
            .unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.tags.len(), 2);
        assert!(updated.tags.contains(&KnowledgeTag::new("updated")));

        let retrieved = store.get("user_1", &id).await.unwrap().unwrap();
        assert_eq!(retrieved.tags, updated.tags);
    }

    #[tokio::test]
    async fn update_tags_wrong_user_returns_none() {
        let (store, _dir) = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let id = entries[0].id.clone();

        let result = store
            .update_tags("user_2", &id, vec![KnowledgeTag::new("hacked")])
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn store_validates_user_id() {
        let (pool, _dir) = fresh_pool().await;
        let store = SqliteKnowledgeStore::new(pool);
        let mut entry = KnowledgeEntry::new("", vec![], "content");
        entry.user_id = String::new();
        let result = store.store(entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn store_validates_content() {
        let (pool, _dir) = fresh_pool().await;
        let store = SqliteKnowledgeStore::new(pool);
        let mut entry = KnowledgeEntry::new("user_1", vec![], "");
        entry.content = String::new();
        let result = store.store(entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn user_isolation() {
        let (store, _dir) = seeded_store().await;
        let u1 = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap();
        let u2 = store
            .list(&KnowledgeQuery::for_user("user_2"))
            .await
            .unwrap();
        assert_eq!(u1.total_count, 3);
        assert_eq!(u2.total_count, 1);

        // user_2 search should NOT return user_1's entries
        let u2_rust = store
            .search("user_2", "ownership", None, 50, 0)
            .await
            .unwrap();
        assert_eq!(u2_rust.total_count, 0);
    }

    #[tokio::test]
    async fn persistence_across_connections() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool1 = open_pool(&db_path).await.unwrap();
        run_migrations(&pool1).await.unwrap();

        let store1 = SqliteKnowledgeStore::new(pool1);
        let entry = KnowledgeEntry::new(
            "user_1",
            vec![KnowledgeTag::new("persist")],
            "Persistent data",
        );
        let id = entry.id.clone();
        store1.store(entry).await.unwrap();

        // Drop first pool, open a new one
        drop(store1);
        let pool2 = open_pool(&db_path).await.unwrap();
        let store2 = SqliteKnowledgeStore::new(pool2);

        let retrieved = store2.get("user_1", &id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "Persistent data");
    }
}
