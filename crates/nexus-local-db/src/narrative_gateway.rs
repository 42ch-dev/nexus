//! SQLite-backed `NarrativeGateway` implementation.
//!
//! Implements the `NarrativeGateway` trait from `nexus-narrative` using
//! the workspace `state.db` pool. Uses compile-time checked `sqlx` queries
//! for all static SQL. Fork info is not stored in this V1.26 iteration
//! (forks are in-memory only), so `is_fork` always returns `false`.
//!
//! # Test helpers
//!
//! The [`seed`] submodule provides async functions to insert test data
//! (worlds, timeline events) into the database for integration tests.

use nexus_narrative::timeline_event::{SpokeTimelineEvent, TimelineEvent};
use nexus_narrative::{
    EventSnapshot, NarrativeContext, NarrativeError, NarrativeGateway, NarrativeQuery,
    TimelinePosition, WorldState,
};
use nexus_spoke_adapter::{order_timeline_events_by_ids, SpokeReject, SpokeResult};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Test helpers for seeding narrative data into the database.
///
/// These functions are intended for tests and development fixtures only.
/// They create the necessary FK parent rows (e.g. creators) if missing.
pub mod seed {
    use super::super::seed_shared;
    use sqlx::SqlitePool;

    /// Seed a test world row into `narrative_worlds`.
    ///
    /// Delegates to the shared `seed_shared::world` helper.
    pub async fn world(
        pool: &SqlitePool,
        world_id: &str,
        owner_creator_id: &str,
        title: &str,
        slug: &str,
        visibility: &str,
        time_policy: &str,
    ) {
        seed_shared::world(
            pool,
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
        )
        .await;
    }

    /// Seed a test timeline event row into `narrative_timeline_events`.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails (e.g., FK violation).
    pub async fn event(
        pool: &SqlitePool,
        event_id: &str,
        world_id: &str,
        branch_id: &str,
        event_type: &str,
        sequence_no: i64,
    ) {
        sqlx::query!(
            r#"INSERT INTO narrative_timeline_events
                (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, metadata_json)
               VALUES (?, ?, ?, ?, 'provisional', ?, '{}')"#,
            event_id,
            world_id,
            branch_id,
            event_type,
            sequence_no,
        )
        .execute(pool)
        .await
        .unwrap();
    }
}

/// SQLite-backed read-only narrative gateway.
///
/// Holds an `Arc<SqlitePool>` shared per active workspace. Construct once
/// at daemon boot and inject as `Arc<dyn NarrativeGateway>`.
pub struct SqliteNarrativeGateway {
    pool: Arc<SqlitePool>,
}

