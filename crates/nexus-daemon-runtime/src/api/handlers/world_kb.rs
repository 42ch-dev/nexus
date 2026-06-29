//! Canvas World KB Local API handlers (V1.73 P0 Track A).
//!
//! Four World KB routes under `/v1/local/worlds/{world_id}/kb/*`, exposing
//! the World-scoped `KeyBlock` graph + promotion state machine
//! (entity-scope-model §5.5) to the canvas. Writes use per-row OCC on
//! `kb_key_blocks.revision` (entity edits) and `kb_extract_jobs.version`
//! (promotion), per the architect Phase 2b lock — no new migration.
//!
//! # Endpoints
//!
//! - `POST /v1/local/worlds/{world_id}/kb/patch-entity` — edit an entity
//!   (`title/body/aliases/block_type`) with per-row OCC.
//! - `POST /v1/local/worlds/{world_id}/kb/promote-candidate` —
//!   adopt/reject/merge a pending candidate.
//! - `GET  /v1/local/worlds/{world_id}/kb/graph` — entity graph projection.
//! - `GET  /v1/local/worlds/{world_id}/kb/candidates` — pending candidates.
//!
//! # Conflict model
//!
//! Conflict (409 `WorldKbConflictError`) fires per-entity on version
//! mismatch only. Domain-rule violations return 422
//! `WorldKbValidationError`. Stale versions short-circuit before any write.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    PaginationInfo, WorldKbCandidateProjection, WorldKbCandidatesResponse, WorldKbEntityPatch,
    WorldKbEntityProjection, WorldKbExtractJobProjection, WorldKbGraphResponse,
    WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbPatchRelationshipRequest,
    WorldKbPatchRelationshipResponse, WorldKbPromoteCandidateRequest,
    WorldKbPromoteCandidateResponse, WorldKbRelationshipInput, WorldKbRelationshipKind,
    WorldKbRelationshipProjection, WorldKbSourceAnchorProjection,
};
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::{
    get_promotion, list_pending_for_world_after, mark_confirmed_in_tx_with_cas, KbExtractPromotion,
};
use nexus_local_db::kb_relationships::{
    delete_relationship_in_tx, generate_relationship_id, get_relationship,
    insert_relationship_in_tx, list_relationships_for_world, update_relationship_in_tx,
    InsertRelationshipParams, KbRelationshipRow, UpdateRelationshipParams,
};
use nexus_local_db::kb_store::{self, cas_update_key_block_fields};
use nexus_local_db::LocalDbError;
use serde::Deserialize;
use tracing::info;

/// Maximum entities returned by the graph projection (mirrors `kb_store`
/// `LIST_BY_WORLD_LIMIT` safety cap).
const GRAPH_ENTITY_CAP: usize = 500;
/// Default + max page size for the candidates endpoint.
const DEFAULT_CANDIDATE_LIMIT: i64 = 50;
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

/// Build the wire projection of a `KeyBlock`.
fn project_entity(kb: &KeyBlock) -> WorldKbEntityProjection {
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
        key_block_id: kb.key_block_id.clone(),
        world_id: kb.world_id.clone(),
        block_type: kb.block_type,
        canonical_name: kb.canonical_name.clone(),
        status: kb.status.clone(),
        version: kb.revision.unwrap_or(0),
        body: body_value,
        aliases,
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
        block_type: parse_block_type(c.block_type_guess.as_deref().unwrap_or("character")),
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
        candidate_ids: Some(vec![]),
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

/// `POST /v1/local/worlds/{world_id}/kb/patch-entity` — entity-level patch.
pub async fn patch_entity(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPatchEntityRequest>,
) -> Result<Json<WorldKbPatchEntityResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool();

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
        .get_key_block(&req.entity_id)
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
    let new_name = req.patch.title.clone();
    let new_block_type = req.patch.block_type;
    let (body_json_str, body_for_validation) = compute_body(&kb, &req.patch)?;

    if let Some(ref name) = new_name {
        validate_canonical_name(name)
            .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;
    }
    let validation_block_type = new_block_type.unwrap_or(kb.block_type);
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
    let updated =
        store
            .get_key_block(&req.entity_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    Ok(Json(WorldKbPatchEntityResponse {
        entity: project_entity(&updated),
        version: new_version,
        validation_summary: validation_summary(&[], &[]),
    }))
}

/// `true` when the patch carries no editable field.
const fn patch_is_empty(patch: &WorldKbEntityPatch) -> bool {
    patch.title.is_none()
        && patch.body.is_none()
        && patch.aliases.is_none()
        && patch.block_type.is_none()
}

/// Resolve the new `body_json` DB string (and a `KeyBlockBody` for validation)
/// from the patch + the current entity body. `aliases` are merged into
/// `body.attributes.aliases`.
fn compute_body(
    kb: &KeyBlock,
    patch: &WorldKbEntityPatch,
) -> Result<(Option<String>, Option<KeyBlockBody>), NexusApiError> {
    if patch.body.is_none() && patch.aliases.is_none() {
        return Ok((None, None));
    }
    // Start from the patch body, else the current body, else an empty body.
    let mut value = patch
        .body
        .clone()
        .or_else(|| {
            kb.body
                .as_ref()
                .map(|b| serde_json::to_value(b).unwrap_or_default())
        })
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(ref aliases) = patch.aliases {
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
            aliases
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
        );
    }
    let body: KeyBlockBody =
        serde_json::from_value(value.clone()).map_err(|e| NexusApiError::InvalidInput {
            field: "body".to_string(),
            reason: format!("body is not a valid KeyBlockBody: {e}"),
        })?;
    let json_str = serde_json::to_string(&value).unwrap_or_default();
    Ok((Some(json_str), Some(body)))
}

