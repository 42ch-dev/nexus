//! MCA (Moment Context Assembly) `WorldKB` read path (V1.145 P2 Option 2).
//!
//! [`SpokeBackedKbStore`] is the [`KbStore`] implementation injected at the
//! `assemble_moment` wiring site ([`NexusAdapter`]'s MCA fetch) so the
//! `WorldKB` read crosses the spoke-adapter boundary: storage rows → spoke
//! `KnowledgeEntry` (via the [`conversion`] seam) → [`KnowledgeEntryRecord`] (via
//! [`spoke_to_knowledge_record`]).
//!
//! # `scope.extensions["nexus"]` (spoke-native, ≥ 0.6.0)
//!
//! spoke 0.6.0's `Scope` gained an `extensions` map. The `KbQuery` filters
//! MCA needs that spoke `Scope` has no native field for (`text_search` /
//! `canonical_name` / `computable` / `limit` / `offset`) ride
//! `scope.extensions["nexus"]` (looked up via the typify `ScopeExtensionsKey`
//! newtype), alongside the `Scope`'s native `entry_types` (mapped from
//! [`KbQuery::block_type`]). This is the architect's original §7.5
//! scope-pushdown design — feasible since 0.6.0 (the prior typed
//! `KbScopeFilters` carrier was a 0.5.0 workaround, now removed). The
//! round-trip is proven by the `scope_extensions_round_trip` smoke test.
//!
//! # Behavior preservation (HARD)
//!
//! [`SpokeBackedKbStore::query`] produces a byte-identical [`KbQueryResult`]
//! to [`SqliteKbStore::query`]: the adapter's scoped read extracts the nexus
//! filters from `scope.extensions["nexus"]`, reconstructs the equivalent
//! [`KbQuery`], and delegates to [`SqliteKbStore::query`] — same silent
//! 500-row window, same in-memory filter + pagination, **no**
//! reject-on-overflow (the spoke `ScopeQueryPort::list_knowledge_entries`
//! reject serves spoke orchestrators, not MCA).
//!
//! [`conversion`]: crate::conversion
//! [`spoke_to_knowledge_record`]: crate::conversion::spoke_to_knowledge_record
//! [`knowledge_record_to_spoke`]: crate::conversion::knowledge_record_to_spoke
use super::NexusAdapter;
use crate::conversion::{spoke_to_knowledge_record, knowledge_record_to_spoke};
use crate::extensions::set_nexus_body;
use crate::{KnowledgeEntry, Scope, ScopeExtensionsKey};
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::query::{KbQuery, KbQueryResult};
use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::SqliteKbStore;
use serde_json::Value;
use sqlx::SqlitePool;

// ── Adapter scoped-read method ──────────────────────────────────────────

/// Result of [`NexusAdapter::list_knowledge_entries_scoped`] — the
/// spoke-wire analogue of [`KbQueryResult`] (items are spoke
/// [`KnowledgeEntry`] instead of [`KnowledgeEntryRecord`]).
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

