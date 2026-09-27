//! World structured rules and their advisory findings read projection.
//! Carrier grammar stays in nexus-spoke-adapter; persistence stays in the
//! existing `spoke_rules/world_findings` repositories. Validation ordering,
//! open read vocabulary, whole-carrier replacement and 500-row caps are retained.
//!
//! V1.198 (P0, §13) makes `archived` the fourth write status and a terminal,
//! read-only row: the caller-selected omission of archived rows happens once
//! in `CoreService::list_world_rules` and is pushed into SQL **before** the
//! 501-row probe, and the PATCH terminal guard shares one `BEGIN IMMEDIATE`
//! transaction with the current-row read and the write.

use crate::world_kb::guards;
use crate::{CoreAccess, CoreError, CoreResult, CoreService, Principal};
use nexus_contracts::daemon_api::WorldFindingsListResponse;

/// The PATCH status that turns an existing rule into a terminal, read-only
/// row (§13). Refused at create; never an authoring state.
const ARCHIVED_STATUS: &str = "archived";

/// Which members a PATCH request actually supplied.
///
/// The generated `WorldRuleUpdateRequest` is a lossy carrier for presence:
/// the constraint `Map` treats absent and `{}` identically
/// (`skip_serializing_if = Map::is_empty`) and an explicit JSON `null` decodes
/// to the same `None` as an absent member. The archived terminal guard has to
/// see the *supplied* member set, so it is carried beside the generated DTO:
/// native decoding collects the raw JSON object keys before typed decoding and
/// a direct caller (CLI) names the fields it authored.
///
/// Presence metadata only — never a second wire DTO and never a second value
/// validator. The core still requires the typed member itself (`status` must be
/// literally [`ARCHIVED_STATUS`], every other member must be nonempty), so
/// inconsistent metadata can never authorize an archived edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulePatchPresence {
    canonical_name: bool,
    constraint: bool,
    kind: bool,
    severity_hint: bool,
    statement: bool,
    status: bool,
    target_entry_types: bool,
}

impl RulePatchPresence {
    /// No member supplied (the raw-`{}` request); also the builder starting
    /// point. Deliberately not `Default`: a caller that never thought about
    /// presence must not silently claim "nothing was supplied".
    const NONE: Self = Self {
        canonical_name: false,
        constraint: false,
        kind: false,
        severity_hint: false,
        statement: false,
        status: false,
        target_entry_types: false,
    };

    /// Presence from the supplied member keys — native callers pass the raw
    /// JSON object keys, direct callers the fields they authored.
    ///
    /// Unknown keys are ignored: the request DTO is `deny_unknown_fields`, so
    /// a key outside the seven mutable members cannot reach the core.
    #[must_use]
    pub fn from_supplied_keys(keys: &[&str]) -> Self {
        let mut presence = Self::NONE;
        for key in keys {
            match *key {
                "canonical_name" => presence.canonical_name = true,
                "constraint" => presence.constraint = true,
                "kind" => presence.kind = true,
                "severity_hint" => presence.severity_hint = true,
                "statement" => presence.statement = true,
                "status" => presence.status = true,
                "target_entry_types" => presence.target_entry_types = true,
                _ => {}
            }
        }
        presence
    }
}

impl CoreService {
    /// List the first 500 rules in canonical-name/id order, omitting archived
    /// rows unless `include_archived` selects them (§13: the default read hides
    /// archived rules; draft/deprecated/NULL/unknown statuses stay visible).
    /// # Errors
    /// Returns principal, World ownership or storage errors.
    pub async fn list_world_rules(
        &self,
        principal: &Principal,
        world_id: String,
        include_archived: bool,
    ) -> CoreResult<WorldRulesListResponse> {
        self.verify_principal(principal)?;
        guards::require_world_owner(&self.inner.pool, &world_id, principal.creator_id()).await?;
        list_world_rules(&self.inner.pool, world_id, include_archived).await
    }

