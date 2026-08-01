//! Daemon route — `GET /v1/daemon/worlds/:world_id/timeline/events`
//! (V1.147 P2 T1): cursor-paginated per-World timeline-events read.
//!
//! Serves the production `narrative_timeline_events` storage (write path:
//! P0 Accept appends `compute_result` events with `extensions_nexus_json`
//! provenance). Filters: optional `branch_id` (defaults to the World's
//! current branch — `root_fork_branch_id`, root fallback), `status`
//! (`canon|provisional|rejected`, default `canon`), `event_type`
//! (exact match). Keyset cursor on `(branch_id, sequence_no)` per the
//! `UNIQUE (world_id, branch_id, sequence_no)` index.
//!
//! Ownership guard: world must exist (404) and be owned by the active
//! creator (403) before any read.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::timeline::list_timeline_events_response::{
    ListTimelineEventsResponse, TimelineEventInfo, TimelineEventInfoStatus,
};
use nexus_local_db::narrative_gateway::list_timeline_events_page;
use nexus_local_db::narrative_write::is_world_owned;
use serde::Deserialize;
use serde_json::{Map, Value};
use tracing::info;

const DEFAULT_PAGE_SIZE: u32 = 20;
const MAX_PAGE_SIZE: u32 = 100;
/// Opaque keyset cursor prefix (`ev1:` — future encodings may coexist).
const CURSOR_PREFIX: &str = "ev1:";
/// Legacy root branch fallback when `narrative_worlds.root_fork_branch_id`
/// is unset (matches `resolve_run_branch` in `compute_runs.rs`).
const ROOT_BRANCH_FALLBACK: &str = "fbk_root";

#[derive(Debug, Deserialize)]
pub struct TimelineEventsParams {
    pub branch_id: Option<String>,
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// Decode an opaque keyset cursor into `(branch_id, sequence_no)`.
///
/// Format: `ev1:<branch_id>:<sequence_no>`. Branch ids are `fbk_*` /
/// `fbk_root` (no colons), so the last `:` separator is unambiguous.
fn decode_cursor(raw: &str) -> Result<(String, i64), NexusApiError> {
    if raw.len() > 512 {
        return Err(NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor too long".to_string(),
        });
    }
    let payload = raw
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor format".to_string(),
        })?;
    let (branch_id, seq_str) =
        payload
            .rsplit_once(':')
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "cursor".to_string(),
                reason: "invalid cursor format".to_string(),
            })?;
    if branch_id.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "cursor branch is empty".to_string(),
        });
    }
    let seq = seq_str
        .parse::<i64>()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor sequence".to_string(),
        })?;
    if seq < 0 {
        return Err(NexusApiError::InvalidInput {
            field: "cursor".to_string(),
            reason: "invalid cursor sequence".to_string(),
        });
    }
    Ok((branch_id.to_string(), seq))
}

fn encode_cursor(branch_id: &str, sequence_no: i64) -> String {
    format!("{CURSOR_PREFIX}{branch_id}:{sequence_no}")
}

