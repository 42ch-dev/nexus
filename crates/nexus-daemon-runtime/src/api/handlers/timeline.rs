#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Query, State};
use axum::Json;
use nexus_contracts::TimelineOverviewResponse;
use serde::Deserialize;
use tracing::info;

const DEFAULT_PAGE_SIZE: usize = 20;
const CURSOR_PREFIX: &str = "tl:";

#[derive(Debug, Deserialize)]
pub struct TimelineOverviewParams {
    pub cursor: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct WorldOverviewRow {
    world_id: String,
    title: String,
    era_count: i64,
    event_count: i64,
    last_event_at: Option<String>,
}

fn decode_cursor(raw: &str) -> Result<String, NexusApiError> {
    if raw.len() > 256 {
        return Err(NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor too long".to_string(),
        });
    }
    let world_id = raw
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor format".to_string(),
        })?;
    if world_id.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor is empty".to_string(),
        });
    }
    Ok(world_id.to_string())
}

fn encode_cursor(world_id: &str) -> String {
    format!("{CURSOR_PREFIX}{world_id}")
}

pub async fn get_timeline_overview(
    State(state): State<WorkspaceState>,
    Query(params): Query<TimelineOverviewParams>,
) -> Result<Json<TimelineOverviewResponse>, NexusApiError> {
    #[allow(clippy::uninlined_format_args)]
    let pool = state.pool_or_uninit()?;
    let limit = DEFAULT_PAGE_SIZE + 1;

    // SAFETY: dynamic SQL — cursor condition is constructed at runtime.
    let (where_clause, bind_value) = if let Some(ref cursor_raw) = params.cursor {
        let world_id = decode_cursor(cursor_raw)?;
        ("WHERE nw.world_id > ?1".to_string(), Some(world_id))
    } else {
        (String::new(), None)
    };

    #[allow(clippy::uninlined_format_args)]
    let sql = format!(
        r"SELECT
            nw.world_id,
            nw.title,
            COALESCE(kb_agg.era_count, 0) as era_count,
            COALESCE(kb_agg.event_count, 0) as event_count,
            kb_agg.last_event_at
        FROM narrative_worlds nw
        LEFT JOIN (
            SELECT
                world_id,
                SUM(CASE WHEN block_type = 'era' THEN 1 ELSE 0 END) as era_count,
                SUM(CASE WHEN block_type = 'event' THEN 1 ELSE 0 END) as event_count,
                MAX(CASE WHEN block_type = 'event' THEN created_at ELSE NULL END) as last_event_at
            FROM kb_key_blocks
            WHERE status NOT IN ('deleted', 'merged', 'deprecated')
            GROUP BY world_id
        ) kb_agg ON nw.world_id = kb_agg.world_id
        {where_clause}
        ORDER BY nw.world_id ASC
        LIMIT {limit}",
        where_clause = where_clause,
        limit = limit,
    );

    // SAFETY: dynamic SQL — cursor condition is constructed at runtime.
    let mut query = sqlx::query_as::<_, WorldOverviewRow>(&sql);
    if let Some(ref cursor_world_id) = bind_value {
        query = query.bind(cursor_world_id);
    }
    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let has_more = rows.len() > DEFAULT_PAGE_SIZE;
    let worlds = rows.into_iter().take(DEFAULT_PAGE_SIZE).collect::<Vec<_>>();

    let cursor = if has_more {
        worlds.last().map(|w| encode_cursor(&w.world_id))
    } else {
        None
    };

    let total_worlds: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM narrative_worlds")
        .fetch_one(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    info!(
        "timeline_overview: {} worlds (page, has_more={})",
        worlds.len(),
        has_more
    );

    Ok(Json(TimelineOverviewResponse {
        worlds: worlds
            .into_iter()
            .map(|w| nexus_contracts::TimelineOverviewResponseWorldsItem {
                world_id: w.world_id,
                title: Some(w.title),
                era_count: u64::try_from(w.era_count).unwrap_or(0),
                event_count: u64::try_from(w.event_count).unwrap_or(0),
                last_event_at: w.last_event_at.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                }),
            })
            .collect(),
        cursor,
        #[allow(clippy::cast_sign_loss)]
        total_worlds: u64::try_from(total_worlds).unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use crate::workspace::WorkspaceState;
    use axum::extract::State;

    fn make_query(cursor: Option<String>) -> Query<TimelineOverviewParams> {
        Query(TimelineOverviewParams { cursor })
    }

    async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str, title: &str) {
        let slug = title.to_lowercase().replace(' ', "-");
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?1, 'ws', 'test_creator', ?2, ?3, 'active', 'private', 'manual', \
              '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(title)
        .bind(&slug)
        .execute(pool)
        .await
        .expect("seed world");
    }

    async fn seed_kb_block(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        block_type: &str,
        created_at: Option<&str>,
    ) {
        use uuid::Uuid;
        let block_id = format!("kb_{}", Uuid::new_v4().to_string().replace('-', ""));
        let name = format!("{block_type}_{}", &block_id[3..11]);
        let ts = created_at.unwrap_or("2026-01-01T00:00:00Z");
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'confirmed', ?5)",
        )
        .bind(&block_id)
        .bind(world_id)
        .bind(block_type)
        .bind(&name)
        .bind(ts)
        .execute(pool)
        .await
        .expect("seed kb block");
    }

    #[tokio::test]
    async fn empty_world_list() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let result = get_timeline_overview(State(state), make_query(None)).await;
        let resp = result.expect("handler should succeed");
        assert!(resp.worlds.is_empty());
        assert!(resp.cursor.is_none());
        assert_eq!(resp.total_worlds, 0);
        drop(tmp);
    }

    #[tokio::test]
    async fn worlds_with_zero_counts() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool().expect("pool");
        crate::test_utils::seed_test_creator_and_world(pool).await;
        seed_world(pool, "wld_a", "World A").await;
        seed_world(pool, "wld_b", "World B").await;

        let result = get_timeline_overview(State(state.clone()), make_query(None)).await;
        let resp = result.expect("handler should succeed");
        assert_eq!(
            resp.worlds.len(),
            3,
            "should include seeded world + 2 test worlds"
        );
        assert_eq!(resp.worlds[0].world_id, "wld_a");
        assert_eq!(resp.worlds[0].title.as_deref(), Some("World A"));
        assert_eq!(resp.worlds[0].era_count, 0);
        assert_eq!(resp.worlds[0].event_count, 0);
        assert!(resp.worlds[0].last_event_at.is_none());
        assert_eq!(resp.worlds[1].world_id, "wld_b");
        assert_eq!(resp.worlds[2].world_id, "wld_test_world");
        assert_eq!(resp.total_worlds, 3);
        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn worlds_with_eras_and_events() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool().expect("pool");
        crate::test_utils::seed_test_creator_and_world(pool).await;
        seed_world(pool, "wld_a", "World A").await;
        seed_world(pool, "wld_b", "World B").await;
        seed_kb_block(pool, "wld_a", "era", Some("2026-06-01T00:00:00Z")).await;
        seed_kb_block(pool, "wld_a", "era", Some("2026-06-02T00:00:00Z")).await;
        seed_kb_block(pool, "wld_a", "event", Some("2026-06-03T00:00:00Z")).await;
        seed_kb_block(pool, "wld_b", "event", Some("2026-06-04T00:00:00Z")).await;

        let result = get_timeline_overview(State(state.clone()), make_query(None)).await;
        let resp = result.expect("handler should succeed");
        assert_eq!(
            resp.worlds.len(),
            3,
            "should include seeded world + 2 test worlds"
        );

        let wld_a = resp
            .worlds
            .iter()
            .find(|w| w.world_id == "wld_a")
            .expect("wld_a");
        assert_eq!(wld_a.era_count, 2);
        assert_eq!(wld_a.event_count, 1);
        assert_eq!(wld_a.last_event_at.as_deref(), Some("2026-06-03T00:00:00Z"));

        let wld_b = resp
            .worlds
            .iter()
            .find(|w| w.world_id == "wld_b")
            .expect("wld_b");
        assert_eq!(wld_b.era_count, 0);
        assert_eq!(wld_b.event_count, 1);
        assert_eq!(wld_b.last_event_at.as_deref(), Some("2026-06-04T00:00:00Z"));

        let seeded = resp
            .worlds
            .iter()
            .find(|w| w.world_id == "wld_test_world")
            .expect("seeded");
        assert_eq!(seeded.era_count, 0);
        assert_eq!(seeded.event_count, 0);

        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn cursor_pagination() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool().expect("pool");
        crate::test_utils::seed_test_creator_and_world(pool).await;
        for i in 0..25 {
            let wld = format!("wld_{:03}", i);
            seed_world(pool, &wld, &format!("World {i}")).await;
        }

        let page1 = get_timeline_overview(State(state.clone()), make_query(None))
            .await
            .expect("page 1");
        assert_eq!(page1.worlds.len(), 20);
        assert!(page1.cursor.is_some(), "page 1 should have a cursor");
        assert_eq!(page1.total_worlds, 26);
        assert_eq!(page1.worlds[0].world_id, "wld_000");
        assert_eq!(page1.worlds[19].world_id, "wld_019");

        let page2 = get_timeline_overview(State(state.clone()), make_query(page1.cursor.clone()))
            .await
            .expect("page 2");
        assert_eq!(page2.worlds.len(), 6);
        assert!(page2.cursor.is_none(), "page 2 should have no cursor");
        assert_eq!(page2.worlds[0].world_id, "wld_020");
        assert_eq!(page2.worlds[5].world_id, "wld_test_world");

        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn cursor_too_long() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let long_cursor = format!("tl:{}", "a".repeat(1000));
        let result = get_timeline_overview(State(state), make_query(Some(long_cursor))).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::InvalidInput { field, reason } => {
                assert_eq!(field, "cursor");
                assert_eq!(reason, "cursor too long");
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
        drop(tmp);
    }

    #[tokio::test]
    async fn cursor_invalid_format() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let result =
            get_timeline_overview(State(state), make_query(Some("invalid-cursor".to_string())))
                .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::InvalidInput { field, .. } => {
                assert_eq!(field, "cursor");
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
        drop(tmp);
    }
}
