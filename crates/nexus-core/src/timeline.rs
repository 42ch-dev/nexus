//! World timeline read projection and bounded stream source
//! (v1.190 P0-T3).
//!
//! Extracted from the daemon `timeline` / `timeline_events` HTTP handlers:
//! the overview page, the per-World keyset-paginated event page, their
//! opaque cursors, branch/status/event-type filters and the
//! modules/extensions row mapping run here against the guarded pool — one
//! SQL authority. Both reads are bounded pulls; HTTP SSE framing and
//! gap/disconnect handling stay a P5-T4 transport surface. A stream is a
//! read feed, not a workflow-engine concern.

use nexus_contracts::daemon_api::timeline::list_timeline_events_response::{
    ListTimelineEventsResponse, TimelineEventInfo, TimelineEventInfoStatus,
};
use nexus_contracts::{TimelineOverviewResponse, TimelineOverviewResponseWorldsItem};
use nexus_local_db::narrative_gateway::{list_timeline_events_page, TimelineEventPageRow};
use serde_json::{Map, Value};
use sqlx::SqlitePool;

use crate::error::{db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::world_kb::guards::{check_world_owner, WorldOwnerGuardFailure};
use crate::worlds::narrative_internal;

/// Overview worlds per page (unchanged wire contract).
const OVERVIEW_PAGE_SIZE: usize = 20;
const OVERVIEW_CURSOR_PREFIX: &str = "tl:";

/// Per-World events page bounds (unchanged wire contract).
const EVENTS_DEFAULT_PAGE_SIZE: u32 = 20;
const EVENTS_MAX_PAGE_SIZE: u32 = 100;
/// Opaque keyset cursor prefix (`ev1:` — future encodings may coexist).
const EVENTS_CURSOR_PREFIX: &str = "ev1:";
/// Legacy root branch fallback when `narrative_worlds.root_fork_branch_id`
/// is unset (matches `resolve_run_branch` in the daemon compute reads).
const ROOT_BRANCH_FALLBACK: &str = "fbk_root";

/// Bounded timeline-overview page query. The Core-prefixed wrapper schema is
/// owned by the P5-T0 `schemas/core/timeline-api.schema.json` Create entry.
#[derive(Debug, Clone, Default)]
pub struct CoreTimelineOverviewQuery {
    pub cursor: Option<String>,
}

/// Bounded per-World timeline-events page query. The Core-prefixed wrapper
/// schema is owned by the P5-T0 `schemas/core/timeline_events-api.schema.json`
/// Create entry.
#[derive(Debug, Clone, Default)]
pub struct CoreTimelineEventsQuery {
    pub branch_id: Option<String>,
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// Overview projection: worlds with aggregated key-block counts
/// (`era` / `event`, live statuses only) and the last event timestamp.
#[derive(Debug, sqlx::FromRow)]
struct WorldOverviewRow {
    world_id: String,
    title: String,
    era_count: i64,
    event_count: i64,
    last_event_at: Option<String>,
}

/// Overview projection query without a cursor (`?` = bound fetch limit).
const OVERVIEW_PAGE_SQL: &str = r"SELECT
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
ORDER BY nw.world_id ASC
LIMIT ?";

/// Overview projection query with a keyset cursor (`?1` = cursor world_id,
/// `?2` = bound fetch limit).
const OVERVIEW_CURSOR_PAGE_SQL: &str = r"SELECT
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
WHERE nw.world_id > ?1
ORDER BY nw.world_id ASC
LIMIT ?2";

impl CoreService {
    /// Read one bounded timeline-overview page: every workspace world with
    /// its aggregated era/event counts, ordered by `world_id`, keyset
    /// cursor on `world_id`.
    ///
    /// Read-scope invariant (workspace-single-owner): the workspace state DB
    /// holds one owner's workspace, so the overview intentionally returns
    /// workspace-wide rows with no `owner_creator_id` filter — parity with
    /// the pre-migration daemon read. Owner isolation is a workspace
    /// boundary property, not a per-read filter; per-World writes stay
    /// ownership-guarded.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::InvalidInput`] when the
    /// cursor is malformed, and [`CoreError::Internal`] on storage failure.
    pub async fn timeline_overview(
        &self,
        principal: &Principal,
        query: CoreTimelineOverviewQuery,
    ) -> CoreResult<TimelineOverviewResponse> {
        self.verify_principal(principal)?;
        overview_page(&self.inner.pool, query).await
    }

    /// Read one bounded timeline-events page for an owned World
    /// (`narrative_timeline_events`), keyset cursor on
    /// `(branch_id, sequence_no)`.
    ///
    /// The branch filter defaults to the World's current branch
    /// (`root_fork_branch_id`, falling back to `fbk_root`); the status
    /// filter defaults to `canon`; `event_type` is an exact match. This is
    /// the bounded pull/read data source; HTTP SSE framing and
    /// gap/disconnect handling belong to the P5 transport surface.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::NotFound`] for an unknown
    /// world, [`CoreError::Forbidden`] when the caller does not own the
    /// world, [`CoreError::InvalidInput`] for a malformed cursor or status
    /// filter, and [`CoreError::Internal`] on storage failure.
    pub async fn list_timeline_events(
        &self,
        principal: &Principal,
        world_id: String,
        query: CoreTimelineEventsQuery,
    ) -> CoreResult<ListTimelineEventsResponse> {
        self.verify_principal(principal)?;
        let pool = &self.inner.pool;
        // Ownership guard before any read: missing world → NotFound, not
        // owned → Forbidden. The denial renders with this route's retained
        // envelopes (`world {id} not found` / `you do not own this world`).
        check_world_owner(pool, &world_id, principal.creator_id())
            .await
            .map_err(WorldOwnerGuardFailure::into_timeline_error)?;

        // The guard above proves the row exists; `fetch_optional` still
        // guards the concurrent-delete race. `root_fork_branch_id` unset →
        // legacy `fbk_root` fallback.
        let world_row: Option<Option<String>> = sqlx::query_scalar(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = ?",
        )
        .bind(&world_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(&e))?;
        let root_branch = world_row
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("world {world_id} not found"),
            })?
            .unwrap_or_else(|| ROOT_BRANCH_FALLBACK.to_string());

        // The read always targets a single branch (explicit filter or the
        // World's current branch); keep the Option for the page-query
        // signature.
        let branch_filter = Some(
            query
                .branch_id
                .as_deref()
                .unwrap_or(&root_branch)
                .to_string(),
        );
        let status_filter = parse_status_filter(query.status.as_deref())?;
        let event_type_filter = query.event_type.as_deref();
        let cursor = query
            .cursor
            .as_deref()
            .map(decode_events_cursor)
            .transpose()?;
        // `list_timeline_events_page` takes `Option<(&str, i64)>`; the owned
        // decoded values are reborrowed here.
        let cursor_ref = cursor
            .as_ref()
            .map(|(branch, sequence)| (branch.as_str(), *sequence));

        let limit = query
            .limit
            .unwrap_or(EVENTS_DEFAULT_PAGE_SIZE)
            .min(EVENTS_MAX_PAGE_SIZE);
        // W-1 (QC): `limit=0` must not report `has_more` — the wire contract
        // states `has_more` is equivalent to `next_cursor` being non-null,
        // and a `has_more=true, next_cursor=null` response would growth-loop
        // a keyset client (null cursor → re-request page 1). An empty page
        // with no continuation is the only honest answer.
        if limit == 0 {
            return Ok(ListTimelineEventsResponse {
                items: Vec::new(),
                has_more: false,
                next_cursor: None,
            });
        }
        // Fetch one extra row to detect has_more.
        let rows = list_timeline_events_page(
            pool,
            &world_id,
            branch_filter.as_deref(),
            Some(status_filter),
            event_type_filter,
            cursor_ref,
            i64::from(limit) + 1,
        )
        .await
        .map_err(|e| narrative_internal("timeline_events.page", &e))?;

        // `fetch_limit = limit + 1`; more rows than `limit` means another
        // page. Rows are capped by the SQL LIMIT, so this cast cannot
        // truncate.
        #[allow(clippy::cast_possible_truncation)]
        let has_more = rows.len() as u32 > limit;
        let page_rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();

        let next_cursor = if has_more {
            page_rows
                .last()
                .map(|r| encode_events_cursor(&r.branch_id, r.sequence_no))
        } else {
            None
        };

        let items = page_rows
            .into_iter()
            .map(event_info)
            .collect::<CoreResult<Vec<_>>>()?;

        let branch = branch_filter.as_deref().unwrap_or_default();
        let status = status_filter;
        tracing::info!(
            "timeline_events: world={world_id} branch={branch} status={status} page={} has_more={has_more}",
            items.len(),
        );

        Ok(ListTimelineEventsResponse {
            items,
            has_more,
            next_cursor,
        })
    }
}

