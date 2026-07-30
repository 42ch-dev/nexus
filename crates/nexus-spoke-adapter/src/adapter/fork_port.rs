//! `ForkTimelineQueryPort` impl — production fork-scoped timeline events
//! (spec §7.4, plan Decision 3).
//!
//! # Fork-scoped timeline query
//!
//! [`ForkTimelineQueryPort::list_fork_timeline_events`] maps `scope.fork_id`
//! to the nexus `branch_id` column in `narrative_timeline_events` and the
//! world id from `scope.scope_id`. It delegates to
//! [`list_timeline_events_scoped`] (the same production read primitive used by
//! [`ScopeQueryPort::list_timeline_events`]), applying the fork id as the
//! branch filter and optionally the `scope.timeline_event_ids` narrow.
//!
//! Rows are projected through the V1.143 `TimelineEvent → SpokeTimelineEvent`
//! conversion seam (spec §7.1) — the same `From` impl used by `scope_query_port`.
//!
//! # Validation
//!
//! - `scope.fork_id` is **required** — `None` rejects with `InvalidInput`.
//! - The world (`scope.scope_id`) must exist in `narrative_worlds` —
//!   unknown worlds also reject with `InvalidInput` (unknown fork → reject,
//!   not silent empty, per plan Decision 3 error-mapping rule).
//! - An empty event list for a real world + branch IS `Ok(vec![])` — the
//!   fork exists but has no persisted timeline events yet.

use super::NexusAdapter;
use crate::{
    ForkTimelineQueryPort, Scope, SpokeReject, SpokeRejectCode, SpokeResult, TimelineEvent,
};
use nexus_local_db::narrative_gateway::list_timeline_events_scoped;
use serde_json::{json, Map, Value};

impl ForkTimelineQueryPort for NexusAdapter<'_> {
    fn list_fork_timeline_events(&self, scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        // ── fork_id is required for fork-scoped queries ─────────────────
        let fork_id: String = match &scope.fork_id {
            Some(fid) => fid.to_string(),
            None => {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    "fork_id is required for fork timeline queries; use ScopeQueryPort::list_timeline_events for world-level timeline access",
                    json!({ "scope_id": scope.scope_id }),
                );
            }
        };

        let pool = self.pool.clone();
        let world_id = scope.scope_id.clone();
        let timeline_event_ids = scope.timeline_event_ids.clone();

        self.block_on(async move {
            // ── Validate world existence (unknown fork → reject, not empty) ──
            // SAFETY: runtime query for existence check — SELECT 1 pattern,
            // single bind parameter.
            let world_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM narrative_worlds WHERE world_id = ?)",
            )
            .bind(&world_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

            if !world_exists {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!(
                        "fork scope references unknown world {world_id}; fork_id {fork_id} cannot be resolved"
                    ),
                    json!({ "scope_id": world_id, "fork_id": fork_id }),
                );
            }

            // ── Query timeline events with fork_id as branch filter ─────
            // Decision 3 mapping: scope.fork_id → branch_id in
            // narrative_timeline_events. Pass fork_id as branch_id to the
            // same production read primitive used by ScopeQueryPort.
            let rows = match list_timeline_events_scoped(
                &pool,
                &world_id,
                Some(&fork_id),
                &timeline_event_ids,
            )
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on list_fork_timeline_events: {e}"),
                        json!({ "scope_id": world_id, "fork_id": fork_id }),
                    );
                }
            };

            // ── Convert via V1.143 seam (same as ScopeQueryPort) ────────
            // The `From<nexus_narrative::TimelineEvent>` impl packs the 7
            // typed nexus fields into extensions.nexus (spec §7.1).
            let wire: Vec<TimelineEvent> = rows.into_iter().map(Into::into).collect();
            SpokeResult::Ok(wire)
        })
    }
}

// ── ForkPorts blanket impl ──────────────────────────────────────────────
//
// NexusAdapter implements ForkTimelineQueryPort above. The blanket
// `impl<T: BaselinePorts + ForkTimelineQueryPort> ForkPorts for T`
// (spoke-operations src/adapter/ports.rs:135-142) auto-satisfies
// ForkPorts → as_fork_timeline → Some(self).