    /// Create a structured rule after member-aware carrier validation.
    /// # Errors
    /// Returns principal, write-access, World ownership, validation or storage errors.
    pub async fn create_world_rule(
        &self,
        principal: &Principal,
        world_id: String,
        request: WorldRuleCreateRequest,
    ) -> CoreResult<WorldRuleResponse> {
        self.verify_principal(principal)?;
        require_write_access(self)?;
        guards::require_world_owner(&self.inner.pool, &world_id, principal.creator_id()).await?;
        create_world_rule(&self.inner.pool, world_id, request).await
    }

    /// Update only supplied fields; replace the entire constraint carrier.
    ///
    /// `presence` carries the supplied-member set the generated request DTO
    /// erases (see [`RulePatchPresence`]); the archived terminal guard reads it
    /// beside the typed members.
    ///
    /// # Errors
    /// Returns principal, write-access, World/rule ownership, validation or storage errors.
    pub async fn update_world_rule(
        &self,
        principal: &Principal,
        world_id: String,
        rule_id: String,
        request: WorldRuleUpdateRequest,
        presence: RulePatchPresence,
    ) -> CoreResult<WorldRuleResponse> {
        self.verify_principal(principal)?;
        require_write_access(self)?;
        guards::require_world_owner(&self.inner.pool, &world_id, principal.creator_id()).await?;
        update_world_rule(&self.inner.pool, world_id, rule_id, request, presence).await
    }

    /// Read the newest 500 advisory World findings, with an honest cap flag.
    /// # Errors
    /// Returns principal, World ownership or storage errors.
    pub async fn list_world_findings(
        &self,
        principal: &Principal,
        world_id: String,
    ) -> CoreResult<WorldFindingsListResponse> {
        self.verify_principal(principal)?;
        guards::require_world_owner(&self.inner.pool, &world_id, principal.creator_id()).await?;
        findings::list_world_findings(&self.inner.pool, world_id).await
    }
}

fn require_write_access(service: &CoreService) -> CoreResult<()> {
    if service.inner.access == CoreAccess::ReadOnly {
        return Err(CoreError::Forbidden {
            resource: "world_rule_write: read-only core access".to_string(),
        });
    }
    Ok(())
}

