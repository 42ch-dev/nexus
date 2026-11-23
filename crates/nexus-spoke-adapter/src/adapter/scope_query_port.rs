// contract — docs/specs/architecture/spoke-adapter-architecture.md.
//! Knowledge/timeline query ports — production for both knowledge entries
//! (via `KnowledgeQueryPort`) and timeline events (via `ScopeQueryPort`);
//! spec §7.4 production-vs-stub matrix.
//!
//! # Knowledge entries (production)
//!
//! [`KnowledgeQueryPort::list_knowledge_entries`] uses the admitted
//! [`SqliteKbStore::list_by_owner_keyset`] reader for unfiltered scopes
//! (cap-checked: more than `LIST_BY_WORLD_LIMIT` eligible rows rejects, so
//! orchestrators never receive a silently incomplete scope). Filtered scopes
//! (`entry_ids` / `entry_types`) load the complete admitted set via
//! `list_by_owner_complete` and apply those client filters in memory;
//! filtered scopes are not subject to the unfiltered cap window.
//!
//! Rows are projected through the V1.139 `KnowledgeEntryRecord → SpokeKnowledgeEntry`
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

use super::NexusAdapter;
use crate::conversion::knowledge_entry::block_type_to_entry_type;
use crate::conversion::knowledge_record_to_spoke;
use crate::extensions::has_nexus_body;
use crate::{
    KnowledgeEntry, KnowledgeQueryPort, Scope, ScopeQueryPort, SpokeReject, SpokeRejectCode,
    SpokeResult, TimelineEvent,
};
use async_trait::async_trait;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef;
use nexus_local_db::kb_store::{SqliteKbStore, LIST_BY_WORLD_LIMIT};
use nexus_local_db::narrative_gateway::list_timeline_events_scoped;
use serde_json::{json, Map, Value};
use spoke_schemas::ExtensionKey;
#[async_trait]
impl KnowledgeQueryPort for NexusAdapter<'_> {
    /// List the active knowledge entries for the scope's world.
    ///
    /// v1.191 P1 T8 (durable §4.1/§4.2): the listing is served from the
    /// adapter's **request-bound selection**, so the §4.2 visibility conjunct
    /// is applied in SQL before the listing window and only eligible rows ever
    /// count towards [`LIST_BY_WORLD_LIMIT`]. The spoke `Scope` can only narrow
    /// that selection:
    ///
    /// - a wire `scope_id` outside the bound containers contributes nothing
    ///   (the same empty answer the storage selection gives for a foreign
    ///   container), and
    /// - a wire `viewpoint` that is not the bound resolved holder is refused
    ///   ([`NexusAdapter::refuse_foreign_scope_viewpoint`]).
    ///
    /// Unfiltered full-world listings still reject when more than
    /// `LIST_BY_WORLD_LIMIT` *eligible* rows exist (no silent truncation); a
    /// scoped `entry_ids` / `entry_types` filter narrows the eligible set and
    /// is not capped, exactly as before.
    async fn list_knowledge_entries(&self, scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        let selection = match self.require_read_scope("list_knowledge_entries") {
            Ok(selection) => selection,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if let Err(reject) = self.refuse_foreign_scope_viewpoint(scope) {
            return SpokeResult::Reject(reject);
        }
        let world_id = scope.scope_id.clone();
        let world_owner = KnowledgeOwnerRef::world(world_id.as_str());
        if !selection.containers().contains(&world_owner) {
            return SpokeResult::Ok(Vec::new());
        }
        let entry_ids: Vec<String> = scope.entry_ids.clone().unwrap_or_default();
        let entry_types: Vec<String> = scope.entry_types.clone().unwrap_or_default();

        let store = SqliteKbStore::new(self.pool.clone());
        let entries = if entry_ids.is_empty() && entry_types.is_empty() {
            let probe = u32::try_from(LIST_BY_WORLD_LIMIT)
                .map_or(u32::MAX, |limit| limit.saturating_add(1));
            let rows = match store
                .list_by_owner_keyset(&world_owner, None, probe, selection)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on list_by_owner_keyset: {e}"),
                        json!({ "scope_id": world_id }),
                    );
                }
            };
            let cap = usize::try_from(LIST_BY_WORLD_LIMIT).unwrap_or(usize::MAX);
            if rows.len() > cap {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!(
                        "world {world_id} has more active knowledge entries than the safety cap; \
                         narrow the scope with entry_ids or entry_types"
                    ),
                    json!({
                        "scope_id": world_id,
                        "cap": LIST_BY_WORLD_LIMIT,
                        "truncated": true,
                    }),
                );
            }
            rows
        } else {
            let rows = match store.list_by_owner_complete(&world_owner, selection).await {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on list_by_owner_complete: {e}"),
                        json!({ "scope_id": world_id }),
                    );
                }
            };
            rows.into_iter()
                .filter(|row| {
                    (entry_ids.is_empty() || entry_ids.contains(&row.entry_id))
                        && (entry_types.is_empty()
                            || entry_types.contains(&block_type_to_entry_type(row.block_type)))
                })
                .collect()
        };

        let wire: Vec<KnowledgeEntry> = entries.iter().map(knowledge_record_to_spoke).collect();

        debug_assert!(
            !wire.iter().any(has_nexus_body),
            "KnowledgeQueryPort::list_knowledge_entries must not carry the _nexus_body carrier \
             (MCA/persist-only); a leaked carrier would reach the extensions DB column"
        );

        SpokeResult::Ok(wire)
    }
}

