//! KB Store trait — abstract storage for World-scoped narrative KB graph operations.
//!
//! The `KbStore` trait defines insert/query/update/delete operations scoped by
//! `world_id`. An in-memory implementation is provided for testing.
//!
//! # Validation
//!
//! `InMemoryKbStore` runs body validation on insert and update when a
//! [`ValidationMode`](crate::world_kb::validation::ValidationMode) is configured.
//! The default mode is `Generic` (no novel-specific checks). Set `validation_mode`
//! to [`ValidationMode::Novel`](crate::world_kb::validation::ValidationMode::Novel) to
//! enforce `body.attributes.novel_category` requirements per
//! entity-scope-model.md §5.1.1.

use crate::world_kb::errors::{KbError, ValidationError};
use crate::world_kb::knowledge_entry::{KnowledgeEntryRecord, KnowledgeOwnerRef};
use crate::world_kb::query::{KbInsertResult, KbQuery, KbQueryResult};
use crate::world_kb::source_anchor::SourceAnchor;
use crate::world_kb::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_contracts::BlockType;
use std::collections::HashMap;
use std::sync::RwLock;

// ── Store Error ─────────────────────────────────────────────────────

/// Error type for KB store operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KbStoreError {
    /// Uniqueness constraint violation — an active `KnowledgeEntryRecord` with
    /// the same `(canonical_name, block_type)` already exists under the same
    /// owner (v1.184 owner-scoped uniqueness).
    #[error(
        "duplicate: canonical_name={name}, block_type={block_type:?} \
         already active for owner {owner:?}"
    )]
    Duplicate {
        /// Owner where the conflict occurred.
        owner: KnowledgeOwnerRef,
        /// Canonical name that conflicts.
        name: String,
        /// Block type that conflicts.
        block_type: BlockType,
    },

    /// `KnowledgeEntryRecord` not found.
    #[error("key block not found: {0}")]
    NotFound(String),

    /// Owner or `creator_only` change attempted on update (both immutable,
    /// v1.184 P1 — moving knowledge is explicit create/copy work).
    #[error("owner is immutable for entry {0}")]
    ImmutableOwner(String),

    /// Native governance change attempted through the ordinary store path
    /// (durable §3): `holder_entry_id` / `disclosure` move only through the
    /// admitted audience-authoring transaction, so `KbStore::update` is not a
    /// transfer mechanism.
    #[error("governance is immutable through the ordinary store path for entry {0}")]
    ImmutableGovernance(String),

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
/// Under the same `world_id`, at most one **active** `KnowledgeEntryRecord` may exist
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
    /// Insert a new `KnowledgeEntryRecord`.
    ///
    /// Returns [`KbInsertResult`] on success.
    /// Returns [`KbStoreError::Duplicate`] if an active `KnowledgeEntryRecord` with the
    /// same `(canonical_name, block_type)` already exists in the same world.
    async fn insert_knowledge_entry(
        &self,
        kb: KnowledgeEntryRecord,
    ) -> Result<KbInsertResult, KbStoreError>;

    /// Get a `KnowledgeEntryRecord` by its ID.
    async fn get_knowledge_entry(
        &self,
        entry_id: &str,
    ) -> Result<KnowledgeEntryRecord, KbStoreError>;

    /// List all active `KnowledgeEntryRecord`s in a world.
    async fn list_by_world(
        &self,
        world_id: &str,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError>;

    /// Query `KnowledgeEntryRecord`s with filters.
    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError>;

    /// Attach a [`SourceAnchor`] to a `KnowledgeEntryRecord`.
    ///
    /// Multiple anchors can be attached to the same `KnowledgeEntryRecord`.
    async fn attach_source_anchor(
        &self,
        entry_id: &str,
        anchor: SourceAnchor,
    ) -> Result<(), KbStoreError>;

    /// Get all [`SourceAnchor`] instances attached to a `KnowledgeEntryRecord`.
    async fn get_anchors(&self, entry_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError>;

    /// Update an existing `KnowledgeEntryRecord` in place.
    ///
    /// Returns [`KbStoreError::NotFound`] if the `KnowledgeEntryRecord` does not exist.
    /// Returns [`KbStoreError::Duplicate`] if the update would violate
    /// the uniqueness constraint.
    async fn update_knowledge_entry(&self, kb: KnowledgeEntryRecord) -> Result<(), KbStoreError>;

    /// Soft-delete a `KnowledgeEntryRecord` by ID.
    ///
    /// Sets status to `deleted`. The record is retained.
    async fn delete_knowledge_entry(&self, entry_id: &str) -> Result<(), KbStoreError>;
}

// ── Read selection (v1.191 P1 T2, durable §4.1) ─────────────────────

/// Server-chosen read policy for one knowledge read (durable §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeReadPolicy {
    /// Authorized Creator management review: the owned World / Character /
    /// binding containers, including known private rows for any authorized
    /// owned Character. A distinct management query — never encoded as an
    /// absent viewpoint.
    CreatorManagement,
    /// Exact admitted Creator or Character holder plus the authorized
    /// containers. Character selection is World + that Character + this
    /// binding.
    ActorView,
}

