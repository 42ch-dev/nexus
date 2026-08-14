//! World findings read surface (V1.165 P1 T3 / DR-68, AR-3) —
//! `GET /v1/daemon/worlds/:world_id/findings`.
//!
//! Serves world-attached mental-layer check findings (`world_findings`
//! table — the AR-1 home) to Control Room / agents. Read-only, additive
//! wire (AR-6: P1 `wire_contracts_changed = true`).
//!
//! # Guards (authz parity — AR-3)
//!
//! Identical to `kb/graph`, `kb/candidates`, `timeline/events` via the
//! shared [`require_creator`] + [`require_world_owner`]
//! (`world_kb_guards.rs`): 401 no active creator; 404 unknown world; 403
//! cross-author (`owner_creator_id` mismatch).
//!
//! # Shape
//!
//! Items mirror the spoke `Finding` wire shape already carried by
//! `check-response.schema.json` — the check route returns the same shape
//! verbatim. The projection converts the stored INTEGER Unix-epoch
//! timestamps to RFC 3339 (spoke `Timestamp` = RFC 3339 date-time) and
//! rehydrates the stored JSON columns (`source_anchor` / `text_position` /
//! `extensions`). Severity/status are spoke vocabulary **verbatim**
//! (AR-1: no nexus mapping on the world path — world findings are advisory
//! check output, not Work manuscript findings).
//!
//! # Ordering + cap (AR-3)
//!
//! `ORDER BY created_at DESC, finding_id ASC` (legacy list convention) is
//! the store's query; the route applies the safety cap of the newest 500
//! and reports `truncated: true` when more rows exist — an honest flag on
//! a read surface. Pagination lands with the Control Room panel (plan
//! roadmap). Owned world with zero findings → 200 +
//! `{"findings": [], "truncated": false}` (PD-3).
//!
//! # Errors
//!
//! 401 / 404 / 403 per the guard chain; 500 on storage failure.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::worlds::world_findings_list_response::{
    WorldFindingsListResponse, WorldFindingsListResponseFindingsItem,
};
use nexus_local_db::world_findings::{list_world_findings_by_world, WorldFindingRow};
use serde_json::{Map, Value};
use std::num::NonZeroU64;

/// Safety cap on the read surface: the newest 500 findings per world
/// (AR-3). Pagination lands with the Control Room panel — roadmap.
const WORLD_FINDINGS_CAP: usize = 500;

/// `GET /v1/daemon/worlds/:world_id/findings` — list world-attached
/// check findings, newest-first, capped at [`WORLD_FINDINGS_CAP`].
#[allow(clippy::missing_errors_doc)]
pub async fn list_world_findings(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<WorldFindingsListResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    let rows = list_world_findings_by_world(pool, &world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Honest truncation flag: more stored rows than the cap → `truncated:
    // true`, response carries the newest 500 (store order is newest-first).
    let truncated = rows.len() > WORLD_FINDINGS_CAP;
    let findings = rows
        .into_iter()
        .take(WORLD_FINDINGS_CAP)
        .map(row_to_item)
        .collect();

    Ok(Json(WorldFindingsListResponse {
        findings,
        truncated,
    }))
}

/// Project one `world_findings` row onto the wire item.
///
/// JSON columns are parsed leniently (malformed stored JSON degrades to
/// `None` / empty rather than failing the list — mirrors the
/// `timeline_events` `rows_to_items` read idiom); epoch seconds → RFC 3339
/// via `chrono`, falling back to `None` for out-of-range epochs.
fn row_to_item(r: WorldFindingRow) -> WorldFindingsListResponseFindingsItem {
    WorldFindingsListResponseFindingsItem {
        finding_id: r.finding_id,
        schema_version: NonZeroU64::new(u64::try_from(r.schema_version).unwrap_or(1))
            .unwrap_or(NonZeroU64::MIN),
        severity: r.severity,
        status: r.status,
        title: r.title,
        description: r.description,
        kind: r.kind,
        target_entry_id: r.target_entry_id,
        source_anchor: r
            .source_anchor_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok()),
        suggested_fix: r.suggested_fix,
        // The column is NOT NULL DEFAULT '{}' — verbatim spoke Map.
        text_position: parse_json_object(Some(r.text_position_json.as_str())).unwrap_or_default(),
        // The column is NOT NULL DEFAULT '{}' — verbatim spoke ExtensionMap
        // (incl. the stamped `extensions.nexus.world_id` / `creator_id`).
        extensions: parse_json_object(Some(r.extensions_json.as_str())).unwrap_or_default(),
        created_at: epoch_to_rfc3339(r.created_at),
        updated_at: epoch_to_rfc3339(r.updated_at),
    }
}

/// Parse a stored JSON object column leniently (`None` when absent or
/// malformed) — same idiom as `timeline_events::parse_json_object`.
fn parse_json_object(raw: Option<&str>) -> Option<Map<String, Value>> {
    raw.and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok())
}

/// Convert Unix-epoch seconds to an RFC 3339 UTC datetime (`None` for
/// out-of-range epochs — the column is NOT NULL, so valid rows always map).
const fn epoch_to_rfc3339(epoch: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp(epoch, 0)
}
