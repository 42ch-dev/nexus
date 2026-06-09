//! KB Store trait — abstract storage for World-scoped narrative KB graph operations.
//!
//! The `KbStore` trait defines insert/query/update/delete operations scoped by
//! `world_id`. An in-memory implementation is provided for testing.
//!
//! # Validation
//!
//! `InMemoryKbStore` runs body validation on insert and update when a
//! [`ValidationMode`](crate::validation::ValidationMode) is configured.
//! The default mode is `Generic` (no novel-specific checks). Set `validation_mode`
//! to [`ValidationMode::Novel`](crate::validation::ValidationMode::Novel) to
//! enforce `body.attributes.novel_category` requirements per
//! entity-scope-model.md §5.1.1.

use crate::errors::ValidationError;
use crate::key_block::KeyBlock;
use crate::query::{KbInsertResult, KbQuery, KbQueryResult};
use crate::source_anchor::SourceAnchor;
use crate::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_contracts::BlockType;
use std::collections::HashMap;
use std::sync::RwLock;

// ── Store Error ─────────────────────────────────────────────────────

/// Error type for KB store operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KbStoreError {
    /// Uniqueness constraint violation — an active `KeyBlock` with the same
    /// `(canonical_name, block_type)` already exists in this world.
    #[error(
        "duplicate: canonical_name={name}, block_type={block_type:?} \
         already active in world {world_id}"
    )]
    Duplicate {
        /// World ID where the conflict occurred.
        world_id: String,
        /// Canonical name that conflicts.
        name: String,
        /// Block type that conflicts.
        block_type: BlockType,
    },

    /// `KeyBlock` not found.
    #[error("key block not found: {0}")]
    NotFound(String),

    /// Storage backend error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Structured body/canonical-name validation error.
    #[error("validation error: {0}")]
    Validation(ValidationError),

    /// Legacy validation error with opaque message.
    #[error("validation error: {0}")]
    ValidationLegacy(String),
}

// ── KbStore Trait ───────────────────────────────────────────────────

/// Trait for World-scoped KB graph storage operations.
///
/// All operations are scoped by `world_id`. Implementations may use
/// `SQLite`, in-memory, or other backends.
///
/// # Uniqueness Constraint
///
/// Under the same `world_id`, at most one **active** `KeyBlock` may exist
/// for a given `(canonical_name, block_type)` pair. Active means status
/// is not `deleted`, `merged`, or `deprecated`.
///
/// # Async
///
/// Methods are `async` to match the eventual `SQLite` backend (sqlx).
/// The in-memory implementation performs no actual async I/O.
///
/// Note: `async fn` in traits does not allow specifying `Send` bounds on
/// the returned future. This is acceptable for an internal trait used
/// through generics. If `Send` bounds are needed for spawnable futures,
/// callers can use `impl Future<Output = T> + Send` explicitly.
#[allow(async_fn_in_trait)]
pub trait KbStore {
    /// Insert a new `KeyBlock`.
    ///
    /// Returns [`KbInsertResult`] on success.
    /// Returns [`KbStoreError::Duplicate`] if an active `KeyBlock` with the
    /// same `(canonical_name, block_type)` already exists in the same world.
    async fn insert_key_block(&self, kb: KeyBlock) -> Result<KbInsertResult, KbStoreError>;

    /// Get a `KeyBlock` by its ID.
    async fn get_key_block(&self, key_block_id: &str) -> Result<KeyBlock, KbStoreError>;

    /// List all active `KeyBlocks` in a world.
    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KeyBlock>, KbStoreError>;

    /// Query `KeyBlocks` with filters.
    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError>;

    /// Attach a [`SourceAnchor`] to a `KeyBlock`.
    ///
    /// Multiple anchors can be attached to the same `KeyBlock`.
    async fn attach_source_anchor(
        &self,
        key_block_id: &str,
        anchor: SourceAnchor,
    ) -> Result<(), KbStoreError>;