// ─── promote-candidate ──────────────────────────────────────────────────────

/// `POST /v1/local/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge.
pub async fn promote_candidate(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPromoteCandidateRequest>,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool();

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
    body: KeyBlockBody,
    block_type: nexus_contracts::BlockType,
    canonical_name: String,
}

/// Parse the candidate `proposed_payload` and apply optional `patch`
/// refinements (`title`/`body`/`aliases`/`block_type`) into a validated adopt plan.
fn build_adopt_plan(
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<AdoptPlan, NexusApiError> {
    let mut body: KeyBlockBody = serde_json::from_str(
        candidate.proposed_payload.as_deref().unwrap_or("{}"),
    )
    .map_err(|e| NexusApiError::Internal {
        code: "KB_PAYLOAD_INVALID".to_string(),
        message: format!("proposed_payload is not a valid KeyBlockBody: {e}"),
    })?;
    let block_type = req
        .patch
        .as_ref()
        .and_then(|p| p.block_type)
        .unwrap_or_else(|| {
            parse_block_type(candidate.block_type_guess.as_deref().unwrap_or("character"))
        });
    let canonical_name = req
        .patch
        .as_ref()
        .and_then(|p| p.title.clone())
        .or_else(|| candidate.canonical_name_guess.clone())
        .ok_or_else(|| {
            NexusApiError::world_kb_validation_failed(
                &["candidate has no canonical_name_guess and no patch.title".to_string()],
                &[],
            )
        })?;
    if let Some(ref p) = req.patch {
        if let Some(ref b) = p.body {
            body = serde_json::from_value(b.clone()).map_err(|e| NexusApiError::InvalidInput {
                field: "patch.body".to_string(),
                reason: format!("not a valid KeyBlockBody: {e}"),
            })?;
        }
        if let Some(ref aliases) = p.aliases {
            merge_aliases_into_body(&mut body, aliases);
        }
    }
    Ok(AdoptPlan {
        body,
        block_type,
        canonical_name,
    })
}

/// Set `body.attributes.aliases` in place.
fn merge_aliases_into_body(body: &mut KeyBlockBody, aliases: &[String]) {
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
    if let Ok(merged) = serde_json::from_value::<KeyBlockBody>(value) {
        *body = merged;
    }
}

/// Adopt: parse `proposed_payload` → confirmed `KeyBlock`; atomic insert + CAS flip.
async fn promote_adopt(
    state: &WorkspaceState,
    world_id: &str,
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let pool = state.pool();
    let store = kb_store::SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);

    let AdoptPlan {
        body,
        block_type,
        canonical_name,
    } = build_adopt_plan(candidate, req)?;

    validate_canonical_name(&canonical_name)
        .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;
    validate_body(block_type, Some(&body), ValidationMode::Novel)
        .map_err(|e| NexusApiError::world_kb_validation_failed(&[e.to_string()], &[]))?;

    let mut kb = KeyBlock::new(world_id, block_type, &canonical_name);
    kb.body = Some(body);
    kb.status = "confirmed".to_string();
    kb.created_at = chrono::Utc::now().to_rfc3339();
    kb.source_work_id = candidate.work_id.clone();
    kb.source_chapter = candidate.source_chapter_id;
    kb.source_provenance_kind = if candidate.llm_confidence.is_some() {
        Some("review_time_extract".to_string())
    } else {
        Some("manual".to_string())
    };

    // Atomic promotion: insert + CAS flip in one transaction.
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let insert = store
        .insert_key_block_in_tx(&mut tx, kb.clone())
        .await
        .map_err(|e| map_kb_store_err(&e, &req.job_id))?;
    let flipped = mark_confirmed_in_tx_with_cas(
        &mut tx,
        &req.job_id,
        i64::try_from(req.expected_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_cas_err(e, &req.job_id, "version"))?;
    if !flipped {
        // Race: row left pending state between read and flip. Roll back the
        // orphan KeyBlock insert; surface a validation error (not a conflict).
        let _ = tx.rollback().await;
        return Err(NexusApiError::world_kb_validation_failed(
            &[
                "candidate was no longer pending (already confirmed/rejected); rolled back"
                    .to_string(),
            ],
            &[],
        ));
    }
    tx.commit().await.map_err(NexusApiError::from)?;

    let kb_id = insert.key_block_id.clone();
    let updated_kb = store
        .get_key_block(&kb_id)
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
        entity: Some(project_entity(&updated_kb)),
        job: project_job(&job),
        version: new_version,
        validation_summary: validation_summary(&[], &[]),
    }))
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
        job: project_job(&job),
        version: new_version,
        validation_summary: validation_summary(&[], &[]),
    }))
}