/// Read the overview page (projection body of
/// [`CoreService::timeline_overview`]).
async fn overview_page(
    pool: &SqlitePool,
    query: CoreTimelineOverviewQuery,
) -> CoreResult<TimelineOverviewResponse> {
    // Fetch one extra row to detect has_more.
    let fetch_limit = i64::try_from(OVERVIEW_PAGE_SIZE + 1).map_err(|_| CoreError::Internal {
        category: "timeline_overview: page size overflow".to_string(),
    })?;
    let rows: Vec<WorldOverviewRow> = match decode_overview_cursor(query.cursor.as_deref())? {
        Some(after_world_id) => {
            sqlx::query_as(OVERVIEW_CURSOR_PAGE_SQL)
                .bind(after_world_id)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await
        }
        None => {
            sqlx::query_as(OVERVIEW_PAGE_SQL)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await
        }
    }
    .map_err(|e| db_err(&e))?;

    let has_more = rows.len() > OVERVIEW_PAGE_SIZE;
    let worlds = rows
        .into_iter()
        .take(OVERVIEW_PAGE_SIZE)
        .collect::<Vec<_>>();

    let cursor = if has_more {
        worlds.last().map(|w| encode_overview_cursor(&w.world_id))
    } else {
        None
    };

    let total_worlds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM narrative_worlds")
        .fetch_one(pool)
        .await
        .map_err(|e| db_err(&e))?;

    let page_len = worlds.len();
    tracing::info!("timeline_overview: {page_len} worlds (page, has_more={has_more})");

    let items = worlds.into_iter().map(overview_item).collect();
    Ok(TimelineOverviewResponse {
        worlds: items,
        cursor,
        total_worlds: u64::try_from(total_worlds).map_err(|_| CoreError::Internal {
            category: format!("timeline_overview: negative total_worlds {total_worlds}"),
        })?,
    })
}

