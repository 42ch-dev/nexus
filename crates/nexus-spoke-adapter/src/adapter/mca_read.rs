//! MCA (Moment Context Assembly) `WorldKB` read path (V1.145 P2 Option 2).
//!
//! [`SpokeBackedKbStore`] is the [`KbStore`] implementation injected at the
//! `assemble_moment` wiring site ([`NexusBaselineAdapter`]'s MCA fetch) so the
//! `WorldKB` read crosses the spoke-adapter boundary: storage rows → spoke
//! `KnowledgeEntry` (via the [`conversion`] seam) → [`WorldKbEntry`] (via
//! [`spoke_to_world_kb`]).
//!
//! # Why a typed carrier (not `Scope.extensions`)
//!
//! spoke 0.5.0's `Scope` carries only 7 native fields and has **no**
//! `extensions` map, so the `KbQuery` filters MCA needs (`text_search` /
//! `canonical_name` / `computable` / `limit` / `offset`) cannot ride on the
//! spoke `Scope`. They ride the typed [`KbScopeFilters`] carrier alongside the
//! `Scope`'s native `entry_types`, applied in the same SQL read (P2 spec §7.4
//! amendment). See the carrier's doc comment in `nexus-local-db::kb_store`.
//!
//! # Behavior preservation (HARD)
//!
//! [`SpokeBackedKbStore::query`] produces a byte-identical [`KbQueryResult`]
//! to [`SqliteKbStore::query`]: the adapter's scoped read delegates to
//! [`SqliteKbStore::query_scoped`], which in turn delegates to
//! [`SqliteKbStore::query`] — same silent 500-row window, same in-memory
//! filter + pagination, **no** reject-on-overflow (the spoke
//! `ScopeQueryPort::list_knowledge_entries` reject serves spoke orchestrators,
//! not MCA).
//!
//! [`conversion`]: crate::conversion
//! [`spoke_to_world_kb`]: crate::conversion::spoke_to_world_kb
//! [`world_kb_to_spoke`]: crate::conversion::world_kb_to_spoke

use super::NexusBaselineAdapter;
use crate::conversion::{spoke_to_world_kb, world_kb_to_spoke};
use crate::extensions::set_nexus_body;
use crate::KnowledgeEntry;
use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
use nexus_knowledge::world_kb::query::{KbQuery, KbQueryResult};
use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::{KbScopeFilters, SqliteKbStore};
use sqlx::SqlitePool;

// ── Adapter scoped-read method ──────────────────────────────────────────

/// Result of [`NexusBaselineAdapter::list_knowledge_entries_scoped`] — the
/// spoke-wire analogue of [`KbQueryResult`] (items are spoke
/// [`KnowledgeEntry`] instead of [`WorldKbEntry`]).
#[derive(Debug, Clone)]
pub struct ScopedKbRead {
    /// Matching spoke `KnowledgeEntry`s (after pagination).
    pub items: Vec<KnowledgeEntry>,
    /// Total matching count (ignoring limit/offset) — mirrors
    /// [`KbQueryResult::total_count`].
    pub total_count: usize,
    /// Whether more results exist beyond the current page — mirrors
    /// [`KbQueryResult::has_more`].
    pub has_more: bool,
}

impl NexusBaselineAdapter<'_> {
    /// MCA `WorldKB` read: list active knowledge entries for a world as spoke
    /// wire types, applying the typed [`KbScopeFilters`] carrier + the spoke
    /// `Scope`'s native `entry_types` (V1.145 P2 Option 2).
    ///
    /// Routes through [`SqliteKbStore::query_scoped`] (→ [`SqliteKbStore::query`])
    /// so the result matches the direct `query` path's limit/overflow semantics
    /// **exactly** (silent 500-row window; no reject). Each [`WorldKbEntry`] row
    /// is projected to a spoke [`KnowledgeEntry`] via the sole conversion seam
    /// ([`world_kb_to_spoke`]); the lossless `_nexus_body` carrier is stashed so
    /// the reverse conversion in [`SpokeBackedKbStore`] recovers the exact body
    /// (V1.143 body-fidelity mechanism, applied to the read path).
    ///
    /// This is an async inherent method (not the sync spoke `ScopeQueryPort`
    /// trait method): MCA is an async caller and must NOT inherit the spoke
    /// port's reject-on-overflow contract.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError`] on storage failure (same surface as
    /// [`KbStore::query`]).
    pub async fn list_knowledge_entries_scoped(
        &self,
        world_id: &str,
        entry_types: &[String],
        filters: &KbScopeFilters,
    ) -> Result<ScopedKbRead, KbStoreError> {
        let store = SqliteKbStore::new(self.pool.clone());
        let result = store.query_scoped(world_id, entry_types, filters).await?;

        let items = result
            .items
            .iter()
            .map(|entry| {
                let mut spoke = world_kb_to_spoke(entry);
                // Stash the lossless body carrier so `spoke_to_world_kb` in
                // `SpokeBackedKbStore` recovers the exact body (attributes,
                // computable, etc.) instead of the spoke-truncated fallback.
                // Only the MCA read path sets this; the spoke orchestrator
                // path (scope_query_port) does not, so orchestrators never see
                // the reserved `_nexus_body` key.
                if let Some(body) = &entry.body {
                    let body_value = serde_json::to_value(body).unwrap_or_default();
                    set_nexus_body(&mut spoke, Some(&body_value));
                }
                spoke
            })
            .collect();

        Ok(ScopedKbRead {
            items,
            total_count: result.total_count,
            has_more: result.has_more,
        })
    }
}

