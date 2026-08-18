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
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::daemon_api::worlds::world_rule_create_request::WorldRuleCreateRequest;
use nexus_contracts::daemon_api::worlds::world_rule_response::WorldRuleResponse;
use nexus_contracts::daemon_api::worlds::world_rule_update_request::WorldRuleUpdateRequest;
use nexus_contracts::daemon_api::worlds::world_rules_list_response::{
    WorldRulesListResponse, WorldRulesListResponseRulesItem,
};
use nexus_local_db::spoke_rules::{
    get_spoke_rules_by_ids, insert_rule, list_rules_by_world_limited, update_rule, RuleUpdate,
    SpokeRuleRow,
};
use nexus_spoke_adapter::constraint::{parse_carrier_json_member, Constraint};
use serde_json::{json, Map, Value};

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

/// Project a `spoke_rules` row onto the single-item write response
/// (V1.169 P1, AR-1): the `WorldRuleResponse` schema is the
/// `WorldRulesListResponseRulesItem` shape verbatim, so the projection
/// reuses [`row_to_item`] and converts nominally.
fn item_to_response(item: WorldRulesListResponseRulesItem) -> WorldRuleResponse {
    WorldRuleResponse {
        rule_id: item.rule_id,
        canonical_name: item.canonical_name,
        kind: item.kind,
        statement: item.statement,
        description: item.description,
        severity_hint: item.severity_hint,
        status: item.status,
        target_entry_types: item.target_entry_types,
        constraint: item.constraint,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

/// Convert Unix-epoch seconds to an RFC 3339 UTC datetime (`None` for
/// out-of-range epochs or unknown/null stored timestamps).
const fn epoch_to_rfc3339(epoch: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp(epoch, 0)
}

/// AR-2 projection: `constraint.{member}`, with the carrier-level member
/// (`"constraint"` — non-object root / closed-shape reject) projecting to
/// the bare `constraint` field (locks AR-2: "root → `constraint`").
fn constraint_field(member: &'static str) -> String {
    if member == "constraint" {
        "constraint".to_string()
    } else {
        format!("constraint.{member}")
    }
}

/// `POST /v1/daemon/worlds/:world_id/rules` — create a structured rule
/// (V1.169 P1, AR-2/AR-3/AR-5).
///
/// Mirrors the CLI `rule add` row assembly: guards (401/404/403) → carrier
/// grammar via the member-aware seam (`constraint.*` envelope errors) →
/// `observer_cardinality` × `target_entry_types` conflict (effective pair,
/// both fresh at create) → meta-field value checks (`canonical_name` /
/// `statement` trim; `status` core set; `severity_hint` / `kind`
/// non-empty) → server-side `rul_<uuid v4 simple>` mint → full-row insert →
/// 201 + item.
///
/// Create defaults (AR-3): `status` omitted → `active`; `kind` omitted →
/// `rule`; `severity_hint` omitted → NULL; `target_entry_types` omitted →
/// `[]`; `description` / `source_anchor_json` → NULL;
/// `extensions_json` = `{"nexus":{"constraint":<carrier>}}` (namespace
/// written fresh at create — CLI row-assembly parity);
/// `created_at` = `updated_at` = now epoch; `schema_version` = 1.
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
// ^ the locked fail-early validation order (AR-5) is one cohesive chain;
// splitting would obscure the contract order it documents (works.rs
// create_work precedent).
pub async fn create_world_rule(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldRuleCreateRequest>,
) -> Result<(StatusCode, Json<WorldRuleResponse>), NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    // AR-2 seam: the carrier grammar lives in the spoke adapter (sole
    // consumer) — the daemon never parses carriers itself. The
    // member-aware error projects onto the closed `constraint.*` envelope
    // vocabulary (no message string-sniffing).
    let constraint =
        parse_carrier_json_member(&Value::Object(req.constraint.clone())).map_err(|e| {
            NexusApiError::InvalidInput {
                field: constraint_field(e.member),
                reason: e.reason,
            }
        })?;

    // AR-5: observer_cardinality applies to timeline events (no
    // entry_type) — combining with target_entry_types is rejected on the
    // effective pair (both fresh at create), never silently ignored (CLI
    // parity, rule.rs:197-207).
    if !req.target_entry_types.is_empty()
        && matches!(constraint, Constraint::ObserverCardinality { .. })
    {
        return Err(NexusApiError::InvalidInput {
            field: "target_entry_types".to_string(),
            reason: "target_entry_types cannot be combined with an observer_cardinality \
                     constraint: observer_cardinality applies to timeline events, which \
                     carry no entry_type"
                .to_string(),
        });
    }

    // AR-2: target_entry_types members must be non-empty ([] is meaningful
    // — all entry types in check scope — but [""] is not).
    if req.target_entry_types.iter().any(|t| t.trim().is_empty()) {
        return Err(NexusApiError::InvalidInput {
            field: "target_entry_types".to_string(),
            reason: "target_entry_types members must be non-empty strings".to_string(),
        });
    }

    // AR-2 meta-field value checks (handler-side; the schemas are
    // type-only per AR-1 so these surface through the envelope).
    let canonical_name = req.canonical_name.trim().to_string();
    if canonical_name.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "canonical_name".to_string(),
            reason: "canonical_name must be a non-empty string after trimming".to_string(),
        });
    }
    let statement = req.statement.trim().to_string();
    if statement.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "statement".to_string(),
            reason: "statement must be a non-empty string after trimming".to_string(),
        });
    }
    let status = req.status.as_deref().unwrap_or("active");
    if !matches!(status, "draft" | "active" | "deprecated") {
        return Err(NexusApiError::InvalidInput {
            field: "status".to_string(),
            reason: format!("status must be one of draft | active | deprecated, got {status:?}"),
        });
    }
    if let Some(severity_hint) = req.severity_hint.as_deref() {
        if severity_hint.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "severity_hint".to_string(),
                reason: "severity_hint must be a non-empty string when present".to_string(),
            });
        }
    }
    if let Some(kind) = req.kind.as_deref() {
        if kind.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "kind".to_string(),
                reason: "kind must be a non-empty string when present".to_string(),
            });
        }
    }

    // AR-2 id minting (V1.166 AR-2): rul_ ++ uuid v4 simple (32 hex, no
    // hyphens), minted server-side — immutable, path-addressed, never a
    // DTO field.
    let rule_id = format!("rul_{}", uuid::Uuid::new_v4().simple());
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default();

    let row = SpokeRuleRow {
        rule_id: rule_id.clone(),
        world_id,
        schema_version: 1,
        canonical_name,
        kind: req.kind.clone().unwrap_or_else(|| "rule".to_string()),
        statement: Some(statement),
        description: None,
        target_entry_types_json: serde_json::to_string(&req.target_entry_types).map_err(|e| {
            NexusApiError::Internal {
                code: "SERIALIZATION_ERROR".to_string(),
                message: e.to_string(),
            }
        })?,
        severity_hint: req.severity_hint.clone(),
        status: Some(status.to_string()),
        source_anchor_json: None,
        // AR-2/AR-3 CLI row-assembly parity: the nexus namespace is written
        // fresh at create with the carrier verbatim (rule.rs:230).
        extensions_json: json!({ "nexus": { "constraint": req.constraint } }).to_string(),
        created_at: Some(now_epoch),
        updated_at: Some(now_epoch),
    };
    insert_rule(pool, &row)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok((
        StatusCode::CREATED,
        Json(item_to_response(row_to_item(row))),
    ))
}