impl NexusAdapter<'_> {
    /// MCA `WorldKB` read: list active knowledge entries for a world as spoke
    /// wire types, applying the spoke `Scope`'s native `entry_types` plus the
    /// nexus-specific filters carried under `scope.extensions["nexus"]`
    /// (V1.145 P2 — scope-pushdown via extensions, spoke-native since 0.6.0).
    ///
    /// The nexus filters (`text_search` / `canonical_name` / `computable` /
    /// `limit` / `offset`) are extracted from `scope.extensions["nexus"]`, the
    /// scope's native `entry_types` is mapped back to [`KbQuery::block_type`],
    /// and the reconstructed [`KbQuery`] is delegated to
    /// [`SqliteKbStore::query`] — the canonical read path — so the result
    /// matches the direct `query` path's limit/overflow semantics **exactly**
    /// (silent 500-row window; no reject).
    ///
    /// # Limit-semantics decision (HARD)
    ///
    /// This is an async **inherent** method (not the sync spoke
    /// [`ScopeQueryPort::list_knowledge_entries`] trait method). MCA must NOT
    /// inherit the spoke port's reject-on-overflow contract: that reject
    /// serves spoke orchestrators, while MCA needs the silent-truncate-at-500
    /// window that [`SqliteKbStore::query`] provides. Unifying the two paths
    /// would either drop the orchestrator's reject (a regression) or make MCA
    /// reject (breaks byte-identical `assemble_moment` output). They stay
    /// separate — the extensions mechanism changes WHERE the MCA filters ride,
    /// not the limit contract.
    ///
    /// Each [`KnowledgeEntryRecord`] row is projected to a spoke [`KnowledgeEntry`]
    /// via the sole conversion seam ([`knowledge_record_to_spoke`]); the lossless
    /// `_nexus_body` carrier is stashed so the reverse conversion in
    /// [`SpokeBackedKbStore`] recovers the exact body (V1.143 body-fidelity
    /// mechanism, applied to the read path).
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError`] on storage failure (same surface as
    /// [`KbStore::query`]).
    pub async fn list_knowledge_entries_scoped(
        &self,
        scope: &Scope,
    ) -> Result<ScopedKbRead, KbStoreError> {
        let query = kb_query_from_scope(scope);

        let store = SqliteKbStore::new(self.pool.clone());
        let result = store.query(&query).await?;

        let items = result
            .items
            .iter()
            .map(|entry| {
                let mut spoke = knowledge_record_to_spoke(entry);
                // Stash the lossless body carrier so `spoke_to_knowledge_record` in
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

/// `KbStore` implementation backed by [`NexusAdapter`], used at the
/// `assemble_moment` wiring site so the MCA `WorldKB` read crosses the
/// spoke-adapter boundary (V1.145 P2 Option 2).
///
/// [`query`](Self::query) is the production path: it builds a spoke `Scope`
/// (native `entry_types` from [`KbQuery::block_type`] + the nexus-specific
/// filters under `scope.extensions["nexus"]`) from the inbound [`KbQuery`],
/// calls the adapter's scoped read, and converts each spoke [`KnowledgeEntry`]
/// back to a [`KnowledgeEntryRecord`] via [`spoke_to_knowledge_record`]. The result is
/// byte-identical to [`SqliteKbStore::query`] (see the module docs).
///
/// The remaining read methods (`get_knowledge_entry` / `list_by_world` /
/// `get_anchors`) delegate to [`SqliteKbStore`] directly — MCA does not call
/// them, and the daemon CRUD path uses `SqliteKbStore` unchanged. Write methods
/// return [`KbStoreError::Storage`]: this store is read-only for MCA; writes go
/// through the daemon CRUD / spoke orchestrator paths.
pub struct SpokeBackedKbStore {
    /// The adapter the `query` path routes through (storage → spoke conversion).
    adapter: NexusAdapter<'static>,
    /// Pool for delegating the non-MCA read methods to `SqliteKbStore`.
    pool: SqlitePool,
}

impl SpokeBackedKbStore {
    /// Construct from a [`SqlitePool`]. The adapter's port methods are
    /// natively `async fn` (spoke-operations 0.9.1 surface) and await
    /// `SQLite` I/O on the caller's runtime — no runtime handle is captured
    /// and no tokio runtime is required at construction (see
    /// [`NexusAdapter::new`]).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let adapter = NexusAdapter::new(pool.clone());
        Self { adapter, pool }
    }
}

/// Map an inbound [`KbQuery::block_type`] to the spoke `Scope.entry_types`
/// wire string (`snake_case`). MCA sends at most one; `None` → empty.
///
/// Reuses `BlockType`'s serde mapping (the single source of truth for the
/// wire string — `#[serde(rename_all = "snake_case")]`) rather than a
/// hand-written variant table, so newly-added variants map automatically.
/// The serde representation of a unit enum is always a `Value::String`, so the
/// non-string / error arms are unreachable for the current `BlockType`; any
/// divergence is surfaced loudly rather than turned into a silent query miss.
fn block_type_to_entry_types(block_type: Option<nexus_contracts::BlockType>) -> Vec<String> {
    block_type
        .map(|bt| match serde_json::to_value(bt) {
            Ok(serde_json::Value::String(s)) => s,
            Ok(other) => unreachable!(
                "BlockType serialized to non-string {other:?}; \
                 rename_all = snake_case invariant broken — update the wire mapping deliberately"
            ),
            Err(e) => {
                unreachable!("BlockType (unit enum) serialization cannot fail: {e}")
            }
        })
        .into_iter()
        .collect()
}

