//! SQLite-backed `NarrativeGateway` implementation.
//!
//! Implements the `NarrativeGateway` trait from `nexus-narrative` using
//! the workspace `state.db` pool. Uses compile-time checked `sqlx` queries
//! for all static SQL. Local forks ARE stored since V1.60 — a fork is a
//! `fork_created` marker event on a dedicated branch; branch-level lineage
//! rides the marker's `extensions_nexus_json` (`fork_lineage`), surfaced via
//! the timeline-events route's `extensions` field. The world-level fork fields
//! (`is_fork`, `fork_branch_id`, `parent_world_id`, `forked_from_event_id`)
//! stay hardcoded: they model the platform-only world fork, which has no local
//! counterpart (the branch-level lineage is the carrier).
//!
//! # Test helpers
//!
//! The [`seed`] submodule provides async functions to insert test data
//! (worlds, timeline events) into the database for integration tests.

use nexus_narrative::timeline_event::TimelineEvent;
use nexus_narrative::{
    EventSnapshot, NarrativeContext, NarrativeError, NarrativeGateway, NarrativeQuery,
    TimelinePosition, WorldState,
};
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
}

// ── V1.145 P3 production read primitive for ScopeQueryPort ───────────────

/// Read timeline events for a world, optionally narrowed by `branch_id` and/or
/// a set of `timeline_event_id`s (V1.145 P3 production read primitive).
///
/// Backs `ScopeQueryPort::list_timeline_events` (spec §7.4). This is a **free
/// function** taking a `&SqlitePool` (not a `SqliteNarrativeGateway` method) so
/// the production `NexusAdapter` port can call it directly without
/// constructing a gateway. `branch_id` and `event_ids` are both optional;
/// `None`/empty means no filter on that dimension.
///
/// # Filters (applied in SQL)
///
/// - `world_id` — always required (`WHERE world_id = ?`); mirrors
///   `Scope.scope_id` → `world_id` (spec §7.4 timeline Scope filter alignment).
/// - `branch_id` — optional strict equality (`scope.extensions["nexus"]
///   ["branch_id"]`).
/// - `event_ids` — optional `IN (SELECT value FROM json_each(?))`, the same
///   `SQLite` idiom `kb_store::list_by_world_scoped` uses for `entry_ids`
///   (`scope.timeline_event_ids`).
///
/// # Ordering
///
/// Results are ordered by `sequence_no ASC` within a branch; across branches
/// (when `branch_id` is `None`) by `branch_id, sequence_no` — matching
/// [`get_timeline`](SqliteNarrativeGateway::get_timeline)'s no-branch ordering.
///
/// # Overflow contract
///
/// Unlike `list_knowledge_entries`, there is **no** overflow-safety cap:
/// timeline events are append-ordered and bounded per branch (spec §7.4
/// timeline row lists only the three scope filters; no `LIST_BY_WORLD_LIMIT`
/// reject applies).
///
/// # Errors
///
/// Returns [`NarrativeError::Storage`] on database failure.
pub async fn list_timeline_events_scoped(
    pool: &SqlitePool,
    world_id: &str,
    branch_id: Option<&str>,
    event_ids: &[String],
) -> Result<Vec<TimelineEvent>, NarrativeError> {
    let has_id_filter = !event_ids.is_empty();
    let has_branch = branch_id.is_some();

    // SAFETY: static column list; the only dynamic fragments are the optional
    // `branch_id` equality and the optional `event_ids` IN clause — both use
    // bind params only (no user-controlled SQL). Same runtime-query pattern as
    // `get_timeline` (line ~320) and `kb_store::list_by_world_scoped` (line
    // ~328). The bind order below matches the `?` order in the SQL: world_id,
    // then branch_id (if present), then the event_ids JSON array (if present).
    let mut sql = String::from(if has_branch {
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
                modules_json,
                created_at
            FROM narrative_timeline_events
            WHERE world_id = ? AND branch_id = ?"
    } else {
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
                modules_json,
                created_at
            FROM narrative_timeline_events
            WHERE world_id = ?"
    });
    if has_id_filter {
        sql.push_str(" AND timeline_event_id IN (SELECT value FROM json_each(?))");
    }
    sql.push_str(if has_branch {
        " ORDER BY sequence_no ASC"
    } else {
        " ORDER BY branch_id ASC, sequence_no ASC"
    });

    let mut q = sqlx::query_as::<_, TimelineEventRow>(&sql).bind(world_id);
    if let Some(bid) = branch_id {
        q = q.bind(bid);
    }
    if has_id_filter {
        q = q.bind(serde_json::to_string(event_ids).unwrap_or_else(|_| "[]".to_string()));
    }

    let rows = q.fetch_all(pool).await.map_err(|e| db_err(&e))?;
    Ok(rows
        .iter()
        .map(TimelineEventRow::to_timeline_event)
        .collect())
}