/// Authorized read selection for one knowledge read (durable §4.1).
///
/// Lower layer only, and deliberately not a wire type: no `serde`, no
/// `Default`, private fields, and no accessor that implies a client-supplied
/// value is authority. `nexus-core` admission constructs it from an admitted
/// principal; this crate never depends on `nexus-core`, so the type cannot
/// receive a core `Principal`, and the public daemon/CLI handlers never accept
/// it. Adapters and stores validate the scope's stored subject/container
/// tuple when they resolve rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeReadScope {
    policy: KnowledgeReadPolicy,
    holder_entry_id: Option<String>,
    containers: Vec<KnowledgeOwnerRef>,
}

impl KnowledgeReadScope {
    /// Creator management review over explicitly owned containers.
    ///
    /// Carries no resolved holder: management selection is container-scoped
    /// plus the known-governance rule, not holder-filtered.
    #[must_use]
    pub const fn creator_management(containers: Vec<KnowledgeOwnerRef>) -> Self {
        Self {
            policy: KnowledgeReadPolicy::CreatorManagement,
            holder_entry_id: None,
            containers,
        }
    }

    /// Actor view: the exact admitted holder plus authorized containers.
    ///
    /// # Errors
    /// Returns [`KbError::ValidationError`] when the holder is empty — a
    /// holder-scoped read without a resolved holder would silently widen or
    /// empty the selection.
    pub fn actor_view(
        holder_entry_id: impl Into<String>,
        containers: Vec<KnowledgeOwnerRef>,
    ) -> Result<Self, KbError> {
        let holder_entry_id = holder_entry_id.into();
        if holder_entry_id.is_empty() {
            return Err(KbError::ValidationError(
                "ActorView read scope requires a nonempty resolved holder".into(),
            ));
        }
        Ok(Self {
            policy: KnowledgeReadPolicy::ActorView,
            holder_entry_id: Some(holder_entry_id),
            containers,
        })
    }

    /// The server-chosen policy of this selection.
    #[must_use]
    pub const fn policy(&self) -> KnowledgeReadPolicy {
        self.policy
    }

    /// The resolved holder entry id, present exactly for
    /// [`KnowledgeReadPolicy::ActorView`].
    #[must_use]
    pub fn holder_entry_id(&self) -> Option<&str> {
        self.holder_entry_id.as_deref()
    }

    /// The authorized container selectors.
    #[must_use]
    pub fn containers(&self) -> &[KnowledgeOwnerRef] {
        &self.containers
    }
}

// ── In-Memory Implementation ────────────────────────────────────────

/// In-memory KB store for testing and development.
///
/// Thread-safe via interior mutability (`std::sync::RwLock`).
/// Suitable for unit tests; not intended for production use.
pub struct InMemoryKbStore {
    blocks: RwLock<HashMap<String, KnowledgeEntryRecord>>,
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

    /// Check if a `KnowledgeEntryRecord` is "active" (not deleted, merged, or deprecated).
    fn is_active(kb: &KnowledgeEntryRecord) -> bool {
        !matches!(kb.status.as_str(), "deleted" | "merged" | "deprecated")
    }

    /// Check the owner-scoped uniqueness constraint for
    /// `(owner, canonical_name, block_type)` (v1.184 P1).
    ///
    /// If `exclude_id` is provided, that `KnowledgeEntryRecord` is excluded from the check
    /// (used during updates where the block keeps its own ID).
    fn check_uniqueness(
        blocks: &HashMap<String, KnowledgeEntryRecord>,
        owner: &KnowledgeOwnerRef,
        canonical_name: &str,
        block_type: BlockType,
        exclude_id: Option<&str>,
    ) -> Result<(), KbStoreError> {
        for kb in blocks.values() {
            if &kb.owner == owner
                && kb.canonical_name == canonical_name
                && kb.block_type == block_type
                && Self::is_active(kb)
                && exclude_id != Some(kb.entry_id.as_str())
            {
                return Err(KbStoreError::Duplicate {
                    owner: owner.clone(),
                    name: canonical_name.to_string(),
                    block_type,
                });
            }
        }
        Ok(())
    }