/// Reverse map: spoke `Scope.entry_types` (`snake_case` wire string) →
/// [`KbQuery::block_type`]. MCA sends at most one element, so only the first
/// is consulted. serde round-trips the `snake_case` string emitted by
/// [`block_type_to_entry_types`] exactly; the legacy `PascalCase` fallback in
/// `nexus-local-db::kb_store::parse_block_type` is not needed here because
/// the MCA `entry_types` always originate from the serde forward map (never
/// from a DB-stored string).
fn entry_type_to_block_type(s: &str) -> Option<nexus_contracts::BlockType> {
    serde_json::from_value::<nexus_contracts::BlockType>(Value::String(s.to_string())).ok()
}

// SAFETY: sqlx SQLite futures borrow the connection pool internally; safe for
// single-threaded SQLite usage within our tokio runtime (mirrors the
// `SqliteKbStore` impl).
#[allow(clippy::future_not_send)]
// `unused_async_trait_impl` (new in clippy 1.98): the read-only write stubs
// (insert/attach/update/delete) return `Err` without awaiting; `async` is by
// `KbStore` trait contract — toolchain-drift debt.
#[allow(clippy::unused_async_trait_impl)]
impl KbStore for SpokeBackedKbStore {
    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        // Build the spoke Scope: native `entry_types` (from block_type) + the
        // nexus-specific filters under `scope.extensions["nexus"]` (spoke-native
        // since 0.6.0). The Scope crosses the spoke boundary carrying every
        // filter MCA needs — no separate typed carrier.
        let scope = scope_from_kb_query(query);