use nexus_contracts::daemon_api::worlds::world_rule_create_request::WorldRuleCreateRequest;
use nexus_contracts::daemon_api::worlds::world_rule_response::WorldRuleResponse;
use nexus_contracts::daemon_api::worlds::world_rule_update_request::WorldRuleUpdateRequest;
use nexus_contracts::daemon_api::worlds::world_rules_list_response::{
    WorldRulesListResponse, WorldRulesListResponseRulesItem,
};
use nexus_local_db::spoke_rules::{
    get_rule_in_tx, insert_rule, list_rules_by_world_limited, update_rule_in_tx, RuleUpdate,
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

/// `GET /v1/daemon/worlds/:world_id/rules?include_archived=` — list a world's
/// structured rules, `canonical_name ASC, rule_id ASC`, capped at
/// [`WORLD_RULES_CAP`].
///
/// §13: the default read omits only `archived` and the omission is pushed into
/// SQL **before** the `LIMIT ?` probe, so an archived row can neither occupy a
/// default slot nor flip `truncated` (which counts selected rows only). An
/// explicit `include_archived` returns every stored status; NULL/unknown stored
/// statuses stay visible either way.
#[allow(clippy::missing_errors_doc)]
async fn list_world_rules(
    pool: &sqlx::SqlitePool,
    world_id: String,
    include_archived: bool,
) -> CoreResult<WorldRulesListResponse> {
    // Fetch one past the cap (501): the store bounds the read SQL-side via
    // `LIMIT ?` (Bugbot 4bad2fca) — the +1 probe returns the single row
    // just beyond the cap so `truncated` below stays honest without ever
    // loading the full set. The caller-selected status omission travels with
    // the query, so the probe and the cap both count selected rows.
    let excluded_status = (!include_archived).then_some(ARCHIVED_STATUS);
    let rows = list_rules_by_world_limited(pool, &world_id, excluded_status, WORLD_RULES_PROBE)
        .await
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?;

    // Honest truncation flag: more *selected* rows than the cap → `truncated:
    // true`, response carries the first 500 (store order is
    // canonical_name ASC, rule_id ASC).
    let truncated = rows.len() > WORLD_RULES_CAP;
    let rules = rows
        .into_iter()
        .take(WORLD_RULES_CAP)
        .map(row_to_item)
        .collect();

    Ok(WorldRulesListResponse { rules, truncated })
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
async fn create_world_rule(
    pool: &sqlx::SqlitePool,
    world_id: String,
    req: WorldRuleCreateRequest,
) -> CoreResult<WorldRuleResponse> {
    // AR-2 seam: the carrier grammar lives in the spoke adapter (sole
    // consumer) — the daemon never parses carriers itself. The
    // member-aware error projects onto the closed `constraint.*` envelope
    // vocabulary (no message string-sniffing).
    let constraint =
        parse_carrier_json_member(&Value::Object(req.constraint.clone())).map_err(|e| {
            CoreError::InvalidInput {
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
        return Err(CoreError::InvalidInput {
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
        return Err(CoreError::InvalidInput {
            field: "target_entry_types".to_string(),
            reason: "target_entry_types members must be non-empty strings".to_string(),
        });
    }

    // AR-2 meta-field value checks (handler-side; the schemas are
    // type-only per AR-1 so these surface through the envelope).
    let canonical_name = req.canonical_name.trim().to_string();
    if canonical_name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "canonical_name".to_string(),
            reason: "canonical_name must be a non-empty string after trimming".to_string(),
        });
    }
    let statement = req.statement.trim().to_string();
    if statement.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "statement".to_string(),
            reason: "statement must be a non-empty string after trimming".to_string(),
        });
    }
    let status = req.status.as_deref().unwrap_or("active");
    // §13: create is an authoring path — `archived` is a transition applied to
    // an existing rule, never an authoring state, so it is refused here.
    if !matches!(status, "draft" | "active" | "deprecated") {
        return Err(CoreError::InvalidInput {
            field: "status".to_string(),
            reason: format!("status must be one of draft | active | deprecated, got {status:?}"),
        });
    }
    if let Some(severity_hint) = req.severity_hint.as_deref() {
        if severity_hint.trim().is_empty() {
            return Err(CoreError::InvalidInput {
                field: "severity_hint".to_string(),
                reason: "severity_hint must be a non-empty string when present".to_string(),
            });
        }
    }
    if let Some(kind) = req.kind.as_deref() {
        if kind.trim().is_empty() {
            return Err(CoreError::InvalidInput {
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
            CoreError::Internal {
                category: e.to_string(),
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
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?;

    Ok(item_to_response(row_to_item(row)))
}

/// `PATCH /v1/daemon/worlds/:world_id/rules/:rule_id` — per-field edit
/// (V1.169 P1, AR-2/AR-3/AR-5/AR-6; V1.198 §13 terminal archive).
///
/// Validation order is contract (AR-5): guards (401/404/403) → current-row
/// fetch + world filter (404 — a `rule_id` that belongs to a different
/// world is indistinguishable from an unknown id, AR-6) → empty-PATCH
/// reject (`patch`) → terminal guard for an archived row (§13) → archive-only
/// repeat short-circuit (no write, §13) → carrier validation if provided
/// (`constraint.*`) → effective-pair
/// `observer_cardinality` × `target_entry_types` conflict → meta-field value
/// checks → `RuleUpdate` assembly (whole-carrier replacement preserves the
/// rest of the extensions bag, AR-3) → `update_rule_in_tx` (`Ok(false)` →
/// 404) → re-read → 200 + item.
///
/// §13 atomicity: the current-row read, the guard decision and the write run
/// in ONE `BEGIN IMMEDIATE` transaction, so two requests cannot pass a stale
/// non-archived preimage and a concurrent edit cannot alter a tombstone.
///
/// `status=deprecated` is the Deactivate recovery and `status=archived` is the
/// terminal archive transition (from any non-archived status; repeating it on
/// an archived row succeeds — product lock, no DELETE and no restore route).
/// The repeat is a strict no-write success: the stored row is returned as it
/// stands, `updated_at` included, so replaying the archive is invisible.
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::too_many_lines)]
// ^ the locked validation order (AR-5) is one cohesive chain; splitting
// would obscure the contract order it documents (works.rs patch_work
// precedent).
async fn update_world_rule(
    pool: &sqlx::SqlitePool,
    world_id: String,
    rule_id: String,
    req: WorldRuleUpdateRequest,
    presence: RulePatchPresence,
) -> CoreResult<WorldRuleResponse> {
    // §13: one `BEGIN IMMEDIATE` transaction owns the read → guard → write →
    // re-read sequence. The write lock is taken up front, so a competing
    // archive commits before this read or waits behind this write — neither
    // request can act on a stale preimage.
    let mut tx = nexus_local_db::begin_immediate(pool)
        .await
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?;

    // AR-5 order: addressing precedes payload. The pre-fetch is
    // world-scoped (filter below): a rule_id owned by a different world —
    // even another world of the same creator — is 404 naming only the id,
    // with no existence leak (AR-6).
    let Some(current) = get_rule_in_tx(&mut tx, &rule_id)
        .await
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?
        .filter(|row| row.world_id == world_id)
    else {
        return Err(CoreError::NotFound {
            resource: format!("rule {rule_id}"),
        });
    };

    // §13: archived is terminal, so the tombstone accepts exactly the
    // status-only request that produced it. The guard runs straight after the
    // scoped lookup — before the typed empty-PATCH test — so a supplied member
    // the generated DTO collapses (an empty `{}` / `[]`, an explicit `null`, or
    // a `status` exit) is still refused by name instead of being discarded into
    // the empty-PATCH or archive-only shapes.
    guard_archived_patch(current.status.as_deref() == Some(ARCHIVED_STATUS), &req, presence)?;

    // §13 strict idempotence: the archive-only repeat on an already archived
    // row is a **no-write** success. The shared update statement refreshes
    // `updated_at` on every matched row, so letting the repeat reach the write
    // path would mutate the stored row — and the caller-visible response —
    // without changing its content. Only the exact repeat short-circuits here:
    // the guard above already refused every other archived-row shape by name,
    // and a raw `{}` (no supplied member at all) still falls through to the
    // empty-PATCH rejection below. No write, so no re-read either — the current
    // row is already the state this commit publishes.
    if current.status.as_deref() == Some(ARCHIVED_STATUS)
        && req.status.as_deref() == Some(ARCHIVED_STATUS)
    {
        return Ok(item_to_response(row_to_item(current)));
    }

    // AR-3: empty PATCH (no mutable field present) → 400 field=`patch` —
    // fail-early beats a no-op write that would still refresh updated_at.
    // `constraint` counts as present only when non-empty (typify's
    // generated `Map` treats absent and `{}` identically via
    // `skip_serializing_if = Map::is_empty`; an empty carrier is invalid
    // anyway, so it cannot be a legitimate PATCH payload). A raw `{}` carries
    // no supplied member either, so it reaches this branch on an archived row
    // too and keeps naming `patch`.
    if req.canonical_name.is_none()
        && req.statement.is_none()
        && req.severity_hint.is_none()
        && req.status.is_none()
        && req.kind.is_none()
        && req.target_entry_types.is_none()
        && req.constraint.is_empty()
    {
        return Err(CoreError::InvalidInput {
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
                CoreError::InvalidInput {
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
        return Err(CoreError::InvalidInput {
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
            return Err(CoreError::InvalidInput {
                field: "target_entry_types".to_string(),
                reason: "target_entry_types members must be non-empty strings".to_string(),
            });
        }
    }
    let canonical_name = match req.canonical_name.as_ref() {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(CoreError::InvalidInput {
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
                return Err(CoreError::InvalidInput {
                    field: "statement".to_string(),
                    reason: "statement must be a non-empty string after trimming".to_string(),
                });
            }
            Some(trimmed.to_string())
        }
        None => None,
    };
    if let Some(status) = req.status.as_deref() {
        // §13 write vocabulary: `archived` joins the three authoring states
        // (create still refuses it). An archived *row* never reaches this
        // branch with a non-archived value — the terminal guard above already
        // refused the status exit.
        if !matches!(status, "draft" | "active" | "deprecated" | ARCHIVED_STATUS) {
            return Err(CoreError::InvalidInput {
                field: "status".to_string(),
                reason: format!(
                    "status must be one of draft | active | deprecated | archived, got {status:?}"
                ),
            });
        }
    }
    if let Some(severity_hint) = req.severity_hint.as_deref() {
        if severity_hint.trim().is_empty() {
            return Err(CoreError::InvalidInput {
                field: "severity_hint".to_string(),
                reason: "severity_hint must be a non-empty string when present".to_string(),
            });
        }
    }
    if let Some(kind) = req.kind.as_deref() {
        if kind.trim().is_empty() {
            return Err(CoreError::InvalidInput {
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
                serde_json::to_string(v).map_err(|e| CoreError::Internal {
                    category: e.to_string(),
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
                .map_err(|e| CoreError::Internal { category: e })?,
        );
    }

    // AR-4: Ok(false) = unknown id OR foreign world (storage does not
    // distinguish) → 404 naming only the id (AR-6).
    if !update_rule_in_tx(&mut tx, &world_id, &rule_id, &update)
        .await
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?
    {
        return Err(CoreError::NotFound {
            resource: format!("rule {rule_id}"),
        });
    }

    // Re-read for the response item inside the same transaction: the row this
    // commit is about to publish, never a concurrent writer's state.
    let Some(row) = get_rule_in_tx(&mut tx, &rule_id)
        .await
        .map_err(|e| CoreError::Internal {
            category: e.to_string(),
        })?
    else {
        return Err(CoreError::Internal {
            category: format!("rule {rule_id} vanished after a matched update"),
        });
    };

    tx.commit().await.map_err(|e| CoreError::Internal {
        category: e.to_string(),
    })?;

    Ok(item_to_response(row_to_item(row)))
}

/// §13 terminal guard: an archived row accepts exactly the status-only
/// request that produced it.
///
/// Rejections use the existing field-level `invalid_input` envelope and follow
/// the shipped request-field order (the generated `WorldRuleUpdateRequest`
/// declaration order: `canonical_name`, `constraint`, `kind`, `severity_hint`,
/// `statement`, `status`, `target_entry_types`), naming the first offending
/// member. `status` is offending when the caller supplied it with anything but
/// the literal `archived` (a status exit — an explicit `null` included); every
/// other member is offending when the caller supplied its key **or** its typed
/// value is nonempty, so an empty object/array or an explicit null can never be
/// discarded to turn a mixed PATCH into an archive-only repeat, and inconsistent
/// presence metadata can never authorize an archived edit.
///
/// # Errors
///
/// Returns `CoreError::InvalidInput` naming the first offending member.
fn guard_archived_patch(
    archived: bool,
    req: &WorldRuleUpdateRequest,
    presence: RulePatchPresence,
) -> CoreResult<()> {
    if !archived {
        return Ok(());
    }
    let offenders: [(&str, bool); 7] = [
        (
            "canonical_name",
            presence.canonical_name || req.canonical_name.is_some(),
        ),
        ("constraint", presence.constraint || !req.constraint.is_empty()),
        ("kind", presence.kind || req.kind.is_some()),
        (
            "severity_hint",
            presence.severity_hint || req.severity_hint.is_some(),
        ),
        ("statement", presence.statement || req.statement.is_some()),
        // A status exit is a *supplied* status other than `archived` (an
        // explicit null counts: `presence` records the key while the typed
        // value is the absent member). An absent status is not an offender —
        // the supplied content member is the one named.
        (
            "status",
            (presence.status || req.status.is_some())
                && !matches!(req.status.as_deref(), Some(ARCHIVED_STATUS)),
        ),
        (
            "target_entry_types",
            presence.target_entry_types || req.target_entry_types.is_some(),
        ),
    ];
    let Some((field, _)) = offenders.into_iter().find(|(_, offending)| *offending) else {
        return Ok(());
    };
    let reason = if field == "status" {
        "an archived rule is terminal: only status=archived is accepted, so no status exit \
         (and no restore route) exists"
            .to_string()
    } else {
        format!(
            "an archived rule is read-only: only status=archived may be supplied, got {field}"
        )
    };
    Err(CoreError::InvalidInput {
        field: field.to_string(),
        reason,
    })
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

mod findings {
    use crate::{CoreError, CoreResult};
    use nexus_contracts::daemon_api::worlds::world_findings_list_response::{
        WorldFindingsListResponse, WorldFindingsListResponseFindingsItem,
    };
    use nexus_local_db::world_findings::{list_world_findings_by_world, WorldFindingRow};
    use serde_json::{Map, Value};
    use std::num::NonZeroU64;

    /// Safety cap on the read surface: the newest 500 findings per world
    /// (AR-3). Pagination lands with the Control Room panel — roadmap.
    const WORLD_FINDINGS_CAP: usize = 500;

    /// SQL-side probe bound for the store query: one past
    /// [`WORLD_FINDINGS_CAP`], so the `LIMIT ?` returns the overflow row and
    /// `truncated` stays honest without loading the full set (Bugbot
    /// 4bad2fca). Derived from the cap so the two cannot drift.
    #[allow(clippy::cast_possible_wrap)] // const-evaluated literal (500): always fits i64
    const WORLD_FINDINGS_PROBE: i64 = WORLD_FINDINGS_CAP as i64 + 1;

    /// `GET /v1/daemon/worlds/:world_id/findings` — list world-attached
    /// check findings, newest-first, capped at [`WORLD_FINDINGS_CAP`].
    #[allow(clippy::missing_errors_doc)]
    pub(super) async fn list_world_findings(
        pool: &sqlx::SqlitePool,
        world_id: String,
    ) -> CoreResult<WorldFindingsListResponse> {
        // Fetch one past the cap (501): the store bounds the read SQL-side via
        // `LIMIT ?` (Bugbot 4bad2fca) — the +1 probe returns the single row
        // just beyond the cap so `truncated` below stays honest without ever
        // loading the full set.
        let rows = list_world_findings_by_world(pool, &world_id, WORLD_FINDINGS_PROBE)
            .await
            .map_err(|e| CoreError::Internal {
                category: e.to_string(),
            })?;

        // Honest truncation flag: more stored rows than the cap → `truncated:
        // true`, response carries the newest 500 (store order is newest-first).
        let truncated = rows.len() > WORLD_FINDINGS_CAP;
        let findings = rows
            .into_iter()
            .take(WORLD_FINDINGS_CAP)
            .map(row_to_item)
            .collect();

        Ok(WorldFindingsListResponse {
            findings,
            truncated,
        })
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
            text_position: parse_json_object(Some(r.text_position_json.as_str()))
                .unwrap_or_default(),
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
}
