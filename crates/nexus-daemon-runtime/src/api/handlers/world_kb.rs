//! Canvas World KB Daemon API handlers (V1.73 P0 Track A).
//!
//! Four World KB routes under `/v1/daemon/worlds/{world_id}/kb/*`, exposing
//! the World-scoped `WorldKbEntry` graph + promotion state machine
//! (entity-scope-model §5.5) to the canvas. Writes use per-row OCC on
//! `kb_key_blocks.revision` (entity edits) and `kb_extract_jobs.version`
//! (promotion), per the architect Phase 2b lock — no new migration.
//!
//! # Endpoints
//!
//! - `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — edit an entity
//!   (`title/body/aliases/block_type`) with per-row OCC.
//! - `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` —
//!   adopt/reject/merge a pending candidate.
//! - `GET  /v1/daemon/worlds/{world_id}/kb/graph` — entity graph projection.
//! - `GET  /v1/daemon/worlds/{world_id}/kb/candidates` — pending candidates.
//!
//! # Conflict model
//!
//! Conflict (409 `WorldKbConflictError`) fires per-entity on version
//! mismatch only. Domain-rule violations return 422
//! `WorldKbValidationError`. Stale versions short-circuit before any write.

#![allow(clippy::missing_errors_doc)]

use super::wire_cast;
use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    world_kb_patch_entity_request::NexusWorldKbEntityPatch,
    world_kb_patch_relationship_request::{
        NexusWorldKbRelationshipInput, NexusWorldKbRelationshipKind,
    },
    PaginationInfo, WorldKbCandidateProjection, WorldKbCandidatesResponse, WorldKbEntityProjection,
    WorldKbExtractJobProjection, WorldKbGraphResponse, WorldKbKeyBlockStateResponse,
    WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbPatchRelationshipRequest,
    WorldKbPatchRelationshipResponse, WorldKbPromoteCandidateRequest,
    WorldKbPromoteCandidateResponse, WorldKbRelationshipProjection, WorldKbSourceAnchorProjection,
};
use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
use nexus_knowledge::world_kb::validation::{
    validate_body, validate_canonical_name, ValidationMode,
};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_extract_job::{
    get_promotion, list_pending_for_world_after, mark_confirmed_in_tx_with_cas, KbExtractPromotion,
};
use nexus_local_db::kb_relationships::{
    delete_relationship_in_tx, generate_relationship_id, get_relationship,
    insert_relationship_in_tx, list_relationships_for_world, update_relationship_in_tx,
    InsertRelationshipParams, KbRelationshipRow, UpdateRelationshipParams,
};
use nexus_local_db::kb_store::{self, cas_update_key_block_fields};
use nexus_local_db::spoke_adapter::NexusBaselineAdapter;
use nexus_local_db::LocalDbError;
// V1.142 P2: first production orchestrator cutover. `promote_adopt` routes
// through `orchestrate_promote(&NexusBaselineAdapter, PromoteRequest)`.
// These spoke types are re-exported through `nexus_spoke_adapter` (the
// single boundary that crosses into spoke standard objects; spec §7).
use nexus_spoke_adapter::{
    orchestrate_promote, KnowledgeEntry as SpokeKnowledgeEntry, PromoteRequest, PromoteResponse,
    SpokeReject, SpokeRejectCode, SpokeResult,
};
use serde::Deserialize;
use tracing::{info, warn};

/// Maximum entities returned by the graph projection (mirrors `kb_store`
/// `LIST_BY_WORLD_LIMIT` safety cap).
const GRAPH_ENTITY_CAP: usize = 500;
/// Maximum stored relationships projected by the graph endpoint before
/// symmetric-reverse derivation (qc1 F-003 / `R-V176QC1-S002`). Bounds the
/// payload for worlds that have accumulated many extraction suggestions across
/// rescans, mirroring `GRAPH_ENTITY_CAP`. The cap is applied to *stored* rows
/// so a stored edge and its symmetric reverse are never split across the
/// boundary. Pre-1.0 local-first datasets stay well under this; a future
/// `limit`/`cursor` pagination pass (see `get_graph` TODO) can replace it once
/// the wire contract gains a `truncated`/`next_cursor` envelope.
const GRAPH_RELATIONSHIP_CAP: usize = 1000;
/// Default page size for the candidates endpoint. 50 matches the UI's default
/// list viewport plus one buffer page, so the first render does not block on a
/// large fetch while still amortizing pagination overhead.
const DEFAULT_CANDIDATE_LIMIT: i64 = 50;
/// Hard upper bound for the candidates endpoint. 250 prevents a malformed or
/// malicious `?limit=` value from materializing an unbounded response; it
/// aligns with the largest list viewport we expect in the local SPA.
const MAX_CANDIDATE_LIMIT: i64 = 250;

/// Prefix for candidate-list keyset cursors (`kb promotion`). Distinguishes
/// the V1.73 qc3 W-01 keyset cursor from any legacy bare-`job_id` cursor so a
/// malformed/old cursor surfaces as 400 instead of silently mis-paginating.
const CANDIDATE_CURSOR_PREFIX: &str = "kbp:";

// ─── Shared helpers ─────────────────────────────────────────────────────────

/// Read the active creator id or return `AuthRequired`.
fn require_creator(state: &WorkspaceState) -> Result<String, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;
    Ok(creator_id)
}

/// Verify the active creator owns the World (`narrative_worlds.owner_creator_id`).
/// Returns 404 when the world is missing, 403 on cross-author access.
async fn require_world_owner(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    creator_id: &str,
) -> Result<(), NexusApiError> {
    // SAFETY: SELECT against the known narrative_worlds table schema.
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(NexusApiError::from)?;
    match owner {
        None => Err(NexusApiError::NotFound(format!("world {world_id}"))),
        Some(Some(owner_id)) if owner_id == creator_id => Ok(()),
        Some(Some(_)) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason:
                "active creator does not own this world; cross-author World KB edits are forbidden"
                    .to_string(),
        }),
        Some(None) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "world has no owner_creator_id; cannot authorize World KB edit".to_string(),
        }),
    }
}

/// Map a `LocalDbError::VersionMismatch` to a 409 `WorldKbConflictError`;
/// everything else to a 500.
///
/// `conflicting_path` lets the merge target CAS miss tag itself as
/// `"merge_target"` (distinct from a candidate's `"version"`) so the client
/// can tell WHICH entity conflicted and refresh the right list instead of
/// blindly retrying the candidate with the target's revision as
/// `expected_version` (greptile P1, iter 5).
fn map_cas_err(e: LocalDbError, entity_id: &str, conflicting_path: &str) -> NexusApiError {
    match e {
        LocalDbError::VersionMismatch { actual, .. } => NexusApiError::world_kb_conflict(
            actual.unwrap_or(0).max(0).cast_unsigned(),
            entity_id,
            conflicting_path,
            "refetch the World KB graph and reapply",
        ),
        other => NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: other.to_string(),
        },
    }
}

/// Re-read the actual current `kb_extract_jobs.version` for `job_id` after a
/// promote-path CAS miss, normalized the same way as the outer OCC
/// precondition (`u64::try_from(version).unwrap_or(0)`).
///
/// The promote handlers run an outer version check, then a separate CAS
/// `UPDATE ... WHERE version = ?`. A concurrent write between the two makes
/// the CAS affect 0 rows. Echoing the stale `req.expected_version` as the
/// 409 `current_version` (greptile P1) sends the client retrying with the
/// same stale version — a second avoidable conflict. Re-reading the row
/// gives the client the NEW version it must retry against.
async fn reread_promotion_version(
    pool: &sqlx::SqlitePool,
    job_id: &str,
) -> Result<u64, NexusApiError> {
    Ok(get_promotion(pool, job_id)
        .await
        .map_err(NexusApiError::from)?
        .map_or(0, |j| u64::try_from(j.version).unwrap_or(0)))
}

/// Build the wire projection of a `WorldKbEntry`.
fn project_entity(kb: &WorldKbEntry) -> WorldKbEntityProjection {
    let body_value = kb
        .body
        .as_ref()
        .map(|b| serde_json::to_value(b).unwrap_or_default());
    let aliases = body_value
        .as_ref()
        .and_then(|v| v.get("attributes"))
        .and_then(|a| a.get("aliases"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect::<Vec<_>>()
        });
    let source_anchor_count = u64::from(kb.source_work_id.is_some());
    WorldKbEntityProjection {
        key_block_id: kb.entry_id.clone(),
        world_id: kb.world_id.clone(),
        block_type: wire_cast(kb.block_type),
        canonical_name: wire_cast(kb.canonical_name.clone()),
        status: kb.status.clone(),
        version: kb.revision.unwrap_or(0),
        body: body_value
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        aliases: aliases.unwrap_or_default(),
        source_anchor_count: Some(source_anchor_count),
        updated_at: kb.updated_at.clone(),
    }
}