/// Parse the `status` filter param; defaults to `canon`.
fn parse_status_filter(raw: Option<&str>) -> Result<&str, NexusApiError> {
    match raw {
        None => Ok("canon"),
        Some(s @ ("canon" | "provisional" | "rejected")) => Ok(s),
        Some(other) => Err(NexusApiError::InvalidInput {
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
/// guarantees `canon|provisional|rejected`, so failure indicates a
/// schema regression.
fn map_status(s: &str) -> Result<TimelineEventInfoStatus, NexusApiError> {
    match s {
        "canon" => Ok(TimelineEventInfoStatus::Canon),
        "provisional" => Ok(TimelineEventInfoStatus::Provisional),
        "rejected" => Ok(TimelineEventInfoStatus::Rejected),
        other => Err(NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("unexpected timeline event status '{other}' in database"),
        }),
    }
}

/// Map a `narrative_timeline_events` page row to the wire `TimelineEventInfo`.
///
/// JSON columns are parsed leniently (malformed stored JSON degrades to
/// `None` / empty rather than failing the page); `created_at` handles both
/// `RFC3339` and `SQLite` `datetime('now')` formats via the shared
/// `nexus_narrative::timeline_event::parse_created_at`.
fn rows_to_items(
    r: nexus_local_db::narrative_gateway::TimelineEventPageRow,
) -> Result<TimelineEventInfo, NexusApiError> {
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
        created_at: nexus_narrative::timeline_event::parse_created_at(&r.created_at)
            .unwrap_or(chrono::DateTime::UNIX_EPOCH),
    })
}

/// `GET /v1/daemon/worlds/:world_id/timeline/events`
///
/// Ownership guard runs before any read: world must exist (404) and be owned
/// by the active creator (403). Branch filter defaults to the World's current
/// branch (`root_fork_branch_id`, falling back to `fbk_root`).
#[allow(clippy::missing_errors_doc)]
pub async fn get_timeline_events(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(params): Query<TimelineEventsParams>,
) -> Result<Json<ListTimelineEventsResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // World existence (404) + root branch for the default branch filter.
    // Compile-time checked query (daemon-runtime AGENTS.md mandatory rule).
    let world = sqlx::query!(
        r#"SELECT root_fork_branch_id as "root_fork_branch_id" FROM narrative_worlds WHERE world_id = ?"#,
        world_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?
    .ok_or_else(|| NexusApiError::NotFound(format!("world {world_id} not found")))?;

    // Ownership guard (403) before any read.
    let owned = is_world_owned(pool, &creator_id, &world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "you do not own this world".to_string(),
        });
    }

    // Resolve the effective branch: explicit param, else world's current
    // branch (root fallback, matching `resolve_run_branch`).
    let root_branch = world
        .root_fork_branch_id
        .unwrap_or_else(|| ROOT_BRANCH_FALLBACK.to_string());
    // The route always reads a single branch (param or default), so the
    // filter is always `Some`; keep the Option for the page-query signature.
    let branch_filter = Some(
        params
            .branch_id
            .as_deref()
            .unwrap_or(&root_branch)
            .to_string(),
    );

    // Status filter (enum, default canon) + event_type exact-match.
    let status_filter = parse_status_filter(params.status.as_deref())?;
    let event_type_filter = params.event_type.as_deref();

    // Keyset cursor on (branch_id, sequence_no).
    let cursor = match params.cursor.as_deref() {
        None => None,
        Some(raw) => Some(decode_cursor(raw)?),
    };
    // `list_timeline_events_page` takes `Option<(&str, i64)>`; the handler
    // owns the decoded values so reborrowing here is safe.
    let cursor_ref = cursor.as_ref().map(|(b, s)| (b.as_str(), *s));

    let limit = params.limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE);
    // W-1 (QC): `limit=0` must not report `has_more` — the wire contract
    // states `has_more` is equivalent to `next_cursor` being non-null, and a
    // `has_more=true, next_cursor=null` response would growth-loop a keyset
    // client (null cursor → re-request page 1). An empty page with no
    // continuation is the only honest answer.
    if limit == 0 {
        return Ok(Json(ListTimelineEventsResponse {
            items: Vec::new(),
            has_more: false,
            next_cursor: None,
        }));
    }
    // Fetch one extra row to detect has_more.
    let fetch_limit = i64::from(limit) + 1;

    let rows = list_timeline_events_page(
        pool,
        &world_id,
        branch_filter.as_deref(),
        Some(status_filter),
        event_type_filter,
        cursor_ref,
        fetch_limit,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    // `fetch_limit = limit + 1`; more rows than `limit` means another page.
    // Rows are capped by the SQL LIMIT (i64), so this cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    let has_more = rows.len() as u32 > limit;
    let page_rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();

    let next_cursor = if has_more {
        page_rows
            .last()
            .map(|r| encode_cursor(&r.branch_id, r.sequence_no))
    } else {
        None
    };

    let items = page_rows
        .into_iter()
        .map(rows_to_items)
        .collect::<Result<Vec<_>, NexusApiError>>()?;

    info!(
        "timeline_events: world={world_id} branch={branch:?} status={status:?} page={} has_more={has_more}",
        items.len(),
        branch = branch_filter,
        status = status_filter,
    );

    // typify inlines the `TimelineEventInfo` item into the response module;
    // constructing that inlined copy directly is the repo pattern (same as
    // compute_runs builds `run_list_response::NexusRunSummary`).
    Ok(Json(ListTimelineEventsResponse {
        items,
        has_more,
        next_cursor,
    }))
}