/// Map an overview row to the wire item; `last_event_at` tolerates only the
/// RFC3339 timestamps the projection itself produces.
fn overview_item(w: WorldOverviewRow) -> TimelineOverviewResponseWorldsItem {
    TimelineOverviewResponseWorldsItem {
        world_id: w.world_id,
        title: Some(w.title),
        era_count: u64::try_from(w.era_count).unwrap_or(0),
        event_count: u64::try_from(w.event_count).unwrap_or(0),
        last_event_at: w.last_event_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        }),
    }
}

/// Decode the opaque overview cursor (`tl:<world_id>`); `None` = first page.
fn decode_overview_cursor(raw: Option<&str>) -> CoreResult<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() > 256 {
        return Err(CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor too long".to_string(),
        });
    }
    let world_id =
        raw.strip_prefix(OVERVIEW_CURSOR_PREFIX)
            .ok_or_else(|| CoreError::InvalidInput {
                field: "cursor".to_string(),
                reason: "invalid cursor format".to_string(),
            })?;
    if world_id.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor is empty".to_string(),
        });
    }
    Ok(Some(world_id.to_string()))
}

fn encode_overview_cursor(world_id: &str) -> String {
    format!("{OVERVIEW_CURSOR_PREFIX}{world_id}")
}

/// Decode an opaque keyset cursor into `(branch_id, sequence_no)`.
///
/// Format: `ev1:<branch_id>:<sequence_no>`. Branch ids are `fbk_*` /
/// `fbk_root` (no colons), so the last `:` separator is unambiguous.
fn decode_events_cursor(raw: &str) -> CoreResult<(String, i64)> {
    if raw.len() > 512 {
        return Err(CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor too long".to_string(),
        });
    }
    let payload =
        raw.strip_prefix(EVENTS_CURSOR_PREFIX)
            .ok_or_else(|| CoreError::InvalidInput {
                field: "cursor".to_string(),
                reason: "invalid cursor format".to_string(),
            })?;
    let (branch_id, seq_str) = payload
        .rsplit_once(':')
        .ok_or_else(|| CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor format".to_string(),
        })?;
    if branch_id.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor branch is empty".to_string(),
        });
    }
    let sequence_no = seq_str
        .parse::<i64>()
        .map_err(|_| CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor sequence".to_string(),
        })?;
    if sequence_no < 0 {
        return Err(CoreError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor sequence".to_string(),
        });
    }
    Ok((branch_id.to_string(), sequence_no))
}