        // Cross the spoke-adapter boundary: storage → spoke KnowledgeEntry,
        // then convert back to the domain type via the sole conversion seam.
        let read = self.adapter.list_knowledge_entries_scoped(&scope).await?;
        // v1.184 P1: the reverse seam is fallible (fails closed on missing
        // owner metadata). Rows originate from storage (always owned), so an
        // error here is a malformed-row signal — surface it as a storage error
        // rather than fabricating a World owner.
        let items: Vec<KnowledgeEntryRecord> = read
            .items
            .into_iter()
            .map(|spoke| {
                spoke_to_knowledge_record(spoke)
                    .map_err(|e| KbStoreError::Storage(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KbQueryResult {
            items,
            total_count: read.total_count,
            has_more: read.has_more,
        })
    }

    async fn insert_knowledge_entry(
        &self,
        _kb: KnowledgeEntryRecord,
    ) -> Result<nexus_knowledge::world_kb::query::KbInsertResult, KbStoreError> {
        Err(read_only_error("insert_knowledge_entry"))
    }

    async fn get_knowledge_entry(&self, entry_id: &str) -> Result<KnowledgeEntryRecord, KbStoreError> {
        // Delegate to SqliteKbStore (MCA does not call this; the daemon CRUD
        // path uses SqliteKbStore directly — unchanged).
        SqliteKbStore::new(self.pool.clone())
            .get_knowledge_entry(entry_id)
            .await
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
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

    async fn update_knowledge_entry(&self, _kb: KnowledgeEntryRecord) -> Result<(), KbStoreError> {
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

// ── Scope ↔ KbQuery bridge (spoke-native scope.extensions, ≥ 0.6.0) ──────

/// Build the typed `ScopeExtensionsKey` for the `"nexus"` namespace.
///
/// The literal `"nexus"` always satisfies the typify `^[a-z][a-z0-9_-]*$`
/// namespace regex, so construction is infallible at runtime. The newtype does
/// not implement `Borrow<str>`, so `HashMap::get("nexus")` does not compile —
/// this bridges that gap (mirrors `extensions::nexus_key` for the entry key).
fn nexus_scope_key() -> ScopeExtensionsKey {
    ScopeExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
}

/// Build a spoke [`Scope`] from an inbound [`KbQuery`] so the `WorldKB` read
/// crosses the spoke boundary carrying every MCA filter — native `entry_types`
/// (from `block_type`) + the 5 nexus-specific filters under
/// `scope.extensions["nexus"]` (V1.145 P2 scope-pushdown, spoke-native ≥ 0.6.0).
///
/// Omitted filters (the `None` arms) are left absent so the reverse extraction
/// in [`kb_query_from_scope`] recovers `None` (round-trips `None ↔ absent`,
/// matching [`KbQuery`] semantics). The serde construction mirrors the wire
/// shape proven by the `scope_extensions_round_trip` smoke test.
fn scope_from_kb_query(query: &KbQuery) -> Scope {
    let entry_types = block_type_to_entry_types(query.block_type);

    let mut nexus_ns = serde_json::Map::new();
    if let Some(ts) = &query.text_search {
        nexus_ns.insert("text_search".into(), Value::String(ts.clone()));
    }
    if let Some(name) = &query.canonical_name {
        nexus_ns.insert("canonical_name".into(), Value::String(name.clone()));
    }
    if let Some(computable) = query.computable {
        nexus_ns.insert("computable".into(), Value::Bool(computable));
    }
    // `usize` → JSON number; read back via `as_u64` on the reverse path
    // (lossless on 64-bit; saturates to u64::MAX on absurd values).
    if let Some(limit) = query.limit {
        nexus_ns.insert(
            "limit".into(),
            Value::from(u64::try_from(limit).unwrap_or(u64::MAX)),
        );
    }
    if let Some(offset) = query.offset {
        nexus_ns.insert(
            "offset".into(),
            Value::from(u64::try_from(offset).unwrap_or(u64::MAX)),
        );
    }

    let mut wire = serde_json::Map::new();
    wire.insert("scope_id".into(), Value::String(query.world_id.clone()));
    if !entry_types.is_empty() {
        wire.insert(
            "entry_types".into(),
            Value::Array(entry_types.into_iter().map(Value::String).collect()),
        );
    }
    if !nexus_ns.is_empty() {
        wire.insert(
            "extensions".into(),
            Value::Object(
                std::iter::once(("nexus".to_string(), Value::Object(nexus_ns))).collect(),
            ),
        );
    }

    serde_json::from_value(Value::Object(wire))
        .expect("KbQuery → Scope wire shape is schema-valid (mirrors scope_extensions_round_trip)")
}

/// Reverse of [`scope_from_kb_query`]: reconstruct the [`KbQuery`] from a spoke
/// [`Scope`] so the adapter's MCA read can delegate to [`SqliteKbStore::query`]
/// (the canonical silent-truncate path). Called by
/// [`NexusAdapter::list_knowledge_entries_scoped`].
///
/// Guarantees byte-identical behavior to a direct [`SqliteKbStore::query`]
/// call: the reconstructed [`KbQuery`] is the exact input that path consumes,
/// so the 500-row window, in-memory filter, and pagination are identical.
fn kb_query_from_scope(scope: &Scope) -> KbQuery {
    let world_id = scope.scope_id.clone();

    // MCA sends at most one entry_type; multiple would be silently dropped by
    // the KbQuery.block_type single-value mapping. debug_assert surfaces a
    // future caller before shipping a silent truncation.
    debug_assert!(
        scope.entry_types.len() <= 1,
        "MCA sends at most one entry_type; multiple would be silently dropped \
         by the KbQuery.block_type single-value mapping"
    );
    let block_type = scope
        .entry_types
        .first()
        .and_then(|s| entry_type_to_block_type(s));

    // Extract the 5 nexus-specific filters from scope.extensions["nexus"]; an
    // absent namespace or absent key recovers `None` (the None ↔ absent round
    // trip from scope_from_kb_query).
    let (text_search, canonical_name, computable, limit, offset) = scope
        .extensions
        .get(&nexus_scope_key())
        .map_or((None, None, None, None, None), |ns| {
            let text_search = ns
                .get("text_search")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let canonical_name = ns
                .get("canonical_name")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let computable = ns.get("computable").and_then(Value::as_bool);
            let limit = ns
                .get("limit")
                .and_then(Value::as_u64)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX));
            let offset = ns
                .get("offset")
                .and_then(Value::as_u64)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX));
            (text_search, canonical_name, computable, limit, offset)
        });

    KbQuery {
        world_id,
        block_type,
        canonical_name,
        text_search,
        computable,
        limit,
        offset,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody;
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world_with_entries(pool: &sqlx::SqlitePool) -> (String, Vec<KnowledgeEntryRecord>) {
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
            let mut entry = KnowledgeEntryRecord::new("wld_mca", block_type, name);
            entry.entry_id = format!("kb_mca_{idx}");
            entry.body = Some(KnowledgeEntryBody {
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
        let mut entry = KnowledgeEntryRecord::new(&world_id, BlockType::Character, "Numeric");
        entry.entry_id = "kb_mca_num".to_string();
        entry.body = Some(KnowledgeEntryBody {
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
        let entry = KnowledgeEntryRecord::new("wld_mca", BlockType::Character, "Ghost");
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
            let mut entry = KnowledgeEntryRecord::new("wld_big", BlockType::Item, &format!("Row_{i:04}"));
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