#[async_trait]
impl ScopeQueryPort for NexusAdapter<'_> {
    /// List the timeline events matching the scope (production — V1.145 P3).
    ///
    /// Queries `narrative_timeline_events` via [`list_timeline_events_scoped`]
    /// (the local-db read primitive). Filters: `scope.scope_id` → `world_id`;
    /// `scope.extensions["nexus"]["branch_id"]` → `branch_id`;
    /// `scope.timeline_event_ids` → event-id IN filter. Rows are converted to
    /// spoke [`TimelineEvent`] via the V1.143 conversion seam. See the
    /// module-level docs for the full filter matrix.
    async fn list_timeline_events(&self, scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        let pool = self.pool.clone();
        let world_id = scope.scope_id.clone();
        let branch_id = scope
            .extensions
            .as_ref()
            .and_then(|ext| ext.get(&nexus_scope_key()))
            .and_then(|ns| ns.get("branch_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let timeline_event_ids = scope.timeline_event_ids.clone().unwrap_or_default();

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

        let wire: Vec<TimelineEvent> = rows.into_iter().map(Into::into).collect();
        SpokeResult::Ok(wire)
    }
}

///
/// The literal `"nexus"` always satisfies the typify `^[a-z][a-z0-9_-]*$`
/// namespace regex, so construction is infallible at runtime. The newtype does
/// not implement `Borrow<str>`, so `HashMap::get("nexus")` does not compile —
/// this bridges that gap (mirrors `mca_read::nexus_scope_key` /
/// `extensions::nexus_key`).
fn nexus_scope_key() -> ExtensionKey {
    ExtensionKey::try_from("nexus")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnowledgeQueryPort, ScopeQueryPort};
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE};
    use nexus_knowledge::world_kb::store::KbStore;
    use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeReadScope};
    use nexus_local_db::kb_store::LIST_BY_WORLD_LIMIT;
    use nexus_local_db::narrative_gateway::seed;
    use nexus_local_db::{open_pool, run_migrations};
    use spoke_schemas::timeline_event::ExtensionKey;

    /// Register the fixture creator's holder so a governed row can be stored
    /// (the native `holder_entry_id` column is FK-bound to the registry, and
    /// these selections authorize no holder — so the row is private to them).
    async fn register_creator_holder(pool: &sqlx::SqlitePool) -> String {
        let mut tx = pool.begin().await.unwrap();
        let holder = nexus_local_db::holders::ensure_creator_holder_in_tx(&mut tx, "ctr_test")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        holder
    }

    /// A KE-capable adapter whose selection authorizes exactly `world_id`
    /// (`CreatorManagement` over one World container, no known-governance
    /// holders), so these tests exercise the request-bound listing.
    fn scoped(pool: sqlx::SqlitePool, world_id: &str) -> NexusAdapter<'static> {
        NexusAdapter::new(
            pool,
            KnowledgeReadScope::creator_management(
                vec![KnowledgeOwnerRef::world(world_id)],
                Vec::new(),
            ),
        )
    }

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world_with_entries(
        pool: &sqlx::SqlitePool,
    ) -> (String, Vec<KnowledgeEntryRecord>) {
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
            let mut entry = KnowledgeEntryRecord::new("wld_scope", block_type, name);
            entry.entry_id = format!("kb_scope_{idx}");
            entry.body = Some(KnowledgeEntryBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            store.insert_knowledge_entry(entry.clone()).await.unwrap();
            seeded.push(entry);
        }
        ("wld_scope".to_string(), seeded)
    }

    fn scope_for(world_id: &str) -> Scope {
        serde_json::from_value(json!({
            "scope_id": world_id,
        }))
        .expect("minimal scope is schema-valid")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_returns_world_entries_as_wire() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        let adapter = scoped(pool, &world_id);
        let entries = match adapter.list_knowledge_entries(&scope_for(&world_id)).await {
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

        let adapter = scoped(pool, "wld_empty");
        let entries = match adapter
            .list_knowledge_entries(&scope_for("wld_empty"))
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(entries.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_applies_entry_ids_filter() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        let target_id = seeded[1].entry_id.clone();
        let mut scope = scope_for(&world_id);
        scope.entry_ids = Some(vec![target_id.clone()]);

        let adapter = scoped(pool, &world_id);
        let entries = match adapter.list_knowledge_entries(&scope).await {
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
        scope.entry_types = Some(vec!["character".to_string()]);

        let adapter = scoped(pool, &world_id);
        let entries = match adapter.list_knowledge_entries(&scope).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(entries.len(), 1, "only the character row matches");
        assert_eq!(entries[0].canonical_name.to_string(), "Alice");
    }

    /// v1.191 P1 T8: the listing is served from the bound selection, so a
    /// private row of another holder never appears — and a world outside the
    /// selection yields nothing at all (the wire scope cannot widen it).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_stays_inside_the_bound_selection() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _seeded) = seed_world_with_entries(&pool).await;

        let mut private = KnowledgeEntryRecord::new(&world_id, BlockType::Character, "HiddenNote");
        private.entry_id = "kb_hidden_scope".to_string();
        private.holder_entry_id = Some(register_creator_holder(&pool).await);
        private.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
        SqliteKbStore::new(pool.clone())
            .insert_knowledge_entry(private)
            .await
            .unwrap();

        let adapter = scoped(pool, &world_id);
        let entries = match adapter.list_knowledge_entries(&scope_for(&world_id)).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(
            entries.iter().all(|e| e.entry_id != "kb_hidden_scope"),
            "a foreign-holder private row must not be listed: {entries:?}"
        );

        let foreign = match adapter
            .list_knowledge_entries(&scope_for("wld_foreign"))
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(foreign.is_empty(), "foreign world must contribute nothing");
    }

    /// v1.191 P1 T8 (durable §4.1/§9): a wire `Scope.viewpoint` is a client
    /// value — it must equal the bound resolved holder, and a management
    /// selection carries none, so any supplied viewpoint is refused.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_refuses_foreign_scope_viewpoint() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _seeded) = seed_world_with_entries(&pool).await;

        let adapter = scoped(pool.clone(), &world_id);
        let mut scope = scope_for(&world_id);
        scope.viewpoint = Some(
            spoke_schemas::ScopeViewpoint::try_from("hld_spoofed")
                .expect("non-empty viewpoint is schema-valid"),
        );

        match adapter.list_knowledge_entries(&scope).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput, "got {r:?}");
                assert_eq!(
                    r.details.as_ref().and_then(|d| d.get("scope_id")),
                    Some(&Value::String(world_id.clone()))
                );
                assert!(
                    !r.message.contains("hld_spoofed"),
                    "the client viewpoint must not be echoed as authority: {}",
                    r.message
                );
            }
            SpokeResult::Ok(_) => panic!("a foreign viewpoint must reject"),
        }

        let host = NexusAdapter::new_host(pool.clone());
        match host.list_knowledge_entries(&scope_for("wld_any")).await {
            SpokeResult::Reject(r) => assert_eq!(
                r.details.as_ref().and_then(|d| d.get("read_scope_missing")),
                Some(&Value::Bool(true)),
                "got {r:?}"
            ),
            SpokeResult::Ok(_) => panic!("host-only adapter must not list knowledge"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_filtered_target_beyond_list_window() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, seeded) = seed_world_with_entries(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        for i in 0..LIST_BY_WORLD_LIMIT {
            let mut filler =
                KnowledgeEntryRecord::new(&world_id, BlockType::Item, &format!("Filler_{i:03}"));
            filler.entry_id = format!("kb_fill_{i:03}");
            filler.created_at = "2000-01-01T00:00:00Z".to_string();
            store.insert_knowledge_entry(filler).await.unwrap();
        }

        let target_id = seeded[0].entry_id.clone();
        sqlx::query(
            "UPDATE kb_key_blocks SET created_at = '2099-01-01T00:00:00Z' \
             WHERE key_block_id = ?",
        )
        .bind(&target_id)
        .execute(&pool)
        .await
        .unwrap();

        let window = store
            .list_by_owner_keyset(
                &KnowledgeOwnerRef::world(&world_id),
                None,
                u32::try_from(LIST_BY_WORLD_LIMIT).unwrap(),
                &KnowledgeReadScope::creator_management(
                    vec![KnowledgeOwnerRef::world(&world_id)],
                    Vec::new(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(window.len(), usize::try_from(LIST_BY_WORLD_LIMIT).unwrap());
        assert!(
            !window.iter().any(|row| row.entry_id == target_id),
            "target {target_id} must lie beyond the first unfiltered \
             {LIST_BY_WORLD_LIMIT}-row window"
        );

        let mut scope = scope_for(&world_id);
        scope.entry_ids = Some(vec![target_id.clone()]);

        let adapter = scoped(pool, &world_id);
        let entries = match adapter.list_knowledge_entries(&scope).await {
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
            let mut entry =
                KnowledgeEntryRecord::new("wld_big", BlockType::Item, &format!("Row_{i:03}"));
            entry.entry_id = format!("kb_big_{i:03}");
            store.insert_knowledge_entry(entry).await.unwrap();
        }

        let adapter = scoped(pool, "wld_big");
        match adapter.list_knowledge_entries(&scope_for("wld_big")).await {
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

        let adapter = NexusAdapter::new_host(pool);
        let events = match adapter.list_timeline_events(&scope_for(&world_id)).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

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

        let adapter = NexusAdapter::new_host(pool);
        let scope = timeline_scope(&world_id, Some("fbk_root"), &[]);
        let events = match adapter.list_timeline_events(&scope).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

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

        let adapter = NexusAdapter::new_host(pool);
        let scope = timeline_scope(&world_id, None, &["evt_tl_1"]);
        let events = match adapter.list_timeline_events(&scope).await {
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

        let adapter = NexusAdapter::new_host(pool);
        let events = match adapter.list_timeline_events(&scope_for("wld_empty")).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(events.is_empty());
    }

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

    /// Typed `ExtensionKey` for the `"nexus"` namespace — the only
    /// way to read `extensions.nexus` on a spoke `TimelineEvent` (the typify
    /// newtype does not impl `Borrow<str>`).
    fn nexus_te_key() -> ExtensionKey {
        ExtensionKey::try_from("nexus")
            .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_knowledge_entries_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_entries(&pool).await;
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = scoped(pool, &world_id);
        match adapter.list_knowledge_entries(&scope_for(&world_id)).await {
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

    /// DB failure (dropped table) on `list_timeline_events` surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_world_with_timeline_events(&pool).await;
        sqlx::query("DROP TABLE narrative_timeline_events")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new_host(pool);
        match adapter.list_timeline_events(&scope_for(&world_id)).await {
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
