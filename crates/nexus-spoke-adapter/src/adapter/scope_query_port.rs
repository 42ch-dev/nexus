//! `ScopeQueryPort` impl — production for both knowledge entries and
//! timeline events (spec §7.4 production-vs-stub matrix).
//!
//! # Knowledge entries (production)
//!
//! [`ScopeQueryPort::list_knowledge_entries`] routes through
//! [`SqliteKbStore::list_by_world_scoped`], applying optional `entry_ids` /
//! `entry_types` filters in SQL before any safety cap. Filtered scopes are not
//! subject to the unfiltered `LIST_BY_WORLD_LIMIT` window. Unfiltered full-world
//! listings reject when the cap is exceeded so orchestrators never receive a
//! silently incomplete scope.
//!
//! Rows are projected through the V1.139 `WorldKbEntry → SpokeKnowledgeEntry`
//! conversion seam (spec §7.1). The world id is taken from `scope.scope_id`.
//!
//! # Timeline events (production — V1.145 P3)
//!
//! [`ScopeQueryPort::list_timeline_events`] queries the `narrative_timeline_events`
//! table (V1.26) via [`list_timeline_events_scoped`], the production local-db
//! read primitive. Scope filters (spec §7.4 timeline Scope filter alignment):
//!
//! | Scope field | SQL filter |
//! |-------------|------------|
//! | `scope_id` | `WHERE world_id = ?` (always) |
//! | `extensions["nexus"]["branch_id"]` | `AND branch_id = ?` (optional) |
//! | `timeline_event_ids` | `AND timeline_event_id IN (json_each(?))` (optional) |
//!
//! Rows are projected through the V1.143 `TimelineEvent → SpokeTimelineEvent`
//! conversion seam (spec §7.1) — the `From<nexus_narrative::TimelineEvent>` impl
//! packs the 7 typed nexus fields into the spoke type's `extensions.nexus`.
//! `timeline_scale` / `fork_id` are no-op pass-through (nexus does not use
//! spoke's fork model yet). Unlike knowledge entries, there is no overflow cap —
//! timeline events are append-ordered and bounded per branch.