/// `PATCH /v1/daemon/worlds/:world_id/rules/:rule_id` — per-field edit
/// (V1.169 P1, AR-2/AR-3/AR-5/AR-6).
///
/// Validation order is contract (AR-5): guards (401/404/403) → current-row
/// fetch + world filter (404 — a `rule_id` that belongs to a different
/// world is indistinguishable from an unknown id, AR-6) → empty-PATCH
/// reject (`patch`) → carrier validation if provided (`constraint.*`) →
/// effective-pair `observer_cardinality` × `target_entry_types` conflict →
/// meta-field value checks → `RuleUpdate` assembly (whole-carrier
/// replacement preserves the rest of the extensions bag, AR-3) →
/// `update_rule` (`Ok(false)` → 404) → re-read → 200 + item.
///
/// `status=deprecated` is the Deactivate recovery (product lock — no
/// DELETE route).
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
// ^ the locked validation order (AR-5) is one cohesive chain; splitting
// would obscure the contract order it documents (works.rs patch_work
// precedent).
pub async fn update_world_rule(
    State(state): State<WorkspaceState>,
    Path((world_id, rule_id)): Path<(String, String)>,
    Json(req): Json<WorldRuleUpdateRequest>,
) -> Result<Json<WorldRuleResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    // AR-5 order: addressing precedes payload. The pre-fetch is
    // world-scoped (filter below): a rule_id owned by a different world —
    // even another world of the same creator — is 404 naming only the id,
    // with no existence leak (AR-6).
    let rows = get_spoke_rules_by_ids(pool, std::slice::from_ref(&rule_id))
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    let Some(current) = rows.into_iter().find(|row| row.world_id == world_id) else {
        return Err(NexusApiError::NotFound(format!("rule {rule_id}")));
    };

    // AR-3: empty PATCH (no mutable field present) → 400 field=`patch` —
    // fail-early beats a no-op write that would still refresh updated_at.
    // `constraint` counts as present only when non-empty (typify's
    // generated `Map` treats absent and `{}` identically via
    // `skip_serializing_if = Map::is_empty`; an empty carrier is invalid
    // anyway, so it cannot be a legitimate PATCH payload).
    if req.canonical_name.is_none()
        && req.statement.is_none()
        && req.severity_hint.is_none()
        && req.status.is_none()
        && req.kind.is_none()
        && req.target_entry_types.is_none()
        && req.constraint.is_empty()
    {
        return Err(NexusApiError::InvalidInput {
            field: "patch".to_string(),
            reason: "at least one of canonical_name | statement | severity_hint | status | \
                     kind | target_entry_types | constraint is required"
                .to_string(),
        });
    }

    // AR-2 seam: validate the provided carrier member-aware before any
    // assembly (whole-carrier replacement, AR-3).
    let provided_constraint = if req.constraint.is_empty() {
        None
    } else {
        Some(
            parse_carrier_json_member(&Value::Object(req.constraint.clone())).map_err(|e| {
                NexusApiError::InvalidInput {
                    field: constraint_field(e.member),
                    reason: e.reason,
                }
            })?,
        )
    };

    // AR-5: effective-pair conflict — family = provided carrier's family
    // or stored family (via the read-side projection), target set =
    // provided or stored; rejected on `target_entry_types` regardless of
    // which side each half came from (the pair rule, not grammar — the
    // daemon writes no second parser, AR-7).
    let effective_observer_cardinality = if let Some(c) = provided_constraint {
        matches!(c, Constraint::ObserverCardinality { .. })
    } else {
        let stored = constraint_from_extensions(current.extensions_json.as_str());
        stored.get("family").and_then(Value::as_str) == Some("observer_cardinality")
    };
    // Lenient stored parse (read-projection parity): malformed stored JSON
    // degrades to the empty set.
    let stored_targets =
        serde_json::from_str::<Vec<String>>(&current.target_entry_types_json).unwrap_or_default();
    let effective_target_types: &[String] =
        req.target_entry_types.as_deref().unwrap_or(&stored_targets);
    if effective_observer_cardinality && !effective_target_types.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "target_entry_types".to_string(),
            reason: "target_entry_types cannot be combined with an observer_cardinality \
                     constraint: observer_cardinality applies to timeline events, which \
                     carry no entry_type"
                .to_string(),
        });
    }

    // AR-2 meta-field value checks — provided fields only (per-field
    // replace, AR-3).
    if let Some(targets) = req.target_entry_types.as_ref() {
        if targets.iter().any(|t| t.trim().is_empty()) {
            return Err(NexusApiError::InvalidInput {
                field: "target_entry_types".to_string(),
                reason: "target_entry_types members must be non-empty strings".to_string(),
            });
        }
    }
    let canonical_name = match req.canonical_name.as_ref() {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(NexusApiError::InvalidInput {
                    field: "canonical_name".to_string(),
                    reason: "canonical_name must be a non-empty string after trimming".to_string(),
                });
            }
            Some(trimmed.to_string())
        }
        None => None,
    };
    let statement = match req.statement.as_ref() {
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(NexusApiError::InvalidInput {
                    field: "statement".to_string(),
                    reason: "statement must be a non-empty string after trimming".to_string(),
                });
            }
            Some(trimmed.to_string())
        }
        None => None,
    };
    if let Some(status) = req.status.as_deref() {
        if !matches!(status, "draft" | "active" | "deprecated") {
            return Err(NexusApiError::InvalidInput {
                field: "status".to_string(),
                reason: format!(
                    "status must be one of draft | active | deprecated, got {status:?}"
                ),
            });
        }
    }
    if let Some(severity_hint) = req.severity_hint.as_deref() {
        if severity_hint.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "severity_hint".to_string(),
                reason: "severity_hint must be a non-empty string when present".to_string(),
            });
        }
    }
    if let Some(kind) = req.kind.as_deref() {
        if kind.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "kind".to_string(),
                reason: "kind must be a non-empty string when present".to_string(),
            });
        }
    }

    // AR-4 assembly: pre-serialized JSON columns; storage stays opaque
    // spoke-vocabulary-free (sole-consumer rule).
    let mut update = RuleUpdate {
        canonical_name,
        statement,
        severity_hint: req.severity_hint.clone(),
        status: req.status.clone(),
        kind: req.kind.clone(),
        target_entry_types_json: req
            .target_entry_types
            .as_ref()
            .map(|v| {
                serde_json::to_string(v).map_err(|e| NexusApiError::Internal {
                    code: "SERIALIZATION_ERROR".to_string(),
                    message: e.to_string(),
                })
            })
            .transpose()?,
        extensions_json: None,
    };

    // AR-3 whole-carrier replacement: only extensions.nexus.constraint is
    // overwritten — the rest of the nexus namespace and all other
    // namespaces survive. Malformed stored extensions_json → 500
    // fail-closed (storage corruption; rows written by CLI/API are always
    // valid JSON — same spirit as host_manifest_port's corrupt-row
    // contract).
    if !req.constraint.is_empty() {
        update.extensions_json = Some(
            replace_constraint_in_extensions(current.extensions_json.as_str(), &req.constraint)
                .map_err(|e| NexusApiError::Internal {
                    code: "EXTENSIONS_CORRUPT".to_string(),
                    message: e,
                })?,
        );
    }

    // AR-4: Ok(false) = unknown id OR foreign world (storage does not
    // distinguish) → 404 naming only the id (AR-6).
    if !update_rule(pool, &world_id, &rule_id, &update)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
    {
        return Err(NexusApiError::NotFound(format!("rule {rule_id}")));
    }

    // Re-read for the response item (AR-4: no new fetch fn).
    let rows = get_spoke_rules_by_ids(pool, std::slice::from_ref(&rule_id))
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    let Some(row) = rows.into_iter().next() else {
        return Err(NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("rule {rule_id} vanished after a matched update"),
        });
    };

    Ok(Json(item_to_response(row_to_item(row))))
}

/// AR-3 whole-carrier replacement: overwrite `extensions["nexus"]
/// ["constraint"]` with `carrier`, preserving the rest of the extensions
/// bag — other nexus keys + all other namespaces. A missing nexus
/// namespace is created (the carrier is being set); a nexus value that is
/// not an object is storage corruption → `Err` (the caller fails closed
/// with 500).
fn replace_constraint_in_extensions(
    extensions_json: &str,
    carrier: &Map<String, Value>,
) -> Result<String, String> {
    let mut extensions: Value = serde_json::from_str(extensions_json)
        .map_err(|e| format!("stored extensions_json is not valid JSON: {e}"))?;
    let extensions_obj = extensions
        .as_object_mut()
        .ok_or_else(|| "stored extensions_json is not a JSON object".to_string())?;
    let nexus = extensions_obj
        .entry("nexus")
        .or_insert_with(|| Value::Object(Map::new()));
    let nexus_obj = nexus
        .as_object_mut()
        .ok_or_else(|| "stored extensions.nexus is not a JSON object".to_string())?;
    nexus_obj.insert("constraint".to_string(), Value::Object(carrier.clone()));
    serde_json::to_string(&extensions)
        .map_err(|e| format!("failed to serialize extensions bag: {e}"))
}
