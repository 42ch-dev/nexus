//! World rules read surface (V1.166 P1 T4 / DR-64, AR-3) —
//! `GET /v1/daemon/worlds/:world_id/rules`.
//!
//! Serves a world's structured rules (`spoke_rules` table — the CLI-first
//! authoring home, PD-1/PD-2) to Control Room / agents. Read-only, additive
//! wire (AR-3: `wire_contracts_changed = true`).
//!
//! # Guards (authz parity — AR-3)
//!
//! Identical to `kb/graph`, `timeline/events`, and the world findings read
//! surface via the shared [`require_creator`] + [`require_world_owner`]
//! (`world_kb_guards.rs`): 401 no active creator; 404 unknown world; 403
//! cross-author (`owner_creator_id` mismatch).
//!
//! # Shape
//!
//! Items project the spoke `Rule` author metadata **verbatim** — `kind`,
//! `severity_hint`, `status`, `target_entry_types` are open spoke vocabulary
//! with no nexus coercion at rest (PD-1) — plus the AR-2 constraint carrier
//! surfaced **first-class** from `extensions["nexus"]["constraint"]`
//! (absent/malformed → omitted; the `extensions` bag itself is not
//! exposed). The projection converts the stored INTEGER Unix-epoch
//! timestamps to RFC 3339.
//!
//! # Ordering + cap (AR-3)
//!
//! Store order is `canonical_name ASC, rule_id ASC` (author-metadata list,
//! not newest-first — PD-1 list order). The store bounds the read SQL-side
//! via `LIMIT ?` (Bugbot 4bad2fca idiom — same as the findings route); the
//! route applies the safety cap of the first 500 of that order and reports
//! `truncated: true` when the probe exceeded the cap — an honest flag on a
//! read surface. Pagination lands with the Control Room panel (plan
//! roadmap). Owned world with zero rules → 200 + `{"rules": [],
//! "truncated": false}`.
//!
//! # Errors
//!
//! 401 / 404 / 403 per the guard chain; 500 on storage failure.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::worlds::world_rules_list_response::{
    WorldRulesListResponse, WorldRulesListResponseRulesItem,
};
use nexus_local_db::spoke_rules::{list_rules_by_world_limited, SpokeRuleRow};
use serde_json::{Map, Value};

/// Safety cap on the read surface: the first 500 rules per world in store
/// order (`canonical_name ASC, rule_id ASC` — AR-3). Pagination lands with
/// the Control Room panel — roadmap.
const WORLD_RULES_CAP: usize = 500;

/// SQL-side probe bound for the store query: one past [`WORLD_RULES_CAP`],
/// so the `LIMIT ?` returns the overflow row and `truncated` stays honest
/// without loading the full set (Bugbot 4bad2fca). Derived from the cap so
/// the two cannot drift.
#[allow(clippy::cast_possible_wrap)] // const-evaluated literal (500): always fits i64
const WORLD_RULES_PROBE: i64 = WORLD_RULES_CAP as i64 + 1;

/// `GET /v1/daemon/worlds/:world_id/rules` — list a world's structured
/// rules, `canonical_name ASC, rule_id ASC`, capped at [`WORLD_RULES_CAP`].
#[allow(clippy::missing_errors_doc)]
pub async fn list_world_rules(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<WorldRulesListResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    // Fetch one past the cap (501): the store bounds the read SQL-side via
    // `LIMIT ?` (Bugbot 4bad2fca) — the +1 probe returns the single row
    // just beyond the cap so `truncated` below stays honest without ever
    // loading the full set.
    let rows = list_rules_by_world_limited(pool, &world_id, WORLD_RULES_PROBE)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Honest truncation flag: more stored rows than the cap → `truncated:
    // true`, response carries the first 500 (store order is
    // canonical_name ASC, rule_id ASC).
    let truncated = rows.len() > WORLD_RULES_CAP;
    let rules = rows
        .into_iter()
        .take(WORLD_RULES_CAP)
        .map(row_to_item)
        .collect();

    Ok(Json(WorldRulesListResponse { rules, truncated }))
}

/// Project one `spoke_rules` row onto the wire item.
///
/// JSON columns are parsed leniently (malformed stored JSON degrades to
/// empty rather than failing the list — mirrors the `world_findings`
/// `row_to_item` idiom); epoch seconds → RFC 3339 via `chrono`, `None` for
/// unknown epochs (the columns are nullable).
fn row_to_item(r: SpokeRuleRow) -> WorldRulesListResponseRulesItem {
    WorldRulesListResponseRulesItem {
        rule_id: r.rule_id,
        canonical_name: r.canonical_name,
        kind: r.kind,
        statement: r.statement,
        description: r.description,
        severity_hint: r.severity_hint,
        status: r.status,
        // Spoke vocabulary verbatim; malformed stored JSON → empty
        // (all-types) targeting, same lenient read as the findings route.
        target_entry_types: serde_json::from_str::<Vec<String>>(&r.target_entry_types_json)
            .unwrap_or_default(),
        // AR-3 first-class carrier: `extensions["nexus"]["constraint"]`.
        // Absent/malformed → empty map (the wire omits it via the
        // generated `skip_serializing_if`). The extensions bag itself is
        // NOT exposed.
        constraint: constraint_from_extensions(r.extensions_json.as_str()),
        created_at: r.created_at.and_then(epoch_to_rfc3339),
        updated_at: r.updated_at.and_then(epoch_to_rfc3339),
    }
}

/// Extract the AR-2 constraint carrier from the stored `extensions` bag:
/// `extensions["nexus"]["constraint"]` as a JSON object. Absent namespace /
/// absent key / malformed carrier → empty map (omitted on the wire).
fn constraint_from_extensions(extensions_json: &str) -> Map<String, Value> {
    let Ok(Value::Object(extensions)) = serde_json::from_str::<Value>(extensions_json) else {
        return Map::new();
    };
    match extensions
        .get("nexus")
        .and_then(|nexus| nexus.get("constraint"))
    {
        Some(Value::Object(carrier)) => carrier.clone(),
        _ => Map::new(),
    }
}

/// Convert Unix-epoch seconds to an RFC 3339 UTC datetime (`None` for
/// out-of-range epochs or unknown/null stored timestamps).
const fn epoch_to_rfc3339(epoch: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp(epoch, 0)
}
