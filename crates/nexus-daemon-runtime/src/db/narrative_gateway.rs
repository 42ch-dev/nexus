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
#[cfg(test)]
pub mod seed {
    use sqlx::SqlitePool;

    /// Seed a test world row into `narrative_worlds`.
    ///
    /// Creates a minimal creator row for FK satisfaction if it does not exist.
    pub async fn world(
        pool: &SqlitePool,
        world_id: &str,
        owner_creator_id: &str,
        title: &str,
        slug: &str,
        visibility: &str,
        time_policy: &str,
    ) {
        sqlx::query!(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')",
            owner_creator_id,
            owner_creator_id,
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES (?, 'wrk_test', ?, ?, ?, 'active', ?, ?, '{}')"#,
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Seed a test timeline event row into `narrative_timeline_events`.
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
    NarrativeError::ValidationError(format!("database error: {err}"))
}

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
    ) -> Result<Vec<TimelineEvent>, NarrativeError> {
        let events = if let Some(bid) = branch_id {
            sqlx::query_as!(
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
                WHERE world_id = ? AND branch_id = ?
                ORDER BY sequence_no ASC"#,
                world_id,
                bid
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?
        } else {
            sqlx::query_as!(
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
                WHERE world_id = ?
                ORDER BY branch_id ASC, sequence_no ASC"#,
                world_id
            )
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
            let events = self.get_timeline(&query.world_id, Some(branch_id)).await?;
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
    use nexus_local_db::{open_pool, run_migrations};

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
        let all = gw.get_timeline("wld_1", None).await.unwrap();
        assert_eq!(all.len(), 3);

        // Filtered by branch
        let root = gw.get_timeline("wld_1", Some("fbk_root")).await.unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(root[0].sequence_no, 1);
        assert_eq!(root[1].sequence_no, 2);

        let fork = gw.get_timeline("wld_1", Some("fbk_fork")).await.unwrap();
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