fn encode_events_cursor(branch_id: &str, sequence_no: i64) -> String {
    format!("{EVENTS_CURSOR_PREFIX}{branch_id}:{sequence_no}")
}

/// Parse the `status` filter param; defaults to `canon`.
fn parse_status_filter(raw: Option<&str>) -> CoreResult<&str> {
    match raw {
        None => Ok("canon"),
        Some(s @ ("canon" | "provisional" | "rejected")) => Ok(s),
        Some(other) => Err(CoreError::InvalidInput {
            field: "status".to_string(),
            reason: format!("invalid status '{other}'; expected canon|provisional|rejected"),
        }),
    }
}

fn parse_json_array(raw: Option<&str>) -> Option<Vec<String>> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
}

fn parse_json_object(raw: Option<&str>) -> Option<Map<String, Value>> {
    raw.and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok())
}

/// Map a DB status string to the wire enum. The DB CHECK constraint
/// guarantees `canon|provisional|rejected`, so failure indicates a schema
/// regression.
fn map_status(s: &str) -> CoreResult<TimelineEventInfoStatus> {
    match s {
        "canon" => Ok(TimelineEventInfoStatus::Canon),
        "provisional" => Ok(TimelineEventInfoStatus::Provisional),
        "rejected" => Ok(TimelineEventInfoStatus::Rejected),
        other => Err(CoreError::Internal {
            category: format!("unexpected timeline event status '{other}' in database"),
        }),
    }
}

/// Map a `narrative_timeline_events` page row to the wire `TimelineEventInfo`.
///
/// JSON columns are parsed leniently (malformed stored JSON degrades to
/// `None` / empty rather than failing the page); `created_at` handles both
/// `RFC3339` and SQLite `datetime('now')` formats via the shared
/// `nexus_narrative::timeline_event::parse_created_at`.
fn event_info(r: TimelineEventPageRow) -> CoreResult<TimelineEventInfo> {
    Ok(TimelineEventInfo {
        id: r.timeline_event_id,
        branch_id: r.branch_id,
        event_type: r.event_type,
        status: map_status(&r.status)?,
        #[allow(clippy::cast_sign_loss)]
        sequence_no: u64::try_from(r.sequence_no).unwrap_or(0),
        title: r.title,
        summary: r.summary,
        affected_key_block_ids: parse_json_array(r.affected_key_block_ids_json.as_deref()),
        caused_by_event_ids: parse_json_array(r.caused_by_event_ids_json.as_deref()),
        source_command_id: r.source_command_id,
        metadata: parse_json_object(r.metadata_json.as_deref()).unwrap_or_default(),
        extensions: parse_json_object(r.extensions_nexus_json.as_deref()),
        // V1.164 P3 T1 (AR-2): carry the functional-dialect modules verbatim
        // from `narrative_timeline_events.modules_json` (NULL → empty map,
        // omitted from the wire via `skip_serializing_if` — schema type
        // `object`, absent when unrecorded).
        modules: parse_json_object(r.modules_json.as_deref()).unwrap_or_default(),
        created_at: nexus_narrative::timeline_event::parse_created_at(&r.created_at)
            .unwrap_or(chrono::DateTime::UNIX_EPOCH),
    })
}