// ── Helpers ─────────────────────────────────────────────────────────────

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `scope_query_port.rs` and `knowledge_entry_port.rs`).
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForkPorts;
    use crate::ForkTimelineQueryPort;
    use nexus_local_db::narrative_gateway::seed;
    use nexus_local_db::{open_pool, run_migrations};
    use serde_json::Value;

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    /// Seed a world with timeline events across two branches:
    /// `evt_fk_0`/`evt_fk_1` on `fbk_main`, `evt_fk_2` on `fbk_alt`.
    async fn seed_fork_world(pool: &sqlx::SqlitePool) -> (String, [String; 3]) {
        seed::world(
            pool,
            "wld_fork",
            "ctr_test",
            "Fork World",
            "fork-world",
            "private",
            "manual",
        )
        .await;
        let event_ids = [
            "evt_fk_0".to_string(),
            "evt_fk_1".to_string(),
            "evt_fk_2".to_string(),
        ];
        seed::event(
            pool,
            &event_ids[0],
            "wld_fork",
            "fbk_main",
            "story_advance",
            0,
        )
        .await;
        seed::event(
            pool,
            &event_ids[1],
            "wld_fork",
            "fbk_main",
            "story_advance",
            1,
        )
        .await;
        seed::event(
            pool,
            &event_ids[2],
            "wld_fork",
            "fbk_alt",
            "story_advance",
            0,
        )
        .await;
        ("wld_fork".to_string(), event_ids)
    }

    /// Build a spoke `Scope` with fork_id set (the fork port's required field)
    /// plus optional event_ids filter.
    fn fork_scope(world_id: &str, fork_id: &str, event_ids: &[&str]) -> Scope {
        let mut wire = serde_json::Map::new();
        wire.insert("scope_id".into(), Value::String(world_id.to_string()));
        wire.insert("fork_id".into(), Value::String(fork_id.to_string()));
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
        serde_json::from_value(Value::Object(wire)).expect("fork scope wire shape is schema-valid")
    }

    // ── Branch isolation: fork_id filter returns only that branch's events ──

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_isolates_by_fork_id() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_fork_world(&pool).await;

        let adapter = NexusAdapter::new(pool);

        // Query fbk_main branch — should return 2 events (evt_fk_0, evt_fk_1)
        let scope = fork_scope(&world_id, "fbk_main", &[]);
        let events = match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(
            events.len(),
            2,
            "fbk_main fork returns its 2 events, not the alt-branch event"
        );

        // Query fbk_alt branch — should return 1 event (evt_fk_2)
        let scope_alt = fork_scope(&world_id, "fbk_alt", &[]);
        let events_alt = match adapter.list_fork_timeline_events(&scope_alt) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(
            events_alt.len(),
            1,
            "fbk_alt fork returns exactly its 1 event"
        );
    }

    // ── Empty fork: known world + branch but no events → Ok(vec![]) ─────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_empty_branch_returns_ok_empty() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_fork_world(&pool).await;

        let adapter = NexusAdapter::new(pool);

        // fbk_empty does not exist as a branch in the timeline events table,
        // but the world does exist. Per Decision 3: an empty event list
        // for a real world IS Ok(vec![]).
        let scope = fork_scope(&world_id, "fbk_empty", &[]);
        let events = match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(events.is_empty(), "empty fork returns Ok(vec![])");
    }

    // ── Unknown world → reject (not empty) ─────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_unknown_world_rejects() {
        let (pool, _dir) = fresh_pool().await;

        let adapter = NexusAdapter::new(pool);
        let scope = fork_scope("wld_nonexistent", "fbk_any", &[]);
        match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(_) => panic!("unknown world must reject, not return Ok"),
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "unknown world → InvalidInput"
                );
                assert!(
                    r.message.contains("unknown world"),
                    "reject message should mention unknown world: {}",
                    r.message
                );
            }
        }
    }

    // ── Missing fork_id → reject ───────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_missing_fork_id_rejects() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_fork_world(&pool).await;

        let adapter = NexusAdapter::new(pool);

        // Scope without fork_id
        let scope: Scope = serde_json::from_value(json!({
            "scope_id": world_id,
        }))
        .expect("minimal scope is schema-valid");

        match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(_) => panic!("missing fork_id must reject"),
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "missing fork_id → InvalidInput"
                );
                assert!(
                    r.message.contains("fork_id is required"),
                    "reject message should mention fork_id: {}",
                    r.message
                );
            }
        }
    }

    // ── timeline_event_ids filter narrows ──────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_filters_by_timeline_event_ids() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, event_ids) = seed_fork_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let scope = fork_scope(&world_id, "fbk_main", &[&event_ids[0]]);
        let events = match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(
            events.len(),
            1,
            "timeline_event_ids filter narrows to one event"
        );
        assert_eq!(events[0].timeline_event_id, event_ids[0]);
    }

    // ── V1.143 conversion seam: nexus fields in extensions.nexus ────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_carries_nexus_extensions() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_fork_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let scope = fork_scope(&world_id, "fbk_main", &[]);
        let events = match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert!(!events.is_empty());

        let key = spoke_schemas::timeline_event::TimelineEventExtensionsKey::try_from("nexus")
            .expect("valid namespace key");

        for e in &events {
            let ns = e.extensions.get(&key).unwrap_or_else(|| {
                panic!("event {} missing extensions.nexus", e.timeline_event_id)
            });
            assert_eq!(
                ns.get("world_id").and_then(Value::as_str),
                Some(world_id.as_str()),
                "world_id survives V1.143 seam"
            );
            assert_eq!(
                ns.get("branch_id").and_then(Value::as_str),
                Some("fbk_main"),
                "branch_id matches fork filter"
            );
            // event_type + timeline_status + sequence_no are always present
            // in the V1.143 conversion (spec §7.1 field table).
            assert!(
                ns.get("event_type").and_then(Value::as_str).is_some(),
                "event_type in extensions.nexus"
            );
            assert!(
                ns.get("timeline_status").and_then(Value::as_str).is_some(),
                "timeline_status in extensions.nexus"
            );
            assert!(
                ns.get("sequence_no").is_some(),
                "sequence_no in extensions.nexus"
            );
        }
    }

    // ── DB failure → InternalError ─────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_fork_timeline_events_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _) = seed_fork_world(&pool).await;
        sqlx::query("DROP TABLE narrative_timeline_events")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        let scope = fork_scope(&world_id, "fbk_main", &[]);
        match adapter.list_fork_timeline_events(&scope) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    // ── Compile-time trait satisfaction ────────────────────────────────

    /// Compile-time proof: `NexusAdapter` satisfies `ForkTimelineQueryPort`
    /// and the `ForkPorts` blanket.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn nexus_adapter_satisfies_fork_ports() {
        fn accepts_fork_timeline_port(_: &dyn ForkTimelineQueryPort) {}
        fn accepts_fork_ports(_: &dyn ForkPorts) {}

        let (pool, _dir) = fresh_pool().await;
        let _ = seed_fork_world(&pool).await;
        let adapter = NexusAdapter::new(pool);

        accepts_fork_timeline_port(&adapter);
        accepts_fork_ports(&adapter);
    }
}
