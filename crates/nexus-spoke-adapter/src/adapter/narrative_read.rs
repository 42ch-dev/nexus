//! Ordered timeline-read facet — the adapter-owned counterpart to the
//! `ScopeQueryPort::list_timeline_events` (V1.145 P3) unordered read.
//!
//! V1.146 P1 closes the dep-topology deferral (spec §7.4 "Read-path `ScopeQuery`
//! adoption"): narrative timeline ordering now lives behind the
//! `nexus-spoke-adapter` boundary instead of inside the local-db / in-memory
//! gateways. [`NexusAdapter::list_timeline_events_ordered`] is the
//! adapter-owned ordered read; the gateway ordered methods have been removed
//! (Task 3 of this plan dropped `narrative` + `local-db` direct
//! `spoke-operations` ordering deps — this facet is now the canonical ordered
//! read).
//!
//! # Semantics (parity with the V1.143 T2/T3 ordering suites)
//!
//! - Events listed in `ordered_ids` come first **in that order**.
//! - Remaining world/branch-matching events are appended in `sequence_no`
//!   order (the **stable tail**).
//! - Unknown `ordered_ids` / duplicate `ordered_ids` → `SpokeResult::Reject`
//!   (`SpokeRejectCode::InvalidInput`), the validation-error-class reject at
//!   the spoke boundary (consistent with the gateways' former
//!   `SpokeReject → NarrativeError::ValidationError` mapping — the adapter
//!   IS the spoke boundary, so it surfaces `SpokeReject` directly).
//! - The method never mutates stored events (read-only query).
//!
//! # Call-boundary invariant §7
//!
//! The facet's public signature accepts a spoke [`Scope`] + `ordered_ids` and
//! returns spoke [`TimelineEvent`] wire types only. Internally it reuses the
//! local-db read primitive ([`list_timeline_events_scoped`]) for the raw
//! world/branch query, converts nexus domain rows → spoke wire via the V1.143
//! conversion seam, delegates ordering to [`order_timeline_events_by_ids`],
//! and returns the spoke events the helper produced. The nexus domain type
//! never crosses the public boundary, and — because the return type is already
//! spoke — no reverse (lossy) conversion is performed (the gateway paths
//! reordered the *original nexus* events to dodge the lossy reverse; this
//! facet returns the helper's spoke output directly, so the round-trip hazard
//! does not arise).
//!
//! # Scope filters (mirrors `scope_query_port::list_timeline_events`)
//!
//! | Scope field | Filter |
//! |-------------|--------|
//! | `scope_id` | `world_id` (always) |
//! | `extensions["nexus"]["branch_id"]` | `branch_id` (optional) |

use super::NexusAdapter;
use crate::{Scope, ScopeExtensionsKey, SpokeReject, SpokeRejectCode, SpokeResult, TimelineEvent};
use nexus_local_db::narrative_gateway::list_timeline_events_scoped;
use serde_json::{json, Map, Value};
use spoke_operations::order_timeline_events_by_ids;