/// Build the wire projection of a pending promotion candidate.
fn project_candidate(c: &KbExtractPromotion) -> WorldKbCandidateProjection {
    WorldKbCandidateProjection {
        // `job_id` is the unique row PK of `kb_extract_jobs` and the value the
        // promote path already keys on. `canonical_name_guess` is NOT unique
        // within a world (two source works can guess the same character name),
        // so using it here made React Flow node IDs collide and caused the
        // wrong candidate to be promoted (V1.73 greploop issue 2).
        candidate_id: c.job_id.clone(),
        job_id: c.job_id.clone(),
        world_id: c.world_id.clone(),
        block_type: wire_cast(parse_block_type(
            c.block_type_guess.as_deref().unwrap_or("character"),
        )),
        canonical_name: c.canonical_name_guess.clone().unwrap_or_default(),
        status: Some(c.promotion_status.clone()),
        version: u64::try_from(c.version).unwrap_or(0),
        source_anchor_count: Some(u64::from(c.work_id.is_some())),
        created_at: Some(c.created_at.clone()),
    }
}

/// Build the extract-job projection after a promotion action.
fn project_job(c: &KbExtractPromotion) -> WorldKbExtractJobProjection {
    WorldKbExtractJobProjection {
        job_id: c.job_id.clone(),
        world_id: c.world_id.clone(),
        status: c.promotion_status.clone(),
        version: u64::try_from(c.version).unwrap_or(0),
        candidate_ids: vec![],
        updated_at: c.auto_promoted_at.clone(),
    }
}

/// Parse a `snake_case` wire `block_type` string into the enum; falls back to
/// `Character` for unknown values (mirrors the CLI adopt fallback).
fn parse_block_type(s: &str) -> nexus_contracts::BlockType {
    use nexus_contracts::BlockType::{
        Ability, Act, Beat, Conflict, Deity, Dialogue, EconomyTier, Event, Faction, InfoPoint,
        Item, Level, MagicSystem, Organization, Scene, Species, Technology,
    };
    match s {
        "ability" => Ability,
        "scene" => Scene,
        "organization" => Organization,
        "item" => Item,
        "conflict" => Conflict,
        "info_point" => InfoPoint,
        "event" => Event,
        "species" => Species,
        "faction" => Faction,
        "magic_system" => MagicSystem,
        "technology" => Technology,
        "deity" => Deity,
        "level" => Level,
        "economy_tier" => EconomyTier,
        "dialogue" => Dialogue,
        "beat" => Beat,
        "act" => Act,
        // "character" + unknown values fall back to Character (mirrors CLI adopt).
        _ => nexus_contracts::BlockType::Character,
    }
}

/// Build an empty `validation_summary` with the given errors/warnings.
fn validation_summary(errors: &[String], warnings: &[String]) -> serde_json::Value {
    serde_json::json!({ "errors": errors, "warnings": warnings })
}

// ─── patch-entity ───────────────────────────────────────────────────────────

