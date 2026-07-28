//! `ScopeQueryPort` impl — production for knowledge entries, stub for
//! timeline events (spec §7.4 production-vs-stub matrix).
//!
//! # Knowledge entries (production)
//!
//! [`ScopeQueryPort::list_knowledge_entries`] routes through the existing
//! [`SqliteKbStore::list_by_world`] (the production `kb_key_blocks`
//! query) and reuses the V1.139 `WorldKbEntry → SpokeKnowledgeEntry`
//! conversion seam (spec §7.1) as the sole wire-type projection. The
//! world id is taken from `scope.scope_id` (the spoke `Scope` is a
//! protocol-neutral opaque selector — nexus maps world ids through
//! that slot). Optional `entry_ids` / `entry_types` filters are
//! applied at the boundary; this is consistent with how the existing
//! `KbStore::query` path applies optional filters in-memory.
//!
//! # Timeline events (stub)
//!
//! [`ScopeQueryPort::list_timeline_events`] is a documented stub: nexus
//! has no persisted `TimelineEvent` storage today (`nexus-narrative`
//! holds timeline events in-memory inside active narrative sessions),
//! so the query surface returns the documented empty set rather than
//! fabricating events from session state.
//!
//! ## Roadmap trigger (timeline events)
//!
//! Spec §7.4 stub matrix — upgrade is a roadmap item tracked via the
//! iteration compass "Roadmap Next" rows. The upgrade path is: add a
//! `timeline_events` persistence layer; teach the orchestrator which
//! scope filters (`timeline_event_ids`, `timeline_scale`, `fork_id`)
//! route to SQL vs in-memory session state.

use super::NexusBaselineAdapter;
use crate::kb_store::SqliteKbStore;
use nexus_knowledge::world_kb::store::KbStore;
use nexus_knowledge::world_kb::WorldKbEntry;
use nexus_spoke_adapter::{
    KnowledgeEntry, Scope, ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult, TimelineEvent,
};
use serde_json::{json, Map, Value};
impl ScopeQueryPort for NexusBaselineAdapter {
    fn list_knowledge_entries(&self, scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        let pool = self.pool.clone();
        // Clone the filters so the async block is 'static (the sync trait
        // method takes `&Scope`; the async bridge owns its own copy).
        let world_id = scope.scope_id.clone();
        let entry_ids: Vec<String> = scope.entry_ids.clone();
        let entry_types: Vec<String> = scope.entry_types.clone();

        self.block_on(async move {
            let store = SqliteKbStore::new(pool);
            let world_entries: Vec<WorldKbEntry> = match store.list_by_world(&world_id).await {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("storage error on list_by_world: {e}"),
                        json!({ "scope_id": world_id }),
                    );
                }
            };

            // Apply optional boundary filters (entry_ids / entry_types).
            // Mirrors the KbStore::query in-memory filter pattern — the
            // world dataset is small enough that fetching all active rows
            // and projecting is preferable to building dynamic SQL for
            // every port call.
            //
            // Filtering happens AFTER the V1.139 conversion seam so the
            // `entry_types` matcher compares against the same
            // `KnowledgeEntry.entry_type` string callers see on the wire
            // (the conversion seam already projects BlockType →
            // snake_case `entry_type`; we don't need to duplicate that
            // mapping here).
            let filtered: Vec<KnowledgeEntry> = world_entries
                .into_iter()
                // Reuse the V1.139 conversion seam — sole boundary
                // between WorldKbEntry rows and the spoke wire type
                // (spec §7.1).
                .map(KnowledgeEntry::from)
                .filter(|entry| {
                    if !entry_ids.is_empty() && !entry_ids.contains(&entry.entry_id) {
                        return false;
                    }
                    if !entry_types.is_empty() && !entry_types.contains(&entry.entry_type) {
                        return false;
                    }
                    true
                })
                .collect();

            SpokeResult::Ok(filtered)
        })
    }

    /// Stub — returns the documented empty list (spec §7.4).
    ///
    /// Nexus has no persisted `TimelineEvent` storage today; the full
    /// impl is a roadmap item triggered when timeline persistence
    /// lands. See the module-level docs.
    fn list_timeline_events(&self, _scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        SpokeResult::Ok(Vec::new())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `knowledge_entry_port.rs`).
fn reject<T>(code: SpokeRejectCode, message: impl Into<String>, details: Value) -> SpokeResult<T> {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::{WorldKbBody, WorldKbEntry};
    use nexus_spoke_adapter::ScopeQueryPort;

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
             VALUES ('wld_scope', 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')",
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
            let mut entry = WorldKbEntry::new("wld_scope", block_type, name);
            // Distinct entry ids so the `entry_ids` filter test has signal.
            entry.entry_id = format!("kb_scope_{idx}");
            entry.body = Some(WorldKbBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            store.insert_knowledge_entry(entry.clone()).await.unwrap();
            seeded.push(entry);
        }
        ("wld_scope".to_string(), seeded)
    }

    fn scope_for(world_id: &str) -> Scope {
        // Round-trip-safe construction: spoke's `Scope` carries the world
        // id in `scope_id` (the protocol-neutral opaque selector). The
        // minimal scope has just `scope_id`; other filters default empty.
        serde_json::from_value(json!({
            "scope_id": world_id,
        }))
        .expect("minimal scope is schema-valid")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_returns_world_entries_as_wire() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entries = match adapter.list_knowledge_entries(&scope_for(&world_id)) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(
            entries.len(),
            seeded.len(),
            "all active world entries return"
        );
        let canonical_names: Vec<String> = entries
            .iter()
            .map(|e| e.canonical_name.to_string())
            .collect();
        assert!(canonical_names.contains(&"Alice".to_string()));
        assert!(canonical_names.contains(&"Atlantis".to_string()));
        assert!(canonical_names.contains(&"Anvil".to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_empty_world_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entries = match adapter.list_knowledge_entries(&scope_for("wld_empty")) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        // The world does not exist, but list_by_world treats it as an
        // empty result (no rows match the WHERE clause). The port
        // surfaces that as an empty success rather than a reject — the
        // spoke scope query is "what entries exist in this scope?" and
        // an empty answer is valid.
        assert!(entries.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_applies_entry_ids_filter() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        let target_id = seeded[1].entry_id.clone();
        let mut scope = scope_for(&world_id);
        scope.entry_ids = vec![target_id.clone()];

        let adapter = NexusBaselineAdapter::new(pool);
        let entries = match adapter.list_knowledge_entries(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(entries.len(), 1, "entry_ids filter narrows to one row");
        assert_eq!(entries[0].entry_id, target_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_applies_entry_types_filter() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _seeded) = seed_world_with_entries(&pool).await;

        let mut scope = scope_for(&world_id);
        scope.entry_types = vec!["character".to_string()];

        let adapter = NexusBaselineAdapter::new(pool);
        let entries = match adapter.list_knowledge_entries(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(entries.len(), 1, "only the character row matches");
        assert_eq!(entries[0].canonical_name.to_string(), "Alice");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_stub_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;
        let adapter = NexusBaselineAdapter::new(pool);

        let events = match adapter.list_timeline_events(&scope_for("wld_any")) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("stub must return Ok: {r:?}"),
        };
        assert!(events.is_empty(), "stub returns the documented empty list");
    }
}