/// Merge: fold the candidate summary into an existing confirmed target, then
/// dismiss the candidate. `merge_target_id` must reference a confirmed/manual
/// `KeyBlock` in the same world.
// simplify: V1.73 β merge folds the candidate summary into the target body and
// rejects the candidate job. Full attribute-level merge with conflict surfacing
// is deferred to V1.74 alongside the relationships surface.
async fn promote_merge(
    state: &WorkspaceState,
    world_id: &str,
    candidate: &KbExtractPromotion,
    req: &WorldKbPromoteCandidateRequest,
) -> Result<Json<WorldKbPromoteCandidateResponse>, NexusApiError> {
    let pool = state.pool();
    let target_id = req
        .merge_target_id
        .as_deref()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "merge_target_id".to_string(),
            reason: "merge requires merge_target_id".to_string(),
        })?;
    let store = kb_store::SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
    let target = store
        .get_key_block(target_id)
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
        .and_then(|p| serde_json::from_str::<KeyBlockBody>(p).ok())
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
            .get_key_block(target_id)
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
        entity: Some(project_entity(&updated_target)),
        job: project_job(&job),
        version: new_version,
        validation_summary: validation_summary(&[], &[]),
    }))
}

fn map_kb_store_err(e: &nexus_kb::store::KbStoreError, job_id: &str) -> NexusApiError {
    use nexus_kb::store::KbStoreError;
    match e {
        KbStoreError::Validation(_) | KbStoreError::ValidationLegacy(_) => {
            NexusApiError::world_kb_validation_failed(&[e.to_string()], &[])
        }
        KbStoreError::Duplicate { .. } => NexusApiError::world_kb_validation_failed(
            &[
                "an active KeyBlock with the same name/type already exists in this world"
                    .to_string(),
            ],
            &[],
        ),
        _ => NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("adopt insert failed for {job_id}: {e}"),
        },
    }
}

// ─── read endpoints ─────────────────────────────────────────────────────────

/// `GET /v1/local/worlds/{world_id}/kb/graph` — entity graph projection.
pub async fn get_graph(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<WorldKbGraphResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool(), &world_id, &creator_id).await?;

    let store = kb_store::SqliteKbStore::new(state.pool().clone());
    let blocks = store
        .list_by_world(&world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // simplify: V1.73 derives source-anchor provenance edges from the
    // KeyBlock's own source_work_id/source_provenance_kind rather than a
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
                source_anchor_id: format!("sa_{}", kb.key_block_id),
                key_block_id: kb.key_block_id.clone(),
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

    Ok(Json(WorldKbGraphResponse {
        entities,
        source_anchors,
        relationships: project_relationships_for_world(state.pool(), &world_id)
            .await?,
    }))
}

/// Read all relationships for the world and emit stored + derived symmetric-reverse
/// projections.
async fn project_relationships_for_world(
    pool: &sqlx::SqlitePool,
    world_id: &str,
) -> Result<Vec<WorldKbRelationshipProjection>, NexusApiError> {
    let rows = list_relationships_for_world(pool, world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    let mut projections = Vec::with_capacity(rows.len() * 2);
    for row in rows {
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

/// `GET /v1/local/worlds/{world_id}/kb/candidates` — pending candidates list.
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
    require_world_owner(state.pool(), &world_id, &creator_id).await?;

    let limit = query
        .limit
        .unwrap_or(DEFAULT_CANDIDATE_LIMIT)
        .clamp(1, MAX_CANDIDATE_LIMIT);
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    let (cursor_created_at, cursor_job_id) = decode_candidate_cursor(query.cursor.as_ref())?;

    // Fetch `limit + 1` rows starting strictly after the cursor tuple so the
    // extra row detects `has_more` without truncating later pages.
    let pending = list_pending_for_world_after(
        state.pool(),
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
        items,
        pagination: PaginationInfo {
            limit,
            has_more,
            next_cursor,
        },
    }))
}