/// `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — entity-level patch.
pub async fn patch_entity(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPatchEntityRequest>,
) -> Result<Json<WorldKbPatchEntityResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;

    // Authorization FIRST: verify the active creator owns the world BEFORE any
    // entity read. `world_id` comes from the PATH (not the entity), so this is
    // safe to check first. Doing the entity read + cross-world scope check
    // before this point leaked entity existence across world boundaries — an
    // unauthenticated-but-locally-active creator could distinguish `NotFound`
    // ("entity not in this world") from `Forbidden` ("not your world"). This
    // matches the order already used by `promote_candidate` and the read
    // endpoints (V1.73 greploop issue 3).
    require_world_owner(pool, &world_id, &creator_id).await?;

    // ID existence + scope: the entity must live in this world.
    let store = nexus_local_db::kb_store::SqliteKbStore::with_validation_mode(
        pool.clone(),
        ValidationMode::Novel,
    );
    let kb = store
        .get_knowledge_entry(&req.entity_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if kb.world_id != world_id {
        return Err(NexusApiError::NotFound(format!(
            "entity {} in world {world_id}",
            req.entity_id
        )));
    }

    // Editability invariant: deleted entities are terminal and cannot be
    // patched. (Pending candidates live on kb_extract_jobs, not
    // kb_key_blocks — they are promoted via promote-candidate, not edited
    // here.) 'merged' entities remain editable to allow post-merge cleanup.
    if kb.status == "deleted" {
        return Err(NexusApiError::world_kb_validation_failed(
            &["deleted entities are terminal and cannot be patched".to_string()],
            &[],
        ));
    }

    let current_version = kb.revision.unwrap_or(0);
    // OCC precondition.
    if req.expected_version != current_version {
        return Err(NexusApiError::world_kb_conflict(
            current_version,
            req.entity_id,
            "version",
            "refetch the World KB graph and reapply",
        ));
    }

    // Validate the patch carries at least one field.
    if patch_is_empty(&req.patch) {
        return Err(NexusApiError::InvalidInput {
            field: "patch".to_string(),
            reason: "at least one of title/body/aliases/block_type must be provided".to_string(),
        });
    }

    // Compute new field values + validate.
    let new_name = req.patch.title.as_ref().map(|t| t.to_string());
    let new_block_type = req.patch.block_type;
    let (body_json_str, body_for_validation) = compute_body(&kb, &req.patch)?;

    if let Some(ref name) = new_name {
        validate_canonical_name(name)
            .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;
    }
    let validation_block_type =
        new_block_type.map_or(kb.block_type, wire_cast::<nexus_contracts::BlockType, _>);
    if let Some(ref body) = body_for_validation {
        validate_body(validation_block_type, Some(body), ValidationMode::Novel)
            .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;
    }

    // Atomic CAS write.
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let new_version = cas_update_key_block_fields(
        &mut tx,
        &req.entity_id,
        new_name.as_deref(),
        new_block_type.map(|bt| bt.as_str()),
        body_json_str.as_deref(),
        i64::try_from(current_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_cas_err(e, &req.entity_id, "version"))?;
    tx.commit().await.map_err(NexusApiError::from)?;

    info!(entity_id = %req.entity_id, new_version, "world_kb.patch_entity committed");

    // Re-read canonical post-write state for the response projection.
    let updated = store
        .get_knowledge_entry(&req.entity_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok(Json(WorldKbPatchEntityResponse {
        entity: wire_cast(project_entity(&updated)),
        version: new_version,
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

/// `true` when the patch carries no editable field.
fn patch_is_empty(patch: &NexusWorldKbEntityPatch) -> bool {
    patch.title.is_none()
        && patch.body.is_empty()
        && patch.aliases.is_empty()
        && patch.block_type.is_none()
}

/// Resolve the new `body_json` DB string (and a `WorldKbBody` for validation)
/// from the patch + the current entity body. `aliases` are merged into
/// `body.attributes.aliases`.
fn compute_body(
    kb: &WorldKbEntry,
    patch: &NexusWorldKbEntityPatch,
) -> Result<(Option<String>, Option<WorldKbBody>), NexusApiError> {
    if patch.body.is_empty() && patch.aliases.is_empty() {
        return Ok((None, None));
    }
    // Start from the patch body, else the current body, else an empty body.
    let mut value = if patch.body.is_empty() {
        kb.body.as_ref().map_or_else(
            || serde_json::json!({}),
            |b| serde_json::to_value(b).unwrap_or_default(),
        )
    } else {
        serde_json::Value::Object(patch.body.clone())
    };
    if !patch.aliases.is_empty() {
        let obj = value
            .as_object_mut()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "body".to_string(),
                reason: "body must be a JSON object to set aliases".to_string(),
            })?;
        let attrs = obj
            .entry("attributes")
            .or_insert_with(|| serde_json::json!({}));
        attrs["aliases"] = serde_json::Value::Array(
            patch
                .aliases
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
        );
    }
    let body: WorldKbBody =
        serde_json::from_value(value.clone()).map_err(|e| NexusApiError::InvalidInput {
            field: "body".to_string(),
            reason: format!("body is not a valid WorldKbBody: {e}"),
        })?;
    let json_str = serde_json::to_string(&value).unwrap_or_default();
    Ok((Some(json_str), Some(body)))
}

// ─── promote-candidate ──────────────────────────────────────────────────────

/// `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge.
pub async fn promote_candidate(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPromoteCandidateRequest>,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;

    require_world_owner(pool, &world_id, &creator_id).await?;

    // Load the promotion candidate.
    let candidate = get_promotion(pool, &req.job_id)
        .await
        .map_err(NexusApiError::from)?
        .ok_or_else(|| NexusApiError::NotFound(format!("promotion job {}", req.job_id)))?;
    if candidate.world_id != world_id {
        return Err(NexusApiError::NotFound(format!(
            "promotion job {} in world {world_id}",
            req.job_id
        )));
    }

    // Promotion transition validity: candidate must be pending.
    if candidate.promotion_status != "pending" {
        return Err(NexusApiError::world_kb_validation_failed(
            &[format!(
                "candidate is in terminal state '{}' (entity-scope-model §5.5.2); \
                 only pending candidates can be adopted/rejected/merged",
                candidate.promotion_status
            )],
            &[],
        ));
    }

    // OCC precondition on kb_extract_jobs.version.
    let current_version = u64::try_from(candidate.version).unwrap_or(0);
    if req.expected_version != current_version {
        return Err(NexusApiError::world_kb_conflict(
            current_version,
            &req.job_id,
            "version",
            "refetch the candidates list and reapply",
        ));
    }

    match req.action.as_str() {
        "adopt" => promote_adopt(&state, &world_id, &candidate, &req).await,
        "reject" => promote_reject(pool, &candidate, &req).await,
        "merge" => promote_merge(&state, &world_id, &candidate, &req).await,
        other => Err(NexusApiError::InvalidInput {
            field: "action".to_string(),
            reason: format!("action must be adopt|reject|merge, got '{other}'"),
        }),
    }
}

/// Resolved adopt inputs (parsed payload + optional patch refinements).
struct AdoptPlan {
    body: WorldKbBody,
    block_type: nexus_contracts::BlockType,
    canonical_name: String,
}

/// Parse the candidate `proposed_payload` and apply optional `patch`
/// refinements (`title`/`body`/`aliases`/`block_type`) into a validated adopt plan.
fn build_adopt_plan(
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<AdoptPlan, NexusApiError> {
    let mut body: WorldKbBody = serde_json::from_str(
        candidate.proposed_payload.as_deref().unwrap_or("{}"),
    )
    .map_err(|e| NexusApiError::Internal {
        code: "KB_PAYLOAD_INVALID".to_string(),
        message: format!("proposed_payload is not a valid WorldKbBody: {e}"),
    })?;
    let block_type = req.patch.as_ref().and_then(|p| p.block_type).map_or_else(
        || parse_block_type(candidate.block_type_guess.as_deref().unwrap_or("character")),
        wire_cast::<nexus_contracts::BlockType, _>,
    );
    let canonical_name = req
        .patch
        .as_ref()
        .and_then(|p| p.title.as_ref().map(|t| t.to_string()))
        .or_else(|| candidate.canonical_name_guess.clone())
        .ok_or_else(|| {
            NexusApiError::world_kb_validation_failed(
                &["candidate has no canonical_name_guess and no patch.title".to_string()],
                &[],
            )
        })?;
    if let Some(ref p) = req.patch {
        if !p.body.is_empty() {
            body =
                serde_json::from_value(serde_json::Value::Object(p.body.clone())).map_err(|e| {
                    NexusApiError::InvalidInput {
                        field: "patch.body".to_string(),
                        reason: format!("not a valid WorldKbBody: {e}"),
                    }
                })?;
        }
        if !p.aliases.is_empty() {
            merge_aliases_into_body(&mut body, &p.aliases);
        }
    }
    Ok(AdoptPlan {
        body,
        block_type,
        canonical_name,
    })
}

/// Set `body.attributes.aliases` in place.
fn merge_aliases_into_body(body: &mut WorldKbBody, aliases: &[String]) {
    let mut value = serde_json::to_value(&*body).unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        let attrs = obj
            .entry("attributes")
            .or_insert_with(|| serde_json::json!({}));
        attrs["aliases"] = serde_json::Value::Array(
            aliases
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
        );
    }
    if let Ok(merged) = serde_json::from_value::<WorldKbBody>(value) {
        *body = merged;
    }
}

/// Adopt: build a provisional candidate, route through `orchestrate_promote`
/// (which validates → applies promote acceptance → confirmed + bumps revision
/// → persists via the production adapter), then flip the promotion job in a
/// separate transaction.
///
/// V1.142 P2: first production orchestrator cutover. The candidate is built
/// with `status = "provisional"` (NOT `"confirmed"` as the pre-V1.142
/// direct-create path did) — `orchestrate_promote`'s `validate_promote_request`
/// requires provisional, and `apply_promote_acceptance` transitions it to
/// `"confirmed"` and bumps the revision.
///
/// # Pre-1.0 known trade-off: transaction boundary split
///
/// This handler routes through `orchestrate_promote`, which creates the
/// confirmed `KnowledgeEntry` in its own storage call (its own transaction
/// inside the adapter's `put_knowledge_entry`). The extract-job flip below
/// happens in a SEPARATE transaction. If the job flip fails after the entry
/// is committed, an orphan confirmed entry persists.
///
/// The retry-safe idempotency pattern mitigates this for the common case: on
/// retry, the `idx_kb_key_blocks_active_unique(world_id, block_type,
/// canonical_name)` index collides with the prior partial attempt's orphan,
/// the orchestrator returns `KnowledgeEntryAlreadyExists`, and
/// [`map_promote_response`] recovers via the retry-safe branch (catch
/// `KnowledgeEntryAlreadyExists` on retry → check job status). This does NOT
/// guarantee cleanup if the caller gives up after the partial attempt — a
/// confirmed entry then persists with a still-pending job (logged as an orphan
/// warning; surfaced to the operator as 422 on the next attempt). Full
/// transaction unification (single tx across orchestrator + job flip) is
/// roadmap next, tracked in residual R-V1142P2-003.
async fn promote_adopt(
    state: &WorkspaceState,
    world_id: &str,
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;

    let AdoptPlan {
        body,
        block_type,
        canonical_name,
    } = build_adopt_plan(candidate, req)?;

    validate_canonical_name(&canonical_name)
        .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;
    validate_body(block_type, Some(&body), ValidationMode::Novel)
        .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;

    // Build the candidate with `status = "provisional"` — the orchestrator
    // flips it to "confirmed" via `apply_promote_acceptance`. Setting it to
    // "confirmed" here would reject with `CandidateNotProvisional`.
    let mut kb = WorldKbEntry::new(world_id, block_type, &canonical_name);
    kb.body = Some(body);
    kb.status = "provisional".to_string();
    kb.created_at = chrono::Utc::now().to_rfc3339();
    kb.source_work_id = candidate.work_id.clone();
    kb.source_chapter = candidate.source_chapter_id;
    kb.source_provenance_kind = if candidate.llm_confidence.is_some() {
        Some("review_time_extract".to_string())
    } else {
        Some("manual".to_string())
    };

    // V1.142 P2: route through `orchestrate_promote`. `NexusBaselineAdapter`
    // bridges the sync spoke port to async SQLite via `block_in_place`;
    // construction requires the current thread to be inside a tokio
    // multi-threaded runtime (the daemon's production runtime satisfies
    // this; tests use `#[tokio::test(flavor = "multi_thread")]`).
    let adapter = NexusBaselineAdapter::new(pool.clone());
    let spoke_req = build_spoke_promote_request(&kb);
    let result = orchestrate_promote(&adapter, spoke_req);
    let knowledge_entry = map_promote_response(result, pool, &req.job_id, &kb).await?;

    // Job flip — separate transaction from the orchestrator's entry create
    // (transaction-boundary split, see function doc). The CAS guard +
    // outer `promote_candidate` pending-check together prevent double-flip.
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let flipped = mark_confirmed_in_tx_with_cas(
        &mut tx,
        &req.job_id,
        i64::try_from(req.expected_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_cas_err(e, &req.job_id, "version"))?;
    if !flipped {
        // Race: row left pending state between read and flip. The confirmed
        // entry from the orchestrator is now orphaned (entry committed, job
        // still pending). Roll back the job-flip tx and surface as
        // validation_failed; the caller's retry hits the retry-safe branch
        // in `map_promote_response` via the unique-index collision.
        let _ = tx.rollback().await;
        return Err(NexusApiError::world_kb_validation_failed(
            &[
                "candidate was no longer pending (already confirmed/rejected); \
                 rolled back the job flip"
                    .to_string(),
            ],
            &[],
        ));
    }
    tx.commit().await.map_err(NexusApiError::from)?;

    // Build response — re-read the entry to project the post-write state
    // (mirrors the pre-V1.142 direct-create path's get-after-insert).
    let store = kb_store::SqliteKbStore::new(pool.clone());
    let kb_id = knowledge_entry.entry_id.clone();
    let updated_kb =
        store
            .get_knowledge_entry(&kb_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
    let job = get_promotion(pool, &req.job_id)
        .await
        .map_err(NexusApiError::from)?
        .unwrap_or_else(|| candidate.clone());
    let new_version = u64::try_from(job.version).unwrap_or(0);

    Ok(Json(WorldKbPromoteCandidateResponse {
        entity: Some(wire_cast(project_entity(&updated_kb))),
        job: wire_cast(project_job(&job)),
        version: new_version,
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

// ─── promote-adopt orchestration helpers (V1.142 P2) ────────────────────────

/// Build a spoke [`PromoteRequest`] from a nexus [`WorldKbEntry`] candidate.
///
/// The candidate is converted to the spoke [`SpokeKnowledgeEntry`] boundary
/// type via the sole `From<WorldKbEntry>` conversion seam (spec §7.1), then
/// round-tripped through JSON to fit the `PromoteRequest.candidate` wire
/// shape. The spoke codegen emits a distinct struct per wire shape even
/// when the schema is shared; the orchestrator's internal
/// `promote_candidate_as_data` round-trips anyway, so this is the canonical
/// pattern (mirrors `spoke_orchestrator_integration` test fixtures).
///
/// # Panics
///
/// Panics if the round-trip fails — the candidate has already been through
/// nexus validation (`validate_canonical_name`, `validate_body`), so a
/// serialization/deserialization failure here indicates a wire-shape drift
/// between `WorldKbEntry` and spoke `KnowledgeEntry`, not a runtime input
/// error. That class of failure should surface as a panic at the seam,
/// not a misleading 422 to the caller.
fn build_spoke_promote_request(candidate: &WorldKbEntry) -> PromoteRequest {
    let spoke_entry: SpokeKnowledgeEntry = candidate.clone().into();
    let wire = serde_json::to_value(&spoke_entry).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::from_value(serde_json::json!({ "candidate": wire }))
        .expect("KnowledgeEntry-derived candidate fits PromoteRequest.candidate shape")
}

/// Map `orchestrate_promote`'s [`SpokeResult<PromoteResponse>`] to the
/// confirmed [`SpokeKnowledgeEntry`] on success, or to a [`NexusApiError`]
/// on reject.
///
/// # `SpokeRejectCode` → `NexusApiError` mapping
///
/// | `SpokeRejectCode`                       | `NexusApiError`                    |
/// |-----------------------------------------|------------------------------------|
/// | `MissingRequiredField`                  | `world_kb_validation_failed` (422) |
/// | `EmptyCanonicalName`                    | `world_kb_validation_failed` (422) |
/// | `CandidateTerminalStatus`               | `world_kb_validation_failed` (422) |
/// | `CandidateNotProvisional`               | `world_kb_validation_failed` (422) |
/// | `MergeTargetSelf`                       | `world_kb_validation_failed` (422) |
/// | `InvalidKnowledgeEntryStatus*`          | `world_kb_validation_failed` (422) |
/// | `DuplicateActiveKnowledgeEntry`         | `world_kb_validation_failed` (422) |
/// | `KnowledgeEntryTerminalStatus`          | `world_kb_validation_failed` (422) |
/// | `KnowledgeEntryAlreadyExists`           | retry-safe check (see below)       |
/// | `RevisionConflict`                      | `world_kb_conflict` (409)          |
/// | `StoredRevisionStale`                   | `world_kb_conflict` (409)          |
/// | `InvalidInput` / `CapabilityPortMissing` / other | `Internal` (500)         |
///
/// # Retry-safe idempotency (transaction-boundary split)
///
/// V1.142 P2 splits the original single-transaction insert+flip into two
/// steps: (1) `orchestrate_promote` creates the confirmed `WorldKbEntry`
/// (its own transaction inside the adapter); (2) the handler flips the
/// promotion job (separate transaction). If (2) fails after (1) commits,
/// the caller sees an error and retries. On retry, `promote_adopt` builds a
/// fresh candidate (new UUID `entry_id`, same content) and calls the
/// orchestrator again; the new INSERT collides with the prior partial
/// attempt's orphan on `idx_kb_key_blocks_active_unique` and the orchestrator
/// returns `KnowledgeEntryAlreadyExists`. This helper catches that specific
/// reject and consults the promotion job:
///
/// - **job `confirmed`** → prior partial attempt completed despite returning
///   an error; recover the existing entry (looked up via the same unique
///   key) and return it as a retry-safe success.
/// - **job `pending`** → genuine orphan (entry committed, job not flipped);
///   log a warning and surface as 422 so the operator can refresh the
///   candidates list and re-apply. Pre-1.0 known trade-off.
///
/// The outer `promote_candidate` handler's `promotion_status == "pending"`
/// gate plus `mark_confirmed_in_tx_with_cas`'s CAS guard together prevent
/// double job-flips; the unique index prevents silent orphan duplicates.
async fn map_promote_response(
    result: SpokeResult<PromoteResponse>,
    pool: &sqlx::SqlitePool,
    job_id: &str,
    candidate_lookup: &WorldKbEntry,
) -> Result<SpokeKnowledgeEntry, NexusApiError> {
    match result {
        SpokeResult::Ok(PromoteResponse::Variant0 { knowledge_entry, .. }) => {
            // The codegen emits a DISTINCT `KnowledgeEntry` struct per wire
            // shape (one in `data::knowledge_entry`, another inlined into
            // `ops::promote_response`) even though they share the same JSON
            // schema. Round-trip through JSON to coerce the response-only
            // type into the canonical data type that downstream nexus code
            // consumes (mirrors the orchestrator's internal
            // `promote_candidate_as_data` wire_convert pattern).
            let wire = serde_json::to_value(&knowledge_entry)
                .map_err(|e| NexusApiError::Internal {
                    code: "SPOKE_RESPONSE_DECODE".to_string(),
                    message: format!(
                        "orchestrate_promote returned a non-serializable KnowledgeEntry: {e}"
                    ),
                })?;
            serde_json::from_value::<SpokeKnowledgeEntry>(wire).map_err(|e| {
                NexusApiError::Internal {
                    code: "SPOKE_RESPONSE_DECODE".to_string(),
                    message: format!(
                        "orchestrate_promote response did not match KnowledgeEntry shape: {e}"
                    ),
                }
            })
        }
        // Variant1 is the wire error-envelope case. The orchestrator always
        // surfaces errors via `SpokeResult::Reject` (not Variant1), so this
        // arm is defensive — if a future spoke version starts returning
        // Variant1 from `orchestrate_promote`, this surfaces it as 500
        // rather than silently dropping the error envelope.
        SpokeResult::Ok(PromoteResponse::Variant1 { .. }) => Err(NexusApiError::Internal {
            code: "SPOKE_ORCHESTRATOR_ERROR_ENVELOPE".to_string(),
            message: format!(
                "orchestrate_promote returned a PromoteResponse::Variant1 error envelope for {job_id}; \
                 expected a SpokeResult::Reject (spoke wire-shape drift)"
            ),
        }),
        SpokeResult::Reject(reject) => {
            map_promote_reject(reject, pool, job_id, candidate_lookup).await
        }
    }
}

/// Reject-handling tail of [`map_promote_response`]. Separated because the
/// retry-safe branch is async (re-reads job + entry) and the happy-error
/// branch is short.
async fn map_promote_reject(
    reject: SpokeReject,
    pool: &sqlx::SqlitePool,
    job_id: &str,
    candidate_lookup: &WorldKbEntry,
) -> Result<SpokeKnowledgeEntry, NexusApiError> {
    if reject.code == SpokeRejectCode::KnowledgeEntryAlreadyExists {
        // Retry-safe branch: the unique constraint fired — either a genuine
        // duplicate (a confirmed entry with the same world/type/name already
        // exists) or a retry after a partial prior attempt. Disambiguate via
        // the promotion job's status.
        let job = get_promotion(pool, job_id)
            .await
            .map_err(NexusApiError::from)?;
        if let Some(job) = job {
            if job.promotion_status == "confirmed" {
                // Prior partial attempt completed successfully despite
                // returning an error to the caller. Recover the existing
                // entry (authoritative via the unique key) and return it.
                if let Some(existing) = find_active_entry_for(
                    pool,
                    &candidate_lookup.world_id,
                    &candidate_lookup.canonical_name,
                    candidate_lookup.block_type,
                )
                .await?
                {
                    tracing::info!(
                        job_id = %job_id,
                        entry_id = %existing.entry_id,
                        "promote_adopt retry-safe: returning existing confirmed entry from prior partial attempt"
                    );
                    return Ok(existing.into());
                }
                // Defensive: unique fired but the active entry isn't found
                // by the lookup (status drift between INSERT and SELECT).
                tracing::warn!(
                    job_id = %job_id,
                    "promote_adopt retry-safe: job is confirmed but no matching active entry found"
                );
            } else {
                tracing::warn!(
                    job_id = %job_id,
                    "promote_adopt orphan: prior partial attempt left confirmed entry with pending job; manual cleanup needed"
                );
            }
        }
        return Err(NexusApiError::world_kb_validation_failed(
            &[
                "an active WorldKbEntry with the same name/type already exists in this world \
                 (possible prior partial promote attempt — refresh the candidates list)"
                    .to_string(),
            ],
            &[],
        ));
    }
    Err(spoke_reject_to_api_error(reject, pool, job_id).await)
}

/// Look up the active [`WorldKbEntry`] matching the same unique key as the
/// candidate. Used by the retry-safe branch of [`map_promote_response`] to
/// recover the existing confirmed entry on retry.
async fn find_active_entry_for(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    canonical_name: &str,
    block_type: nexus_contracts::BlockType,
) -> Result<Option<WorldKbEntry>, NexusApiError> {
    let store = kb_store::SqliteKbStore::new(pool.clone());
    let entries = store
        .list_by_world(world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    Ok(entries.into_iter().find(|kb| {
        kb.canonical_name == canonical_name
            && kb.block_type == block_type
            && !matches!(kb.status.as_str(), "deleted" | "merged" | "deprecated")
    }))
}

/// Map a (non-retry-safe) [`SpokeReject`] to a [`NexusApiError`]. The
/// mapping table is documented on [`map_promote_response`].
async fn spoke_reject_to_api_error(
    reject: SpokeReject,
    pool: &sqlx::SqlitePool,
    job_id: &str,
) -> NexusApiError {
    match reject.code {
        SpokeRejectCode::MissingRequiredField
        | SpokeRejectCode::EmptyCanonicalName
        | SpokeRejectCode::CandidateTerminalStatus
        | SpokeRejectCode::CandidateNotProvisional
        | SpokeRejectCode::MergeTargetSelf
        | SpokeRejectCode::InvalidKnowledgeEntryStatus
        | SpokeRejectCode::InvalidKnowledgeEntryStatusTransition
        | SpokeRejectCode::DuplicateActiveKnowledgeEntry
        | SpokeRejectCode::KnowledgeEntryTerminalStatus => {
            NexusApiError::world_kb_validation_failed(&[reject.message], &[])
        }
        SpokeRejectCode::RevisionConflict | SpokeRejectCode::StoredRevisionStale => {
            let current = reread_promotion_version(pool, job_id).await.unwrap_or(0);
            NexusApiError::world_kb_conflict(
                current,
                job_id,
                "version",
                "refetch the candidates list and reapply",
            )
        }
        _ => NexusApiError::Internal {
            code: "SPOKE_ORCHESTRATOR_REJECT".to_string(),
            message: format!(
                "orchestrate_promote rejected: {}: {}",
                reject.code, reject.message
            ),
        },
    }
}

/// Reject: CAS flip pending → rejected (with version guard).
async fn promote_reject(
    pool: &sqlx::SqlitePool,
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    // SAFETY: runtime UPDATE with version guard — mirrors the V1.51 CAS pattern.
    let result = sqlx::query(
        "UPDATE kb_extract_jobs \
         SET promotion_status = 'rejected', version = version + 1 \
         WHERE job_id = ? AND promotion_status = 'pending' AND version = ?",
    )
    .bind(&req.job_id)
    .bind(i64::try_from(req.expected_version).unwrap_or(0))
    .execute(pool)
    .await
    .map_err(NexusApiError::from)?;
    if result.rows_affected() != 1 {
        // CAS miss: a concurrent write bumped `version` (or flipped
        // `promotion_status`) between the outer check and this UPDATE. Re-read
        // the actual current version so the client retries against the NEW
        // version instead of resubmitting the stale `expected_version`
        // (greptile P1).
        let current = reread_promotion_version(pool, &req.job_id).await?;
        return Err(NexusApiError::world_kb_conflict(
            current,
            &req.job_id,
            "version",
            "refetch the candidates list and reapply",
        ));
    }
    let job = get_promotion(pool, &req.job_id)
        .await
        .map_err(NexusApiError::from)?
        .unwrap_or_else(|| candidate.clone());
    let new_version = u64::try_from(job.version).unwrap_or(0);
    Ok(Json(WorldKbPromoteCandidateResponse {
        entity: None,
        job: wire_cast(project_job(&job)),
        version: new_version,
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

/// Merge: fold the candidate summary into an existing confirmed target, then
/// dismiss the candidate. `merge_target_id` must reference a confirmed/manual
/// `WorldKbEntry` in the same world.
// simplify: V1.73 β merge folds the candidate summary into the target body and
// rejects the candidate job. Full attribute-level merge with conflict surfacing
// is deferred to V1.74 alongside the relationships surface.
async fn promote_merge(
    state: &WorkspaceState,
    world_id: &str,
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let target_id = req
        .merge_target_id
        .as_deref()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "merge_target_id".to_string(),
            reason: "merge requires merge_target_id".to_string(),
        })?;
    let store = kb_store::SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
    let target =
        store
            .get_knowledge_entry(target_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
    if target.world_id != world_id {
        return Err(NexusApiError::NotFound(format!(
            "merge target {target_id} in world {world_id}"
        )));
    }
    if target.status != "confirmed" && target.status != "manual" {
        return Err(NexusApiError::world_kb_validation_failed(
            &[format!(
                "merge target must be confirmed or manual; got '{}'",
                target.status
            )],
            &[],
        ));
    }

    // Fold the candidate summary into the target body summary.
    let candidate_summary = candidate
        .proposed_payload
        .as_deref()
        .and_then(|p| serde_json::from_str::<WorldKbBody>(p).ok())
        .and_then(|b| b.summary);
    let mut target_body = target.body.clone().unwrap_or_default();
    if let Some(cs) = candidate_summary {
        let merged = target_body.summary.as_ref().map_or_else(
            || format!("— merged: {cs}"),
            |existing| format!("{existing}\n\n— merged: {cs}"),
        );
        target_body.summary = Some(merged);
    }
    let body_value = serde_json::to_value(&target_body).unwrap_or_default();
    let body_json_str = serde_json::to_string(&body_value).unwrap_or_default();
    let target_version = target.revision.unwrap_or(0);

    // Atomic: CAS-update target body + CAS-reject candidate job in one tx.
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    // Target CAS miss is tagged "merge_target" (not the candidate's "version")
    // so the client refreshes the target, not the candidate (greptile P1, iter 5).
    let _new_target_version = cas_update_key_block_fields(
        &mut tx,
        target_id,
        None,
        None,
        Some(&body_json_str),
        i64::try_from(target_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_cas_err(e, target_id, "merge_target"))?;
    let reject = sqlx::query(
        "UPDATE kb_extract_jobs \
         SET promotion_status = 'rejected', version = version + 1 \
         WHERE job_id = ? AND promotion_status = 'pending' AND version = ?",
    )
    .bind(&req.job_id)
    .bind(i64::try_from(req.expected_version).unwrap_or(0))
    .execute(&mut *tx)
    .await
    .map_err(NexusApiError::from)?;
    if reject.rows_affected() != 1 {
        // CAS miss: a concurrent write bumped the candidate `version` (or
        // flipped its `promotion_status`) between the outer check and this
        // in-tx UPDATE. Roll back the target fold and re-read the candidate's
        // actual current version so the client retries against the NEW version
        // instead of resubmitting the stale `expected_version` (greptile P1).
        let _ = tx.rollback().await;
        let current = reread_promotion_version(pool, &req.job_id).await?;
        return Err(NexusApiError::world_kb_conflict(
            current,
            &req.job_id,
            "version",
            "refetch the candidates list and reapply",
        ));
    }
    tx.commit().await.map_err(NexusApiError::from)?;

    let updated_target =
        store
            .get_knowledge_entry(target_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
    let job = get_promotion(pool, &req.job_id)
        .await
        .map_err(NexusApiError::from)?
        .unwrap_or_else(|| candidate.clone());
    let new_version = u64::try_from(job.version).unwrap_or(0);

    Ok(Json(WorldKbPromoteCandidateResponse {
        entity: Some(wire_cast(project_entity(&updated_target))),
        job: wire_cast(project_job(&job)),
        version: new_version,
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

// ─── read endpoints ─────────────────────────────────────────────────────────

/// `GET /v1/daemon/worlds/{world_id}/kb/graph` — entity graph projection.
///
/// V1.76: defaults to excluding `needs_review = 1` (extraction-suggested)
/// relationships from the graph. Pass `?include_suggested=true` to surface
/// them (rendered as dashed edges by the client). Existing data is unaffected
/// — all rows default to `needs_review = 0` (migration
/// `202606300001_kb_relationships_needs_review.sql`).
pub async fn get_graph(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<WorldKbGraphResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;

    let store = kb_store::SqliteKbStore::new(state.pool_or_uninit()?.clone());
    let blocks = store
        .list_by_world(&world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // simplify: V1.73 derives source-anchor provenance edges from the
    // WorldKbEntry's own source_work_id/source_provenance_kind rather than a
    // separate kb_source_anchors join. One edge per entity with provenance.
    let mut entities = Vec::with_capacity(blocks.len().min(GRAPH_ENTITY_CAP));
    let mut source_anchors = Vec::new();
    for kb in blocks.into_iter().take(GRAPH_ENTITY_CAP) {
        if kb.status == "deleted" {
            continue;
        }
        if kb.source_work_id.is_some() {
            let reference = match kb.source_chapter {
                Some(ch) => format!(
                    "work:{},chapter:{ch}",
                    kb.source_work_id.clone().unwrap_or_default()
                ),
                None => format!("work:{}", kb.source_work_id.clone().unwrap_or_default()),
            };
            source_anchors.push(WorldKbSourceAnchorProjection {
                source_anchor_id: format!("sa_{}", kb.entry_id),
                key_block_id: kb.entry_id.clone(),
                source_type: kb
                    .source_provenance_kind
                    .clone()
                    .unwrap_or_else(|| "manual".to_string()),
                reference,
                created_at: Some(kb.created_at.clone()),
            });
        }
        entities.push(project_entity(&kb));
    }

    // V1.76 TODO: relationship graph pagination. The response currently ships
    // the entire graph for the world (entities capped by `GRAPH_ENTITY_CAP`,
    // relationships capped by `GRAPH_RELATIONSHIP_CAP` as of V1.77). When the
    // relationship count exceeds the client viewport budget, introduce
    // `limit`/`cursor` query params and a `truncated` flag plus `next_cursor`
    // in `WorldKbGraphResponse` so callers can paginate without losing the
    // symmetric-reverse derived edges. That requires a wire-contract change
    // (new response fields + schema/codegen), so it is deferred past the V1.77
    // `wire_contracts_changed: FALSE` polish pass; the cap is the interim
    // safety bound (qc1 F-003 / `R-V176QC1-S002`).
    Ok(Json(WorldKbGraphResponse {
        entities: wire_cast(entities),
        source_anchors: wire_cast(source_anchors),
        relationships: wire_cast(
            project_relationships_for_world(
                state.pool_or_uninit()?,
                &world_id,
                query.include_suggested.unwrap_or(false),
            )
            .await?,
        ),
    }))
}

/// `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state` —
/// computable `WorldKbEntry` state read.
///
/// V1.114 P2: dedicated read surface for `body.state` of computable `KeyBlocks`.
/// Returns `state` when `body.computable` is true; `state: null` and
/// `is_computable: false` otherwise. `version` mirrors the per-row OCC
/// revision so callers can use the same OCC pattern as the graph/patch flows.
pub async fn get_key_block_state(
    State(state): State<WorkspaceState>,
    Path((world_id, key_block_id)): Path<(String, String)>,
) -> Result<Json<WorldKbKeyBlockStateResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;

    let store = kb_store::SqliteKbStore::new(state.pool_or_uninit()?.clone());
    let kb = store
        .get_knowledge_entry(&key_block_id)
        .await
        .map_err(|e| match e {
            nexus_knowledge::world_kb::store::KbStoreError::NotFound(_) => {
                NexusApiError::NotFound(format!("key block {key_block_id} in world {world_id}"))
            }
            other => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: other.to_string(),
            },
        })?;

    // Scope check: the WorldKbEntry must live in the path world. Treat a row
    // belonging to a different world as 404 (same as patch_entity).
    if kb.world_id != world_id {
        return Err(NexusApiError::NotFound(format!(
            "key block {key_block_id} in world {world_id}"
        )));
    }

    let is_computable = kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false);
    let state = if is_computable {
        kb.body
            .as_ref()
            .and_then(|b| b.state.clone())
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };

    Ok(Json(WorldKbKeyBlockStateResponse {
        state: state.as_object().cloned(),
        is_computable,
        version: kb.revision.unwrap_or(0),
    }))
}

/// Query params for the graph endpoint (V1.76).
#[derive(Debug, Deserialize)]
pub struct GraphQuery {
    /// When `true`, include `needs_review = 1` (extraction-suggested)
    /// relationships in the graph projection. Defaults to `false` so the
    /// confirmed graph is not flooded by co-occurrence suggestions.
    pub include_suggested: Option<bool>,
}

/// Read all relationships for the world and emit stored + derived symmetric-reverse
/// projections.
///
/// V1.76: when `include_suggested` is `false` (the default), the `needs_review = 1`
/// filter is pushed into the storage query (see [`list_relationships_for_world`])
/// so the confirmed graph uses the `(world_id, needs_review)` index and never
/// materializes extraction-suggestion rows. Symmetric-reverse derivation is
/// unchanged — only stored rows are projected, so the symmetric reverse of a
/// suggestion cannot leak into the confirmed graph.
///
/// V1.77: the `GRAPH_RELATIONSHIP_CAP` is pushed into the SQL `LIMIT` (qc3
/// W-QC3-P1-001) by passing `CAP + 1` to the DAO; the extra row detects
/// truncation. When truncation is detected, a structured `tracing::warn!` is
/// emitted (qc3 W-QC3-P1-002) so operators/authors have an observable signal
/// that older relationships were dropped. A wire `truncated` flag remains a
/// future contract change; the warn is the interim server-side observability.
async fn project_relationships_for_world(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    include_suggested: bool,
) -> Result<Vec<WorldKbRelationshipProjection>, NexusApiError> {
    // Fetch CAP + 1 rows so truncation is detectable: if the DAO returns more
    // than GRAPH_RELATIONSHIP_CAP rows, the world exceeded the safety cap and
    // older relationships are silently dropped from the projection. The cap is
    // pushed into SQL (qc3 W-QC3-P1-001) so the hot path never materializes
    // unbounded rows.
    let fetch_limit = i64::try_from(GRAPH_RELATIONSHIP_CAP + 1).unwrap_or(i64::MAX);
    let rows = list_relationships_for_world(pool, world_id, include_suggested, fetch_limit)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let observed = rows.len();
    if observed > GRAPH_RELATIONSHIP_CAP {
        // qc3 W-QC3-P1-002: surface silent truncation so operators/authors can
        // detect that older relationships were dropped from the projection. A
        // wire `truncated` flag is a future contract change; this server-side
        // warn is the interim observable signal.
        warn!(
            metric = "world_kb_graph_relationships_truncated",
            world_id,
            include_suggested,
            cap = GRAPH_RELATIONSHIP_CAP,
            observed_count = observed,
            "graph relationship cap reached; older relationships are not projected"
        );
    }

    // Cap stored rows before projection so the symmetric-reverse derivation
    // never splits a stored edge from its reverse (qc1 F-003 / `R-V176QC1-S002`).
    // Each stored row yields at most 2 projections, so the wire payload is
    // bounded by `2 * GRAPH_RELATIONSHIP_CAP`. The `.take()` is belt-and-
    // suspenders: the SQL LIMIT already caps the DAO output, but this guards
    // correctness if a future caller raises the DAO limit above the cap.
    let mut projections = Vec::with_capacity(rows.len().min(GRAPH_RELATIONSHIP_CAP) * 2);
    for row in rows.into_iter().take(GRAPH_RELATIONSHIP_CAP) {
        projections.push(project_relationship(&row, "stored"));
        if row.symmetric != 0 {
            let mut reverse = row.clone();
            std::mem::swap(&mut reverse.source_entity_id, &mut reverse.target_entity_id);
            projections.push(project_relationship(&reverse, "symmetric_reverse"));
        }
    }
    Ok(projections)
}

#[derive(Debug, Deserialize)]
pub struct CandidatesQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// Decode an opaque candidates cursor into the `(created_at, job_id)` keyset
/// tuple that the next page must start strictly after. `None` decodes to
/// `(None, None)` so the first page includes the oldest candidate.
///
/// Format: `kbp:<created_at>|<job_id>`. `|` never appears in either field
/// (`created_at` is `datetime('now')` ISO8601; `job_id` is `xj_<uuid hex>`).
fn decode_candidate_cursor(
    cursor: Option<&String>,
) -> Result<(Option<String>, Option<String>), NexusApiError> {
    let Some(raw) = cursor else {
        return Ok((None, None));
    };
    let stripped =
        raw.strip_prefix(CANDIDATE_CURSOR_PREFIX)
            .ok_or_else(|| NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: "invalid candidates cursor; pass the next_cursor value unchanged"
                    .to_string(),
            })?;
    let mut parts = stripped.splitn(2, '|');
    let created_at =
        parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: "invalid candidates cursor: missing created_at".to_string(),
            })?;
    let job_id =
        parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: "invalid candidates cursor: missing job_id".to_string(),
            })?;
    Ok((Some(created_at.to_string()), Some(job_id.to_string())))
}

/// Encode the keyset tuple of the last row visible on the current page into an
/// opaque cursor token for the next page request.
fn encode_candidate_cursor(created_at: &str, job_id: &str) -> String {
    format!("{CANDIDATE_CURSOR_PREFIX}{created_at}|{job_id}")
}

/// `GET /v1/daemon/worlds/{world_id}/kb/candidates` — pending candidates list.
///
/// Cursor-paginated via a `(created_at, job_id)` keyset applied **inside** the
/// storage query (V1.73 qc3 W-01 fix). The previous implementation fetched the
/// first `limit + 1` rows and then skipped forward to the cursor in Rust,
/// which made page 2+ unreachable once a world had more than one page of
/// candidates. The keyset filter now lives in the SQL `WHERE` clause so every
/// row beyond the cursor is reachable.
pub async fn get_candidates(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(query): Query<CandidatesQuery>,
) -> Result<Json<WorldKbCandidatesResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;

    let limit = query
        .limit
        .unwrap_or(DEFAULT_CANDIDATE_LIMIT)
        .clamp(1, MAX_CANDIDATE_LIMIT);
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    let (cursor_created_at, cursor_job_id) = decode_candidate_cursor(query.cursor.as_ref())?;

    // Fetch `limit + 1` rows starting strictly after the cursor tuple so the
    // extra row detects `has_more` without truncating later pages.
    let pending = list_pending_for_world_after(
        state.pool_or_uninit()?,
        &world_id,
        cursor_created_at.as_deref(),
        cursor_job_id.as_deref(),
        limit + 1,
    )
    .await
    .map_err(NexusApiError::from)?;

    // Cursor = keyset of the last row ON the current page (index limit-1), so
    // the next page starts strictly after it. Mirrors `chapter_page_meta`.
    let next_cursor = if pending.len() > limit_us {
        let last = &pending[limit_us - 1];
        Some(encode_candidate_cursor(&last.created_at, &last.job_id))
    } else {
        None
    };
    let has_more = next_cursor.is_some();

    let items: Vec<WorldKbCandidateProjection> = pending
        .iter()
        .take(limit_us)
        .map(project_candidate)
        .collect();

    Ok(Json(WorldKbCandidatesResponse {
        items: wire_cast(items),
        pagination: wire_cast(PaginationInfo {
            limit,
            has_more,
            next_cursor,
        }),
    }))
}

// ─── patch-relationship ─────────────────────────────────────────────────────

/// `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` — add/update/remove a
/// typed relationship between two World KB entities.
pub async fn patch_relationship(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPatchRelationshipRequest>,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    require_world_owner(pool, &world_id, &creator_id).await?;

    let now = chrono::Utc::now().to_rfc3339();

    match req.action.as_str() {
        "add" => patch_relationship_add(pool, &world_id, req.relationship, &now).await,
        "update" => {
            patch_relationship_update(
                pool,
                &world_id,
                req.relationship_id.as_deref(),
                req.expected_version,
                req.relationship,
                &now,
            )
            .await
        }
        "remove" => {
            patch_relationship_remove(
                pool,
                &world_id,
                req.relationship_id.as_deref(),
                req.expected_version,
            )
            .await
        }
        other => Err(NexusApiError::InvalidInput {
            field: "action".to_string(),
            reason: format!("unknown action '{other}'; expected add, update, or remove"),
        }),
    }
}

async fn patch_relationship_add(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    input: Option<NexusWorldKbRelationshipInput>,
    now: &str,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let input = input.ok_or_else(|| NexusApiError::InvalidInput {
        field: "relationship".to_string(),
        reason: "relationship payload is required for add".to_string(),
    })?;
    validate_relationship_input(&input)?;
    require_entities_in_world(
        pool,
        world_id,
        &input.source_entity_id,
        &input.target_entity_id,
    )
    .await?;
    require_valid_source_anchors(pool, world_id, Some(input.source_anchor_ids.as_slice())).await?;

    let relationship_id = generate_relationship_id();
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let row = insert_relationship_in_tx(
        &mut tx,
        &InsertRelationshipParams {
            relationship_id: relationship_id.clone(),
            world_id: world_id.to_string(),
            source_entity_id: input.source_entity_id.clone(),
            target_entity_id: input.target_entity_id.clone(),
            relation_type: input.relation_type.as_str().to_string(),
            custom_label: input.custom_label.as_ref().map(|c| c.to_string()),
            symmetric: input.symmetric,
            confidence: input.confidence,
            source_anchor_ids: input.source_anchor_ids.clone(),
            metadata: Some(serde_json::Value::Object(input.metadata.clone())),
            created_at: now.to_string(),
            updated_at: now.to_string(),
            // V1.76: author adds default to confirmed (needs_review = the
            // input's value or false); source is always manual for author
            // creates (extraction goes through the upsert path, not here).
            needs_review: input.needs_review.unwrap_or(false),
            source: nexus_local_db::kb_relationships::SOURCE_MANUAL.to_string(),
        },
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;
    tx.commit().await.map_err(NexusApiError::from)?;

    Ok(Json(WorldKbPatchRelationshipResponse {
        relationship: Some(wire_cast(project_relationship(&row, "stored"))),
        version: u64::try_from(row.revision).unwrap_or(0),
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

async fn patch_relationship_update(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    relationship_id: Option<&str>,
    expected_version: Option<u64>,
    input: Option<NexusWorldKbRelationshipInput>,
    now: &str,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let relationship_id = relationship_id.ok_or_else(|| NexusApiError::InvalidInput {
        field: "relationship_id".to_string(),
        reason: "relationship_id is required for update".to_string(),
    })?;
    let expected_version = expected_version.ok_or_else(|| NexusApiError::InvalidInput {
        field: "expected_version".to_string(),
        reason: "expected_version is required for update".to_string(),
    })?;
    let input = input.ok_or_else(|| NexusApiError::InvalidInput {
        field: "relationship".to_string(),
        reason: "relationship payload is required for update".to_string(),
    })?;

    validate_relationship_input(&input)?;
    require_entities_in_world(
        pool,
        world_id,
        &input.source_entity_id,
        &input.target_entity_id,
    )
    .await?;
    require_valid_source_anchors(pool, world_id, Some(input.source_anchor_ids.as_slice())).await?;

    // Scope check: the row must belong to this world.
    let existing = get_relationship(pool, relationship_id)
        .await
        .map_err(|e| match e {
            LocalDbError::Sqlx(sqlx::Error::RowNotFound) => {
                NexusApiError::NotFound(format!("relationship {relationship_id}"))
            }
            other => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: other.to_string(),
            },
        })?;
    if existing.world_id != world_id {
        return Err(NexusApiError::Forbidden {
            resource: format!("relationship {relationship_id}"),
            reason: format!(
                "relationship belongs to world {}; cross-world access is forbidden",
                existing.world_id
            ),
        });
    }

    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let row = update_relationship_in_tx(
        &mut tx,
        relationship_id,
        &UpdateRelationshipParams {
            relation_type: input.relation_type.as_str().to_string(),
            custom_label: input.custom_label.as_ref().map(|c| c.to_string()),
            symmetric: input.symmetric,
            confidence: input.confidence,
            source_anchor_ids: input.source_anchor_ids.clone(),
            metadata: Some(serde_json::Value::Object(input.metadata.clone())),
            updated_at: now.to_string(),
            // V1.76: promotion clears the needs_review gate. When the input
            // omits needs_review, preserve the existing flag so a routine
            // relation_type/symmetric edit does not silently confirm a
            // suggestion (the client must explicitly set needs_review=false to
            // promote).
            needs_review: input.needs_review.unwrap_or(existing.needs_review != 0),
        },
        i64::try_from(expected_version).unwrap_or(0),
        &existing,
    )
    .await
    .map_err(|e| map_relationship_cas_err(e, relationship_id))?;
    tx.commit().await.map_err(NexusApiError::from)?;

    Ok(Json(WorldKbPatchRelationshipResponse {
        relationship: Some(wire_cast(project_relationship(&row, "stored"))),
        version: u64::try_from(row.revision).unwrap_or(0),
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

async fn patch_relationship_remove(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    relationship_id: Option<&str>,
    expected_version: Option<u64>,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let relationship_id = relationship_id.ok_or_else(|| NexusApiError::InvalidInput {
        field: "relationship_id".to_string(),
        reason: "relationship_id is required for remove".to_string(),
    })?;
    let expected_version = expected_version.ok_or_else(|| NexusApiError::InvalidInput {
        field: "expected_version".to_string(),
        reason: "expected_version is required for remove".to_string(),
    })?;

    // Scope check: the row must belong to this world.
    let existing = get_relationship(pool, relationship_id)
        .await
        .map_err(|e| match e {
            LocalDbError::Sqlx(sqlx::Error::RowNotFound) => {
                NexusApiError::NotFound(format!("relationship {relationship_id}"))
            }
            other => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: other.to_string(),
            },
        })?;
    if existing.world_id != world_id {
        return Err(NexusApiError::Forbidden {
            resource: format!("relationship {relationship_id}"),
            reason: format!(
                "relationship belongs to world {}; cross-world access is forbidden",
                existing.world_id
            ),
        });
    }

    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    delete_relationship_in_tx(
        &mut tx,
        relationship_id,
        i64::try_from(expected_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_relationship_cas_err(e, relationship_id))?;
    tx.commit().await.map_err(NexusApiError::from)?;

    Ok(Json(WorldKbPatchRelationshipResponse {
        relationship: None,
        version: expected_version,
        validation_summary: wire_cast(validation_summary(&[], &[])),
    }))
}

/// Domain validation for a relationship payload.
fn validate_relationship_input(input: &NexusWorldKbRelationshipInput) -> Result<(), NexusApiError> {
    if input.source_entity_id == input.target_entity_id {
        return Err(NexusApiError::world_kb_validation_failed(
            &["source_entity_id and target_entity_id must be different".to_string()],
            &[],
        ));
    }
    if input.relation_type == NexusWorldKbRelationshipKind::Custom && input.custom_label.is_none() {
        return Err(NexusApiError::world_kb_validation_failed(
            &["custom relation_type requires custom_label".to_string()],
            &[],
        ));
    }
    if let Some(confidence) = input.confidence {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(NexusApiError::world_kb_validation_failed(
                &["confidence must be between 0.0 and 1.0".to_string()],
                &[],
            ));
        }
    }
    Ok(())
}

/// Verify both endpoint entities exist in the world and are not deleted.
async fn require_entities_in_world(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    source_id: &str,
    target_id: &str,
) -> Result<(), NexusApiError> {
    // SAFETY: compile-time checked query against kb_key_blocks.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key_block_id, status FROM kb_key_blocks \
         WHERE world_id = ? AND key_block_id IN (?, ?)",
    )
    .bind(world_id)
    .bind(source_id)
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let mut missing = Vec::new();
    let mut deleted = Vec::new();
    for id in [source_id, target_id] {
        match rows.iter().find(|(k, _)| k == id) {
            None => missing.push(id.to_string()),
            Some((_, status)) if status == "deleted" => deleted.push(id.to_string()),
            Some(_) => {}
        }
    }

    if !missing.is_empty() {
        return Err(NexusApiError::world_kb_validation_failed(
            &[format!(
                "entities not found in world {world_id}: {}",
                missing.join(", ")
            )],
            &[],
        ));
    }
    if !deleted.is_empty() {
        return Err(NexusApiError::world_kb_validation_failed(
            &[format!(
                "cannot relate deleted entities: {}",
                deleted.join(", ")
            )],
            &[],
        ));
    }
    Ok(())
}

/// Row shape for source-anchor validation lookups.
#[derive(sqlx::FromRow)]
struct AnchorValidationRow {
    key_block_id: String,
    source_work_id: Option<String>,
}

/// Verify every source-anchor projection id references a `WorldKbEntry` in the world
/// that actually has provenance (`source_work_id IS NOT NULL`). Anchor ids use
/// the V1.73 graph projection format `sa_<key_block_id>`.
async fn require_valid_source_anchors(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    anchor_ids: Option<&[String]>,
) -> Result<(), NexusApiError> {
    let ids = anchor_ids.unwrap_or_default();
    if ids.is_empty() {
        return Ok(());
    }

    let mut key_block_ids = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(kb_id) = id.strip_prefix("sa_") else {
            return Err(NexusApiError::world_kb_validation_failed(
                &[format!(
                    "source_anchor_id '{id}' is not a valid anchor projection id"
                )],
                &[],
            ));
        };
        key_block_ids.push(kb_id.to_string());
    }

    // SAFETY: runtime query with a dynamic JSON array binding. The SQL is
    // otherwise static; compile-time macros cannot bind a variable-length list.
    let rows: Vec<AnchorValidationRow> = sqlx::query_as(
        "SELECT key_block_id, source_work_id FROM kb_key_blocks \
         WHERE world_id = ? AND key_block_id IN (SELECT value FROM json_each(?))",
    )
    .bind(world_id)
    .bind(serde_json::to_string(&key_block_ids).unwrap_or_default())
    .fetch_all(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let mut errors = Vec::new();
    for id in ids {
        let kb_id = id.strip_prefix("sa_").unwrap_or(id);
        match rows.iter().find(|r| r.key_block_id == kb_id) {
            None => errors.push(format!(
                "source anchor '{id}' does not reference an entity in this world"
            )),
            Some(row) if row.source_work_id.is_none() => errors.push(format!(
                "source anchor '{id}' references an entity without provenance"
            )),
            Some(_) => {}
        }
    }

    if !errors.is_empty() {
        return Err(NexusApiError::world_kb_validation_failed(&errors, &[]));
    }
    Ok(())
}

/// Map a relationship CAS miss to a 409 `WorldKbConflict`; other DB errors to 500.
fn map_relationship_cas_err(e: LocalDbError, relationship_id: &str) -> NexusApiError {
    match e {
        LocalDbError::VersionMismatch { actual, .. } => NexusApiError::world_kb_conflict(
            actual.unwrap_or(0).max(0).cast_unsigned(),
            relationship_id,
            "version",
            "refetch the World KB graph and reapply",
        ),
        LocalDbError::Sqlx(sqlx::Error::RowNotFound) => {
            NexusApiError::NotFound(format!("relationship {relationship_id}"))
        }
        other => NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: other.to_string(),
        },
    }
}

/// Build a wire projection from a stored relationship row.
fn project_relationship(row: &KbRelationshipRow, direction: &str) -> WorldKbRelationshipProjection {
    let source_anchor_ids = row
        .source_anchor_ids
        .as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let metadata = row
        .metadata
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    let relation_type: nexus_contracts::WorldKbRelationshipKind =
        row.relation_type.parse().unwrap_or_else(|_| {
            warn!(
                metric = "world_kb_relation_type_coercion",
                relationship_id = row.relationship_id,
                world_id = row.world_id,
                relation_type = row.relation_type,
                %direction,
                "unknown relation_type stored in kb_relationships; projecting as Custom"
            );
            nexus_contracts::WorldKbRelationshipKind::Custom
        });

    WorldKbRelationshipProjection {
        relationship_id: row.relationship_id.clone(),
        world_id: row.world_id.clone(),
        source_entity_id: row.source_entity_id.clone(),
        target_entity_id: row.target_entity_id.clone(),
        relation_type: wire_cast(relation_type),
        custom_label: row.custom_label.clone(),
        symmetric: row.symmetric != 0,
        confidence: row.confidence,
        source_anchor_ids,
        metadata: metadata
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        needs_review: row.needs_review != 0,
        source: wire_cast(row.source.clone()),
        version: u64::try_from(row.revision).unwrap_or(0),
        updated_at: row.updated_at.clone(),
        projection_direction: wire_cast(direction.to_string()),
    }
}