impl SqliteNarrativeGateway {
    /// Create a new gateway backed by the given pool.
    ///
    /// The pool is wrapped in `Arc` for cheap cloning if needed.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Get timeline events ordered by an explicit ID list, delegating the
    /// ordering to the spoke `order_timeline_events_by_ids` beat-assist helper
    /// (V1.143 P0 T3 — sqlite parity with `InMemoryNarrativeGateway::get_timeline_ordered`).
    ///
    /// Events listed in `ordered_ids` come first (in that order); any remaining
    /// events matching the world/branch filter are appended in `sequence_no`
    /// order (the stable tail). This is a **different purpose** from
    /// [`get_timeline`](NarrativeGateway::get_timeline), which sorts purely by
    /// `sequence_no` — both paths coexist (dual path, not replacement).
    ///
    /// This is an inherent method (not on the `NarrativeGateway` trait),
    /// mirroring the in-memory gateway's T2 design choice to avoid trait
    /// ripple. Unlike the sync in-memory version, this method is `async`
    /// (DB I/O); the spoke conversion + ordering inside is synchronous.
    ///
    /// Spoke reject cases (duplicate `ordered_ids`, unknown ids, duplicate
    /// event ids in storage) surface as [`NarrativeError::ValidationError`] —
    /// the spoke `SpokeReject` payload is surfaced as `{code}: {message}` for
    /// diagnosability (consistent with the `map_spoke_reject` pattern in
    /// `nexus-knowledge`). The gateway never panics on a reject.
    ///
    /// # Call-boundary invariant §7
    ///
    /// The spoke helper receives only the converted spoke wire type
    /// ([`SpokeTimelineEvent`]); nexus→spoke conversion happens before the
    /// call, spoke→nexus conversion after. The nexus domain type never
    /// crosses the boundary.
    ///
    /// # Expected first caller
    ///
    /// No production call site yet (V1.143 P0). Expected first consumer:
    /// Moment Context Assembly timeline ordering, or a future
    /// `ScopeQueryPort`-backed ordered-timeline path.
    ///
    /// # Errors
    ///
    /// - `Storage` — underlying `get_timeline` DB read failed.
    /// - `ValidationError` — spoke ordering rejected (unknown/duplicate ids).
    pub async fn get_timeline_ordered(
        &self,
        world_id: &str,
        branch_id: Option<&str>,
        ordered_ids: &[String],
    ) -> Result<Vec<TimelineEvent>, NarrativeError> {
        // Phase 1: fetch via the existing DB-read path (dual path: the
        // sequence_no sort in `get_timeline` stays untouched). No limit —
        // the ordered view needs the full matching set so the spoke helper
        // can build a correct stable tail.
        let mut filtered = self.get_timeline(world_id, branch_id, None).await?;
        // Re-sort purely by sequence_no so the spoke helper's "stable tail"
        // (un-listed events) is in deterministic sequence order — matching
        // the in-memory gateway's T2 stable-tail semantics for cross-gateway
        // parity (sqlite `get_timeline` with no branch groups by branch_id
        // first; the ordered view's tail is sequence-only).
        filtered.sort_by_key(|e| e.sequence_no);
        // Convert nexus → spoke (call-boundary §7).
        let spoke_events: Vec<SpokeTimelineEvent> = filtered.into_iter().map(Into::into).collect();

        // Phase 2: delegate ordering to the spoke beat-assist helper (pure,
        // synchronous — no DB I/O inside the helper).
        match order_timeline_events_by_ids(&spoke_events, ordered_ids) {
            SpokeResult::Ok(ordered_spoke) => {
                // Convert spoke → nexus (call-boundary §7).
                Ok(ordered_spoke.into_iter().map(Into::into).collect())
            }
            SpokeResult::Reject(SpokeReject { code, message, .. }) => {
                Err(NarrativeError::ValidationError(format!(
                    "timeline ordering rejected: {}: {}",
                    code.as_str(),
                    message
                )))
            }
        }
    }
}

// Row type matching the narrative_worlds DDL.
#[derive(Debug, Clone, sqlx::FromRow)]
struct WorldRow {
    world_id: String,
    title: String,
    slug: String,
    status: String,
    canon_revision: Option<i64>,
    current_timeline_head_id: Option<String>,
    current_time_pointer: Option<String>,
    created_at: String,
    #[allow(dead_code)]
    root_fork_branch_id: Option<String>,
}

impl WorldRow {
    fn to_world_state(&self) -> WorldState {
        WorldState {
            world_id: self.world_id.clone(),
            title: self.title.clone(),
            slug: self.slug.clone(),
            status: self.status.clone(),
            is_fork: false,
            fork_branch_id: None,
            parent_world_id: None,
            forked_from_event_id: None,
            canon_revision: self.canon_revision.map(i64::cast_unsigned),
            current_timeline_head_id: self.current_timeline_head_id.clone(),
            current_time_pointer: self.current_time_pointer.clone(),
            created_at: self.created_at.clone(),
        }
    }
}

// Row type matching the narrative_timeline_events DDL.
#[derive(Debug, Clone, sqlx::FromRow)]
struct TimelineEventRow {
    timeline_event_id: String,
    world_id: String,
    branch_id: String,
    event_type: String,
    status: String,
    sequence_no: i64,
    title: Option<String>,
    summary: Option<String>,
    caused_by_event_ids_json: Option<String>,
    affected_key_block_ids_json: Option<String>,
    source_command_id: Option<String>,
    created_at: String,
}