// ─── patch-relationship ─────────────────────────────────────────────────────

/// `POST /v1/local/worlds/{world_id}/kb/patch-relationship` — add/update/remove a
/// typed relationship between two World KB entities.
pub async fn patch_relationship(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldKbPatchRelationshipRequest>,
) -> Result<Json<WorldKbPatchRelationshipResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    let pool = state.pool();
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
    input: Option<WorldKbRelationshipInput>,
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
    require_valid_source_anchors(pool, world_id, input.source_anchor_ids.as_deref()).await?;

    let relationship_id = generate_relationship_id();
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    insert_relationship_in_tx(
        &mut tx,
        &InsertRelationshipParams {
            relationship_id: relationship_id.clone(),
            world_id: world_id.to_string(),
            source_entity_id: input.source_entity_id.clone(),
            target_entity_id: input.target_entity_id.clone(),
            relation_type: input.relation_type.as_str().to_string(),
            custom_label: input.custom_label.clone(),
            symmetric: input.symmetric,
            confidence: input.confidence,
            source_anchor_ids: input.source_anchor_ids.clone().unwrap_or_default(),
            metadata: input.metadata.clone(),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;
    tx.commit().await.map_err(NexusApiError::from)?;

    let row = get_relationship(pool, &relationship_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok(Json(WorldKbPatchRelationshipResponse {
        relationship: Some(project_relationship(&row, "stored")),
        version: u64::try_from(row.revision).unwrap_or(0),
        validation_summary: validation_summary(&[], &[]),
    }))
}

async fn patch_relationship_update(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    relationship_id: Option<&str>,
    expected_version: Option<u64>,
    input: Option<WorldKbRelationshipInput>,
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
    require_valid_source_anchors(pool, world_id, input.source_anchor_ids.as_deref()).await?;

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
        return Err(NexusApiError::NotFound(format!(
            "relationship {relationship_id} in world {world_id}"
        )));
    }

    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let new_version = update_relationship_in_tx(
        &mut tx,
        relationship_id,
        &UpdateRelationshipParams {
            relation_type: input.relation_type.as_str().to_string(),
            custom_label: input.custom_label,
            symmetric: input.symmetric,
            confidence: input.confidence,
            source_anchor_ids: input.source_anchor_ids.unwrap_or_default(),
            metadata: input.metadata,
            updated_at: now.to_string(),
        },
        i64::try_from(expected_version).unwrap_or(0),
    )
    .await
    .map_err(|e| map_relationship_cas_err(e, relationship_id))?;
    tx.commit().await.map_err(NexusApiError::from)?;

    let row =
        get_relationship(pool, relationship_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    Ok(Json(WorldKbPatchRelationshipResponse {
        relationship: Some(project_relationship(&row, "stored")),
        version: new_version,
        validation_summary: validation_summary(&[], &[]),
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
        return Err(NexusApiError::NotFound(format!(
            "relationship {relationship_id} in world {world_id}"
        )));
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
        validation_summary: validation_summary(&[], &[]),
    }))
}

/// Domain validation for a relationship payload.
fn validate_relationship_input(input: &WorldKbRelationshipInput) -> Result<(), NexusApiError> {
    if input.source_entity_id == input.target_entity_id {
        return Err(NexusApiError::world_kb_validation_failed(
            &["source_entity_id and target_entity_id must be different".to_string()],
            &[],
        ));
    }
    if input.relation_type == WorldKbRelationshipKind::Custom && input.custom_label.is_none() {
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

/// Verify every source-anchor projection id references a `KeyBlock` in the world
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
                &[format!("source_anchor_id '{id}' is not a valid anchor projection id")],
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
            None => errors.push(format!("source anchor '{id}' does not reference an entity in this world")),
            Some(row) if row.source_work_id.is_none() => errors.push(format!("source anchor '{id}' references an entity without provenance")),
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

    WorldKbRelationshipProjection {
        relationship_id: row.relationship_id.clone(),
        world_id: row.world_id.clone(),
        source_entity_id: row.source_entity_id.clone(),
        target_entity_id: row.target_entity_id.clone(),
        relation_type: row
            .relation_type
            .parse()
            .unwrap_or(WorldKbRelationshipKind::Custom),
        custom_label: row.custom_label.clone(),
        symmetric: row.symmetric != 0,
        confidence: row.confidence,
        source_anchor_ids,
        metadata,
        version: u64::try_from(row.revision).unwrap_or(0),
        updated_at: row.updated_at.clone(),
        projection_direction: direction.to_string(),
    }
}