use super::NexusBaselineAdapter;
use crate::conversion::world_kb_to_spoke;
use crate::extensions::has_nexus_body;
use crate::{
    KnowledgeEntry, Scope, ScopeExtensionsKey, ScopeQueryPort, SpokeReject, SpokeRejectCode,
    SpokeResult, TimelineEvent,
};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::narrative_gateway::list_timeline_events_scoped;
use serde_json::{json, Map, Value};
impl ScopeQueryPort for NexusBaselineAdapter<'_> {
    /// List the active knowledge entries for the scope's world.
    ///
    /// Routes through [`SqliteKbStore::list_by_world_scoped`] so optional
    /// `entry_ids` / `entry_types` filters are applied in SQL. Unfiltered
    /// full-world listings reject when more than `LIST_BY_WORLD_LIMIT` active
    /// rows exist (no silent truncation).
    fn list_knowledge_entries(&self, scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        let pool = self.pool.clone();
        // Clone the filters so the async block is 'static (the sync trait
        // method takes `&Scope`; the async bridge owns its own copy).
        let world_id = scope.scope_id.clone();
        let entry_ids: Vec<String> = scope.entry_ids.clone();
        let entry_types: Vec<String> = scope.entry_types.clone();

        self.block_on(async move {
            let store = SqliteKbStore::new(pool);
            let scoped = match store
                .list_by_world_scoped(&world_id, &entry_ids, &entry_types)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on list_by_world_scoped: {e}"),
                        json!({ "scope_id": world_id }),
                    );
                }
            };

            if scoped.truncated {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!(
                        "world {world_id} has more active knowledge entries than the safety cap; \
                         narrow the scope with entry_ids or entry_types"
                    ),
                    json!({
                        "scope_id": world_id,
                        "cap": nexus_local_db::kb_store::LIST_BY_WORLD_LIMIT,
                        "truncated": true,
                    }),
                );
            }

            let wire: Vec<KnowledgeEntry> = scoped
                .entries
                .iter()
                // Reuse the V1.139 conversion seam — sole boundary between
                // WorldKbEntry rows and the spoke wire type (spec §7.1); free
                // function in nexus-spoke-adapter since V1.145 P1a.
                .map(world_kb_to_spoke)
                .collect();

            // Carrier-boundary guard (QC2-W003): the reserved `_nexus_body`
            // carrier is MCA-read-path + persist-path only. This orchestrator
            // read path hands spoke entries straight back to the orchestrator,
            // so a leaked carrier would persist into the `extensions` DB
            // column. `world_kb_to_spoke` never sets it, but this debug-only
            // assertion catches a future caller that accidentally stashes one.
            debug_assert!(
                !wire.iter().any(has_nexus_body),
                "ScopeQueryPort::list_knowledge_entries must not carry the _nexus_body carrier \
                 (MCA/persist-only); a leaked carrier would reach the extensions DB column"
            );

            SpokeResult::Ok(wire)
        })
    }

    /// List the timeline events matching the scope (production — V1.145 P3).
    ///
    /// Queries `narrative_timeline_events` via [`list_timeline_events_scoped`]
    /// (the local-db read primitive). Filters: `scope.scope_id` → `world_id`;
    /// `scope.extensions["nexus"]["branch_id"]` → `branch_id`;
    /// `scope.timeline_event_ids` → event-id IN filter. Rows are converted to
    /// spoke [`TimelineEvent`] via the V1.143 conversion seam. See the
    /// module-level docs for the full filter matrix.
    fn list_timeline_events(&self, scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        let pool = self.pool.clone();
        let world_id = scope.scope_id.clone();
        // branch_id rides scope.extensions["nexus"] (spoke-native since 0.6.0),
        // looked up via the typify ScopeExtensionsKey newtype — mirrors the
        // MCA `kb_query_from_scope` extraction (spec §7.5 scope-pushdown).
        let branch_id = scope
            .extensions
            .get(&nexus_scope_key())
            .and_then(|ns| ns.get("branch_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let timeline_event_ids = scope.timeline_event_ids.clone();

        self.block_on(async move {
            let rows = match list_timeline_events_scoped(
                &pool,
                &world_id,
                branch_id.as_deref(),
                &timeline_event_ids,
            )
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on list_timeline_events_scoped: {e}"),
                        json!({ "scope_id": world_id }),
                    );
                }
            };

            // Convert nexus TimelineEvent → spoke TimelineEvent via the V1.143
            // conversion seam (the `From<nexus_narrative::TimelineEvent>` impl
            // defined in nexus-narrative packs the 7 typed nexus fields into
            // extensions.nexus). Sole boundary between the nexus domain row and
            // the spoke wire type; call-boundary invariant §7 preserved (the
            // primitive's nexus type never crosses). The `Vec<TimelineEvent>`
            // annotation pins the `Into` target to the spoke type (nexus
            // TimelineEvent also has a `From` for nexus_contracts::TimelineEvent,
            // so `.into()` is otherwise ambiguous).
            let wire: Vec<TimelineEvent> = rows.into_iter().map(Into::into).collect();
            SpokeResult::Ok(wire)
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Construct the typed `ScopeExtensionsKey` for the `"nexus"` namespace.
///
/// The literal `"nexus"` always satisfies the typify `^[a-z][a-z0-9_-]*$`
/// namespace regex, so construction is infallible at runtime. The newtype does
/// not implement `Borrow<str>`, so `HashMap::get("nexus")` does not compile —
/// this bridges that gap (mirrors `mca_read::nexus_scope_key` /
/// `extensions::nexus_key`).
fn nexus_scope_key() -> ScopeExtensionsKey {
    ScopeExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
}

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
    use crate::ScopeQueryPort;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::store::KbStore;
    use nexus_knowledge::world_kb::{WorldKbBody, WorldKbEntry};
    use nexus_local_db::kb_store::LIST_BY_WORLD_LIMIT;
    use nexus_local_db::narrative_gateway::seed;
    use nexus_local_db::{open_pool, run_migrations};
    use spoke_schemas::timeline_event::TimelineEventExtensionsKey;

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
    async fn list_knowledge_entries_filtered_target_beyond_list_window() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        // Fill the unfiltered list window with filler rows so the seeded
        // entries would be omitted by the old list_by_world + in-memory filter.
        let store = SqliteKbStore::new(pool.clone());
        for i in 0..LIST_BY_WORLD_LIMIT {
            let mut filler =
                WorldKbEntry::new(&world_id, BlockType::Item, &format!("Filler_{i:03}"));
            filler.entry_id = format!("kb_fill_{i:03}");
            store.insert_knowledge_entry(filler).await.unwrap();
        }

        let target_id = seeded[0].entry_id.clone();
        let mut scope = scope_for(&world_id);
        scope.entry_ids = vec![target_id.clone()];

        let adapter = NexusBaselineAdapter::new(pool);
        let entries = match adapter.list_knowledge_entries(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(
            entries.len(),
            1,
            "SQL-scoped filter must find row beyond window"
        );
        assert_eq!(entries[0].entry_id, target_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_unfiltered_truncation_rejects() {
        let (pool, _dir) = fresh_pool().await;
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

        let store = SqliteKbStore::new(pool.clone());
        for i in 0..=LIST_BY_WORLD_LIMIT {
            let mut entry = WorldKbEntry::new("wld_big", BlockType::Item, &format!("Row_{i:03}"));
            entry.entry_id = format!("kb_big_{i:03}");
            store.insert_knowledge_entry(entry).await.unwrap();
        }

        let adapter = NexusBaselineAdapter::new(pool);
        match adapter.list_knowledge_entries(&scope_for("wld_big")) {
            SpokeResult::Ok(_) => panic!("unfiltered cap overflow must reject"),
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(
                    r.message.contains("safety cap"),
                    "reject message should mention cap: {}",
                    r.message
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_returns_all_world_events_as_spoke() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_timeline_events(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let events = match adapter.list_timeline_events(&scope_for(&world_id)) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

        // Unfiltered scope returns every event in the world (3 seeded across
        // 2 branches). The port converts each nexus row to spoke via the
        // V1.143 seam; the spoke type carries world_id + branch_id in
        // extensions.nexus (spec §7.1 conversion contract).
        assert_eq!(events.len(), 3, "unfiltered scope returns all world events");
        let key = nexus_te_key();
        let has_root_branch = events.iter().any(|e| {
            e.extensions
                .get(&key)
                .and_then(|ns| ns.get("branch_id"))
                .and_then(Value::as_str)
                == Some("fbk_root")
        });
        let has_fork_branch = events.iter().any(|e| {
            e.extensions
                .get(&key)
                .and_then(|ns| ns.get("branch_id"))
                .and_then(Value::as_str)
                == Some("fbk_fork")
        });
        assert!(
            has_root_branch,
            "spoke events carry branch_id in extensions.nexus"
        );
        assert!(has_fork_branch, "events from both branches are present");
        // world_id survives the conversion seam (extensions.nexus.world_id).
        assert!(events.iter().all(|e| {
            e.extensions
                .get(&key)
                .and_then(|ns| ns.get("world_id"))
                .and_then(Value::as_str)
                == Some(world_id.as_str())
        }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_filters_by_branch_via_extensions() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_timeline_events(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        // branch_id rides scope.extensions["nexus"] (spoke-native ≥ 0.6.0).
        let scope = timeline_scope(&world_id, Some("fbk_root"), &[]);
        let events = match adapter.list_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

        // Only the two events on fbk_root match; the fbk_fork event is excluded.
        assert_eq!(events.len(), 2, "branch_id filter narrows to one branch");
        let key = nexus_te_key();
        assert!(events.iter().all(|e| {
            e.extensions
                .get(&key)
                .and_then(|ns| ns.get("branch_id"))
                .and_then(Value::as_str)
                == Some("fbk_root")
        }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_filters_by_timeline_event_ids() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_timeline_events(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        // timeline_event_ids is a native Scope field (not under extensions).
        let scope = timeline_scope(&world_id, None, &["evt_tl_1"]);
        let events = match adapter.list_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

        assert_eq!(
            events.len(),
            1,
            "timeline_event_ids filter narrows to one row"
        );
        assert_eq!(events[0].timeline_event_id, "evt_tl_1");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_empty_world_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;

        let adapter = NexusBaselineAdapter::new(pool);
        let events = match adapter.list_timeline_events(&scope_for("wld_empty")) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        // No persisted events for the world → empty success (matches
        // list_knowledge_entries' empty-world behavior; an empty answer is valid).
        assert!(events.is_empty());
    }

    // ── timeline test helpers ──────────────────────────────────────────

    /// Seed a world with timeline events across two branches for scope-filter
    /// tests: `evt_tl_0`/`evt_tl_1` on `fbk_root`, `evt_tl_2` on `fbk_fork`.
    async fn seed_world_with_timeline_events(pool: &sqlx::SqlitePool) -> (String, [String; 3]) {
        seed::world(
            pool,
            "wld_tl",
            "ctr_test",
            "Timeline World",
            "timeline-world",
            "private",
            "manual",
        )
        .await;
        let event_ids = [
            "evt_tl_0".to_string(),
            "evt_tl_1".to_string(),
            "evt_tl_2".to_string(),
        ];
        // Root branch: sequence 0 and 1.
        seed::event(
            pool,
            &event_ids[0],
            "wld_tl",
            "fbk_root",
            "story_advance",
            0,
        )
        .await;
        seed::event(
            pool,
            &event_ids[1],
            "wld_tl",
            "fbk_root",
            "story_advance",
            1,
        )
        .await;
        // Fork branch: sequence 0.
        seed::event(
            pool,
            &event_ids[2],
            "wld_tl",
            "fbk_fork",
            "story_advance",
            0,
        )
        .await;
        ("wld_tl".to_string(), event_ids)
    }

    /// Build a spoke `Scope` for timeline queries: `scope_id` = world, optional
    /// `extensions["nexus"]["branch_id"]`, optional `timeline_event_ids`.
    fn timeline_scope(world_id: &str, branch_id: Option<&str>, event_ids: &[&str]) -> Scope {
        let mut wire = serde_json::Map::new();
        wire.insert("scope_id".into(), Value::String(world_id.to_string()));
        if let Some(branch) = branch_id {
            wire.insert(
                "extensions".into(),
                Value::Object(
                    std::iter::once((
                        "nexus".to_string(),
                        Value::Object(
                            std::iter::once((
                                "branch_id".to_string(),
                                Value::String(branch.to_string()),
                            ))
                            .collect(),
                        ),
                    ))
                    .collect(),
                ),
            );
        }
        if !event_ids.is_empty() {
            wire.insert(
                "timeline_event_ids".into(),
                Value::Array(
                    event_ids
                        .iter()
                        .map(|s| Value::String((*s).to_string()))
                        .collect(),
                ),
            );
        }
        serde_json::from_value(Value::Object(wire))
            .expect("timeline scope wire shape is schema-valid")
    }

    /// Typed `TimelineEventExtensionsKey` for the `"nexus"` namespace — the only
    /// way to read `extensions.nexus` on a spoke `TimelineEvent` (the typify
    /// newtype does not impl `Borrow<str>`).
    fn nexus_te_key() -> TimelineEventExtensionsKey {
        TimelineEventExtensionsKey::try_from("nexus")
            .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
    }

    // ── V1.146 P0: InternalError on DB failure ─────────────────────────

    /// DB failure (dropped table) on list_knowledge_entries surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_entries(&pool).await;
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        match adapter.list_knowledge_entries(&scope_for(&world_id)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR on list_knowledge_entries"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    /// DB failure (dropped table) on list_timeline_events surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_timeline_events(&pool).await;
        sqlx::query("DROP TABLE narrative_timeline_events")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        match adapter.list_timeline_events(&scope_for(&world_id)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR on list_timeline_events"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }
}