impl TimelineEventRow {
    fn to_timeline_event(&self) -> TimelineEvent {
        TimelineEvent {
            schema_version: 1,
            timeline_event_id: self.timeline_event_id.clone(),
            world_id: self.world_id.clone(),
            branch_id: self.branch_id.clone(),
            event_type: self.event_type.clone(),
            status: self.status.clone(),
            sequence_no: self.sequence_no.cast_unsigned(),
            title: self.title.clone(),
            summary: self.summary.clone(),
            caused_by_event_ids: self
                .caused_by_event_ids_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok()),
            affected_key_block_ids: self
                .affected_key_block_ids_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok()),
            source_command_id: self.source_command_id.clone(),
            created_at: self.created_at.clone(),
        }
    }
}

/// Convert a sqlx error into a `NarrativeError`.
fn db_err(err: &sqlx::Error) -> NarrativeError {
    NarrativeError::Storage(format!("database error: {err}"))
}

// SAFETY: sqlx SQLite futures borrow the connection pool internally;
// safe for single-threaded SQLite usage within our tokio runtime.
#[allow(clippy::future_not_send)]
impl NarrativeGateway for SqliteNarrativeGateway {
    async fn get_world_state(&self, world_id: &str) -> Result<WorldState, NarrativeError> {
        let row = sqlx::query_as!(
            WorldRow,
            r#"SELECT
                world_id as "world_id!",
                title as "title!",
                slug as "slug!",
                status as "status!",
                canon_revision,
                current_timeline_head_id,
                current_time_pointer,
                created_at as "created_at!",
                root_fork_branch_id
            FROM narrative_worlds
            WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .ok_or_else(|| NarrativeError::ValidationError(format!("world not found: {world_id}")))?;

        Ok(row.to_world_state())
    }

    async fn get_timeline(
        &self,
        world_id: &str,
        branch_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<TimelineEvent>, NarrativeError> {
        // SQLite `LIMIT -1` means no limit.
        let limit_i64: i64 = limit.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1);

        // SAFETY: dynamic SQL — LIMIT + ordering depend on whether limit is set.
        // When limit is requested, ORDER BY DESC to get most-recent events first
        // (caller reverses for ASC display). Uses the same runtime format! pattern
        // as kb_store.rs:427-451.
        // Note: runtime query_as maps by column-name-to-field-name matching;
        // the `as "field!"` aliases used by the compile-time macros are omitted.
        let (order_dir, limit_clause) = if limit_i64 > 0 {
            ("DESC", format!("LIMIT {limit_i64}"))
        } else {
            ("ASC", String::new())
        };
        let events = if let Some(bid) = branch_id {
            sqlx::query_as::<_, TimelineEventRow>(&format!(
                r"SELECT
                    timeline_event_id,
                    world_id,
                    branch_id,
                    event_type,
                    status,
                    sequence_no,
                    title,
                    summary,
                    caused_by_event_ids_json,
                    affected_key_block_ids_json,
                    source_command_id,
                    created_at
                FROM narrative_timeline_events
                WHERE world_id = ? AND branch_id = ?
                ORDER BY sequence_no {order_dir}
                {limit_clause}"
            ))
            .bind(world_id)
            .bind(bid)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?
        } else {
            sqlx::query_as::<_, TimelineEventRow>(&format!(
                r"SELECT
                    timeline_event_id,
                    world_id,
                    branch_id,
                    event_type,
                    status,
                    sequence_no,
                    title,
                    summary,
                    caused_by_event_ids_json,
                    affected_key_block_ids_json,
                    source_command_id,
                    created_at
                FROM narrative_timeline_events
                WHERE world_id = ?
                ORDER BY branch_id {order_dir}, sequence_no {order_dir}
                {limit_clause}"
            ))
            .bind(world_id)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?
        };

        Ok(events
            .iter()
            .map(TimelineEventRow::to_timeline_event)
            .collect())
    }

    async fn get_event(&self, event_id: &str) -> Result<TimelineEvent, NarrativeError> {
        let row = sqlx::query_as!(
            TimelineEventRow,
            r#"SELECT
                timeline_event_id as "timeline_event_id!",
                world_id as "world_id!",
                branch_id as "branch_id!",
                event_type as "event_type!",
                status as "status!",
                sequence_no as "sequence_no!",
                title,
                summary,
                caused_by_event_ids_json,
                affected_key_block_ids_json,
                source_command_id,
                created_at as "created_at!"
            FROM narrative_timeline_events
            WHERE timeline_event_id = ?"#,
            event_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .ok_or_else(|| NarrativeError::ValidationError(format!("event not found: {event_id}")))?;

        Ok(row.to_timeline_event())
    }

    async fn get_narrative_context(
        &self,
        query: &NarrativeQuery,
    ) -> Result<NarrativeContext, NarrativeError> {
        // Phase 1: resolve world state
        let world_state = self.get_world_state(&query.world_id).await?;

        // Phase 2: resolve timeline position
        let timeline_position = if let Some(ref branch_id) = query.branch_id {
            let events = self
                .get_timeline(&query.world_id, Some(branch_id), None)
                .await?;
            if events.is_empty() {
                None
            } else {
                let max_seq = events.iter().map(|e| e.sequence_no).max().unwrap_or(0);
                let current_event_id = events
                    .iter()
                    .find(|e| e.sequence_no == max_seq)
                    .map(|e| e.timeline_event_id.clone());
                Some(TimelinePosition {
                    branch_id: branch_id.clone(),
                    world_id: query.world_id.clone(),
                    event_index: max_seq,
                    is_fork: false,
                    current_event_id,
                })
            }
        } else if let Some(ref head_id) = world_state.current_timeline_head_id {
            // Resolve head event to get branch info
            let evt = self.get_event(head_id).await.ok();
            evt.map(|e| TimelinePosition {
                branch_id: e.branch_id.clone(),
                world_id: query.world_id.clone(),
                event_index: e.sequence_no,
                is_fork: false,
                current_event_id: Some(e.timeline_event_id),
            })
        } else {
            None
        };

        // Phase 3: resolve event snapshot
        let event_snapshot = if let Some(ref event_id) = query.event_id {
            self.get_event(event_id).await.ok().map(|e| EventSnapshot {
                event_id: e.timeline_event_id,
                world_id: e.world_id,
                branch_id: e.branch_id,
                event_type: e.event_type,
                event_status: e.status,
                sequence_no: e.sequence_no,
                title: e.title,
                summary: e.summary,
                created_at: e.created_at,
            })
        } else if let Some(ref pos) = timeline_position {
            if let Some(ref eid) = pos.current_event_id {
                self.get_event(eid).await.ok().map(|e| EventSnapshot {
                    event_id: e.timeline_event_id,
                    world_id: e.world_id,
                    branch_id: e.branch_id,
                    event_type: e.event_type,
                    event_status: e.status,
                    sequence_no: e.sequence_no,
                    title: e.title,
                    summary: e.summary,
                    created_at: e.created_at,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(NarrativeContext {
            world: world_state,
            timeline_position,
            event_snapshot,
        })
    }

    async fn list_worlds(&self) -> Result<Vec<WorldState>, NarrativeError> {
        let rows = sqlx::query_as!(
            WorldRow,
            r#"SELECT
                world_id as "world_id!",
                title as "title!",
                slug as "slug!",
                status as "status!",
                canon_revision,
                current_timeline_head_id,
                current_time_pointer,
                created_at as "created_at!",
                root_fork_branch_id
            FROM narrative_worlds
            ORDER BY created_at ASC"#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        Ok(rows.iter().map(WorldRow::to_world_state).collect())
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn test_list_worlds_empty() {
        let (pool, _dir) = fresh_pool().await;
        let gw = SqliteNarrativeGateway::new(pool);
        let worlds = gw.list_worlds().await.unwrap();
        assert!(worlds.is_empty());
    }

    #[tokio::test]
    async fn test_list_worlds_with_data() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool,
            "wld_1",
            "ctr_test",
            "World One",
            "world-one",
            "private",
            "manual",
        )
        .await;
        seed::world(
            &pool,
            "wld_2",
            "ctr_test",
            "World Two",
            "world-two",
            "private",
            "manual",
        )
        .await;

        let gw = SqliteNarrativeGateway::new(pool);
        let worlds = gw.list_worlds().await.unwrap();
        assert_eq!(worlds.len(), 2);
        assert_eq!(worlds[0].world_id, "wld_1");
        assert_eq!(worlds[1].world_id, "wld_2");
    }

    #[tokio::test]
    async fn test_get_world_state_found() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool,
            "wld_1",
            "ctr_test",
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await;

        let gw = SqliteNarrativeGateway::new(pool);
        let state = gw.get_world_state("wld_1").await.unwrap();
        assert_eq!(state.world_id, "wld_1");
        assert_eq!(state.title, "Test World");
        assert_eq!(state.status, "active");
        assert!(!state.is_fork);
    }

    #[tokio::test]
    async fn test_get_world_state_not_found() {
        let (pool, _dir) = fresh_pool().await;
        let gw = SqliteNarrativeGateway::new(pool);
        let result = gw.get_world_state("wld_missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_timeline() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 1).await;
        seed::event(&pool, "evt_2", "wld_1", "fbk_root", "story_advance", 2).await;
        seed::event(&pool, "evt_3", "wld_1", "fbk_fork", "story_advance", 1).await;

        let gw = SqliteNarrativeGateway::new(pool);

        // All events for world
        let all = gw.get_timeline("wld_1", None, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // Filtered by branch
        let root = gw
            .get_timeline("wld_1", Some("fbk_root"), None)
            .await
            .unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(root[0].sequence_no, 1);
        assert_eq!(root[1].sequence_no, 2);

        let fork = gw
            .get_timeline("wld_1", Some("fbk_fork"), None)
            .await
            .unwrap();
        assert_eq!(fork.len(), 1);
    }

    #[tokio::test]
    async fn test_get_event() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 1).await;

        let gw = SqliteNarrativeGateway::new(pool);
        let event = gw.get_event("evt_1").await.unwrap();
        assert_eq!(event.timeline_event_id, "evt_1");
        assert_eq!(event.world_id, "wld_1");
        assert_eq!(event.sequence_no, 1);
    }

    #[tokio::test]
    async fn test_get_event_not_found() {
        let (pool, _dir) = fresh_pool().await;
        let gw = SqliteNarrativeGateway::new(pool);
        let result = gw.get_event("evt_missing").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_db_err_maps_to_storage() {
        // R8: db_err should return NarrativeError::Storage
        let sqlx_err = sqlx::Error::Configuration("test config error".into());
        let narrative_err = db_err(&sqlx_err);
        assert!(
            matches!(narrative_err, NarrativeError::Storage(ref msg) if msg.contains("database error")),
            "db_err should return NarrativeError::Storage, got: {narrative_err:?}"
        );
    }

    #[tokio::test]
    async fn test_get_narrative_context() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 1).await;

        let gw = SqliteNarrativeGateway::new(pool);
        let query = NarrativeQuery::new("wld_1")
            .with_branch("fbk_root")
            .with_event("evt_1");
        let ctx = gw.get_narrative_context(&query).await.unwrap();

        assert_eq!(ctx.world.world_id, "wld_1");
        assert!(ctx.timeline_position.is_some());
        assert!(ctx.event_snapshot.is_some());
        assert_eq!(ctx.event_snapshot.unwrap().event_id, "evt_1");
    }

    // ── V1.143 P0 T3: SqliteNarrativeGateway.get_timeline_ordered parity ──
    //
    // Mirrors the in-memory T2 suite (test_get_timeline_ordered_*) through the
    // real SQLite path: DB-read → nexus→spoke conversion → spoke
    // order_timeline_events_by_ids → spoke→nexus conversion. Proves the
    // ordered capability is storage-backed, not just in-memory.

    // T3a: explicit ids come first (in requested order), remaining events
    // appended in sequence_no order (stable tail). Storage is seeded with
    // shuffled sequence_no assignments to prove ordering is independent of
    // insertion/storage order.
    #[tokio::test]
    async fn test_get_timeline_ordered_explicit_ids_then_sequence_tail() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        // Shuffled sequence_no assignment (id ↛ sequence):
        //   evt_1 → seq 3, evt_2 → seq 1, evt_3 → seq 5,
        //   evt_4 → seq 2, evt_5 → seq 4
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 3).await;
        seed::event(&pool, "evt_2", "wld_1", "fbk_root", "story_advance", 1).await;
        seed::event(&pool, "evt_3", "wld_1", "fbk_root", "story_advance", 5).await;
        seed::event(&pool, "evt_4", "wld_1", "fbk_root", "story_advance", 2).await;
        seed::event(&pool, "evt_5", "wld_1", "fbk_root", "story_advance", 4).await;

        let gw = SqliteNarrativeGateway::new(pool);
        // Request [evt_3, evt_1, evt_5] explicitly; remaining (evt_2 seq1,
        // evt_4 seq2) form the stable tail in sequence_no order.
        let ordered = gw
            .get_timeline_ordered(
                "wld_1",
                Some("fbk_root"),
                &[
                    "evt_3".to_string(),
                    "evt_1".to_string(),
                    "evt_5".to_string(),
                ],
            )
            .await
            .unwrap();

        assert_eq!(ordered.len(), 5);
        assert_eq!(ordered[0].timeline_event_id, "evt_3");
        assert_eq!(ordered[1].timeline_event_id, "evt_1");
        assert_eq!(ordered[2].timeline_event_id, "evt_5");
        // Stable tail: evt_2 (seq 1) then evt_4 (seq 2).
        assert_eq!(ordered[3].timeline_event_id, "evt_2");
        assert_eq!(ordered[4].timeline_event_id, "evt_4");
    }

    // T3b: unknown ordered ids surface as ValidationError (no panic). Mirrors
    // the in-memory T2b reject contract through the sqlite path.
    #[tokio::test]
    async fn test_get_timeline_ordered_rejects_unknown_ids() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 1).await;

        let gw = SqliteNarrativeGateway::new(pool);
        let result = gw
            .get_timeline_ordered(
                "wld_1",
                Some("fbk_root"),
                &["evt_1".to_string(), "evt_missing".to_string()],
            )
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, NarrativeError::ValidationError(ref msg) if msg.contains("rejected")),
            "expected ValidationError with reject detail, got: {err:?}"
        );
    }

    // T3c: dual path intact — get_timeline (sequence_no sort) is unaffected by
    // the new ordered method; both coexist on the sqlite gateway.
    #[tokio::test]
    async fn test_get_timeline_dual_path_sequence_sort_unchanged() {
        let (pool, _dir) = fresh_pool().await;
        seed::world(
            &pool, "wld_1", "ctr_test", "Test", "test", "private", "manual",
        )
        .await;
        seed::event(&pool, "evt_1", "wld_1", "fbk_root", "story_advance", 3).await;
        seed::event(&pool, "evt_2", "wld_1", "fbk_root", "story_advance", 1).await;
        seed::event(&pool, "evt_3", "wld_1", "fbk_root", "story_advance", 2).await;

        let gw = SqliteNarrativeGateway::new(pool);
        // The sequence_no path is the default and must remain unchanged.
        let by_seq = gw
            .get_timeline("wld_1", Some("fbk_root"), None)
            .await
            .unwrap();
        assert_eq!(
            by_seq.iter().map(|e| e.sequence_no).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
}