// ── SpokeBackedKbStore — KbStore impl for the MCA read path ─────────────

/// `KbStore` implementation backed by [`NexusBaselineAdapter`], used at the
/// `assemble_moment` wiring site so the MCA `WorldKB` read crosses the
/// spoke-adapter boundary (V1.145 P2 Option 2).
///
/// [`query`](Self::query) is the production path: it builds a spoke `Scope`
/// (native fields only) + a [`KbScopeFilters`] carrier from the inbound
/// [`KbQuery`], calls the adapter's scoped read, and converts each spoke
/// [`KnowledgeEntry`] back to a [`WorldKbEntry`] via [`spoke_to_world_kb`]. The
/// result is byte-identical to [`SqliteKbStore::query`] (see the module docs).
///
/// The remaining read methods (`get_knowledge_entry` / `list_by_world` /
/// `get_anchors`) delegate to [`SqliteKbStore`] directly — MCA does not call
/// them, and the daemon CRUD path uses `SqliteKbStore` unchanged. Write methods
/// return [`KbStoreError::Storage`]: this store is read-only for MCA; writes go
/// through the daemon CRUD / spoke orchestrator paths.
pub struct SpokeBackedKbStore {
    /// The adapter the `query` path routes through (storage → spoke conversion).
    adapter: NexusBaselineAdapter<'static>,
    /// Pool for delegating the non-MCA read methods to `SqliteKbStore`.
    pool: SqlitePool,
}

impl SpokeBackedKbStore {
    /// Construct from a [`SqlitePool`], built inside a tokio multi-threaded
    /// runtime (the adapter captures the runtime [`Handle`] — see
    /// [`NexusBaselineAdapter::new`]).
    ///
    /// # Panics
    ///
    /// Panics if no tokio runtime is running on the current thread (same
    /// precondition as [`NexusBaselineAdapter::new`]).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let adapter = NexusBaselineAdapter::new(pool.clone());
        Self { adapter, pool }
    }

    /// Map an inbound [`KbQuery::block_type`] to the spoke `Scope.entry_types`
    /// wire string (`snake_case`). MCA sends at most one; `None` → empty.
    fn block_type_to_entry_types(block_type: Option<nexus_contracts::BlockType>) -> Vec<String> {
        block_type
            .map(|bt| {
                serde_json::to_value(bt)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("{bt:?}").to_lowercase())
            })
            .into_iter()
            .collect()
    }
}

// SAFETY: sqlx SQLite futures borrow the connection pool internally; safe for
// single-threaded SQLite usage within our tokio runtime (mirrors the
// `SqliteKbStore` impl).
#[allow(clippy::future_not_send)]
impl KbStore for SpokeBackedKbStore {
    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        // Build the spoke Scope's native filters + the typed carrier for the
        // filters spoke Scope cannot hold (spoke 0.5.0 Scope has no extensions).
        let entry_types = Self::block_type_to_entry_types(query.block_type);
        let filters = KbScopeFilters {
            text_search: query.text_search.clone(),
            canonical_name: query.canonical_name.clone(),
            computable: query.computable,
            limit: query.limit,
            offset: query.offset,
        };