impl NexusAdapter<'_> {
    /// List timeline events ordered by an explicit `ordered_ids` list, with
    /// the remaining world/branch-matching events appended in `sequence_no`
    /// order (stable tail).
    ///
    /// This is the adapter-owned ordered read (V1.146 P1). It reuses the
    /// [`list_timeline_events_scoped`] production read primitive for the raw
    /// world/branch query and delegates the explicit-id-first ordering to the
    /// spoke [`order_timeline_events_by_ids`] beat-assist helper. Scope
    /// filters: `scope.scope_id` → `world_id`; optional
    /// `scope.extensions["nexus"]["branch_id"]` → `branch_id`.
    ///
    /// Returns spoke [`TimelineEvent`] wire types — the helper output flows
    /// straight back through the boundary, so no reverse nexus conversion runs
    /// (read-only ordering op; no field mutation). Spoke ordering rejects
    /// (unknown/duplicate `ordered_ids`) surface verbatim as
    /// [`SpokeResult::Reject`] (`InvalidInput`).
    ///
    /// # Expected first caller
    ///
    /// No production call site yet (V1.146 P1). Expected first consumer:
    /// Moment Context Assembly timeline ordering. The former gateway
    /// `get_timeline_ordered` methods (and their direct `spoke-operations`
    /// deps) were removed in Task 3 of this plan — this facet is the
    /// canonical ordered read for the workspace.
    ///
    pub async fn list_timeline_events_ordered(
        &self,
        scope: &Scope,
        ordered_ids: &[String],
    ) -> SpokeResult<Vec<TimelineEvent>> {
        let pool = self.pool.clone();
        let world_id = scope.scope_id.clone();
        // branch_id rides scope.extensions["nexus"] (spoke-native ≥ 0.6.0),
        // looked up via the typify ScopeExtensionsKey newtype — mirrors the
        // `scope_query_port::list_timeline_events` extraction.
        let branch_id = scope
            .extensions
            .get(&nexus_scope_key())
            .and_then(|ns| ns.get("branch_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let ordered_ids = ordered_ids.to_vec();

        let rows = match list_timeline_events_scoped(
            &pool,
            &world_id,
            branch_id.as_deref(),
            // No event_ids filter: the ordered view needs the full matching
            // set so the spoke helper can build a correct stable tail.
            &[],
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

        // Sort purely by sequence_no so the spoke helper's "stable tail"
        // (un-listed events) is appended in deterministic sequence order —
        // matching the V1.143 T2/T3 stable-tail semantics the gateways
        // established (`list_timeline_events_scoped` with a branch already
        // returns sequence_no order; the re-sort keeps parity for the
        // no-branch / cross-branch case).
        let mut sorted = rows;
        sorted.sort_by_key(|e| e.sequence_no);

        // Convert nexus TimelineEvent → spoke TimelineEvent via the V1.143
        // conversion seam (the `From<nexus_narrative::TimelineEvent>` impl
        // in nexus-narrative). Call-boundary §7 preserved: the spoke helper
        // receives only spoke wire types. The `Vec<TimelineEvent>`
        // annotation pins the `Into` target to the spoke type.
        let spoke_events: Vec<TimelineEvent> = sorted.into_iter().map(Into::into).collect();

        // Delegate ordering to the spoke beat-assist helper (pure,
        // synchronous — no DB I/O inside). The helper returns the reordered
        // spoke events directly; since our return type is already spoke,
        // we surface its `SpokeResult` verbatim (Ok or Reject) with no
        // reverse conversion — the read-only ordering cannot mutate fields.
        order_timeline_events_by_ids(&spoke_events, &ordered_ids)
    }
}

// ── Helpers (mirror scope_query_port's per-module convention) ──────────

/// Construct the typed `ScopeExtensionsKey` for the `"nexus"` namespace.
///
/// Mirrors `scope_query_port::nexus_scope_key` / `mca_read::nexus_scope_key`:
/// the literal `"nexus"` always satisfies the typify `^[a-z][a-z0-9_-]*$`
/// namespace regex, so construction is infallible at runtime. The newtype does
/// not implement `Borrow<str>`, so `HashMap::get("nexus")` does not compile —
/// this bridges that gap.
fn nexus_scope_key() -> ScopeExtensionsKey {
    ScopeExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
}

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `scope_query_port.rs` / `knowledge_entry_port.rs`).
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
//
// Migrated from the V1.143 T2/T3 ordering suites (`nexus-narrative`'s
// in-memory `InMemoryNarrativeGateway::get_timeline_ordered` tests and
// `nexus-local-db`'s `SqliteNarrativeGateway::get_timeline_ordered` parity
// tests). The adapter is SQLite-backed (`NexusAdapter`), so the T3
// storage path is the natural migration target; the same five named
// assertions are preserved: explicit-ids-first + sequence tail, unknown-id
// reject, duplicate-id reject, shuffled-storage ordering, and the no-mutation
// regression. The gateway methods + their original tests were removed in
// Task 3 of this plan (along with `narrative` + `local-db` direct
// `spoke-operations` ordering deps) — this facet is the sole surviving
// ordered-timeline coverage.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScopeQueryPort;
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

    /// Seed a world + branch events with shuffled `sequence_no` assignment
    /// (id ↛ sequence), proving ordering is independent of storage order.
    ///
    /// Assignment: `evt_1→seq3, evt_2→seq1, evt_3→seq5, evt_4→seq2, evt_5→seq4`.
    async fn seed_world_shuffled(pool: &sqlx::SqlitePool) {
        seed::world(
            pool,
            "wld_ord",
            "ctr_test",
            "Ordered World",
            "ordered-world",
            "private",
            "manual",
        )
        .await;
        seed::event(pool, "evt_1", "wld_ord", "fbk_root", "story_advance", 3).await;
        seed::event(pool, "evt_2", "wld_ord", "fbk_root", "story_advance", 1).await;
        seed::event(pool, "evt_3", "wld_ord", "fbk_root", "story_advance", 5).await;
        seed::event(pool, "evt_4", "wld_ord", "fbk_root", "story_advance", 2).await;
        seed::event(pool, "evt_5", "wld_ord", "fbk_root", "story_advance", 4).await;
    }

    /// Build a spoke `Scope` for the ordered-timeline facet: `scope_id` = world,
    /// optional `extensions["nexus"]["branch_id"]`. (Mirrors the
    /// `scope_query_port` test helper.)
    fn ordered_scope(world_id: &str, branch_id: Option<&str>) -> Scope {
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
        serde_json::from_value(Value::Object(wire))
            .expect("ordered-timeline scope wire shape is schema-valid")
    }

    fn nexus_te_key() -> TimelineEventExtensionsKey {
        TimelineEventExtensionsKey::try_from("nexus")
            .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
    }

    // T-mig-1 (covers T2a/T3a/T4a): explicit ids come first in requested
    // order; remaining events appended in sequence_no order (stable tail).
    // Storage is seeded with shuffled sequence_no assignments to prove the
    // ordering is driven solely by the explicit id list + sequence tail, not
    // by insertion/storage order.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_explicit_ids_then_sequence_tail() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_shuffled(&pool).await;

        let adapter = NexusAdapter::new(pool);
        // Request [evt_3, evt_1, evt_5] explicitly; remaining (evt_2 seq1,
        // evt_4 seq2) form the stable tail in sequence_no order.
        let ordered = match adapter
            .list_timeline_events_ordered(
                &ordered_scope("wld_ord", Some("fbk_root")),
                &[
                    "evt_3".to_string(),
                    "evt_1".to_string(),
                    "evt_5".to_string(),
                ],
            )
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };

        let ids: Vec<&str> = ordered
            .iter()
            .map(|e| e.timeline_event_id.as_str())
            .collect();
        assert_eq!(ordered.len(), 5);
        assert_eq!(
            ids,
            vec!["evt_3", "evt_1", "evt_5", "evt_2", "evt_4"],
            "explicit ids first in requested order, then stable tail in sequence_no order"
        );
        // The scope filter narrowed to fbk_root; every returned event carries
        // that branch_id in extensions.nexus (conversion seam).
        let key = nexus_te_key();
        assert!(ordered.iter().all(|e| {
            e.extensions
                .get(&key)
                .and_then(|ns| ns.get("branch_id"))
                .and_then(Value::as_str)
                == Some("fbk_root")
        }));
    }

    // T-mig-2 (T3-Phase5 regression): ordering must NOT mutate event data.
    // The spoke facet returns the helper's spoke output directly (no reverse
    // nexus conversion), so a title=NULL / summary=NULL seeded event's
    // canonical_name (falls back to the id) and created_at must be byte-identical
    // to the un-ordered spoke conversion of the same row. Compares against the
    // `ScopeQueryPort::list_timeline_events` baseline to catch any future
    // re-introduction of a lossy round-trip in this path.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_preserves_event_fields_no_mutation() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool,
            "wld_nomut",
            "ctr_test",
            "NoMut",
            "nomut",
            "private",
            "manual",
        )
        .await;
        // evt_1 seeded with title=NULL, summary=NULL, created_at=DB default
        // (SQLite datetime('now'), NOT RFC3339). The forward nexus→spoke
        // conversion sets canonical_name = id (the fallback chain) and parses
        // created_at to Option<DateTime<Utc>>. A lossy reverse would corrupt
        // both; returning the helper's spoke output preserves both exactly.
        seed::event(&pool, "evt_1", "wld_nomut", "fbk_root", "story_advance", 1).await;
        seed::event(&pool, "evt_2", "wld_nomut", "fbk_root", "story_advance", 2).await;

        let adapter = NexusAdapter::new(pool.clone());
        let scope = ordered_scope("wld_nomut", Some("fbk_root"));

        // Baseline: the same rows via the un-ordered spoke read.
        let baseline = match adapter.list_timeline_events(&scope).await {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("baseline read rejected: {r:?}"),
        };
        let b1 = baseline
            .iter()
            .find(|e| e.timeline_event_id == "evt_1")
            .expect("evt_1 in baseline");

        let ordered = match adapter
            .list_timeline_events_ordered(&scope, &["evt_1".to_string(), "evt_2".to_string()])
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].timeline_event_id, "evt_1");

        // canonical_name (the spoke "title" carrier) is untouched: for a
        // title=NULL/summary=NULL row it falls back to the event id — the
        // ordering op must not synthesize anything else.
        assert_eq!(ordered[0].canonical_name, b1.canonical_name);
        assert_eq!(
            ordered[0].canonical_name.to_string(),
            "evt_1",
            "canonical_name falls back to id for title=NULL/summary=NULL row"
        );
        // created_at is the original parsed value, untouched by any round-trip.
        assert_eq!(ordered[0].created_at, b1.created_at);
        // description (nexus summary carrier) preserved.
        assert_eq!(ordered[0].description, b1.description);
    }

    // T-mig-3 (T2b/T3b): unknown ordered ids surface as a spoke reject
    // (InvalidInput — the validation-error class at the spoke boundary).
    // No panic; the helper's reject flows through verbatim.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_rejects_unknown_ids() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_shuffled(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let result = adapter
            .list_timeline_events_ordered(
                &ordered_scope("wld_ord", Some("fbk_root")),
                &["evt_1".to_string(), "evt_missing".to_string()],
            )
            .await;
        let SpokeResult::Reject(reject) = result else {
            panic!("expected reject for unknown ordered id, got: {result:?}");
        };
        assert_eq!(reject.code, SpokeRejectCode::InvalidInput);
        assert!(
            reject.message.contains("not present"),
            "reject message should mention unknown ids: {}",
            reject.message
        );
    }

    // T-mig-4 (brief requirement): duplicate ordered ids surface as a spoke
    // reject (InvalidInput). The spoke `order_timeline_events_by_ids` helper
    // rejects `orderedIds contains duplicate timeline_event_id values`; this
    // asserts the facet propagates that reject rather than de-duplicating.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_rejects_duplicate_ids() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_dup", "ctr_test", "Dup", "dup", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_dup", "fbk_root", "story_advance", 1).await;

        let adapter = NexusAdapter::new(pool);
        let result = adapter
            .list_timeline_events_ordered(
                &ordered_scope("wld_dup", Some("fbk_root")),
                &["evt_1".to_string(), "evt_1".to_string()],
            )
            .await;
        let SpokeResult::Reject(reject) = result else {
            panic!("expected reject for duplicate ordered id, got: {result:?}");
        };
        assert_eq!(reject.code, SpokeRejectCode::InvalidInput);
        assert!(
            reject.message.contains("duplicate"),
            "reject message should mention duplicates: {}",
            reject.message
        );
    }

    // T-mig-5 (edge case, qc2 S-002 / qc3 F-003): empty `ordered_ids` returns
    // the full world set in `sequence_no` order. The spoke helper passes its
    // input through verbatim when no explicit ids pin the head (the ordered-id
    // set is empty, so the entire input forms the "stable tail"); the facet
    // pre-sorts by `sequence_no`, so the result is the sequence-ordered view of
    // the shuffled fixture (seq1→evt_2, seq2→evt_4, seq3→evt_1, seq4→evt_5,
    // seq5→evt_3). Asserts no reject + exact sequence ordering.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_empty_ids_returns_full_sequence_sorted() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_shuffled(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let ordered = match adapter
            .list_timeline_events_ordered(&ordered_scope("wld_ord", Some("fbk_root")), &[])
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => {
                panic!("expected ok for empty ordered_ids, got reject: {r:?}")
            }
        };

        let ids: Vec<&str> = ordered
            .iter()
            .map(|e| e.timeline_event_id.as_str())
            .collect();
        assert_eq!(
            ordered.len(),
            5,
            "empty ordered_ids returns the full world set"
        );
        assert_eq!(
            ids,
            vec!["evt_2", "evt_4", "evt_1", "evt_5", "evt_3"],
            "empty ordered_ids → full set in sequence_no order (stable tail)"
        );
    }

    // T-mig-6 (edge case, qc2 S-002 / qc3 F-003): a `scope` without
    // `extensions.nexus.branch_id` widens the world query to ALL branches (no
    // branch filter in `list_timeline_events_scoped`). Events from every branch
    // of the world are returned in stable `sequence_no` order — the facet's
    // pre-sort flattens the cross-branch `(branch_id, sequence_no)` storage
    // order into a single sequence_no order. Uses empty `ordered_ids` to
    // isolate the scope-filter behavior from the explicit-id ordering path.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_no_branch_scope_returns_all_branches_in_sequence_order() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool,
            "wld_nb",
            "ctr_test",
            "NoBranch",
            "no-branch",
            "private",
            "manual",
        )
        .await;
        // Three different branches interleaved by sequence_no:
        //   evt_a → fbk_root,   seq 2
        //   evt_b → fbk_alt,    seq 1
        //   evt_c → fbk_root,   seq 4
        //   evt_d → fbk_other,  seq 3
        seed::event(&pool, "evt_a", "wld_nb", "fbk_root", "story_advance", 2).await;
        seed::event(&pool, "evt_b", "wld_nb", "fbk_alt", "story_advance", 1).await;
        seed::event(&pool, "evt_c", "wld_nb", "fbk_root", "story_advance", 4).await;
        seed::event(&pool, "evt_d", "wld_nb", "fbk_other", "story_advance", 3).await;

        let adapter = NexusAdapter::new(pool);
        // Scope carries world_id only — no `extensions.nexus.branch_id`.
        let ordered = match adapter
            .list_timeline_events_ordered(&ordered_scope("wld_nb", None), &[])
            .await
        {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok for no-branch scope, got reject: {r:?}"),
        };

        let ids: Vec<&str> = ordered
            .iter()
            .map(|e| e.timeline_event_id.as_str())
            .collect();
        assert_eq!(
            ordered.len(),
            4,
            "no-branch scope returns events from all branches"
        );
        assert_eq!(
            ids,
            vec!["evt_b", "evt_a", "evt_d", "evt_c"],
            "no-branch scope → all branches in stable sequence_no order"
        );
        // Confirm the result genuinely spans branches (not silently narrowed).
        let key = nexus_te_key();
        let branches: Vec<String> = ordered
            .iter()
            .map(|e| {
                e.extensions
                    .get(&key)
                    .and_then(|ns| ns.get("branch_id"))
                    .and_then(Value::as_str)
                    .map_or_else(
                        || {
                            panic!(
                                "missing extensions.nexus.branch_id on {}",
                                e.timeline_event_id
                            )
                        },
                        str::to_owned,
                    )
            })
            .collect();
        let mut unique = branches.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique,
            vec![
                "fbk_alt".to_string(),
                "fbk_other".to_string(),
                "fbk_root".to_string()
            ],
            "result spans all three branches: {branches:?}"
        );
    }

    // ── V1.146 P0: InternalError on DB failure ─────────────────────────

    /// DB failure (dropped table) on `list_timeline_events_ordered` surfaces
    /// `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_timeline_events_ordered_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_shuffled(&pool).await;
        sqlx::query("DROP TABLE narrative_timeline_events")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        match adapter
            .list_timeline_events_ordered(
                &ordered_scope("wld_ord", Some("fbk_root")),
                &["evt_1".to_string()],
            )
            .await
        {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR on list_timeline_events_ordered"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }
}