/// Read a page of timeline events for a world (V1.147 P2 T1 route read
/// primitive).
///
/// Serves `GET /v1/daemon/worlds/:world_id/timeline/events`. Unlike
/// [`list_timeline_events_scoped`] (orchestrator read path, returns domain
/// [`TimelineEvent`]s without the JSON extension columns), this page read
/// returns the full row surface the daemon wire DTO needs — including
/// `metadata_json` and `extensions_nexus_json` — and applies optional
/// `status` / `event_type` equality filters plus a keyset cursor on
/// `(branch_id, sequence_no)`.
///
/// # Filter semantics
///
/// - `world_id` — always required (`WHERE world_id = ?`).
/// - `branch_id` — optional strict equality. When absent, rows from all
///   branches are returned ordered by `(branch_id, sequence_no)`; the keyset
///   cursor then carries the composite pair (matching the
///   `UNIQUE (world_id, branch_id, sequence_no)` index).
/// - `status` / `event_type` — optional strict equality.
/// - `cursor` — `Some((branch_id, sequence_no))` exclusive keyset; the caller
///   is responsible for the opaque cursor encoding/validation.
/// - `limit` — max rows to return (caller typically requests `limit + 1` to
///   detect `has_more`).
///
/// # Ordering
///
/// `sequence_no ASC` within the branch; across branches
/// `branch_id ASC, sequence_no ASC` — both index-backed by
/// `idx_narrative_timeline_events_world_branch_sequence`.
///
/// # Errors
///
/// Returns [`NarrativeError::Storage`] on database failure.
pub async fn list_timeline_events_page(
    pool: &SqlitePool,
    world_id: &str,
    branch_id: Option<&str>,
    status: Option<&str>,
    event_type: Option<&str>,
    cursor: Option<(&str, i64)>,
    limit: i64,
) -> Result<Vec<TimelineEventPageRow>, NarrativeError> {
    // SAFETY: static column list; the only dynamic fragments are optional
    // `branch_id` / `status` / `event_type` equalities and the keyset cursor
    // predicate — all bind-parameter driven (no user-controlled SQL), same
    // runtime-query pattern as `list_timeline_events_scoped` above.
    let has_branch = branch_id.is_some();
    let has_status = status.is_some();
    let has_event_type = event_type.is_some();

    let mut sql = String::from(
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
                metadata_json,
                extensions_nexus_json,
                created_at
            FROM narrative_timeline_events
            WHERE world_id = ?",
    );
    if has_branch {
        sql.push_str(" AND branch_id = ?");
    }
    if has_status {
        sql.push_str(" AND status = ?");
    }
    if has_event_type {
        sql.push_str(" AND event_type = ?");
    }
    if cursor.is_some() {
        if has_branch {
            sql.push_str(" AND sequence_no > ?");
        } else {
            // Composite keyset: strictly after (cursor_branch, cursor_seq).
            sql.push_str(" AND (branch_id > ? OR (branch_id = ? AND sequence_no > ?))");
        }
    }
    sql.push_str(if has_branch {
        " ORDER BY sequence_no ASC LIMIT ?"
    } else {
        " ORDER BY branch_id ASC, sequence_no ASC LIMIT ?"
    });

    let mut q = sqlx::query_as::<_, TimelineEventPageRow>(&sql).bind(world_id);
    if let Some(bid) = branch_id {
        q = q.bind(bid);
    }
    if let Some(st) = status {
        q = q.bind(st);
    }
    if let Some(et) = event_type {
        q = q.bind(et);
    }
    if let Some((cursor_branch, cursor_seq)) = cursor {
        if has_branch {
            q = q.bind(cursor_seq);
        } else {
            q = q.bind(cursor_branch).bind(cursor_branch).bind(cursor_seq);
        }
    }
    q = q.bind(limit);

    q.fetch_all(pool).await.map_err(|e| db_err(&e))
}

/// Full row surface for the daemon timeline-events page read, including the
/// JSON extension columns ([`list_timeline_events_page`]).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimelineEventPageRow {
    pub timeline_event_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub event_type: String,
    pub status: String,
    pub sequence_no: i64,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub caused_by_event_ids_json: Option<String>,
    pub affected_key_block_ids_json: Option<String>,
    pub source_command_id: Option<String>,
    pub metadata_json: Option<String>,
    pub extensions_nexus_json: Option<String>,
    pub created_at: String,
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
    // V1.164 P1: full serialized `modules` namespace (l5-mind observation).
    // Carries per-event functional-dialect modules as a JSON object. NULL for
    // rows written before the additive migration or without modules data
    // (unrecorded per spoke handbook).
    modules_json: Option<String>,
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
            // V1.164 P1: surface modules_json as TimelineEvent.modules — NULL
            // → None (unrecorded), verbatim JSON otherwise (matches the
            // kb_key_blocks.modules_json read pattern in kb_store.rs).
            modules: self
                .modules_json
                .as_ref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()),
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
                    modules_json,
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
                    modules_json,
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
                modules_json,
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
}