    /// Validate the native governance pair of a record before it is stored
    /// (durable §3).
    ///
    /// Parity companion to `validate_creator_only_owner`: the in-memory
    /// backend owns the invariant that the `SQLite` columns + registry
    /// reference enforce, so an invalid pair is rejected before it can be
    /// observed or projected onto the wire.
    fn validate_governance(kb: &KnowledgeEntryRecord) -> Result<(), KbStoreError> {
        crate::world_kb::knowledge_entry::validate_native_governance(
            kb.holder_entry_id.as_deref(),
            kb.disclosure.as_deref(),
        )
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))
    }

    /// Acquire a read lock on the blocks map.
    fn read_blocks(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, KnowledgeEntryRecord>>, KbStoreError>
    {
        self.blocks
            .read()
            .map_err(|e| KbStoreError::Storage(e.to_string()))
    }

    /// Acquire a write lock on the blocks map.
    fn write_blocks(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, KnowledgeEntryRecord>>, KbStoreError>
    {
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

// `async` here is by trait contract, not by need: `KbStore` is async to
// match the future SQLite backend (see the trait docs) and this in-memory
// impl mirrors it. The methods perform no real async I/O, so clippy 1.98's
// `unused_async_trait_impl` (new in 1.98, toolchain drift) is silenced.
#[allow(clippy::unused_async_trait_impl)]
impl KbStore for InMemoryKbStore {
    async fn insert_knowledge_entry(
        &self,
        kb: KnowledgeEntryRecord,
    ) -> Result<KbInsertResult, KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(|e| match e {
            crate::world_kb::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
            crate::world_kb::errors::KbError::ValidationError(msg) => {
                KbStoreError::ValidationLegacy(msg)
            }
            other => KbStoreError::ValidationLegacy(other.to_string()),
        })?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode).map_err(
            |e| match e {
                crate::world_kb::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
                crate::world_kb::errors::KbError::ValidationError(msg) => {
                    KbStoreError::ValidationLegacy(msg)
                }
                other => KbStoreError::ValidationLegacy(other.to_string()),
            },
        )?;

        // v1.184 P1 fix: `creator_only` is World-only — the in-memory store
        // enforces the same invariant as the SQLite schema CHECK (which the
        // in-memory backend cannot rely on), so a non-World owner carrying the
        // flag is rejected before it can be observed or emit an invalid spoke
        // projection.
        crate::world_kb::knowledge_entry::validate_creator_only_owner(&kb.owner, kb.creator_only)
            .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

        // v1.191 P1 T2: the native governance pair is validated on every write
        // path, exactly like the owner/flag invariant above.
        Self::validate_governance(&kb)?;

        let entry_id = kb.entry_id.clone();
        let owner = kb.owner.clone();
        let created_at = kb.created_at.clone();

        {
            // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P1-S3
            // — concurrent-uniqueness race: InMemoryKbStore check+insert is not
            // atomic under concurrent access; acceptable for single-user daemon.
            let mut blocks = self.write_blocks()?;
            Self::check_uniqueness(&blocks, &kb.owner, &kb.canonical_name, kb.block_type, None)?;
            blocks.insert(entry_id.clone(), kb);
        }

        Ok(KbInsertResult {
            entry_id,
            owner,
            created_at,
        })
    }

    async fn get_knowledge_entry(
        &self,
        entry_id: &str,
    ) -> Result<KnowledgeEntryRecord, KbStoreError> {
        let blocks = self.read_blocks()?;
        blocks
            .get(entry_id)
            .cloned()
            .ok_or_else(|| KbStoreError::NotFound(entry_id.to_string()))
    }

    async fn list_by_world(
        &self,
        world_id: &str,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        // v1.184 P1: `list_by_world` remains a World-scoped read — only
        // World-owned active records match (Character/binding rows carry a
        // `None` world_id and are excluded).
        let items: Vec<KnowledgeEntryRecord> = self
            .read_blocks()?
            .values()
            .filter(|kb| kb.owner.world_id() == Some(world_id) && Self::is_active(kb))
            .cloned()
            .collect();
        Ok(items)
    }

    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        let (matches, total_count, has_more) = {
            let blocks = self.read_blocks()?;

            let mut matches: Vec<KnowledgeEntryRecord> =
                blocks
                    .values()
                    .filter(|kb| {
                        // World-scoped query only (World-owned active records).
                        if kb.owner.world_id() != Some(query.world_id.as_str())
                            || !Self::is_active(kb)
                        {
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
                        // V1.61 P1: filter by computable flag
                        if let Some(want) = query.computable {
                            let is_computable =
                                kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false);
                            if is_computable != want {
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
        entry_id: &str,
        anchor: SourceAnchor,
    ) -> Result<(), KbStoreError> {
        {
            let blocks = self.read_blocks()?;
            if !blocks.contains_key(entry_id) {
                return Err(KbStoreError::NotFound(entry_id.to_string()));
            }
        }

        {
            let mut anchors = self
                .anchors
                .write()
                .map_err(|e| KbStoreError::Storage(e.to_string()))?;
            anchors
                .entry(entry_id.to_string())
                .or_default()
                .push(anchor);
        }

        Ok(())
    }

    async fn get_anchors(&self, entry_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError> {
        let anchors = self
            .anchors
            .read()
            .map_err(|e| KbStoreError::Storage(e.to_string()))?;
        Ok(anchors.get(entry_id).cloned().unwrap_or_default())
    }

    async fn update_knowledge_entry(&self, kb: KnowledgeEntryRecord) -> Result<(), KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(|e| match e {
            crate::world_kb::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
            crate::world_kb::errors::KbError::ValidationError(msg) => {
                KbStoreError::ValidationLegacy(msg)
            }
            other => KbStoreError::ValidationLegacy(other.to_string()),
        })?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode).map_err(
            |e| match e {
                crate::world_kb::errors::KbError::Validation(ve) => KbStoreError::Validation(ve),
                crate::world_kb::errors::KbError::ValidationError(msg) => {
                    KbStoreError::ValidationLegacy(msg)
                }
                other => KbStoreError::ValidationLegacy(other.to_string()),
            },
        )?;

        Self::validate_governance(&kb)?;

        {
            let mut blocks = self.write_blocks()?;

            let existing = blocks
                .get(&kb.entry_id)
                .ok_or_else(|| KbStoreError::NotFound(kb.entry_id.clone()))?;

            // v1.184 P1: owner and creator_only are immutable through patch
            // APIs — moving knowledge is explicit create/copy work.
            if existing.owner != kb.owner || existing.creator_only != kb.creator_only {
                return Err(KbStoreError::ImmutableOwner(kb.entry_id.clone()));
            }

            // v1.191 P1 T2 (durable §3): the ordinary store path carries
            // content, not governance — the admitted audience-authoring
            // transaction is the only writer of holder_entry_id/disclosure, so
            // an ordinary update (compute, promote, sync) cannot transfer or
            // clear an existing private audience.
            if existing.holder_entry_id != kb.holder_entry_id
                || existing.disclosure != kb.disclosure
            {
                return Err(KbStoreError::ImmutableGovernance(kb.entry_id.clone()));
            }

            // Re-check owner-scoped uniqueness if name or type changed
            if existing.canonical_name != kb.canonical_name || existing.block_type != kb.block_type
            {
                Self::check_uniqueness(
                    &blocks,
                    &kb.owner,
                    &kb.canonical_name,
                    kb.block_type,
                    Some(&kb.entry_id),
                )?;
            }

            blocks.insert(kb.entry_id.clone(), kb);
        }
        Ok(())
    }

    async fn delete_knowledge_entry(&self, entry_id: &str) -> Result<(), KbStoreError> {
        let mut blocks = self.write_blocks()?;
        let kb = blocks
            .get_mut(entry_id)
            .ok_or_else(|| KbStoreError::NotFound(entry_id.to_string()))?;
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
    use crate::world_kb::knowledge_entry::KnowledgeEntryBody;
    use nexus_contracts::KeyBlockStatus;

    fn make_block(world_id: &str, block_type: BlockType, name: &str) -> KnowledgeEntryRecord {
        KnowledgeEntryRecord::new(world_id, block_type, name)
    }

    // T1: Insert and retrieve a KnowledgeEntryRecord
    #[tokio::test]
    async fn test_insert_and_get() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");

        let result = store.insert_knowledge_entry(kb.clone()).await.unwrap();
        assert_eq!(result.entry_id, kb.entry_id);
        assert_eq!(result.owner, KnowledgeOwnerRef::world("wld_1"));

        let fetched = store.get_knowledge_entry(&kb.entry_id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Hero");
        assert_eq!(fetched.world_id(), Some("wld_1"));
    }

    // T2: Get non-existent KnowledgeEntryRecord returns NotFound
    #[tokio::test]
    async fn test_get_not_found() {
        let store = InMemoryKbStore::new();
        let err = store.get_knowledge_entry("nonexistent").await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(ref s) if s == "nonexistent"));
    }

    // T3: List by world returns only active blocks in that world
    #[tokio::test]
    async fn test_list_by_world() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        let kb2 = make_block("wld_1", BlockType::Scene, "Forest");
        let kb3 = make_block("wld_2", BlockType::Character, "Villain");

        store.insert_knowledge_entry(kb1).await.unwrap();
        store.insert_knowledge_entry(kb2).await.unwrap();
        store.insert_knowledge_entry(kb3).await.unwrap();

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
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        let err = store.insert_knowledge_entry(kb2).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Duplicate { ref owner, ref name, .. }
                if owner == &KnowledgeOwnerRef::world("wld_1") && name == "Hero")
        );
    }

    // T5: Same canonical_name in different block types is allowed
    #[tokio::test]
    async fn test_uniqueness_allows_different_type() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Dragon");
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Event, "Dragon");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    // T6: Same canonical_name + type in different worlds is allowed
    #[tokio::test]
    async fn test_uniqueness_allows_different_world() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = make_block("wld_2", BlockType::Character, "Hero");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    // T7: Soft-deleted block does not block uniqueness re-insertion
    #[tokio::test]
    async fn test_deleted_allows_reinsertion() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        store.delete_knowledge_entry(&id).await.unwrap();

        // Re-insert with same canonical_name + type should succeed
        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    // T8: Query with block_type filter
    #[tokio::test]
    async fn test_query_by_block_type() {
        let store = InMemoryKbStore::new();
        store
            .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_block("wld_1", BlockType::Scene, "Forest"))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Villain"))
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
            .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Villain"))
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
        kb1.set_body(KnowledgeEntryBody {
            summary: Some("A brooding hero".to_string()),
            attributes: None,
            tags: Some(vec!["gothic".to_string()]),
            ..Default::default()
        })
        .unwrap();
        store.insert_knowledge_entry(kb1).await.unwrap();

        let mut kb2 = make_block("wld_1", BlockType::Scene, "Enchanted Forest");
        kb2.set_body(KnowledgeEntryBody {
            summary: Some("A magical woodland".to_string()),
            attributes: None,
            tags: Some(vec!["fantasy".to_string()]),
            ..Default::default()
        })
        .unwrap();
        store.insert_knowledge_entry(kb2).await.unwrap();

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
                .insert_knowledge_entry(make_block(
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
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let anchor1 = SourceAnchor::new("stm_1", "sum_1", Some("chapter"));
        let anchor2 = SourceAnchor::new("stm_2", "sum_2", None);

        store.attach_source_anchor(&id, anchor1).await.unwrap();
        store.attach_source_anchor(&id, anchor2).await.unwrap();

        let fetched = store.get_anchors(&id).await.unwrap();
        assert_eq!(fetched.len(), 2);
    }

    // T13: Attach anchor to non-existent KnowledgeEntryRecord fails
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
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let anchors = store.get_anchors(&id).await.unwrap();
        assert!(anchors.is_empty());
    }

    // T15: Update a KnowledgeEntryRecord
    #[tokio::test]
    async fn test_update_knowledge_entry() {
        let store = InMemoryKbStore::new();
        let mut kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        kb.canonical_name = "Superhero".to_string();
        store.update_knowledge_entry(kb).await.unwrap();

        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Superhero");
    }

    // T16: Update to conflicting canonical_name + block_type fails
    #[tokio::test]
    async fn test_update_conflict() {
        let store = InMemoryKbStore::new();
        let kb1 = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_knowledge_entry(kb1).await.unwrap();

        let mut kb2 = make_block("wld_1", BlockType::Character, "Villain");
        store.insert_knowledge_entry(kb2.clone()).await.unwrap();

        // Rename kb2 to "Hero" — should conflict with kb1
        kb2.canonical_name = "Hero".to_string();
        let err = store.update_knowledge_entry(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    // T17: Update non-existent KnowledgeEntryRecord fails
    #[tokio::test]
    async fn test_update_not_found() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Ghost");
        let err = store.update_knowledge_entry(kb).await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(_)));
    }

    // T18: Delete a KnowledgeEntryRecord (soft delete)
    #[tokio::test]
    async fn test_delete_knowledge_entry() {
        let store = InMemoryKbStore::new();
        let kb = make_block("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        store.delete_knowledge_entry(&id).await.unwrap();

        // Block still exists but is marked deleted
        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert_eq!(fetched.status, "deleted");

        // list_by_world excludes deleted
        let listed = store.list_by_world("wld_1").await.unwrap();
        assert!(listed.is_empty());
    }

    // T19: Delete non-existent KnowledgeEntryRecord fails
    #[tokio::test]
    async fn test_delete_not_found() {
        let store = InMemoryKbStore::new();
        let err = store.delete_knowledge_entry("ghost").await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(_)));
    }

    // T20: Query scoped to different world returns nothing
    #[tokio::test]
    async fn test_query_world_isolation() {
        let store = InMemoryKbStore::new();
        store
            .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Hero"))
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
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        kb.status = KeyBlockStatus::Deprecated.as_str().to_string();
        store.update_knowledge_entry(kb).await.unwrap();

        // Re-insert with same name + type should succeed
        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    // T22: Merged block does not block uniqueness
    #[tokio::test]
    async fn test_merged_allows_reinsertion() {
        let store = InMemoryKbStore::new();
        let mut kb = make_block("wld_1", BlockType::Character, "Hero");
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        kb.status = KeyBlockStatus::Merged.as_str().to_string();
        store.update_knowledge_entry(kb).await.unwrap();

        let kb2 = make_block("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    // ── P1 taxonomy tests (plan 2026-06-10-v1.40-world-kb-taxonomy T5) ──

    /// Helper: make a block with a novel body for taxonomy tests.
    fn make_novel_block(
        world_id: &str,
        block_type: BlockType,
        name: &str,
        novel_category: &str,
    ) -> KnowledgeEntryRecord {
        let mut kb = KnowledgeEntryRecord::new(world_id, block_type, name);
        kb.set_body(KnowledgeEntryBody {
            summary: Some(format!("{novel_category}: {name}")),
            attributes: Some(serde_json::json!({
                "novel_category": novel_category,
                "traits": ["test"]
            })),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        })
        .unwrap();
        kb
    }

    // AC1: Invalid wire block_type fails with structured error.
    // (BlockType is a Rust enum — unknown strings fail at deserialization,
    //  which is a structured parse error before reaching the store.
    //  This test confirms the validation path surfaces KbStoreError::Validation.)
    // R-V140P1-S2: renamed from test_invalid_block_type_via_deserialization —
    // the test exercises serde BlockType enum rejection, not the store path.
    #[tokio::test]
    async fn test_block_type_enum_rejects_unknown_variant() {
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
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // scene → location
        let kb = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // organization → society
        let kb = make_novel_block(
            "wld_1",
            BlockType::Organization,
            "org_solar_cult",
            "society",
        );
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // conflict → rules
        let kb = make_novel_block("wld_1", BlockType::Conflict, "rule_magic_cost", "rules");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // item → economy
        let kb = make_novel_block("wld_1", BlockType::Item, "item_memory_crystal", "economy");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // info_point → foundation
        let kb = make_novel_block("wld_1", BlockType::InfoPoint, "fnd_cosmology", "foundation");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // event → background
        let kb = make_novel_block("wld_1", BlockType::Event, "evt_great_fire", "background");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());

        // ability (no novel_category mapping, but novel mode allows any valid category)
        let kb = make_novel_block("wld_1", BlockType::Ability, "abl_shadow_walk", "character");
        assert!(store.insert_knowledge_entry(kb).await.is_ok());
    }

    // AC2 negative: Missing novel_category on character block fails in Novel mode.
    #[tokio::test]
    async fn test_novel_missing_category_rejected() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "char_no_cat");
        kb.set_body(KnowledgeEntryBody {
            summary: Some("A character without category".to_string()),
            attributes: Some(serde_json::json!({"aliases": ["NoCat"]})),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        })
        .unwrap();

        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        let err = store.insert_knowledge_entry(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { ref name, .. } if name == "char_lin_xia"));
    }

    // AC4: world_refs resolution — query by canonical_name after insert.
    // Simulates resolving world_refs like "char_lin_xia" against stored items.
    #[tokio::test]
    async fn test_world_refs_resolve_by_canonical_name() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let kb_char = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        store.insert_knowledge_entry(kb_char).await.unwrap();

        let kb_loc = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        store.insert_knowledge_entry(kb_loc).await.unwrap();

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
        let body: KnowledgeEntryBody = serde_json::from_value(value["body"].clone()).unwrap();
        assert!(validate_body(bt, Some(&body), ValidationMode::Novel).is_ok());
    }

    // Confirm that generic store does NOT enforce novel_category.
    #[tokio::test]
    async fn test_generic_store_accepts_body_without_novel_category() {
        let store = InMemoryKbStore::new(); // Generic mode by default
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "char_generic");
        kb.set_body(KnowledgeEntryBody {
            summary: Some("A generic character".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        })
        .unwrap();

        assert!(store.insert_knowledge_entry(kb).await.is_ok());
    }

    // Novel mode update also validates body.
    #[tokio::test]
    async fn test_novel_update_validates_body() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let mut kb = make_novel_block("wld_1", BlockType::Character, "char_hero", "character");
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        // Update to body missing novel_category should fail
        kb.set_body(KnowledgeEntryBody {
            summary: Some("updated".to_string()),
            attributes: Some(serde_json::json!({"traits": ["old"]})),
            tags: None,
            ..Default::default()
        })
        .unwrap();

        let err = store.update_knowledge_entry(kb).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Validation(ref ve) if ve.message.contains("novel_category is required"))
        );
    }

    // ── Computable query filter (V1.61 P1) ─────────────────────────

    fn make_computable_block(world_id: &str, name: &str, computable: bool) -> KnowledgeEntryRecord {
        let mut kb = KnowledgeEntryRecord::new(world_id, BlockType::Character, name);
        kb.set_body(KnowledgeEntryBody {
            summary: Some(format!("{name} summary")),
            attributes: if computable {
                Some(serde_json::json!({"max_hp": 100}))
            } else {
                None
            },
            tags: None,
            computable: Some(computable),
            state: if computable {
                Some(serde_json::json!({"character": {"current_hp": 80}}))
            } else {
                None
            },
        })
        .unwrap();
        kb
    }

    #[tokio::test]
    async fn query_computable_true_filters_only_computable_blocks() {
        let store = InMemoryKbStore::new();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "Hero", true))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "NPC", false))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "Place", false))
            .await
            .unwrap();

        let q = KbQuery::new("wld_1").with_computable(Some(true));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.items[0].canonical_name, "Hero");
    }

    #[tokio::test]
    async fn query_computable_false_filters_only_non_computable() {
        let store = InMemoryKbStore::new();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "Hero", true))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "NPC", false))
            .await
            .unwrap();

        let q = KbQuery::new("wld_1").with_computable(Some(false));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.items[0].canonical_name, "NPC");
    }

    #[tokio::test]
    async fn query_computable_none_returns_all() {
        let store = InMemoryKbStore::new();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "Hero", true))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_block("wld_1", "NPC", false))
            .await
            .unwrap();

        let q = KbQuery::new("wld_1"); // computable defaults to None
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 2);
    }

    #[tokio::test]
    async fn query_computable_with_computable_absent_in_body() {
        let store = InMemoryKbStore::new();
        // Insert a block with no computable field at all (legacy block)
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Legacy");
        kb.set_body(KnowledgeEntryBody {
            summary: Some("legacy".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        })
        .unwrap();
        store.insert_knowledge_entry(kb).await.unwrap();

        // Filtering for computable=true should exclude the legacy block
        let q = KbQuery::new("wld_1").with_computable(Some(true));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 0);

        // Filtering for computable=false should include the legacy block
        let q = KbQuery::new("wld_1").with_computable(Some(false));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
    }

    // v1.184 P1 fix parity: the in-memory store must reject `creator_only`
    // set on a Character- or binding-owned record (the SQLite CHECK is not
    // available to the in-memory backend) — the invariant must match across
    // domain / memory / SQLite / conversion.
    #[tokio::test]
    async fn insert_rejects_creator_only_on_character_owner() {
        let store = InMemoryKbStore::new();
        let mut kb = KnowledgeEntryRecord::for_character("chr_1", BlockType::Character, "Flagged");
        kb.creator_only = true;
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(m) if m.contains("creator_only")),
            "character-owned creator_only must be rejected, got {err:?}"
        );
    }

    #[tokio::test]
    async fn insert_rejects_creator_only_on_binding_owner() {
        let store = InMemoryKbStore::new();
        let mut kb = KnowledgeEntryRecord::for_binding("awb_1", BlockType::Character, "Flagged");
        kb.creator_only = true;
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(m) if m.contains("creator_only")),
            "binding-owned creator_only must be rejected, got {err:?}"
        );
    }

    // World-owned creator_only remains accepted (parity with SQLite).
    #[tokio::test]
    async fn insert_accepts_creator_only_on_world_owner() {
        let store = InMemoryKbStore::new();
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Flagged");
        kb.creator_only = true;
        let result = store.insert_knowledge_entry(kb).await.unwrap();
        assert_eq!(result.owner, KnowledgeOwnerRef::world("wld_1"));
    }

    // ── v1.191 P1 T2: native holder governance (durable §§1, 3) ─────────
    //
    // Domain-parity home for the governance contract: audience
    // omission/explicit semantics, the closed native pair, reserved-key
    // rejection, read-selection construction, and the ordinary store path's
    // refusal to move governance.

    use crate::world_kb::knowledge_entry::{
        reject_reserved_authoring_keys, resolve_authored_governance, validate_native_governance,
        KnowledgeAudience, KnowledgeAuthoringOp, KnowledgeGovernance, ReservedAuthoringKey,
        DISCLOSURE_OWNER_PRIVATE, LEGACY_CREATOR_ONLY_KEY, LEGACY_CREATOR_ONLY_UNSUPPORTED,
    };

    /// A resolved holder entry id in the reserved `hld_` namespace.
    const HLD: &str = "hld_0000000000000000000000000000000000000000000000000000000000000000";

    fn chr() -> String {
        format!("chr_{}", "a".repeat(32))
    }

    #[tokio::test]
    async fn v1191_governance_omitted_create_audience_is_shared() {
        let store = InMemoryKbStore::new();

        let omitted = resolve_authored_governance(KnowledgeAuthoringOp::Create, None, None)
            .expect("omitted create audience resolves")
            .expect("create always authors a pair");
        assert!(omitted.is_shared(), "omitted create audience is shared");
        assert_eq!(omitted, KnowledgeGovernance::shared());

        let explicit = resolve_authored_governance(
            KnowledgeAuthoringOp::Create,
            Some(&KnowledgeAudience::Shared),
            None,
        )
        .expect("explicit shared resolves")
        .expect("create authors a pair");
        assert_eq!(explicit, omitted, "explicit shared on create == omitted");

        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Shared note");
        kb.holder_entry_id.clone_from(&omitted.holder_entry_id);
        kb.disclosure.clone_from(&omitted.disclosure);
        let entry_id = store.insert_knowledge_entry(kb).await.unwrap().entry_id;

        let stored = store.get_knowledge_entry(&entry_id).await.unwrap();
        assert_eq!(
            (
                stored.holder_entry_id.as_deref(),
                stored.disclosure.as_deref()
            ),
            (None, None),
            "a shared row projects both governance columns absent"
        );
    }

    #[tokio::test]
    async fn v1191_governance_omitted_patch_preserves_stored_pair() {
        let store = InMemoryKbStore::new();

        let pair = resolve_authored_governance(
            KnowledgeAuthoringOp::Create,
            Some(&KnowledgeAudience::AuthorOnly),
            Some(HLD),
        )
        .expect("private create audience resolves")
        .expect("create authors a pair");
        assert_eq!(pair.holder_entry_id.as_deref(), Some(HLD));
        assert_eq!(pair.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));

        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Private note");
        kb.holder_entry_id.clone_from(&pair.holder_entry_id);
        kb.disclosure.clone_from(&pair.disclosure);
        let entry_id = store.insert_knowledge_entry(kb).await.unwrap().entry_id;

        let authored = resolve_authored_governance(KnowledgeAuthoringOp::Patch, None, Some(HLD))
            .expect("omitted patch audience resolves");
        assert_eq!(
            authored, None,
            "an omitted patch audience authors no governance column"
        );

        // A content-only edit therefore keeps the stored private audience.
        let mut edited = store.get_knowledge_entry(&entry_id).await.unwrap();
        edited.canonical_name = "Private note v2".to_string();
        store.update_knowledge_entry(edited).await.unwrap();

        let stored = store.get_knowledge_entry(&entry_id).await.unwrap();
        assert_eq!(stored.holder_entry_id.as_deref(), Some(HLD));
        assert_eq!(stored.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    }

    #[test]
    fn v1191_governance_explicit_shared_clears_both_columns() {
        let cleared = resolve_authored_governance(
            KnowledgeAuthoringOp::Patch,
            Some(&KnowledgeAudience::Shared),
            Some(HLD),
        )
        .expect("explicit shared resolves")
        .expect("explicit shared authors the pair");
        assert_eq!(
            cleared,
            KnowledgeGovernance::shared(),
            "explicit shared clears holder AND disclosure"
        );
        assert!(cleared.holder_entry_id.is_none());
        assert!(cleared.disclosure.is_none());
    }

    #[test]
    fn v1191_governance_private_audience_requires_resolved_holder() {
        let character = chr();
        let audiences = [
            KnowledgeAudience::AuthorOnly,
            KnowledgeAudience::character_private(character.clone()).expect("chr_ id accepted"),
        ];
        for audience in &audiences {
            for unresolved in [None, Some("")] {
                let err = resolve_authored_governance(
                    KnowledgeAuthoringOp::Patch,
                    Some(audience),
                    unresolved,
                )
                .expect_err("private audience without a resolved holder must fail");
                assert!(
                    err.to_string()
                        .contains("requires a resolved holder entry id"),
                    "unexpected error: {err}"
                );
            }
            let pair =
                resolve_authored_governance(KnowledgeAuthoringOp::Patch, Some(audience), Some(HLD))
                    .expect("resolved holder accepted")
                    .expect("private audience authors a pair");
            assert_eq!(pair.holder_entry_id.as_deref(), Some(HLD));
            assert_eq!(pair.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
        }

        assert_eq!(KnowledgeAudience::Shared.kind(), "shared");
        assert_eq!(KnowledgeAudience::AuthorOnly.kind(), "author-only");
        assert_eq!(audiences[1].kind(), "character-private");
        assert_eq!(audiences[1].character_id(), Some(character.as_str()));
        assert!(!KnowledgeAudience::Shared.requires_holder());
        assert!(audiences[1].requires_holder());
        assert!(
            KnowledgeAudience::character_private("wld_not_a_character").is_err(),
            "a non-Character target is not a permitted private audience"
        );
    }

    #[test]
    fn v1191_governance_legacy_creator_only_key_is_rejected() {
        let rejected = reject_reserved_authoring_keys(&serde_json::json!({
            "owner_kind": "character",
            "canonical_name": "note-alpha",
            "creator_only": false,
            "audience": { "kind": "shared" },
        }))
        .expect_err("legacy key presence must be rejected even when false");
        assert_eq!(rejected, ReservedAuthoringKey::LegacyCreatorOnly);
        assert_eq!(rejected.key(), LEGACY_CREATOR_ONLY_KEY);
        assert_eq!(rejected.reason(), LEGACY_CREATOR_ONLY_UNSUPPORTED);
        assert!(
            rejected
                .to_string()
                .starts_with("legacy_creator_only_unsupported:"),
            "unexpected message: {rejected}"
        );

        assert_eq!(
            reject_reserved_authoring_keys(&serde_json::json!({ "holder_entry_id": HLD }))
                .expect_err("service-resolved holder is not authored"),
            ReservedAuthoringKey::HolderEntryId
        );
        assert_eq!(
            reject_reserved_authoring_keys(&serde_json::json!({
                "disclosure": DISCLOSURE_OWNER_PRIVATE
            }))
            .expect_err("service-resolved disclosure is not authored"),
            ReservedAuthoringKey::Disclosure
        );

        reject_reserved_authoring_keys(&serde_json::json!({
            "canonical_name": "note-alpha",
            "audience": { "kind": "character-private", "character_id": chr() },
        }))
        .expect("an audience-aware input without reserved keys is accepted");
    }

    #[test]
    fn v1191_governance_read_scope_construction() {
        let containers = vec![
            KnowledgeOwnerRef::world("wld_1"),
            KnowledgeOwnerRef::character(chr()),
        ];

        let management = KnowledgeReadScope::creator_management(containers.clone());
        assert_eq!(management.policy(), KnowledgeReadPolicy::CreatorManagement);
        assert_eq!(management.holder_entry_id(), None);
        assert_eq!(management.containers(), containers.as_slice());

        let view = KnowledgeReadScope::actor_view(HLD, vec![KnowledgeOwnerRef::world("wld_1")])
            .expect("holder-scoped view");
        assert_eq!(view.policy(), KnowledgeReadPolicy::ActorView);
        assert_eq!(view.holder_entry_id(), Some(HLD));
        assert_eq!(view.containers(), &[KnowledgeOwnerRef::world("wld_1")]);

        assert!(
            KnowledgeReadScope::actor_view("", containers).is_err(),
            "a holder-scoped read without a resolved holder must not construct"
        );
    }

    #[test]
    fn v1191_governance_native_pair_validation() {
        assert_eq!(DISCLOSURE_OWNER_PRIVATE, "owner-private");

        validate_native_governance(None, None).expect("shared pair");
        validate_native_governance(Some(HLD), None)
            .expect("adopted foreign holder without disclosure");
        validate_native_governance(Some(HLD), Some(DISCLOSURE_OWNER_PRIVATE))
            .expect("private pair");

        assert!(validate_native_governance(None, Some(DISCLOSURE_OWNER_PRIVATE)).is_err());
        assert!(validate_native_governance(Some(""), None).is_err());
        assert!(validate_native_governance(Some(HLD), Some("")).is_err());
        assert!(
            validate_native_governance(Some(HLD), Some("shared")).is_err(),
            "shared is the absence of disclosure, never the string"
        );
        assert!(validate_native_governance(Some(HLD), Some("owner-public")).is_err());
    }

    #[tokio::test]
    async fn v1191_governance_store_refuses_governance_transfer() {
        let store = InMemoryKbStore::new();
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Private note");
        kb.holder_entry_id = Some(HLD.to_string());
        kb.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
        let entry_id = store.insert_knowledge_entry(kb).await.unwrap().entry_id;

        let mut cleared = store.get_knowledge_entry(&entry_id).await.unwrap();
        cleared.holder_entry_id = None;
        cleared.disclosure = None;
        let err = store.update_knowledge_entry(cleared).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ImmutableGovernance(id) if id == &entry_id),
            "ordinary update must not clear governance: {err:?}"
        );

        let mut moved = store.get_knowledge_entry(&entry_id).await.unwrap();
        moved.holder_entry_id = Some("hld_moved_elsewhere".to_string());
        let err = store.update_knowledge_entry(moved).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ImmutableGovernance(_)),
            "ordinary update must not move governance: {err:?}"
        );

        let stored = store.get_knowledge_entry(&entry_id).await.unwrap();
        assert_eq!(stored.holder_entry_id.as_deref(), Some(HLD));
        assert_eq!(stored.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    }

    #[tokio::test]
    async fn v1191_governance_store_rejects_invalid_pair_on_insert() {
        let store = InMemoryKbStore::new();

        let mut orphan = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Orphan");
        orphan.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
        let err = store.insert_knowledge_entry(orphan).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(m) if m.contains("owner-private disclosure requires a nonempty holder_entry_id")),
            "disclosure without a holder must be rejected: {err:?}"
        );

        let mut unknown = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Unknown");
        unknown.holder_entry_id = Some(HLD.to_string());
        unknown.disclosure = Some("owner-public".to_string());
        let err = store.insert_knowledge_entry(unknown).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(m) if m.contains("unknown disclosure vocabulary")),
            "non-core disclosure must be rejected natively: {err:?}"
        );
        assert_eq!(
            store.list_by_world("wld_1").await.unwrap().len(),
            0,
            "a rejected pair writes nothing"
        );
    }
}