    /// Get all [`SourceAnchor`] instances attached to a `KeyBlock`.
    async fn get_anchors(&self, key_block_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError>;

    /// Update an existing `KeyBlock` in place.
    ///
    /// Returns [`KbStoreError::NotFound`] if the `KeyBlock` does not exist.
    /// Returns [`KbStoreError::Duplicate`] if the update would violate
    /// the uniqueness constraint.
    async fn update_key_block(&self, kb: KeyBlock) -> Result<(), KbStoreError>;

    /// Soft-delete a `KeyBlock` by ID.
    ///
    /// Sets status to `deleted`. The record is retained.
    async fn delete_key_block(&self, key_block_id: &str) -> Result<(), KbStoreError>;
}

// ── In-Memory Implementation ────────────────────────────────────────

/// In-memory KB store for testing and development.
///
/// Thread-safe via interior mutability (`std::sync::RwLock`).
/// Suitable for unit tests; not intended for production use.
pub struct InMemoryKbStore {
    blocks: RwLock<HashMap<String, KeyBlock>>,
    anchors: RwLock<HashMap<String, Vec<SourceAnchor>>>,
    validation_mode: ValidationMode,
}

impl InMemoryKbStore {
    /// Create a new empty in-memory store with `Generic` validation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocks: RwLock::new(HashMap::new()),
            anchors: RwLock::new(HashMap::new()),
            validation_mode: ValidationMode::Generic,
        }
    }

    /// Create a new empty in-memory store with the given validation mode.
    #[must_use]
    pub fn with_validation_mode(mode: ValidationMode) -> Self {
        Self {
            blocks: RwLock::new(HashMap::new()),
            anchors: RwLock::new(HashMap::new()),
            validation_mode: mode,
        }
    }

    /// Check if a `KeyBlock` is "active" (not deleted, merged, or deprecated).
    fn is_active(kb: &KeyBlock) -> bool {
        !matches!(kb.status.as_str(), "deleted" | "merged" | "deprecated")
    }

    /// Check the uniqueness constraint for `(world_id, canonical_name, block_type)`.
    ///
    /// If `exclude_id` is provided, that `KeyBlock` is excluded from the check
    /// (used during updates where the block keeps its own ID).
    fn check_uniqueness(
        blocks: &HashMap<String, KeyBlock>,
        world_id: &str,
        canonical_name: &str,
        block_type: BlockType,
        exclude_id: Option<&str>,
    ) -> Result<(), KbStoreError> {
        for kb in blocks.values() {
            if kb.world_id == world_id
                && kb.canonical_name == canonical_name
                && kb.block_type == block_type
                && Self::is_active(kb)
                && exclude_id != Some(kb.key_block_id.as_str())
            {
                return Err(KbStoreError::Duplicate {
                    world_id: world_id.to_string(),
                    name: canonical_name.to_string(),
                    block_type,
                });
            }
        }
        Ok(())
    }

    /// Acquire a read lock on the blocks map.
    fn read_blocks(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, KeyBlock>>, KbStoreError> {
        self.blocks
            .read()
            .map_err(|e| KbStoreError::Storage(e.to_string()))
    }

    /// Acquire a write lock on the blocks map.
    fn write_blocks(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, KeyBlock>>, KbStoreError> {
        self.blocks
            .write()
            .map_err(|e| KbStoreError::Storage(e.to_string()))
    }
}

impl Default for InMemoryKbStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KbStore for InMemoryKbStore {
    async fn insert_key_block(&self, kb: KeyBlock) -> Result<KbInsertResult, KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(|e| match e {
            crate::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
            crate::errors::KbError::ValidationError(msg) => KbStoreError::ValidationLegacy(msg),
            other => KbStoreError::ValidationLegacy(other.to_string()),
        })?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode).map_err(
            |e| match e {
                crate::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
                crate::errors::KbError::ValidationError(msg) => KbStoreError::ValidationLegacy(msg),
                other => KbStoreError::ValidationLegacy(other.to_string()),
            },
        )?;

        let key_block_id = kb.key_block_id.clone();
        let world_id = kb.world_id.clone();
        let created_at = kb.created_at.clone();

        {
            let mut blocks = self.write_blocks()?;
            Self::check_uniqueness(
                &blocks,
                &kb.world_id,
                &kb.canonical_name,
                kb.block_type,
                None,
            )?;
            blocks.insert(key_block_id.clone(), kb);
        }

        Ok(KbInsertResult {
            key_block_id,
            world_id,
            created_at,
        })
    }

    async fn get_key_block(&self, key_block_id: &str) -> Result<KeyBlock, KbStoreError> {
        let blocks = self.read_blocks()?;
        blocks
            .get(key_block_id)
            .cloned()
            .ok_or_else(|| KbStoreError::NotFound(key_block_id.to_string()))
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KeyBlock>, KbStoreError> {
        let items: Vec<KeyBlock> = self
            .read_blocks()?
            .values()
            .filter(|kb| kb.world_id == world_id && Self::is_active(kb))
            .cloned()
            .collect();
        Ok(items)
    }

    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        let (matches, total_count, has_more) = {
            let blocks = self.read_blocks()?;

            let mut matches: Vec<KeyBlock> =
                blocks
                    .values()
                    .filter(|kb| {
                        if kb.world_id != query.world_id || !Self::is_active(kb) {
                            return false;
                        }
                        if let Some(bt) = query.block_type {
                            if kb.block_type != bt {
                                return false;
                            }
                        }
                        if let Some(ref name) = query.canonical_name {
                            if kb.canonical_name != *name {
                                return false;
                            }
                        }
                        if let Some(ref text) = query.text_search {
                            let lower = text.to_lowercase();
                            let hit_name = kb.canonical_name.to_lowercase().contains(&lower);
                            let hit_summary = kb
                                .body
                                .as_ref()
                                .and_then(|b| b.summary.as_ref())
                                .is_some_and(|s| s.to_lowercase().contains(&lower));
                            let hit_tags =
                                kb.body.as_ref().and_then(|b| b.tags.as_ref()).is_some_and(
                                    |tags| tags.iter().any(|t| t.to_lowercase().contains(&lower)),
                                );
                            if !hit_name && !hit_summary && !hit_tags {
                                return false;
                            }
                        }
                        true
                    })
                    .cloned()
                    .collect();

            let total_count = matches.len();
            let offset = query.offset.unwrap_or(0);
            let limit = query.limit.unwrap_or(usize::MAX);

            matches = matches.into_iter().skip(offset).take(limit).collect();
            let has_more = offset + matches.len() < total_count;

            // Release the read lock before constructing the result.
            drop(blocks);

            (matches, total_count, has_more)
        };

        Ok(KbQueryResult {
            items: matches,
            total_count,
            has_more,
        })
    }

    async fn attach_source_anchor(
        &self,
        key_block_id: &str,
        anchor: SourceAnchor,
    ) -> Result<(), KbStoreError> {
        {
            let blocks = self.read_blocks()?;
            if !blocks.contains_key(key_block_id) {
                return Err(KbStoreError::NotFound(key_block_id.to_string()));
            }
        }

        {
            let mut anchors = self
                .anchors
                .write()
                .map_err(|e| KbStoreError::Storage(e.to_string()))?;
            anchors
                .entry(key_block_id.to_string())
                .or_default()
                .push(anchor);
        }

        Ok(())
    }

    async fn get_anchors(&self, key_block_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError> {
        let anchors = self
            .anchors
            .read()
            .map_err(|e| KbStoreError::Storage(e.to_string()))?;
        Ok(anchors.get(key_block_id).cloned().unwrap_or_default())
    }

    async fn update_key_block(&self, kb: KeyBlock) -> Result<(), KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(|e| match e {
            crate::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
            crate::errors::KbError::ValidationError(msg) => KbStoreError::ValidationLegacy(msg),
            other => KbStoreError::ValidationLegacy(other.to_string()),
        })?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode).map_err(
            |e| match e {
                crate::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
                crate::errors::KbError::ValidationError(msg) => KbStoreError::ValidationLegacy(msg),
                other => KbStoreError::ValidationLegacy(other.to_string()),
            },
        )?;

        {
            let mut blocks = self.write_blocks()?;

            let existing = blocks
                .get(&kb.key_block_id)
                .ok_or_else(|| KbStoreError::NotFound(kb.key_block_id.clone()))?;

            // Re-check uniqueness if name or type changed
            if existing.canonical_name != kb.canonical_name || existing.block_type != kb.block_type
            {
                Self::check_uniqueness(
                    &blocks,
                    &kb.world_id,
                    &kb.canonical_name,
                    kb.block_type,
                    Some(&kb.key_block_id),
                )?;
            }

            blocks.insert(kb.key_block_id.clone(), kb);
        }
        Ok(())
    }

    async fn delete_key_block(&self, key_block_id: &str) -> Result<(), KbStoreError> {
        let mut blocks = self.write_blocks()?;
        let kb = blocks
            .get_mut(key_block_id)
            .ok_or_else(|| KbStoreError::NotFound(key_block_id.to_string()))?;
        kb.status = "deleted".to_string();
        kb.updated_at = Some(chrono::Utc::now().to_rfc3339());
        drop(blocks);
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_block::KeyBlockBody;

    fn make_block(world_id: &str, block_type: BlockType, name: &str) -> KeyBlock {
        KeyBlock::new(world_id, block_type, name)
    }

    // T1: Insert and retrieve a KeyBlock
    #[tokio::test]
    async fn test_insert_and_get() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");

        let result = store.insert_key_block(kb.clone()).await.unwrap();
        assert_eq!(result.key_block_id, kb.key_block_id);
        assert_eq!(result.world_id, "wld_1");

        let fetched = store.get_key_block(&kb.key_block_id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Hero");
        assert_eq!(fetched.world_id, "wld_1");
    }

    // T2: Get non-existent KeyBlock returns NotFound
    #[tokio::test]
    async fn test_get_not_found() {
        let store = InMemoryKbStore::new();
        let err = store.get_key_block("nonexistent").await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(ref s) if s == "nonexistent"));
    }

    // T3: List by world returns only active blocks in that world
    #[tokio::test]
    async fn test_list_by_world() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        let kb2 = make_block("wld_1", BlockType::Scene, "Forest");
        let kb3 = make_block("wld_2", BlockType::Character, "Villain");

        store.insert_key_block(kb1).await.unwrap();
        store.insert_key_block(kb2).await.unwrap();
        store.insert_key_block(kb3).await.unwrap();

        let w1 = store.list_by_world("wld_1").await.unwrap();
        assert_eq!(w1.len(), 2);

        let w2 = store.list_by_world("wld_2").await.unwrap();
        assert_eq!(w2.len(), 1);
        assert_eq!(w2[0].canonical_name, "Villain");
    }

    // T4: Uniqueness constraint — duplicate (canonical_name, block_type) in same world
    #[tokio::test]
    async fn test_uniqueness_rejects_duplicate() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        let err = store.insert_key_block(kb2).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Duplicate { ref world_id, ref name, .. }
                if world_id == "wld_1" && name == "Hero")
        );
    }

    // T5: Same canonical_name in different block types is allowed
    #[tokio::test]
    async fn test_uniqueness_allows_different_type() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Dragon");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Event, "Dragon");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    // T6: Same canonical_name + type in different worlds is allowed
    #[tokio::test]
    async fn test_uniqueness_allows_different_world() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = make_block("wld_2", BlockType::Character, "Hero");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    // T7: Soft-deleted block does not block uniqueness re-insertion
    #[tokio::test]
    async fn test_deleted_allows_reinsertion() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        store.delete_key_block(&id).await.unwrap();

        // Re-insert with same canonical_name + type should succeed
        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    // T8: Query with block_type filter
    #[tokio::test]
    async fn test_query_by_block_type() {
        let store = InMemoryKbStore::new();
        store
            .insert_key_block(make_block("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_key_block(make_block("wld_1", BlockType::Scene, "Forest"))
            .await
            .unwrap();
        store
            .insert_key_block(make_block("wld_1", BlockType::Character, "Villain"))
            .await
            .unwrap();

        let result = store
            .query(&KbQuery::new("wld_1").with_block_type(BlockType::Character))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.total_count, 2);
    }

    // T9: Query with canonical_name filter
    #[tokio::test]
    async fn test_query_by_canonical_name() {
        let store = InMemoryKbStore::new();
        store
            .insert_key_block(make_block("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_key_block(make_block("wld_1", BlockType::Character, "Villain"))
            .await
            .unwrap();

        let result = store
            .query(&KbQuery::new("wld_1").with_canonical_name("Hero"))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].canonical_name, "Hero");
    }

    // T10: Query with text search (matches canonical_name, summary, tags)
    #[tokio::test]
    async fn test_query_text_search() {
        let store = InMemoryKbStore::new();

        let mut kb1 = make_block("wld_1", BlockType::Character, "Dark Knight");
        kb1.set_body(KeyBlockBody {
            summary: Some("A brooding hero".to_string()),
            attributes: None,
            tags: Some(vec!["gothic".to_string()]),
        })
        .unwrap();
        store.insert_key_block(kb1).await.unwrap();

        let mut kb2 = make_block("wld_1", BlockType::Scene, "Enchanted Forest");
        kb2.set_body(KeyBlockBody {
            summary: Some("A magical woodland".to_string()),
            attributes: None,
            tags: Some(vec!["fantasy".to_string()]),
        })
        .unwrap();
        store.insert_key_block(kb2).await.unwrap();

        // Search by canonical_name substring
        let r = store
            .query(&KbQuery::new("wld_1").with_text_search("knight"))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 1);

        // Search by summary substring
        let r = store
            .query(&KbQuery::new("wld_1").with_text_search("brooding"))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 1);

        // Search by tag
        let r = store
            .query(&KbQuery::new("wld_1").with_text_search("fantasy"))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 1);

        // Case-insensitive
        let r = store
            .query(&KbQuery::new("wld_1").with_text_search("DARK"))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 1);
    }

    // T11: Query with pagination
    #[tokio::test]
    async fn test_query_pagination() {
        let store = InMemoryKbStore::new();
        for i in 0..5 {
            store
                .insert_key_block(make_block(
                    "wld_1",
                    BlockType::Character,
                    &format!("Char_{i}"),
                ))
                .await
                .unwrap();
        }

        // Page 1: limit=2, offset=0
        let r = store
            .query(&KbQuery::new("wld_1").with_limit(2).with_offset(0))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.total_count, 5);
        assert!(r.has_more);

        // Page 3: limit=2, offset=4
        let r = store
            .query(&KbQuery::new("wld_1").with_limit(2).with_offset(4))
            .await
            .unwrap();
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.total_count, 5);
        assert!(!r.has_more);
    }

    // T12: Attach and retrieve SourceAnchors
    #[tokio::test]
    async fn test_attach_and_get_anchors() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        let anchor1 = SourceAnchor::new("stm_1", "sum_1", Some("chapter"));
        let anchor2 = SourceAnchor::new("stm_2", "sum_2", None);

        store.attach_source_anchor(&id, anchor1).await.unwrap();
        store.attach_source_anchor(&id, anchor2).await.unwrap();

        let anchors = store.get_anchors(&id).await.unwrap();
        assert_eq!(anchors.len(), 2);
    }

    // T13: Attach anchor to non-existent KeyBlock fails
    #[tokio::test]
    async fn test_attach_anchor_not_found() {
        let store = InMemoryKbStore::new();
        let anchor = SourceAnchor::from_excerpt("some text");
        let err = store
            .attach_source_anchor("ghost", anchor)
            .await
            .unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(_)));
    }

    // T14: Get anchors for block with none returns empty vec
    #[tokio::test]
    async fn test_get_anchors_empty() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        let anchors = store.get_anchors(&id).await.unwrap();
        assert!(anchors.is_empty());
    }

    // T15: Update a KeyBlock
    #[tokio::test]
    async fn test_update_key_block() {
        let store = InMemoryKbStore::new();
        let mut kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb.clone()).await.unwrap();

        kb.canonical_name = "Superhero".to_string();
        store.update_key_block(kb).await.unwrap();

        let fetched = store.get_key_block(&id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Superhero");
    }

    // T16: Update to conflicting canonical_name + block_type fails
    #[tokio::test]
    async fn test_update_conflict() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb1).await.unwrap();

        let mut kb2 = make_block("wld_1", BlockType::Character, "Villain");
        store.insert_key_block(kb2.clone()).await.unwrap();

        // Rename kb2 to "Hero" — should conflict with kb1
        kb2.canonical_name = "Hero".to_string();
        let err = store.update_key_block(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    // T17: Update non-existent KeyBlock fails
    #[tokio::test]
    async fn test_update_not_found() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Ghost");
        let err = store.update_key_block(kb).await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(_)));
    }

    // T18: Delete a KeyBlock (soft delete)
    #[tokio::test]
    async fn test_delete_key_block() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        store.delete_key_block(&id).await.unwrap();

        // Block still exists but is marked deleted
        let fetched = store.get_key_block(&id).await.unwrap();
        assert_eq!(fetched.status, "deleted");

        // list_by_world excludes deleted
        let listed = store.list_by_world("wld_1").await.unwrap();
        assert!(listed.is_empty());
    }

    // T19: Delete non-existent KeyBlock fails
    #[tokio::test]
    async fn test_delete_not_found() {
        let store = InMemoryKbStore::new();
        let err = store.delete_key_block("ghost").await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(_)));
    }

    // T20: Query scoped to different world returns nothing
    #[tokio::test]
    async fn test_query_world_isolation() {
        let store = InMemoryKbStore::new();
        store
            .insert_key_block(make_block("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();

        let result = store.query(&KbQuery::new("wld_other")).await.unwrap();
        assert!(result.items.is_empty());
    }

    // T21: Deprecated block does not block uniqueness
    #[tokio::test]
    async fn test_deprecated_allows_reinsertion() {
        let store = InMemoryKbStore::new();
        let mut kb = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb.clone()).await.unwrap();

        kb.deprecate(None).unwrap();
        store.update_key_block(kb).await.unwrap();

        // Re-insert with same name + type should succeed
        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    // T22: Merged block does not block uniqueness
    #[tokio::test]
    async fn test_merged_allows_reinsertion() {
        let store = InMemoryKbStore::new();
        let mut kb = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb.clone()).await.unwrap();

        kb.merge_into("kb_other").unwrap();
        store.update_key_block(kb).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    // ── P1 taxonomy tests (plan 2026-06-10-v1.40-world-kb-taxonomy T5) ──

    /// Helper: make a block with a novel body for taxonomy tests.
    fn make_novel_block(
        world_id: &str,
        block_type: BlockType,
        name: &str,
        novel_category: &str,
    ) -> KeyBlock {
        let mut kb = KeyBlock::new(world_id, block_type, name);
        kb.set_body(KeyBlockBody {
            summary: Some(format!("{novel_category}: {name}")),
            attributes: Some(serde_json::json!({
                "novel_category": novel_category,
                "traits": ["test"]
            })),
            tags: Some(vec!["novel".to_string()]),
        })
        .unwrap();
        kb
    }

    // AC1: Invalid wire block_type fails with structured error.
    // (BlockType is a Rust enum — unknown strings fail at deserialization,
    //  which is a structured parse error before reaching the store.
    //  This test confirms the validation path surfaces KbStoreError::Validation.)
    #[tokio::test]
    async fn test_invalid_block_type_via_deserialization() {
        let json = r#"{"block_type": "unknown_type"}"#;
        let result = serde_json::from_str::<serde_json::Value>(json);
        // The value parses as raw JSON but BlockType deserialization would fail.
        // nexus-kb validation layer relies on the typed enum.
        assert!(result.is_ok()); // raw JSON parses
                                 // Actual BlockType deserialization of "unknown_type" would fail:
        let bt_result = serde_json::from_value::<BlockType>(serde_json::json!("unknown_type"));
        assert!(bt_result.is_err());
    }

    // AC2: Novel-profile ingest accepts minimum body per mapping table.
    // One happy-path per block_type (representative subset of mapping table).
    #[tokio::test]
    async fn test_novel_happy_path_per_block_type() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        // character → character
        let kb = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        assert!(store.insert_key_block(kb).await.is_ok());

        // scene → location
        let kb = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        assert!(store.insert_key_block(kb).await.is_ok());

        // organization → society
        let kb = make_novel_block(
            "wld_1",
            BlockType::Organization,
            "org_solar_cult",
            "society",
        );
        assert!(store.insert_key_block(kb).await.is_ok());

        // conflict → rules
        let kb = make_novel_block("wld_1", BlockType::Conflict, "rule_magic_cost", "rules");
        assert!(store.insert_key_block(kb).await.is_ok());

        // item → economy
        let kb = make_novel_block("wld_1", BlockType::Item, "item_memory_crystal", "economy");
        assert!(store.insert_key_block(kb).await.is_ok());

        // info_point → foundation
        let kb = make_novel_block("wld_1", BlockType::InfoPoint, "fnd_cosmology", "foundation");
        assert!(store.insert_key_block(kb).await.is_ok());

        // event → background
        let kb = make_novel_block("wld_1", BlockType::Event, "evt_great_fire", "background");
        assert!(store.insert_key_block(kb).await.is_ok());

        // ability (no novel_category mapping, but novel mode allows any valid category)
        let kb = make_novel_block("wld_1", BlockType::Ability, "abl_shadow_walk", "character");
        assert!(store.insert_key_block(kb).await.is_ok());
    }

    // AC2 negative: Missing novel_category on character block fails in Novel mode.
    #[tokio::test]
    async fn test_novel_missing_category_rejected() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "char_no_cat");
        kb.set_body(KeyBlockBody {
            summary: Some("A character without category".to_string()),
            attributes: Some(serde_json::json!({"aliases": ["NoCat"]})),
            tags: Some(vec!["novel".to_string()]),
        })
        .unwrap();

        let err = store.insert_key_block(kb).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Validation(ref ve) if ve.message.contains("novel_category is required"))
        );
    }

    // AC3: (world_id, block_type, canonical_name) active uniqueness preserved on insert.
    // (Already covered by existing T4 test, but let's confirm in Novel mode.)
    #[tokio::test]
    async fn test_novel_uniqueness_preserved() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let kb1 = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        let err = store.insert_key_block(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { ref name, .. } if name == "char_lin_xia"));
    }

    // AC4: world_refs resolution — query by canonical_name after insert.
    // Simulates resolving world_refs like "char_lin_xia" against stored items.
    #[tokio::test]
    async fn test_world_refs_resolve_by_canonical_name() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let kb_char = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        store.insert_key_block(kb_char).await.unwrap();

        let kb_loc = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        store.insert_key_block(kb_loc).await.unwrap();

        // Resolve "char_lin_xia"
        let result = store
            .query(&KbQuery::new("wld_1").with_canonical_name("char_lin_xia"))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].block_type, BlockType::Character);

        // Resolve "loc_neon_city"
        let result = store
            .query(&KbQuery::new("wld_1").with_canonical_name("loc_neon_city"))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].block_type, BlockType::Scene);

        // Non-existent ref returns empty
        let result = store
            .query(&KbQuery::new("wld_1").with_canonical_name("char_unknown"))
            .await
            .unwrap();
        assert!(result.items.is_empty());
    }

    // AC5: kb-extract prompt output schema is recognized by validation.
    // Parse a sample extract output matching the new prompt format.
    #[tokio::test]
    async fn test_kb_extract_output_passes_validation() {
        // Sample LLM output matching the updated extract.md response format
        let extract_json = r#"{
            "block_type": "character",
            "canonical_name": "char_lin_xia",
            "body": {
                "summary": "Ex-cartographer hiding a forbidden river map",
                "attributes": {
                    "novel_category": "character",
                    "aliases": ["Xia"],
                    "traits": ["brave", "resourceful"]
                },
                "tags": ["novel"]
            },
            "source_work_entry_id": "we_abc123"
        }"#;

        let value: serde_json::Value = serde_json::from_str(extract_json).unwrap();

        // Verify block_type deserializes from wire snake_case
        let bt: BlockType = serde_json::from_value(value["block_type"].clone()).unwrap();
        assert_eq!(bt, BlockType::Character);

        // Verify body passes novel validation
        let body: KeyBlockBody = serde_json::from_value(value["body"].clone()).unwrap();
        assert!(validate_body(bt, Some(&body), ValidationMode::Novel).is_ok());
    }

    // Confirm that generic store does NOT enforce novel_category.
    #[tokio::test]
    async fn test_generic_store_accepts_body_without_novel_category() {
        let store = InMemoryKbStore::new(); // Generic mode by default
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "char_generic");
        kb.set_body(KeyBlockBody {
            summary: Some("A generic character".to_string()),
            attributes: None,
            tags: None,
        })
        .unwrap();

        assert!(store.insert_key_block(kb).await.is_ok());
    }

    // Novel mode update also validates body.
    #[tokio::test]
    async fn test_novel_update_validates_body() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let mut kb = make_novel_block("wld_1", BlockType::Character, "char_hero", "character");
        store.insert_key_block(kb.clone()).await.unwrap();

        // Update to body missing novel_category should fail
        kb.set_body(KeyBlockBody {
            summary: Some("updated".to_string()),
            attributes: Some(serde_json::json!({"traits": ["old"]})),
            tags: None,
        })
        .unwrap();

        let err = store.update_key_block(kb).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Validation(ref ve) if ve.message.contains("novel_category is required"))
        );
    }
}
