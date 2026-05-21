//! Knowledge storage trait and default in-memory implementation.
//!
//! The `KnowledgeStore` trait abstracts persistence for User-scoped knowledge entries.
//! Implementations may use `SQLite` (via `nexus-local-db`), file-backed storage, or
//! the provided `InMemoryKnowledgeStore` for testing and prototyping.
//!
//! # User-scope invariant
//!
//! Every method requires or implies a `user_id` — knowledge entries are strictly
//! scoped to a single User and must not leak across user boundaries.

use async_trait::async_trait;

use crate::errors::KnowledgeError;
use crate::knowledge::{KnowledgeEntry, KnowledgeQuery, KnowledgeResult, KnowledgeTag};

/// Storage trait for User-scoped knowledge entries.
///
/// All operations are async to support I/O-backed implementations (`SQLite`, file, network).
/// The trait is object-safe (`Send + Sync`) for use behind `Arc<dyn KnowledgeStore>`.
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// Store a new knowledge entry. Returns the stored entry with generated ID and timestamps.
    ///
    /// # Errors
    ///
    /// Returns `KnowledgeError` if validation fails or storage encounters an error.
    async fn store(&self, entry: KnowledgeEntry) -> Result<KnowledgeEntry, KnowledgeError>;

    /// Retrieve a single knowledge entry by ID, scoped to `user_id`.
    ///
    /// Returns `None` if the entry does not exist or belongs to a different user.
    async fn get(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError>;

    /// List knowledge entries for a user, with optional tag filtering and pagination.
    ///
    /// Returns a paginated result. Only entries belonging to `user_id` are included.
    async fn list(&self, query: &KnowledgeQuery) -> Result<KnowledgeResult, KnowledgeError>;

    /// Search knowledge entries by text content (substring, case-insensitive),
    /// scoped to `user_id` and optionally filtered by tags.
    ///
    /// This is a convenience method equivalent to `list` with a text filter.
    async fn search(
        &self,
        user_id: &str,
        text: &str,
        tags: Option<&[KnowledgeTag]>,
        limit: u32,
        offset: u32,
    ) -> Result<KnowledgeResult, KnowledgeError>;

    /// Delete a knowledge entry by ID, scoped to `user_id`.
    ///
    /// Returns `true` if the entry was found and deleted, `false` if not found.
    async fn delete(&self, user_id: &str, entry_id: &str) -> Result<bool, KnowledgeError>;

    /// Replace the tags on an existing knowledge entry.
    ///
    /// Returns the updated entry, or `None` if not found / wrong user.
    async fn update_tags(
        &self,
        user_id: &str,
        entry_id: &str,
        new_tags: Vec<KnowledgeTag>,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError>;
}

// ── In-memory implementation ─────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory knowledge store for testing and prototyping.
///
/// Thread-safe via `tokio::sync::RwLock`. Entries are indexed by `(user_id, entry_id)`.
/// Suitable for unit tests, integration tests, and single-process prototyping.
/// Not suitable for production persistence (data is lost on process exit).
pub struct InMemoryKnowledgeStore {
    entries: Arc<RwLock<HashMap<String, KnowledgeEntry>>>,
}

impl InMemoryKnowledgeStore {
    /// Create an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate a composite key for the internal map.
    fn key(user_id: &str, entry_id: &str) -> String {
        format!("{user_id}::{entry_id}")
    }
}

impl Default for InMemoryKnowledgeStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KnowledgeStore for InMemoryKnowledgeStore {
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
        let key = Self::key(&entry.user_id, &entry.id);
        let stored = entry.clone();
        self.entries.write().await.insert(key, stored);
        Ok(entry)
    }

    async fn get(
        &self,
        user_id: &str,
        entry_id: &str,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError> {
        let key = Self::key(user_id, entry_id);
        let map = self.entries.read().await;
        Ok(map.get(&key).cloned())
    }

    async fn list(&self, query: &KnowledgeQuery) -> Result<KnowledgeResult, KnowledgeError> {
        let limit = query.effective_limit();
        let offset = query.effective_offset();

        // Scope the read guard to release before creating the result
        let matched: Vec<KnowledgeEntry> = {
            let map = self.entries.read().await;
            map.values()
                .filter(|e| e.user_id == query.user_id)
                .filter(|e| {
                    query
                        .tags
                        .as_ref()
                        .is_none_or(|required| e.has_all_tags(required))
                })
                .filter(|e| query.text.as_ref().is_none_or(|t| e.content_contains(t)))
                .cloned()
                .collect()
        };

        let mut matched = matched;
        // Stable ordering: sort by created_at descending (newest first)
        matched.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total_count: u32 = matched.len().try_into().unwrap_or(u32::MAX);
        let paginated: Vec<KnowledgeEntry> = matched
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(KnowledgeResult::new(paginated, total_count, limit, offset))
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
        let key = Self::key(user_id, entry_id);
        let mut map = self.entries.write().await;
        Ok(map.remove(&key).is_some())
    }

    async fn update_tags(
        &self,
        user_id: &str,
        entry_id: &str,
        new_tags: Vec<KnowledgeTag>,
    ) -> Result<Option<KnowledgeEntry>, KnowledgeError> {
        let key = Self::key(user_id, entry_id);
        let mut map = self.entries.write().await;
        if let Some(entry) = map.get_mut(&key) {
            entry.tags = new_tags;
            entry.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(Some(entry.clone()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a store with a few seeded entries.
    async fn seeded_store() -> InMemoryKnowledgeStore {
        let store = InMemoryKnowledgeStore::new();
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
        store
    }

    #[tokio::test]
    async fn store_and_get() {
        let store = InMemoryKnowledgeStore::new();
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
        let store = seeded_store().await;
        let entries = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap()
            .entries;
        let first_id = entries[0].id.clone();

        // Different user should not see user_1's entries
        let result = store.get("user_2", &first_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_all_for_user() {
        let store = seeded_store().await;
        let result = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap();
        assert_eq!(result.total_count, 3);
        assert_eq!(result.entries.len(), 3);
    }

    #[tokio::test]
    async fn list_filter_by_tags() {
        let store = seeded_store().await;
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
    async fn list_filter_by_multiple_tags() {
        let store = seeded_store().await;
        let result = store
            .list(
                &KnowledgeQuery::for_user("user_1")
                    .with_tags(vec![KnowledgeTag::new("rust"), KnowledgeTag::new("async")]),
            )
            .await
            .unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].content, "Tokio async runtime overview");
    }

    #[tokio::test]
    async fn search_by_text() {
        let store = seeded_store().await;
        let result = store
            .search("user_1", "ownership", None, 50, 0)
            .await
            .unwrap();
        assert_eq!(result.total_count, 1);
        assert!(result.entries[0].content.contains("ownership"));
    }

    #[tokio::test]
    async fn search_by_text_and_tags() {
        let store = seeded_store().await;
        let result = store
            .search(
                "user_1",
                "rust",
                Some(&[KnowledgeTag::new("design")]),
                50,
                0,
            )
            .await
            .unwrap();
        assert_eq!(result.total_count, 0);
    }

    #[tokio::test]
    async fn pagination() {
        let store = seeded_store().await;
        let page1 = store
            .list(
                &KnowledgeQuery::for_user("user_1")
                    .with_limit(2)
                    .with_offset(0),
            )
            .await
            .unwrap();
        assert_eq!(page1.entries.len(), 2);
        assert_eq!(page1.total_count, 3);
        assert!(page1.has_more());

        let page2 = store
            .list(
                &KnowledgeQuery::for_user("user_1")
                    .with_limit(2)
                    .with_offset(2),
            )
            .await
            .unwrap();
        assert_eq!(page2.entries.len(), 1);
        assert!(!page2.has_more());
    }

    #[tokio::test]
    async fn delete_entry() {
        let store = seeded_store().await;
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

        // Total should be one less
        let result = store
            .list(&KnowledgeQuery::for_user("user_1"))
            .await
            .unwrap();
        assert_eq!(result.total_count, 2);
    }

    #[tokio::test]
    async fn delete_wrong_user_returns_false() {
        let store = seeded_store().await;
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
        let store = seeded_store().await;
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

        // Verify persisted
        let retrieved = store.get("user_1", &id).await.unwrap().unwrap();
        assert_eq!(retrieved.tags, updated.tags);
    }

    #[tokio::test]
    async fn update_tags_wrong_user_returns_none() {
        let store = seeded_store().await;
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
        let store = InMemoryKnowledgeStore::new();
        let mut entry = KnowledgeEntry::new("", vec![], "content");
        entry.user_id = String::new();
        let result = store.store(entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn store_validates_content() {
        let store = InMemoryKnowledgeStore::new();
        let mut entry = KnowledgeEntry::new("user_1", vec![], "");
        entry.content = String::new();
        let result = store.store(entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn user_isolation() {
        let store = seeded_store().await;
        // user_1 has 3 entries, user_2 has 1
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

        // user_2's search for "rust" should NOT return user_1's entries
        let u2_rust = store
            .search("user_2", "ownership", None, 50, 0)
            .await
            .unwrap();
        assert_eq!(u2_rust.total_count, 0);
    }
}