        // Cross the spoke-adapter boundary: storage → spoke KnowledgeEntry,
        // then convert back to the domain type via the sole conversion seam.
        let read = self
            .adapter
            .list_knowledge_entries_scoped(&query.world_id, &entry_types, &filters)
            .await?;
        let items: Vec<WorldKbEntry> = read.items.into_iter().map(spoke_to_world_kb).collect();
        Ok(KbQueryResult {
            items,
            total_count: read.total_count,
            has_more: read.has_more,
        })
    }

    async fn insert_knowledge_entry(
        &self,
        _kb: WorldKbEntry,
    ) -> Result<nexus_knowledge::world_kb::query::KbInsertResult, KbStoreError> {
        Err(read_only_error("insert_knowledge_entry"))
    }

    async fn get_knowledge_entry(&self, entry_id: &str) -> Result<WorldKbEntry, KbStoreError> {
        // Delegate to SqliteKbStore (MCA does not call this; the daemon CRUD
        // path uses SqliteKbStore directly — unchanged).
        SqliteKbStore::new(self.pool.clone())
            .get_knowledge_entry(entry_id)
            .await
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<WorldKbEntry>, KbStoreError> {
        SqliteKbStore::new(self.pool.clone())
            .list_by_world(world_id)
            .await
    }

    async fn attach_source_anchor(
        &self,
        _entry_id: &str,
        _anchor: SourceAnchor,
    ) -> Result<(), KbStoreError> {
        Err(read_only_error("attach_source_anchor"))
    }

    async fn get_anchors(&self, entry_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError> {
        SqliteKbStore::new(self.pool.clone())
            .get_anchors(entry_id)
            .await
    }

    async fn update_knowledge_entry(&self, _kb: WorldKbEntry) -> Result<(), KbStoreError> {
        Err(read_only_error("update_knowledge_entry"))
    }

    async fn delete_knowledge_entry(&self, _entry_id: &str) -> Result<(), KbStoreError> {
        Err(read_only_error("delete_knowledge_entry"))
    }
}

/// Construct the canonical "read-only MCA store" error for a write method.
fn read_only_error(method: &str) -> KbStoreError {
    KbStoreError::Storage(format!(
        "SpokeBackedKbStore is read-only (MCA `WorldKB` read path); \
         `{method}` is not supported — use the daemon CRUD / spoke orchestrator paths"
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::WorldKbBody;
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world_with_entries(pool: &sqlx::SqlitePool) -> (String, Vec<WorldKbEntry>) {
        // SAFETY: test-only static INSERTs with bind params.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_mca', 'wrk_test', 'ctr_test', 'MCA World', 'mca-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();

        let store = SqliteKbStore::new(pool.clone());
        let mut seeded = Vec::new();
        for (idx, (block_type, name)) in [
            (BlockType::Character, "Alice"),
            (BlockType::Item, "Atlantis"),
            (BlockType::Organization, "Anvil"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut entry = WorldKbEntry::new("wld_mca", block_type, name);
            entry.entry_id = format!("kb_mca_{idx}");
            entry.body = Some(WorldKbBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            store.insert_knowledge_entry(entry.clone()).await.unwrap();
            seeded.push(entry);
        }
        ("wld_mca".to_string(), seeded)
    }

    /// C1: `SpokeBackedKbStore::query` returns the same items as
    /// `SqliteKbStore::query` for the same data + filters (the core P2 value:
    /// the MCA read crosses the spoke boundary yet stays behavior-equivalent).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn query_matches_sqlite_store_on_same_data() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _seeded) = seed_world_with_entries(&pool).await;

        let sqlite = SqliteKbStore::new(pool.clone());
        let spoke_backed = SpokeBackedKbStore::new(pool);

        // Unfiltered
        let q = KbQuery::new(&world_id);
        let sqlite_res = sqlite.query(&q).await.unwrap();
        let spoke_res = spoke_backed.query(&q).await.unwrap();
        assert_eq!(sqlite_res.total_count, spoke_res.total_count);
        assert_eq!(sqlite_res.has_more, spoke_res.has_more);
        assert_eq!(sqlite_res.items.len(), spoke_res.items.len());
        // canonical_name + summary + block_type are the fields MCA renders;
        // they must match verbatim after the spoke round-trip.
        for (a, b) in sqlite_res.items.iter().zip(spoke_res.items.iter()) {
            assert_eq!(a.canonical_name, b.canonical_name);
            assert_eq!(a.block_type, b.block_type);
            assert_eq!(
                a.body.as_ref().and_then(|b| b.summary.as_deref()),
                b.body.as_ref().and_then(|b| b.summary.as_deref()),
            );
        }

        // block_type filter
        let q = KbQuery::new(&world_id).with_block_type(BlockType::Character);
        let sqlite_res = sqlite.query(&q).await.unwrap();
        let spoke_res = spoke_backed.query(&q).await.unwrap();
        assert_eq!(sqlite_res.items.len(), spoke_res.items.len());
        assert_eq!(spoke_res.items.len(), 1);
        assert_eq!(spoke_res.items[0].canonical_name, "Alice");

        // text_search filter
        let q = KbQuery::new(&world_id).with_text_search("atl");
        let sqlite_res = sqlite.query(&q).await.unwrap();
        let spoke_res = spoke_backed.query(&q).await.unwrap();
        assert_eq!(sqlite_res.items.len(), spoke_res.items.len());
        assert_eq!(spoke_res.items[0].canonical_name, "Atlantis");

        // limit
        let q = KbQuery::new(&world_id).with_limit(2);
        let sqlite_res = sqlite.query(&q).await.unwrap();
        let spoke_res = spoke_backed.query(&q).await.unwrap();
        assert_eq!(sqlite_res.items.len(), spoke_res.items.len());
        assert_eq!(spoke_res.items.len(), 2);
        assert!(sqlite_res.has_more && spoke_res.has_more);
    }

    /// C2: `SpokeBackedKbStore::query` applies the lossless body carrier, so a
    /// body with an integer attribute round-trips byte-identical (no int→float
    /// drift). Proves the carrier stashed on the read path is recovered.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn query_round_trips_full_body_via_carrier() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _seeded) = seed_world_with_entries(&pool).await;

        // Re-seed one entry with an integer attribute that the spoke typed body
        // alone would round-trip as a float.
        let mut entry = WorldKbEntry::new(&world_id, BlockType::Character, "Numeric");
        entry.entry_id = "kb_mca_num".to_string();
        entry.body = Some(WorldKbBody {
            summary: Some("Numeric body".to_string()),
            attributes: Some(serde_json::json!({"age": 28})),
            ..Default::default()
        });
        let sqlite_store = SqliteKbStore::new(pool.clone());
        sqlite_store.insert_knowledge_entry(entry).await.unwrap();

        let spoke_backed = SpokeBackedKbStore::new(pool);
        let q = KbQuery::new(&world_id).with_canonical_name("Numeric");
        let res = spoke_backed.query(&q).await.unwrap();
        let body = res.items[0].body.as_ref().unwrap();
        let attrs = body.attributes.as_ref().unwrap().as_object().unwrap();
        // Carrier recovery: the integer survives as an integer (not 28.0).
        assert_eq!(attrs["age"].as_i64(), Some(28));
    }

    /// C3: write methods return a read-only error (MCA store is read-only).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_methods_return_read_only_error() {
        let (pool, _dir) = fresh_pool().await;
        let store = SpokeBackedKbStore::new(pool);
        let entry = WorldKbEntry::new("wld_mca", BlockType::Character, "Ghost");
        let err = store.insert_knowledge_entry(entry).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Storage(ref s) if s.contains("read-only")));
    }

    /// C4 (>500-row edge): `SpokeBackedKbStore::query` matches
    /// `SqliteKbStore::query`'s silent 500-row window — NO reject-on-overflow.
    /// The P2 HARD rule forbids introducing the spoke port's reject here.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn query_silently_truncates_at_500_no_reject() {
        let (pool, _dir) = fresh_pool().await;
        // SAFETY: test-only static INSERTs with bind params.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_big', 'wrk_test', 'ctr_test', 'Big', 'big', 'active', 'private', 'manual', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Seed beyond the 500-row window.
        let sqlite = SqliteKbStore::new(pool.clone());
        for i in 0..(nexus_local_db::kb_store::LIST_BY_WORLD_LIMIT + 50) {
            let mut entry = WorldKbEntry::new("wld_big", BlockType::Item, &format!("Row_{i:04}"));
            entry.entry_id = format!("kb_big_{i:04}");
            sqlite.insert_knowledge_entry(entry).await.unwrap();
        }

        let spoke_backed = SpokeBackedKbStore::new(pool);

        // Unfiltered query: both stores MUST return the same silently-truncated
        // window (LIST_BY_WORLD_LIMIT rows) — neither rejects.
        let q = KbQuery::new("wld_big");
        let sqlite_res = sqlite
            .query(&q)
            .await
            .expect("sqlite query must not reject");
        let spoke_res = spoke_backed
            .query(&q)
            .await
            .expect("SpokeBackedKbStore query must NOT reject on >500 rows");
        assert_eq!(
            sqlite_res.items.len(),
            spoke_res.items.len(),
            "both stores truncate to the same window"
        );
        assert_eq!(
            sqlite_res.items.len(),
            usize::try_from(nexus_local_db::kb_store::LIST_BY_WORLD_LIMIT).unwrap(),
        );
        assert_eq!(sqlite_res.total_count, spoke_res.total_count);
        assert_eq!(sqlite_res.has_more, spoke_res.has_more);
        // The actual rows match (same ordering, same canonical_names).
        let sqlite_names: Vec<&str> = sqlite_res
            .items
            .iter()
            .map(|e| e.canonical_name.as_str())
            .collect();
        let spoke_names: Vec<&str> = spoke_res
            .items
            .iter()
            .map(|e| e.canonical_name.as_str())
            .collect();
        assert_eq!(sqlite_names, spoke_names);
    }
}
